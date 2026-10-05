//! Capa de red: libp2p con Kademlia (registros de identidad y KeyPackages),
//! mDNS (red local), identify, request-response (mensajes directos entre
//! dispositivos) y, para atravesar NAT, circuit relay v2 + DCUtR + UPnP y
//! servicios onion de Tor.
//!
//! El swarm vive en su propia tarea; el resto del programa habla con él a
//! través de [`Client`] y recibe sus avisos como [`NetEvent`].

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Result, anyhow};
use futures::StreamExt;
use libp2p::core::transport::OptionalTransport;
use libp2p::core::{ConnectedPoint, Transport, upgrade};
use libp2p::kad::{self, Mode, QueryId, Quorum, Record, RecordKey, StoreInserts, store::RecordStore};
use libp2p::multiaddr::Protocol;
use libp2p::request_response::{self, OutboundRequestId, ProtocolSupport, ResponseChannel};
use libp2p::swarm::{NetworkBehaviour, SwarmEvent, behaviour::toggle::Toggle};
use libp2p::{
    Multiaddr, PeerId, StreamProtocol, Swarm, dcutr, identify, identity, mdns, noise, relay, tcp, upnp, yamux,
};
use p2m_core::mls::verify_key_package;
use p2m_core::record::{SignedRecord, dht_key, key_package_key};
use tokio::sync::{mpsc, oneshot};
use tracing::{debug, info, warn};

use crate::peers::PeerCache;
use crate::proto::{Request, Response};
use crate::tor::TorTransport;

const KAD_PROTOCOL: StreamProtocol = StreamProtocol::new("/p2m/kad/1.0.0");
const MSG_PROTOCOL: StreamProtocol = StreamProtocol::new("/p2m/msg/1");
/// Reservas simultáneas en relays (D-018).
const MAX_RELAYS: usize = 2;
/// Tras encontrar el primer valor de una clave, cuánto más se espera por
/// otras copias (quizá con `seq` mayor) antes de cerrar la búsqueda. Sin esto
/// la búsqueda dura hasta recorrer todos los nodos cercanos o agotar el plazo.
const RECORD_GRACE: Duration = Duration::from_millis(1500);
/// Sin ninguna conexión, cada cuánto se vuelve a llamar a los nodos de
/// arranque. Al arrancar Tor aún no está listo y el primer intento por onion
/// suele fallar.
const REDIAL_EVERY: Duration = Duration::from_secs(30);

#[derive(NetworkBehaviour)]
struct Behaviour {
    kad: kad::Behaviour<kad::store::MemoryStore>,
    mdns: Toggle<mdns::tokio::Behaviour>,
    identify: identify::Behaviour,
    rr: request_response::cbor::Behaviour<Request, Response>,
    relay_client: relay::client::Behaviour,
    relay_server: Toggle<relay::Behaviour>,
    dcutr: dcutr::Behaviour,
    upnp: upnp::tokio::Behaviour,
}

pub struct NetConfig {
    /// Puerto TCP/QUIC (0 = aleatorio).
    pub port: u16,
    /// Escuchar conexiones directas. `false` simula un nodo tras NAT: solo
    /// es alcanzable a través de un relay.
    pub listen: bool,
    /// Ofrecer relay a otros nodos (pensado para nodos con IP pública).
    pub relay_server: bool,
    /// Descubrir peers en la red local.
    pub mdns: bool,
    /// Directorio para Tor; `None` = sin Tor (D-020).
    pub tor: Option<PathBuf>,
    /// Nodos a los que llamar al arrancar (y mientras no haya conexiones).
    pub bootstrap: Vec<Multiaddr>,
    /// Archivo de la caché de peers (D-021).
    pub peer_cache: Option<PathBuf>,
}

enum Command {
    PutRecord { key: Vec<u8>, value: Vec<u8>, reply: oneshot::Sender<Result<()>> },
    GetRecords { key: Vec<u8>, reply: oneshot::Sender<Vec<Vec<u8>>> },
    Request { peer: PeerId, req: Request, reply: oneshot::Sender<Result<Response>> },
    Respond { channel: ResponseChannel<Response>, resp: Response },
    ClosestPeers { key: Vec<u8>, reply: oneshot::Sender<Vec<PeerId>> },
    ConnectedPeers { reply: oneshot::Sender<Vec<PeerId>> },
    ListenAddrs { reply: oneshot::Sender<Vec<Multiaddr>> },
    Dial(Multiaddr),
    /// Registro entrante ya validado (la validación usa Argon2 y va aparte).
    StoreValidated(Record),
    /// Cierra una búsqueda que ya encontró algo (ver `RECORD_GRACE`).
    FinishQuery(QueryId),
}

