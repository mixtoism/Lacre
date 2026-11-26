//! Mensajes del protocolo directo entre dispositivos (`/p2m/msg/1`).

use std::collections::BTreeMap;

use lacre_core::envelope::Envelope;
use serde::{Deserialize, Serialize};

use crate::files::StoredOffer;
use crate::history::StoredMessage;
use crate::people::BlockEntry;

/// Tamaño máximo de un KeyPackage publicado en la DHT.
pub const MAX_KEY_PACKAGE: usize = 4 * 1024;
/// Tamaño máximo de un sobre en un buzón. Los campos binarios van como
/// cadenas de bytes CBOR (`serde_bytes`), no como listas de enteros, que
/// casi duplicarían el tamaño (importa para los stickers, D-025).
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

/// Un mensaje del historial que pasa a otro dispositivo nuestro (D-050),
/// con los bytes del sticker y la oferta del archivo, si los hay.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HistoryItem {
    pub message: StoredMessage,
    #[serde(default, skip_serializing_if = "Option::is_none", with = "serde_bytes")]
    pub sticker: Option<Vec<u8>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offer: Option<StoredOffer>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum Request {
    /// Pide un KeyPackage nuevo para añadir este dispositivo a un grupo.
    KeyPackage,
    Welcome {
        #[serde(with = "serde_bytes")]
        welcome: Vec<u8>,
        meta: ChatMeta,
    },
    /// Mensaje MLS (aplicación o commit).
    Mls(#[serde(with = "serde_bytes")] Vec<u8>),
    /// Avisa al dispositivo recién vinculado de su identidad (registro
    /// firmado). `secret` es el secreto de un solo uso de su código de
    /// vinculación: sin él cualquiera podría meter un dispositivo en una
    /// identidad ajena (D-039).
    LinkGrant {
        #[serde(with = "serde_bytes")]
        record: Vec<u8>,
        #[serde(with = "serde_bytes")]
        secret: Vec<u8>,
    },
    /// Un dispositivo nuevo pide entrar con la invitación que muestra el
    /// primario (D-037). Si vale, el primario contesta `Granted`.
    Join {
        #[serde(with = "serde_bytes")]
        token: Vec<u8>,
        label: String,
    },
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
    /// Entrega activa (D-034): un buzón trae sobres a su destinatario, que
    /// responde con `Taken` y los ids que ya no hace falta guardar.
    Deliver { list: Vec<Envelope> },

    // --- Archivos (D-026) ---
    /// Pide un trozo cifrado de un archivo. `token` se deriva de la clave
    /// del archivo: solo lo conoce quien recibió la oferta por MLS.
    Chunk { file: String, index: u32, token: [u8; 32] },
    /// Deposita un trozo cifrado en un nodo cercano al archivo en la DHT,
    /// para quien lo pida con el remitente desconectado (D-054). `check` es
    /// el hash del testigo: el depósito no puede pedirlo a otros.
    StoreChunk {
        file: String,
        index: u32,
        check: [u8; 32],
        #[serde(with = "serde_bytes")]
        data: Vec<u8>,
    },

    /// Mantiene viva la conexión con un miembro de un chat (D-028).
    Ping,
    /// Pide la foto de perfil de la persona de este dispositivo (D-030).
    Avatar,
    /// Entre dispositivos de la misma persona: estos chats están aceptados
    /// (D-029). Solo se admite de un dispositivo de nuestro registro.
    Accepted { groups: Vec<Vec<u8>> },
    /// Entre dispositivos de la misma persona: bloqueos y desbloqueos, con
    /// su hora (D-031). Solo se admite de un dispositivo de nuestro registro.
    Blocks { list: BTreeMap<String, BlockEntry> },
    /// Historial de un chat desde el mensaje `from`, a otro dispositivo
    /// nuestro que ya estaba en él (D-050).
    History { group_id: Vec<u8>, from: u32 },
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum Response {
    KeyPackage(#[serde(with = "serde_bytes")] Vec<u8>),
    Ack,
    /// Sobres del buzón, del más antiguo al más reciente. `more` indica que
    /// quedan más (hay que confirmar estos y volver a pedir).
    Envelopes { list: Vec<Envelope>, more: bool },
    Chunk(#[serde(with = "serde_bytes")] Vec<u8>),
    /// Versión y foto (`None`: sin foto).
    Avatar {
        version: u64,
        #[serde(with = "serde_bytes")]
        image: Option<Vec<u8>>,
    },
    /// Sobres entregados (respuesta a `Deliver`).
    Taken { ids: Vec<[u8; 32]> },
    /// Registro firmado que ya incluye al dispositivo (respuesta a `Join`).
    Granted {
        #[serde(with = "serde_bytes")]
        record: Vec<u8>,
    },
    /// Un trozo del historial; `next`: desde dónde seguir pidiendo.
    History { list: Vec<HistoryItem>, next: Option<u32> },
    Error(String),
}
