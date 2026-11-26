//! API que la app Flutter llama a través de flutter_rust_bridge (D-022).
//!
//! Hay un único nodo por proceso: `start` lo arranca en un runtime de Tokio
//! propio y el resto de funciones lo usan. Los avisos llegan por
//! `event_stream`.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex, OnceLock};

use anyhow::{Context, Result, anyhow};
use flutter_rust_bridge::frb;
use lacre_node::app::{App, Event, LinkError, SearchHit, Target};
use lacre_node::history::StoredMessage;
use lacre_node::node::{self, Options};
use tokio::sync::mpsc::UnboundedReceiver;

use crate::frb_generated::StreamSink;

pub(crate) static RT: LazyLock<tokio::runtime::Runtime> =
    LazyLock::new(|| tokio::runtime::Runtime::new().expect("runtime de Tokio"));
static NODE: OnceLock<(Arc<App>, PathBuf)> = OnceLock::new();
/// Suscriptor actual de los avisos. Puede cambiar: en Android el nodo sigue
/// vivo en el servicio aunque se cierre la interfaz (D-032).
static SINK: Mutex<Option<StreamSink<UiEvent>>> = Mutex::new(None);

#[frb(init)]
pub fn init_app() {
    // En Android stdout no llega a ningún sitio: sin suscriptor, tracing
    // emite registros de `log`, que van a logcat. El logger se instala antes
    // que el de frb (gana el primero) para filtrar: yamux y rustls saturan
    // logcat en Debug/Trace, y mDNS da un error por paquete en las
    // interfaces de datos móviles, que no admiten multicast.
    #[cfg(target_os = "android")]
    {
        let filter = android_logger::FilterBuilder::new()
            .parse("info,lacre_node=debug,libp2p_mdns=off")
            .build();
        android_logger::init_once(
            android_logger::Config::default()
                .with_max_level(log::LevelFilter::Debug)
                .with_tag("lacre")
                .with_filter(filter),
        );
        log::set_max_level(log::LevelFilter::Debug);
        flutter_rust_bridge::setup_default_user_utils();
    }
    #[cfg(not(target_os = "android"))]
    {
        flutter_rust_bridge::setup_default_user_utils();
        let _ = tracing_subscriber::fmt()
            .with_env_filter(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| "warn,lacre_node=info".into()),
            )
            .try_init();
    }
}

fn app() -> Result<Arc<App>> {
    Ok(NODE.get().context("el nodo no está arrancado")?.0.clone())
}

/// Ejecuta un futuro en el runtime del nodo (las llamadas de frb llegan
/// desde sus propios hilos).
async fn on_rt<T: Send + 'static>(f: impl Future<Output = Result<T>> + Send + 'static) -> Result<T> {
    RT.spawn(f).await.map_err(|e| anyhow!("tarea abortada: {e}"))?
}

// ---------------------------------------------------------------- tipos

pub struct Profile {
    /// `None` si el dispositivo aún no tiene identidad.
    pub handle: Option<String>,
    /// Código para vincular este dispositivo desde otro.
    pub device_code: String,
    /// Tiene la clave raíz (puede vincular y revocar dispositivos).
    pub primary: bool,
    /// El estado se guarda cifrado en disco (D-033).
    pub encrypted: bool,
}

pub struct Message {
    pub from: String,
    pub text: String,
    pub sent_at: i64,
    pub mine: bool,
    /// Aviso del chat («ana añadió a carl»), no un mensaje escrito.
    pub notice: bool,
    /// Sticker (D-025); en ese caso `text` va vacío.
    pub sticker: Option<Sticker>,
    /// Archivo adjunto (D-026); en ese caso `text` va vacío.
    pub file: Option<FileAttachment>,
}

