//! API que la app Flutter llama a través de flutter_rust_bridge (D-022).
//!
//! Hay un único nodo por proceso: `start` lo arranca en un runtime de Tokio
//! propio y el resto de funciones lo usan. Los avisos llegan por
//! `event_stream`.

use std::future::Future;
use std::path::PathBuf;
use std::sync::{Arc, LazyLock, Mutex, OnceLock};

use anyhow::{Context, Result, anyhow};
use flutter_rust_bridge::frb;
use p2m_node::app::{App, Event, Target};
use p2m_node::history::StoredMessage;
use p2m_node::node::{self, Options};
use tokio::sync::mpsc::UnboundedReceiver;

use crate::frb_generated::StreamSink;

static RT: LazyLock<tokio::runtime::Runtime> =
    LazyLock::new(|| tokio::runtime::Runtime::new().expect("runtime de Tokio"));
static NODE: OnceLock<(Arc<App>, PathBuf)> = OnceLock::new();
static EVENTS: Mutex<Option<UnboundedReceiver<Event>>> = Mutex::new(None);

#[frb(init)]
pub fn init_app() {
    flutter_rust_bridge::setup_default_user_utils();
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "warn,p2m_node=info".into()),
        )
        .try_init();
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
}

pub struct Message {
    pub from: String,
    pub text: String,
    pub sent_at: i64,
    pub mine: bool,
}

impl From<StoredMessage> for Message {
    fn from(m: StoredMessage) -> Self {
        Self { from: m.from, text: m.text, sent_at: m.sent_at as i64, mine: m.mine }
    }
}

pub struct ChatInfo {
    pub index: u32,
    pub name: String,
    pub devices: u32,
    pub active: bool,
    pub last: Option<Message>,
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
}

pub struct SendResult {
    pub stored: u32,
    pub failed: u32,
}

pub enum UiEvent {
    Linked { handle: String },
    NewChat { index: u32, name: String },
    Message { index: u32, message: Message, local: bool },
    Removed { index: u32 },
    UnverifiedMember { index: u32, name: String },
    RelayReady { addr: String },
}

impl From<Event> for UiEvent {
    fn from(ev: Event) -> Self {
        match ev {
            Event::Linked { handle } => Self::Linked { handle },
            Event::NewChat { index, name } => Self::NewChat { index: index as u32, name },
            Event::Message { index, message, local } => Self::Message { index: index as u32, message: message.into(), local },
            Event::Removed { index } => Self::Removed { index: index as u32 },
            Event::UnverifiedMember { index, name } => Self::UnverifiedMember { index: index as u32, name },
            Event::RelayReady(addr) => Self::RelayReady { addr },
        }
    }
}

// ---------------------------------------------------------------- nodo

/// Arranca el nodo con los datos de `data_dir`. Llamarla otra vez no hace
/// nada (devuelve el perfil).
pub fn start(data_dir: String) -> Result<Profile> {
    if NODE.get().is_none() {
        let data = PathBuf::from(&data_dir);
        std::fs::create_dir_all(&data)?;
        let n = {
            let _guard = RT.enter();
            node::start(Options::new(data.clone()))?
        };
        *EVENTS.lock().unwrap() = Some(n.events);
        let _ = NODE.set((n.app, data));
    }
    profile()
}

#[frb(sync)]
pub fn profile() -> Result<Profile> {
    let (app, dir) = NODE.get().context("el nodo no está arrancado")?;
    Ok(Profile {
        handle: app.handle().map(|h| h.to_string()),
        device_code: app.device_code(),
        primary: dir.join("root.key").exists(),
    })
}

/// Flujo de avisos. Solo puede haber un suscriptor.
pub fn event_stream(sink: StreamSink<UiEvent>) -> Result<()> {
    let mut rx = EVENTS.lock().unwrap().take().context("ya hay un suscriptor de eventos")?;
    RT.spawn(async move {
        while let Some(ev) = rx.recv().await {
            if sink.add(ev.into()).is_err() {
                break;
            }
        }
    });
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
        Ok(Status { peers, onion })
    })
    .await
}

// ---------------------------------------------------------------- identidad

/// Crea una identidad nueva con este dispositivo como primario.
pub async fn create_identity(name: String) -> Result<String> {
    let app = app()?;
    on_rt(async move { Ok(app.init(&name).await?.to_string()) }).await
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

pub async fn link_device(code: String, label: String) -> Result<()> {
    let app = app()?;
    on_rt(async move { app.link(code.trim(), &label).await }).await
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
    let targets = targets
        .iter()
        .map(|t| t.trim())
        .filter(|t| !t.is_empty())
        .map(|t| t.parse::<Target>())
        .collect::<Result<Vec<_>>>()?;
    on_rt(async move { Ok(app.create_chat(title.filter(|t| !t.trim().is_empty()), targets).await? as u32) }).await
}

pub async fn send(index: u32, text: String) -> Result<SendResult> {
    let app = app()?;
    on_rt(async move {
        let r = app.send(index as usize, &text).await?;
        Ok(SendResult { stored: r.stored as u32, failed: r.failed as u32 })
    })
    .await
}
