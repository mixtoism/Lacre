//! Cifrado de grupo con MLS (RFC 9420) vía openmls.
//!
//! Cada *dispositivo* es una hoja del árbol MLS. Un chat 1:1 es un grupo con
//! todos los dispositivos de ambas personas. La credencial de cada hoja es un
//! [`MemberId`] (clave raíz + clave de dispositivo) y la clave de firma de la
//! hoja es la propia clave del dispositivo. Ver `docs/DECISIONES.md` (D-006).

use std::collections::HashMap;
use std::path::PathBuf;

use ed25519_dalek::SigningKey;
use openmls::prelude::tls_codec::{Deserialize as _, Serialize as _};
use openmls::prelude::*;
use openmls_basic_credential::SignatureKeyPair;
use openmls_rust_crypto::{MemoryStorage, RustCrypto};
use openmls_traits::OpenMlsProvider;
use openmls_traits::types::SignatureScheme;
use serde::{Deserialize, Serialize};
use serde_bytes::{ByteBuf, Bytes};

use crate::{Error, cbor_from, cbor_to, vault};

pub use openmls::prelude::KeyPackage;

pub const CIPHERSUITE: Ciphersuite =
    Ciphersuite::MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519;
/// Épocas pasadas cuyas claves se conservan, para descifrar mensajes que
/// llegan tarde tras un commit.
const MAX_PAST_EPOCHS: usize = 5;

/// Identidad de una hoja MLS: a qué persona (raíz) pertenece qué dispositivo.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MemberId {
    pub root_pk: [u8; 32],
    pub device_pk: [u8; 32],
}

#[derive(Default)]
pub struct Provider {
    crypto: RustCrypto,
    storage: MemoryStorage,
}

impl OpenMlsProvider for Provider {
    type CryptoProvider = RustCrypto;
    type RandProvider = RustCrypto;
    type StorageProvider = MemoryStorage;

    fn storage(&self) -> &MemoryStorage {
        &self.storage
    }
    fn crypto(&self) -> &RustCrypto {
        &self.crypto
    }
    fn rand(&self) -> &RustCrypto {
        &self.crypto
    }
}

pub enum Incoming {
    Application {
        group_id: Vec<u8>,
        sender: MemberId,
        body: Vec<u8>,
    },
    /// Se aplicó un commit (altas/bajas de dispositivos).
    Commit { group_id: Vec<u8>, removed_self: bool },
    /// Mensaje válido sin efecto visible (p. ej. una propuesta).
    Ignored,
}

/// Id del grupo al que va dirigido un mensaje MLS, sin descifrarlo.
pub fn group_id_of(bytes: &[u8]) -> Result<Vec<u8>, Error> {
    let msg = MlsMessageIn::tls_deserialize_exact(bytes).map_err(mls_err)?;
    let protocol = msg
        .try_into_protocol_message()
        .map_err(|_| Error::Mls("no es un mensaje de grupo".into()))?;
    Ok(protocol.group_id().to_vec())
}

/// Valida un KeyPackage (firma, suite, credencial coherente con la clave de
/// firma) y devuelve a quién pertenece. No necesita estado: la usan también
/// los nodos de la DHT antes de guardar un KeyPackage publicado.
pub fn verify_key_package(bytes: &[u8]) -> Result<(KeyPackage, MemberId), Error> {
    let kp = KeyPackageIn::tls_deserialize_exact(bytes)
        .map_err(mls_err)?
        .validate(&RustCrypto::default(), ProtocolVersion::Mls10)
        .map_err(mls_err)?;
    if kp.ciphersuite() != CIPHERSUITE {
        return Err(Error::Mls("suite no soportada".into()));
    }
    let leaf = kp.leaf_node();
    let id = member_of(leaf.credential(), leaf.signature_key().as_slice())?;
    Ok((kp, id))
}

pub struct MlsEngine {
    provider: Provider,
    signer: SignatureKeyPair,
    me: MemberId,
    path: Option<PathBuf>,
}