impl From<StoredMessage> for Message {
    fn from(m: StoredMessage) -> Self {
        let sticker = m.sticker.map(|id| Sticker { id });
        let file = m.file.map(|f| {
            let downloaded = app().is_ok_and(|a| a.file_ready(&f.id));
            FileAttachment { downloaded, id: f.id, name: f.name, size: f.size }
        });
        Self { from: m.from, text: m.text, sent_at: m.sent_at as i64, mine: m.mine, notice: m.notice, sticker, file }
    }
}

/// Sticker guardado en el dispositivo. La imagen (PNG, GIF, WebP o JPEG)
/// va cifrada en disco: se pide con [`sticker_bytes`].
pub struct Sticker {
    pub id: String,
}

/// Imagen de un sticker, descifrada.
#[frb(sync)]
pub fn sticker_bytes(id: String) -> Result<Vec<u8>> {
    app()?.sticker_bytes(&id)
}

/// Archivo enviado o recibido en un chat.
pub struct FileAttachment {
    pub id: String,
    pub name: String,
    pub size: u64,
    /// Ya está entero en este dispositivo (cifrado: para abrirlo,
    /// [`open_file`]).
    pub downloaded: bool,
}

pub struct ChatMembers {
    /// Handles de las personas del chat.
    pub participants: Vec<String>,
    /// Este dispositivo puede añadir y quitar personas (creó el chat).
    pub can_edit: bool,
    /// Nuestro handle, para no ofrecer quitarnos.
    pub me: Option<String>,
}

pub struct ChatInfo {
    pub index: u32,
    pub name: String,
    pub devices: u32,
    pub active: bool,
    pub last: Option<Message>,
    /// Solicitud de chat pendiente de aceptar (D-029): no se descargan sus
    /// archivos ni se muestran sus stickers.
    pub request: bool,
    /// La otra persona, si es un chat 1:1.
    pub peer: Option<String>,
    /// Hay foto de la otra persona (solo en chats aceptados); se pide con
    /// [`avatar_bytes`].
    pub avatar: bool,
}

/// Mensaje encontrado por `search`.
pub struct Hit {
    pub index: u32,
    /// Posición del mensaje en `messages(index)`.
    pub position: u32,
    pub message: Message,
}

impl From<SearchHit> for Hit {
    fn from(h: SearchHit) -> Self {
        Self { index: h.index as u32, position: h.position as u32, message: h.message.into() }
    }
}

pub struct Device {
    pub code: String,
    pub label: String,
    pub this_device: bool,
}

pub struct Status {
    pub peers: u32,
    /// Dirección onion propia, cuando ya está publicada.
    pub onion: Option<String>,
    /// Tor activado (sin él, `onion` nunca llega).
    pub tor: bool,
    /// Última comprobación desde fuera del servicio onion (D-053): `None`
    /// mientras no se ha hecho.
    pub onion_reachable: Option<bool>,
}

pub struct SendResult {
    pub stored: u32,
    pub failed: u32,
}

pub enum UiEvent {
    /// `confirm`: código que el otro dispositivo también muestra (D-047).
    Linked { handle: String, confirm: String },
    /// Un dispositivo nuevo ha entrado con nuestra invitación (D-037).
    DeviceLinked { label: String, confirm: String },
    /// `request`: de alguien con quien aún no hablamos; hay que aceptarlo.
    NewChat { index: u32, name: String, request: bool },
    /// Ha cambiado la foto de perfil de alguien (o la nuestra).
    Avatar { handle: String },
    /// Ha cambiado la lista de chats (p. ej. otro dispositivo aceptó una
    /// solicitud).
    ChatsChanged,
    Message { index: u32, message: Message, local: bool },
    Removed { index: u32 },
    UnverifiedMember { index: u32, name: String },
    RelayReady { addr: String },
    /// Progreso de una descarga: trozos que ya están de `total`.
    FileProgress { id: String, done: u32, total: u32 },
    /// Otro dispositivo nuestro nos ha pasado `added` mensajes anteriores
    /// del chat (D-050): hay que volver a cargarlo.
    HistoryLoaded { index: u32, added: u32 },
}

