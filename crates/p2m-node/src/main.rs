//! Nodo de línea de comandos de p2message (prueba de concepto).

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use clap::Parser;
use libp2p::Multiaddr;
use tokio::io::{AsyncBufReadExt, BufReader};

use p2m_node::app::{App, Event, Target};
use p2m_node::node::{self, Options};
use p2m_node::rendezvous;

#[derive(Parser)]
#[command(about = "Nodo p2message: mensajería P2P cifrada de extremo a extremo")]
struct Args {
    /// Directorio de datos de este dispositivo (claves y estado).
    #[arg(long, default_value = "./p2m-data")]
    data: PathBuf,
    /// Puerto TCP/QUIC de escucha (0 = aleatorio).
    #[arg(long, default_value_t = 0)]
    port: u16,
    /// Nodo al que conectarse al arrancar (además de mDNS en la red local).
    /// Si ofrece relay, se reserva un hueco en él para ser alcanzable tras NAT.
    #[arg(long)]
    bootstrap: Vec<Multiaddr>,
    /// Ofrecer relay a otros nodos (para un nodo con IP pública o puerto abierto).
    #[arg(long)]
    relay_server: bool,
    /// No escuchar conexiones directas (simula estar tras NAT; para pruebas).
    #[arg(long, hide = true)]
    no_listen: bool,
    /// No buscar peers en la red local (para pruebas).
    #[arg(long, hide = true)]
    no_mdns: bool,
    /// No usar Tor: sin servicio onion ni llamadas a direcciones `.onion`.
    #[arg(long)]
    no_tor: bool,
    /// No usar la DHT Mainline ni trackers para encontrar relays en otras redes.
    #[arg(long)]
    no_rendezvous: bool,
    /// Nodo de arranque de la DHT Mainline (`host:puerto`); sustituye a los de serie.
    #[arg(long)]
    dht_bootstrap: Vec<String>,
    /// Tracker UDP de respaldo (`host:puerto` o `udp://…`); sustituye a la
    /// lista de serie y a `trackers.txt` del directorio de datos.
    #[arg(long)]
    tracker: Vec<String>,
    /// Sin consola: no lee comandos y sigue hasta recibir SIGINT/SIGTERM
    /// (para servicios systemd o contenedores).
    #[arg(long)]
    headless: bool,
    /// Intervalo de republicación y sincronización, en segundos.
    #[arg(long, default_value_t = 30)]
    tick: u64,
}

const HELP: &str = "\
comandos:
  init <nombre>                crea tu identidad (este dispositivo será el primario)
  whoami                       muestra tu handle, el QR y el código de este dispositivo
  devices                      lista tus dispositivos
  link <código> [etiqueta]     vincula un dispositivo nuevo (solo en el primario)
  unlink <código>              revoca un dispositivo (solo en el primario)
  whois <handle>               resuelve un handle en la red
  chat <handle|qr>             abre un chat 1:1
  group <título> <h1> <h2>...  crea un grupo
  chats                        lista tus chats
  send <n> <texto>             envía un mensaje al chat n
  history <n>                  muestra los mensajes del chat n
  dial <multiaddr>             conecta con un nodo concreto
  addrs                        muestra las direcciones por las que te pueden llamar
  help | quit";

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "warn".into()),
        )
        .with_writer(std::io::stderr)
        .init();

    let args = Args::parse();
    let opts = Options {
        port: args.port,
        listen: !args.no_listen,
        relay_server: args.relay_server,
        mdns: !args.no_mdns,
        tor: !args.no_tor,
        rendezvous: !args.no_rendezvous,
        bootstrap: args.bootstrap.clone(),
        dht_bootstrap: args.dht_bootstrap.clone(),
        trackers: (!args.tracker.is_empty())
            .then(|| args.tracker.iter().map(|t| rendezvous::normalize_tracker(t)).collect()),
        tick: Duration::from_secs(args.tick),
        ..Options::new(args.data.clone())
    };
    let node::Node { app, peer_id, mut events } = node::start(opts)?;

    println!("p2message · dispositivo {peer_id}");
    match app.handle() {
        Some(h) => println!("identidad: {h}"),
        None => println!(
            "sin identidad. Usa `init <nombre>`, o vincula este dispositivo desde el primario con:\n  link {}",
            app.device_code()
        ),
    }
    if args.relay_server {
        println!("ofreciendo relay a otros nodos");
    }
    println!("escribe `help` para ver los comandos");

    tokio::spawn(async move {
        while let Some(ev) = events.recv().await {
            print_event(ev);
        }
    });

    if args.headless {
        wait_for_shutdown().await?;
        return Ok(());
    }
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    while let Some(line) = lines.next_line().await? {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if line == "quit" || line == "exit" {
            break;
        }
        if let Err(e) = run_command(&app, line).await {
            println!("error: {e:#}");
        }
    }
    Ok(())
}

