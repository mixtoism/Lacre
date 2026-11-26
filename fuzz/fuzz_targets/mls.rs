//! Mensajes MLS, Welcomes y KeyPackages de origen desconocido.
#![no_main]
use ed25519_dalek::SigningKey;
use lacre_core::mls::{self, MlsEngine};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = mls::group_id_of(data);
    let _ = mls::verify_key_package(data);
    let device = SigningKey::from_bytes(&[1; 32]);
    let root = SigningKey::from_bytes(&[2; 32]).verifying_key().to_bytes();
    let engine = MlsEngine::open(&device, root, None).unwrap();
    let _ = engine.key_package();
    let _ = engine.join(data, |_| false);
    let _ = engine.process(data, &[3; 32]);
});