impl From<Event> for UiEvent {
    fn from(ev: Event) -> Self {
        match ev {
            Event::Linked { handle, confirm } => Self::Linked { handle, confirm },
            Event::DeviceLinked { label, confirm } => Self::DeviceLinked { label, confirm },
            Event::NewChat { index, name, request } => Self::NewChat { index: index as u32, name, request },
            Event::Avatar { handle } => Self::Avatar { handle },
            Event::ChatsChanged => Self::ChatsChanged,
            Event::Message { index, message, local } => Self::Message { index: index as u32, message: message.into(), local },
            Event::Removed { index } => Self::Removed { index: index as u32 },
            Event::UnverifiedMember { index, name } => Self::UnverifiedMember { index: index as u32, name },
            Event::RelayReady(addr) => Self::RelayReady { addr },
            Event::FileProgress { id, done, total } => Self::FileProgress { id, done, total },
            Event::HistoryLoaded { index, added } => Self::HistoryLoaded { index: index as u32, added: added as u32 },
        }
    }
}

// ---------------------------------------------------------------- nodo

/// Arranca el nodo con los datos de `data_dir`. Llamarla otra vez no hace
/// nada (devuelve el perfil). Con `lan_only` no usa Tor ni busca relays en
/// Internet: solo encuentra a otros por mDNS en la red local (para pruebas).
///
/// `data_key` es la clave para cifrar el estado en disco (D-033); en Android
/// la da el Keystore. Sin ella se usa `LACRE_DATA_KEY` o, en el escritorio, el
/// llavero del sistema.
///
/// Con `accept_current_state`, acepta los archivos de estado tal como están
/// aunque parezcan una copia anterior (D-046): solo tras restaurar a
/// propósito una copia de seguridad.
pub fn start(data_dir: String, lan_only: bool, data_key: Option<Vec<u8>>, accept_current_state: bool) -> Result<Profile> {
    if NODE.get().is_none() {
        let data = PathBuf::from(&data_dir);
        std::fs::create_dir_all(&data)?;
        let mut opts = Options::new(data.clone());
        opts.accept_current_state = accept_current_state;
        if lan_only {
            opts.tor = false;
            opts.rendezvous = false;
        }
        opts.data_key = match data_key {
            Some(k) => Some(k.try_into().map_err(|_| anyhow!("la clave de datos debe medir 32 bytes"))?),
            None => node::data_key_from_env()?.map_or_else(|| desktop_key(&data), |k| Ok(Some(k)))?,
        };
        let n = {
            let _guard = RT.enter();
            node::start(opts).inspect_err(|e| {
                ROLLBACK.store(e.is::<node::StateRollback>(), std::sync::atomic::Ordering::Relaxed)
            })?
        };
        forward_events(n.events);
        let _ = NODE.set((n.app, data));
    }
    profile()
}

static ROLLBACK: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// ¿Falló el último `start` porque el estado parece una copia anterior
/// (D-046)? Entonces la app ofrece aceptarlo.
#[frb(sync)]
pub fn start_failed_by_rollback() -> bool {
    ROLLBACK.load(std::sync::atomic::Ordering::Relaxed)
}

/// Clave de datos del escritorio (D-033, D-042): Secret Service en Linux,
/// Llavero en macOS y DPAPI en Windows. Si no se puede obtener, la app no
/// arranca: nunca se guarda el estado sin cifrar (como alternativa queda
/// `LACRE_DATA_KEY`).
#[allow(unreachable_code)]
fn desktop_key(data: &Path) -> Result<Option<[u8; 32]>> {
    let create = !lacre_node::app::has_sealed_state(data);
    let found: Result<[u8; 32]> = {
        #[cfg(target_os = "linux")]
        {
            RT.block_on(crate::keystore::load_or_create(data, create))
        }
        #[cfg(target_os = "macos")]
        {
            crate::keystore_macos::load_or_create(data, create)
        }
        #[cfg(windows)]
        {
            crate::keystore_windows::load_or_create(data, create)
        }
        #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
        {
            let _ = create;
            Err(anyhow::anyhow!("plataforma sin almacén de claves"))
        }
    };
    let key = found.context(
        "no se pudo obtener la clave de datos del llavero del sistema; desbloquéalo o define LACRE_DATA_KEY",
    )?;
    Ok(Some(key))
}

