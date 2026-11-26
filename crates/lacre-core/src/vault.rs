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
//!
//! Vuelta atrás (D-046): los archivos de estado importantes (claves, chats,
//! contactos, bloqueos…) llevan un número de guardado, autenticado con el
//! resto, y `vault.manifest` guarda el último de cada uno. Al leer, el
//! número tiene que coincidir: una copia antigua de un archivo suelto, o
//! su ausencia, se rechaza. Devolver todo el directorio, registro incluido,
//! a una copia anterior coherente no se puede detectar sin un contador del
//! hardware.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{self, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use rand::RngCore;
use serde::{Deserialize, Serialize};

/// Cabeceras de un archivo cifrado: v1 (dato asociado = nombre) y v2
/// (dato asociado = ruta relativa al directorio de datos).
const MAGIC_V1: &[u8; 8] = b"P2MVLT1\0";
const MAGIC: &[u8; 8] = b"P2MVLT2\0";
const AAD_V2: &[u8] = b"p2message/vault/v2\0";
/// v3: con número de guardado, para los archivos protegidos (D-046).
const MAGIC_V3: &[u8; 8] = b"P2MVLT3\0";
const AAD_V3: &[u8] = b"p2message/vault/v3\0";
const NONCE: usize = 24;
/// Registro de números de guardado, en el directorio de datos.
pub const MANIFEST: &str = "vault.manifest";

/// Clave de datos que comparten las pruebas del crate (la clave es global
/// del proceso y las pruebas corren en paralelo).
#[cfg(test)]
pub(crate) const TEST_KEY: [u8; 32] = [7; 32];

static KEY: OnceLock<[u8; 32]> = OnceLock::new();
static ROOT: OnceLock<PathBuf> = OnceLock::new();
static LEDGER: Mutex<Option<Ledger>> = Mutex::new(None);

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
    data.starts_with(MAGIC) || data.starts_with(MAGIC_V1) || data.starts_with(MAGIC_V3)
}

fn aad_v1(path: &Path) -> Vec<u8> {
    path.file_name().map(|n| n.as_encoded_bytes().to_vec()).unwrap_or_default()
}

/// `path` relativa a `root`, con `/` en todos los sistemas.
fn rel_to(root: Option<&Path>, path: &Path) -> Vec<u8> {
    let rel = root.and_then(|r| path.strip_prefix(r).ok()).unwrap_or(path);
    let parts: Vec<&[u8]> = rel
        .components()
        .filter_map(|c| match c {
            Component::Normal(n) => Some(n.as_encoded_bytes()),
            _ => None,
        })
        .collect();
    parts.join(&b'/')
}

/// Ruta relativa al directorio de datos, con `/` en todos los sistemas.
fn aad(path: &Path) -> Vec<u8> {
    [AAD_V2, &rel_to(ROOT.get().map(PathBuf::as_path), path)].concat()
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
    if data.starts_with(MAGIC_V3) {
        return Err(invalid(format!("{} tiene número de guardado y no es un archivo protegido", path.display())));
    }
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
    if let (Some(key), Some(ledger)) = (KEY.get(), LEDGER.lock().unwrap().as_mut())
        && ledger.covers(path)
    {
        return ledger.read(key, path);
    }
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
    if let (Some(key), Some(ledger)) = (KEY.get(), LEDGER.lock().unwrap().as_mut())
        && ledger.covers(path)
    {
        return ledger.write(key, path, plain);
    }
    write_raw(path, &seal(path, plain))
}

/// Escritura atómica (temporal + `fsync` + renombrar), modo `0600`.
fn write_raw(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let tmp = path.with_extension("tmp");
    let mut opts = fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut f = opts.open(&tmp)?;
    f.write_all(bytes)?;
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
    if data.starts_with(MAGIC) || data.starts_with(MAGIC_V3) {
        return Ok(false);
    }
    let plain = open_with(path, data, true)?;
    write(path, &plain)?;
    Ok(true)
}

