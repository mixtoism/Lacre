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

/// Sufijos memorizados como mucho (cada entrada ocupa unos 50 bytes).
const SUFFIX_CACHE: usize = 4096;

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
    // De uno en uno: cada cálculo reserva 64 MiB y quien nos manda claves
    // nuevas a propósito no debe poder multiplicarlo (D-042).
    static ARGON: Mutex<()> = Mutex::new(());
    let _turn = ARGON.lock().unwrap_or_else(|e| e.into_inner());
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
    let mut cache = cache.lock().unwrap();
    // Memoria acotada aunque lleguen muchas claves distintas.
    if cache.len() >= SUFFIX_CACHE {
        cache.clear();
    }
    cache.insert(*root_pk.as_bytes(), suffix.clone());
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
/// Número de seguridad de dos identidades (D-043): 60 cifras en 12 grupos de
/// 5, iguales para las dos partes. Se calcula con las claves raíz completas,
/// así que comprobarlo en persona (o por otro canal) descarta que alguien
/// esté suplantando un handle, por mucho cómputo que gaste en su sufijo.
pub fn safety_number(a: &[u8; 32], b: &[u8; 32]) -> String {
    let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
    let mut out = [0u8; 60];
    blake3::Hasher::new_derive_key("p2message/safety-number/v1")
        .update(lo)
        .update(hi)
        .finalize_xof()
        .fill(&mut out);
    out.chunks(5)
        .map(|c| {
            let n = c.iter().fold(0u64, |acc, &x| acc << 8 | u64::from(x));
            format!("{:05}", n % 100_000)
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Código de confirmación de una vinculación (D-047): 6 cifras que los dos
/// dispositivos muestran y la persona compara. Depende de la identidad, del
/// dispositivo vinculado y del secreto de un solo uso del código o la
/// invitación, así que solo coincide si el que vinculó es quien se cree.
pub fn link_confirmation(root: &[u8; 32], device: &[u8; 32], secret: &[u8]) -> String {
    let h = blake3::Hasher::new_derive_key("p2message/link-confirm/v1")
        .update(root)
        .update(device)
        .update(secret)
        .finalize();
    let n = u32::from_be_bytes(h.as_bytes()[..4].try_into().unwrap()) % 1_000_000;
    format!("{:03} {:03}", n / 1000, n % 1000)
}

/// Frase de recuperación (D-048): la clave raíz en 24 palabras BIP39
/// (inglés), con su suma de comprobación. Quien la tenga es el dueño de la
/// identidad.
pub fn root_phrase(root: &SigningKey) -> zeroize::Zeroizing<String> {
    let m = bip39::Mnemonic::from_entropy(root.as_bytes()).expect("32 bytes son entropía BIP39 válida");
    zeroize::Zeroizing::new(m.to_string())
}

/// Recupera la clave raíz de su frase. Admite mayúsculas y espacios de más.
pub fn root_from_phrase(phrase: &str) -> Result<SigningKey, Error> {
    let clean = phrase.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase();
    let m = bip39::Mnemonic::parse_in_normalized(bip39::Language::English, &clean)
        .map_err(|e| Error::BadPhrase(e.to_string()))?;
    let entropy = zeroize::Zeroizing::new(m.to_entropy());
    let seed: [u8; 32] = entropy
        .as_slice()
        .try_into()
        .map_err(|_| Error::BadPhrase("tienen que ser 24 palabras".into()))?;
    Ok(SigningKey::from_bytes(&seed))
}

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

    #[test]
    fn safety_number_is_symmetric_and_distinct() {
        let (a, b, c) = ([1u8; 32], [2u8; 32], [3u8; 32]);
        let ab = safety_number(&a, &b);
        assert_eq!(ab, safety_number(&b, &a));
        assert_ne!(ab, safety_number(&a, &c));
        assert_eq!(ab.len(), 12 * 5 + 11);
        assert!(ab.split(' ').all(|g| g.len() == 5 && g.bytes().all(|d| d.is_ascii_digit())));
    }

    #[test]
    fn link_confirmation_depends_on_every_input() {
        let (r, d) = ([1u8; 32], [2u8; 32]);
        let c = link_confirmation(&r, &d, b"secreto");
        assert_eq!(c, link_confirmation(&r, &d, b"secreto"));
        assert_eq!(c.len(), 7);
        assert_ne!(c, link_confirmation(&[3u8; 32], &d, b"secreto"));
        assert_ne!(c, link_confirmation(&r, &[3u8; 32], b"secreto"));
        assert_ne!(c, link_confirmation(&r, &d, b"otro"));
    }

    #[test]
    fn root_phrase_roundtrip() {
        let k = generate_key();
        let p = root_phrase(&k);
        assert_eq!(p.split(' ').count(), 24);
        let back = root_from_phrase(&format!("  {}  ", p.to_uppercase().replace(' ', "   "))).unwrap();
        assert_eq!(back.to_bytes(), k.to_bytes());
        // Dos palabras cambiadas de sitio rompen la suma de comprobación
        // (con esta clave fija; en general falla 255 de cada 256 veces).
        let fixed = root_phrase(&SigningKey::from_bytes(&[1u8; 32]));
        let mut words: Vec<&str> = fixed.split(' ').collect();
        words.swap(0, 1);
        assert!(root_from_phrase(&words.join(" ")).is_err());
        assert!(root_from_phrase("hola que tal").is_err());
        // 12 palabras válidas (128 bits) no son una clave raíz.
        let twelve = bip39::Mnemonic::from_entropy(&[7u8; 16]).unwrap().to_string();
        assert!(root_from_phrase(&twelve).is_err());
    }
}
