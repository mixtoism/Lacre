//! Registros de identidad tal como llegan de la DHT.
#![no_main]
use lacre_core::record::SignedRecord;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(r) = SignedRecord::from_bytes(data) {
        // `verify` calcula Argon2 (lento) solo si la firma ya es válida.
        let _ = r.verify();
    }
});