/// Activa la protección contra la vuelta atrás (D-046) para `names`,
/// relativos a `dir` (el directorio de datos). Hace falta la clave de datos.
/// Los archivos protegidos que aún estén en v2 se reescriben en v3.
///
/// Con `accept_current`, el registro se rehace con lo que haya en el disco
/// (para recuperarse a sabiendas de un aviso de vuelta atrás, por ejemplo
/// tras restaurar una copia de seguridad propia).
pub fn protect(dir: &Path, names: &[&str], accept_current: bool) -> io::Result<()> {
    let key = KEY.get().ok_or_else(|| io::Error::other("sin clave de datos no hay protección"))?;
    let mut ledger = Ledger::open(key, dir, names)?;
    if accept_current {
        ledger.rebuild(key)?;
    }
    ledger.adopt(key)?;
    ledger.check(key)?;
    *LEDGER.lock().unwrap() = Some(ledger);
    Ok(())
}

#[derive(Serialize, Deserialize, Default)]
struct Manifest {
    /// Último número de guardado usado.
    last: u64,
    /// Número de guardado de cada archivo protegido, por ruta relativa.
    files: BTreeMap<String, u64>,
}

/// Registro de números de guardado de un directorio de datos (D-046).
struct Ledger {
    dir: PathBuf,
    protected: BTreeSet<String>,
    manifest: Manifest,
}

fn v3_aad(rel: &str, generation: u64) -> Vec<u8> {
    [AAD_V3, &generation.to_be_bytes(), rel.as_bytes()].concat()
}

fn rollback(rel: &str, why: &str) -> io::Error {
    invalid(format!("{rel}: {why} (¿se ha devuelto a una copia anterior?)"))
}

