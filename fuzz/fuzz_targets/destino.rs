//! Handles y contenido de QR escritos o escaneados por el usuario.
#![no_main]
use lacre_node::app::Target;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        let _ = s.parse::<Target>();
        let _ = s.parse::<lacre_core::identity::Handle>();
    }
});