/// Reenvía los avisos del nodo al suscriptor que haya en cada momento.
fn forward_events(mut rx: UnboundedReceiver<Event>) {
    RT.spawn(async move {
        while let Some(ev) = rx.recv().await {
            let mut sink = SINK.lock().unwrap();
            if sink.as_ref().is_some_and(|s| s.add(ev.into()).is_err()) {
                *sink = None;
            }
        }
    });
}

#[frb(sync)]
pub fn profile() -> Result<Profile> {
    let (app, _) = NODE.get().context("el nodo no está arrancado")?;
    Ok(Profile {
        handle: app.handle().map(|h| h.to_string()),
        device_code: app.device_code(),
        primary: app.is_primary(),
        encrypted: lacre_core::vault::enabled(),
    })
}

/// Modo de energía (D-034). Con `background` (app en segundo plano en el
/// móvil) el nodo entra en reposo: solo se le llega por su servicio onion,
/// Tor duerme y las tareas periódicas se agrupan cada ~12 min. mDNS solo
/// funciona en primer plano y con `wifi`.
#[frb(sync)]
pub fn set_power(background: bool, wifi: bool) -> Result<()> {
    app()?.net().set_power(lacre_node::net::Power { low: background, wifi });
    Ok(())
}

/// Flujo de avisos. Un suscriptor nuevo sustituye al anterior.
pub fn event_stream(sink: StreamSink<UiEvent>) -> Result<()> {
    *SINK.lock().unwrap() = Some(sink);
    Ok(())
}

pub async fn status() -> Result<Status> {
    let app = app()?;
    on_rt(async move {
        let peers = app.net().connected_peers().await?.len() as u32;
        let onion = app
            .net()
            .listen_addrs()
            .await?
            .into_iter()
            .find(|a| a.to_string().starts_with("/onion3/"))
            .map(|a| a.to_string());
        let net = app.net();
        Ok(Status { peers, onion, tor: net.has_tor(), onion_reachable: net.onion_check() })
    })
    .await
}

// ---------------------------------------------------------------- identidad

/// Crea una identidad nueva con este dispositivo como primario.
pub async fn create_identity(name: String) -> Result<String> {
    let app = app()?;
    on_rt(async move { Ok(app.init(&name).await?.to_string()) }).await
}

/// Frase de recuperación (24 palabras, D-048). Solo en el primario.
#[frb(sync)]
pub fn recovery_phrase() -> Result<String> {
    Ok(app()?.recovery_phrase()?.to_string())
}

/// Recupera la identidad con su frase: en un dispositivo sin identidad o en
/// uno vinculado que pasa a ser primario. Con `only_this`, quita los demás
/// dispositivos del registro. Devuelve el handle.
pub async fn recover_identity(phrase: String, label: String, only_this: bool) -> Result<String> {
    let app = app()?;
    on_rt(async move { Ok(app.recover(&phrase, &label, only_this).await?.to_string()) }).await
}

/// Borra la clave raíz de este dispositivo, que deja de ser primario.
#[frb(sync)]
pub fn forget_root() -> Result<()> {
    app()?.forget_root()
}

/// Contenido del QR para que otros nos añadan.
pub async fn qr_payload() -> Result<String> {
    let app = app()?;
    on_rt(async move { app.qr_payload().await.context("todavía no hay identidad") }).await
}

#[frb(sync)]
pub fn devices() -> Result<Vec<Device>> {
    Ok(app()?
        .devices()
        .into_iter()
        .map(|(code, label, this_device)| Device { code, label, this_device })
        .collect())
}

