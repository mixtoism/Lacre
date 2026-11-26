//! Historial de mensajes de cada chat (D-022).
//!
//! Un archivo CBOR con los mensajes de todos los chats, reescrito en cada
//! mensaje. Basta para una prueba de concepto; con historiales largos habrá
//! que pasar a una base de datos.

use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::Result;
use lacre_core::{cbor_from, cbor_to};
use serde::{Deserialize, Serialize};

use crate::app::{read_private, write_private};

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
    /// Aviso del chat (p. ej. «ana añadió a carl»), no un mensaje escrito.
    #[serde(default)]
    pub notice: bool,
    /// Sticker (nombre en el almacén de stickers, D-025); `text` va vacío.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sticker: Option<String>,
    /// Archivo adjunto (D-026); `text` va vacío.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<FileRef>,
}

/// Archivo de un mensaje. Los bytes se descargan aparte, cuando se piden.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct FileRef {
    pub id: String,
    pub name: String,
    pub size: u64,
}

pub struct History {
    path: PathBuf,
    chats: HashMap<Vec<u8>, Vec<StoredMessage>>,
}

impl History {
    pub fn open(path: PathBuf) -> Result<Self> {
        let chats = match read_private(&path)? {
            Some(b) => cbor_from(&b)?,
            None => HashMap::new(),
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

    /// Borra los mensajes de un chat.
    pub fn clear(&mut self, group_id: &[u8]) -> Result<()> {
        if self.chats.remove(group_id).is_some() {
            write_private(&self.path, &cbor_to(&self.chats)?)?;
        }
        Ok(())
    }

    /// Mensajes guardados de un chat.
    pub fn count(&self, group_id: &[u8]) -> usize {
        self.chats.get(group_id).map_or(0, Vec::len)
    }

    pub fn get(&self, group_id: &[u8]) -> Vec<StoredMessage> {
        self.chats.get(group_id).cloned().unwrap_or_default()
    }

    pub fn last(&self, group_id: &[u8]) -> Option<&StoredMessage> {
        self.chats.get(group_id)?.last()
    }
}
