//! Lógica de la aplicación: identidad local, vinculación de dispositivos,
//! chats (1:1 y grupos), sincronización de dispositivos en los grupos y
//! entrega diferida mediante buzones (D-016).
//!
//! Regla de concurrencia: el estado (`State`) se protege con un mutex
//! síncrono que nunca se mantiene durante una espera de red. Los envíos de
//! este dispositivo se serializan con `send_lock` para que los commits
//! lleguen antes que los mensajes de la época siguiente.

use zeroize::Zeroizing;
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow, bail};
use ed25519_dalek::{SigningKey, VerifyingKey};
use futures::future::join_all;
use libp2p::multiaddr::Protocol;
use libp2p::{Multiaddr, PeerId};
use lacre_core::identity::{Handle, decode_key, encode_key, generate_key, handle_suffix};
use lacre_core::envelope::{self, Envelope};
use lacre_core::mls::{Incoming, KeyPackage, MemberId, MlsEngine, verify_key_package};
use lacre_core::record::{
    IdentityRecord, SignedRecord, VerifiedRecord, dht_key, select_record, key_package_key, mailbox_key,
};
use lacre_core::{cbor_from, cbor_to, vault};
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

use crate::files::{self, FileOffer, Files, Partial, StoredOffer};
use crate::history::{FileRef, History, StoredMessage};
use crate::mailbox::Mailbox;
use crate::net::{Client, Inbound, device_pk_of, peer_id_of};
use crate::peers::is_dialable_from_anywhere;
use crate::people::{self, Avatars, Blocked, fold};
use crate::proto::{ChatMeta, Request, Response};
use crate::sticker::{self, Stickers};

/// Mensajes MLS que no se pudieron procesar (p. ej. de una época futura) y
/// se reintentan tras el siguiente commit.
const MAX_PENDING: usize = 128;
/// Cupo de la cola de reintentos por grupo y para grupos desconocidos (D-042).
const MAX_PENDING_PER_GROUP: usize = 32;
const MAX_PENDING_UNKNOWN: usize = 32;
const MAX_ATTEMPTS: u8 = 5;
/// Copias remotas de cada sobre para un dispositivo desconectado (además
/// de la que guarda el propio remitente).
const MAILBOX_REPLICAS: usize = 3;
/// Sobres que se aceptan por `Deliver` de un mismo peer en
/// `DELIVER_WINDOW` segundos (D-034).
const DELIVER_BUDGET: usize = 300;
const DELIVER_WINDOW: u64 = 600;
/// KeyPackages de un solo uso que se generan por peer y en total en cada
/// ventana de `KP_WINDOW` segundos; pasado el cupo, el otro usa el de
/// último recurso de la DHT (D-042).
const KP_PER_PEER: usize = 8;
const KP_TOTAL: usize = 100;
const KP_WINDOW: u64 = 3600;
/// Segundos mínimos entre sincronizaciones de un grupo pedidas por aviso.
const HINT_EVERY: u64 = 20;
/// Caracteres de un mensaje de texto que se guardan.
const MAX_TEXT: usize = 16 * 1024;
/// Mensajes que se guardan de un chat aún sin aceptar.
const MAX_REQUEST_MESSAGES: usize = 50;
/// Solicitudes de chat pendientes a la vez; más se rechazan.
const MAX_REQUESTS: usize = 50;
/// Ids de sobres vigentes que se recuerdan para descartar duplicados (D-042).
const MAX_SEEN: usize = 20_000;
/// Antigüedad a partir de la cual se renueva el KeyPackage de último recurso.
const LAST_RESORT_MAX_AGE: u64 = 7 * 24 * 3600;
/// Cada cuánto se hace ping a los miembros de los chats: por debajo del
/// cierre por inactividad de la red (120 s) para no perder la conexión.
pub const KEEPALIVE_EVERY: Duration = Duration::from_secs(45);
/// Espera ante un dispositivo que no responde; se dobla en cada fallo.
const KEEPALIVE_RETRY_MIN: Duration = Duration::from_secs(60);
const KEEPALIVE_RETRY_MAX: Duration = Duration::from_secs(15 * 60);

/// Avisos para la interfaz (la CLI los imprime; la app los muestra).
#[derive(Clone, Debug)]
pub enum Event {
    /// Este dispositivo ha sido vinculado a una identidad.
    Linked { handle: String },
    /// Un dispositivo nuevo ha entrado con nuestra invitación (D-037).
    DeviceLinked { label: String },
    /// Nos han añadido a un chat. `request`: de alguien con quien aún no
    /// hablamos; hay que aceptarlo (D-029).
    NewChat { index: usize, name: String, request: bool },
    /// Ha cambiado la foto de perfil de alguien (o la nuestra).
    Avatar { handle: String },
    /// Ha cambiado la lista de chats (p. ej. otro dispositivo nuestro aceptó
    /// una solicitud).
    ChatsChanged,
    /// `local`: enviado desde este mismo dispositivo.
    Message { index: usize, message: StoredMessage, local: bool },
    /// Este dispositivo ha sido expulsado de un chat.
    Removed { index: usize },
    /// Hay en el chat un dispositivo que su persona no ha autorizado.
    UnverifiedMember { index: usize, name: String },
    /// Somos alcanzables a través de un relay en esta dirección.
    RelayReady(String),
    /// Progreso de una descarga (`done == total`: terminada).
    FileProgress { id: String, done: u32, total: u32 },
}

/// Resumen de un chat para listarlo.
#[derive(Clone, Debug)]
pub struct ChatSummary {
    pub index: usize,
    pub name: String,
    pub devices: usize,
    pub active: bool,
    pub last: Option<StoredMessage>,
    /// Solicitud de chat pendiente de aceptar (D-029).
    pub request: bool,
    /// La otra persona, si es un chat 1:1.
    pub peer: Option<String>,
    /// Hay foto de la otra persona (1:1 aceptado).
    pub avatar: bool,
}

/// Mensaje encontrado por [`App::search`].
#[derive(Clone, Debug)]
pub struct SearchHit {
    pub index: usize,
    /// Posición en [`App::messages`] de ese chat.
    pub position: usize,
    pub message: StoredMessage,
}

/// Resultados como máximo de una búsqueda.
const MAX_HITS: usize = 200;

/// Resultado de un envío.
#[derive(Clone, Copy, Debug, Default)]
pub struct SendReport {
    /// Dispositivos desconectados cuyo mensaje quedó en buzones.
    pub stored: usize,
    /// Dispositivos a los que no se pudo entregar ni dejar en buzones.
    pub failed: usize,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Chat {
    pub group_id: Vec<u8>,
    pub meta: ChatMeta,
    pub active: bool,
    /// Borrado por esta persona: no se lista hasta que llegue otro mensaje.
    #[serde(default)]
    pub hidden: bool,
    /// `false`: solicitud de chat sin aceptar (D-029).
    #[serde(default = "yes")]
    pub accepted: bool,
    /// Quién nos invitó (handle del committer al unirnos).
    #[serde(default)]
    pub invited_by: Option<String>,
    /// Nuestra persona salió del grupo al borrarlo (D-031): ya no se
    /// muestran sus mensajes aunque el commit que nos saca tarde en llegar.
    #[serde(default)]
    pub left: bool,
}

fn yes() -> bool {
    true
}

#[derive(Serialize, Deserialize)]
struct AppMessage {
    text: String,
    sent_at: u64,
    /// Nueva lista de participantes: solo la manda el committer al añadir o
    /// quitar personas (en ese caso `text` va vacío).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    meta: Option<ChatMeta>,
    /// Imagen del sticker (D-025); en ese caso `text` va vacío.
    #[serde(default, skip_serializing_if = "Option::is_none", with = "serde_bytes")]
    sticker: Option<Vec<u8>>,
    /// Oferta de un archivo (D-026): nombre, tamaño, clave y hashes; los
    /// bytes se piden aparte. `text` va vacío.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    file: Option<FileOffer>,
    /// Versión de la foto de perfil de quien envía (D-030). Un mensaje sin
    /// nada más solo anuncia una foto nueva y no se muestra.
    #[serde(default, skip_serializing_if = "is_zero")]
    avatar: u64,
    /// Quien envía sale del grupo (D-031): se lo pide al committer. Con
    /// `meta`, el committer avisa de que los quitados salieron ellos.
    #[serde(default, skip_serializing_if = "is_false")]
    leave: bool,
}

fn is_false(v: &bool) -> bool {
    !*v
}

fn is_zero(v: &u64) -> bool {
    *v == 0
}

struct State {
    root: Option<SigningKey>,
    identity: Option<VerifiedRecord>,
    mls: Option<MlsEngine>,
    chats: Vec<Chat>,
    /// Registros de otras identidades ya resueltos, por clave raíz.
    contacts: HashMap<[u8; 32], VerifiedRecord>,
    /// (mensaje, intentos)
    pending: Vec<(Vec<u8>, u8)>,
    /// Sobres ya procesados (pueden llegar copias desde varios buzones).
    seen: Seen,
    blocked: Blocked,
    /// Grupos aceptados en otro dispositivo nuestro de los que aún no
    /// tenemos el Welcome.
    pre_accepted: HashSet<Vec<u8>>,
    /// Personas que han pedido salir de un grupo nuestro: (grupo, clave
    /// raíz). Se atienden fuera de `process_mls`, que no puede esperar.
    leaves: Vec<(Vec<u8>, [u8; 32])>,
}

/// Ping a un dispositivo: en curso, o cuándo toca el siguiente si falló.
#[derive(Default)]
struct Ping {
    busy: bool,
    next: Option<Instant>,
    wait: Duration,
}

pub struct App {
    dir: PathBuf,
    device: SigningKey,
    net: Client,
    state: Mutex<State>,
    mailbox: Mutex<Mailbox>,
    history: Mutex<History>,
    stickers: Mutex<Stickers>,
    files: Mutex<Files>,
    /// Archivos que se están descargando ahora.
    downloading: Mutex<HashSet<String>>,
    send_lock: tokio::sync::Mutex<()>,
    /// Estado del ping a cada dispositivo de nuestros chats (D-028).
    pings: Mutex<HashMap<[u8; 32], Ping>>,
    avatars: Mutex<Avatars>,
    /// Hay una ronda de entregas desde el buzón en curso (D-034).
    pushing: std::sync::atomic::AtomicBool,
    /// Sobres aceptados por `Deliver` de cada peer: (inicio de ventana, cuántos).
    delivered: Mutex<HashMap<PeerId, (u64, usize)>>,
    /// Invitación para vincular que muestra ahora este dispositivo (D-037).
    invite: Mutex<Option<Invite>>,
    /// Secreto del código de vinculación que muestra este dispositivo
    /// nuevo: solo se acepta un `LinkGrant` que lo traiga (D-039).
    link_secret: Mutex<Option<Invite>>,
    /// KeyPackages generados para cada peer y en total (D-042): cada uno
    /// deja una clave privada guardada.
    kp_limit: Mutex<(RateLimit<PeerId>, RateLimit<()>)>,
    /// Última sincronización pedida por aviso de cada grupo.
    hinted: Mutex<HashMap<Vec<u8>, u64>>,
    /// Depósitos en nuestro buzón por peer.
    deposit_limit: Mutex<RateLimit<PeerId>>,
    events: tokio::sync::mpsc::UnboundedSender<Event>,
    /// Runtime donde se lanzan las tareas que siguen a una llamada síncrona.
    rt: tokio::runtime::Handle,
}

/// Resultado de enviar a varios dispositivos.
#[derive(Default)]
struct Delivery {
    direct: usize,
    /// Desconectados cuyo mensaje quedó en buzones.
    stored: usize,
    failed: usize,
}

/// Cupo por clave en ventanas de tiempo fijas, con un número acotado de
/// claves a la vez (D-042).
struct RateLimit<K> {
    per_key: usize,
    window: u64,
    max_keys: usize,
    used: HashMap<K, (u64, usize)>,
}

impl<K: std::hash::Hash + Eq> RateLimit<K> {
    fn new(per_key: usize, window: u64, max_keys: usize) -> Self {
        Self { per_key, window, max_keys, used: HashMap::new() }
    }

    /// Cuenta un uso de `key`; `false` si ya agotó su cupo.
    fn take(&mut self, key: K) -> bool {
        let t = now();
        if self.used.len() >= self.max_keys {
            let window = self.window;
            self.used.retain(|_, (start, _)| *start + window > t);
            if self.used.len() >= self.max_keys && !self.used.contains_key(&key) {
                return false;
            }
        }
        let (start, n) = self.used.entry(key).or_insert((t, 0));
        if *start + self.window <= t {
            *start = t;
            *n = 0;
        }
        *n += 1;
        *n <= self.per_key
    }
}

pub(crate) fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

fn read_key(path: &Path) -> Result<Option<SigningKey>> {
    // Las copias intermedias de la clave se borran de la memoria (D-042).
    let Some(bytes) = vault::read_opt(path)? else { return Ok(None) };
    let bytes = Zeroizing::new(bytes);
    let key: Zeroizing<[u8; 32]> = Zeroizing::new(
        bytes.as_slice().try_into().map_err(|_| anyhow!("clave corrupta en {}", path.display()))?,
    );
    Ok(Some(SigningKey::from_bytes(&key)))
}

/// Escribe un archivo de estado: atómico, solo para el usuario y cifrado si
/// hay clave de datos (D-033).
pub(crate) fn write_private(path: &Path, bytes: &[u8]) -> Result<()> {
    Ok(vault::write(path, bytes)?)
}

/// Lee un archivo de estado (`None` si no existe).
pub(crate) fn read_private(path: &Path) -> Result<Option<Vec<u8>>> {
    Ok(vault::read_opt(path)?)
}

/// Copias descifradas de archivos abiertos con otras aplicaciones.
pub const OPENED: &str = "abiertos";

/// Archivos de estado que van cifrados (D-033), relativos al directorio de
/// datos. Además: todo `avatars/` y `media/` (fotos y stickers) y
/// `files/*/oferta.cbor`. Los archivos van cifrados con su propia clave
/// (ver [`files`]) y se migran al abrir el nodo.
const STATE_FILES: &[&str] = &[
    "device.key", "root.key", "identity.cbor", "mls.json", "chats.cbor", "seen.cbor",
    "mailbox.cbor", "history.cbor", "contacts.cbor", "lastresort.kp", "blocked.cbor",
    "stickers.cbor", "peers.txt",
];

/// Cifra los archivos de estado que aún estén en claro. Devuelve cuántos.
pub fn migrate_to_vault(dir: &Path) -> Result<usize> {
    let mut paths: Vec<PathBuf> = STATE_FILES.iter().map(|f| dir.join(f)).collect();
    for sub in ["avatars", "media"] {
        if let Ok(entries) = fs::read_dir(dir.join(sub)) {
            paths.extend(entries.flatten().map(|e| e.path()).filter(|p| p.is_file()));
        }
    }
    if let Ok(entries) = fs::read_dir(dir.join("files")) {
        paths.extend(entries.flatten().map(|e| e.path().join("oferta.cbor")));
    }
    let mut n = 0;
    for p in paths {
        n += usize::from(vault::migrate(&p).with_context(|| format!("no se pudo cifrar {}", p.display()))?);
    }
    Ok(n)
}

/// El directorio tiene estado cifrado (hace falta la clave de datos).
pub fn has_sealed_state(dir: &Path) -> bool {
    fs::read(dir.join("device.key")).is_ok_and(|b| vault::is_sealed(&b))
}

/// Carga (o crea) la clave de este dispositivo.
pub fn load_device_key(dir: &Path) -> Result<SigningKey> {
    fs::create_dir_all(dir)?;
    let path = dir.join("device.key");
    if let Some(k) = read_key(&path)? {
        return Ok(k);
    }
    let k = generate_key();
    write_private(&path, Zeroizing::new(k.to_bytes()).as_slice())?;
    Ok(k)
}

/// Sobres ya procesados (D-042). Cada id se recuerda con su fecha de
/// sellado hasta que el sobre caduca ([`envelope::MAX_AGE`]); a partir de
/// ahí `open` ya lo rechaza solo, así que ninguna repetición se acepta. Si
/// llegan más de [`MAX_SEEN`] sobres vigentes (una inundación), se olvida el
/// más antiguo y `floor` sube a su fecha: lo sellado antes se trata como
/// repetido. Así un sobre legítimo que tarda días en llegar por un buzón solo
/// se pierde bajo inundación, nunca por tráfico normal.
#[derive(Serialize, Deserialize, Default)]
struct Seen {
    ids: VecDeque<([u8; 32], u64)>,
    floor: u64,
}

impl Seen {
    /// Lee `seen.cbor`; el formato anterior (solo ids) se migra con la fecha
    /// actual, que los mantiene hasta que caduquen.
    fn from_bytes(b: &[u8]) -> Result<Self> {
        if let Ok(s) = cbor_from(b) {
            return Ok(s);
        }
        let old: VecDeque<[u8; 32]> = cbor_from(b)?;
        let now = now();
        Ok(Seen { ids: old.into_iter().map(|i| (i, now)).collect(), floor: 0 })
    }

