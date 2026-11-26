//! Peticiones y respuestas del protocolo directo (CBOR de la red).
#![no_main]
use lacre_node::proto::{Request, Response};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = lacre_core::cbor_from::<Request>(data);
    let _ = lacre_core::cbor_from::<Response>(data);
});