fn member_of(credential: &Credential, signature_key: &[u8]) -> Result<MemberId, Error> {
    let basic = BasicCredential::try_from(credential.clone())
        .map_err(|_| Error::Mls("credencial no básica".into()))?;
    let id: MemberId = cbor_from(basic.identity())?;
    // La clave de firma de la hoja debe ser la del dispositivo que declara.
    if id.device_pk != signature_key {
        return Err(Error::Mls("credencial no coincide con la clave de firma".into()));
    }
    Ok(id)
}

/// Estado MLS en el JSON de `openmls_memory_storage` (claves y valores en
/// base64), el formato anterior a D-033.
fn legacy_json(bytes: &[u8]) -> Result<HashMap<Vec<u8>, Vec<u8>>, Error> {
    #[derive(Deserialize)]
    struct Legacy {
        values: HashMap<String, String>,
    }
    let legacy: Legacy = serde_json::from_slice(bytes).map_err(|e| Error::Storage(e.to_string()))?;
    let b64 = |s: &str| {
        data_encoding::BASE64
            .decode(s.as_bytes())
            .map_err(|e| Error::Storage(e.to_string()))
    };
    legacy.values.iter().map(|(k, v)| Ok((b64(k)?, b64(v)?))).collect()
}

fn mls_err(e: impl std::fmt::Debug) -> Error {
    Error::Mls(format!("{e:?}"))
}

impl MlsEngine {
    /// Abre el motor de un dispositivo. Si `path` existe, carga el estado.
    pub fn open(device: &SigningKey, root_pk: [u8; 32], path: Option<PathBuf>) -> Result<Self, Error> {
        let provider = Provider::default();
        if let Some(p) = &path
            && let Some(bytes) = vault::read_opt(p)?
        {
            if bytes.first() == Some(&b'{') {
                // Formato anterior: el JSON de openmls (en claro, o ya
                // cifrado tal cual por la migración).
                *provider.storage.values.write().unwrap() = legacy_json(&bytes)?;
            } else {
                let values: Vec<(ByteBuf, ByteBuf)> = cbor_from(&bytes)?;
                *provider.storage.values.write().unwrap() =
                    values.into_iter().map(|(k, v)| (k.into_vec(), v.into_vec())).collect();
            }
        }
        let device_pk = device.verifying_key().to_bytes();
        let signer = SignatureKeyPair::from_raw(
            SignatureScheme::ED25519,
            device.to_bytes().to_vec(),
            device_pk.to_vec(),
        );
        Ok(Self {
            provider,
            signer,
            me: MemberId { root_pk, device_pk },
            path,
        })
    }

    pub fn me(&self) -> MemberId {
        self.me
    }

    fn save(&self) -> Result<(), Error> {
        let Some(path) = &self.path else { return Ok(()) };
        // CBOR, cifrado si hay clave de datos (D-033). El nombre mls.json
        // es del formato anterior.
        let bytes = {
            let map = self.provider.storage.values.read().unwrap();
            cbor_to(&map.iter().map(|(k, v)| (Bytes::new(k), Bytes::new(v))).collect::<Vec<_>>())?
        };
        vault::write(path, &bytes)?;
        Ok(())
    }

    fn credential(&self) -> Result<CredentialWithKey, Error> {
        Ok(CredentialWithKey {
            credential: BasicCredential::new(cbor_to(&self.me)?).into(),
            signature_key: self.signer.to_public_vec().into(),
        })
    }

    fn create_config() -> MlsGroupCreateConfig {
        MlsGroupCreateConfig::builder()
            .ciphersuite(CIPHERSUITE)
            .use_ratchet_tree_extension(true)
            .max_past_epochs(MAX_PAST_EPOCHS)
            .build()
    }

    fn join_config() -> MlsGroupJoinConfig {
        MlsGroupJoinConfig::builder()
            .use_ratchet_tree_extension(true)
            .max_past_epochs(MAX_PAST_EPOCHS)
            .build()
    }

    fn load(&self, group_id: &[u8]) -> Result<MlsGroup, Error> {
        MlsGroup::load(self.provider.storage(), &GroupId::from_slice(group_id))
            .map_err(mls_err)?
            .ok_or(Error::UnknownGroup)
    }

