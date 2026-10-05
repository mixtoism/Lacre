//! Punto de encuentro entre redes (D-019).
//!
//! mDNS solo funciona en la red local. Para que un nodo en otra red encuentre
//! la nuestra sin una lista fija de servidores, los nodos que ofrecen relay se
//! anuncian en un *infohash* fijo de la DHT Mainline de BitTorrent y, como
//! respaldo, en trackers UDP públicos. Los demás nodos preguntan por ese
//! infohash, se conectan a los relays que aparezcan y desde ahí entran en
//! nuestra DHT (y reservan hueco en el relay si están tras NAT).
//!
//! Solo se intercambian direcciones IP y puertos: la identidad del nodo la
//! comprueba Noise al conectar, así que una entrada falsa solo cuesta un dial.

use std::collections::HashSet;
use std::net::{SocketAddr, SocketAddrV4};
use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use futures::StreamExt;
use libp2p::Multiaddr;
use libp2p::multiaddr::Protocol;
use mainline::async_dht::AsyncDht;
use mainline::{Dht, Id};
use rand::seq::SliceRandom;
use tokio::net::UdpSocket;
use tracing::{debug, info, warn};

use crate::net::Client;

/// Trackers UDP de respaldo. Cambian con el tiempo: se pueden sustituir con
/// `--tracker` o con un archivo `trackers.txt` en el directorio de datos.
pub const DEFAULT_TRACKERS: &[&str] = &[
    "tracker.opentrackr.org:1337",
    "open.stealth.si:80",
    "tracker.torrent.eu.org:451",
    "exodus.desync.com:6969",
    "open.demonii.com:1337",
];

/// Con menos conexiones que esto se busca en el punto de encuentro.
const MIN_PEERS: usize = 3;
/// Relays a los que se llama en cada búsqueda.
const MAX_DIALS: usize = 8;
const CHECK_EVERY: Duration = Duration::from_secs(60);
/// Los anuncios en Mainline caducan a los ~30 min.
const ANNOUNCE_EVERY: Duration = Duration::from_secs(15 * 60);
const TRACKER_TIMEOUT: Duration = Duration::from_secs(5);

pub struct Config {
    /// Anunciarse como relay (solo nodos con `--relay-server`).
    pub announce: bool,
    /// Nodos de arranque de Mainline (`host:puerto`); vacío = los de siempre.
    pub dht_bootstrap: Vec<String>,
    pub trackers: Vec<String>,
}

/// Infohash donde se anuncian los relays de p2message.
fn topic() -> [u8; 20] {
    let h = blake3::derive_key("p2message/v1 rendezvous relays", b"");
    h[..20].try_into().unwrap()
}

/// Lista de trackers: `trackers.txt` del directorio de datos si existe
/// (uno por línea, `#` para comentarios), o la de serie.
pub fn load_trackers(data: &Path) -> Vec<String> {
    match std::fs::read_to_string(data.join("trackers.txt")) {
        Ok(text) => text
            .lines()
            .map(|l| l.split('#').next().unwrap().trim())
            .filter(|l| !l.is_empty())
            .map(normalize_tracker)
            .collect(),
        Err(_) => DEFAULT_TRACKERS.iter().map(|s| s.to_string()).collect(),
    }
}

/// Acepta `udp://host:puerto/announce` o `host:puerto`.
pub fn normalize_tracker(t: &str) -> String {
    let t = t.trim_start_matches("udp://");
    t.split('/').next().unwrap_or(t).to_string()
}