/// Petición entrante de otro dispositivo; hay que responder con [`Client::respond`].
pub struct Inbound {
    pub peer: PeerId,
    pub req: Request,
    pub channel: ResponseChannel<Response>,
}

pub enum NetEvent {
    Request(Inbound),
    /// Nueva conexión con un peer (buen momento para vaciar su buzón).
    Connected(PeerId),
    /// Somos alcanzables a través de un relay en esta dirección.
    RelayReady(Multiaddr),
}

#[derive(Clone)]
pub struct Client {
    tx: mpsc::Sender<Command>,
}

impl Client {
    async fn call<T>(&self, make: impl FnOnce(oneshot::Sender<T>) -> Command) -> Result<T> {
        let (reply, rx) = oneshot::channel();
        self.tx.send(make(reply)).await.map_err(|_| anyhow!("red detenida"))?;
        rx.await.map_err(|_| anyhow!("red detenida"))
    }

    pub async fn put_record(&self, key: Vec<u8>, value: Vec<u8>) -> Result<()> {
        self.call(|reply| Command::PutRecord { key, value, reply }).await?
    }

    /// Todos los valores encontrados para la clave (sin validar).
    pub async fn get_records(&self, key: Vec<u8>) -> Result<Vec<Vec<u8>>> {
        self.call(|reply| Command::GetRecords { key, reply }).await
    }

    /// Envía una petición directa. Si no sabemos llegar al peer, lo busca
    /// en la DHT y reintenta una vez.
    pub async fn request(&self, peer: PeerId, req: Request) -> Result<Response> {
        let first = self
            .call(|reply| Command::Request { peer, req: req.clone(), reply })
            .await?;
        match first {
            Ok(resp) => Ok(resp),
            Err(e) => {
                debug!(%peer, "petición fallida ({e}); buscando en la DHT");
                self.closest_peers(peer.to_bytes()).await?;
                self.call(|reply| Command::Request { peer, req, reply }).await?
            }
        }
    }

    /// Peers más cercanos a una clave en la DHT (sin incluirnos).
    pub async fn closest_peers(&self, key: Vec<u8>) -> Result<Vec<PeerId>> {
        self.call(|reply| Command::ClosestPeers { key, reply }).await
    }

    pub async fn connected_peers(&self) -> Result<Vec<PeerId>> {
        self.call(|reply| Command::ConnectedPeers { reply }).await
    }

    /// Direcciones de escucha y externas confirmadas, con nuestro `/p2p/`.
    pub async fn listen_addrs(&self) -> Result<Vec<Multiaddr>> {
        self.call(|reply| Command::ListenAddrs { reply }).await
    }

    pub async fn respond(&self, channel: ResponseChannel<Response>, resp: Response) {
        let _ = self.tx.send(Command::Respond { channel, resp }).await;
    }

    pub async fn dial(&self, addr: Multiaddr) {
        let _ = self.tx.send(Command::Dial(addr)).await;
    }
}

pub fn peer_id_of(device_pk: &[u8; 32]) -> Result<PeerId> {
    let pk = identity::ed25519::PublicKey::try_from_bytes(device_pk)?;
    Ok(identity::PublicKey::from(pk).to_peer_id())
}

/// Clave de dispositivo a partir del `PeerId` (autenticado por Noise).
pub fn device_pk_of(peer: &PeerId) -> Option<[u8; 32]> {
    let pk = identity::PublicKey::try_decode_protobuf(peer.as_ref().digest()).ok()?;
    Some(pk.try_into_ed25519().ok()?.to_bytes())
}

fn relay_server_config() -> relay::Config {
    // Los valores por defecto (2 min, 128 KiB por circuito) están pensados
    // solo para coordinar el hole punching. Nuestros relays también llevan
    // tráfico cuando DCUtR falla (NAT simétrico), así que son más generosos.
    relay::Config {
        max_reservations: 512,
        max_reservations_per_peer: 4,
        reservation_duration: Duration::from_secs(60 * 60),
        max_circuits: 256,
        max_circuits_per_peer: 16,
        max_circuit_duration: Duration::from_secs(30 * 60),
        max_circuit_bytes: 64 << 20,
        ..Default::default()
    }
}

