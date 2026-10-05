//! Sobres para el almacenamiento diferido (buzones).
//!
//! Cuando un dispositivo está desconectado, sus mensajes se dejan en buzones
//! de otros nodos. El buzón no debe poder leer nada (ni siquiera los
//! metadatos del chat que acompañan a un Welcome), así que el contenido va
//! **sellado** para el dispositivo destinatario y **firmado** por el
//! dispositivo remitente. Ver `docs/DECISIONES.md` (D-016).
//!
//! Sellado: X25519 efímero contra la clave del destinatario (su Ed25519
//! convertida a Montgomery), clave derivada con BLAKE3 y ChaCha20-Poly1305.
//! La firma cubre destinatario y texto cifrado, y la comprueba también el
//! buzón, que así rechaza basura sin poder descifrar.

use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
use curve25519_dalek::montgomery::MontgomeryPoint;
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use rand::RngCore;
use serde::{Deserialize, Serialize};

use crate::Error;

const SIGN_DOMAIN: &[u8] = b"p2message/envelope/v1\0";
const KDF_CONTEXT: &str = "p2message/envelope/v1 seal";

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Envelope {
    /// Dispositivo remitente.
    pub from: [u8; 32],
    /// Dispositivo destinatario.
    pub to: [u8; 32],
    /// Clave efímera (32 bytes) seguida del texto cifrado.
    pub sealed: Vec<u8>,
    pub sig: Vec<u8>,
}

fn signed_bytes(to: &[u8; 32], sealed: &[u8]) -> Vec<u8> {
    [SIGN_DOMAIN, to, sealed].concat()
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
    /// Sella `plaintext` para el dispositivo `to` y lo firma con `sender`.
    pub fn seal(sender: &SigningKey, to: &[u8; 32], plaintext: &[u8]) -> Result<Self, Error> {
        let recipient = VerifyingKey::from_bytes(to).map_err(|_| Error::InvalidKey)?.to_montgomery();
        let mut eph_secret = [0u8; 32];
        rand::rngs::OsRng.fill_bytes(&mut eph_secret);
        let eph = MontgomeryPoint::mul_base_clamped(eph_secret);
        let key = derive_key(&eph, &recipient, &recipient.mul_clamped(eph_secret))?;
        // La clave es única por sobre (clave efímera nueva): el nonce puede ser fijo.
        let ct = ChaCha20Poly1305::new(&key)
            .encrypt(&Nonce::default(), plaintext)
            .map_err(|_| Error::Codec("cifrado del sobre".into()))?;
        let sealed = [eph.as_bytes().as_slice(), &ct].concat();
        let sig = sender.sign(&signed_bytes(to, &sealed)).to_bytes().to_vec();
        Ok(Self { from: sender.verifying_key().to_bytes(), to: *to, sealed, sig })
    }

    /// Comprueba la firma del remitente (no hace falta poder descifrar).
    pub fn verify(&self) -> Result<(), Error> {
        let from = VerifyingKey::from_bytes(&self.from).map_err(|_| Error::InvalidKey)?;
        let sig = Signature::from_slice(&self.sig).map_err(|_| Error::BadSignature)?;
        from.verify(&signed_bytes(&self.to, &self.sealed), &sig)
            .map_err(|_| Error::BadSignature)
    }

    /// Verifica y descifra con la clave del dispositivo destinatario.
    pub fn open(&self, recipient: &SigningKey) -> Result<Vec<u8>, Error> {
        if self.to != recipient.verifying_key().to_bytes() {
            return Err(Error::Codec("el sobre es para otro dispositivo".into()));
        }
        self.verify()?;
        if self.sealed.len() < 32 {
            return Err(Error::Codec("sobre truncado".into()));
        }
        let (eph, ct) = self.sealed.split_at(32);
        let eph = MontgomeryPoint(eph.try_into().unwrap());
        let me = recipient.verifying_key().to_montgomery();
        let key = derive_key(&eph, &me, &eph.mul_clamped(recipient.to_scalar_bytes()))?;
        ChaCha20Poly1305::new(&key)
            .decrypt(&Nonce::default(), ct)
            .map_err(|_| Error::Codec("no se pudo descifrar el sobre".into()))
    }

    /// Identificador estable, para confirmar entregas y descartar duplicados.
    pub fn id(&self) -> [u8; 32] {
        blake3::hash(&self.sig).into()
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
        env.verify().unwrap();
        assert_eq!(env.open(&bob).unwrap(), b"hola");
        // Nadie más puede abrirlo, ni haciéndose pasar por el destinatario.
        assert!(env.open(&alice).is_err());
        let mut other = env.clone();
        other.to = alice.verifying_key().to_bytes();
        assert!(other.open(&alice).is_err());
    }

    #[test]
    fn tampering_is_detected() {
        let (alice, bob) = (generate_key(), generate_key());
        let mut env = Envelope::seal(&alice, &bob.verifying_key().to_bytes(), b"hola").unwrap();
        *env.sealed.last_mut().unwrap() ^= 1;
        assert!(env.verify().is_err());
        assert!(env.open(&bob).is_err());
        // Cambiar el remitente invalida la firma.
        let mut env = Envelope::seal(&alice, &bob.verifying_key().to_bytes(), b"hola").unwrap();
        env.from = generate_key().verifying_key().to_bytes();
        assert!(env.verify().is_err());
    }
}