impl Ledger {
    fn open(key: &[u8; 32], dir: &Path, names: &[&str]) -> io::Result<Self> {
        let path = dir.join(MANIFEST);
        let manifest = match fs::read(&path) {
            Ok(data) => {
                let plain = Self::open_v2(key, dir, &path, &data)?;
                crate::cbor_from(&plain).map_err(|e| invalid(format!("{MANIFEST}: {e}")))?
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => Manifest::default(),
            Err(e) => return Err(e),
        };
        Ok(Self { dir: dir.to_path_buf(), protected: names.iter().map(|n| (*n).to_owned()).collect(), manifest })
    }

    /// Descifra un v2 con la ruta relativa a `dir` como dato asociado.
    fn open_v2(key: &[u8; 32], dir: &Path, path: &Path, data: &[u8]) -> io::Result<Vec<u8>> {
        let name = path.display();
        let body = data.strip_prefix(MAGIC.as_slice()).ok_or_else(|| invalid(format!("{name} no está cifrado")))?;
        if body.len() < NONCE {
            return Err(invalid(format!("{name} está truncado")));
        }
        let (nonce, ct) = body.split_at(NONCE);
        let aad = [AAD_V2, &rel_to(Some(dir), path)].concat();
        XChaCha20Poly1305::new(key.into())
            .decrypt(XNonce::from_slice(nonce), Payload { msg: ct, aad: &aad })
            .map_err(|_| invalid(format!("no se pudo descifrar {name} (¿otra clave de datos?)")))
    }

    fn save_manifest(&self, key: &[u8; 32]) -> io::Result<()> {
        let path = self.dir.join(MANIFEST);
        let plain = crate::cbor_to(&self.manifest).map_err(|e| invalid(e.to_string()))?;
        let aad = [AAD_V2, &rel_to(Some(&self.dir), &path)].concat();
        let mut nonce = [0u8; NONCE];
        rand::rngs::OsRng.fill_bytes(&mut nonce);
        let ct = XChaCha20Poly1305::new(key.into())
            .encrypt(XNonce::from_slice(&nonce), Payload { msg: &plain, aad: &aad })
            .expect("cifrado");
        write_raw(&path, &[MAGIC.as_slice(), &nonce, &ct].concat())
    }

    fn rel(&self, path: &Path) -> Option<String> {
        path.strip_prefix(&self.dir).ok()?;
        String::from_utf8(rel_to(Some(&self.dir), path)).ok()
    }

    fn covers(&self, path: &Path) -> bool {
        self.rel(path).is_some_and(|r| self.protected.contains(&r))
    }

    /// Rehace el registro con los números de los archivos que hay.
    fn rebuild(&mut self, key: &[u8; 32]) -> io::Result<()> {
        self.manifest.files.clear();
        for name in self.protected.clone() {
            let path = self.dir.join(&name);
            let Ok(data) = fs::read(&path) else { continue };
            if let Some((generation, _)) = Self::open_v3(key, &name, &data)? {
                self.manifest.files.insert(name, generation);
                self.manifest.last = self.manifest.last.max(generation);
            }
        }
        self.save_manifest(key)
    }

    /// Lee todos los protegidos, para avisar de una vuelta atrás al arrancar
    /// y no a mitad de uso.
    fn check(&mut self, key: &[u8; 32]) -> io::Result<()> {
        for name in self.protected.clone() {
            match self.read(key, &self.dir.join(&name)) {
                Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                r => {
                    r?;
                }
            }
        }
        Ok(())
    }

    /// Descifra un v3: `None` si no es v3.
    fn open_v3(key: &[u8; 32], rel: &str, data: &[u8]) -> io::Result<Option<(u64, Vec<u8>)>> {
        let Some(body) = data.strip_prefix(MAGIC_V3.as_slice()) else { return Ok(None) };
        if body.len() < 8 + NONCE {
            return Err(invalid(format!("{rel} está truncado")));
        }
        let (generation, rest) = body.split_at(8);
        let generation = u64::from_be_bytes(generation.try_into().unwrap());
        let (nonce, ct) = rest.split_at(NONCE);
        let plain = XChaCha20Poly1305::new(key.into())
            .decrypt(XNonce::from_slice(nonce), Payload { msg: ct, aad: &v3_aad(rel, generation) })
            .map_err(|_| invalid(format!("no se pudo descifrar {rel} (¿otra clave de datos?)")))?;
        Ok(Some((generation, plain)))
    }

    /// Reescribe en v3 los protegidos que estén en v2 (de antes de D-046).
    fn adopt(&mut self, key: &[u8; 32]) -> io::Result<()> {
        for name in self.protected.clone() {
            let path = self.dir.join(&name);
            let Ok(data) = fs::read(&path) else { continue };
            if data.starts_with(MAGIC) && !self.manifest.files.contains_key(&name) {
                let plain = Self::open_v2(key, &self.dir, &path, &data)?;
                self.write(key, &path, &plain)?;
            }
        }
        Ok(())
    }

    fn write(&mut self, key: &[u8; 32], path: &Path, plain: &[u8]) -> io::Result<()> {
        let rel = self.rel(path).ok_or_else(|| invalid(format!("{} fuera del directorio", path.display())))?;
        let generation = self.manifest.last + 1;
        let mut nonce = [0u8; NONCE];
        rand::rngs::OsRng.fill_bytes(&mut nonce);
        let ct = XChaCha20Poly1305::new(key.into())
            .encrypt(XNonce::from_slice(&nonce), Payload { msg: plain, aad: &v3_aad(&rel, generation) })
            .expect("cifrado");
        // Primero el archivo y luego el registro: si se corta entre los dos,
        // el archivo lleva justo el número siguiente y se acepta al leer.
        write_raw(path, &[MAGIC_V3.as_slice(), &generation.to_be_bytes(), &nonce, &ct].concat())?;
        self.manifest.last = generation;
        self.manifest.files.insert(rel, generation);
        self.save_manifest(key)
    }

    fn read(&mut self, key: &[u8; 32], path: &Path) -> io::Result<Vec<u8>> {
        let rel = self.rel(path).ok_or_else(|| invalid(format!("{} fuera del directorio", path.display())))?;
        let expected = self.manifest.files.get(&rel).copied();
        let data = match fs::read(path) {
            Ok(d) => d,
            Err(e) if e.kind() == io::ErrorKind::NotFound && expected.is_some() => {
                return Err(rollback(&rel, "falta un archivo que había guardado"));
            }
            Err(e) => return Err(e),
        };
        let Some((generation, plain)) = Self::open_v3(key, &rel, &data)? else {
            if expected.is_some() {
                return Err(rollback(&rel, "no lleva número de guardado"));
            }
            // De antes de D-046 y aún sin adoptar.
            return Self::open_v2(key, &self.dir, path, &data);
        };
        if Some(generation) != expected {
            // Solo vale el número siguiente al último: un corte entre el
            // archivo y el registro. Se completa el registro.
            if generation != self.manifest.last + 1 {
                return Err(rollback(&rel, "el número de guardado no coincide"));
            }
            self.manifest.last = generation;
            self.manifest.files.insert(rel, generation);
            self.save_manifest(key)?;
        }
        Ok(plain)
    }
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

    fn is_rollback(e: io::Error) -> bool {
        e.kind() == io::ErrorKind::InvalidData && e.to_string().contains("copia anterior")
    }

    #[test]
    fn ledger_rejects_rollback() {
        let key = &TEST_KEY;
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        let (a, b) = (d.join("a.cbor"), d.join("b.cbor"));
        let open = || Ledger::open(key, d, &["a.cbor", "b.cbor"]).unwrap();
        let mut l = open();
        l.write(key, &a, b"a1").unwrap();
        l.write(key, &b, b"b1").unwrap();
        let old_a = fs::read(&a).unwrap();
        let snapshot = (fs::read(&b).unwrap(), fs::read(d.join(MANIFEST)).unwrap());
        l.write(key, &a, b"a2").unwrap();
        l.write(key, &b, b"b2").unwrap();
        l.write(key, &a, b"a3").unwrap();
        assert_eq!(open().read(key, &a).unwrap(), b"a3");

        // Una copia vieja de un archivo suelto: rechazada.
        let new_a = fs::read(&a).unwrap();
        fs::write(&a, &old_a).unwrap();
        assert!(is_rollback(open().read(key, &a).unwrap_err()));
        fs::write(&a, &new_a).unwrap();

        // Borrarlo: rechazado (no es lo mismo que no haberlo creado nunca).
        fs::remove_file(&a).unwrap();
        assert!(is_rollback(open().read(key, &a).unwrap_err()));
        assert!(is_rollback(open().check(key).unwrap_err()));
        fs::write(&a, &new_a).unwrap();

        // Volver el registro y un archivo a una copia anterior, dejando el
        // otro nuevo: el nuevo lleva un número que el registro viejo no
        // admite. (Con un solo guardado desde la copia no se distinguiría
        // de un corte: es el límite aceptado.)
        fs::write(&b, &snapshot.0).unwrap();
        fs::write(d.join(MANIFEST), &snapshot.1).unwrap();
        assert!(is_rollback(open().check(key).unwrap_err()));

        // A sabiendas, se rehace el registro con lo que hay.
        let mut l = open();
        l.rebuild(key).unwrap();
        l.check(key).unwrap();
        assert_eq!(open().read(key, &b).unwrap(), b"b1");
    }

    #[test]
    fn ledger_survives_interrupted_write_and_adopts_v2() {
        let key = &TEST_KEY;
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        let a = d.join("a.cbor");
        let mut l = Ledger::open(key, d, &["a.cbor"]).unwrap();
        l.write(key, &a, b"uno").unwrap();
        let manifest = fs::read(d.join(MANIFEST)).unwrap();
        l.write(key, &a, b"dos").unwrap();
        // Corte entre el archivo y el registro: vale (es el número siguiente).
        fs::write(d.join(MANIFEST), &manifest).unwrap();
        let mut l = Ledger::open(key, d, &["a.cbor"]).unwrap();
        assert_eq!(l.read(key, &a).unwrap(), b"dos");
        l.check(key).unwrap();

        // Un archivo protegido en v2 (de antes de D-046) se adopta en v3.
        let b = d.join("b.cbor");
        let mut nonce = [0u8; NONCE];
        rand::rngs::OsRng.fill_bytes(&mut nonce);
        let aad = [AAD_V2, b"b.cbor".as_slice()].concat();
        let ct = XChaCha20Poly1305::new(key.into())
            .encrypt(XNonce::from_slice(&nonce), Payload { msg: b"antiguo".as_slice(), aad: &aad })
            .unwrap();
        let v2 = [MAGIC.as_slice(), &nonce, &ct].concat();
        fs::write(&b, &v2).unwrap();
        let mut l = Ledger::open(key, d, &["a.cbor", "b.cbor"]).unwrap();
        l.adopt(key).unwrap();
        assert!(fs::read(&b).unwrap().starts_with(MAGIC_V3));
        assert_eq!(l.read(key, &b).unwrap(), b"antiguo");
        // Y volver a poner la copia v2 ya no vale.
        fs::write(&b, &v2).unwrap();
        assert!(is_rollback(Ledger::open(key, d, &["a.cbor", "b.cbor"]).unwrap().read(key, &b).unwrap_err()));
    }
}
