//! Mensajes del protocolo directo entre dispositivos (`/p2m/msg/1`).

use p2m_core::envelope::Envelope;
use serde::{Deserialize, Serialize};

/// Tamaño máximo de un KeyPackage publicado en la DHT.
pub const MAX_KEY_PACKAGE: usize = 4 * 1024;
/// Tamaño máximo de un sobre en un buzón.
pub const MAX_ENVELOPE: usize = 256 * 1024;

/// Metadatos de un chat que viajan junto al Welcome.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ChatMeta {
    pub title: Option<String>,
    /// Handles de los participantes (personas, no dispositivos).
    pub participants: Vec<String>,
    /// Dispositivo que crea el grupo; es el único que hace commits (D-007).
    pub committer: [u8; 32],
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum Request {
    /// Pide un KeyPackage nuevo para añadir este dispositivo a un grupo.
    KeyPackage,
    Welcome { welcome: Vec<u8>, meta: ChatMeta },
    /// Mensaje MLS (aplicación o commit).
    Mls(Vec<u8>),
    /// Avisa al dispositivo recién vinculado de su identidad (registro firmado).
    LinkGrant { record: Vec<u8> },
    /// Pide al committer de un grupo que sincronice los dispositivos.
    SyncHint { group_id: Vec<u8> },

    // --- Buzones (almacenamiento diferido, D-016) ---
    /// Guarda un sobre para un dispositivo desconectado.
    Deposit(Envelope),
    /// Pide los sobres guardados para quien pregunta (el buzón lo sabe por
    /// el canal Noise autenticado, no hace falta decir quién eres).
    Fetch,
    /// Confirma la recepción de sobres; el buzón los borra.
    AckEnvelopes { ids: Vec<[u8; 32]> },
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum Response {
    KeyPackage(Vec<u8>),
    Ack,
    /// Sobres del buzón, del más antiguo al más reciente. `more` indica que
    /// quedan más (hay que confirmar estos y volver a pedir).
    Envelopes { list: Vec<Envelope>, more: bool },
    Error(String),
}