pub fn spawn(cfg: Config, net: Client) {
    tokio::spawn(async move {
        let dht = match start_dht(&cfg) {
            Ok(d) => Some(d),
            Err(e) => {
                warn!("no se pudo arrancar Mainline: {e:#}");
                None
            }
        };
        let topic = topic();
        let mut last_announce: Option<tokio::time::Instant> = None;
        loop {
            if cfg.announce && last_announce.is_none_or(|t| t.elapsed() >= ANNOUNCE_EVERY) {
                match tcp_port(&net).await {
                    Some(port) => {
                        announce(dht.as_ref(), &cfg.trackers, topic, port).await;
                        last_announce = Some(tokio::time::Instant::now());
                    }
                    None => debug!("sin puerto de escucha todavía; no se anuncia"),
                }
            }
            let connected = net.connected_peers().await.map(|p| p.len()).unwrap_or(0);
            if connected < MIN_PEERS {
                // Algunos trackers rechazan el puerto 0 aunque solo preguntemos.
                let port = tcp_port(&net).await.unwrap_or(6881);
                let found = discover(dht.as_ref(), &cfg.trackers, topic, port).await;
                // Un relay puede encontrarse a sí mismo: libp2p rechaza ese dial.
                let mut found: Vec<_> = found.into_iter().collect();
                found.shuffle(&mut rand::thread_rng());
                if !found.is_empty() {
                    info!("punto de encuentro: {} relay(s) encontrados", found.len());
                    debug!(?found, "relays del punto de encuentro");
                }
                for addr in found.into_iter().take(MAX_DIALS) {
                    net.dial(tcp_multiaddr(addr)).await;
                }
            }
            tokio::time::sleep(CHECK_EVERY).await;
        }
    });
}

fn start_dht(cfg: &Config) -> Result<AsyncDht> {
    let mut b = Dht::builder();
    if !cfg.dht_bootstrap.is_empty() {
        b.bootstrap(&cfg.dht_bootstrap);
    }
    if cfg.announce {
        // Un relay tiene IP pública y está siempre encendido: puede ayudar a
        // la DHT Mainline en vez de solo usarla.
        b.server_mode();
    }
    Ok(b.build().context("socket de Mainline")?.as_async())
}

async fn tcp_port(net: &Client) -> Option<u16> {
    net.listen_addrs().await.ok()?.iter().find_map(|a| {
        let mut it = a.iter();
        match (it.next(), it.next()) {
            (Some(Protocol::Ip4(_)), Some(Protocol::Tcp(p))) => Some(p),
            _ => None,
        }
    })
}

fn tcp_multiaddr(addr: SocketAddrV4) -> Multiaddr {
    Multiaddr::empty().with(Protocol::Ip4(*addr.ip())).with(Protocol::Tcp(addr.port()))
}

async fn announce(dht: Option<&AsyncDht>, trackers: &[String], topic: [u8; 20], port: u16) {
    if let Some(dht) = dht {
        match dht.announce_peer(Id::from(topic), Some(port)).await {
            Ok(_) => info!("anunciado como relay en Mainline (puerto {port})"),
            Err(e) => warn!("anuncio en Mainline fallido: {e}"),
        }
    }
    for t in trackers {
        match tracker_announce(t, topic, port, true).await {
            Ok(_) => debug!(tracker = %t, "anunciado como relay"),
            Err(e) => debug!(tracker = %t, "anuncio fallido: {e:#}"),
        }
    }
}

async fn discover(dht: Option<&AsyncDht>, trackers: &[String], topic: [u8; 20], port: u16) -> HashSet<SocketAddrV4> {
    let mut found = HashSet::new();
    if let Some(dht) = dht {
        let mut stream = dht.get_peers(Id::from(topic));
        let collect = async {
            while let Some(batch) = stream.next().await {
                found.extend(batch);
            }
        };
        let _ = tokio::time::timeout(Duration::from_secs(20), collect).await;
    }
    for t in trackers {
        match tracker_announce(t, topic, port, false).await {
            Ok(peers) => found.extend(peers),
            Err(e) => debug!(tracker = %t, "consulta fallida: {e:#}"),
        }
    }
    found.retain(|a| a.port() != 0 && !a.ip().is_unspecified());
    found
}