/// Cómo acabó un intento de vincular (D-037).
pub enum LinkResult {
    /// `confirm`: tiene que coincidir con el que muestra el otro (D-047).
    Done { confirm: String },
    /// No es un código de vinculación o está mal copiado.
    BadCode,
    /// No se llegó al otro dispositivo (¿app cerrada, sin conexión?).
    Unreachable,
    /// El otro dispositivo dijo que no (p. ej. invitación caducada).
    Refused { reason: String },
}

fn link_result(r: Result<String>) -> Result<LinkResult> {
    let e = match r {
        Ok(confirm) => return Ok(LinkResult::Done { confirm }),
        Err(e) => e,
    };
    match e.downcast_ref::<LinkError>() {
        Some(LinkError::BadCode) => Ok(LinkResult::BadCode),
        Some(LinkError::Unreachable) => Ok(LinkResult::Unreachable),
        Some(LinkError::Refused(reason)) => Ok(LinkResult::Refused { reason: reason.clone() }),
        None => Err(e),
    }
}

/// Código para vincular este dispositivo desde el primario, con las
/// direcciones por las que llamarlo desde otra red (cambian cuando se
/// publica el servicio onion: conviene pedirlo de nuevo cada poco).
pub async fn link_code() -> Result<String> {
    let app = app()?;
    on_rt(async move { Ok(app.link_code().await) }).await
}

/// Desde el primario: vincula el dispositivo cuyo código se ha escaneado
/// o pegado. Puede tardar un par de minutos si el otro está en otra red.
pub async fn link_device(code: String, label: String) -> Result<LinkResult> {
    let app = app()?;
    on_rt(async move { link_result(app.link(code.trim(), &label).await) }).await
}

/// Desde el primario: invitación para que la escanee el dispositivo nuevo.
/// Vale 10 minutos y una sola vez; pedirla otra vez mientras vale da el
/// mismo token con las direcciones al día.
pub async fn invite_device() -> Result<String> {
    let app = app()?;
    on_rt(async move { app.invite().await }).await
}

#[frb(sync)]
pub fn cancel_invite() -> Result<()> {
    app()?.cancel_invite();
    Ok(())
}

/// Desde el dispositivo nuevo: entra con la invitación del primario.
pub async fn join_identity(code: String, label: String) -> Result<LinkResult> {
    let app = app()?;
    on_rt(async move { link_result(app.join(code.trim(), &label).await) }).await
}

/// ¿Es una invitación (la muestra el primario) y no un código de dispositivo?
#[frb(sync)]
pub fn is_invite(code: String) -> bool {
    code.trim().starts_with(lacre_node::app::INVITE_PREFIX)
}

pub async fn unlink_device(code: String) -> Result<()> {
    let app = app()?;
    on_rt(async move { app.unlink(&code).await }).await
}

// ---------------------------------------------------------------- chats

#[frb(sync)]
pub fn chats() -> Result<Vec<ChatInfo>> {
    Ok(app()?
        .chats()
        .into_iter()
        .map(|c| ChatInfo {
            index: c.index as u32,
            name: c.name,
            devices: c.devices as u32,
            active: c.active,
            last: c.last.map(Into::into),
            request: c.request,
            peer: c.peer,
            avatar: c.avatar,
        })
        .collect())
}

#[frb(sync)]
pub fn messages(index: u32) -> Result<Vec<Message>> {
    Ok(app()?.messages(index as usize)?.into_iter().map(Into::into).collect())
}

/// Crea un chat con uno o varios destinos (handles o contenidos de QR).
/// Con más de un destino, o con título, es un grupo.
pub async fn create_chat(title: Option<String>, targets: Vec<String>) -> Result<u32> {
    let app = app()?;
    let targets = parse_targets(&targets)?;
    on_rt(async move { Ok(app.create_chat(title.filter(|t| !t.trim().is_empty()), targets).await? as u32) }).await
}

