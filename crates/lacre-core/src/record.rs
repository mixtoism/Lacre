//! Registro de identidad publicado en la DHT.
//!
//! Lo firma la clave raíz y enumera los dispositivos autorizados. Es lo que
//! se obtiene al resolver un handle. Ver `docs/DECISIONES.md` (D-004, D-005).

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};

use crate::identity::{Handle, validate_name};
use crate::{Error, cbor_from, cbor_to};

pub const RECORD_VERSION: u8 = 1;
const SIGN_DOMAIN: &[u8] = b"p2message/identity-record/v1\0";
pub const MAX_DEVICES: usize = 16;
/// Tamaño máximo aceptado al leer de la red, antes de decodificar.
pub const MAX_RECORD_BYTES: usize = 8 * 1024;

/// Clave DHT bajo la que se publica el registro de una identidad.
///
/// Se indexa solo por sufijo: el nombre se comprueba al verificar. El espacio
/// `/p2m/v1/name/` queda reservado para los handles cortos (opción B).
pub fn dht_key(suffix: &str) -> Vec<u8> {
    format!("/p2m/v1/id/{suffix}").into_bytes()
}

/// Clave DHT del KeyPackage de último recurso de un dispositivo (D-017).
pub fn key_package_key(device_pk: &[u8; 32]) -> Vec<u8> {
    format!("/p2m/v1/kp/{}", data_encoding::BASE32_NOPAD.encode(device_pk).to_lowercase()).into_bytes()
}

