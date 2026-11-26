//! Transporte libp2p sobre Tor (D-020).
//!
//! Cada nodo publica un servicio onion y acepta conexiones por él, así que es
//! alcanzable aunque esté tras CGNAT y sin ningún servidor público propio. Las
//! direcciones `/onion3/…` se difunden como cualquier otra (identify y la
//! DHT). Encima van Noise y yamux, igual que sobre TCP.
//!
//! La clave del servicio onion se deriva de la del dispositivo (BLAKE3, en
//! un dominio propio) y vive solo en memoria: Arti usa un almacén de claves
//! efímero, así que en `<datos>/tor` no queda ninguna clave privada (D-044).
//! No es la clave del dispositivo, pero la misma clave da siempre la misma
//! dirección onion.

use std::fmt;
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use anyhow::{Context as _, Result};
use arti_client::config::TorClientConfigBuilder;
use arti_client::config::onion_service::OnionServiceConfigBuilder;
use arti_client::{DataStream, TorClient};
use futures::future::{BoxFuture, Ready, ready};
use futures::{FutureExt, StreamExt};
use libp2p::core::transport::{DialOpts, ListenerId, TransportError, TransportEvent};
use libp2p::multiaddr::{Onion3Addr, Protocol};
use libp2p::{Multiaddr, Transport};
use safelog::DisplayRedacted;
use tokio::sync::mpsc;
use tor_cell::relaycell::msg::Connected;
use tor_config::ExplicitOrAuto;
use tor_hscrypto::pk::HsIdKeypair;
use tor_keymgr::config::ArtiKeystoreKind;
use tor_hsservice::{HsNickname, RunningOnionService, StreamRequest, handle_rend_requests};
use tor_proto::stream::IncomingStreamRequest;
use tor_rtcompat::PreferredRuntime;
use tracing::{debug, info, warn};

/// Puerto virtual del servicio onion (no abre nada en la máquina).
pub const ONION_PORT: u16 = 4001;

#[derive(Debug)]
pub struct TorError(String);

impl fmt::Display for TorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "tor: {}", self.0)
    }
}

impl std::error::Error for TorError {}

enum ServiceEvent {
    /// El descriptor está publicado: ya se nos puede llamar.
    Reachable,
    Stream(Box<DataStream>),
}

pub struct TorTransport {
    client: Arc<TorClient<PreferredRuntime>>,
    /// Nuestra dirección onion.
    addr: Multiaddr,
    events: mpsc::Receiver<ServiceEvent>,
    listener: Option<ListenerId>,
    reachable: bool,
    /// Mantiene vivo el servicio.
    _service: Arc<RunningOnionService>,
}

impl TorTransport {
    /// Arranca el cliente Tor y el servicio onion. No espera a que estén
    /// listos: el arranque sigue en segundo plano.
    pub fn new(data: &Path, device_secret: &[u8; 32]) -> Result<Self> {
        let dir = data.join("tor");
        // Versiones anteriores guardaban aquí la clave onion en claro. Y el
        // estado de los puntos de introducción apunta a claves que, con el
        // almacén efímero, no sobreviven a un reinicio: se empieza de cero.
        for stale in ["keystore", "hss"] {
            let path = dir.join("state").join(stale);
            if path.exists() {
                std::fs::remove_dir_all(&path).with_context(|| format!("borrar {}", path.display()))?;
            }
        }
        let mut config = TorClientConfigBuilder::from_directories(dir.join("state"), dir.join("cache"));
        config
            .storage()
            .keystore()
            .primary()
            .kind(ExplicitOrAuto::Explicit(ArtiKeystoreKind::Ephemeral));
        // En el móvil, menos relleno en los canales: menos radio despierta
        // (D-034). Sigue habiendo el mínimo que pide la red.
        if cfg!(target_os = "android") {
            config.channel().padding(tor_config::PaddingLevel::Reduced);
        }
        let config = config.build().context("configuración de Tor")?;
        let client = TorClient::builder().config(config).create_unbootstrapped().context("cliente Tor")?;

        let nick: HsNickname = "p2message".parse().expect("apodo válido");
        let svc = OnionServiceConfigBuilder::default().nickname(nick).build().context("servicio onion")?;
        let (service, requests) = client
            .launch_onion_service_with_hsid(svc, onion_keypair(device_secret))
            .context("arranque del servicio onion")?
            .context("servicio onion desactivado")?;
        let hsid = service.onion_address().context("servicio onion sin dirección")?;
        let addr = onion_multiaddr(&hsid.display_unredacted().to_string())?;

        {
            let client = client.clone();
            tokio::spawn(async move {
                let start = std::time::Instant::now();
                match client.bootstrap().await {
                    Ok(()) => info!(ms = start.elapsed().as_millis(), "conectado a la red Tor"),
                    Err(e) => warn!("no se pudo conectar a Tor: {e}"),
                }
            });
        }
        let (tx, events) = mpsc::channel(32);
        let mut status = service.status_events();
        let ready_tx = tx.clone();
        tokio::spawn(async move {
            let start = std::time::Instant::now();
            while let Some(s) = status.next().await {
                debug!(estado = ?s.state(), "servicio onion");
                if s.state().is_fully_reachable() {
                    info!(ms = start.elapsed().as_millis(), "servicio onion alcanzable");
                    let _ = ready_tx.send(ServiceEvent::Reachable).await;
                    break;
                }
            }
        });
        tokio::spawn(async move {
            let mut streams = Box::pin(handle_rend_requests(requests));
            while let Some(req) = streams.next().await {
                let tx = tx.clone();
                tokio::spawn(async move {
                    if let Some(s) = accept(req).await {
                        let _ = tx.send(ServiceEvent::Stream(Box::new(s))).await;
                    }
                });
            }
        });
        Ok(Self { client, addr, events, listener: None, reachable: false, _service: service })
    }