    /// KeyPackage nuevo y de un solo uso para que otro nos añada a un grupo.
    pub fn key_package(&self) -> Result<Vec<u8>, Error> {
        let bundle = KeyPackage::builder()
            .build(CIPHERSUITE, &self.provider, &self.signer, self.credential()?)
            .map_err(mls_err)?;
        self.save()?;
        bundle.key_package().tls_serialize_detached().map_err(mls_err)
    }

    /// KeyPackage *de último recurso*: reutilizable, se publica en la DHT
    /// para que puedan añadirnos a un grupo mientras estamos desconectados
    /// (D-017).
    pub fn last_resort_key_package(&self) -> Result<Vec<u8>, Error> {
        let bundle = KeyPackage::builder()
            .mark_as_last_resort()
            // La hoja debe declarar que entiende la extensión.
            .leaf_node_capabilities(
                Capabilities::builder().extensions(vec![ExtensionType::LastResort]).build(),
            )
            .build(CIPHERSUITE, &self.provider, &self.signer, self.credential()?)
            .map_err(mls_err)?;
        self.save()?;
        bundle.key_package().tls_serialize_detached().map_err(mls_err)
    }

    /// Valida un KeyPackage recibido y devuelve a quién pertenece.
    pub fn validate_key_package(&self, bytes: &[u8]) -> Result<(KeyPackage, MemberId), Error> {
        verify_key_package(bytes)
    }

    pub fn create_group(&self) -> Result<Vec<u8>, Error> {
        let group = MlsGroup::new(&self.provider, &self.signer, &Self::create_config(), self.credential()?)
            .map_err(mls_err)?;
        self.save()?;
        Ok(group.group_id().to_vec())
    }

    /// Añade dispositivos. Devuelve `(commit, welcome)` ya aplicado localmente.
    pub fn add(&self, group_id: &[u8], kps: &[KeyPackage]) -> Result<(Vec<u8>, Vec<u8>), Error> {
        let mut group = self.load(group_id)?;
        let (commit, welcome, _) = group
            .add_members(&self.provider, &self.signer, kps)
            .map_err(mls_err)?;
        group.merge_pending_commit(&self.provider).map_err(mls_err)?;
        self.save()?;
        Ok((
            commit.to_bytes().map_err(mls_err)?,
            welcome.to_bytes().map_err(mls_err)?,
        ))
    }

    /// Expulsa dispositivos (revocados). Devuelve el commit.
    pub fn remove(&self, group_id: &[u8], members: &[MemberId]) -> Result<Vec<u8>, Error> {
        let mut group = self.load(group_id)?;
        let leaves: Vec<_> = group
            .members()
            .filter(|m| {
                member_of(&m.credential, &m.signature_key).is_ok_and(|id| members.contains(&id))
            })
            .map(|m| m.index)
            .collect();
        let (commit, _, _) = group
            .remove_members(&self.provider, &self.signer, &leaves)
            .map_err(mls_err)?;
        group.merge_pending_commit(&self.provider).map_err(mls_err)?;
        self.save()?;
        commit.to_bytes().map_err(mls_err)
    }

    /// Entra en un grupo a partir de un Welcome. Devuelve el id del grupo.
    /// Entra en un grupo con un Welcome. Si ya teníamos un grupo con el
    /// mismo id (nos expulsaron y vuelven a añadirnos), lo sustituye solo si
    /// `may_replace(group_id)` lo permite.
    ///
    /// Es atómico (D-042): OpenMLS borra el KeyPackage al preparar el
    /// Welcome, antes de comprobarlo entero, y un Welcome alterado (o con
    /// credenciales incoherentes) dejaría sin él al válido. Si algo falla, el
    /// estado vuelve a como estaba.
    pub fn join(&self, welcome: &[u8], may_replace: impl FnOnce(&[u8]) -> bool) -> Result<Vec<u8>, Error> {
        let snapshot = self.provider.storage.values.read().unwrap().clone();
        let result = self.join_inner(welcome, may_replace);
        if result.is_err() {
            *self.provider.storage.values.write().unwrap() = snapshot;
        }
        result
    }