/// Arranca la red. La identidad libp2p del nodo es la clave del dispositivo,
/// así que `PeerId` == dispositivo (D-008).
pub fn spawn(device_secret: [u8; 32], cfg: NetConfig) -> Result<(Client, mpsc::Receiver<NetEvent>, PeerId)> {
    let keypair = identity::Keypair::ed25519_from_bytes(device_secret)?;
    let tor = cfg.tor.as_deref().map(TorTransport::new).transpose()?;
    let onion = tor.as_ref().map(TorTransport::address);
    let mut swarm = libp2p::SwarmBuilder::with_existing_identity(keypair)
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
        .with_dns()?
        .with_relay_client(noise::Config::new, yamux::Config::default)?
        .with_behaviour(|key, relay_client| {
            let peer_id = key.public().to_peer_id();
            let mut kad_cfg = kad::Config::new(KAD_PROTOCOL);
            // Los registros entrantes se validan antes de guardarse.
            kad_cfg.set_record_filtering(StoreInserts::FilterBoth);
            kad_cfg.set_query_timeout(Duration::from_secs(20));
            let mut kad = kad::Behaviour::with_config(
                peer_id,
                kad::store::MemoryStore::new(peer_id),
                kad_cfg,
            );
            // Sin direcciones públicas Kademlia se pondría en modo cliente y
            // nadie nos guardaría en su tabla; en esta PoC todos son servidores.
            kad.set_mode(Some(Mode::Server));
            Ok(Behaviour {
                kad,
                mdns: match cfg.mdns {
                    true => Some(mdns::tokio::Behaviour::new(mdns::Config::default(), peer_id)?),
                    false => None,
                }
                .into(),
                identify: identify::Behaviour::new(identify::Config::new(
                    "/p2m/id/1".into(),
                    key.public(),
                )),
                rr: request_response::cbor::Behaviour::new(
                    [(MSG_PROTOCOL, ProtocolSupport::Full)],
                    request_response::Config::default()
                        .with_request_timeout(Duration::from_secs(30)),
                ),
                relay_client,
                relay_server: cfg
                    .relay_server
                    .then(|| relay::Behaviour::new(peer_id, relay_server_config()))
                    .into(),
                dcutr: dcutr::Behaviour::new(peer_id),
                upnp: upnp::tokio::Behaviour::default(),
            })
        })?
        .with_swarm_config(|c| c.with_idle_connection_timeout(Duration::from_secs(120)))
        // Abrir un circuito hasta un servicio onion lleva bastante más que los
        // 10 s de serie.
        .with_connection_timeout(Duration::from_secs(60))
        .build();

    if cfg.listen {
        swarm.listen_on(format!("/ip4/0.0.0.0/tcp/{}", cfg.port).parse()?)?;
        swarm.listen_on(format!("/ip4/0.0.0.0/udp/{}/quic-v1", cfg.port).parse()?)?;
    }
    // El servicio onion funciona también tras NAT, así que no depende de `listen`.
    if let Some(addr) = onion {
        swarm.listen_on(addr)?;
    }

    let cache = cfg.peer_cache.map(PeerCache::open);
    let mut bootstrap = cfg.bootstrap;
    bootstrap.extend(cache.iter().flat_map(PeerCache::addrs));

    let peer_id = *swarm.local_peer_id();
    let (tx, rx) = mpsc::channel(64);
    let (ev_tx, ev_rx) = mpsc::channel(64);
    let net = Network {
        swarm,
        self_tx: tx.clone(),
        events: ev_tx,
        puts: HashMap::new(),
        gets: HashMap::new(),
        requests: HashMap::new(),
        closest: HashMap::new(),
        dialed: HashMap::new(),
        relays: HashSet::new(),
        bootstrap,
        cache,
        relay_server: cfg.relay_server,
    };
    tokio::spawn(net.run(rx));
    Ok((Client { tx }, ev_rx, peer_id))
}