// --- Trackers UDP (BEP 15) ------------------------------------------------

const PROTOCOL_ID: u64 = 0x41727101980;

/// Anuncio en un tracker UDP. Los relays se anuncian como *seeders*
/// (`left = 0`); quien solo busca se anuncia como *leecher*, que es a quien el
/// tracker devuelve seeders, y después se da de baja para no aparecer en la
/// lista.
async fn tracker_announce(tracker: &str, topic: [u8; 20], port: u16, relay: bool) -> Result<Vec<SocketAddrV4>> {
    let addr = tokio::net::lookup_host(tracker)
        .await?
        .find(SocketAddr::is_ipv4)
        .context("sin dirección IPv4")?;
    let sock = UdpSocket::bind("0.0.0.0:0").await?;
    sock.connect(addr).await?;

    let tx: u32 = rand::random();
    let mut req = Vec::with_capacity(16);
    req.extend(PROTOCOL_ID.to_be_bytes());
    req.extend(0u32.to_be_bytes());
    req.extend(tx.to_be_bytes());
    let resp = exchange(&sock, &req, 16).await?;
    if u32_at(&resp, 0) != 0 || u32_at(&resp, 4) != tx {
        bail!("respuesta de conexión inválida");
    }
    let conn: [u8; 8] = resp[8..16].try_into().unwrap();

    let peer_id: [u8; 20] = rand::random();
    let key: u32 = rand::random();
    let packet = |event: u32| {
        let mut a = Vec::with_capacity(98);
        a.extend(conn);
        a.extend(1u32.to_be_bytes());
        a.extend(tx.to_be_bytes());
        a.extend(topic);
        a.extend(peer_id);
        a.extend(0u64.to_be_bytes()); // descargado
        a.extend((if relay { 0u64 } else { 1 }).to_be_bytes()); // pendiente
        a.extend(0u64.to_be_bytes()); // subido
        a.extend(event.to_be_bytes());
        a.extend(0u32.to_be_bytes()); // IP: la de origen
        a.extend(key.to_be_bytes());
        a.extend((-1i32).to_be_bytes()); // los que el tracker quiera
        a.extend(port.to_be_bytes());
        a
    };
    // 2 = started, 3 = stopped
    let resp = exchange(&sock, &packet(if relay { 2 } else { 0 }), 8).await?;
    if u32_at(&resp, 0) == 3 {
        bail!("el tracker responde: {}", String::from_utf8_lossy(&resp[8..]));
    }
    if resp.len() < 20 || u32_at(&resp, 0) != 1 || u32_at(&resp, 4) != tx {
        bail!("respuesta de anuncio inválida");
    }
    let peers = resp[20..]
        .chunks_exact(6)
        .map(|c| SocketAddrV4::new([c[0], c[1], c[2], c[3]].into(), u16::from_be_bytes([c[4], c[5]])))
        .collect();
    if !relay {
        let _ = sock.send(&packet(3)).await;
    }
    Ok(peers)
}

async fn exchange(sock: &UdpSocket, req: &[u8], min_len: usize) -> Result<Vec<u8>> {
    let mut buf = vec![0u8; 2048];
    for _ in 0..2 {
        sock.send(req).await?;
        if let Ok(n) = tokio::time::timeout(TRACKER_TIMEOUT, sock.recv(&mut buf)).await {
            let n = n?;
            if n >= min_len {
                buf.truncate(n);
                return Ok(buf);
            }
        }
    }
    bail!("sin respuesta")
}

fn u32_at(b: &[u8], i: usize) -> u32 {
    u32::from_be_bytes(b[i..i + 4].try_into().unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracker_urls_are_normalized() {
        assert_eq!(normalize_tracker("udp://tracker.opentrackr.org:1337/announce"), "tracker.opentrackr.org:1337");
        assert_eq!(normalize_tracker("open.stealth.si:80"), "open.stealth.si:80");
    }
}
