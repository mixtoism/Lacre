//! Nodo p2message: red libp2p, buzones, Tor y lógica de chats. Lo usan la
//! CLI (`main.rs`) y la app (`p2m-api`).

pub mod app;
pub mod history;
pub mod mailbox;
pub mod net;
pub mod node;
pub mod peers;
pub mod proto;
pub mod rendezvous;
pub mod tor;
