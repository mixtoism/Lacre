//! Identidad: clave raíz, claves de dispositivo y handles `nombre#sufijo`.
//!
//! El sufijo se deriva de la clave pública raíz con Argon2id, de modo que
//! fabricar una clave que produzca un sufijo concreto (para suplantar un
//! handle) cueste del orden de 2^39 evaluaciones de Argon2. Ver
//! `docs/DECISIONES.md` (D-003).

use std::collections::HashMap;
use std::fmt;
use std::str::FromStr;
use std::sync::{Mutex, OnceLock};

use argon2::{Algorithm, Argon2, Params, Version};
use data_encoding::BASE32_NOPAD;
use ed25519_dalek::{SigningKey, VerifyingKey};
use rand::rngs::OsRng;

use crate::Error;

/// Longitud del sufijo en caracteres base32 (5 bytes = 40 bits).
pub const SUFFIX_LEN: usize = 8;
const SUFFIX_BYTES: usize = 5;
const SUFFIX_SALT: &[u8] = b"p2message/handle-suffix/v1";

// Coste de Argon2id para derivar el sufijo. Cambiarlo cambia todos los
// handles existentes: es parte del protocolo v1.
const ARGON_M_KIB: u32 = 64 * 1024;
const ARGON_T: u32 = 3;
const ARGON_P: u32 = 1;

pub const NAME_MIN: usize = 3;
pub const NAME_MAX: usize = 32;

pub fn generate_key() -> SigningKey {
    SigningKey::generate(&mut OsRng)
}

/// Deriva el sufijo del handle a partir de la clave pública raíz.
///
/// Es caro a propósito (~100-300 ms), así que el resultado se memoriza.
pub fn handle_suffix(root_pk: &VerifyingKey) -> String {
    static CACHE: OnceLock<Mutex<HashMap<[u8; 32], String>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    if let Some(s) = cache.lock().unwrap().get(root_pk.as_bytes()) {
        return s.clone();
    }
    let params = Params::new(ARGON_M_KIB, ARGON_T, ARGON_P, Some(SUFFIX_BYTES))
        .expect("parámetros de Argon2 válidos");
    let mut out = [0u8; SUFFIX_BYTES];
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .hash_password_into(root_pk.as_bytes(), SUFFIX_SALT, &mut out)
        .expect("Argon2 con parámetros fijos no falla");
    let suffix = BASE32_NOPAD.encode(&out).to_lowercase();
    cache.lock().unwrap().insert(*root_pk.as_bytes(), suffix.clone());
    suffix
}

pub fn validate_name(name: &str) -> Result<(), Error> {
    let ok_len = (NAME_MIN..=NAME_MAX).contains(&name.len());
    let ok_chars = name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
    if ok_len && ok_chars {
        Ok(())
    } else {
        Err(Error::InvalidHandle(format!(
            "el nombre debe tener {NAME_MIN}-{NAME_MAX} caracteres [a-z0-9_]"
        )))
    }
}

/// Handle verificable `nombre#sufijo`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Handle {
    pub name: String,
    pub suffix: String,
}

impl Handle {
    pub fn for_key(name: &str, root_pk: &VerifyingKey) -> Result<Self, Error> {
        validate_name(name)?;
        Ok(Self {
            name: name.to_owned(),
            suffix: handle_suffix(root_pk),
        })
    }

    /// Comprueba que este handle corresponde a la clave raíz dada.
    pub fn matches(&self, name: &str, root_pk: &VerifyingKey) -> bool {
        self.name == name && self.suffix == handle_suffix(root_pk)
    }
}

impl fmt::Display for Handle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}#{}", self.name, self.suffix)
    }
}

impl FromStr for Handle {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Error> {
        let s = s.trim().trim_start_matches('@').to_lowercase();
        let (name, suffix) = s
            .split_once('#')
            .ok_or_else(|| Error::InvalidHandle("formato esperado: nombre#sufijo".into()))?;
        validate_name(name)?;
        let ok_suffix = suffix.len() == SUFFIX_LEN
            && suffix.chars().all(|c| c.is_ascii_lowercase() || ('2'..='7').contains(&c));
        if !ok_suffix {
            return Err(Error::InvalidHandle(format!(
                "el sufijo debe tener {SUFFIX_LEN} caracteres base32"
            )));
        }
        Ok(Self {
            name: name.to_owned(),
            suffix: suffix.to_owned(),
        })
    }
}

/// Codificación compacta de una clave pública (para QR y códigos de enlace).
pub fn encode_key(pk: &VerifyingKey) -> String {
    BASE32_NOPAD.encode(pk.as_bytes()).to_lowercase()
}

pub fn decode_key(s: &str) -> Result<VerifyingKey, Error> {
    let bytes = BASE32_NOPAD
        .decode(s.trim().to_uppercase().as_bytes())
        .map_err(|_| Error::InvalidKey)?;
    let bytes: [u8; 32] = bytes.try_into().map_err(|_| Error::InvalidKey)?;
    VerifyingKey::from_bytes(&bytes).map_err(|_| Error::InvalidKey)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suffix_is_deterministic_and_well_formed() {
        let k = generate_key().verifying_key();
        let s = handle_suffix(&k);
        assert_eq!(s.len(), SUFFIX_LEN);
        assert_eq!(s, handle_suffix(&k));
        assert_ne!(s, handle_suffix(&generate_key().verifying_key()));
    }

    #[test]
    fn handle_roundtrip_and_match() {
        let k = generate_key().verifying_key();
        let h = Handle::for_key("pablo", &k).unwrap();
        let parsed: Handle = h.to_string().parse().unwrap();
        assert_eq!(parsed, h);
        assert!(parsed.matches("pablo", &k));
        assert!(!parsed.matches("pablo", &generate_key().verifying_key()));
    }

    #[test]
    fn rejects_bad_handles() {
        for bad in ["pa#abcdefgh", "pablo", "pablo#abc", "Pa-blo#abcdefgh", "pablo#abcdefg1"] {
            assert!(bad.parse::<Handle>().is_err(), "{bad}");
        }
    }

    #[test]
    fn key_encoding_roundtrip() {
        let k = generate_key().verifying_key();
        assert_eq!(decode_key(&encode_key(&k)).unwrap(), k);
    }
}
