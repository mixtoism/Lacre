//! Cifrado del estado en disco (D-033, D-042).
//!
//! Con una clave de datos (la guarda el almacén del sistema: Keystore,
//! Secret Service…), los archivos de estado se escriben cifrados con
//! XChaCha20-Poly1305. La ruta relativa al directorio de datos va como dato
//! asociado, para que no se puedan intercambiar unos por otros (en la v1
//! solo iba el nombre del archivo, y dos `oferta.cbor` de carpetas
//! distintas eran intercambiables). Sin clave se escriben en claro.
//!
//! Con clave, leer un archivo en claro o en formato v1 es un error: solo
//! [`migrate`] los acepta, para reescribirlos en v2. Así nadie puede colar
//! un archivo en claro en lugar de uno cifrado.
//!
//! La clave es del proceso: hay un nodo por proceso.

use std::fs;
use std::io::{self, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::OnceLock;

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use rand::RngCore;

/// Cabeceras de un archivo cifrado: v1 (dato asociado = nombre) y v2
/// (dato asociado = ruta relativa al directorio de datos).
const MAGIC_V1: &[u8; 8] = b"P2MVLT1\0";
const MAGIC: &[u8; 8] = b"P2MVLT2\0";
const AAD_V2: &[u8] = b"p2message/vault/v2\0";
const NONCE: usize = 24;

/// Clave de datos que comparten las pruebas del crate (la clave es global
/// del proceso y las pruebas corren en paralelo).
#[cfg(test)]
pub(crate) const TEST_KEY: [u8; 32] = [7; 32];

static KEY: OnceLock<[u8; 32]> = OnceLock::new();
static ROOT: OnceLock<PathBuf> = OnceLock::new();

/// Fija la clave de datos del proceso. Falla si ya había otra.
pub fn set_key(key: [u8; 32]) -> io::Result<()> {
    if *KEY.get_or_init(|| key) != key {
        return Err(io::Error::other("ya hay otra clave de datos"));
    }
    Ok(())
}

/// Fija el directorio de datos, respecto al que se calculan las rutas que
/// van como dato asociado. Sin él se usa la ruta tal cual.
pub fn set_root(dir: &Path) -> io::Result<()> {
    if ROOT.get_or_init(|| dir.to_path_buf()) != dir {
        return Err(io::Error::other("ya hay otro directorio de datos"));
    }
    Ok(())
}

/// Hay clave: lo que se escriba irá cifrado.
pub fn enabled() -> bool {
    KEY.get().is_some()
}

/// El contenido está cifrado con alguno de los formatos.
pub fn is_sealed(data: &[u8]) -> bool {
    data.starts_with(MAGIC) || data.starts_with(MAGIC_V1)
}

fn aad_v1(path: &Path) -> Vec<u8> {
    path.file_name().map(|n| n.as_encoded_bytes().to_vec()).unwrap_or_default()
}

/// Ruta relativa al directorio de datos, con `/` en todos los sistemas.
fn aad(path: &Path) -> Vec<u8> {
    let rel = ROOT.get().and_then(|r| path.strip_prefix(r).ok()).unwrap_or(path);
    let parts: Vec<&[u8]> = rel
        .components()
        .filter_map(|c| match c {
            Component::Normal(n) => Some(n.as_encoded_bytes()),
            _ => None,
        })
        .collect();
    [AAD_V2, &parts.join(&b'/')].concat()
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

/// Descifra lo leído de `path`. Sin clave, lo que no está cifrado se
/// devuelve tal cual; con clave, solo vale el formato v2.
pub fn open(path: &Path, data: Vec<u8>) -> io::Result<Vec<u8>> {
    open_with(path, data, false)
}

fn open_with(path: &Path, data: Vec<u8>, legacy: bool) -> io::Result<Vec<u8>> {
    let v2 = data.starts_with(MAGIC);
    if !v2 && !data.starts_with(MAGIC_V1) {
        if enabled() && !legacy {
            return Err(invalid(format!("{} está en claro y debería estar cifrado", path.display())));
        }
        return Ok(data);
    }
    if !v2 && !legacy {
        return Err(invalid(format!("{} tiene el formato cifrado antiguo sin migrar", path.display())));
    }
    let key = KEY
        .get()
        .ok_or_else(|| invalid(format!("{} está cifrado y no hay clave de datos", path.display())))?;
    let body = &data[MAGIC.len()..];
    if body.len() < NONCE {
        return Err(invalid(format!("{} está truncado", path.display())));
    }
    let (nonce, ct) = body.split_at(NONCE);
    let aad = if v2 { aad(path) } else { aad_v1(path) };
    XChaCha20Poly1305::new(key.into())
        .decrypt(XNonce::from_slice(nonce), Payload { msg: ct, aad: &aad })
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

/// Vuelve a escribir en v2 un archivo que está en claro o en v1. Devuelve
/// si lo hizo.
pub fn migrate(path: &Path) -> io::Result<bool> {
    if !enabled() {
        return Ok(false);
    }
    let Ok(data) = fs::read(path) else { return Ok(false) };
    if data.starts_with(MAGIC) {
        return Ok(false);
    }
    let plain = open_with(path, data, true)?;
    write(path, &plain)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seal_open_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.cbor");
        // Un archivo en claro de antes de tener clave; se migra después.
        // (Otra prueba puede haber fijado ya la clave: se escribe a mano.)
        fs::write(&a, b"hola").unwrap();
        set_key(TEST_KEY).unwrap();
        assert!(set_key([8; 32]).is_err());
        // Con clave, en claro no se acepta hasta migrarlo.
        assert_eq!(read(&a).unwrap_err().kind(), io::ErrorKind::InvalidData);
        assert!(migrate(&a).unwrap());
        assert!(!migrate(&a).unwrap());
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
        // El mismo nombre en otra carpeta tampoco (v2).
        write(&a, b"hola").unwrap();
        let sub = dir.path().join("sub");
        fs::create_dir_all(&sub).unwrap();
        fs::copy(&a, sub.join("a.cbor")).unwrap();
        assert!(read(&sub.join("a.cbor")).is_err());
        // Un v1 se lee solo para migrarlo.
        let v1 = dir.path().join("v1.cbor");
        let mut nonce = [0u8; NONCE];
        rand::rngs::OsRng.fill_bytes(&mut nonce);
        let ct = XChaCha20Poly1305::new((&[7u8; 32]).into())
            .encrypt(XNonce::from_slice(&nonce), Payload { msg: b"viejo".as_slice(), aad: &aad_v1(&v1) })
            .unwrap();
        fs::write(&v1, [MAGIC_V1.as_slice(), &nonce, &ct].concat()).unwrap();
        assert!(read(&v1).is_err());
        assert!(migrate(&v1).unwrap());
        assert_eq!(read(&v1).unwrap(), b"viejo");
    }
}