    fn contains(&self, id: &[u8; 32]) -> bool {
        self.ids.iter().any(|(i, _)| i == id)
    }

    /// Apunta un sobre sellado en `sent`. `false` si ya se vio o es anterior
    /// a `floor`.
    fn insert(&mut self, id: [u8; 32], sent: u64, now: u64) -> bool {
        if sent < self.floor || self.contains(&id) {
            return false;
        }
        // Fuera los caducados (con margen por relojes adelantados).
        let horizon = now.saturating_sub(envelope::MAX_AGE + envelope::MAX_SKEW);
        self.ids.retain(|(_, t)| *t >= horizon);
        self.ids.push_back((id, sent));
        while self.ids.len() > MAX_SEEN {
            let (pos, t) = self
                .ids
                .iter()
                .enumerate()
                .min_by_key(|(_, (_, t))| *t)
                .map(|(p, (_, t))| (p, *t))
                .unwrap();
            self.ids.remove(pos);
            self.floor = self.floor.max(t + 1);
        }
        true
    }
}

/// Destino de un chat: un handle o el contenido de un QR
/// (`p2m:handle:clave[@dirección…]`). El QR además fija la clave raíz
/// completa y puede llevar direcciones por las que llamar al dispositivo que
/// lo muestra, para entrar a la red sin ningún otro nodo conocido (D-021).
pub struct Target {
    handle: Handle,
    root: Option<VerifyingKey>,
    addrs: Vec<Multiaddr>,
}

impl std::str::FromStr for Target {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self> {
        if let Some(rest) = s.strip_prefix("p2m:") {
            let mut parts = rest.split('@');
            let (h, k) = parts.next().unwrap().split_once(':').context("QR mal formado")?;
            // Una dirección que no entendemos (quizá de una versión más
            // nueva) no invalida el QR.
            let addrs = parts.filter_map(|a| a.parse().ok()).collect();
            return Ok(Self { handle: h.parse()?, root: Some(decode_key(k)?), addrs });
        }
        Ok(Self { handle: s.parse()?, root: None, addrs: Vec::new() })
    }
}

/// Nombre para mostrar: el título o, si no lo tiene, el resto de
/// participantes.
fn chat_name(meta: &ChatMeta, me: Option<&str>) -> String {
    if let Some(t) = &meta.title {
        return t.clone();
    }
    let others: Vec<&str> = meta.participants.iter().map(String::as_str).filter(|p| Some(*p) != me).collect();
    if others.is_empty() { meta.participants.join(", ") } else { others.join(", ") }
}

/// Grupo (con título o con un número de personas distinto de dos), no 1:1.
fn is_group(chat: &Chat) -> bool {
    chat.meta.title.is_some() || chat.meta.participants.len() != 2
}

/// Clave de un chat 1:1 activo (sin título y con dos personas): los sufijos
/// de los dos handles, ordenados. `None` para grupos y chats inactivos.
fn direct_key(chat: &Chat) -> Option<[String; 2]> {
    if !chat.active || chat.meta.title.is_some() || chat.meta.participants.len() != 2 {
        return None;
    }
    let suffix = |p: &String| p.parse::<Handle>().map_or_else(|_| p.clone(), |h| h.suffix);
    let mut key = [suffix(&chat.meta.participants[0]), suffix(&chat.meta.participants[1])];
    key.sort();
    Some(key)
}

/// Chat que se muestra en lugar de `index` (D-027). Entre dos personas puede
/// haber varios grupos MLS 1:1 (los dos crearon el chat a la vez, o uno lo
/// creó desde un dispositivo que aún no sabía del otro): se muestran como
/// uno solo, el primero de la lista, con los mensajes de todos.
fn canonical(chats: &[Chat], index: usize) -> usize {
    let Some(key) = chats.get(index).and_then(direct_key) else { return index };
    chats.iter().position(|c| direct_key(c).as_ref() == Some(&key)).unwrap_or(index)
}

/// Índices que forman el chat `index` (él y sus duplicados 1:1).
fn merged_indexes(chats: &[Chat], index: usize) -> Vec<usize> {
    let index = canonical(chats, index);
    (0..chats.len()).filter(|&i| canonical(chats, i) == index).collect()
}

/// Grupos MLS que forman el chat `index`.
fn merged_groups(chats: &[Chat], index: usize) -> Vec<Vec<u8>> {
    merged_indexes(chats, index).into_iter().map(|i| chats[i].group_id.clone()).collect()
}

/// Sufijo de un handle escrito como `nombre#sufijo` (o el texto tal cual).
fn suffix_of(handle: &str) -> String {
    handle.parse::<Handle>().map_or_else(|_| handle.to_owned(), |h| h.suffix)
}

fn suffix_of_root(root_pk: &[u8; 32]) -> Option<String> {
    Some(handle_suffix(&VerifyingKey::from_bytes(root_pk).ok()?))
}

/// La otra persona de un chat 1:1 (`None` en grupos).
fn direct_peer(chat: &Chat, me: &str) -> Option<String> {
    direct_key(chat)?;
    chat.meta.participants.iter().find(|p| suffix_of(p) != me).cloned()
}

/// Chat oculto por un bloqueo: 1:1 con alguien bloqueado, o invitación sin
/// aceptar de alguien bloqueado.
fn blocked_chat(chat: &Chat, blocked: &Blocked, me: &str) -> bool {
    direct_peer(chat, me).is_some_and(|p| blocked.contains(&suffix_of(&p)))
        || (!chat.accepted && chat.invited_by.as_deref().is_some_and(|h| blocked.contains(&suffix_of(h))))
}

/// Handle del participante cuya clave raíz es `root_pk` (el sufijo del
/// handle se deriva de ella).
fn participant_of(participants: &[String], root_pk: &[u8; 32]) -> String {
    let Ok(root) = VerifyingKey::from_bytes(root_pk) else { return "¿clave inválida?".into() };
    let suffix = handle_suffix(&root);
    participants
        .iter()
        .find(|p| p.parse::<Handle>().is_ok_and(|h| h.suffix == suffix))
        .cloned()
        .unwrap_or_else(|| format!("desconocido#{suffix}"))
}

/// Aviso que resume un cambio de participantes hecho por `by`. Con `leave`,
/// los quitados salieron ellos (D-031).
fn meta_notice(old: &ChatMeta, new: &ChatMeta, by: &str, leave: bool) -> Option<String> {
    let added: Vec<&str> =
        new.participants.iter().filter(|p| !old.participants.contains(p)).map(String::as_str).collect();
    let removed: Vec<&str> =
        old.participants.iter().filter(|p| !new.participants.contains(p)).map(String::as_str).collect();
    let mut parts = Vec::new();
    if !added.is_empty() {
        parts.push(format!("{by} añadió a {}", added.join(", ")));
    }
    if !removed.is_empty() && leave {
        parts.push(format!("{} salió del chat", removed.join(", ")));
    } else if !removed.is_empty() {
        parts.push(format!("{by} quitó a {}", removed.join(", ")));
    }
    (!parts.is_empty()).then(|| parts.join("; "))
}

/// Trozos de un archivo que se piden a la vez.
const FILE_WORKERS: usize = 4;

/// Cuánto se espera a conectar con las direcciones de un QR.
const QR_DIAL_TIMEOUT: Duration = Duration::from_secs(60);
/// Prefijo de las invitaciones para vincular, que muestra el primario
/// (D-037): `lacre-link:clave-del-primario:token[@dirección…]`.
pub const INVITE_PREFIX: &str = "lacre-link:";
const INVITE_TTL: Duration = Duration::from_secs(10 * 60);
/// Cuánto se intenta llegar al otro dispositivo al vincular. Entre redes
/// distintas solo se llega por onion, y el servicio tarda en publicarse.
const LINK_TIMEOUT: Duration = Duration::from_secs(150);
const LINK_REDIAL: Duration = Duration::from_secs(20);
const MAX_LABEL: usize = 64;

/// Por qué no se pudo vincular, para explicarlo bien en la app (D-037).
#[derive(Debug)]
pub enum LinkError {
    /// No es un código de vinculación (o está mal copiado).
    BadCode,
    /// No se llegó al otro dispositivo en `LINK_TIMEOUT`.
    Unreachable,
    /// El otro dispositivo contestó que no: invitación caducada o ya usada,
    /// ya pertenece a otra identidad…
    Refused(String),
}

impl std::fmt::Display for LinkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BadCode => write!(f, "el código no es de vinculación"),
            Self::Unreachable => write!(f, "no se pudo contactar con el otro dispositivo"),
            Self::Refused(why) => write!(f, "el otro dispositivo no aceptó: {why}"),
        }
    }
}

impl std::error::Error for LinkError {}

/// Invitación vigente (solo en memoria y de un solo uso).
struct Invite {
    token: [u8; 16],
    until: Instant,
}

/// Código de vinculación de un dispositivo nuevo: su clave, un secreto de
/// un solo uso (D-039) y, desde D-037, direcciones por las que llamarlo
/// (`clave:secreto[@dirección…]`).
type ParsedLinkCode = ([u8; 32], Vec<u8>, Vec<Multiaddr>);

fn parse_link_code(code: &str) -> Result<ParsedLinkCode, LinkError> {
    let mut parts = code.trim().split('@');
    let (key, secret) = parts.next().unwrap_or("").trim().split_once(':').ok_or(LinkError::BadCode)?;
    let key = decode_key(key).map_err(|_| LinkError::BadCode)?.to_bytes();
    let secret = data_encoding::BASE32_NOPAD
        .decode(secret.to_uppercase().as_bytes())
        .map_err(|_| LinkError::BadCode)?;
    if secret.len() != 16 {
        return Err(LinkError::BadCode);
    }
    Ok((key, secret, parts.filter_map(|a| a.trim().parse().ok()).collect()))
}

/// Token vigente de `slot` o uno nuevo si no hay o caducó.
fn current_token(slot: &Mutex<Option<Invite>>) -> [u8; 16] {
    let mut inv = slot.lock().unwrap();
    match inv.as_ref() {
        Some(i) if i.until > Instant::now() => i.token,
        _ => {
            let mut token = [0u8; 16];
            rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut token);
            *inv = Some(Invite { token, until: Instant::now() + INVITE_TTL });
            token
        }
    }
}

/// Consume el token de `slot` si coincide con `token` y no ha caducado.
/// Compara hashes: no revela por tiempos cuánto del token coincide.
fn take_token(slot: &Mutex<Option<Invite>>, token: &[u8]) -> bool {
    let mut inv = slot.lock().unwrap();
    let valid = inv
        .as_ref()
        .is_some_and(|i| i.until > Instant::now() && blake3::hash(&i.token) == blake3::hash(token));
    if valid {
        *inv = None;
    }
    valid
}

/// Invitación: (clave del primario, token, direcciones).
type ParsedInvite = ([u8; 32], Vec<u8>, Vec<Multiaddr>);

fn parse_invite(code: &str) -> Result<ParsedInvite, LinkError> {
    let rest = code.trim().strip_prefix(INVITE_PREFIX).ok_or(LinkError::BadCode)?;
    let mut parts = rest.split('@');
    let (key, token) = parts.next().unwrap_or("").split_once(':').ok_or(LinkError::BadCode)?;
    let key = decode_key(key).map_err(|_| LinkError::BadCode)?.to_bytes();
    let token = data_encoding::BASE32_NOPAD
        .decode(token.to_uppercase().as_bytes())
        .map_err(|_| LinkError::BadCode)?;
    Ok((key, token, parts.filter_map(|a| a.trim().parse().ok()).collect()))
}

impl App {
    pub fn open(
        dir: PathBuf,
        device: SigningKey,
        net: Client,
        events: tokio::sync::mpsc::UnboundedSender<Event>,
    ) -> Result<Arc<Self>> {
        let root = read_key(&dir.join("root.key"))?;
        let identity = match read_private(&dir.join("identity.cbor"))? {
            Some(b) => Some(SignedRecord::from_bytes(&b)?.verify()?),
            None => None,
        };
        let mls = match &identity {
            Some(id) => Some(MlsEngine::open(&device, id.record.root_pk, Some(dir.join("mls.json")))?),
            None => None,
        };
        let chats = match read_private(&dir.join("chats.cbor"))? {
            Some(b) => cbor_from(&b)?,
            None => Vec::new(),
        };
        let seen = match read_private(&dir.join("seen.cbor"))? {
            // Formato anterior: solo la lista de ids.
            Some(b) => Seen::from_bytes(&b)?,
            None => Seen::default(),
        };
        let mailbox = Mailbox::open(dir.join("mailbox.cbor"))?;
        let history = History::open(dir.join("history.cbor"))?;
        let stickers = Stickers::open(&dir)?;
        // Registros de contactos ya resueltos (firmados: se vuelven a verificar).
        let contacts = match read_private(&dir.join("contacts.cbor"))? {
            Some(b) => cbor_from::<Vec<Vec<u8>>>(&b)?
                .iter()
                .filter_map(|r| SignedRecord::from_bytes(r).ok()?.verify().ok())
                .map(|v| (v.record.root_pk, v))
                .collect(),
            None => HashMap::new(),
        };
        let mut files = Files::open(&dir);
        if vault::enabled() {
            match files.migrate() {
                Ok(0) => {}
                Ok(n) => info!("{n} archivos recibidos cifrados en disco"),
                Err(e) => warn!("no se pudieron cifrar los archivos recibidos: {e:#}"),
            }
        }
        // Copias descifradas para abrir con otras aplicaciones: no deben
        // sobrevivir al nodo.
        let _ = fs::remove_dir_all(dir.join(OPENED));
        let blocked = Blocked::open(&dir)?;
        let avatars = Avatars::open(&dir)?;
        Ok(Arc::new(Self {
            dir,
            device,
            net,
            state: Mutex::new(State {
                root,
                identity,
                mls,
                chats,
                contacts,
                pending: Vec::new(),
                seen,
                blocked,
                pre_accepted: HashSet::new(),
                leaves: Vec::new(),
            }),
            mailbox: Mutex::new(mailbox),
            history: Mutex::new(history),
            stickers: Mutex::new(stickers),
            files: Mutex::new(files),
            downloading: Mutex::new(HashSet::new()),
            send_lock: tokio::sync::Mutex::new(()),
            pings: Mutex::new(HashMap::new()),
            avatars: Mutex::new(avatars),
            pushing: Default::default(),
            delivered: Mutex::new(HashMap::new()),
            invite: Mutex::new(None),
            link_secret: Mutex::new(None),
            hinted: Mutex::new(HashMap::new()),
            deposit_limit: Mutex::new(RateLimit::new(DELIVER_BUDGET, DELIVER_WINDOW, 4096)),
            kp_limit: Mutex::new((RateLimit::new(KP_PER_PEER, KP_WINDOW, 4096), RateLimit::new(KP_TOTAL, KP_WINDOW, 1))),
            rt: tokio::runtime::Handle::current(),
            events,
        }))
    }

