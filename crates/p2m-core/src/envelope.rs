//! Sobres para el almacenamiento diferido (buzones).
//!
//! Cuando un dispositivo está desconectado, sus mensajes se dejan en buzones
//! de otros nodos. El buzón no debe poder leer nada (ni siquiera los
//! metadatos del chat que acompañan a un Welcome) **ni saber quién escribe**:
//! fuera solo va el destinatario. Remitente y firma viajan dentro, sellados
//! para el dispositivo destinatario (remitente sellado, D-034; antes D-016).
//!
//! Sellado: X25519 efímero contra la clave del destinatario (su Ed25519
//! convertida a Montgomery), clave derivada con BLAKE3 y ChaCha20-Poly1305.
//! Dentro va `{remitente, firma, contenido}`; la firma cubre destinatario,
//! clave efímera y contenido, así que el destinatario sabe quién lo envió y
//! nadie puede reenvolver el contenido firmado para otro.

use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
use curve25519_dalek::montgomery::MontgomeryPoint;
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use rand::RngCore;
use serde::{Deserialize, Serialize};

use crate::{Error, cbor_from, cbor_to};

const SIGN_DOMAIN: &[u8] = b"p2message/envelope/v2\0";
const KDF_CONTEXT: &str = "p2message/envelope/v2 seal";
/// Clave efímera + etiqueta de Poly1305.
const MIN_SEALED: usize = 32 + 16;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Envelope {
    /// Dispositivo destinatario (lo único que ve el buzón).
    pub to: [u8; 32],
    /// Clave efímera (32 bytes) seguida del texto cifrado de [`Inner`].
    #[serde(with = "serde_bytes")]
    pub sealed: Vec<u8>,
}

/// Lo que va sellado.
#[derive(Serialize, Deserialize)]
struct Inner {
    from: [u8; 32],
    #[serde(with = "serde_bytes")]
    sig: Vec<u8>,
    #[serde(with = "serde_bytes")]
    payload: Vec<u8>,
}

fn signed_bytes(to: &[u8; 32], eph: &[u8], payload: &[u8]) -> Vec<u8> {
    [SIGN_DOMAIN, to, eph, payload].concat()
}

fn derive_key(eph: &MontgomeryPoint, recipient: &MontgomeryPoint, shared: &MontgomeryPoint) -> Result<Key, Error> {
    // Un punto de orden bajo daría un secreto compartido nulo.
    if shared.as_bytes() == &[0u8; 32] {
        return Err(Error::InvalidKey);
    }
    let ikm = [eph.as_bytes().as_slice(), recipient.as_bytes(), shared.as_bytes()].concat();
    Ok(blake3::derive_key(KDF_CONTEXT, &ikm).into())
}

impl Envelope {
    /// Sella `plaintext` para el dispositivo `to`, firmado por `sender`.
    pub fn seal(sender: &SigningKey, to: &[u8; 32], plaintext: &[u8]) -> Result<Self, Error> {
        let recipient = VerifyingKey::from_bytes(to).map_err(|_| Error::InvalidKey)?.to_montgomery();
        let mut eph_secret = [0u8; 32];
        rand::rngs::OsRng.fill_bytes(&mut eph_secret);
        let eph = MontgomeryPoint::mul_base_clamped(eph_secret);
        let key = derive_key(&eph, &recipient, &recipient.mul_clamped(eph_secret))?;
        let inner = Inner {
            from: sender.verifying_key().to_bytes(),
            sig: sender.sign(&signed_bytes(to, eph.as_bytes(), plaintext)).to_bytes().to_vec(),
            payload: plaintext.to_vec(),
        };
        // La clave es única por sobre (clave efímera nueva): el nonce puede ser fijo.
        let ct = ChaCha20Poly1305::new(&key)
            .encrypt(&Nonce::default(), cbor_to(&inner)?.as_slice())
            .map_err(|_| Error::Codec("cifrado del sobre".into()))?;
        Ok(Self { to: *to, sealed: [eph.as_bytes().as_slice(), &ct].concat() })
    }

    /// Comprobación barata para un buzón, que no puede descifrar: el
    /// destinatario es una clave válida y el sobre no está truncado.
    pub fn check(&self) -> Result<(), Error> {
        VerifyingKey::from_bytes(&self.to).map_err(|_| Error::InvalidKey)?;
        if self.sealed.len() < MIN_SEALED {
            return Err(Error::Codec("sobre truncado".into()));
        }
        Ok(())
    }

