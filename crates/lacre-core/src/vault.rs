//! Cifrado del estado en disco (D-033).
//!
//! Con una clave de datos (la guarda el almacén del sistema: Keystore,
//! Secret Service…), los archivos de estado se escriben cifrados con
//! XChaCha20-Poly1305. El nombre del archivo va como dato asociado, para que
//! no se puedan intercambiar unos por otros. Sin clave se escriben en claro,
//! y los archivos en claro se siguen leyendo (migración).
//!
//! La clave es del proceso: hay un nodo por proceso.

use std::fs;
use std::io::{self, Write};
use std::path::Path;
use std::sync::OnceLock;

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use rand::RngCore;

/// Cabecera de un archivo cifrado.
const MAGIC: &[u8; 8] = b"P2MVLT1\0";
const NONCE: usize = 24;

static KEY: OnceLock<[u8; 32]> = OnceLock::new();

/// Fija la clave de datos del proceso. Falla si ya había otra.
pub fn set_key(key: [u8; 32]) -> io::Result<()> {
    if *KEY.get_or_init(|| key) != key {
        return Err(io::Error::other("ya hay otra clave de datos"));
    }
    Ok(())
}

/// Hay clave: lo que se escriba irá cifrado.
pub fn enabled() -> bool {
    KEY.get().is_some()
}

/// El contenido está cifrado con este formato.
pub fn is_sealed(data: &[u8]) -> bool {
    data.starts_with(MAGIC)
}

fn aad(path: &Path) -> Vec<u8> {
    path.file_name().map(|n| n.as_encoded_bytes().to_vec()).unwrap_or_default()
}

fn invalid(msg: String) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg)
}

/// Cifra `plain` para guardarlo en `path` (sin clave, lo devuelve tal cual).
pub fn seal(path: &Path, plain: &[u8]) -> Vec<u8> {
    let Some(key) = KEY.get() else { return plain.to_vec() };
    let mut nonce = [0u8; NONCE];
    rand::rngs::OsRng.fill_bytes(&mut nonce);
    let ct = XChaCha20Poly1305::new(key.into())
        .encrypt(XNonce::from_slice(&nonce), Payload { msg: plain, aad: &aad(path) })
        .expect("cifrado");
    let mut out = Vec::with_capacity(MAGIC.len() + NONCE + ct.len());
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&ct);
    out
}

/// Descifra lo leído de `path`. Lo que no está cifrado se devuelve tal cual.
pub fn open(path: &Path, data: Vec<u8>) -> io::Result<Vec<u8>> {
    if !is_sealed(&data) {
        return Ok(data);
    }
    let key = KEY
        .get()
        .ok_or_else(|| invalid(format!("{} está cifrado y no hay clave de datos", path.display())))?;
    let body = &data[MAGIC.len()..];
    if body.len() < NONCE {
        return Err(invalid(format!("{} está truncado", path.display())));
    }
    let (nonce, ct) = body.split_at(NONCE);
    XChaCha20Poly1305::new(key.into())
        .decrypt(XNonce::from_slice(nonce), Payload { msg: ct, aad: &aad(path) })
        .map_err(|_| invalid(format!("no se pudo descifrar {} (¿otra clave de datos?)", path.display())))
}

/// Lee y descifra un archivo.
pub fn read(path: &Path) -> io::Result<Vec<u8>> {
    open(path, fs::read(path)?)
}

/// Como [`read`], pero `None` si el archivo no existe. Cualquier otro error
/// (también no poder descifrarlo) se devuelve: no hay que confundir un
/// archivo ilegible con uno vacío.
pub fn read_opt(path: &Path) -> io::Result<Option<Vec<u8>>> {
    match read(path) {
        Ok(b) => Ok(Some(b)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

/// Escribe cifrado y de forma atómica, solo legible por el usuario.
pub fn write(path: &Path, plain: &[u8]) -> io::Result<()> {
    let tmp = path.with_extension("tmp");
    let mut opts = fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut f = opts.open(&tmp)?;
    f.write_all(&seal(path, plain))?;
    f.sync_all()?;
    drop(f);
    fs::rename(tmp, path)
}

/// Vuelve a escribir cifrado un archivo que está en claro. Devuelve si lo
/// hizo.
pub fn migrate(path: &Path) -> io::Result<bool> {
    if !enabled() {
        return Ok(false);
    }
    let Ok(data) = fs::read(path) else { return Ok(false) };
    if is_sealed(&data) {
        return Ok(false);
    }
    write(path, &data)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seal_open_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.cbor");
        // En claro antes de tener clave; se migra después.
        write(&a, b"hola").unwrap();
        assert_eq!(fs::read(&a).unwrap(), b"hola");
        set_key([7; 32]).unwrap();
        assert!(set_key([8; 32]).is_err());
        assert!(migrate(&a).unwrap());
        let raw = fs::read(&a).unwrap();
        assert!(is_sealed(&raw) && !raw.windows(4).any(|w| w == b"hola"));
        assert_eq!(read(&a).unwrap(), b"hola");
        // Con otro nombre (dato asociado) no se descifra.
        let b = dir.path().join("b.cbor");
        fs::write(&b, &raw).unwrap();
        assert_eq!(read(&b).unwrap_err().kind(), io::ErrorKind::InvalidData);
        // Un bit cambiado tampoco.
        let mut bad = raw.clone();
        *bad.last_mut().unwrap() ^= 1;
        fs::write(&a, &bad).unwrap();
        assert!(read_opt(&a).is_err());
        assert!(read_opt(&dir.path().join("no")).unwrap().is_none());
    }
}