fn print_event(ev: Event) {
    match ev {
        Event::Linked { handle } => println!("\n✔ dispositivo vinculado a {handle}"),
        Event::NewChat { index, name } => println!("\n✉ nuevo chat [{index}] {name}"),
        Event::Message { local: true, .. } => {}
        Event::Message { index, message, .. } => println!("\n[{index}] {}: {}", message.from, message.text),
        Event::Removed { .. } => println!("\n⚠ este dispositivo ha sido expulsado de un chat"),
        Event::UnverifiedMember { name, .. } => println!("\n⚠ miembro no verificado en el chat: {name}"),
        Event::RelayReady(addr) => println!("\n✔ alcanzable a través de relay: {addr}"),
    }
}

async fn wait_for_shutdown() -> Result<()> {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        let mut term = signal(SignalKind::terminate())?;
        tokio::select! {
            r = tokio::signal::ctrl_c() => r?,
            _ = term.recv() => {}
        }
    }
    #[cfg(not(unix))]
    tokio::signal::ctrl_c().await?;
    Ok(())
}

async fn run_command(app: &Arc<App>, line: &str) -> Result<()> {
    let (cmd, rest) = line.split_once(' ').unwrap_or((line, ""));
    let rest = rest.trim();
    match cmd {
        "help" => println!("{HELP}"),
        "init" => {
            let h = app.init(rest).await?;
            println!("identidad creada: {h}");
        }
        "whoami" => {
            match app.handle() {
                Some(h) => println!("handle: {h}"),
                None => println!("sin identidad"),
            }
            if let Some(qr) = app.qr_payload().await {
                println!("qr:     {qr}");
            }
            println!("código de dispositivo: {}", app.device_code());
        }
        "devices" => {
            for (code, label, this) in app.devices() {
                println!("  {}{label}: {code}", if this { "* " } else { "  " });
            }
        }
        "link" => {
            let (code, label) = rest.split_once(' ').unwrap_or((rest, "dispositivo"));
            app.link(code, label.trim()).await?;
            println!("dispositivo «{}» vinculado", label.trim());
        }
        "unlink" => {
            app.unlink(rest).await?;
            println!("dispositivo revocado");
        }
        "whois" => {
            let rec = app.resolve(&rest.parse()?).await?;
            println!("{} · {} dispositivo(s) · versión {}", rec.handle, rec.record.devices.len(), rec.record.seq);
            for d in &rec.record.devices {
                println!("  - {}", d.label);
            }
        }
        "chat" => {
            let target: Target = rest.parse()?;
            let i = app.create_chat(None, vec![target]).await?;
            println!("chat [{i}] creado");
        }
        "group" => {
            let mut parts = rest.split_whitespace();
            let title = parts.next().context("falta el título")?.to_owned();
            let targets = parts.map(str::parse).collect::<Result<Vec<Target>>>()?;
            if targets.is_empty() {
                anyhow::bail!("indica al menos un participante");
            }
            let i = app.create_chat(Some(title), targets).await?;
            println!("grupo [{i}] creado");
        }
        "history" => {
            for m in app.messages(rest.parse()?)? {
                println!("  {}: {}", m.from, m.text);
            }
        }
        "chats" => {
            for c in app.chats() {
                let state = if c.active { "" } else { " (expulsado)" };
                println!("  [{}] {} · {} dispositivo(s){state}", c.index, c.name, c.devices);
            }
        }
        "send" => {
            let (n, text) = rest.split_once(' ').context("uso: send <n> <texto>")?;
            let r = app.send(n.parse()?, text).await?;
            if r.stored > 0 {
                println!("  ({} dispositivo(s) desconectado(s): guardado en buzones)", r.stored);
            }
            if r.failed > 0 {
                println!("  (no entregado a {} dispositivo(s))", r.failed);
            }
        }
        "dial" => app.net().dial(rest.parse()?).await,
        "addrs" => {
            for a in app.net().listen_addrs().await? {
                println!("  {a}");
            }
        }
        _ => println!("comando desconocido; escribe `help`"),
    }
    Ok(())
}