    pub fn net(&self) -> &Client {
        &self.net
    }

    pub fn emit(&self, ev: Event) {
        let _ = self.events.send(ev);
    }

    /// Guarda un mensaje en el historial y avisa a la interfaz.
    /// Un chat borrado vuelve a la lista. Escribir en una solicitud (desde
    /// este o desde otro de nuestros dispositivos) la acepta.
    fn record(&self, index: usize, group_id: &[u8], message: StoredMessage, local: bool) {
        if let Err(e) = self.history.lock().unwrap().add(group_id, message.clone()) {
            warn!("no se pudo guardar el historial: {e:#}");
        }
        let index = {
            let mut st = self.state.lock().unwrap();
            let index = canonical(&st.chats, index);
            let accept = message.mine && !message.notice;
            let merged = merged_indexes(&st.chats, index);
            // Un aviso (alguien entra o sale) no hace reaparecer un chat
            // borrado; un mensaje sí.
            let mut changed = false;
            if !message.notice {
                changed = st.chats[index].hidden;
                st.chats[index].hidden = false;
            }
            for i in merged {
                changed |= accept && !st.chats[i].accepted;
                st.chats[i].accepted |= accept;
            }
            if changed && let Err(e) = self.save_chats(&st) {
                warn!("no se pudieron guardar los chats: {e:#}");
            }
            index
        };
        self.emit(Event::Message { index, message, local });
    }

    /// Mensajes de un chat, del más antiguo al más reciente.
    pub fn messages(&self, index: usize) -> Result<Vec<StoredMessage>> {
        let groups = {
            let st = self.state.lock().unwrap();
            st.chats.get(index).context("no existe ese chat")?;
            merged_groups(&st.chats, index)
        };
        let history = self.history.lock().unwrap();
        let mut list: Vec<StoredMessage> = groups.iter().flat_map(|g| history.get(g)).collect();
        if groups.len() > 1 {
            list.sort_by_key(|m| m.sent_at);
        }
        Ok(list)
    }

    // ---------------------------------------------------------- stickers

    /// Bytes de un sticker guardado (de un mensaje o de la colección).
    pub fn sticker_bytes(&self, id: &str) -> Result<Vec<u8>> {
        self.stickers.lock().unwrap().read(id)
    }

    /// Colección propia de stickers, del más reciente al más antiguo.
    pub fn stickers(&self) -> Vec<String> {
        self.stickers.lock().unwrap().list().to_vec()
    }

    /// Añade una imagen a la colección.
    pub fn add_sticker(&self, bytes: &[u8]) -> Result<String> {
        let mut st = self.stickers.lock().unwrap();
        let id = st.put(bytes)?;
        st.keep(&id)?;
        Ok(id)
    }

    /// Añade a la colección un sticker ya guardado (p. ej. uno recibido).
    pub fn keep_sticker(&self, id: &str) -> Result<()> {
        self.stickers.lock().unwrap().keep(id)
    }

    pub fn remove_sticker(&self, id: &str) -> Result<()> {
        self.stickers.lock().unwrap().remove(id)
    }

    // ---------------------------------------------------------- archivos

    fn register_file(&self, offer: &FileOffer, group_id: &[u8], sender: [u8; 32]) -> Result<FileRef> {
        let stored = StoredOffer { offer: offer.clone(), group_id: group_id.to_vec(), sender };
        let id = self.files.lock().unwrap().register(stored)?;
        Ok(FileRef { id, name: offer.name.clone(), size: offer.size })
    }

    /// El archivo ya está entero en este dispositivo.
    pub fn file_ready(&self, id: &str) -> bool {
        self.files.lock().unwrap().complete(id)
    }

    /// Descifra un archivo descargado para abrirlo con otra aplicación. La
    /// copia va a `abiertos/` y se borra al parar o arrancar el nodo.
    pub fn open_file(&self, id: &str) -> Result<PathBuf> {
        self.files.lock().unwrap().export(id, &self.dir.join(OPENED))
    }

    /// Borra las copias descifradas de los archivos abiertos.
    pub fn forget_opened(&self) {
        let _ = fs::remove_dir_all(self.dir.join(OPENED));
    }

    /// Descarga un archivo de los dispositivos del grupo que lo tengan
    /// (primero quien lo envió). Si se interrumpe, la siguiente vez sigue
    /// donde se quedó.
    pub async fn download(self: &Arc<Self>, id: &str) -> Result<()> {
        let (stored, (done, part)) = {
            let mut files = self.files.lock().unwrap();
            (files.get(id)?, files.path(id)?)
        };
        if done.exists() {
            return Ok(());
        }
        {
            let st = self.state.lock().unwrap();
            if st.chats.iter().any(|c| c.group_id == stored.group_id && !c.accepted) {
                bail!("acepta antes la solicitud de chat");
            }
        }
        if !self.downloading.lock().unwrap().insert(id.to_owned()) {
            bail!("ya se está descargando");
        }
        let result = self.fetch_file(id, stored, &done, &part).await;
        self.downloading.lock().unwrap().remove(id);
        result
    }

    async fn fetch_file(&self, id: &str, stored: StoredOffer, done: &Path, part: &Path) -> Result<()> {
        let me = self.device_pk();
        let mut sources: Vec<PeerId> = self
            .with_mls(|m| Ok(m.members(&stored.group_id)?))
            .context("ya no perteneces a este chat")?
            .into_iter()
            .filter(|m| m.device_pk != me && m.device_pk != stored.sender)
            .filter_map(|m| peer_id_of(&m.device_pk).ok())
            .collect();
        if stored.sender != me {
            sources.insert(0, peer_id_of(&stored.sender)?);
        }
        if sources.is_empty() {
            bail!("no hay nadie más en el chat a quien pedir el archivo");
        }
        let offer = stored.offer;
        let total = offer.chunks();
        let (partial, missing) = {
            let (part, offer) = (part.to_owned(), offer.clone());
            tokio::task::spawn_blocking(move || Partial::open(&part, offer)).await??
        };
        let partial = Mutex::new(partial);
        let queue = Mutex::new(VecDeque::from(missing));
        let mut got = total - queue.lock().unwrap().len() as u32;
        self.emit(Event::FileProgress { id: id.to_owned(), done: got, total });
        // Fuentes que no respondieron: no se les vuelve a preguntar (cada
        // intento fallido puede costar una búsqueda en la DHT).
        let dead: Mutex<HashSet<PeerId>> = Mutex::new(HashSet::new());
        let progress = Mutex::new(&mut got);
        let token = offer.token();
        let worker = |w: usize| {
            let (sources, partial, queue, dead, progress) = (&sources, &partial, &queue, &dead, &progress);
            async move {
                loop {
                    let Some(index) = queue.lock().unwrap().pop_front() else { return Ok(()) };
                    let mut ok = false;
                    // Cada trabajador empieza por una fuente distinta.
                    for k in 0..sources.len() {
                        let peer = sources[(w + k) % sources.len()];
                        if dead.lock().unwrap().contains(&peer) {
                            continue;
                        }
                        let req = Request::Chunk { file: id.to_owned(), index, token };
                        match self.net.request(peer, req).await {
                            Ok(Response::Chunk(bytes)) => match partial.lock().unwrap().write(index, &bytes) {
                                Ok(()) => {
                                    ok = true;
                                    break;
                                }
                                Err(e) => warn!(%peer, "trozo rechazado: {e:#}"),
                            },
                            // Responde pero no lo tiene (aún no lo ha descargado).
                            Ok(other) => debug!(%peer, "sin el trozo: {other:?}"),
                            Err(e) => {
                                debug!(%peer, "fuente no disponible: {e:#}");
                                dead.lock().unwrap().insert(peer);
                            }
                        }
                    }
                    if !ok {
                        queue.lock().unwrap().clear();
                        bail!("ningún dispositivo en línea tiene el archivo; prueba más tarde");
                    }
                    let mut n = progress.lock().unwrap();
                    **n += 1;
                    self.emit(Event::FileProgress { id: id.to_owned(), done: **n, total });
                }
            }
        };
        let results = join_all((0..FILE_WORKERS).map(worker)).await;
        results.into_iter().collect::<Result<Vec<()>>>()?;
        let partial = partial.into_inner().unwrap();
        let (part, done) = (part.to_owned(), done.to_owned());
        tokio::task::spawn_blocking(move || partial.finish(&part, &done)).await??;
        self.emit(Event::FileProgress { id: id.to_owned(), done: total, total });
        Ok(())
    }

    fn is_mine_in(&self, st: &State, root_pk: &[u8; 32]) -> bool {
        st.identity.as_ref().is_some_and(|v| v.record.root_pk == *root_pk)
    }

    /// Alguien con quien ya hablamos: comparte un chat 1:1 aceptado o un
    /// chat que creamos nosotros. Sus invitaciones no son solicitudes.
    fn trusted(&self, st: &State, suffix: &str) -> bool {
        let mine = |device: &[u8; 32]| st.identity.as_ref().is_some_and(|v| v.record.has_device(device));
        !st.blocked.contains(suffix)
            && st.chats.iter().any(|c| {
                c.accepted
                    && (direct_key(c).is_some() || mine(&c.meta.committer))
                    && c.meta.participants.iter().any(|p| suffix_of(p) == suffix)
            })
    }

    fn is_mine(&self, root_pk: &[u8; 32]) -> bool {
        self.state.lock().unwrap().identity.as_ref().is_some_and(|v| v.record.root_pk == *root_pk)
    }

    fn device_pk(&self) -> [u8; 32] {
        self.device.verifying_key().to_bytes()
    }

    fn save_chats(&self, st: &State) -> Result<()> {
        write_private(&self.dir.join("chats.cbor"), &cbor_to(&st.chats)?)
    }

    fn set_identity(&self, st: &mut State, verified: VerifiedRecord) -> Result<()> {
        write_private(&self.dir.join("identity.cbor"), &verified.signed.to_bytes()?)?;
        if st.mls.is_none() {
            st.mls = Some(MlsEngine::open(
                &self.device,
                verified.record.root_pk,
                Some(self.dir.join("mls.json")),
            )?);
        }
        st.identity = Some(verified);
        Ok(())
    }

    pub fn handle(&self) -> Option<Handle> {
        self.state.lock().unwrap().identity.as_ref().map(|v| v.handle.clone())
    }

    pub fn device_code(&self) -> String {
        encode_key(&self.device.verifying_key())
    }

    /// Contenido del QR para que otros nos añadan: handle, clave raíz y las
    /// direcciones de este dispositivo alcanzables desde otras redes.
    pub async fn qr_payload(&self) -> Option<String> {
        let mut qr = {
            let st = self.state.lock().unwrap();
            let id = st.identity.as_ref()?;
            let root = VerifyingKey::from_bytes(&id.record.root_pk).ok()?;
            format!("p2m:{}:{}", id.handle, encode_key(&root))
        };
        self.append_addrs(&mut qr).await;
        Some(qr)
    }

    /// Añade `@dirección` por cada dirección nuestra alcanzable desde otras
    /// redes (la onion, cuando ya está publicada).
    async fn append_addrs(&self, code: &mut String) {
        for a in self.net.listen_addrs().await.unwrap_or_default() {
            if is_dialable_from_anywhere(&a) {
                code.push('@');
                code.push_str(&a.to_string());
            }
        }
    }

    /// Código para que el primario vincule este dispositivo: la clave, un
    /// secreto de un solo uso (D-039) y las direcciones por las que llamarlo
    /// desde otra red (D-037). Mientras siga vigente se reutiliza el secreto.
    pub async fn link_code(&self) -> String {
        let token = current_token(&self.link_secret);
        let mut code =
            format!("{}:{}", self.device_code(), data_encoding::BASE32_NOPAD.encode(&token).to_lowercase());
        self.append_addrs(&mut code).await;
        code
    }

    // ---------------------------------------------------------------- identidad

    /// Crea una identidad nueva con este dispositivo como primario.
    pub async fn init(self: &Arc<Self>, name: &str) -> Result<Handle> {
        let verified = {
            let mut st = self.state.lock().unwrap();
            if st.identity.is_some() {
                bail!("este dispositivo ya tiene identidad");
            }
            let root = generate_key();
            let mut rec = IdentityRecord::new(&root, name)?;
            rec.add_device(self.device_pk(), "primario")?;
            let verified = rec.sign(&root)?.verify()?;
            write_private(&self.dir.join("root.key"), Zeroizing::new(root.to_bytes()).as_slice())?;
            st.root = Some(root);
            self.set_identity(&mut st, verified.clone())?;
            verified
        };
        // En segundo plano: con algún peer que no responde, publicar tarda
        // lo que el timeout de Kademlia, y el registro ya queda en local.
        let app = self.clone();
        tokio::spawn(async move { app.publish().await });
        Ok(verified.handle)
    }

    /// Publica nuestro registro en la DHT. Sin peers solo queda en local.
    pub async fn publish(&self) {
        let Some((key, value)) = self.state.lock().unwrap().identity.as_ref().and_then(|v| {
            Some((dht_key(&v.handle.suffix), v.signed.to_bytes().ok()?))
        }) else {
            return;
        };
        if let Err(e) = self.net.put_record(key, value).await {
            debug!("publicación del registro sin réplicas remotas: {e}");
        }
        match self.last_resort_key_package() {
            Ok(kp) => {
                if let Err(e) = self.net.put_record(key_package_key(&self.device_pk()), kp).await {
                    debug!("publicación del KeyPackage sin réplicas remotas: {e}");
                }
            }
            Err(e) => warn!("no se pudo preparar el KeyPackage de último recurso: {e:#}"),
        }
    }