struct Network {
    swarm: Swarm<Behaviour>,
    self_tx: mpsc::Sender<Command>,
    events: mpsc::Sender<NetEvent>,
    puts: HashMap<QueryId, oneshot::Sender<Result<()>>>,
    gets: HashMap<QueryId, (Vec<Vec<u8>>, oneshot::Sender<Vec<Vec<u8>>>)>,
    requests: HashMap<OutboundRequestId, oneshot::Sender<Result<Response>>>,
    closest: HashMap<QueryId, oneshot::Sender<Vec<PeerId>>>,
    /// Dirección directa por la que marcamos a cada peer (para reservar
    /// relay a través de ella).
    dialed: HashMap<PeerId, Multiaddr>,
    /// Relays en los que tenemos (o hemos pedido) reserva.
    relays: HashSet<PeerId>,
    bootstrap: Vec<Multiaddr>,
    cache: Option<PeerCache>,
    relay_server: bool,
}

/// No toma `&self`: el swarm no es `Sync` y no puede quedar prestado
/// durante la espera.
async fn emit(events: &mpsc::Sender<NetEvent>, ev: NetEvent) {
    let _ = events.send(ev).await;
}

fn is_relayed(addr: &Multiaddr) -> bool {
    addr.iter().any(|p| p == Protocol::P2pCircuit)
}

impl Network {
    async fn run(mut self, mut commands: mpsc::Receiver<Command>) {
        let mut redial = tokio::time::interval(REDIAL_EVERY);
        loop {
            tokio::select! {
                _ = redial.tick() => {
                    if self.swarm.connected_peers().next().is_none() {
                        for addr in self.bootstrap.clone() {
                            self.dial(addr);
                        }
                    }
                }
                ev = self.swarm.select_next_some() => self.on_event(ev).await,
                cmd = commands.recv() => match cmd {
                    Some(cmd) => self.on_command(cmd),
                    None => return,
                },
            }
        }
    }

    fn dial(&mut self, addr: Multiaddr) {
        if let Err(e) = self.swarm.dial(addr.clone()) {
            debug!(%addr, "no se pudo marcar: {e}");
        }
    }

    fn kad(&mut self) -> &mut kad::Behaviour<kad::store::MemoryStore> {
        &mut self.swarm.behaviour_mut().kad
    }

    fn on_command(&mut self, cmd: Command) {
        match cmd {
            Command::PutRecord { key, value, reply } => {
                let record = Record::new(RecordKey::new(&key), value);
                match self.kad().put_record(record, Quorum::One) {
                    Ok(qid) => {
                        self.puts.insert(qid, reply);
                    }
                    Err(e) => {
                        let _ = reply.send(Err(anyhow!("almacén local: {e:?}")));
                    }
                }
            }
            Command::GetRecords { key, reply } => {
                let qid = self.kad().get_record(RecordKey::new(&key));
                self.gets.insert(qid, (Vec::new(), reply));
            }
            Command::Request { peer, req, reply } => {
                let id = self.swarm.behaviour_mut().rr.send_request(&peer, req);
                self.requests.insert(id, reply);
            }
            Command::Respond { channel, resp } => {
                let _ = self.swarm.behaviour_mut().rr.send_response(channel, resp);
            }
            Command::ClosestPeers { key, reply } => {
                let qid = self.kad().get_closest_peers(key);
                self.closest.insert(qid, reply);
            }
            Command::ConnectedPeers { reply } => {
                let _ = reply.send(self.swarm.connected_peers().copied().collect());
            }
            Command::ListenAddrs { reply } => {
                let me = *self.swarm.local_peer_id();
                let addrs = self
                    .swarm
                    .listeners()
                    .chain(self.swarm.external_addresses())
                    .map(|a| if is_relayed(a) { a.clone() } else { a.clone().with(Protocol::P2p(me)) })
                    .collect();
                let _ = reply.send(addrs);
            }
            Command::Dial(addr) => self.dial(addr),
            Command::StoreValidated(record) => self.store_if_newer(record),
            Command::FinishQuery(id) => {
                if let Some(mut q) = self.kad().query_mut(&id) {
                    q.finish();
                }
            }
        }
    }

    /// Guarda un registro validado salvo que ya tengamos uno de identidad
    /// con mayor `seq`. Los KeyPackages se reemplazan siempre.
    fn store_if_newer(&mut self, record: Record) {
        let seq = |bytes: &[u8]| {
            SignedRecord::from_bytes(bytes)
                .ok()
                .and_then(|s| s.verify().ok())
                .map(|v| v.record.seq)
        };
        let store = self.kad().store_mut();
        if let Some(existing) = store.get(&record.key)
            && seq(&record.value).is_some()
            && seq(&existing.value) >= seq(&record.value)
        {
            return;
        }
        if let Err(e) = store.put(record) {
            warn!("no se pudo guardar el registro: {e:?}");
        }
    }

