//! Caché de peers en disco (D-021).
//!
//! Guarda direcciones por las que se puede volver a llamar a peers conocidos
//! (servicios onion e IP públicas) para entrar a la red al arrancar sin
//! depender de nodos de arranque fijos. Es un archivo de texto con una
//! dirección `…/p2p/<peer>` por línea, del más reciente al más antiguo.

use std::net::Ipv4Addr;
use std::path::PathBuf;

use libp2p::multiaddr::Protocol;
use libp2p::{Multiaddr, PeerId};
use tracing::warn;

use crate::app::{read_private, write_private};

/// Peers distintos que se recuerdan.
const MAX_PEERS: usize = 64;
/// Direcciones por peer.
const MAX_ADDRS: usize = 4;

pub struct PeerCache {
    path: PathBuf,
    /// Del más reciente al más antiguo.
    peers: Vec<(PeerId, Vec<Multiaddr>)>,
}

impl PeerCache {
    pub fn open(path: PathBuf) -> Self {
        let mut peers: Vec<(PeerId, Vec<Multiaddr>)> = Vec::new();
        let text = read_private(&path).ok().flatten().map(|b| String::from_utf8_lossy(&b).into_owned());
        for addr in text.unwrap_or_default().lines().filter_map(|l| l.trim().parse().ok()) {
            let Some((peer, addr)) = split_peer(addr) else { continue };
            match peers.iter_mut().find(|(p, _)| *p == peer) {
                Some((_, addrs)) => addrs.push(addr),
                None => peers.push((peer, vec![addr])),
            }
        }
        Self { path, peers }
    }

    /// Direcciones completas (con `/p2p/`) para llamar al arrancar.
    pub fn addrs(&self) -> Vec<Multiaddr> {
        self.peers
            .iter()
            .flat_map(|(p, addrs)| addrs.iter().map(|a| a.clone().with(Protocol::P2p(*p))))
            .collect()
    }

    /// Anota las direcciones que anuncia un peer (por identify). Solo se
    /// guardan las alcanzables desde otra red.
    pub fn update(&mut self, peer: PeerId, announced: &[Multiaddr]) {
        let mut addrs: Vec<Multiaddr> = announced
            .iter()
            .map(|a| split_peer(a.clone()).map_or_else(|| a.clone(), |(_, a)| a))
            .filter(is_dialable_from_anywhere)
            .collect();
        addrs.dedup();
        addrs.truncate(MAX_ADDRS);
        if addrs.is_empty() {
            return;
        }
        let pos = self.peers.iter().position(|(p, _)| *p == peer);
        if pos == Some(0) && self.peers[0].1 == addrs {
            return;
        }
        if let Some(i) = pos {
            self.peers.remove(i);
        }
        self.peers.insert(0, (peer, addrs));
        self.peers.truncate(MAX_PEERS);
        let text: String = self.addrs().iter().map(|a| format!("{a}\n")).collect();
        if let Err(e) = write_private(&self.path, text.as_bytes()) {
            warn!("could not save the peer cache: {e:#}");
        }
    }
}

/// Separa el `/p2p/<peer>` final de una dirección.
fn split_peer(mut addr: Multiaddr) -> Option<(PeerId, Multiaddr)> {
    match addr.pop() {
        Some(Protocol::P2p(peer)) => Some((peer, addr)),
        _ => None,
    }
}

/// Servicio onion, o IPv4 pública sin pasar por un relay.
pub fn is_dialable_from_anywhere(addr: &Multiaddr) -> bool {
    if addr.iter().any(|p| p == Protocol::P2pCircuit) {
        return false;
    }
    match addr.iter().next() {
        Some(Protocol::Onion3(_)) => true,
        Some(Protocol::Ip4(ip)) => is_public(ip),
        _ => false,
    }
}

pub(crate) fn is_public(ip: Ipv4Addr) -> bool {
    let cgnat = ip.octets()[0] == 100 && (ip.octets()[1] & 0xc0) == 64;
    !(ip.is_private() || ip.is_loopback() || ip.is_link_local() || ip.is_unspecified() || ip.is_broadcast() || cgnat)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_only_reachable_addresses() {
        let dir = std::env::temp_dir().join(format!("lacre-peers-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("peers.txt");
        let peer = PeerId::random();
        let mut cache = PeerCache::open(path.clone());
        let onion: Multiaddr =
            "/onion3/t47o6752rjd26og3ufndd3nhingxqiloxzvyw7llh3pj6bna5wmeaiqd:4001".parse().unwrap();
        cache.update(
            peer,
            &[
                "/ip4/192.168.1.5/tcp/4001".parse().unwrap(),
                "/ip4/100.98.132.201/tcp/4001".parse().unwrap(),
                "/ip4/85.87.64.38/tcp/4001".parse().unwrap(),
                onion.clone(),
            ],
        );
        let reopened = PeerCache::open(path);
        assert_eq!(
            reopened.addrs(),
            vec![
                "/ip4/85.87.64.38/tcp/4001".parse::<Multiaddr>().unwrap().with(Protocol::P2p(peer)),
                onion.with(Protocol::P2p(peer)),
            ]
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
}
