<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="docs/brand/lacre-logo-dark.svg">
    <img src="docs/brand/lacre-logo.svg" alt="Lacre" width="320">
  </picture>
</p>

# Lacre

Peer-to-peer messaging with end-to-end encryption, multi-device support and
no intermediary servers.

> **Status: hardened, unaudited.** An internal security review has been
> applied (D-039–D-042), but the protocol is versioned (`v1`) and not frozen,
> and the code has not had an independent audit. Do not rely on it for
> anything sensitive yet. The CLI output is currently in Spanish.

Every design choice, with its rationale and open issues, is recorded in
[`docs/DECISIONES.md`](docs/DECISIONES.md) (in Spanish). The `D-xxx`
references below point to entries in that file.

---

## Contents

- [Features](#features)
- [Architecture](#architecture)
- [Protocol](#protocol)
  - [Identity and handles](#1-identity-and-handles)
  - [Identity records and the DHT](#2-identity-records-and-the-dht)
  - [Devices and linking](#3-devices-and-linking)
  - [Encryption: MLS groups](#4-encryption-mls-groups)
  - [Direct messaging protocol](#5-direct-messaging-protocol-p2mmsg1)
  - [Offline delivery: sealed envelopes and mailboxes](#6-offline-delivery-sealed-envelopes-and-mailboxes)
  - [Transport and connectivity](#7-transport-and-connectivity)
  - [Bootstrapping without servers: QR codes and peer cache](#8-bootstrapping-without-servers-qr-codes-and-peer-cache)
- [Security and privacy](#security-and-privacy)
- [Getting started](#getting-started)
- [CLI reference](#cli-reference)
- [Running an always-on node](#running-an-always-on-node)
- [Data directory](#data-directory)
- [Testing](#testing)
- [Limitations and roadmap](#limitations-and-roadmap)
- [License](#license)

---

## Features

- **Self-certifying identity.** Each person owns an Ed25519 root key. Their
  handle, `name#suffix`, has a suffix derived from that key with Argon2id, so
  anyone can check that a handle belongs to a key. No registry, no
  blockchain, no phone number.
- **QR contact exchange.** A QR payload carries the handle, the full 256-bit
  root key and the addresses where the device can be reached, so a first
  contact needs no server and no prior network.
- **Multi-device by design.** The root key signs the list of authorized
  devices. Each device has its own key that never leaves it. New devices join
  existing chats automatically and revoked devices are removed.
- **End-to-end encryption with MLS (RFC 9420).** Every device is a leaf in
  the MLS tree. One-to-one chats and group chats use the same mechanism, with
  forward secrecy and post-compromise security.
- **Decentralized discovery.** A dedicated Kademlia DHT (libp2p) stores
  signed identity records and MLS KeyPackages. mDNS finds peers on the local
  network.
- **Works across NATs and CGNAT without servers of our own.**
  - Every desktop node publishes a **Tor onion service** (embedded Arti),
    reachable behind any NAT.
  - Nodes with a public IP can act as **libp2p circuit relays**, with DCUtR
    hole punching and UPnP port mapping.
  - Relays announce themselves on the **BitTorrent Mainline DHT** and on
    public **UDP trackers**, so isolated nodes can find them.
- **Offline delivery.** Messages for offline devices are sealed, signed and
  left in **mailboxes** on other peers (the sender's own node plus up to three
  DHT neighbours). Mailbox hosts cannot read them.
- **Persistent peer cache.** Peers seen before are reconnected on restart,
  with no bootstrap server.
- **Local message history.** Up to 5,000 messages per chat are kept on disk.
- **Headless mode.** Run an always-on node (mailbox, relay, onion entry
  point) under systemd or Docker.
- **Reusable core.** All identity and crypto logic lives in a
  network-independent Rust crate, meant to be exposed to mobile and desktop
  apps via FFI (Flutter + `flutter_rust_bridge` is planned).

---

## Architecture

```
crates/lacre-core   identity, signed records, MLS, sealed envelopes (no networking; FFI-friendly)
crates/lacre-node   libp2p networking, Tor transport, rendezvous, mailboxes, chat logic, CLI
app/              Flutter app (Linux for now); app/rust is the lacre_api bridge crate
scripts/e2e.sh    end-to-end test with several local nodes
deploy/           systemd unit and Dockerfile for an always-on node
docs/             design decision log
```

| Module                     | Responsibility                                                   |
| -------------------------- | ---------------------------------------------------------------- |
| `lacre-core/identity.rs`     | Root/device keys, handles, Argon2id suffix derivation            |
| `lacre-core/record.rs`       | Signed identity records, DHT key layout                          |
| `lacre-core/mls.rs`          | MLS engine (openmls), credentials, KeyPackages                   |
| `lacre-core/envelope.rs`     | Sealed and signed envelopes for offline delivery                 |
| `lacre-node/net.rs`          | libp2p swarm: Kademlia, mDNS, identify, request-response, relay  |
| `lacre-node/tor.rs`          | libp2p `Transport` over Tor onion services (Arti)                |
| `lacre-node/rendezvous.rs`   | Relay discovery through Mainline DHT and UDP trackers            |
| `lacre-node/mailbox.rs`      | Store-and-forward mailbox for other peers' envelopes             |
| `lacre-node/peers.rs`        | On-disk peer cache                                               |
| `lacre-node/app.rs`          | Chats, sync, delivery, linking: the application logic            |
| `lacre-node/history.rs`      | Local message history                                            |
| `lacre-node/proto.rs`        | Wire messages of `/p2m/msg/1`                                    |
| `lacre-node/node.rs`         | Starts a full node; shared by the CLI and the app                |
| `app/rust/src/api/lacre.rs`  | API exposed to Dart through flutter_rust_bridge                  |

---

## Protocol

### 1. Identity and handles

*(D-002, D-003)*

- A person's identity **is their Ed25519 root public key**. Contacts, chats
  and verification are all bound to that key, never to the handle text.
- A handle has the form `name#suffix`:
  - `name`: 3–32 characters from `[a-z0-9_]`.
  - `suffix`: 8 lowercase base32 characters derived **only from the root key**:

    ```
    suffix = base32( Argon2id(root_pk,
                              salt = "p2message/handle-suffix/v1",
                              m = 64 MiB, t = 3, p = 1,
                              output = 5 bytes) )      # 40 bits
    ```

  Brute-forcing a key that matches someone's suffix costs about 2^39 Argon2id
  evaluations at 64 MiB each. Renaming yourself keeps your suffix. These
  parameters are part of protocol v1; changing them would change every
  handle.
- For stronger verification, the QR code (see §8) carries the full 32-byte
  root key and does not depend on the suffix at all.
- The DHT namespace `/p2m/v1/name/` is **reserved** for a future layer of
  short, human-friendly handles (a `name → root key` registration with proof
  of work and expiry). Existing contacts would not change.

### 2. Identity records and the DHT

*(D-004, D-005)*

The root key signs an **identity record**:

```rust
IdentityRecord {
    v: 1,                   // record version
    root_pk: [u8; 32],      // Ed25519 root public key
    name: String,           // handle name
    devices: Vec<{ device_pk: [u8; 32], label: String }>,  // max 16
    seq: u64,               // bumped on every change; highest valid seq wins
}
```

- Encoding: CBOR. Signatures use the domain separator
  `"p2message/identity-record/v1\0"`. Records are at most 8 KiB.
- Records live in a dedicated Kademlia DHT, protocol `/p2m/kad/1.0.0`,
  separate from the IPFS DHT.

| DHT key                          | Value                                                        |
| -------------------------------- | ------------------------------------------------------------ |
| `/p2m/v1/id/<suffix>`            | Signed identity record                                       |
| `/p2m/v1/kp/<device_pk base32>`  | Last-resort MLS KeyPackage of a device (≤ 4 KiB, D-017)      |
| `/p2m/v1/mbox/<device_pk>`       | No value stored; the point in key space whose closest nodes host the device's mailboxes (D-016) |
| `/p2m/v1/name/…`                 | Reserved for short handles                                   |

- **Nodes validate before storing** (`StoreInserts::FilterBoth`): the
  signature must verify, the suffix must match the root key, and the record
  must not be older than the one already held. Without this check anyone
  could overwrite another person's key with garbage.
- Clients also keep the highest `seq` they have seen, so a malicious node
  serving a stale record is partially mitigated.
- Argon2id verification is memoized per root key and runs off the network
  loop.

### 3. Devices and linking

*(D-004, D-008, D-011)*

- Each device generates its own Ed25519 key, which never leaves it. That
  key is also its libp2p identity, so the **PeerId can be derived directly
  from the identity record** and the Noise channel is authenticated with the
  same key.
- The root key lives **only on the primary device** (the one that ran
  `init`). A stolen secondary device can send messages but cannot authorize
  new devices.
- **Linking a device:**
  1. The new device starts without an identity and shows its *link code*:
     its public key and a one-time secret in base32 (`key:secret`, valid
     10 minutes), followed by `@<multiaddr>` for each address reachable from
     other networks, such as its onion service. The new device only accepts
     a `LinkGrant` that carries that secret (D-039).
  2. On the primary, `link <code> <label>` first makes sure it can reach the
     new device, then adds it to the record, bumps `seq`, signs it,
     publishes it to the DHT and sends the new device a `LinkGrant`
     containing the signed record. If the grant cannot be delivered, the
     device is removed from the record again.
     The other way round (D-037), the primary shows an *invitation*
     (`invite`: `lacre-link:<primary key>:<token>[@<multiaddr>]…`, single
     use, valid for 10 minutes) and the new device runs `join <invitation>`;
     the primary checks the token and answers with the signed record. This
     way the device without a camera (the desktop) always shows the QR.
  3. The new device adopts the identity. On their next sync, group
     committers add it to every existing chat.
- **Revoking:** `unlink <code>` removes the device from the record.
  Committers notice on the next sync and remove its leaves from every group.

### 4. Encryption: MLS groups

*(D-006, D-007, D-010, D-017)*

- Implementation: [`openmls`](https://github.com/openmls/openmls), ciphersuite
  `MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519`.
- **Each device is one leaf.** A 1:1 chat is an MLS group containing every
  device of both people; a group chat works the same way with more people.
- **Credentials** are a `MemberId` (root key + device key). The leaf
  signature key *must* equal the device key. MLS itself knows nothing about
  roots, so the application enforces the binding:
  - the committer only adds devices listed in the participant's signed
    record, and the KeyPackage must match;
  - joiners verify every member against the DHT and warn about any
    unverified member.
- **Single committer per group.** Without a server to order messages, two
  concurrent commits would fork the group. Only one device (the committer,
  the creator by default) may commit (adds, removes, epoch changes). Everyone
  else rejects commits from any other device.
  - The committer can hand the role over to another participant, and does so
    before leaving a group it manages. If the committer device is revoked,
    the lowest-keyed remaining device of the same person takes over by
    sending its signed identity record, which proves the revocation (D-049).
  - The committer syncs every tick (30 s by default) and before each send: it
    resolves the participants' records, adds new devices and removes revoked
    ones. Other members can ask for a sync with `SyncHint`.
  - If a person's record cannot be resolved, their devices are left
    untouched, so a network failure never removes anyone.
- **KeyPackages.** Freshly requested KeyPackages are preferred. Each device
  also publishes a reusable *last resort* KeyPackage to the DHT, renewed
  every 7 days. This lets the committer add a device that is **offline**; the
  Welcome reaches it later through mailboxes.
- **Ordering and out-of-epoch messages.**
  - Sends from each device are serialized, so a commit always goes out
    before messages of the new epoch.
  - A message that arrives too early (before its commit or Welcome) is
    queued (up to 128) and retried after the next commit, up to 5 times.
  - Keys of the 5 previous epochs are kept.
  - Message IDs are remembered (last 5,000) to drop duplicates.
- **Application payload** (inside MLS application messages, CBOR):
  `{ text: String, sent_at: u64 }`.
- **Welcome metadata** (`ChatMeta`): optional title, participant handles and
  the committer's device key.

### 5. Direct messaging protocol (`/p2m/msg/1`)

*(D-009)*

Devices talk to each other directly over a libp2p request-response protocol
with CBOR encoding and a 30 s timeout. Every request is acknowledged.

| Request                       | Purpose                                                              | Response                    |
| ----------------------------- | -------------------------------------------------------------------- | --------------------------- |
| `KeyPackage`                  | Ask a device for a fresh KeyPackage to add it to a group             | `KeyPackage(bytes)`         |
| `Welcome { welcome, meta }`   | MLS Welcome plus chat metadata for a new member                      | `Ack`                       |
| `Mls(bytes)`                  | MLS message (application data or commit)                             | `Ack`                       |
| `LinkGrant { record }`        | Hand a newly linked device its signed identity record                | `Ack`                       |
| `Join { token, label }`       | A new device joins with the primary's invitation (D-037)             | `Granted { record }`        |
| `SyncHint { group_id }`       | Ask a group's committer to resync devices                            | `Ack`                       |
| `Deposit(Envelope)`           | Store a sealed envelope for an offline device                        | `Ack`                       |
| `Fetch`                       | Retrieve envelopes addressed to *the caller* (authenticated by Noise) | `Envelopes { list, more }` |
| `AckEnvelopes { ids }`        | Confirm receipt; the mailbox deletes them                            | `Ack`                       |

Any request may be answered with `Error(String)`.

### 6. Offline delivery: sealed envelopes and mailboxes

*(D-016)*

When a device does not answer, the MLS message (or Welcome) is wrapped in an
**envelope**:

```rust
Envelope {
    from: [u8; 32],     // sender device key
    to: [u8; 32],       // recipient device key
    sealed: Vec<u8>,    // ephemeral X25519 key (32 bytes) ‖ ciphertext
    sig: Vec<u8>,       // Ed25519 signature by the sender device
}
```

- **Sealing:** an ephemeral X25519 key agreement against the recipient's
  Ed25519 key converted to Montgomery form, key derivation with BLAKE3
  (`"p2message/envelope/v1 seal"`), then ChaCha20-Poly1305. Low-order points
  are rejected.
- **Signing:** the signature covers the recipient and the ciphertext
  (`"p2message/envelope/v1\0"` domain). Mailboxes verify it, so they reject
  garbage, but they cannot decrypt anything, not even chat metadata.
- **Where envelopes go:** the sender's own node, plus up to 3 DHT nodes
  closest to `mailbox_key(recipient)`.
- **Retrieval:** the recipient fetches from its mailboxes when it connects to
  a peer and on every tick, acknowledges what it received and drops
  duplicates. `Fetch` uses the Noise-authenticated PeerId, so nobody can
  drain somebody else's mailbox. Replies are paged (`more` flag, ≤ 4 MiB per
  page).
- **Abuse limits per mailbox host:** 256 KiB per envelope, 500 envelopes
  per recipient, 2,000 recipients, 14-day expiry.

### 7. Transport and connectivity

*(D-009, D-018, D-019, D-020)*

**Stack:** libp2p over TCP, QUIC and Tor, secured with Noise and multiplexed
with yamux. Peers exchange addresses through identify (`/p2m/id/1`) and
Kademlia, and find each other on the LAN with mDNS. The connection timeout
is 60 s to allow for Tor circuits.

**NAT traversal:**

- **Circuit relay v2.** Every node is a relay client. Nodes started with
  `--relay-server` offer relaying (up to 512 reservations, 1 h each, 30 min
  circuits) and treat their listen addresses as external. A node reserves a
  slot on up to 2 relays it dialed directly.
- **DCUtR** tries to upgrade relayed connections to direct ones by hole
  punching. **UPnP** opens a port when the router allows it.

**Tor onion services (default on):**

- Each desktop node runs [Arti](https://gitlab.torproject.org/tpo/core/arti)
  in-process and publishes a v3 onion service. It accepts libp2p connections
  on virtual port 4001 through a custom `Transport` for `/onion3/<addr>:4001`
  multiaddrs. Noise and yamux run on top, as over TCP.
- Onion services only make outbound connections, so they are **reachable
  behind any NAT or CGNAT**. This is the main way to connect across networks.
- The onion key is derived from the device key with a separate BLAKE3
  domain (so it is **not** the device key itself, avoiding key reuse) and
  is kept only in memory: Arti runs with an ephemeral keystore, so no
  private key is written to `<data>/tor`, and the onion address is stable
  across restarts (D-044).
- Publishing usually takes 20–40 s (up to several minutes on the very first
  run, while Tor downloads its directory). Each onion connection takes a few
  seconds to open.
- Tor is used for reachability, **not anonymity**: TCP, QUIC and mDNS
  addresses are still advertised unless disabled.

**Rendezvous via BitTorrent infrastructure** (for relays with a public IP):

- Relay servers announce themselves under a fixed infohash,
  `blake3("p2message/v1 rendezvous relays")` truncated to 20 bytes:
  - on the **Mainline DHT** (BEP 5) every 15 min;
  - as seeders on public **UDP trackers** (BEP 15) as a fallback.
- A node with fewer than 3 connections queries that infohash every minute and
  dials up to 8 of the relays it finds.
- Only IP:port pairs are exchanged. Identity is checked by Noise on connect,
  so a bogus entry only costs one failed dial. Observers can tell that an IP
  uses Lacre, but not who it is.
- Bootstrap nodes and trackers can be overridden without recompiling
  (`--dht-bootstrap`, `--tracker`, `<data>/trackers.txt`). IPv4 only for now.

### 8. Bootstrapping without servers: QR codes and peer cache

*(D-021)*

**QR payload:**

```
p2m:<name>#<suffix>:<root_pk base32>[@<multiaddr>]…
```

- The addresses are full multiaddrs (`…/p2p/<peer-id>`) of the device showing
  the QR. Only addresses reachable from another network are included: onion
  services and public IPv4. Private, CGNAT and relay addresses are left out.
- When you open a chat from a QR, the node first dials those addresses (for
  up to 60 s), then resolves the handle through the DHT and checks that the
  record's root key matches the QR's key.
- Unknown address types are ignored, so future versions can add new ones.
  QR codes without `@…` are still valid.

**Peer cache** (`<data>/peers.txt`): one multiaddr per line, up to 64 peers
with up to 4 addresses each, most recent first. It is filled from identify
data of peers that speak the Lacre Kademlia protocol, using the same
filter as the QR. On startup the node dials these addresses and any
`--bootstrap` address, and keeps retrying every 30 s while it has no
connections.

As a result, **no server is needed to join the network.** The first contact
arrives by QR, and later starts use the cache. Adding someone by handle alone
(without a QR) requires already being connected to the network.

---

## Security and privacy

What is protected:

- **Message content** is end-to-end encrypted with MLS between devices.
  Relays, mailboxes, DHT nodes and Tor relays never see plaintext.
- **Identities cannot be forged.** Records are signed by the root key and
  validated by every storing node. The worst a malicious DHT node can do is
  hide a record or serve a stale one.
- **Device compromise is contained.** Secondary devices cannot add devices.
  Revoked devices are removed from groups and lose access to future epochs.

Known risks and trade-offs:

- **Metadata.** Mailbox hosts see which device writes to which, when and how
  much. Relays see which PeerIds connect and how much traffic flows.
- **Sybil/eclipse attacks** on the DHT can hide records (not forge them).
- **Key reuse.** The device key is used for Noise, MLS signatures and
  (converted to X25519) envelope sealing. Every use is domain-separated, but
  this is under review (D-008).
- **Last-resort KeyPackages** weaken forward secrecy of the first message
  until the next update.
- **Device linking** requires the one-time secret from the new device's
  link code (valid 10 minutes). Both screens then show a 6-digit
  confirmation code to compare (D-047); a mismatch means someone else linked.
- **The QR reveals addresses**: the device's onion address and, if it has
  one, its public IP.
- **Tor mode is not anonymous** (see §7).
- **Local storage.** State, media and files are encrypted at rest with a key
  from the OS keystore (Android Keystore, Secret Service, macOS Keychain,
  Windows DPAPI); the app refuses to start without it. The onion key never
  touches the disk (D-044). Restoring an old copy of a single state file
  (or deleting one) is detected and the node refuses to start; restoring the
  whole data directory to an earlier consistent copy is not (D-046).
  `--accept-current-state` accepts the disk as it is, after restoring your
  own backup.
- **No independent audit yet.** An internal security review hardened
  linking, MLS membership checks, handle pinning and resource limits
  (D-039–D-049), but nothing should be considered secure until audited.

---

## Getting started

Requirements: a recent stable Rust toolchain (edition 2024). No system Tor or
SQLite is needed; Arti is embedded and builds SQLite statically.

```sh
cargo build --release -p lacre-node
```

### Two nodes on the same machine or LAN

```sh
# terminal 1
cargo run -p lacre-node -- --data /tmp/ana --port 4001
> init ana
> whoami                       # prints ana#xxxxxxxx and the QR payload

# terminal 2
cargo run -p lacre-node -- --data /tmp/beto --port 4002 --bootstrap /ip4/127.0.0.1/tcp/4001
> init beto
> chat ana#xxxxxxxx
> send 0 hello
```

### Across different networks (via Tor)

```sh
# machine A
cargo run -p lacre-node -- --data /tmp/ana
> init ana
> whoami      # wait for the onion service to be published; the QR then includes /onion3/…

# machine B, on any other network
cargo run -p lacre-node -- --data /tmp/beto
> init beto
> chat p2m:ana#…:…@/onion3/…   # paste A's full QR payload
> send 0 hello
```

After this first contact, both nodes remember each other in `peers.txt` and
reconnect on restart.

### Linking a second device

```sh
# on the new device: start without `init`; it prints a link code
cargo run -p lacre-node -- --data /tmp/ana-laptop

# on the primary device
> link <code> laptop
```

The new device receives its identity and is added to all existing chats.

### Group chats

```sh
> group trip beto#xxxxxxxx carla#yyyyyyyy
> chats
> send 1 who brings the tent?
```

---

### Desktop app (Linux)

Requirements: Flutter (stable), GTK 3 development libraries, and the same Rust
toolchain. The Rust part is built automatically by the Flutter build.

```sh
cd app
flutter run -d linux                         # data in ~/.local/share/org.lacre.app/node
LACRE_DATA=/tmp/lacre-b flutter run -d linux     # a second, independent instance
```

The app covers creating an identity or linking to an existing one, the chat
list with network status, 1:1 and group chats, and a profile page with your QR
code, Tor/relay status and device management. Bindings are regenerated with
`flutter_rust_bridge_codegen generate` (from `app/`) after changing
`app/rust/src/api`.

## CLI reference

### Commands

| Command                       | Description                                                          |
| ----------------------------- | -------------------------------------------------------------------- |
| `init <name>`                 | Create your identity; this device becomes the primary               |
| `whoami`                      | Show your handle, QR payload and this device's code                 |
| `devices`                     | List your devices (`*` marks this one)                              |
| `link <code> [label]`         | Authorize a new device (primary only)                               |
| `invite`                      | Show an invitation for a new device to `join` (primary only)        |
| `join <invitation> [label]`   | Link this new device using the primary's invitation                 |
| `unlink <code>`               | Revoke a device (primary only)                                      |
| `whois <handle>`              | Resolve a handle on the network and list its devices                |
| `chat <handle\|qr>`           | Open a 1:1 chat                                                     |
| `group <title> <h1> <h2> …`   | Create a group chat (handles or QR payloads)                        |
| `chats`                       | List chats with their index and device count                        |
| `send <n> <text>`             | Send a message to chat `n`; reports devices reached via mailboxes   |
| `history <n>`                 | Show stored messages of chat `n`                                    |
| `dial <multiaddr>`            | Connect to a specific node                                          |
| `addrs`                       | Show the addresses others can use to reach you                      |
| `help`, `quit`                | Help / exit                                                          |

### Options

| Flag                          | Default        | Description                                                        |
| ----------------------------- | -------------- | ------------------------------------------------------------------ |
| `--data <dir>`                | `./lacre-data`   | Data directory for this device (keys and state)                    |
| `--port <n>`                  | `0` (random)   | TCP/QUIC listen port                                               |
| `--bootstrap <multiaddr>`     | —              | Node to connect to on startup (repeatable; `/onion3/…` works)      |
| `--relay-server`              | off            | Offer relaying (for nodes with a public IP or open port)           |
| `--no-tor`                    | Tor on         | Disable the onion service and `.onion` dialing                     |
| `--no-rendezvous`             | on             | Don't search Mainline DHT / trackers for relays                    |
| `--dht-bootstrap <host:port>` | built-in       | Mainline DHT bootstrap node (repeatable; replaces defaults)        |
| `--tracker <host:port\|udp://…>` | built-in    | UDP tracker (repeatable; replaces defaults and `trackers.txt`)     |
| `--headless`                  | off            | No console; run until SIGINT/SIGTERM                               |
| `--tick <secs>`               | `30`           | Republish and sync interval                                        |

Hidden testing flags: `--no-listen` (simulate being behind NAT), `--no-mdns`.
Log verbosity is controlled with `RUST_LOG` (default `warn`).

---

## Running an always-on node

An always-on node helps the network even without a public IP:

- it hosts **mailboxes** for offline contacts;
- its **onion service** is a stable entry point other nodes can use as
  `--bootstrap`;
- with a public IP and `--relay-server`, it acts as a **relay** and
  announces itself on Mainline and trackers.

**systemd** (`deploy/lacre-node.service`):

```sh
sudo useradd --system --home-dir /var/lib/lacre-node lacre
sudo install -m755 target/release/lacre-node /usr/local/bin/
sudo cp deploy/lacre-node.service /etc/systemd/system/
sudo systemctl enable --now lacre-node
journalctl -u lacre-node | grep 'escuchando en /onion3'   # your onion bootstrap address
```

Open port 4001 (TCP and UDP) if you have a public IP. Behind NAT or CGNAT,
remove `--relay-server` from the unit.

**Docker** (`deploy/Dockerfile`):

```sh
docker build -f deploy/Dockerfile -t lacre-node .
docker run -d --name lacre -p 4001:4001/tcp -p 4001:4001/udp -v lacre:/data lacre-node
```

---

## Data directory

All files are written atomically with `0600` permissions.

| File              | Contents                                               |
| ----------------- | ------------------------------------------------------ |
| `device.key`      | This device's Ed25519 secret key                       |
| `root.key`        | Root secret key (primary device only)                  |
| `identity.cbor`   | Signed identity record                                 |
| `mls.json`        | MLS state (openmls storage)                            |
| `lastresort.kp`   | Last-resort KeyPackage                                 |
| `chats.cbor`      | Chat list and metadata                                 |
| `contacts.cbor`   | Resolved identity records of contacts                  |
| `history.cbor`    | Message history (up to 5,000 per chat)                 |
| `seen.cbor`       | Recently seen message IDs (deduplication)              |
| `mailbox.cbor`    | Envelopes held for other devices                       |
| `peers.txt`       | Peer address cache                                     |
| `trackers.txt`    | Optional custom tracker list                           |
| `tor/`            | Arti state and cache, including the onion service key  |

---

## Testing

```sh
cargo test -p lacre-core     # unit tests for identity, records, MLS and envelopes
scripts/e2e.sh             # full local scenario
```

`scripts/e2e.sh` starts a relay and several users on localhost (with Tor
disabled) and covers: 1:1 chat, group chat, multi-device users, late device
linking, revocation, offline delivery through mailboxes, being added to a chat
while offline, and a node with no inbound connections reaching others through
the relay.

---

## Limitations and roadmap

Current limitations:

- Clients: CLI and a Linux desktop app; no mobile builds or push notifications yet.
- The root key exists only on the primary device and in its 24-word recovery
  phrase (D-048). Whoever has the phrase owns the identity, and a leaked
  root key cannot be rotated: the handle is derived from it.
- Only one device per group (its manager, the creator by default) can commit.
  If it is offline, membership changes wait. Management can be handed over,
  and another device of the same person takes over automatically when the
  manager device is revoked (D-049). If the manager's whole identity is lost,
  membership is frozen.
- Newly linked devices only see messages sent after they joined.
- Relay discovery is IPv4 only; a `--relay-server` behind NAT advertises an
  unreachable address (AutoNAT pending).
- History is a single CBOR file rewritten on every message.

Planned:

- Flutter builds for Android, iOS, Windows and macOS (the app and its
  `lacre_api` bridge already exist; Linux works), QR scanning with the camera.
- History transfer between own devices.
- Multiple admins and fork resolution.
- Short handles (`/p2m/v1/name/`), with proof of work and expiry.
- SQLite/SQLCipher storage and OS keystore integration.
- IPv6, AutoNAT, rate limiting of DHT writes, an optional Tor-only anonymous
  mode.

---

## License

[GPL-3.0-or-later](LICENSE). All dependencies use GPL-3.0-compatible
licenses (MIT, Apache-2.0, BSD, ISC, Zlib, Unicode, MPL-2.0).