    fn join_inner(&self, welcome: &[u8], may_replace: impl FnOnce(&[u8]) -> bool) -> Result<Vec<u8>, Error> {
        let msg = MlsMessageIn::tls_deserialize_exact(welcome).map_err(mls_err)?;
        let MlsMessageBodyIn::Welcome(welcome) = msg.extract() else {
            return Err(Error::Mls("se esperaba un Welcome".into()));
        };
        let mut builder =
            StagedWelcome::build_from_welcome(&self.provider, &Self::join_config(), welcome).map_err(mls_err)?;
        let group_id = builder.processed_welcome().unverified_group_info().group_id().clone();
        if MlsGroup::load(self.provider.storage(), &group_id).map_err(mls_err)?.is_some() {
            if !may_replace(group_id.as_slice()) {
                return Err(Error::Mls("ya perteneces a ese grupo".into()));
            }
            builder = builder.replace_old_group();
        }
        let group = builder
            .build()
            .map_err(mls_err)?
            .into_group(&self.provider)
            .map_err(mls_err)?;
        // Todas las hojas deben tener credenciales coherentes.
        for m in group.members() {
            member_of(&m.credential, &m.signature_key)?;
        }
        self.save()?;
        Ok(group.group_id().to_vec())
    }

    pub fn members(&self, group_id: &[u8]) -> Result<Vec<MemberId>, Error> {
        let group = self.load(group_id)?;
        group
            .members()
            .map(|m| member_of(&m.credential, &m.signature_key))
            .collect()
    }

    pub fn encrypt(&self, group_id: &[u8], body: &[u8]) -> Result<Vec<u8>, Error> {
        let mut group = self.load(group_id)?;
        let msg = group
            .create_message(&self.provider, &self.signer, body)
            .map_err(mls_err)?;
        self.save()?;
        msg.to_bytes().map_err(mls_err)
    }