    /// KeyPackage reutilizable que se publica en la DHT para que nos puedan
    /// añadir a grupos estando desconectados (D-017). Se renueva cada semana.
    fn last_resort_key_package(&self) -> Result<Vec<u8>> {
        let path = self.dir.join("lastresort.kp");
        let age = fs::metadata(&path)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok())
            .map(|d| d.as_secs());
        if let Some(age) = age
            && age < LAST_RESORT_MAX_AGE
        {
            return Ok(vault::read(&path)?);
        }
        let kp = self.with_mls(|m| Ok(m.last_resort_key_package()?))?;
        write_private(&path, &kp)?;
        Ok(kp)
    }

    /// Modifica la lista de dispositivos (requiere la clave raíz).
    async fn edit_devices(&self, f: impl FnOnce(&mut IdentityRecord) -> Result<()>) -> Result<VerifiedRecord> {
        let verified = {
            let mut st = self.state.lock().unwrap();
            let root = st.root.clone().context("solo el dispositivo primario tiene la clave raíz")?;
            let mut rec = st.identity.as_ref().context("sin identidad")?.record.clone();
            f(&mut rec)?;
            let verified = rec.sign(&root)?.verify()?;
            self.set_identity(&mut st, verified.clone())?;
            verified
        };
        self.publish().await;
        Ok(verified)
    }

    /// Vincula un dispositivo nuevo a partir de su código. Primero se
    /// comprueba que se llega a él: un dispositivo que no se entera de que
    /// está vinculado no debe quedarse en el registro (D-037).
    pub async fn link(self: &Arc<Self>, code: &str, label: &str) -> Result<()> {
        if self.state.lock().unwrap().root.is_none() {
            bail!("solo el dispositivo primario puede vincular otros");
        }
        let (device, secret, addrs) = parse_link_code(code)?;
        if device == self.device_pk() {
            return Err(LinkError::BadCode.into());
        }
        let peer = peer_id_of(&device)?;
        self.reach(peer, &addrs).await?;
        let was_linked = self.state.lock().unwrap().identity.as_ref().is_some_and(|v| v.record.has_device(&device));
        let label = clean_label(label);
        let verified = self.edit_devices(|rec| Ok(rec.add_device(device, &label)?)).await?;
        let grant = Request::LinkGrant { record: verified.signed.to_bytes()?, secret };
        let refused = match self.net.request(peer, grant).await {
            Ok(Response::Ack) => None,
            Ok(Response::Error(why)) => Some(LinkError::Refused(why)),
            Ok(other) => Some(LinkError::Refused(format!("{other:?}"))),
            Err(_) => Some(LinkError::Unreachable),
        };
        if let Some(e) = refused {
            if !was_linked {
                let _ = self.edit_devices(|rec| {
                    rec.remove_device(&device);
                    Ok(())
                })
                .await;
            }
            return Err(e.into());
        }
        // Al conectar aún no tenía identidad y no lo habrá admitido.
        self.share_with_own(peer).await;
        self.after_devices_changed().await;
        Ok(())
    }

    /// Invitación para que un dispositivo nuevo se vincule escaneándola
    /// (D-037). Mientras siga vigente se reutiliza el mismo token, así que
    /// se puede volver a pedir para que lleve las direcciones nuevas.
    pub async fn invite(&self) -> Result<String> {
        if self.state.lock().unwrap().root.is_none() {
            bail!("solo el dispositivo primario puede vincular otros");
        }
        let token = current_token(&self.invite);
        let mut code = format!(
            "{INVITE_PREFIX}{}:{}",
            self.device_code(),
            data_encoding::BASE32_NOPAD.encode(&token).to_lowercase()
        );
        self.append_addrs(&mut code).await;
        Ok(code)
    }

    /// Deja de valer la invitación (se cerró la pantalla que la mostraba).
    pub fn cancel_invite(&self) {
        *self.invite.lock().unwrap() = None;
    }

    /// Desde el dispositivo nuevo: entra en la identidad con la invitación
    /// que muestra el primario (D-037).
    pub async fn join(self: &Arc<Self>, code: &str, label: &str) -> Result<()> {
        if self.handle().is_some() {
            bail!("este dispositivo ya tiene identidad");
        }
        let (primary, token, addrs) = parse_invite(code)?;
        let peer = peer_id_of(&primary)?;
        self.reach(peer, &addrs).await?;
        let req = Request::Join { token, label: clean_label(label) };
        match self.net.request(peer, req).await {
            Ok(Response::Granted { record }) => {
                let verified = SignedRecord::from_bytes(&record)?.verify()?;
                if !verified.record.has_device(&primary) {
                    return Err(LinkError::Refused("la invitación no es de esa identidad".into()).into());
                }
                self.accept_grant(verified)
            }
            Ok(Response::Error(why)) => Err(LinkError::Refused(why).into()),
            Ok(other) => Err(LinkError::Refused(format!("{other:?}")).into()),
            Err(_) => Err(LinkError::Unreachable.into()),
        }
    }

    /// Intenta llegar a `peer` durante `LINK_TIMEOUT`: por las direcciones
    /// del código, la red local o la DHT. Entre redes distintas la onion
    /// del otro puede tardar un par de minutos en estar lista.
    async fn reach(&self, peer: PeerId, addrs: &[Multiaddr]) -> Result<(), LinkError> {
        let deadline = tokio::time::Instant::now() + LINK_TIMEOUT;
        let mut next_dial = tokio::time::Instant::now();
        loop {
            let now = tokio::time::Instant::now();
            if now >= next_dial {
                for a in addrs {
                    self.net.dial(a.clone()).await;
                }
                next_dial = now + LINK_REDIAL;
            }
            let left = deadline.saturating_duration_since(now);
            match tokio::time::timeout(left, self.net.request(peer, Request::Ping)).await {
                Ok(Ok(Response::Ack)) => return Ok(()),
                Ok(other) => debug!(%peer, "aún no se llega al otro dispositivo: {other:?}"),
                Err(_) => {}
            }
            if tokio::time::Instant::now() + Duration::from_secs(3) >= deadline {
                return Err(LinkError::Unreachable);
            }
            tokio::time::sleep(Duration::from_secs(3)).await;
        }
    }

    /// Acepta el registro que nos incluye: este dispositivo queda vinculado.
    fn accept_grant(&self, verified: VerifiedRecord) -> Result<()> {
        if !verified.record.has_device(&self.device_pk()) {
            bail!("el registro no incluye este dispositivo");
        }
        let mut st = self.state.lock().unwrap();
        match &st.identity {
            None => {}
            // Un registro anterior no puede deshacer cambios (D-039).
            Some(cur) if cur.record.root_pk == verified.record.root_pk => {
                if verified.record.seq < cur.record.seq {
                    bail!("el registro es más antiguo que el que ya tenemos");
                }
            }
            Some(_) => bail!("este dispositivo ya pertenece a otra identidad"),
        }
        let handle = verified.handle.clone();
        self.set_identity(&mut st, verified)?;
        self.emit(Event::Linked { handle: handle.to_string() });
        Ok(())
    }

    /// Revoca un dispositivo: deja de estar en el registro y se le expulsa
    /// de los grupos en la siguiente sincronización.
    pub async fn unlink(self: &Arc<Self>, code: &str) -> Result<()> {
        let device = decode_key(code)?.to_bytes();
        if device == self.device_pk() {
            bail!("no puedes revocar el dispositivo primario desde sí mismo");
        }
        self.edit_devices(|rec| {
            if rec.remove_device(&device) { Ok(()) } else { bail!("dispositivo no vinculado") }
        })
        .await?;
        self.after_devices_changed().await;
        Ok(())
    }

    /// Tras cambiar nuestros dispositivos: sincroniza los grupos que
    /// controlamos y avisa a los committers del resto.
    async fn after_devices_changed(self: &Arc<Self>) {
        let me = self.device_pk();
        let chats = self.state.lock().unwrap().chats.clone();
        for chat in chats.into_iter().filter(|c| c.active) {
            if chat.meta.committer == me {
                if let Err(e) = self.sync_group(&chat.group_id).await {
                    warn!("sincronización fallida: {e:#}");
                }
            } else if let Ok(peer) = peer_id_of(&chat.meta.committer) {
                let hint = Request::SyncHint { group_id: chat.group_id.clone() };
                let _ = self.net.request(peer, hint).await;
            }
        }
    }

    pub fn devices(&self) -> Vec<(String, String, bool)> {
        let st = self.state.lock().unwrap();
        let Some(id) = &st.identity else { return Vec::new() };
        id.record
            .devices
            .iter()
            .filter_map(|d| {
                let code = encode_key(&VerifyingKey::from_bytes(&d.device_pk).ok()?);
                Some((code, d.label.clone(), d.device_pk == self.device_pk()))
            })
            .collect()
    }

    // ---------------------------------------------------------------- resolución

    /// Resuelve un handle en la DHT y verifica el registro.
    pub async fn resolve(&self, handle: &Handle) -> Result<VerifiedRecord> {
        self.resolve_pinned(handle, None).await
    }

    /// Como `resolve`, pero solo acepta la raíz `root` (la del QR). Si ya
    /// conocíamos a ese contacto, su raíz queda fijada (D-041).
    pub async fn resolve_pinned(&self, handle: &Handle, root: Option<[u8; 32]>) -> Result<VerifiedRecord> {
        let mut candidates = self.net.get_records(dht_key(&handle.suffix)).await?;
        let mut pinned = root;
        {
            let st = self.state.lock().unwrap();
            let known = st
                .identity
                .as_ref()
                .filter(|v| &v.handle == handle)
                .or_else(|| st.contacts.values().find(|v| &v.handle == handle));
            if let Some(k) = known {
                if pinned.is_some_and(|p| p != k.record.root_pk) {
                    bail!("{handle} ya era otra identidad: el QR no coincide con la que conocías");
                }
                pinned = Some(k.record.root_pk);
                candidates.push(k.signed.to_bytes()?);
            }
        }
        let best = select_record(handle, candidates.iter().map(Vec::as_slice), pinned.as_ref())?
            .with_context(|| format!("no se encontró {handle} en la red"))?;
        let mut st = self.state.lock().unwrap();
        let known = st.contacts.get(&best.record.root_pk).map(|c| c.record.seq);
        if known != Some(best.record.seq) {
            st.contacts.insert(best.record.root_pk, best.clone());
            let all: Vec<Vec<u8>> = st.contacts.values().filter_map(|c| c.signed.to_bytes().ok()).collect();
            write_private(&self.dir.join("contacts.cbor"), &cbor_to(&all)?)?;
        }
        Ok(best)
    }

    /// Comprueba contra el registro firmado (resolviéndolo si hace falta)
    /// que el dispositivo de `m` pertenece a la raíz que declara. `handle`
    /// es el participante cuyo sufijo coincide con esa raíz.
    async fn device_verified(&self, m: &MemberId, handle: &str) -> bool {
        if Self::device_known(&self.state.lock().unwrap(), m) == Some(true) {
            return true;
        }
        // Sin registro o con uno que quizá esté desfasado: a la red.
        match handle.parse::<Handle>() {
            Ok(h) => self.resolve_pinned(&h, Some(m.root_pk)).await.is_ok_and(|r| r.record.has_device(&m.device_pk)),
            Err(_) => false,
        }
    }

    /// ¿Está el dispositivo de `m` en el registro firmado de su raíz?
    /// `None` si aún no tenemos ese registro.
    fn device_known(st: &State, m: &MemberId) -> Option<bool> {
        st.identity
            .as_ref()
            .filter(|v| v.record.root_pk == m.root_pk)
            .or_else(|| st.contacts.get(&m.root_pk))
            .map(|v| v.record.has_device(&m.device_pk))
    }

    /// Nombre para mostrar de un dispositivo. Si aún no tenemos el registro
    /// de su persona, se usa el participante del chat cuyo sufijo coincide
    /// con su clave raíz (el sufijo es verificable; el nombre aún no).
    fn display_name(&self, st: &State, m: &MemberId, participants: &[String]) -> String {
        let rec = st
            .identity
            .as_ref()
            .filter(|v| v.record.root_pk == m.root_pk)
            .or_else(|| st.contacts.get(&m.root_pk));
        match rec {
            Some(v) => {
                let label = v.record.devices.iter().find(|d| d.device_pk == m.device_pk);
                match label {
                    Some(d) => format!("{} ({})", v.handle, d.label),
                    None => format!("{} (dispositivo aún no verificado)", v.handle),
                }
            }
            None => {
                let Ok(root) = VerifyingKey::from_bytes(&m.root_pk) else { return "¿clave inválida?".into() };
                let suffix = lacre_core::identity::handle_suffix(&root);
                participants
                    .iter()
                    .filter_map(|p| p.parse::<Handle>().ok())
                    .find(|h| h.suffix == suffix)
                    .map_or_else(|| format!("desconocido#{suffix}"), |h| format!("{h} (sin verificar)"))
            }
        }
    }

    // ---------------------------------------------------------------- chats

    /// Llama a las direcciones de un QR y espera a que alguna conecte (o a
    /// que pase `QR_DIAL_TIMEOUT`). Por Tor puede tardar decenas de segundos.
    async fn dial_and_wait(&self, addrs: &[Multiaddr]) {
        let peers: Vec<PeerId> = addrs
            .iter()
            .filter_map(|a| a.iter().find_map(|p| match p {
                Protocol::P2p(peer) => Some(peer),
                _ => None,
            }))
            .collect();
        if peers.is_empty() {
            return;
        }
        for a in addrs {
            self.net.dial(a.clone()).await;
        }
        let deadline = tokio::time::Instant::now() + QR_DIAL_TIMEOUT;
        while tokio::time::Instant::now() < deadline {
            let connected = self.net.connected_peers().await.unwrap_or_default();
            if peers.iter().any(|p| connected.contains(p)) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        warn!("no se pudo conectar con las direcciones del QR");
    }

    /// Crea un chat con una o varias personas. Este dispositivo es el
    /// committer del grupo.
    pub async fn create_chat(self: &Arc<Self>, title: Option<String>, targets: Vec<Target>) -> Result<usize> {
        let me = self.handle().context("primero crea una identidad (init) o vincula el dispositivo")?;
        // Un chat 1:1 que ya existe no se duplica: se devuelve ese.
        let others: HashSet<&str> =
            targets.iter().map(|t| t.handle.suffix.as_str()).filter(|s| *s != me.suffix).collect();
        if title.is_none() && others.len() == 1 {
            let mut key = [me.suffix.clone(), others.iter().next().unwrap().to_string()];
            key.sort();
            let mut st = self.state.lock().unwrap();
            if let Some(i) = st.chats.iter().position(|c| direct_key(c).as_ref() == Some(&key)) {
                // Crear un chat que existe lo acepta y lo saca de borrados.
                for j in merged_indexes(&st.chats, i) {
                    st.chats[j].accepted = true;
                }
                st.chats[i].hidden = false;
                self.save_chats(&st)?;
                return Ok(i);
            }
        }
        let mut participants = vec![me.to_string()];
        for t in &targets {
            self.dial_and_wait(&t.addrs).await;
            self.resolve_pinned(&t.handle, t.root.map(|r| r.to_bytes())).await?;
            if !participants.contains(&t.handle.to_string()) {
                participants.push(t.handle.to_string());
            }
        }
        let group_id = {
            let mut st = self.state.lock().unwrap();
            let group_id = st.mls.as_ref().unwrap().create_group()?;
            st.chats.push(Chat {
                group_id: group_id.clone(),
                meta: ChatMeta { title, participants, committer: self.device_pk() },
                active: true,
                hidden: false,
                accepted: true,
                invited_by: None,
                left: false,
            });
            self.save_chats(&st)?;
            group_id
        };
        self.sync_group(&group_id).await?;
        let st = self.state.lock().unwrap();
        let index = st.chats.iter().position(|c| c.group_id == group_id).unwrap();
        Ok(canonical(&st.chats, index))
    }

    /// Participantes de un chat y si este dispositivo puede cambiarlos (es
    /// el committer y sigue en el grupo).
    pub fn chat_members(&self, index: usize) -> Result<(Vec<String>, bool)> {
        let st = self.state.lock().unwrap();
        let c = st.chats.get(canonical(&st.chats, index)).context("no existe ese chat")?;
        Ok((c.meta.participants.clone(), c.active && c.meta.committer == self.device_pk()))
    }

    /// Chat que este dispositivo puede cambiar (D-007: solo el committer).
    fn editable_chat(&self, index: usize) -> Result<Chat> {
        let st = self.state.lock().unwrap();
        let chat = st.chats.get(canonical(&st.chats, index)).cloned().context("no existe ese chat")?;
        if !chat.active {
            bail!("ya no perteneces a este chat");
        }
        if chat.meta.committer != self.device_pk() {
            bail!("solo puede añadir o quitar personas el dispositivo que creó el chat");
        }
        Ok(chat)
    }

    /// Número de seguridad con una persona conocida (D-043). Si coincide con
    /// el que ve ella, su handle no está suplantado.
    pub fn safety_number(&self, handle: &str) -> Result<String> {
        let st = self.state.lock().unwrap();
        let me = st.identity.as_ref().context("aún no tienes identidad")?;
        let other = st
            .contacts
            .values()
            .find(|v| v.handle.to_string() == handle)
            .with_context(|| format!("{handle} no es un contacto conocido"))?;
        Ok(lacre_core::identity::safety_number(&me.record.root_pk, &other.record.root_pk))
    }

    /// Añade personas a un chat existente (solo el committer).
    pub async fn add_members(self: &Arc<Self>, index: usize, targets: Vec<Target>) -> Result<()> {
        let chat = self.editable_chat(index)?;
        let mut meta = chat.meta.clone();
        for t in &targets {
            self.dial_and_wait(&t.addrs).await;
            self.resolve_pinned(&t.handle, t.root.map(|r| r.to_bytes())).await?;
            if !meta.participants.contains(&t.handle.to_string()) {
                meta.participants.push(t.handle.to_string());
            }
        }
        if meta.participants.len() == chat.meta.participants.len() {
            bail!("ya estaban todos en el chat");
        }
        // Primero la lista nueva (así los miembros conocen los nombres de
        // quienes entran) y después el commit que los añade, con el Welcome.
        self.update_meta(&chat, meta, false).await?;
        self.sync_group(&chat.group_id).await
    }

    /// Quita a una persona (todos sus dispositivos) de un chat (solo el
    /// committer).
    pub async fn remove_member(self: &Arc<Self>, index: usize, handle: &str) -> Result<()> {
        let chat = self.editable_chat(index)?;
        let handle = handle.trim().parse::<Handle>()?.to_string();
        if Some(&handle) == self.handle().map(|h| h.to_string()).as_ref() {
            bail!("no puedes quitarte: este dispositivo es el que gestiona el grupo");
        }
        if !chat.meta.participants.contains(&handle) {
            bail!("{handle} no está en el chat");
        }
        let mut meta = chat.meta.clone();
        meta.participants.retain(|p| *p != handle);
        // El aviso le llega también a quien sale; después, el commit que lo
        // expulsa.
        self.update_meta(&chat, meta, false).await?;
        self.sync_group(&chat.group_id).await
    }

    /// Guarda la nueva lista de participantes y la manda al grupo, firmada
    /// por el committer dentro de MLS.
    async fn update_meta(&self, chat: &Chat, meta: ChatMeta, leave: bool) -> Result<()> {
        let _guard = self.send_lock.lock().await;
        let sent_at = now();
        let (index, by) = {
            let mut st = self.state.lock().unwrap();
            let index = st.chats.iter().position(|c| c.group_id == chat.group_id).context("chat desconocido")?;
            st.chats[index].meta = meta.clone();
            self.save_chats(&st)?;
            let root_pk = st.identity.as_ref().context("sin identidad")?.record.root_pk;
            (index, participant_of(&meta.participants, &root_pk))
        };
        if let Some(text) = meta_notice(&chat.meta, &meta, &by, leave) {
            self.record(index, &chat.group_id, StoredMessage { from: by, text, sent_at, mine: true, notice: true, sticker: None, file: None }, true);
        }
        let body = cbor_to(&AppMessage { text: String::new(), sent_at, meta: Some(meta), sticker: None, file: None, avatar: 0, leave })?;
        let (msg, members) =
            self.with_mls(|m| Ok((m.encrypt(&chat.group_id, &body)?, m.members(&chat.group_id)?)))?;
        self.fan_out(&members, Request::Mls(msg)).await;
        Ok(())
    }

    pub fn chats(&self) -> Vec<ChatSummary> {
        let st = self.state.lock().unwrap();
        let history = self.history.lock().unwrap();
        let avatars = self.avatars.lock().unwrap();
        let me = st.identity.as_ref().map(|v| v.handle.to_string());
        let my_suffix = st.identity.as_ref().map(|v| v.handle.suffix.clone()).unwrap_or_default();
        st.chats
            .iter()
            .enumerate()
            .filter(|(index, _)| self.visible(&st, *index))
            .map(|(index, c)| {
                let devices = st.mls.as_ref().and_then(|m| m.members(&c.group_id).ok()).map_or(0, |m| m.len());
                let name = chat_name(&c.meta, me.as_deref());
                let last = merged_groups(&st.chats, index)
                    .iter()
                    .filter_map(|g| history.last(g))
                    .max_by_key(|m| m.sent_at)
                    .cloned();
                let peer = direct_peer(c, &my_suffix);
                let avatar = peer.as_ref().filter(|_| c.accepted).is_some_and(|p| avatars.path(&suffix_of(p)).is_some());
                ChatSummary { index, name, devices, active: c.active, last, request: !c.accepted, peer, avatar }
            })
            .collect()
    }

    /// Si el chat `index` sale en la lista: es el canónico de sus duplicados
    /// 1:1, no está borrado y no lo oculta un bloqueo.
    fn visible(&self, st: &State, index: usize) -> bool {
        let my_suffix = st.identity.as_ref().map(|v| v.handle.suffix.as_str()).unwrap_or_default();
        let c = &st.chats[index];
        canonical(&st.chats, index) == index && !c.hidden && !blocked_chat(c, &st.blocked, my_suffix)
    }

    /// Borra un chat de este dispositivo: su historial y de la lista. Un 1:1
    /// vuelve a aparecer (vacío) si llega otro mensaje. De un grupo, además,
    /// se sale: se le pide al committer que nos quite (D-031). Si el
    /// committer somos nosotros, solo se oculta (el grupo sigue para los
    /// demás).
    pub fn delete_chat(self: &Arc<Self>, index: usize) -> Result<()> {
        let leave = {
            let mut st = self.state.lock().unwrap();
            st.chats.get(index).context("no existe ese chat")?;
            let index = canonical(&st.chats, index);
            let merged = merged_indexes(&st.chats, index);
            let mut history = self.history.lock().unwrap();
            for &i in &merged {
                st.chats[i].hidden = true;
                history.clear(&st.chats[i].group_id)?;
            }
            let c = &mut st.chats[index];
            let leave = is_group(c) && c.active && !c.left && c.meta.committer != self.device_pk();
            if leave {
                c.left = true;
            }
            let chat = c.clone();
            self.save_chats(&st)?;
            leave.then_some(chat)
        };
        if let Some(chat) = leave {
            let app = self.clone();
            self.rt.spawn(async move {
                let msg = AppMessage { text: String::new(), sent_at: now(), meta: None, sticker: None, file: None, avatar: 0, leave: true };
                if let Err(e) = app.deliver(&chat, msg).await {
                    warn!("no se pudo pedir la salida del grupo: {e:#}");
                }
            });
        }
        Ok(())
    }

    /// Atiende las peticiones de salida que han llegado a grupos nuestros.
    fn serve_leaves(self: &Arc<Self>) {
        let leaves = std::mem::take(&mut self.state.lock().unwrap().leaves);
        for (group_id, root_pk) in leaves {
            let app = self.clone();
            self.rt.spawn(async move {
                if let Err(e) = app.remove_person(&group_id, &root_pk).await {
                    warn!("no se pudo atender una salida del grupo: {e:#}");
                }
            });
        }
    }

    /// Quita del grupo a quien pidió salir, con un aviso de que salió.
    async fn remove_person(self: &Arc<Self>, group_id: &[u8], root_pk: &[u8; 32]) -> Result<()> {
        let chat = self.chat(group_id)?;
        let handle = participant_of(&chat.meta.participants, root_pk);
        if !chat.active || !chat.meta.participants.contains(&handle) {
            return Ok(());
        }
        let mut meta = chat.meta.clone();
        meta.participants.retain(|p| *p != handle);
        self.update_meta(&chat, meta, true).await?;
        self.sync_group(group_id).await
    }

    /// Acepta una solicitud de chat: desde ahora se muestran sus imágenes y
    /// se pueden descargar sus archivos.
    pub fn accept_chat(self: &Arc<Self>, index: usize) -> Result<()> {
        let (groups, devices) = {
            let mut st = self.state.lock().unwrap();
            st.chats.get(index).context("no existe ese chat")?;
            let mut groups = Vec::new();
            for i in merged_indexes(&st.chats, index) {
                st.chats[i].accepted = true;
                groups.push(st.chats[i].group_id.clone());
            }
            self.save_chats(&st)?;
            (groups, self.my_other_devices(&st))
        };
        self.fetch_avatars();
        // Los demás dispositivos nuestros también la dan por aceptada.
        let app = self.clone();
        tokio::spawn(async move { app.fan_out(&devices, Request::Accepted { groups }).await });
        Ok(())
    }

    fn my_other_devices(&self, st: &State) -> Vec<MemberId> {
        let me = self.device_pk();
        st.identity.as_ref().map_or_else(Vec::new, |v| {
            v.record
                .devices
                .iter()
                .filter(|d| d.device_pk != me)
                .map(|d| MemberId { root_pk: v.record.root_pk, device_pk: d.device_pk })
                .collect()
        })
    }

    /// Al conectar con otro dispositivo nuestro, le decimos qué chats
    /// tenemos aceptados (p. ej. si se acaba de vincular).
    pub async fn on_connected(self: &Arc<Self>, peer: PeerId) {
        // Si guardamos sobres para él, se los llevamos ya.
        if let Some(to) = device_pk_of(&peer)
            && self.mailbox.lock().unwrap().has(&to)
        {
            let ok = self.push_to(&to).await;
            self.mailbox.lock().unwrap().pushed(&to, ok);
        }
        self.share_with_own(peer).await;
    }

    /// Manda a otro dispositivo nuestro los chats aceptados y los bloqueos.
    async fn share_with_own(&self, peer: PeerId) {
        let (groups, list) = {
            let st = self.state.lock().unwrap();
            let ours = self.my_other_devices(&st).iter().any(|m| peer_id_of(&m.device_pk).ok() == Some(peer));
            if !ours {
                return;
            }
            (st.chats.iter().filter(|c| c.accepted).map(|c| c.group_id.clone()).collect::<Vec<_>>(), st.blocked.entries())
        };
        if !groups.is_empty() {
            let _ = self.net.request(peer, Request::Accepted { groups }).await;
        }
        if !list.is_empty() {
            let _ = self.net.request(peer, Request::Blocks { list }).await;
        }
    }

    /// Bloquea a una persona: se ignoran sus mensajes y se ocultan sus chats
    /// 1:1 y sus invitaciones sin aceptar.
    /// El bloqueo vale en todos nuestros dispositivos (D-031).
    pub fn block(self: &Arc<Self>, handle: &str) -> Result<()> {
        let h: Handle = handle.trim().parse().context("handle no válido (nombre#sufijo)")?;
        {
            let mut st = self.state.lock().unwrap();
            if st.identity.as_ref().is_some_and(|v| v.handle.suffix == h.suffix) {
                bail!("no puedes bloquearte a ti");
            }
            st.blocked.set(h.suffix.clone(), h.to_string(), true, now())?;
        }
        self.share_blocks();
        Ok(())
    }

    /// Desbloquea por handle o por sufijo, en todos nuestros dispositivos.
    pub fn unblock(self: &Arc<Self>, handle: &str) -> Result<()> {
        {
            let mut st = self.state.lock().unwrap();
            let suffix = suffix_of(handle.trim());
            let Some(entry) = st.blocked.entries().remove(&suffix).filter(|e| e.blocked) else {
                bail!("{handle} no estaba bloqueado");
            };
            st.blocked.set(suffix, entry.handle, false, now())?;
        }
        self.share_blocks();
        Ok(())
    }

    /// Manda la lista de bloqueos a nuestros otros dispositivos (por buzón
    /// si no están).
    fn share_blocks(self: &Arc<Self>) {
        let (devices, list) = {
            let st = self.state.lock().unwrap();
            (self.my_other_devices(&st), st.blocked.entries())
        };
        if devices.is_empty() {
            return;
        }
        let app = self.clone();
        self.rt.spawn(async move { app.fan_out(&devices, Request::Blocks { list }).await });
    }

    pub fn blocked(&self) -> Vec<String> {
        self.state.lock().unwrap().blocked.handles()
    }

    pub fn is_blocked(&self, handle: &str) -> bool {
        self.state.lock().unwrap().blocked.contains(&suffix_of(handle))
    }

    /// Busca texto (sin distinguir mayúsculas ni tildes) en los mensajes y
    /// nombres de archivo de un chat o, sin `index`, de todos. Del más
    /// reciente al más antiguo.
    pub fn search(&self, query: &str, index: Option<usize>) -> Result<Vec<SearchHit>> {
        let q = fold(query.trim());
        if q.is_empty() {
            return Ok(Vec::new());
        }
        let chats: Vec<usize> = match index {
            Some(i) => vec![canonical(&self.state.lock().unwrap().chats, i)],
            None => {
                let st = self.state.lock().unwrap();
                (0..st.chats.len()).filter(|&i| self.visible(&st, i)).collect()
            }
        };
        let mut hits = Vec::new();
        for index in chats {
            for (position, message) in self.messages(index)?.into_iter().enumerate() {
                let file = message.file.as_ref().map(|f| f.name.as_str()).unwrap_or_default();
                if !message.notice && (fold(&message.text).contains(&q) || fold(file).contains(&q)) {
                    hits.push(SearchHit { index, position, message });
                }
            }
        }
        hits.sort_by_key(|h| std::cmp::Reverse(h.message.sent_at));
        hits.truncate(MAX_HITS);
        Ok(hits)
    }

    /// Ajusta los dispositivos del grupo a los registros actuales de sus
    /// participantes: añade los nuevos y expulsa los revocados.
    pub async fn sync_group(self: &Arc<Self>, group_id: &[u8]) -> Result<()> {
        let _guard = self.send_lock.lock().await;
        let chat = self.chat(group_id)?;
        if chat.meta.committer != self.device_pk() {
            bail!("solo el committer puede sincronizar el grupo");
        }

        // Dispositivos que deberían estar, por persona. Si un registro no se
        // puede resolver, no se toca a esa persona (no expulsar por un fallo
        // de red): se reconocen sus dispositivos por el sufijo del handle.
        let mut desired: HashSet<MemberId> = HashSet::new();
        let mut known_roots: HashSet<[u8; 32]> = HashSet::new();
        let mut unresolved: HashSet<String> = HashSet::new();
        for p in &chat.meta.participants {
            let handle: Handle = p.parse()?;
            let Ok(rec) = self.resolve(&handle).await else {
                warn!("no se pudo resolver {p}; se mantiene tal cual");
                unresolved.insert(handle.suffix);
                continue;
            };
            known_roots.insert(rec.record.root_pk);
            for d in &rec.record.devices {
                desired.insert(MemberId { root_pk: rec.record.root_pk, device_pk: d.device_pk });
            }
        }
        let current: HashSet<MemberId> = self.with_mls(|m| Ok(m.members(group_id)?))?.into_iter().collect();
        // Sale el dispositivo revocado de un participante y cualquiera de una
        // persona que ya no participa (la han quitado).
        let me = self.device_pk();
        let to_remove: Vec<MemberId> = current
            .iter()
            .filter(|m| m.device_pk != me)
            .filter(|m| {
                if known_roots.contains(&m.root_pk) {
                    return !desired.contains(m);
                }
                let suffix = VerifyingKey::from_bytes(&m.root_pk).map(|k| handle_suffix(&k));
                !suffix.is_ok_and(|s| unresolved.contains(&s))
            })
            .copied()
            .collect();
        let to_add: Vec<MemberId> = desired.difference(&current).copied().collect();

        if !to_remove.is_empty() {
            let commit = self.with_mls(|m| Ok(m.remove(group_id, &to_remove)?))?;
            // Los expulsados también reciben el commit para saber que salen.
            let recipients: Vec<MemberId> = current.iter().copied().collect();
            self.fan_out(&recipients, Request::Mls(commit)).await;
        }

        // KeyPackages de los dispositivos nuevos que estén en línea.
        let kps = join_all(to_add.iter().map(|m| self.fetch_key_package(*m))).await;
        let kps: Vec<_> = kps
            .into_iter()
            .zip(&to_add)
            .filter_map(|(r, m)| r.map_err(|e| debug!("sin KeyPackage de {:?}: {e:#}", &m.device_pk[..4])).ok())
            .collect();
        if !kps.is_empty() {
            let existing = self.with_mls(|m| Ok(m.members(group_id)?))?;
            let (commit, welcome) = self.with_mls(|m| Ok(m.add(group_id, &kps)?))?;
            self.fan_out(&existing, Request::Mls(commit)).await;
            let after = self.with_mls(|m| Ok(m.members(group_id)?))?;
            let new: Vec<MemberId> = to_add.iter().filter(|m| after.contains(m)).copied().collect();
            self.fan_out(&new, Request::Welcome { welcome, meta: chat.meta.clone() }).await;
        }
        Ok(())
    }

    /// KeyPackage de un dispositivo: uno nuevo si está en línea; si no, el de
    /// último recurso que publicó en la DHT.
    async fn fetch_key_package(&self, expected: MemberId) -> Result<KeyPackage> {
        match self.fetch_fresh_key_package(expected).await {
            Ok(kp) => Ok(kp),
            Err(e) => {
                debug!("sin KeyPackage directo ({e:#}); probando la DHT");
                let found = self.net.get_records(key_package_key(&expected.device_pk)).await?;
                found
                    .iter()
                    .filter_map(|b| verify_key_package(b).ok())
                    .find(|(kp, id)| *id == expected && kp.last_resort())
                    .map(|(kp, _)| kp)
                    .context("el dispositivo no está en línea ni tiene KeyPackage publicado")
            }
        }
    }

    async fn fetch_fresh_key_package(&self, expected: MemberId) -> Result<KeyPackage> {
        let resp = self.net.request(peer_id_of(&expected.device_pk)?, Request::KeyPackage).await?;
        let Response::KeyPackage(bytes) = resp else { bail!("respuesta inesperada: {resp:?}") };
        let (kp, id) = self.with_mls(|m| Ok(m.validate_key_package(&bytes)?))?;
        if id != expected {
            bail!("el KeyPackage no corresponde al dispositivo esperado");
        }
        Ok(kp)
    }

    pub async fn send(self: &Arc<Self>, index: usize, text: &str) -> Result<SendReport> {
        self.send_app(index, text.to_owned(), None, None).await
    }

    /// Envía un archivo (hasta [`files::MAX_FILE`]). Se cifra y se copia al
    /// almacén; los demás lo descargan de quien lo tenga.
    pub async fn send_file(self: &Arc<Self>, index: usize, path: &Path) -> Result<SendReport> {
        let dir = self.dir.join("files");
        let path = path.to_owned();
        let offer = tokio::task::spawn_blocking(move || files::prepare(&dir, &path)).await??;
        self.send_app(index, String::new(), None, Some(offer)).await
    }

    /// Envía un sticker de la colección (o cualquiera ya guardado).
    pub async fn send_sticker(self: &Arc<Self>, index: usize, id: &str) -> Result<SendReport> {
        let bytes = self.stickers.lock().unwrap().read(id)?;
        self.send_sticker_bytes(index, bytes).await
    }

    /// Envía una imagen como sticker (PNG, GIF, WebP o JPEG de hasta
    /// [`sticker::MAX_STICKER`]).
    pub async fn send_sticker_bytes(self: &Arc<Self>, index: usize, bytes: Vec<u8>) -> Result<SendReport> {
        sticker::check(&bytes)?;
        self.send_app(index, String::new(), Some(bytes), None).await
    }

    async fn send_app(
        self: &Arc<Self>,
        index: usize,
        text: String,
        sticker: Option<Vec<u8>>,
        file: Option<FileOffer>,
    ) -> Result<SendReport> {
        let stored_sticker = match &sticker {
            Some(b) => Some(self.stickers.lock().unwrap().put(b)?),
            None => None,
        };
        let (index, chat) = {
            let st = self.state.lock().unwrap();
            let index = canonical(&st.chats, index);
            (index, st.chats.get(index).cloned().context("no existe ese chat")?)
        };
        if !chat.active {
            bail!("ya no perteneces a este chat");
        }
        let file_ref = match &file {
            Some(offer) => Some(self.register_file(offer, &chat.group_id, self.device_pk())?),
            None => None,
        };
        // Se guarda y se muestra ya: lo que viene después (sincronizar,
        // llamar a dispositivos desconectados, buzones) puede tardar minutos.
        let sent_at = now();
        let from = {
            let st = self.state.lock().unwrap();
            let root_pk = st.identity.as_ref().context("sin identidad")?.record.root_pk;
            self.display_name(&st, &MemberId { root_pk, device_pk: self.device_pk() }, &chat.meta.participants)
        };
        let message = StoredMessage { from, text: text.clone(), sent_at, mine: true, notice: false, sticker: stored_sticker, file: file_ref };
        self.record(index, &chat.group_id, message, true);
        // El committer comprueba antes si hay dispositivos nuevos o revocados.
        if chat.meta.committer == self.device_pk()
            && let Err(e) = self.sync_group(&chat.group_id).await
        {
            warn!("sincronización previa fallida: {e:#}");
        }
        let avatar = self.my_avatar_version();
        self.deliver(&chat, AppMessage { text, sent_at, meta: None, sticker, file, avatar, leave: false }).await
    }

    async fn deliver(&self, chat: &Chat, msg: AppMessage) -> Result<SendReport> {
        let _guard = self.send_lock.lock().await;
        let body = cbor_to(&msg)?;
        let (msg, members) = self.with_mls(|m| {
            Ok((m.encrypt(&chat.group_id, &body)?, m.members(&chat.group_id)?))
        })?;
        let d = self.fan_out(&members, Request::Mls(msg)).await;
        Ok(SendReport { stored: d.stored, failed: d.failed })
    }

    // ------------------------------------------------------- fotos de perfil

    fn my_suffix(&self) -> Option<String> {
        self.state.lock().unwrap().identity.as_ref().map(|v| v.handle.suffix.clone())
    }

    fn my_avatar_version(&self) -> u64 {
        self.my_suffix().map_or(0, |s| self.avatars.lock().unwrap().version(&s))
    }

    /// Foto de perfil de un handle (o la nuestra): (versión, bytes).
    pub fn avatar(&self, handle: &str) -> Result<(u64, Option<Vec<u8>>)> {
        self.avatars.lock().unwrap().read(&suffix_of(handle))
    }

    /// Cambia (o quita, con `None`) nuestra foto y la anuncia en los chats
    /// aceptados. Quien la quiera la pide a este dispositivo.
    pub async fn set_avatar(self: &Arc<Self>, image: Option<Vec<u8>>) -> Result<()> {
        if let Some(b) = &image {
            people::check_avatar(b)?;
        }
        let me = self.my_suffix().context("primero crea una identidad")?;
        let version = {
            let mut avatars = self.avatars.lock().unwrap();
            let version = now().max(avatars.version(&me) + 1);
            avatars.put(&me, version, image.as_deref())?;
            version
        };
        self.emit(Event::Avatar { handle: self.handle().map(|h| h.to_string()).unwrap_or_default() });
        let chats: Vec<Chat> = {
            let st = self.state.lock().unwrap();
            (0..st.chats.len())
                .filter(|&i| self.visible(&st, i) && st.chats[i].active && st.chats[i].accepted)
                .map(|i| st.chats[i].clone())
                .collect()
        };
        for chat in chats {
            let msg = AppMessage { text: String::new(), sent_at: now(), meta: None, sticker: None, file: None, avatar: version, leave: false };
            if let Err(e) = self.deliver(&chat, msg).await {
                debug!("aviso de foto nueva: {e:#}");
            }
        }
        Ok(())
    }

    /// Si `suffix` comparte con nosotros algún chat aceptado (o somos
    /// nosotros): solo entonces se descarga su foto.
    fn approved(&self, st: &State, suffix: &str) -> bool {
        st.identity.as_ref().is_some_and(|v| v.handle.suffix == suffix)
            || (!st.blocked.contains(suffix)
                && st.chats.iter().any(|c| c.accepted && c.meta.participants.iter().any(|p| suffix_of(p) == suffix)))
    }

    /// Pide las fotos anunciadas que faltan a quien las anunció.
    fn fetch_avatars(self: &Arc<Self>) {
        let wanted = self.avatars.lock().unwrap().wanted();
        for (suffix, version, device) in wanted {
            if !self.approved(&self.state.lock().unwrap(), &suffix) {
                continue;
            }
            let app = self.clone();
            tokio::spawn(async move {
                if let Err(e) = app.fetch_avatar(&suffix, version, device).await {
                    debug!(%suffix, "foto de perfil: {e:#}");
                }
            });
        }
    }

    async fn fetch_avatar(&self, suffix: &str, version: u64, device: [u8; 32]) -> Result<()> {
        let resp = self.net.request(peer_id_of(&device)?, Request::Avatar).await?;
        let Response::Avatar { version: got, image } = resp else { bail!("respuesta inesperada: {resp:?}") };
        if got < version {
            bail!("el dispositivo tiene una versión anterior");
        }
        self.avatars.lock().unwrap().put(suffix, got, image.as_deref())?;
        let handle = {
            let st = self.state.lock().unwrap();
            st.chats
                .iter()
                .flat_map(|c| &c.meta.participants)
                .find(|p| suffix_of(p) == suffix)
                .cloned()
                .unwrap_or_else(|| suffix.to_owned())
        };
        self.emit(Event::Avatar { handle });
        Ok(())
    }

    /// Envía a todos los dispositivos indicados salvo a este. A los que no
    /// responden se les deja el mensaje en buzones.
    async fn fan_out(&self, members: &[MemberId], req: Request) -> Delivery {
        let me = self.device_pk();
        let sends = members.iter().filter(|m| m.device_pk != me).map(|m| {
            let req = req.clone();
            async move {
                let start = Instant::now();
                let direct = async {
                    match self.net.request(peer_id_of(&m.device_pk)?, req.clone()).await? {
                        Response::Ack => Ok(()),
                        other => Err(anyhow!("{other:?}")),
                    }
                };
                let direct = direct.await;
                debug!(ms = start.elapsed().as_millis(), ok = direct.is_ok(), "envío directo");
                match direct {
                    Ok(()) => Ok(true),
                    Err(e) => {
                        debug!("envío directo fallido ({e:#}); a buzones");
                        self.deposit(&m.device_pk, &req).await.map(|_| false)
                    }
                }
            }
        });
        let mut d = Delivery::default();
        for r in join_all(sends).await {
            match r {
                Ok(true) => d.direct += 1,
                Ok(false) => d.stored += 1,
                Err(e) => {
                    debug!("no se pudo dejar en buzón: {e:#}");
                    d.failed += 1;
                }
            }
        }
        d
    }

    // ---------------------------------------------------------------- buzones

    /// Sella la petición para `to` y la deja en nuestro propio buzón y en
    /// los de los nodos más cercanos a su clave de buzón. A estos se llega
    /// con el mensajero (D-034): ni el sobre ni la conexión dicen quién
    /// escribe. Solo si así no se guarda ninguna copia se deposita con
    /// nuestra identidad. Devuelve cuántas copias remotas se guardaron.
    async fn deposit(&self, to: &[u8; 32], req: &Request) -> Result<usize> {
        let env = Envelope::seal(&self.device, to, &cbor_to(req)?)?;
        self.mailbox.lock().unwrap().deposit(env.clone())?;
        let recipient = peer_id_of(to)?;
        let peers: Vec<PeerId> = self
            .net
            .closest_peers(mailbox_key(to))
            .await
            .unwrap_or_default()
            .into_iter()
            .filter(|p| *p != recipient)
            .take(MAILBOX_REPLICAS)
            .collect();
        let anon = self
            .net
            .send_anonymously(peers.clone(), Request::Deposit(env.clone()))
            .await
            .unwrap_or_else(|e| {
                debug!("mensajero: {e:#}");
                Default::default()
            });
        let mut copies = anon.values().filter(|r| matches!(r, Response::Ack)).count();
        if copies == 0 {
            let stores = peers.into_iter().map(|p| {
                let env = env.clone();
                async move { matches!(self.net.request(p, Request::Deposit(env)).await, Ok(Response::Ack)) }
            });
            copies = join_all(stores).await.into_iter().filter(|ok| *ok).count();
            debug!(copies, "sobre depositado con nuestra identidad");
        } else {
            debug!(copies, "sobre depositado de forma anónima");
        }
        Ok(copies)
    }

    /// Vacía nuestros buzones en los peers indicados.
    pub async fn fetch_mailboxes(self: &Arc<Self>, peers: Vec<PeerId>) {
        if self.state.lock().unwrap().mls.is_none() {
            return;
        }
        for peer in peers {
            // Un buzón no devuelve más de unos MiB por vez.
            for _ in 0..16 {
                let Ok(Response::Envelopes { list, more }) = self.net.request(peer, Request::Fetch).await else {
                    break;
                };
                if list.is_empty() {
                    break;
                }
                let ids: Vec<[u8; 32]> = list.iter().map(Envelope::id).collect();
                for env in list {
                    if let Err(e) = self.open_envelope(env).await {
                        debug!(%peer, "sobre descartado: {e:#}");
                    }
                }
                let _ = self.net.request(peer, Request::AckEnvelopes { ids }).await;
                if !more {
                    break;
                }
            }
        }
    }

    /// Vacía los buzones de los peers conectados y de los más cercanos a
    /// nuestra clave de buzón.
    async fn fetch_all_mailboxes(self: &Arc<Self>) {
        let mut peers = self.net.closest_peers(mailbox_key(&self.device_pk())).await.unwrap_or_default();
        for p in self.net.connected_peers().await.unwrap_or_default() {
            if !peers.contains(&p) {
                peers.push(p);
            }
        }
        self.fetch_mailboxes(peers).await;
    }

    /// Entrega activa (D-034): lleva los sobres del buzón a los destinatarios
    /// a los que toca, buscando su dirección (onion incluida) en la DHT.
    /// Una sola ronda a la vez.
    fn push_mailbox(self: &Arc<Self>) {
        use std::sync::atomic::Ordering;
        if self.pushing.swap(true, Ordering::AcqRel) {
            return;
        }
        let this = self.clone();
        tokio::spawn(async move {
            let me = this.device_pk();
            let due = {
                let mut mb = this.mailbox.lock().unwrap();
                if let Err(e) = mb.flush() {
                    warn!("buzón: {e:#}");
                }
                mb.due()
            };
            let pushes = due.into_iter().filter(|to| *to != me).map(|to| {
                let this = this.clone();
                async move {
                    let ok = this.push_to(&to).await;
                    this.mailbox.lock().unwrap().pushed(&to, ok);
                }
            });
            join_all(pushes).await;
            this.pushing.store(false, Ordering::Release);
        });
    }

    /// Entrega a `to` lo que le guardamos. `true` si se lo llevó todo.
    async fn push_to(&self, to: &[u8; 32]) -> bool {
        let Ok(peer) = peer_id_of(to) else { return false };
        for _ in 0..16 {
            let (list, _) = self.mailbox.lock().unwrap().take(to, crate::mailbox::MAX_DELIVER_BYTES);
            if list.is_empty() {
                return true;
            }
            let ids = match self.net.request(peer, Request::Deliver { list }).await {
                Ok(Response::Taken { ids }) if !ids.is_empty() => ids,
                Ok(other) => {
                    debug!(%peer, "entrega rechazada: {other:?}");
                    return false;
                }
                Err(e) => {
                    debug!(%peer, "entrega fallida: {e:#}");
                    return false;
                }
            };
            debug!(%peer, n = ids.len(), "sobres entregados");
            if let Err(e) = self.mailbox.lock().unwrap().ack(to, &ids) {
                warn!("buzón: {e:#}");
                return false;
            }
        }
        false
    }

    /// Cuántos sobres más de `peer` se aceptan ahora por `Deliver`.
    fn deliver_budget(&self, peer: PeerId, wanted: usize) -> usize {
        let t = now();
        let mut d = self.delivered.lock().unwrap();
        d.retain(|_, (start, _)| *start + DELIVER_WINDOW > t);
        let (_, used) = d.entry(peer).or_insert((t, 0));
        let n = wanted.min(DELIVER_BUDGET.saturating_sub(*used));
        *used += n;
        n
    }

    /// Procesa un sobre de un buzón como si la petición llegara directamente
    /// de su remitente.
    async fn open_envelope(self: &Arc<Self>, env: Envelope) -> Result<()> {
        let id = env.id();
        if self.state.lock().unwrap().seen.contains(&id) {
            return Ok(());
        }
        // Solo cuentan como vistos los sobres auténticos: la basura no debe
        // poder desplazar de la lista los ids reales (D-042).
        let (from, sent, plaintext) = env.open_dated(&self.device)?;
        {
            let mut st = self.state.lock().unwrap();
            if !st.seen.insert(id, sent, now()) {
                return Ok(());
            }
            write_private(&self.dir.join("seen.cbor"), &cbor_to(&st.seen)?)?;
        }
        let req: Request = cbor_from(&plaintext)?;
        // Solo lo que tiene sentido diferir; nada de peticiones a buzones.
        if !matches!(req, Request::Welcome { .. } | Request::Mls(_) | Request::SyncHint { .. } | Request::Accepted { .. } | Request::Blocks { .. }) {
            bail!("petición no admitida en un sobre");
        }
        Box::pin(self.on_request(peer_id_of(&from)?, req)).await?;
        Ok(())
    }

    fn chat(&self, group_id: &[u8]) -> Result<Chat> {
        let st = self.state.lock().unwrap();
        st.chats.iter().find(|c| c.group_id == group_id).cloned().context("chat desconocido")
    }

    fn with_mls<T>(&self, f: impl FnOnce(&MlsEngine) -> Result<T>) -> Result<T> {
        let st = self.state.lock().unwrap();
        f(st.mls.as_ref().context("sin identidad")?)
    }

    // ---------------------------------------------------------------- entrantes

    pub async fn handle_inbound(self: &Arc<Self>, inbound: Inbound) {
        let Inbound { peer, req, channel } = inbound;
        let resp = match self.on_request(peer, req).await {
            Ok(resp) => resp,
            Err(e) => {
                debug!(%peer, "petición rechazada: {e:#}");
                // Solo el mensaje externo (sin la cadena de causas, que puede
                // llevar rutas o detalles internos) y acotado (D-042).
                Response::Error(e.to_string().chars().take(200).collect())
            }
        };
        self.net.respond(channel, resp).await;
    }

    async fn on_request(self: &Arc<Self>, peer: PeerId, req: Request) -> Result<Response> {
        match req {
            Request::KeyPackage => {
                {
                    let mut l = self.kp_limit.lock().unwrap();
                    if !l.0.take(peer) || !l.1.take(()) {
                        bail!("demasiadas peticiones de KeyPackage");
                    }
                }
                Ok(Response::KeyPackage(self.with_mls(|m| Ok(m.key_package()?))?))
            }
            Request::LinkGrant { record, secret } => {
                // Solo de quien leyó nuestro código (D-039) y desde un
                // dispositivo de esa misma identidad.
                let verified = SignedRecord::from_bytes(&record)?.verify()?;
                let sender = device_pk_of(&peer).context("peer sin clave Ed25519")?;
                if !verified.record.has_device(&sender) {
                    bail!("la vinculación no viene de esa identidad");
                }
                if !take_token(&self.link_secret, &secret) {
                    bail!("el código de vinculación no vale o ha caducado");
                }
                self.accept_grant(verified)?;
                Ok(Response::Ack)
            }
            Request::Join { token, label } => {
                if !take_token(&self.invite, &token) {
                    bail!("la invitación no vale o ha caducado");
                }
                let device = device_pk_of(&peer).context("peer sin clave Ed25519")?;
                let label = clean_label(&label);
                let verified = self.edit_devices(|rec| Ok(rec.add_device(device, &label)?)).await?;
                self.emit(Event::DeviceLinked { label });
                let app = self.clone();
                self.rt.spawn(async move {
                    // Que procese antes el registro de la respuesta.
                    tokio::time::sleep(Duration::from_secs(3)).await;
                    app.share_with_own(peer).await;
                    app.after_devices_changed().await;
                });
                Ok(Response::Granted { record: verified.signed.to_bytes()? })
            }
            Request::Welcome { welcome, meta } => {
                if peer != peer_id_of(&meta.committer)? {
                    bail!("el Welcome no viene del committer declarado");
                }
                // Si ya estuvimos en el grupo (nos quitaron y nos vuelven a
                // añadir), solo su mismo committer puede sustituirlo.
                let (group_id, old) = {
                    let st = self.state.lock().unwrap();
                    let mls = st.mls.as_ref().context("sin identidad")?;
                    // Que nadie pueda llenarnos de grupos (cada uno es estado
                    // MLS en disco) a base de invitaciones (D-042).
                    let pending = st.chats.iter().filter(|c| c.active && !c.accepted && !c.left).count();
                    if pending >= MAX_REQUESTS {
                        bail!("demasiadas solicitudes pendientes");
                    }
                    let group_id = mls.join(&welcome, |gid| {
                        st.chats.iter().any(|c| c.group_id == gid && c.meta.committer == meta.committer)
                    })?;
                    let old = st.chats.iter().find(|c| c.group_id == group_id).cloned();
                    (group_id, old)
                };
                let committer = self.with_mls(|m| Ok(m.members(&group_id)?))?
                    .into_iter()
                    .find(|m| m.device_pk == meta.committer)
                    .context("el committer no es miembro del grupo")?;
                let invited_by = participant_of(&meta.participants, &committer.root_pk);
                // La raíz que declara su credencial solo vale si el registro
                // firmado de esa raíz incluye al committer (D-040). Si no, no
                // puede saltarse la solicitud haciéndose pasar por nosotros o
                // por un contacto de confianza.
                let verified = self.device_verified(&committer, &invited_by).await;
                if !verified {
                    warn!("Welcome de un committer que no está en el registro de su identidad");
                }
                let index = {
                    let mut st = self.state.lock().unwrap();
                    // Solicitud (D-029) salvo que venga de nosotros o de
                    // alguien con quien ya hablamos.
                    let accepted = st.pre_accepted.remove(&group_id)
                        || verified && old.as_ref().map_or_else(
                            || self.is_mine_in(&st, &committer.root_pk) || self.trusted(&st, &suffix_of(&invited_by)),
                            // Si salimos y nos vuelven a añadir, es otra
                            // vez una solicitud (D-031).
                            |o| o.accepted && !o.left,
                        );
                    // Tras salir, una invitación nueva vuelve a mostrarlo.
                    let hidden = old.as_ref().is_some_and(|o| o.hidden && !o.left);
                    let chat = Chat {
                        group_id: group_id.clone(),
                        meta: meta.clone(),
                        active: true,
                        hidden,
                        accepted,
                        invited_by: Some(invited_by.clone()),
                        left: false,
                    };
                    let index = match st.chats.iter().position(|c| c.group_id == group_id) {
                        Some(i) => {
                            st.chats[i] = chat;
                            i
                        }
                        None => {
                            st.chats.push(chat);
                            st.chats.len() - 1
                        }
                    };
                    self.save_chats(&st)?;
                    index
                };
                if let Some(old) = old {
                    let by = participant_of(&meta.participants, &committer.root_pk);
                    if let Some(text) = meta_notice(&old.meta, &meta, &by, false) {
                        let mine = self.is_mine(&committer.root_pk);
                        let message = StoredMessage { from: by, text, sent_at: now(), mine, notice: true, sticker: None, file: None };
                        self.record(index, &group_id, message, false);
                    }
                }
                let name = chat_name(&meta, self.handle().map(|h| h.to_string()).as_deref());
                // Antes de confirmar: así los registros de los participantes
                // están resueltos cuando lleguen sus mensajes.
                self.verify_members(&group_id).await;
                // Un 1:1 duplicado se une al que ya había: no es un chat nuevo.
                // Tampoco se avisa de invitaciones de alguien bloqueado.
                let notify = {
                    let st = self.state.lock().unwrap();
                    self.visible(&st, index).then(|| !st.chats[index].accepted)
                };
                if let Some(request) = notify {
                    self.emit(Event::NewChat { index, name, request });
                }
                Ok(Response::Ack)
            }
            Request::Mls(bytes) => {
                // Tras un commit pueden haber entrado dispositivos nuevos:
                // refrescar sus registros y avisar si alguno no está autorizado.
                let applied = self.process_mls(bytes, 0);
                self.serve_leaves();
                if let Some(group_id) = applied? {
                    let this = self.clone();
                    tokio::spawn(async move { this.verify_members(&group_id).await });
                }
                self.fetch_avatars();
                Ok(Response::Ack)
            }
            Request::Deposit(env) => {
                // Cupo por peer (los depósitos anónimos llegan con identidad
                // efímera: el límite de verdad es el de bytes del buzón).
                if !self.deposit_limit.lock().unwrap().take(peer) {
                    bail!("demasiados depósitos");
                }
                self.mailbox.lock().unwrap().deposit(env)?;
                Ok(Response::Ack)
            }
            Request::Ping => Ok(Response::Ack),
            Request::Accepted { groups } => {
                let device = device_pk_of(&peer).context("peer sin clave Ed25519")?;
                let mut st = self.state.lock().unwrap();
                if !st.identity.as_ref().is_some_and(|v| v.record.has_device(&device)) {
                    bail!("solo de un dispositivo nuestro");
                }
                let mut changed = false;
                for g in groups {
                    match st.chats.iter().position(|c| c.group_id == g) {
                        Some(i) => {
                            for j in merged_indexes(&st.chats, i) {
                                changed |= !st.chats[j].accepted;
                                st.chats[j].accepted = true;
                            }
                        }
                        None => {
                            st.pre_accepted.insert(g);
                        }
                    }
                }
                if changed {
                    self.save_chats(&st)?;
                    drop(st);
                    self.emit(Event::ChatsChanged);
                    self.fetch_avatars();
                }
                Ok(Response::Ack)
            }
            Request::Blocks { list } => {
                let device = device_pk_of(&peer).context("peer sin clave Ed25519")?;
                let changed = {
                    let mut st = self.state.lock().unwrap();
                    if !st.identity.as_ref().is_some_and(|v| v.record.has_device(&device)) {
                        bail!("solo de un dispositivo nuestro");
                    }
                    st.blocked.merge(list)?
                };
                if changed {
                    self.emit(Event::ChatsChanged);
                    self.fetch_avatars();
                }
                Ok(Response::Ack)
            }
            Request::Avatar => {
                // Solo a dispositivos con los que compartimos un chat y cuya
                // persona no está bloqueada.
                let device = device_pk_of(&peer).context("peer sin clave Ed25519")?;
                let allowed = {
                    let st = self.state.lock().unwrap();
                    let mls = st.mls.as_ref().context("sin identidad")?;
                    st.chats
                        .iter()
                        .filter(|c| c.active && c.accepted)
                        .filter_map(|c| mls.members(&c.group_id).ok())
                        .flatten()
                        .find(|m| m.device_pk == device)
                        .and_then(|m| suffix_of_root(&m.root_pk))
                        .is_some_and(|s| !st.blocked.contains(&s))
                };
                if !allowed {
                    bail!("sin chat en común");
                }
                let me = self.my_suffix().context("sin identidad")?;
                let (version, image) = self.avatars.lock().unwrap().read(&me)?;
                Ok(Response::Avatar { version, image })
            }
            Request::Fetch => {
                let me = device_pk_of(&peer).context("peer sin clave Ed25519")?;
                let (list, more) = self.mailbox.lock().unwrap().fetch(&me);
                Ok(Response::Envelopes { list, more })
            }
            Request::Deliver { list } => {
                if self.state.lock().unwrap().mls.is_none() {
                    return Ok(Response::Taken { ids: Vec::new() });
                }
                let me = self.device_pk();
                let n = self.deliver_budget(peer, list.len());
                let mut ids = Vec::new();
                for env in list.into_iter().take(n) {
                    if env.to != me {
                        continue;
                    }
                    let id = env.id();
                    // Uno que no se puede abrir tampoco hay que guardarlo.
                    if let Err(e) = self.open_envelope(env).await {
                        debug!(%peer, "sobre entregado descartado: {e:#}");
                    }
                    ids.push(id);
                }
                Ok(Response::Taken { ids })
            }
            Request::AckEnvelopes { ids } => {
                let me = device_pk_of(&peer).context("peer sin clave Ed25519")?;
                self.mailbox.lock().unwrap().ack(&me, &ids)?;
                Ok(Response::Ack)
            }
            Request::Chunk { file, index, token } => {
                let stored = self.files.lock().unwrap().get(&file)?;
                // Solo a dispositivos que siguen en el grupo del archivo.
                let device = device_pk_of(&peer).context("peer sin clave Ed25519")?;
                let members = self.with_mls(|m| Ok(m.members(&stored.group_id)?))?;
                if !members.iter().any(|m| m.device_pk == device) {
                    bail!("archivo desconocido");
                }
                Ok(Response::Chunk(self.files.lock().unwrap().serve(&file, index, &token)?))
            }
            Request::SyncHint { group_id } => {
                // Solo de un dispositivo del grupo o de una persona del grupo
                // (uno recién vinculado aún no es miembro), y como mucho una
                // sincronización por aviso cada `HINT_EVERY` (D-042).
                let device = device_pk_of(&peer).context("peer sin clave Ed25519")?;
                let members = self.with_mls(|m| Ok(m.members(&group_id)?))?;
                let allowed = members.iter().any(|m| m.device_pk == device) || {
                    let st = self.state.lock().unwrap();
                    members.iter().any(|m| {
                        Self::device_known(&st, &MemberId { root_pk: m.root_pk, device_pk: device }) == Some(true)
                    })
                };
                if !allowed {
                    bail!("aviso de sincronización de alguien ajeno al grupo");
                }
                {
                    let t = now();
                    let mut last = self.hinted.lock().unwrap();
                    if last.get(&group_id).is_some_and(|at| at + HINT_EVERY > t) {
                        return Ok(Response::Ack);
                    }
                    last.retain(|_, at| *at + HINT_EVERY > t);
                    last.insert(group_id.clone(), t);
                }
                let this = self.clone();
                tokio::spawn(async move {
                    if let Err(e) = this.sync_group(&group_id).await {
                        debug!("sync por aviso fallida: {e:#}");
                    }
                });
                Ok(Response::Ack)
            }
        }
    }

    /// Procesa un mensaje MLS. Si falla (p. ej. llega antes que el commit
    /// que lo hace válido), se guarda para reintentarlo tras el siguiente.
    /// Devuelve el grupo si se aplicó un commit.
    fn process_mls(&self, bytes: Vec<u8>, attempts: u8) -> Result<Option<Vec<u8>>> {
        let mut st = self.state.lock().unwrap();
        let mls = st.mls.as_ref().context("sin identidad")?;
        let group_id = lacre_core::mls::group_id_of(&bytes)?;
        let chat = st.chats.iter().find(|c| c.group_id == group_id).cloned();
        let Some(chat) = chat else {
            // Puede llegar antes que el Welcome.
            Self::defer(&mut st, &group_id, false, bytes, attempts);
            return Ok(None);
        };
        match mls.process(&bytes, &chat.meta.committer) {
            Ok(Incoming::Application { sender, body, .. }) => {
                let msg: AppMessage = cbor_from(&body)?;
                let index = st.chats.iter().position(|c| c.group_id == group_id).unwrap();
                // La raíz que declara la credencial solo cuenta si su registro
                // firmado incluye el dispositivo (D-040): una hoja que diga
                // ser nuestra sin estar en nuestro registro no es nuestra.
                let forged = Self::device_known(&st, &sender) == Some(false);
                let mine = !forged && st.identity.as_ref().is_some_and(|v| v.record.root_pk == sender.root_pk);
                // Ya salimos: nada de lo que llegue lo vuelve a mostrar.
                if chat.left {
                    return Ok(None);
                }
                if msg.leave && msg.meta.is_none() {
                    if mine {
                        // Salimos desde otro dispositivo nuestro.
                        st.chats[index].left = true;
                        self.save_chats(&st)?;
                        drop(st);
                        let message = StoredMessage { from: String::new(), text: "Saliste del chat desde otro dispositivo".into(), sent_at: msg.sent_at, mine: true, notice: true, sticker: None, file: None };
                        self.record(index, &group_id, message, false);
                    } else if chat.meta.committer == self.device_pk() && !forged {
                        st.leaves.push((group_id, sender.root_pk));
                    }
                    return Ok(None);
                }
                if let Some(meta) = msg.meta {
                    // Cambio de participantes: solo vale del committer.
                    if sender.device_pk != chat.meta.committer || meta.committer != chat.meta.committer {
                        bail!("cambio de participantes que no viene del committer");
                    }
                    st.chats[index].meta = meta.clone();
                    // Nuestra persona salió (desde otro dispositivo).
                    let me = st.identity.as_ref().map(|v| v.handle.to_string()).unwrap_or_default();
                    if msg.leave && chat.meta.participants.contains(&me) && !meta.participants.contains(&me) {
                        st.chats[index].left = true;
                    }
                    self.save_chats(&st)?;
                    drop(st);
                    let by = participant_of(&meta.participants, &sender.root_pk);
                    if let Some(text) = meta_notice(&chat.meta, &meta, &by, msg.leave) {
                        let message = StoredMessage { from: by, text, sent_at: msg.sent_at, mine, notice: true, sticker: None, file: None };
                        self.record(index, &group_id, message, false);
                    }
                    return Ok(None);
                }
                let sender_suffix = suffix_of_root(&sender.root_pk).unwrap_or_default();
                if !mine && st.blocked.contains(&sender_suffix) {
                    return Ok(None);
                }
                let from = self.display_name(&st, &sender, &chat.meta.participants);
                drop(st);
                // La foto se pide después (`fetch_avatars`), si la persona
                // comparte con nosotros algún chat aceptado.
                if msg.avatar > 0 && !forged {
                    self.avatars.lock().unwrap().saw(&sender_suffix, msg.avatar, sender.device_pk)?;
                }
                if msg.text.is_empty() && msg.sticker.is_none() && msg.file.is_none() {
                    return Ok(None);
                }
                let mut msg = msg;
                // Texto acotado (el sobre ya limita el tamaño, pero cada
                // mensaje reescribe el historial entero).
                if msg.text.chars().count() > MAX_TEXT {
                    msg.text = msg.text.chars().take(MAX_TEXT).collect();
                }
                // De una solicitud aún sin aceptar (D-029) solo se guardan
                // unos pocos mensajes, y sin stickers (hasta 200 KB cada uno).
                // Las ofertas de archivo sí (pocos KB): el contenido no se
                // puede descargar hasta aceptar (D-042).
                if !chat.accepted {
                    if self.history.lock().unwrap().count(&group_id) >= MAX_REQUEST_MESSAGES {
                        return Ok(None);
                    }
                    if msg.sticker.take().is_some() {
                        msg.text = "[sticker no guardado: acepta el chat para recibirlos]".into();
                    }
                }
                let sticker = match msg.sticker {
                    Some(b) => Some(self.stickers.lock().unwrap().put(&b).context("sticker recibido")?),
                    None => None,
                };
                let file = match msg.file {
                    Some(offer) => Some(self.register_file(&offer, &group_id, sender.device_pk).context("archivo recibido")?),
                    None => None,
                };
                let message = StoredMessage { from, text: msg.text, sent_at: msg.sent_at, mine, notice: false, sticker, file };
                self.record(index, &group_id, message, false);
                Ok(None)
            }
            Ok(Incoming::Commit { removed_self, .. }) => {
                if removed_self {
                    let index = st.chats.iter().position(|c| c.group_id == group_id).unwrap();
                    st.chats[index].active = false;
                    self.save_chats(&st)?;
                    // Si salimos nosotros no es una expulsión.
                    if !chat.left {
                        self.emit(Event::Removed { index });
                    }
                }
                drop(st);
                self.retry_pending();
                Ok(Some(group_id))
            }
            Ok(Incoming::Ignored) => Ok(None),
            Err(e) => {
                Self::defer(&mut st, &group_id, true, bytes, attempts);
                Err(e.into())
            }
        }
    }

    /// Comprueba que cada dispositivo de un grupo al que nos han añadido está
    /// autorizado por el registro de su persona.
    async fn verify_members(&self, group_id: &[u8]) {
        let Ok(chat) = self.chat(group_id) else { return };
        let members = match self.with_mls(|m| Ok(m.members(group_id)?)) {
            Ok(m) => m,
            Err(e) => {
                warn!("miembros del grupo ilegibles: {e:#}");
                return;
            }
        };
        let mut authorized = HashSet::new();
        for p in &chat.meta.participants {
            if let Ok(h) = p.parse::<Handle>()
                && let Ok(rec) = self.resolve(&h).await
            {
                for d in &rec.record.devices {
                    authorized.insert(MemberId { root_pk: rec.record.root_pk, device_pk: d.device_pk });
                }
            }
        }
        for m in members.iter().filter(|m| !authorized.contains(m)) {
            let st = self.state.lock().unwrap();
            let Some(index) = st.chats.iter().position(|c| c.group_id == group_id) else { return };
            let name = self.display_name(&st, m, &chat.meta.participants);
            drop(st);
            self.emit(Event::UnverifiedMember { index, name });
        }
    }

    /// Tarea periódica: republicar el registro y el KeyPackage, vaciar
    /// nuestros buzones y sincronizar los grupos que controlamos.
    /// Mantiene abiertas las conexiones con los dispositivos de nuestros
    /// chats (D-028): un ping a cada uno antes de que la conexión se cierre
    /// por inactividad, para que un mensaje no tenga que esperar a montar
    /// otra vez el circuito Tor. Al arrancar sirve de preconexión. A quien
    /// no responde se le vuelve a probar cada vez más espaciado.
    pub fn keepalive(self: &Arc<Self>) {
        let me = self.device_pk();
        let devices: HashSet<[u8; 32]> = {
            let st = self.state.lock().unwrap();
            st.chats
                .iter()
                .filter(|c| c.active && c.accepted && !c.hidden)
                .filter_map(|c| st.mls.as_ref()?.members(&c.group_id).ok())
                .flatten()
                .map(|m| m.device_pk)
                .filter(|d| *d != me)
                .collect()
        };
        let now = Instant::now();
        let due: Vec<[u8; 32]> = {
            let mut pings = self.pings.lock().unwrap();
            pings.retain(|d, _| devices.contains(d));
            devices
                .into_iter()
                .filter(|d| {
                    let p = pings.entry(*d).or_default();
                    let due = !p.busy && p.next.is_none_or(|t| t <= now);
                    p.busy |= due;
                    due
                })
                .collect()
        };
        for device in due {
            let app = self.clone();
            tokio::spawn(async move {
                let start = Instant::now();
                let result = async {
                    match app.net.request(peer_id_of(&device)?, Request::Ping).await? {
                        Response::Ack => Ok(()),
                        other => Err(anyhow!("{other:?}")),
                    }
                }
                .await;
                let ms = start.elapsed().as_millis();
                let mut pings = app.pings.lock().unwrap();
                let Some(p) = pings.get_mut(&device) else { return };
                p.busy = false;
                match result {
                    Ok(()) => {
                        debug!(peer = ?peer_id_of(&device).ok(), ms, "ping");
                        (p.next, p.wait) = (None, Duration::ZERO);
                    }
                    Err(e) => {
                        p.wait = (p.wait * 2).clamp(KEEPALIVE_RETRY_MIN, KEEPALIVE_RETRY_MAX);
                        p.next = Some(Instant::now() + p.wait);
                        debug!(peer = ?peer_id_of(&device).ok(), ms, "ping sin respuesta ({e:#}); otra vez en {:?}", p.wait);
                    }
                }
            });
        }
    }

    pub async fn tick(self: &Arc<Self>) {
        self.publish().await;
        self.fetch_all_mailboxes().await;
        self.push_mailbox();
        let me = self.device_pk();
        let groups: Vec<Vec<u8>> = {
            let st = self.state.lock().unwrap();
            st.chats.iter().filter(|c| c.active && c.meta.committer == me).map(|c| c.group_id.clone()).collect()
        };
        for g in groups {
            if let Err(e) = self.sync_group(&g).await {
                debug!("sync periódica: {e:#}");
            }
        }
        self.retry_pending();
        self.fetch_avatars();
    }

    /// Guarda un mensaje para reintentarlo. Con cupo por grupo y menos sitio
    /// para grupos desconocidos (D-042): cualquiera puede mandarnos mensajes
    /// de grupos inventados y no deben desplazar a los de nuestros chats.
    fn defer(st: &mut State, group_id: &[u8], known: bool, bytes: Vec<u8>, attempts: u8) {
        if attempts + 1 >= MAX_ATTEMPTS || st.pending.len() >= MAX_PENDING {
            return;
        }
        let ids: Vec<Vec<u8>> =
            st.pending.iter().filter_map(|(b, _)| lacre_core::mls::group_id_of(b).ok()).collect();
        let same = ids.iter().filter(|g| g.as_slice() == group_id).count();
        if same >= MAX_PENDING_PER_GROUP {
            return;
        }
        if !known {
            let unknown = ids.iter().filter(|g| !st.chats.iter().any(|c| &c.group_id == *g)).count();
            if unknown >= MAX_PENDING_UNKNOWN {
                return;
            }
        }
        st.pending.push((bytes, attempts + 1));
    }

    fn retry_pending(&self) {
        let pending = std::mem::take(&mut self.state.lock().unwrap().pending);
        for (bytes, attempts) in pending {
            let _ = self.process_mls(bytes, attempts);
        }
    }
}