    async fn on_event(&mut self, ev: SwarmEvent<BehaviourEvent>) {
        match ev {
            SwarmEvent::NewListenAddr { address, .. } => {
                if is_relayed(&address) {
                    info!("escuchando en {address}");
                } else {
                    // Con `/p2p/`, lista para usar como `--bootstrap` de otro nodo.
                    info!("escuchando en {}", address.clone().with(Protocol::P2p(*self.swarm.local_peer_id())));
                }
                if is_relayed(&address) {
                    emit(&self.events, NetEvent::RelayReady(address)).await;
                } else if self.relay_server && !matches!(address.iter().next(), Some(Protocol::Onion3(_))) {
                    // El relay de libp2p solo se ofrece con alguna dirección
                    // externa confirmada. Sin AutoNAT nadie la confirma: un
                    // `--relay-server` se supone con IP pública y da por
                    // buenas las suyas (pendiente: AutoNAT, D-019).
                    self.swarm.add_external_address(address);
                }
            }
            SwarmEvent::ConnectionEstablished { peer_id, endpoint, num_established, .. } => {
                if let ConnectedPoint::Dialer { address, .. } = &endpoint
                    && !is_relayed(address)
                {
                    self.dialed.insert(peer_id, address.clone());
                    // Sin esperar a identify: si es el único peer (un QR, un
                    // nodo de arranque), la primera búsqueda ya puede usarlo.
                    self.kad().add_address(&peer_id, address.clone());
                }
                if num_established.get() == 1 {
                    debug!(%peer_id, relayed = endpoint.is_relayed(), "conectado");
                    emit(&self.events, NetEvent::Connected(peer_id)).await;
                }
            }
            SwarmEvent::OutgoingConnectionError { peer_id, error, .. } => {
                debug!(?peer_id, "llamada fallida: {error}");
            }
            SwarmEvent::ListenerClosed { addresses, .. } => {
                // Si se cae la reserva en un relay, se podrá pedir otra.
                for a in addresses.iter().filter(|a| is_relayed(a)) {
                    if let Some(Protocol::P2p(relay)) = a.iter().find(|p| matches!(p, Protocol::P2p(_))) {
                        self.relays.remove(&relay);
                    }
                }
            }
            SwarmEvent::Behaviour(BehaviourEvent::Mdns(mdns::Event::Discovered(list))) => {
                for (peer, addr) in list {
                    debug!(%peer, %addr, "descubierto por mDNS");
                    self.kad().add_address(&peer, addr);
                }
            }
            SwarmEvent::Behaviour(BehaviourEvent::Identify(identify::Event::Received {
                peer_id,
                info,
                ..
            })) => {
                debug!(%peer_id, protocols = ?info.protocols, "identify");
                if info.protocols.contains(&KAD_PROTOCOL) {
                    for addr in &info.listen_addrs {
                        self.kad().add_address(&peer_id, addr.clone());
                    }
                    if let Some(cache) = &mut self.cache {
                        cache.update(peer_id, &info.listen_addrs);
                    }
                }
                if info.protocols.contains(&relay::HOP_PROTOCOL_NAME) {
                    self.reserve_relay(peer_id);
                }
            }
            SwarmEvent::Behaviour(BehaviourEvent::RelayClient(ev)) => debug!("relay: {ev:?}"),
            SwarmEvent::Behaviour(BehaviourEvent::Dcutr(ev)) => match ev.result {
                Ok(_) => info!(peer = %ev.remote_peer_id, "conexión directa tras hole punching"),
                Err(e) => debug!(peer = %ev.remote_peer_id, "hole punching fallido: {e}"),
            },
            SwarmEvent::Behaviour(BehaviourEvent::Upnp(ev)) => debug!("upnp: {ev:?}"),
            SwarmEvent::Behaviour(BehaviourEvent::Kad(ev)) => self.on_kad(ev),
            SwarmEvent::Behaviour(BehaviourEvent::Rr(ev)) => self.on_rr(ev).await,
            _ => {}
        }
    }

