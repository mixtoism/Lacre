//! Nodo Lacre: red libp2p, buzones, Tor y lógica de chats. Lo usan la
//! CLI (`main.rs`) y la app (`lacre-api`).

pub mod app;
pub mod courier;
pub mod depot;
pub mod files;
pub mod history;
pub mod mailbox;
pub mod net;
pub mod node;
pub mod peers;
pub mod people;
pub mod proto;
pub mod rendezvous;
pub mod sticker;
pub mod tor;