/// Nombre de dispositivo sin espacios sobrantes y de longitud acotada.
fn clean_label(label: &str) -> String {
    label.trim().chars().take(MAX_LABEL).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn link_code_requires_secret() {
        let key = encode_key(&generate_key().verifying_key());
        assert!(matches!(parse_link_code(&key), Err(LinkError::BadCode)));
        let secret = data_encoding::BASE32_NOPAD.encode(&[7u8; 16]).to_lowercase();
        let (_, s, addrs) = parse_link_code(&format!("{key}:{secret}@/ip4/1.2.3.4/tcp/1")).unwrap();
        assert_eq!(s, vec![7u8; 16]);
        assert_eq!(addrs.len(), 1);
        let short = data_encoding::BASE32_NOPAD.encode(&[7u8; 8]).to_lowercase();
        assert!(parse_link_code(&format!("{key}:{short}")).is_err());
    }

    #[test]
    fn tokens_are_single_use() {
        let slot = Mutex::new(None);
        let t = current_token(&slot);
        assert_eq!(t, current_token(&slot), "se reutiliza mientras está vigente");
        assert!(!take_token(&slot, &[0u8; 16]));
        assert!(take_token(&slot, &t));
        assert!(!take_token(&slot, &t), "un solo uso");
        let t2 = current_token(&slot);
        slot.lock().unwrap().as_mut().unwrap().until = Instant::now() - Duration::from_secs(1);
        assert!(!take_token(&slot, &t2), "caducado");
    }
}

