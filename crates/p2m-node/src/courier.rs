//! Mensajero: entrega peticiones con una identidad libp2p de un solo uso
//! (D-034). Se usa para depositar en buzones: el sobre ya no dice quién lo
//! envía, y así tampoco lo dice la conexión (Noise autentica la clave de
//! quien llama). Si hay Tor y el buzón tiene dirección onion, solo se usa
//! esa, con circuitos aislados de los del nodo: el buzón no ve ni la IP.

use std::collections::HashMap;
use std::time::Duration;

use anyhow::Result;
use futures::StreamExt;
use libp2p::core::transport::OptionalTransport;
use libp2p::core::{Transport, upgrade};
use libp2p::multiaddr::Protocol;
use libp2p::request_response::{self, OutboundRequestId, ProtocolSupport};
use libp2p::swarm::SwarmEvent;
use libp2p::{Multiaddr, PeerId, noise, tcp, yamux};
use tracing::debug;

use crate::net::{MSG_PROTOCOL, dns_config};
use crate::proto::{Request, Response};
use crate::tor::TorDialer;

/// Plazo total de una entrega (abrir un circuito onion lleva sus segundos).
const DEADLINE: Duration = Duration::from_secs(90);

fn usable(addr: &Multiaddr) -> bool {
    !addr.iter().any(|p| p == Protocol::P2pCircuit)
}

fn is_onion(addr: &Multiaddr) -> bool {
    matches!(addr.iter().next(), Some(Protocol::Onion3(_)))
}

/// Envía `req` a cada peer, con sus direcciones conocidas, desde una
/// identidad nueva. Devuelve la respuesta de los que contestaron.
pub async fn send(
    tor: Option<TorDialer>,
    targets: Vec<(PeerId, Vec<Multiaddr>)>,
    req: Request,
) -> Result<HashMap<PeerId, Response>> {
    let has_tor = tor.is_some();
    let (dns_cfg, dns_opts) = dns_config();
    let mut swarm = libp2p::SwarmBuilder::with_new_identity()
        .with_tokio()
        .with_tcp(tcp::Config::default(), noise::Config::new, yamux::Config::default)?
        .with_quic()
        .with_other_transport(|key| -> Result<_, Box<dyn std::error::Error + Send + Sync>> {
            let tor = match tor {
                Some(t) => OptionalTransport::some(t),
                None => OptionalTransport::none(),
            };
            Ok(tor
                .upgrade(upgrade::Version::V1)
                .authenticate(noise::Config::new(key)?)
                .multiplex(yamux::Config::default()))
        })?
        .with_dns_config(dns_cfg, dns_opts)
        .with_behaviour(|_| {
            // Solo pide: no ofrece ningún protocolo.
            request_response::cbor::Behaviour::<Request, Response>::new(
                [(MSG_PROTOCOL, ProtocolSupport::Outbound)],
                request_response::Config::default().with_request_timeout(DEADLINE),
            )
        })?
        .with_swarm_config(|c| c.with_idle_connection_timeout(Duration::from_secs(5)))
        .with_connection_timeout(Duration::from_secs(60))
        .build();

    let mut pending: HashMap<OutboundRequestId, PeerId> = HashMap::new();
    for (peer, addrs) in targets {
        let addrs: Vec<Multiaddr> = addrs.into_iter().filter(usable).collect();
        let onion: Vec<Multiaddr> = addrs.iter().filter(|a| is_onion(a)).cloned().collect();
        let addrs = if has_tor && !onion.is_empty() { onion } else { addrs };
        if addrs.is_empty() {
            continue;
        }
        for a in addrs {
            swarm.add_peer_address(peer, a);
        }
        pending.insert(swarm.behaviour_mut().send_request(&peer, req.clone()), peer);
    }

    let mut out = HashMap::new();
    let run = async {
        while !pending.is_empty() {
            use request_response::{Event, Message};
            match swarm.select_next_some().await {
                SwarmEvent::Behaviour(Event::Message { message: Message::Response { request_id, response }, .. }) => {
                    if let Some(peer) = pending.remove(&request_id) {
                        out.insert(peer, response);
                    }
                }
                SwarmEvent::Behaviour(Event::OutboundFailure { request_id, error, peer, .. }) => {
                    debug!(%peer, "mensajero: {error}");
                    pending.remove(&request_id);
                }
                _ => {}
            }
        }
    };
    let _ = tokio::time::timeout(DEADLINE, run).await;
    Ok(out)
}