#[frb(sync)]
pub fn chat_members(index: u32) -> Result<ChatMembers> {
    let app = app()?;
    let (participants, can_edit) = app.chat_members(index as usize)?;
    Ok(ChatMembers { participants, can_edit, me: app.handle().map(|h| h.to_string()) })
}

/// Añade personas (handles o contenidos de QR) a un chat que creamos.
pub async fn add_members(index: u32, targets: Vec<String>) -> Result<()> {
    let app = app()?;
    let targets = parse_targets(&targets)?;
    on_rt(async move { app.add_members(index as usize, targets).await }).await
}

/// Quita a una persona de un chat que creamos.
pub async fn remove_member(index: u32, handle: String) -> Result<()> {
    let app = app()?;
    on_rt(async move { app.remove_member(index as usize, &handle).await }).await
}

/// Pasa la gestión de un chat que gestionamos a otra persona (D-049).
pub async fn hand_over(index: u32, handle: String) -> Result<()> {
    let app = app()?;
    on_rt(async move { app.hand_over(index as usize, &handle).await.map(|_| ()) }).await
}

fn parse_targets(targets: &[String]) -> Result<Vec<Target>> {
    targets
        .iter()
        .map(|t| t.trim())
        .filter(|t| !t.is_empty())
        .map(|t| t.parse::<Target>())
        .collect()
}

/// Tamaño máximo de un sticker, en bytes.
#[frb(sync)]
pub fn max_sticker_bytes() -> u32 {
    lacre_node::sticker::MAX_STICKER as u32
}

/// Convierte un GIF o WebP animado que no cabe en un sticker en un WebP
/// animado de como mucho [`max_sticker_bytes`] (D-055).
pub fn fit_animated_sticker(bytes: Vec<u8>) -> Result<Vec<u8>> {
    lacre_node::anim::fit_animated(&bytes, lacre_node::sticker::MAX_STICKER)
}

/// Colección propia de stickers, del más reciente al más antiguo.
#[frb(sync)]
pub fn stickers() -> Result<Vec<Sticker>> {
    Ok(app()?.stickers().into_iter().map(|id| Sticker { id }).collect())
}

/// Añade una imagen a la colección.
#[frb(sync)]
pub fn add_sticker(bytes: Vec<u8>) -> Result<Sticker> {
    Ok(Sticker { id: app()?.add_sticker(&bytes)? })
}

/// Añade a la colección un sticker recibido.
#[frb(sync)]
pub fn keep_sticker(id: String) -> Result<()> {
    app()?.keep_sticker(&id)
}

#[frb(sync)]
pub fn remove_sticker(id: String) -> Result<()> {
    app()?.remove_sticker(&id)
}

pub async fn send_sticker(index: u32, id: String) -> Result<SendResult> {
    let app = app()?;
    on_rt(async move {
        let r = app.send_sticker(index as usize, &id).await?;
        Ok(SendResult { stored: r.stored as u32, failed: r.failed as u32 })
    })
    .await
}

/// Tamaño máximo de un archivo, en bytes.
#[frb(sync)]
pub fn max_file_bytes() -> u64 {
    lacre_node::files::MAX_FILE
}

/// Envía un archivo (hasta [`max_file_bytes`]): se cifra y se copia a los
/// datos de la app; los demás lo descargan de los dispositivos del grupo.
pub async fn send_file(index: u32, path: String) -> Result<SendResult> {
    let app = app()?;
    on_rt(async move {
        let r = app.send_file(index as usize, Path::new(&path)).await?;
        Ok(SendResult { stored: r.stored as u32, failed: r.failed as u32 })
    })
    .await
}

/// Manda la colección de stickers al chat como paquete (D-055).
pub async fn share_stickers(index: u32, name: String) -> Result<SendResult> {
    let app = app()?;
    on_rt(async move {
        let r = app.share_stickers(index as usize, &name).await?;
        Ok(SendResult { stored: r.stored as u32, failed: r.failed as u32 })
    })
    .await
}