#[cfg(test)]
mod seen_tests {
    use super::*;

    #[test]
    fn seen_rejects_replays_and_forgets_expired() {
        let t = 1_800_000_000;
        let mut s = Seen::default();
        assert!(s.insert([1; 32], t, t));
        assert!(!s.insert([1; 32], t, t));
        // Al caducar se olvida sin subir el suelo.
        let later = t + envelope::MAX_AGE + envelope::MAX_SKEW + 1;
        assert!(s.insert([2; 32], later, later));
        assert!(!s.contains(&[1; 32]));
        assert_eq!(s.floor, 0);
        // Bajo inundación se olvida el más antiguo y el suelo sube.
        for i in 0..MAX_SEEN as u64 {
            let mut id = [0u8; 32];
            id[..8].copy_from_slice(&i.to_le_bytes());
            id[31] = 9;
            assert!(s.insert(id, later + 1 + i, later));
        }
        assert_eq!(s.ids.len(), MAX_SEEN);
        assert!(!s.contains(&[2; 32]));
        assert!(!s.insert([2; 32], later, later));
    }

    #[test]
    fn seen_reads_old_format() {
        let old: VecDeque<[u8; 32]> = VecDeque::from([[7; 32]]);
        let s = Seen::from_bytes(&cbor_to(&old).unwrap()).unwrap();
        assert!(s.contains(&[7; 32]));
    }
}
