//! Historial de mensajes de cada chat (D-022).
//!
//! Un archivo CBOR con los mensajes de todos los chats, reescrito en cada
//! mensaje. Basta para una prueba de concepto; con historiales largos habrá
//! que pasar a una base de datos.

use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::Result;
use p2m_core::{cbor_from, cbor_to};
use serde::{Deserialize, Serialize};

use crate::app::write_private;

/// Mensajes que se conservan por chat (se descartan los más antiguos).
const MAX_PER_CHAT: usize = 5000;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct StoredMessage {
    /// Nombre para mostrar del remitente (handle y dispositivo).
    pub from: String,
    pub text: String,
    /// Hora de envío según el remitente (segundos Unix).
    pub sent_at: u64,
    /// Enviado por esta persona (desde este u otro de sus dispositivos).
    pub mine: bool,
}

pub struct History {
    path: PathBuf,
    chats: HashMap<Vec<u8>, Vec<StoredMessage>>,
}

impl History {
    pub fn open(path: PathBuf) -> Result<Self> {
        let chats = match std::fs::read(&path) {
            Ok(b) => cbor_from(&b)?,
            Err(_) => HashMap::new(),
        };
        Ok(Self { path, chats })
    }

    pub fn add(&mut self, group_id: &[u8], msg: StoredMessage) -> Result<()> {
        let list = self.chats.entry(group_id.to_vec()).or_default();
        list.push(msg);
        if list.len() > MAX_PER_CHAT {
            list.drain(..list.len() - MAX_PER_CHAT);
        }
        write_private(&self.path, &cbor_to(&self.chats)?)
    }

    pub fn get(&self, group_id: &[u8]) -> Vec<StoredMessage> {
        self.chats.get(group_id).cloned().unwrap_or_default()
    }

    pub fn last(&self, group_id: &[u8]) -> Option<&StoredMessage> {
        self.chats.get(group_id)?.last()
    }
}
