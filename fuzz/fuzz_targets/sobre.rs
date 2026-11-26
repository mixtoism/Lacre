//! Sobres sellados dirigidos a una clave fija: nada alterado debe abrirse.
#![no_main]
use ed25519_dalek::SigningKey;
use lacre_core::envelope::Envelope;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let me = SigningKey::from_bytes(&[7; 32]);
    let env = Envelope { to: me.verifying_key().to_bytes(), sealed: data.to_vec() };
    let _ = env.open(&me);
    // También el sobre entero en CBOR, como lo recibe un buzón.
    if let Ok(env) = lacre_core::cbor_from::<Envelope>(data) {
        let _ = env.check();
        let _ = env.open(&me);
    }
});