/// Punto del espacio de la DHT junto al que viven los buzones de un
/// dispositivo (D-016). No se guarda ningún registro con esta clave: solo
/// sirve para elegir los nodos más cercanos.
pub fn mailbox_key(device_pk: &[u8; 32]) -> Vec<u8> {
    [b"/p2m/v1/mbox/".as_slice(), device_pk].concat()
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct DeviceEntry {
    pub device_pk: [u8; 32],
    pub label: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct IdentityRecord {
    pub v: u8,
    pub root_pk: [u8; 32],
    pub name: String,
    pub devices: Vec<DeviceEntry>,
    /// Se incrementa en cada cambio; el registro con mayor `seq` gana.
    pub seq: u64,
}

impl IdentityRecord {
    pub fn new(root: &SigningKey, name: &str) -> Result<Self, Error> {
        validate_name(name)?;
        Ok(Self {
            v: RECORD_VERSION,
            root_pk: root.verifying_key().to_bytes(),
            name: name.to_owned(),
            devices: Vec::new(),
            seq: 0,
        })
    }

    pub fn has_device(&self, device_pk: &[u8; 32]) -> bool {
        self.devices.iter().any(|d| &d.device_pk == device_pk)
    }

    pub fn add_device(&mut self, device_pk: [u8; 32], label: &str) -> Result<(), Error> {
        if self.has_device(&device_pk) {
            return Ok(());
        }
        if self.devices.len() >= MAX_DEVICES {
            return Err(Error::InvalidRecord("demasiados dispositivos".into()));
        }
        self.devices.push(DeviceEntry {
            device_pk,
            label: label.to_owned(),
        });
        self.seq += 1;
        Ok(())
    }

    pub fn remove_device(&mut self, device_pk: &[u8; 32]) -> bool {
        let before = self.devices.len();
        self.devices.retain(|d| &d.device_pk != device_pk);
        let removed = self.devices.len() != before;
        if removed {
            self.seq += 1;
        }
        removed
    }

    pub fn sign(&self, root: &SigningKey) -> Result<SignedRecord, Error> {
        if root.verifying_key().to_bytes() != self.root_pk {
            return Err(Error::InvalidRecord("clave raíz incorrecta".into()));
        }
        let body = cbor_to(self)?;
        let sig = root.sign(&[SIGN_DOMAIN, &body].concat());
        Ok(SignedRecord {
            body,
            sig: sig.to_bytes().to_vec(),
        })
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct SignedRecord {
    pub body: Vec<u8>,
    pub sig: Vec<u8>,
}

/// Registro cuya firma, formato y handle ya se han comprobado.
#[derive(Clone, Debug)]
pub struct VerifiedRecord {
    pub record: IdentityRecord,
    pub handle: Handle,
    pub signed: SignedRecord,
}

impl SignedRecord {
    pub fn to_bytes(&self) -> Result<Vec<u8>, Error> {
        cbor_to(self)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.len() > MAX_RECORD_BYTES {
            return Err(Error::InvalidRecord("registro demasiado grande".into()));
        }
        cbor_from(bytes)
    }

    /// Verifica firma y contenido. Calcula el sufijo (Argon2, memorizado).
    pub fn verify(&self) -> Result<VerifiedRecord, Error> {
        let record: IdentityRecord = cbor_from(&self.body)?;
        if record.v != RECORD_VERSION {
            return Err(Error::InvalidRecord(format!("versión {} no soportada", record.v)));
        }
        let root = VerifyingKey::from_bytes(&record.root_pk).map_err(|_| Error::InvalidKey)?;
        let sig = Signature::from_slice(&self.sig).map_err(|_| Error::BadSignature)?;
        root.verify(&[SIGN_DOMAIN, &self.body].concat(), &sig)
            .map_err(|_| Error::BadSignature)?;
        if record.devices.is_empty() || record.devices.len() > MAX_DEVICES {
            return Err(Error::InvalidRecord("número de dispositivos inválido".into()));
        }
        let handle = Handle::for_key(&record.name, &root)?;
        Ok(VerifiedRecord {
            record,
            handle,
            signed: self.clone(),
        })
    }

    /// Verifica y además exige que corresponda al handle buscado.
    pub fn verify_for(&self, handle: &Handle) -> Result<VerifiedRecord, Error> {
        let v = self.verify()?;
        if &v.handle != handle {
            return Err(Error::InvalidRecord("el registro no corresponde al handle".into()));
        }
        Ok(v)
    }
}

/// De varios registros candidatos (p. ej. respuestas de la DHT), elige el
/// válido para `handle` con mayor `seq`.
pub fn best_record<'a>(
    handle: &Handle,
    candidates: impl IntoIterator<Item = &'a [u8]>,
) -> Option<VerifiedRecord> {
    candidates
        .into_iter()
        .filter_map(|b| SignedRecord::from_bytes(b).ok()?.verify_for(handle).ok())
        .max_by_key(|v| v.record.seq)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::generate_key;

    fn sample() -> (SigningKey, IdentityRecord) {
        let root = generate_key();
        let mut rec = IdentityRecord::new(&root, "pablo").unwrap();
        rec.add_device(generate_key().verifying_key().to_bytes(), "móvil").unwrap();
        (root, rec)
    }

    #[test]
    fn sign_verify_roundtrip() {
        let (root, rec) = sample();
        let signed = rec.sign(&root).unwrap();
        let bytes = signed.to_bytes().unwrap();
        let v = SignedRecord::from_bytes(&bytes).unwrap().verify().unwrap();
        assert_eq!(v.record, rec);
        assert!(v.handle.matches("pablo", &root.verifying_key()));
    }

    #[test]
    fn tampered_record_is_rejected() {
        let (root, rec) = sample();
        let mut signed = rec.sign(&root).unwrap();
        let mut forged = rec.clone();
        forged.devices.push(DeviceEntry {
            device_pk: generate_key().verifying_key().to_bytes(),
            label: "intruso".into(),
        });
        signed.body = cbor_to(&forged).unwrap();
        assert!(matches!(signed.verify(), Err(Error::BadSignature)));
    }

    #[test]
    fn record_signed_by_other_key_does_not_match_handle() {
        let (root, rec) = sample();
        let handle = rec.sign(&root).unwrap().verify().unwrap().handle;
        // Un atacante publica un registro con el mismo nombre y su propia clave.
        let attacker = generate_key();
        let mut fake = IdentityRecord::new(&attacker, "pablo").unwrap();
        fake.add_device(generate_key().verifying_key().to_bytes(), "x").unwrap();
        let fake = fake.sign(&attacker).unwrap();
        assert!(fake.verify_for(&handle).is_err());
    }

    #[test]
    fn best_record_picks_highest_seq() {
        let (root, mut rec) = sample();
        let old = rec.sign(&root).unwrap().to_bytes().unwrap();
        rec.add_device(generate_key().verifying_key().to_bytes(), "portátil").unwrap();
        let new = rec.sign(&root).unwrap().to_bytes().unwrap();
        let handle = Handle::for_key("pablo", &root.verifying_key()).unwrap();
        let best = best_record(&handle, [old.as_slice(), b"basura", new.as_slice()]).unwrap();
        assert_eq!(best.record.devices.len(), 2);
    }
}
