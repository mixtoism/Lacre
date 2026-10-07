//! Arranque de un nodo completo (red, punto de encuentro y lógica de
//! chats). Lo usan la CLI y la app (`p2m-api`).

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use libp2p::{Multiaddr, PeerId};
use tokio::sync::mpsc;

use crate::app::{self, App, Event};
use crate::net::{self, NetConfig, NetEvent};
use crate::rendezvous;

pub struct Options {
    /// Directorio de datos del dispositivo.
    pub data: PathBuf,
    /// Puerto TCP/QUIC (0 = aleatorio).
    pub port: u16,
    /// Escuchar conexiones directas.
    pub listen: bool,
    pub relay_server: bool,
    pub mdns: bool,
    pub tor: bool,
    /// Buscar relays en Mainline y trackers (D-019).
    pub rendezvous: bool,
    pub bootstrap: Vec<Multiaddr>,
    pub dht_bootstrap: Vec<String>,
    /// `None` = `trackers.txt` del directorio de datos o la lista de serie.
    pub trackers: Option<Vec<String>>,
    /// Intervalo de republicación y sincronización.
    pub tick: Duration,
    /// Clave de datos para cifrar el estado en disco (D-033).
    pub data_key: Option<[u8; 32]>,
}

impl Options {
    /// Valores por defecto para un dispositivo normal.
    pub fn new(data: PathBuf) -> Self {
        Self {
            data,
            port: 0,
            listen: true,
            relay_server: false,
            mdns: true,
            tor: true,
            rendezvous: true,
            bootstrap: Vec::new(),
            dht_bootstrap: Vec::new(),
            trackers: None,
            tick: Duration::from_secs(30),
            data_key: None,
        }
    }
}

pub struct Node {
    pub app: Arc<App>,
    pub peer_id: PeerId,
    pub events: mpsc::UnboundedReceiver<Event>,
}

/// Clave de datos de la variable `P2M_DATA_KEY` (64 caracteres hex), para la
/// CLI y las pruebas. Las apps la sacan del almacén del sistema.
pub fn data_key_from_env() -> Result<Option<[u8; 32]>> {
    let Ok(hex) = std::env::var("P2M_DATA_KEY") else { return Ok(None) };
    let bytes = data_encoding::HEXLOWER_PERMISSIVE
        .decode(hex.trim().as_bytes())
        .map_err(|_| anyhow::anyhow!("P2M_DATA_KEY no es hexadecimal"))?;
    let key = bytes.try_into().map_err(|_| anyhow::anyhow!("P2M_DATA_KEY debe tener 32 bytes (64 hex)"))?;
    Ok(Some(key))
}

/// Arranca el nodo. Tiene que llamarse dentro de un runtime de Tokio, que
/// debe seguir vivo mientras se use el nodo.
pub fn start(opts: Options) -> Result<Node> {
    match opts.data_key {
        Some(key) => {
            p2m_core::vault::set_key(key)?;
            let n = app::migrate_to_vault(&opts.data)?;
            if n > 0 {
                tracing::info!("{n} archivos de estado cifrados");
            }
        }
        None if app::has_sealed_state(&opts.data) => {
            anyhow::bail!("el estado de {} está cifrado y falta la clave de datos", opts.data.display())
        }
        None => {}
    }
    let device = app::load_device_key(&opts.data)?;
    let cfg = NetConfig {
        port: opts.port,
        listen: opts.listen,
        relay_server: opts.relay_server,
        mdns: opts.mdns,
        tor: opts.tor.then(|| opts.data.clone()),
        bootstrap: opts.bootstrap,
        peer_cache: Some(opts.data.join("peers.txt")),
    };
    let (client, mut net_events, peer_id) = net::spawn(device.to_bytes(), cfg)?;
    if opts.rendezvous {
        let trackers = opts.trackers.unwrap_or_else(|| rendezvous::load_trackers(&opts.data));
        let cfg = rendezvous::Config { announce: opts.relay_server, dht_bootstrap: opts.dht_bootstrap, trackers };
        rendezvous::spawn(cfg, client.clone());
    }
    let (tx, events) = mpsc::unbounded_channel();
    let app = App::open(opts.data, device, client, tx)?;

    {
        let app = app.clone();
        tokio::spawn(async move {
            while let Some(ev) = net_events.recv().await {
                let app = app.clone();
                match ev {
                    NetEvent::Request(req) => {
                        tokio::spawn(async move { app.handle_inbound(req).await });
                    }
                    NetEvent::Connected(peer) => {
                        tokio::spawn(async move {
                            app.on_connected(peer).await;
                            app.fetch_mailboxes(vec![peer]).await
                        });
                    }
                    NetEvent::RelayReady(addr) => app.emit(Event::RelayReady(addr.to_string())),
                }
            }
        });
    }
    {
        let app = app.clone();
        let every = opts.tick.max(Duration::from_secs(5));
        tokio::spawn(async move {
            let mut power = app.net().power();
            // Primer tick pronto, para dar tiempo a mDNS a descubrir peers.
            tokio::time::sleep(Duration::from_secs(3)).await;
            loop {
                app.tick().await;
                // En reposo, un tick cada mucho (D-034); al salir de él, uno ya.
                let wait = if power.borrow_and_update().low { net::LOW_POWER_EVERY.max(every) } else { every };
                tokio::select! {
                    _ = tokio::time::sleep(wait) => {}
                    Ok(()) = power.changed() => {}
                }
            }
        });
    }
    {
        let app = app.clone();
        tokio::spawn(async move {
            let mut power = app.net().power();
            tokio::time::sleep(Duration::from_secs(3)).await;
            loop {
                // En reposo no se mantienen conexiones: se nos llega por onion.
                if !power.borrow_and_update().low {
                    app.keepalive();
                }
                tokio::select! {
                    _ = tokio::time::sleep(crate::app::KEEPALIVE_EVERY) => {}
                    Ok(()) = power.changed() => {}
                }
            }
        });
    }
    Ok(Node { app, peer_id, events })
}