    /// Reserva un hueco en un relay para que nos puedan llamar aunque
    /// estemos tras NAT (D-018). Solo a través de una conexión directa.
    fn reserve_relay(&mut self, relay: PeerId) {
        if self.relays.len() >= MAX_RELAYS || self.relays.contains(&relay) {
            return;
        }
        let Some(addr) = self.dialed.get(&relay).cloned() else { return };
        let circuit = addr.with(Protocol::P2p(relay)).with(Protocol::P2pCircuit);
        match self.swarm.listen_on(circuit.clone()) {
            Ok(_) => {
                debug!(%circuit, "pidiendo reserva en relay");
                self.relays.insert(relay);
            }
            Err(e) => warn!(%circuit, "no se pudo pedir reserva: {e}"),
        }
    }

    fn on_kad(&mut self, ev: kad::Event) {
        match ev {
            kad::Event::InboundRequest {
                request: kad::InboundRequest::PutRecord { record: Some(record), source, .. },
            } => {
                // Validar cuesta un Argon2 (memorizado por clave raíz): fuera
                // del bucle del swarm.
                let tx = self.self_tx.clone();
                tokio::task::spawn_blocking(move || {
                    if is_valid_record(&record) {
                        let _ = tx.blocking_send(Command::StoreValidated(record));
                    } else {
                        debug!(%source, "registro DHT inválido descartado");
                    }
                });
            }
            kad::Event::OutboundQueryProgressed { id, result, step, .. } => match result {
                kad::QueryResult::PutRecord(res) => {
                    if let Some(reply) = self.puts.remove(&id) {
                        let _ = reply.send(res.map(|_| ()).map_err(|e| anyhow!("{e:?}")));
                    }
                }
                kad::QueryResult::GetRecord(res) => {
                    let done = match res {
                        Ok(kad::GetRecordOk::FoundRecord(r)) => {
                            if let Some((found, _)) = self.gets.get_mut(&id) {
                                if found.is_empty() {
                                    let tx = self.self_tx.clone();
                                    tokio::spawn(async move {
                                        tokio::time::sleep(RECORD_GRACE).await;
                                        let _ = tx.send(Command::FinishQuery(id)).await;
                                    });
                                }
                                found.push(r.record.value);
                            }
                            step.last
                        }
                        _ => true,
                    };
                    if done && let Some((found, reply)) = self.gets.remove(&id) {
                        let _ = reply.send(found);
                    }
                }
                kad::QueryResult::GetClosestPeers(res) => {
                    let peers = match res {
                        Ok(ok) => ok.peers,
                        Err(kad::GetClosestPeersError::Timeout { peers, .. }) => peers,
                    };
                    for p in &peers {
                        for addr in &p.addrs {
                            self.swarm.add_peer_address(p.peer_id, addr.clone());
                        }
                    }
                    if step.last
                        && let Some(reply) = self.closest.remove(&id)
                    {
                        let _ = reply.send(peers.into_iter().map(|p| p.peer_id).collect());
                    }
                }
                _ => {}
            },
            _ => {}
        }
    }

    async fn on_rr(&mut self, ev: request_response::Event<Request, Response>) {
        use request_response::{Event, Message};
        match ev {
            Event::Message { peer, message: Message::Request { request, channel, .. }, .. } => {
                emit(&self.events, NetEvent::Request(Inbound { peer, req: request, channel })).await;
            }
            Event::Message { message: Message::Response { request_id, response }, .. } => {
                if let Some(reply) = self.requests.remove(&request_id) {
                    let _ = reply.send(Ok(response));
                }
            }
            Event::OutboundFailure { request_id, error, .. } => {
                if let Some(reply) = self.requests.remove(&request_id) {
                    let _ = reply.send(Err(anyhow!("{error}")));
                }
            }
            _ => {}
        }
    }
}

/// Solo se guardan registros de identidad bien firmados bajo su sufijo, y
/// KeyPackages válidos bajo la clave de su dispositivo.
fn is_valid_record(record: &Record) -> bool {
    if let Ok(signed) = SignedRecord::from_bytes(&record.value) {
        return signed
            .verify()
            .is_ok_and(|v| record.key == RecordKey::new(&dht_key(&v.handle.suffix)));
    }
    if record.value.len() <= crate::proto::MAX_KEY_PACKAGE {
        return verify_key_package(&record.value)
            .is_ok_and(|(kp, id)| {
                kp.last_resort() && record.key == RecordKey::new(&key_package_key(&id.device_pk))
            });
    }
    false
}