    /// Dirección onion de este nodo.
    pub fn address(&self) -> Multiaddr {
        self.addr.clone()
    }

    /// Cliente Tor, para dormirlo y despertarlo (D-034).
    pub fn client(&self) -> Arc<TorClient<PreferredRuntime>> {
        self.client.clone()
    }

    /// Transporte solo de salida con circuitos propios, que no comparte con
    /// los de este nodo: para depositar en buzones sin que se nos asocie
    /// (D-034).
    pub fn isolated_dialer(&self) -> TorDialer {
        TorDialer { client: self.client.isolated_client() }
    }
}

/// Transporte Tor solo de salida (ver [`TorTransport::isolated_dialer`]).
pub struct TorDialer {
    client: Arc<TorClient<PreferredRuntime>>,
}

impl TorDialer {
    /// Otro transporte, aislado también de este.
    pub fn isolated(&self) -> Self {
        Self { client: self.client.isolated_client() }
    }
}

fn dial_onion(client: &Arc<TorClient<PreferredRuntime>>, host: String, port: u16) -> BoxFuture<'static, Result<DataStream, TorError>> {
    let client = client.clone();
    async move {
        debug!(%host, "llamando por Tor");
        let start = std::time::Instant::now();
        let stream = client.connect((host.as_str(), port)).await.map_err(|e| TorError(e.to_string()));
        debug!(%host, ms = start.elapsed().as_millis(), ok = stream.is_ok(), "llamada por Tor");
        stream
    }
    .boxed()
}

impl Transport for TorDialer {
    type Output = DataStream;
    type Error = TorError;
    type ListenerUpgrade = Ready<Result<DataStream, TorError>>;
    type Dial = BoxFuture<'static, Result<DataStream, TorError>>;

    fn listen_on(&mut self, _id: ListenerId, addr: Multiaddr) -> Result<(), TransportError<TorError>> {
        Err(TransportError::MultiaddrNotSupported(addr))
    }

    fn remove_listener(&mut self, _id: ListenerId) -> bool {
        false
    }

    fn dial(&mut self, addr: Multiaddr, _opts: DialOpts) -> Result<Self::Dial, TransportError<TorError>> {
        let Some((host, port)) = onion_target(&addr) else {
            return Err(TransportError::MultiaddrNotSupported(addr));
        };
        Ok(dial_onion(&self.client, host, port))
    }

    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<TransportEvent<Self::ListenerUpgrade, TorError>> {
        Poll::Pending
    }
}

async fn accept(req: StreamRequest) -> Option<DataStream> {
    match req.request() {
        IncomingStreamRequest::Begin(b) if b.port() == ONION_PORT => {}
        _ => {
            let _ = req.shutdown_circuit();
            return None;
        }
    }
    match req.accept(Connected::new_empty()).await {
        Ok(s) => Some(s),
        Err(e) => {
            debug!("conexión onion rechazada: {e}");
            None
        }
    }
}

/// Clave del servicio onion derivada de la del dispositivo (D-044).
fn onion_keypair(device_secret: &[u8; 32]) -> HsIdKeypair {
    let seed = zeroize::Zeroizing::new(blake3::derive_key("p2message/onion-id/v1", device_secret));
    let kp = tor_llcrypto::pk::ed25519::Keypair::from_bytes(&seed);
    HsIdKeypair::from(tor_llcrypto::pk::ed25519::ExpandedKeypair::from(&kp))
}

