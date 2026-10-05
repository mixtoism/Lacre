//! Lógica de la aplicación: identidad local, vinculación de dispositivos,
//! chats (1:1 y grupos), sincronización de dispositivos en los grupos y
//! entrega diferida mediante buzones (D-016).
//!
//! Regla de concurrencia: el estado (`State`) se protege con un mutex
//! síncrono que nunca se mantiene durante una espera de red. Los envíos de
//! este dispositivo se serializan con `send_lock` para que los commits
//! lleguen antes que los mensajes de la época siguiente.

use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow, bail};
use ed25519_dalek::{SigningKey, VerifyingKey};
use futures::future::join_all;
use libp2p::multiaddr::Protocol;
use libp2p::{Multiaddr, PeerId};
use p2m_core::identity::{Handle, decode_key, encode_key, generate_key};
use p2m_core::envelope::Envelope;
use p2m_core::mls::{Incoming, KeyPackage, MemberId, MlsEngine, verify_key_package};
use p2m_core::record::{
    IdentityRecord, SignedRecord, VerifiedRecord, best_record, dht_key, key_package_key, mailbox_key,
};
use p2m_core::{cbor_from, cbor_to};
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

use crate::history::{History, StoredMessage};
use crate::mailbox::Mailbox;
use crate::net::{Client, Inbound, device_pk_of, peer_id_of};
use crate::peers::is_dialable_from_anywhere;
use crate::proto::{ChatMeta, Request, Response};

/// Mensajes MLS que no se pudieron procesar (p. ej. de una época futura) y
/// se reintentan tras el siguiente commit.
const MAX_PENDING: usize = 128;
const MAX_ATTEMPTS: u8 = 5;
/// Copias remotas de cada sobre para un dispositivo desconectado (además
/// de la que guarda el propio remitente).
const MAILBOX_REPLICAS: usize = 3;
/// Ids de sobres ya procesados que se recuerdan para descartar duplicados.
const MAX_SEEN: usize = 5000;
/// Antigüedad a partir de la cual se renueva el KeyPackage de último recurso.
const LAST_RESORT_MAX_AGE: u64 = 7 * 24 * 3600;

/// Avisos para la interfaz (la CLI los imprime; la app los muestra).
#[derive(Clone, Debug)]
pub enum Event {
    /// Este dispositivo ha sido vinculado a una identidad.
    Linked { handle: String },
    /// Nos han añadido a un chat.
    NewChat { index: usize, name: String },
    /// `local`: enviado desde este mismo dispositivo.
    Message { index: usize, message: StoredMessage, local: bool },
    /// Este dispositivo ha sido expulsado de un chat.
    Removed { index: usize },
    /// Hay en el chat un dispositivo que su persona no ha autorizado.
    UnverifiedMember { index: usize, name: String },
    /// Somos alcanzables a través de un relay en esta dirección.
    RelayReady(String),
}

/// Resumen de un chat para listarlo.
#[derive(Clone, Debug)]
pub struct ChatSummary {
    pub index: usize,
    pub name: String,
    pub devices: usize,
    pub active: bool,
    pub last: Option<StoredMessage>,
}

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
}

#[derive(Serialize, Deserialize)]
struct AppMessage {
    text: String,
    sent_at: u64,
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
    seen: VecDeque<[u8; 32]>,
}

pub struct App {
    dir: PathBuf,
    device: SigningKey,
    net: Client,
    state: Mutex<State>,
    mailbox: Mutex<Mailbox>,
    history: Mutex<History>,
    send_lock: tokio::sync::Mutex<()>,
    events: tokio::sync::mpsc::UnboundedSender<Event>,
}

/// Resultado de enviar a varios dispositivos.
#[derive(Default)]
struct Delivery {
    direct: usize,
    /// Desconectados cuyo mensaje quedó en buzones.
    stored: usize,
    failed: usize,
}

pub(crate) fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

fn read_key(path: &Path) -> Result<Option<SigningKey>> {
    if !path.exists() {
        return Ok(None);
    }
    let bytes: [u8; 32] = fs::read(path)?
        .try_into()
        .map_err(|_| anyhow!("clave corrupta en {}", path.display()))?;
    Ok(Some(SigningKey::from_bytes(&bytes)))
}

