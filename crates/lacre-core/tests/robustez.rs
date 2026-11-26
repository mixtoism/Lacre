//! Pruebas de robustez (D-042): los decodificadores que reciben bytes de la
//! red no deben entrar en pánico ni aceptar entradas alteradas. Se les da
//! basura aleatoria y mutaciones (bits cambiados, recortes, bytes añadidos)
//! de entradas válidas. No sustituyen a un fuzzer guiado por cobertura
//! (`cargo fuzz`), que queda pendiente.

use lacre_core::envelope::Envelope;
use lacre_core::identity::generate_key;
use lacre_core::mls::{self, MlsEngine};
use lacre_core::record::{IdentityRecord, SignedRecord};
use rand::rngs::StdRng;
use rand::{Rng, RngCore, SeedableRng};

/// Semilla fija: si algo falla, se puede repetir.
fn rng() -> StdRng {
    StdRng::seed_from_u64(0x1ac2e)
}

fn random_bytes(rng: &mut StdRng, max: usize) -> Vec<u8> {
    let mut v = vec![0u8; rng.gen_range(0..=max)];
    rng.fill_bytes(&mut v);
    v
}

/// Una mutación de `valid` que no es igual a `valid`.
fn mutate(rng: &mut StdRng, valid: &[u8]) -> Vec<u8> {
    loop {
        let mut v = valid.to_vec();
        match rng.gen_range(0..4) {
            0 if !v.is_empty() => {
                let i = rng.gen_range(0..v.len());
                v[i] ^= 1 << rng.gen_range(0..8);
            }
            1 if !v.is_empty() => v.truncate(rng.gen_range(0..v.len())),
            2 => {
                let i = rng.gen_range(0..=v.len());
                let extra = random_bytes(rng, 16);
                v.splice(i..i, extra);
            }
            _ if !v.is_empty() => {
                let i = rng.gen_range(0..v.len());
                v[i] = rng.r#gen();
            }
            _ => v.push(rng.r#gen()),
        }
        if v != valid {
            return v;
        }
    }
}

#[test]
fn records_reject_garbage_and_mutations() {
    let mut rng = rng();
    let root = generate_key();
    let mut record = IdentityRecord::new(&root, "ana").unwrap();
    record.add_device(generate_key().verifying_key().to_bytes(), "móvil").unwrap();
    let valid = record.sign(&root).unwrap().to_bytes().unwrap();
    // La entrada válida sí pasa (calcula un Argon2).
    SignedRecord::from_bytes(&valid).unwrap().verify().unwrap();
    for _ in 0..2000 {
        let _ = SignedRecord::from_bytes(&random_bytes(&mut rng, 512)).map(|r| r.verify());
    }
    for _ in 0..2000 {
        let bad = mutate(&mut rng, &valid);
        if let Ok(r) = SignedRecord::from_bytes(&bad) {
            // Una mutación puede dar el mismo registro con otra codificación
            // CBOR; si verifica, tiene que ser exactamente el firmado.
            if let Ok(v) = r.verify() {
                assert_eq!(v.record.devices.len(), 1);
                assert_eq!(v.record.name, "ana");
            }
        }
    }
}

#[test]
fn envelopes_reject_garbage_and_mutations() {
    let mut rng = rng();
    let alice = generate_key();
    let bob = generate_key();
    let env = Envelope::seal(&alice, &bob.verifying_key().to_bytes(), b"hola").unwrap();
    let (from, body) = env.open(&bob).unwrap();
    assert_eq!((from, body.as_slice()), (alice.verifying_key().to_bytes(), &b"hola"[..]));
    for _ in 0..3000 {
        let bad = Envelope { to: env.to, sealed: mutate(&mut rng, &env.sealed) };
        assert!(bad.open(&bob).is_err(), "sobre alterado aceptado");
    }
    for _ in 0..3000 {
        let bad = Envelope { to: env.to, sealed: random_bytes(&mut rng, 256) };
        assert!(bad.open(&bob).is_err());
    }
    // Otro destinatario no lo abre.
    assert!(env.open(&alice).is_err());
}

#[test]
fn mls_rejects_garbage_and_mutations() {
    let mut rng = rng();
    let (ra, rb) = (generate_key(), generate_key());
    let da = generate_key();
    let a = MlsEngine::open(&da, ra.verifying_key().to_bytes(), None).unwrap();
    let b = MlsEngine::open(&generate_key(), rb.verifying_key().to_bytes(), None).unwrap();
    let group = a.create_group().unwrap();
    let (kp, _) = mls::verify_key_package(&b.key_package().unwrap()).unwrap();
    let (_commit, welcome) = a.add(&group, &[kp]).unwrap();
    let committer = da.verifying_key().to_bytes();

    // Welcomes alterados: ni pánico ni unirse a un grupo.
    for _ in 0..300 {
        let bad = mutate(&mut rng, &welcome);
        let _ = b.join(&bad, |_| false);
    }
    b.join(&welcome, |_| false).unwrap();

    let msg = a.encrypt(&group, b"secreto").unwrap();
    for _ in 0..1000 {
        let bad = mutate(&mut rng, &msg);
        let _ = mls::group_id_of(&bad);
        assert!(
            !matches!(b.process(&bad, &committer), Ok(mls::Incoming::Application { .. })),
            "mensaje MLS alterado aceptado"
        );
    }
    for _ in 0..1000 {
        let junk = random_bytes(&mut rng, 512);
        let _ = mls::group_id_of(&junk);
        let _ = mls::verify_key_package(&junk);
        let _ = b.process(&junk, &committer);
    }
    // El mensaje original sigue llegando tras toda la basura.
    match b.process(&msg, &committer).unwrap() {
        mls::Incoming::Application { body, .. } => assert_eq!(body, b"secreto"),
        _ => panic!("se esperaba el mensaje"),
    }
}