/// `xxxx.onion` → `/onion3/xxxx:ONION_PORT`.
fn onion_multiaddr(host: &str) -> Result<Multiaddr> {
    let b32 = host.trim_end_matches(".onion").to_ascii_uppercase();
    let raw = data_encoding::BASE32_NOPAD.decode(b32.as_bytes()).context("dirección onion")?;
    let raw: [u8; 35] = raw.try_into().map_err(|_| anyhow::anyhow!("dirección onion de longitud incorrecta"))?;
    Ok(Multiaddr::empty().with(Protocol::Onion3(Onion3Addr::from((raw, ONION_PORT)))))
}

/// `/onion3/xxxx:puerto[/p2p/…]` → (`xxxx.onion`, puerto).
fn onion_target(addr: &Multiaddr) -> Option<(String, u16)> {
    let mut it = addr.iter();
    let Some(Protocol::Onion3(a)) = it.next() else { return None };
    if !matches!(it.next(), None | Some(Protocol::P2p(_))) {
        return None;
    }
    let host = data_encoding::BASE32_NOPAD.encode(a.hash()).to_ascii_lowercase();
    Some((format!("{host}.onion"), a.port()))
}

impl Transport for TorTransport {
    type Output = DataStream;
    type Error = TorError;
    type ListenerUpgrade = Ready<Result<DataStream, TorError>>;
    type Dial = BoxFuture<'static, Result<DataStream, TorError>>;

    fn listen_on(&mut self, id: ListenerId, addr: Multiaddr) -> Result<(), TransportError<TorError>> {
        if addr != self.addr || self.listener.is_some() {
            return Err(TransportError::MultiaddrNotSupported(addr));
        }
        self.listener = Some(id);
        Ok(())
    }

    fn remove_listener(&mut self, id: ListenerId) -> bool {
        if self.listener == Some(id) {
            self.listener = None;
            return true;
        }
        false
    }

    fn dial(&mut self, addr: Multiaddr, _opts: DialOpts) -> Result<Self::Dial, TransportError<TorError>> {
        let Some((host, port)) = onion_target(&addr) else {
            return Err(TransportError::MultiaddrNotSupported(addr));
        };
        if Some((host.clone(), port)) == onion_target(&self.addr) {
            return Err(TransportError::Other(TorError("llamada a nosotros mismos".into())));
        }
        Ok(dial_onion(&self.client, host, port))
    }

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<TransportEvent<Self::ListenerUpgrade, TorError>> {
        let Some(listener_id) = self.listener else { return Poll::Pending };
        loop {
            match self.events.poll_recv(cx) {
                Poll::Ready(Some(ServiceEvent::Reachable)) => {
                    if !self.reachable {
                        self.reachable = true;
                        info!(addr = %self.addr, "servicio onion publicado");
                        return Poll::Ready(TransportEvent::NewAddress { listener_id, listen_addr: self.addr.clone() });
                    }
                }
                Poll::Ready(Some(ServiceEvent::Stream(s))) => {
                    return Poll::Ready(TransportEvent::Incoming {
                        listener_id,
                        upgrade: ready(Ok(*s)),
                        local_addr: self.addr.clone(),
                        // Tor no revela quién llama.
                        send_back_addr: Multiaddr::empty(),
                    });
                }
                Poll::Ready(None) => {
                    warn!("el servicio onion se ha detenido");
                    self.listener = None;
                    return Poll::Ready(TransportEvent::ListenerClosed { listener_id, reason: Ok(()) });
                }
                Poll::Pending => return Poll::Pending,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn onion_key_is_stable_and_not_the_device_key() {
        use tor_hscrypto::pk::HsIdKey;
        let id = |s: &[u8; 32]| HsIdKey::from(&onion_keypair(s)).id();
        assert_eq!(id(&[1; 32]), id(&[1; 32]));
        assert_ne!(id(&[1; 32]), id(&[2; 32]));
        let device = tor_llcrypto::pk::ed25519::Keypair::from_bytes(&[1; 32]);
        assert_ne!(id(&[1; 32]).as_ref(), device.verifying_key().as_bytes());
    }

    #[test]
    fn onion_addresses_round_trip() {
        let host = "pg6mmjiyjmcrsslvykfwnntlaru7p5svn6y2ymmju6nubxndf4pscryd.onion";
        let addr = onion_multiaddr(host).unwrap();
        assert_eq!(onion_target(&addr), Some((host.to_string(), ONION_PORT)));
    }
}