    /// Descifra con la clave del dispositivo destinatario y verifica la
    /// firma interior. Devuelve el remitente y el contenido.
    pub fn open(&self, recipient: &SigningKey) -> Result<([u8; 32], Vec<u8>), Error> {
        if self.to != recipient.verifying_key().to_bytes() {
            return Err(Error::Codec("el sobre es para otro dispositivo".into()));
        }
        self.check()?;
        let (eph, ct) = self.sealed.split_at(32);
        let eph = MontgomeryPoint(eph.try_into().unwrap());
        let me = recipient.verifying_key().to_montgomery();
        let key = derive_key(&eph, &me, &eph.mul_clamped(recipient.to_scalar_bytes()))?;
        let inner = ChaCha20Poly1305::new(&key)
            .decrypt(&Nonce::default(), ct)
            .map_err(|_| Error::Codec("no se pudo descifrar el sobre".into()))?;
        let inner: Inner = cbor_from(&inner)?;
        let from = VerifyingKey::from_bytes(&inner.from).map_err(|_| Error::InvalidKey)?;
        let sig = Signature::from_slice(&inner.sig).map_err(|_| Error::BadSignature)?;
        from.verify(&signed_bytes(&self.to, eph.as_bytes(), &inner.payload), &sig)
            .map_err(|_| Error::BadSignature)?;
        Ok((inner.from, inner.payload))
    }

    /// Identificador estable, para confirmar entregas y descartar duplicados.
    pub fn id(&self) -> [u8; 32] {
        blake3::hash(&self.sealed).into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::generate_key;

    #[test]
    fn seal_and_open() {
        let (alice, bob) = (generate_key(), generate_key());
        let env = Envelope::seal(&alice, &bob.verifying_key().to_bytes(), b"hola").unwrap();
        env.check().unwrap();
        let (from, text) = env.open(&bob).unwrap();
        assert_eq!(from, alice.verifying_key().to_bytes());
        assert_eq!(text, b"hola");
        // Nadie más puede abrirlo, ni haciéndose pasar por el destinatario.
        assert!(env.open(&alice).is_err());
        let mut other = env.clone();
        other.to = alice.verifying_key().to_bytes();
        assert!(other.open(&alice).is_err());
    }

    #[test]
    fn sender_is_hidden() {
        let (alice, bob) = (generate_key(), generate_key());
        let env = Envelope::seal(&alice, &bob.verifying_key().to_bytes(), b"hola").unwrap();
        let bytes = cbor_to(&env).unwrap();
        let pk = alice.verifying_key().to_bytes();
        assert!(!bytes.windows(32).any(|w| w == pk));
    }

    #[test]
    fn tampering_is_detected() {
        let (alice, bob) = (generate_key(), generate_key());
        let mut env = Envelope::seal(&alice, &bob.verifying_key().to_bytes(), b"hola").unwrap();
        *env.sealed.last_mut().unwrap() ^= 1;
        assert!(env.open(&bob).is_err());
        env.sealed.truncate(10);
        assert!(env.check().is_err());
    }

    #[test]
    fn forged_sender_is_rejected() {
        // Un tercero que conoce la clave pública de Bob puede sellarle algo,
        // pero no firmarlo en nombre de Alice.
        let (alice, bob, mallory) = (generate_key(), generate_key(), generate_key());
        let to = bob.verifying_key().to_bytes();
        let recipient = bob.verifying_key().to_montgomery();
        let mut eph_secret = [0u8; 32];
        rand::rngs::OsRng.fill_bytes(&mut eph_secret);
        let eph = MontgomeryPoint::mul_base_clamped(eph_secret);
        let key = derive_key(&eph, &recipient, &recipient.mul_clamped(eph_secret)).unwrap();
        let inner = Inner {
            from: alice.verifying_key().to_bytes(),
            sig: mallory.sign(&signed_bytes(&to, eph.as_bytes(), b"hola")).to_bytes().to_vec(),
            payload: b"hola".to_vec(),
        };
        let ct = ChaCha20Poly1305::new(&key).encrypt(&Nonce::default(), cbor_to(&inner).unwrap().as_slice()).unwrap();
        let env = Envelope { to, sealed: [eph.as_bytes().as_slice(), &ct].concat() };
        assert!(matches!(env.open(&bob), Err(Error::BadSignature)));
    }
}