pub(crate) fn write_private(path: &Path, bytes: &[u8]) -> Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    use std::io::Write;
    let tmp = path.with_extension("tmp");
    fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&tmp)?
        .write_all(bytes)?;
    fs::rename(tmp, path)?;
    Ok(())
}

/// Carga (o crea) la clave de este dispositivo.
pub fn load_device_key(dir: &Path) -> Result<SigningKey> {
    fs::create_dir_all(dir)?;
    let path = dir.join("device.key");
    if let Some(k) = read_key(&path)? {
        return Ok(k);
    }
    let k = generate_key();
    write_private(&path, &k.to_bytes())?;
    Ok(k)
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

/// Cuánto se espera a conectar con las direcciones de un QR.
const QR_DIAL_TIMEOUT: Duration = Duration::from_secs(60);

impl App {
    pub fn open(
        dir: PathBuf,
        device: SigningKey,
        net: Client,
        events: tokio::sync::mpsc::UnboundedSender<Event>,
    ) -> Result<Arc<Self>> {
        let root = read_key(&dir.join("root.key"))?;
        let identity = match fs::read(dir.join("identity.cbor")) {
            Ok(b) => Some(SignedRecord::from_bytes(&b)?.verify()?),
            Err(_) => None,
        };
        let mls = match &identity {
            Some(id) => Some(MlsEngine::open(&device, id.record.root_pk, Some(dir.join("mls.json")))?),
            None => None,
        };
        let chats = match fs::read(dir.join("chats.cbor")) {
            Ok(b) => cbor_from(&b)?,
            Err(_) => Vec::new(),
        };
        let seen = match fs::read(dir.join("seen.cbor")) {
            Ok(b) => cbor_from(&b)?,
            Err(_) => VecDeque::new(),
        };
        let mailbox = Mailbox::open(dir.join("mailbox.cbor"))?;
        let history = History::open(dir.join("history.cbor"))?;
        // Registros de contactos ya resueltos (firmados: se vuelven a verificar).
        let contacts = match fs::read(dir.join("contacts.cbor")) {
            Ok(b) => cbor_from::<Vec<Vec<u8>>>(&b)?
                .iter()
                .filter_map(|r| SignedRecord::from_bytes(r).ok()?.verify().ok())
                .map(|v| (v.record.root_pk, v))
                .collect(),
            Err(_) => HashMap::new(),
        };
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
            }),
            mailbox: Mutex::new(mailbox),
            history: Mutex::new(history),
            send_lock: tokio::sync::Mutex::new(()),
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
    fn record(&self, index: usize, group_id: &[u8], message: StoredMessage, local: bool) {
        if let Err(e) = self.history.lock().unwrap().add(group_id, message.clone()) {
            warn!("no se pudo guardar el historial: {e:#}");
        }
        self.emit(Event::Message { index, message, local });
    }

    /// Mensajes de un chat, del más antiguo al más reciente.
    pub fn messages(&self, index: usize) -> Result<Vec<StoredMessage>> {
        let group_id = {
            let st = self.state.lock().unwrap();
            st.chats.get(index).context("no existe ese chat")?.group_id.clone()
        };
        Ok(self.history.lock().unwrap().get(&group_id))
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
        for a in self.net.listen_addrs().await.unwrap_or_default() {
            if is_dialable_from_anywhere(&a) {
                qr.push('@');
                qr.push_str(&a.to_string());
            }
        }
        Some(qr)
    }

    // ---------------------------------------------------------------- identidad

    /// Crea una identidad nueva con este dispositivo como primario.
    pub async fn init(&self, name: &str) -> Result<Handle> {
        let verified = {
            let mut st = self.state.lock().unwrap();
            if st.identity.is_some() {
                bail!("este dispositivo ya tiene identidad");
            }
            let root = generate_key();
            let mut rec = IdentityRecord::new(&root, name)?;
            rec.add_device(self.device_pk(), "primario")?;
            let verified = rec.sign(&root)?.verify()?;
            write_private(&self.dir.join("root.key"), &root.to_bytes())?;
            st.root = Some(root);
            self.set_identity(&mut st, verified.clone())?;
            verified
        };
        self.publish().await;
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
            return Ok(fs::read(&path)?);
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

    /// Vincula un dispositivo nuevo a partir de su código.
    pub async fn link(self: &Arc<Self>, code: &str, label: &str) -> Result<()> {
        let device = decode_key(code)?.to_bytes();
        let verified = self
            .edit_devices(|rec| Ok(rec.add_device(device, label)?))
            .await?;
        let grant = Request::LinkGrant { record: verified.signed.to_bytes()? };
        match self.net.request(peer_id_of(&device)?, grant).await? {
            Response::Ack => {}
            other => bail!("el dispositivo rechazó el enlace: {other:?}"),
        }
        self.after_devices_changed().await;
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
        let mut candidates = self.net.get_records(dht_key(&handle.suffix)).await?;
        {
            let st = self.state.lock().unwrap();
            if let Some(own) = st.identity.as_ref().filter(|v| &v.handle == handle) {
                candidates.push(own.signed.to_bytes()?);
            }
            if let Some(c) = st.contacts.values().find(|v| &v.handle == handle) {
                candidates.push(c.signed.to_bytes()?);
            }
        }
        let best = best_record(handle, candidates.iter().map(Vec::as_slice))
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
                let suffix = p2m_core::identity::handle_suffix(&root);
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
        let mut participants = vec![me.to_string()];
        for t in &targets {
            self.dial_and_wait(&t.addrs).await;
            let rec = self.resolve(&t.handle).await?;
            if let Some(root) = t.root
                && root.to_bytes() != rec.record.root_pk
            {
                bail!("la clave del QR no coincide con la de {}", t.handle);
            }
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
            });
            self.save_chats(&st)?;
            group_id
        };
        self.sync_group(&group_id).await?;
        let st = self.state.lock().unwrap();
        Ok(st.chats.iter().position(|c| c.group_id == group_id).unwrap())
    }

    pub fn chats(&self) -> Vec<ChatSummary> {
        let st = self.state.lock().unwrap();
        let history = self.history.lock().unwrap();
        let me = st.identity.as_ref().map(|v| v.handle.to_string());
        st.chats
            .iter()
            .enumerate()
            .map(|(index, c)| {
                let devices = st.mls.as_ref().and_then(|m| m.members(&c.group_id).ok()).map_or(0, |m| m.len());
                let name = chat_name(&c.meta, me.as_deref());
                ChatSummary { index, name, devices, active: c.active, last: history.last(&c.group_id).cloned() }
            })
            .collect()
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
        // de red).
        let mut desired: HashSet<MemberId> = HashSet::new();
        let mut known_roots: HashSet<[u8; 32]> = HashSet::new();
        for p in &chat.meta.participants {
            let Ok(rec) = self.resolve(&p.parse()?).await else {
                warn!("no se pudo resolver {p}; se mantiene tal cual");
                continue;
            };
            known_roots.insert(rec.record.root_pk);
            for d in &rec.record.devices {
                desired.insert(MemberId { root_pk: rec.record.root_pk, device_pk: d.device_pk });
            }
        }
        let current: HashSet<MemberId> = self.with_mls(|m| Ok(m.members(group_id)?))?.into_iter().collect();
        let to_remove: Vec<MemberId> = current
            .iter()
            .filter(|m| known_roots.contains(&m.root_pk) && !desired.contains(m))
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
        let chat = {
            let st = self.state.lock().unwrap();
            st.chats.get(index).cloned().context("no existe ese chat")?
        };
        if !chat.active {
            bail!("ya no perteneces a este chat");
        }
        // El committer comprueba antes si hay dispositivos nuevos o revocados.
        if chat.meta.committer == self.device_pk()
            && let Err(e) = self.sync_group(&chat.group_id).await
        {
            warn!("sincronización previa fallida: {e:#}");
        }
        let _guard = self.send_lock.lock().await;
        let sent_at = now();
        let body = cbor_to(&AppMessage { text: text.to_owned(), sent_at })?;
        let (msg, members) = self.with_mls(|m| {
            Ok((m.encrypt(&chat.group_id, &body)?, m.members(&chat.group_id)?))
        })?;
        let from = {
            let st = self.state.lock().unwrap();
            let root_pk = st.identity.as_ref().context("sin identidad")?.record.root_pk;
            self.display_name(&st, &MemberId { root_pk, device_pk: self.device_pk() }, &chat.meta.participants)
        };
        self.record(index, &chat.group_id, StoredMessage { from, text: text.to_owned(), sent_at, mine: true }, true);
        let d = self.fan_out(&members, Request::Mls(msg)).await;
        Ok(SendReport { stored: d.stored, failed: d.failed })
    }

    /// Envía a todos los dispositivos indicados salvo a este. A los que no
    /// responden se les deja el mensaje en buzones.
    async fn fan_out(&self, members: &[MemberId], req: Request) -> Delivery {
        let me = self.device_pk();
        let sends = members.iter().filter(|m| m.device_pk != me).map(|m| {
            let req = req.clone();
            async move {
                let direct = async {
                    match self.net.request(peer_id_of(&m.device_pk)?, req.clone()).await? {
                        Response::Ack => Ok(()),
                        other => Err(anyhow!("{other:?}")),
                    }
                };
                match direct.await {
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
    /// los de los nodos más cercanos a su clave de buzón. Devuelve cuántas
    /// copias remotas se guardaron.
    async fn deposit(&self, to: &[u8; 32], req: &Request) -> Result<usize> {
        let env = Envelope::seal(&self.device, to, &cbor_to(req)?)?;
        self.mailbox.lock().unwrap().deposit(env.clone())?;
        let recipient = peer_id_of(to)?;
        let peers = self.net.closest_peers(mailbox_key(to)).await.unwrap_or_default();
        let stores = peers
            .into_iter()
            .filter(|p| *p != recipient)
            .take(MAILBOX_REPLICAS)
            .map(|p| {
                let env = env.clone();
                async move { matches!(self.net.request(p, Request::Deposit(env)).await, Ok(Response::Ack)) }
            });
        let copies = join_all(stores).await.into_iter().filter(|ok| *ok).count();
        debug!(copies, "sobre depositado");
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

    /// Procesa un sobre de un buzón como si la petición llegara directamente
    /// de su remitente.
    async fn open_envelope(self: &Arc<Self>, env: Envelope) -> Result<()> {
        let id = env.id();
        {
            let mut st = self.state.lock().unwrap();
            if st.seen.contains(&id) {
                return Ok(());
            }
            st.seen.push_back(id);
            if st.seen.len() > MAX_SEEN {
                st.seen.pop_front();
            }
            write_private(&self.dir.join("seen.cbor"), &cbor_to(&st.seen)?)?;
        }
        let req: Request = cbor_from(&env.open(&self.device)?)?;
        // Solo lo que tiene sentido diferir; nada de peticiones a buzones.
        if !matches!(req, Request::Welcome { .. } | Request::Mls(_) | Request::SyncHint { .. }) {
            bail!("petición no admitida en un sobre");
        }
        Box::pin(self.on_request(peer_id_of(&env.from)?, req)).await?;
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
                Response::Error(e.to_string())
            }
        };
        self.net.respond(channel, resp).await;
    }

    async fn on_request(self: &Arc<Self>, peer: PeerId, req: Request) -> Result<Response> {
        match req {
            Request::KeyPackage => Ok(Response::KeyPackage(self.with_mls(|m| Ok(m.key_package()?))?)),
            Request::LinkGrant { record } => {
                let verified = SignedRecord::from_bytes(&record)?.verify()?;
                if !verified.record.has_device(&self.device_pk()) {
                    bail!("el registro no incluye este dispositivo");
                }
                let mut st = self.state.lock().unwrap();
                match &st.identity {
                    None => {}
                    Some(cur) if cur.record.root_pk == verified.record.root_pk => {}
                    Some(_) => bail!("este dispositivo ya pertenece a otra identidad"),
                }
                let handle = verified.handle.clone();
                self.set_identity(&mut st, verified)?;
                self.emit(Event::Linked { handle: handle.to_string() });
                Ok(Response::Ack)
            }
            Request::Welcome { welcome, meta } => {
                if peer != peer_id_of(&meta.committer)? {
                    bail!("el Welcome no viene del committer declarado");
                }
                let group_id = self.with_mls(|m| Ok(m.join(&welcome)?))?;
                let committer_is_member = self.with_mls(|m| Ok(m.members(&group_id)?))?
                    .iter()
                    .any(|m| m.device_pk == meta.committer);
                if !committer_is_member {
                    bail!("el committer no es miembro del grupo");
                }
                let index = {
                    let mut st = self.state.lock().unwrap();
                    st.chats.retain(|c| c.group_id != group_id);
                    st.chats.push(Chat { group_id: group_id.clone(), meta: meta.clone(), active: true });
                    self.save_chats(&st)?;
                    st.chats.len() - 1
                };
                let name = chat_name(&meta, self.handle().map(|h| h.to_string()).as_deref());
                // Antes de confirmar: así los registros de los participantes
                // están resueltos cuando lleguen sus mensajes.
                self.verify_members(&group_id).await;
                self.emit(Event::NewChat { index, name });
                Ok(Response::Ack)
            }
            Request::Mls(bytes) => {
                // Tras un commit pueden haber entrado dispositivos nuevos:
                // refrescar sus registros y avisar si alguno no está autorizado.
                if let Some(group_id) = self.process_mls(bytes, 0)? {
                    let this = self.clone();
                    tokio::spawn(async move { this.verify_members(&group_id).await });
                }
                Ok(Response::Ack)
            }
            Request::Deposit(env) => {
                self.mailbox.lock().unwrap().deposit(env)?;
                Ok(Response::Ack)
            }
            Request::Fetch => {
                let me = device_pk_of(&peer).context("peer sin clave Ed25519")?;
                let (list, more) = self.mailbox.lock().unwrap().fetch(&me);
                Ok(Response::Envelopes { list, more })
            }
            Request::AckEnvelopes { ids } => {
                let me = device_pk_of(&peer).context("peer sin clave Ed25519")?;
                self.mailbox.lock().unwrap().ack(&me, &ids)?;
                Ok(Response::Ack)
            }
            Request::SyncHint { group_id } => {
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
        let group_id = p2m_core::mls::group_id_of(&bytes)?;
        let chat = st.chats.iter().find(|c| c.group_id == group_id).cloned();
        let Some(chat) = chat else {
            // Puede llegar antes que el Welcome.
            Self::defer(&mut st, bytes, attempts);
            return Ok(None);
        };
        match mls.process(&bytes, &chat.meta.committer) {
            Ok(Incoming::Application { sender, body, .. }) => {
                let msg: AppMessage = cbor_from(&body)?;
                let index = st.chats.iter().position(|c| c.group_id == group_id).unwrap();
                let mine = st.identity.as_ref().is_some_and(|v| v.record.root_pk == sender.root_pk);
                let from = self.display_name(&st, &sender, &chat.meta.participants);
                drop(st);
                self.record(index, &group_id, StoredMessage { from, text: msg.text, sent_at: msg.sent_at, mine }, false);
                Ok(None)
            }
            Ok(Incoming::Commit { removed_self, .. }) => {
                if removed_self {
                    let index = st.chats.iter().position(|c| c.group_id == group_id).unwrap();
                    st.chats[index].active = false;
                    self.save_chats(&st)?;
                    self.emit(Event::Removed { index });
                }
                drop(st);
                self.retry_pending();
                Ok(Some(group_id))
            }
            Ok(Incoming::Ignored) => Ok(None),
            Err(e) => {
                Self::defer(&mut st, bytes, attempts);
                Err(e.into())
            }
        }
    }

    /// Comprueba que cada dispositivo de un grupo al que nos han añadido está
    /// autorizado por el registro de su persona.
    async fn verify_members(&self, group_id: &[u8]) {
        let Ok(chat) = self.chat(group_id) else { return };
        let Ok(members) = self.with_mls(|m| Ok(m.members(group_id)?)) else { return };
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
    pub async fn tick(self: &Arc<Self>) {
        self.publish().await;
        self.fetch_all_mailboxes().await;
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
    }

    fn defer(st: &mut State, bytes: Vec<u8>, attempts: u8) {
        if attempts + 1 < MAX_ATTEMPTS && st.pending.len() < MAX_PENDING {
            st.pending.push((bytes, attempts + 1));
        }
    }

    fn retry_pending(&self) {
        let pending = std::mem::take(&mut self.state.lock().unwrap().pending);
        for (bytes, attempts) in pending {
            let _ = self.process_mls(bytes, attempts);
        }
    }
}