/// Añade a la colección los stickers de un paquete descargado; devuelve
/// cuántos eran nuevos.
#[frb(sync)]
pub fn import_stickers(id: String) -> Result<u32> {
    Ok(app()?.import_stickers(&id)? as u32)
}

/// Extensión de los paquetes de stickers.
#[frb(sync)]
pub fn sticker_pack_ext() -> String {
    lacre_node::sticker::PACK_EXT.to_owned()
}

/// Descarga un archivo recibido (queda cifrado en disco). El progreso
/// llega como [`UiEvent::FileProgress`].
pub async fn download_file(id: String) -> Result<()> {
    let app = app()?;
    on_rt(async move { app.download(&id).await }).await
}

/// Descifra un archivo descargado para abrirlo con otra aplicación y
/// devuelve la ruta de la copia. Las copias se borran con
/// [`forget_opened_files`] y al arrancar o parar el nodo.
pub async fn open_file(id: String) -> Result<String> {
    let app = app()?;
    on_rt(async move {
        let a = app.clone();
        let path = tokio::task::spawn_blocking(move || a.open_file(&id)).await??;
        Ok(path.to_string_lossy().into_owned())
    })
    .await
}

/// Borra las copias descifradas de los archivos abiertos.
#[frb(sync)]
pub fn forget_opened_files() -> Result<()> {
    app()?.forget_opened();
    Ok(())
}

pub async fn send(index: u32, text: String) -> Result<SendResult> {
    let app = app()?;
    on_rt(async move {
        let r = app.send(index as usize, &text).await?;
        Ok(SendResult { stored: r.stored as u32, failed: r.failed as u32 })
    })
    .await
}

/// Borra un chat de este dispositivo (su historial incluido). Si alguien
/// vuelve a escribir en él, reaparece vacío.
#[frb(sync)]
pub fn delete_chat(index: u32) -> Result<()> {
    app()?.delete_chat(index as usize)
}

/// Acepta una solicitud de chat (en todos nuestros dispositivos).
pub async fn accept_chat(index: u32) -> Result<()> {
    let app = app()?;
    on_rt(async move { app.accept_chat(index as usize) }).await
}

/// Busca mensajes en todos los chats (`index` nulo) o en uno, sin distinguir
/// mayúsculas ni tildes. Los más recientes primero.
#[frb(sync)]
pub fn search(query: String, index: Option<u32>) -> Result<Vec<Hit>> {
    Ok(app()?.search(&query, index.map(|i| i as usize))?.into_iter().map(Into::into).collect())
}

// ---------------------------------------------------------------- personas

/// Bloquea a una persona: se ignoran sus mensajes e invitaciones.
#[frb(sync)]
pub fn block(handle: String) -> Result<()> {
    app()?.block(&handle)
}

#[frb(sync)]
pub fn unblock(handle: String) -> Result<()> {
    app()?.unblock(&handle)
}

#[frb(sync)]
pub fn blocked() -> Result<Vec<String>> {
    Ok(app()?.blocked())
}

#[frb(sync)]
pub fn is_blocked(handle: String) -> Result<bool> {
    Ok(app()?.is_blocked(&handle))
}

/// Número de seguridad con una persona (D-043): 12 grupos de 5 cifras.
#[frb(sync)]
pub fn safety_number(handle: String) -> Result<String> {
    app()?.safety_number(&handle)
}

/// Foto de perfil de una persona (o la nuestra), descifrada, si la tenemos.
#[frb(sync)]
pub fn avatar_bytes(handle: String) -> Result<Option<Vec<u8>>> {
    Ok(app()?.avatar(&handle)?.1)
}

/// Pone (o quita, con `None`) nuestra foto de perfil: PNG, GIF, WebP o JPEG
/// de hasta 128 KB y 512 px de lado.
pub async fn set_avatar(image: Option<Vec<u8>>) -> Result<()> {
    let app = app()?;
    on_rt(async move { app.set_avatar(image).await }).await
}
