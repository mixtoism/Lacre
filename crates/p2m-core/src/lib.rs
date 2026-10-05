//! Núcleo de p2message: identidad, registros de la DHT y cifrado MLS.
//!
//! No depende de la capa de red, para poder reutilizarse desde las apps
//! (Flutter vía FFI) y desde el nodo de línea de comandos.

pub mod envelope;
pub mod identity;
pub mod mls;
pub mod record;

use serde::Serialize;
use serde::de::DeserializeOwned;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("handle inválido: {0}")]
    InvalidHandle(String),
    #[error("clave inválida")]
    InvalidKey,
    #[error("firma inválida")]
    BadSignature,
    #[error("registro inválido: {0}")]
    InvalidRecord(String),
    #[error("codificación: {0}")]
    Codec(String),
    #[error("MLS: {0}")]
    Mls(String),
    #[error("grupo desconocido")]
    UnknownGroup,
    #[error("almacenamiento: {0}")]
    Storage(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub fn cbor_to<T: Serialize>(value: &T) -> Result<Vec<u8>, Error> {
    let mut out = Vec::new();
    ciborium::into_writer(value, &mut out).map_err(|e| Error::Codec(e.to_string()))?;
    Ok(out)
}

pub fn cbor_from<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, Error> {
    ciborium::from_reader(bytes).map_err(|e| Error::Codec(e.to_string()))
}