    /// Procesa un mensaje MLS (aplicación o commit) recibido de la red.
    ///
    /// Solo se aceptan commits firmados por el dispositivo `committer` del
    /// grupo; cualquier otro se rechaza sin aplicarse (D-007).
    pub fn process(&self, bytes: &[u8], committer: &[u8; 32]) -> Result<Incoming, Error> {
        let msg = MlsMessageIn::tls_deserialize_exact(bytes).map_err(mls_err)?;
        let protocol = msg
            .try_into_protocol_message()
            .map_err(|_| Error::Mls("no es un mensaje de grupo".into()))?;
        let group_id = protocol.group_id().to_vec();
        let mut group = self.load(&group_id)?;
        let processed = group
            .process_message(&self.provider, protocol)
            .map_err(mls_err)?;
        // Solo hojas del grupo (nada de commits externos ni remitentes de
        // fuera) y con la credencial atada a su clave de firma (D-040).
        let Sender::Member(index) = processed.sender() else {
            return Err(Error::Mls("remitente que no es miembro del grupo".into()));
        };
        let leaf = group
            .member_at(*index)
            .ok_or_else(|| Error::Mls("hoja del remitente inexistente".into()))?;
        let sender = member_of(&leaf.credential, &leaf.signature_key)?;
        if member_of(processed.credential(), &leaf.signature_key)? != sender {
            return Err(Error::Mls("credencial del mensaje distinta de la de su hoja".into()));
        }
        let result = match processed.into_content() {
            ProcessedMessageContent::ApplicationMessage(app) => {
                Incoming::Application {
                    group_id,
                    sender,
                    body: app.into_bytes(),
                }
            }
            ProcessedMessageContent::StagedCommitMessage(staged) => {
                if &sender.device_pk != committer {
                    return Err(Error::Mls("commit de un dispositivo que no es el committer".into()));
                }
                let removed_self = staged.self_removed();
                // Las hojas nuevas o actualizadas deben ser coherentes antes
                // de aplicar el commit.
                for leaf in staged.add_proposals().map(|p| p.add_proposal().key_package().leaf_node().clone())
                    .chain(staged.update_proposals().map(|p| p.update_proposal().leaf_node().clone()))
                    .chain(staged.update_path_leaf_node().cloned())
                {
                    member_of(leaf.credential(), leaf.signature_key().as_slice())?;
                }
                group
                    .merge_staged_commit(&self.provider, *staged)
                    .map_err(mls_err)?;
                Incoming::Commit { group_id, removed_self }
            }
            _ => Incoming::Ignored,
        };
        self.save()?;
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::generate_key;

    fn engine(root: &SigningKey) -> MlsEngine {
        MlsEngine::open(&generate_key(), root.verifying_key().to_bytes(), None).unwrap()
    }

    fn app_text(i: Incoming) -> (MemberId, String) {
        match i {
            Incoming::Application { sender, body, .. } => (sender, String::from_utf8(body).unwrap()),
            _ => panic!("se esperaba un mensaje de aplicación"),
        }
    }

    /// El JSON anterior de openmls se lee desde los bytes (ya descifrados),
    /// no desde el archivo, que tras migrar está cifrado.
    #[test]
    fn legacy_json_from_bytes() {
        let storage = MemoryStorage::default();
        storage.values.write().unwrap().insert(vec![1, 2, 255], vec![0, 9]);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mls.json");
        storage.save_to_file(&std::fs::File::create(&path).unwrap()).unwrap();
        let values = legacy_json(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(values.get(&vec![1, 2, 255]), Some(&vec![0, 9]));
    }

    /// Alicia (móvil + portátil) y Bob (un dispositivo) en un chat 1:1;
    /// luego Bob añade un segundo dispositivo y Alicia revoca el portátil.
    #[test]
    fn multi_device_chat() {
        let (alice, bob) = (generate_key(), generate_key());
        let (a1, a2, b1) = (engine(&alice), engine(&alice), engine(&bob));

        let gid = a1.create_group().unwrap();
        let kps: Vec<_> = [&a2, &b1]
            .iter()
            .map(|e| a1.validate_key_package(&e.key_package().unwrap()).unwrap().0)
            .collect();
        let (_commit, welcome) = a1.add(&gid, &kps).unwrap();
        assert_eq!(a2.join(&welcome, |_| false).unwrap(), gid);
        assert_eq!(b1.join(&welcome, |_| false).unwrap(), gid);
        assert_eq!(b1.members(&gid).unwrap().len(), 3);

        // Bob escribe; ambos dispositivos de Alicia lo leen.
        let m = b1.encrypt(&gid, b"hola").unwrap();
        for dev in [&a1, &a2] {
            let (sender, text) = app_text(dev.process(&m, &a1.me().device_pk).unwrap());
            assert_eq!(text, "hola");
            assert_eq!(sender.root_pk, bob.verifying_key().to_bytes());
        }

        // Bob vincula un segundo dispositivo; el creador lo añade.
        let b2 = engine(&bob);
        let kp = a1.validate_key_package(&b2.key_package().unwrap()).unwrap().0;
        let (commit, welcome) = a1.add(&gid, &[kp]).unwrap();
        for dev in [&a2, &b1] {
            assert!(matches!(dev.process(&commit, &a1.me().device_pk).unwrap(), Incoming::Commit { removed_self: false, .. }));
        }
        b2.join(&welcome, |_| false).unwrap();

        // Alicia revoca su portátil.
        let commit = a1.remove(&gid, &[a2.me()]).unwrap();
        assert!(matches!(a2.process(&commit, &a1.me().device_pk).unwrap(), Incoming::Commit { removed_self: true, .. }));
        for dev in [&b1, &b2] {
            dev.process(&commit, &a1.me().device_pk).unwrap();
        }
        let m = a1.encrypt(&gid, b"sin portatil").unwrap();
        for dev in [&b1, &b2] {
            assert_eq!(app_text(dev.process(&m, &a1.me().device_pk).unwrap()).1, "sin portatil");
        }
        assert!(a2.process(&m, &a1.me().device_pk).is_err());
    }

    /// Bob sale del grupo y vuelve a entrar: el Welcome sustituye al grupo
    /// viejo solo si se permite.
    #[test]
    fn rejoin_after_removal() {
        let (alice, bob) = (generate_key(), generate_key());
        let (a1, b1) = (engine(&alice), engine(&bob));
        let gid = a1.create_group().unwrap();
        let kp = a1.validate_key_package(&b1.key_package().unwrap()).unwrap().0;
        let (_, welcome) = a1.add(&gid, &[kp]).unwrap();
        b1.join(&welcome, |_| false).unwrap();

        let commit = a1.remove(&gid, &[b1.me()]).unwrap();
        assert!(matches!(b1.process(&commit, &a1.me().device_pk).unwrap(), Incoming::Commit { removed_self: true, .. }));
        let m = a1.encrypt(&gid, b"sin bob").unwrap();
        assert!(b1.process(&m, &a1.me().device_pk).is_err());

        // Sin permiso no se sustituye (y el KeyPackage, de un solo uso, se
        // gasta: hace falta otro).
        let kp = a1.validate_key_package(&b1.key_package().unwrap()).unwrap().0;
        let (_, welcome) = a1.add(&gid, &[kp]).unwrap();
        assert!(b1.join(&welcome, |_| false).is_err());
        let commit = a1.remove(&gid, &[b1.me()]).unwrap();
        assert!(b1.process(&commit, &a1.me().device_pk).is_err());
        let kp = a1.validate_key_package(&b1.key_package().unwrap()).unwrap().0;
        let (_, welcome) = a1.add(&gid, &[kp]).unwrap();
        assert_eq!(b1.join(&welcome, |g| g == gid.as_slice()).unwrap(), gid);
        let m = a1.encrypt(&gid, b"con bob").unwrap();
        assert_eq!(app_text(b1.process(&m, &a1.me().device_pk).unwrap()).1, "con bob");
    }

    /// Un KeyPackage de último recurso sirve para entrar en varios grupos.
    #[test]
    fn last_resort_key_package_is_reusable() {
        let (alice, bob) = (generate_key(), generate_key());
        let (a1, b1) = (engine(&alice), engine(&bob));
        let bytes = b1.last_resort_key_package().unwrap();
        let (kp, id) = verify_key_package(&bytes).unwrap();
        assert_eq!(id, b1.me());
        for _ in 0..2 {
            let gid = a1.create_group().unwrap();
            let (_, welcome) = a1.add(&gid, &[kp.clone()]).unwrap();
            assert_eq!(b1.join(&welcome, |_| false).unwrap(), gid);
        }
    }

    #[test]
    fn rejects_commit_from_non_committer() {
        let (alice, bob) = (generate_key(), generate_key());
        let (a1, b1, c1) = (engine(&alice), engine(&bob), engine(&bob));
        let gid = a1.create_group().unwrap();
        let kp = a1.validate_key_package(&b1.key_package().unwrap()).unwrap().0;
        let (_, welcome) = a1.add(&gid, &[kp]).unwrap();
        b1.join(&welcome, |_| false).unwrap();
        // Bob intenta añadir un dispositivo por su cuenta.
        let kp = b1.validate_key_package(&c1.key_package().unwrap()).unwrap().0;
        let (commit, _) = b1.add(&gid, &[kp]).unwrap();
        assert!(a1.process(&commit, &a1.me().device_pk).is_err());
        assert_eq!(a1.members(&gid).unwrap().len(), 2);
    }

    #[test]
    fn rejects_key_package_with_forged_credential() {
        let root = generate_key();
        let honest = engine(&root);
        // Un dispositivo que declara ser otro dispositivo.
        let liar_key = generate_key();
        let mut liar = MlsEngine::open(&liar_key, root.verifying_key().to_bytes(), None).unwrap();
        liar.me.device_pk = generate_key().verifying_key().to_bytes();
        let kp = liar.key_package().unwrap();
        assert!(honest.validate_key_package(&kp).is_err());
    }

    #[test]
    fn state_survives_reopen() {
        // La clave de datos es global del proceso y otra prueba la fija: se
        // fija aquí la misma para no depender del orden (vault::tests).
        vault::set_key(vault::TEST_KEY).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mls.json");
        let root = generate_key();
        let dev = generate_key();
        let gid = {
            let e = MlsEngine::open(&dev, root.verifying_key().to_bytes(), Some(path.clone())).unwrap();
            e.create_group().unwrap()
        };
        let e = MlsEngine::open(&dev, root.verifying_key().to_bytes(), Some(path)).unwrap();
        assert_eq!(e.members(&gid).unwrap().len(), 1);
    }
}
