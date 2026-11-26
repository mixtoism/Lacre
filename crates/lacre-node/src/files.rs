//! Archivos (D-026): se cifran en trozos con una clave aleatoria propia del
//! archivo; la clave y los hashes de los trozos cifrados viajan en un
//! mensaje MLS (la «oferta») y los trozos se piden directamente a los
//! dispositivos del grupo que ya tienen el archivo entero.
//!
//! Fuera del grupo nadie ve el nombre, el tipo ni la clave; para pedir un
//! trozo hay que presentar un testigo derivado de la clave y ser miembro
//! actual del grupo. Cada trozo se comprueba con su hash antes de
//! descifrarlo: quien lo sirve no puede colar datos.
//!
//! En disco el archivo se guarda tal como viaja: los trozos cifrados uno
//! detrás de otro (`contenido`). La clave solo está en la oferta, que va en
//! el almacén cifrado (D-033), así que una copia del disco no revela nada.
//! Para abrirlo con otra aplicación se descifra a `abiertos/`, que se vacía
//! al arrancar y al parar el nodo.

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use chacha20poly1305::aead::Aead;
use chacha20poly1305::{ChaCha20Poly1305, KeyInit, Nonce};
use lacre_core::{cbor_from, cbor_to};
use rand::RngCore;
use serde::{Deserialize, Serialize};

use crate::app::{read_private, write_private};

/// Tamaño máximo de un archivo.
pub const MAX_FILE: u64 = 100 * 1024 * 1024;
/// Tamaño de cada trozo (sin cifrar). Uno por petición: cabe de sobra en
/// los límites de request-response y por Tor llega en pocos segundos.
pub const CHUNK: usize = 256 * 1024;
/// Etiqueta de autenticación que añade el cifrado a cada trozo.
const TAG: usize = 16;

/// Lo que viaja en el mensaje MLS: basta para descargar y comprobar el
/// archivo.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct FileOffer {
    pub name: String,
    pub size: u64,
    key: [u8; 32],
    /// Hashes BLAKE3 de los trozos cifrados, concatenados (32 bytes cada uno).
    #[serde(with = "serde_bytes")]
    hashes: Vec<u8>,
}

/// Oferta guardada junto al archivo: de qué grupo es y quién lo mandó.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct StoredOffer {
    pub offer: FileOffer,
    pub group_id: Vec<u8>,
    /// Dispositivo que lo envió (primera fuente a la que preguntar).
    pub sender: [u8; 32],
}

impl FileOffer {
    /// Identificador público del archivo (no revela nada de su contenido).
    pub fn id(&self) -> String {
        hex(&blake3::derive_key("p2message 2026 file id", &self.key))
    }

    /// Testigo que hay que presentar para pedir trozos.
    pub fn token(&self) -> [u8; 32] {
        blake3::derive_key("p2message 2026 file token", &self.key)
    }

    pub fn chunks(&self) -> u32 {
        (self.hashes.len() / 32) as u32
    }

    fn hash(&self, index: u32) -> Option<&[u8]> {
        let i = index as usize * 32;
        self.hashes.get(i..i + 32)
    }

    /// Longitud sin cifrar del trozo `index`.
    fn chunk_len(&self, index: u32) -> usize {
        let start = index as u64 * CHUNK as u64;
        (self.size - start).min(CHUNK as u64) as usize
    }

    /// Posición del trozo cifrado `index` en el archivo guardado.
    fn sealed_offset(index: u32) -> u64 {
        index as u64 * (CHUNK + TAG) as u64
    }

    /// Tamaño del archivo guardado (todos los trozos cifrados).
    pub fn sealed_size(&self) -> u64 {
        self.size + self.chunks() as u64 * TAG as u64
    }

    /// Comprueba que la oferta es coherente (viene de otro dispositivo).
    pub fn check(&self) -> Result<()> {
        if self.size > MAX_FILE {
            bail!("el archivo ocupa {} MB (máximo {} MB)", self.size >> 20, MAX_FILE >> 20);
        }
        if !self.hashes.len().is_multiple_of(32) || self.chunks() as u64 != self.size.div_ceil(CHUNK as u64) {
            bail!("oferta de archivo mal formada");
        }
        if safe_name(&self.name) != self.name {
            bail!("nombre de archivo no válido");
        }
        Ok(())
    }

    fn cipher(&self) -> ChaCha20Poly1305 {
        ChaCha20Poly1305::new(&self.key.into())
    }

    /// Cifra un trozo. Con una clave aleatoria por archivo, el índice sirve
    /// de nonce sin repetirse; y es determinista, así que cualquiera que
    /// tenga el archivo entero sirve exactamente los mismos bytes.
    fn seal(&self, index: u32, plain: &[u8]) -> Vec<u8> {
        self.cipher().encrypt(&nonce(index), plain).expect("cifrado de un trozo")
    }

    /// Comprueba el hash de un trozo recibido y lo descifra.
    pub fn open(&self, index: u32, sealed: &[u8]) -> Result<Vec<u8>> {
        let expected = self.hash(index).context("trozo fuera de rango")?;
        if sealed.len() != self.chunk_len(index) + TAG || blake3::hash(sealed).as_bytes() != expected {
            bail!("trozo {index} corrupto");
        }
        self.cipher().decrypt(&nonce(index), sealed).ok().with_context(|| format!("trozo {index} corrupto"))
    }
}

fn nonce(index: u32) -> Nonce {
    let mut n = [0u8; 12];
    n[..4].copy_from_slice(&index.to_le_bytes());
    n.into()
}

fn hex(bytes: &[u8]) -> String {
    data_encoding::HEXLOWER.encode(bytes)
}

/// Nombre sin rutas ni caracteres problemáticos, para guardarlo en disco.
pub fn safe_name(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or("");
    let clean: String = base
        .chars()
        .filter(|c| !c.is_control() && !matches!(c, ':' | '*' | '?' | '"' | '<' | '>' | '|'))
        .take(120)
        .collect();
    // Windows ignora puntos y espacios finales y reserva CON, NUL, COM1…
    // (también con extensión): se evitan para que el nombre del remitente no
    // apunte a un dispositivo (D-042).
    let clean = clean.trim().trim_start_matches('.').trim_end_matches(['.', ' ']).to_owned();
    if clean.is_empty() {
        return "archivo".into();
    }
    let stem = clean.split('.').next().unwrap_or("").trim_end().to_ascii_uppercase();
    let reserved = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$")
        || (stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && matches!(stem.chars().nth(3), Some('0'..='9' | '¹' | '²' | '³')));
    if reserved { format!("_{clean}") } else { clean }
}

/// Lee hasta llenar `buf` (o hasta el final del archivo).
fn read_full(f: &mut File, buf: &mut [u8]) -> Result<usize> {
    let mut n = 0;
    while n < buf.len() {
        match f.read(&mut buf[n..])? {
            0 => break,
            k => n += k,
        }
    }
    Ok(n)
}

/// Almacén de archivos: `files/<id>/oferta.cbor` y los trozos cifrados en
/// `contenido` (o `contenido.part` mientras se descarga). El nombre del
/// archivo solo está en la oferta cifrada.
pub struct Files {
    dir: PathBuf,
    /// Ofertas ya leídas de disco.
    cache: HashMap<String, StoredOffer>,
}

impl Files {
    pub fn open(data_dir: &Path) -> Self {
        Self { dir: data_dir.join("files"), cache: HashMap::new() }
    }

    /// Carpeta de un archivo. El id viene de la red o de la interfaz: se
    /// valida para no salir de `files/`.
    fn folder(&self, id: &str) -> Result<PathBuf> {
        if id.len() != 64 || !id.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) {
            bail!("archivo desconocido");
        }
        Ok(self.dir.join(id))
    }

    pub fn get(&mut self, id: &str) -> Result<StoredOffer> {
        if let Some(s) = self.cache.get(id) {
            return Ok(s.clone());
        }
        let bytes = read_private(&self.folder(id)?.join("oferta.cbor"))?.context("archivo desconocido")?;
        let stored: StoredOffer = cbor_from(&bytes)?;
        self.cache.insert(id.to_owned(), stored.clone());
        Ok(stored)
    }

    /// Guarda la oferta de un archivo recibido (sin descargarlo).
    pub fn register(&mut self, stored: StoredOffer) -> Result<String> {
        stored.offer.check()?;
        let id = stored.offer.id();
        let folder = self.folder(&id)?;
        if !folder.join("oferta.cbor").exists() {
            fs::create_dir_all(&folder)?;
            write_private(&folder.join("oferta.cbor"), &cbor_to(&stored)?)?;
        }
        self.cache.insert(id.clone(), stored);
        Ok(id)
    }

    /// Ruta del archivo completo (cifrado), y de la descarga a medias.
    pub fn path(&mut self, id: &str) -> Result<(PathBuf, PathBuf)> {
        self.get(id)?;
        let folder = self.folder(id)?;
        Ok((folder.join(CONTENT), folder.join(PART)))
    }

    /// Descifra un archivo completo en `out/<id>/<nombre>` para abrirlo con
    /// otra aplicación y devuelve la ruta.
    pub fn export(&mut self, id: &str, out: &Path) -> Result<PathBuf> {
        let offer = self.get(id)?.offer;
        let (done, _) = self.path(id)?;
        let mut src = File::open(&done).context("el archivo no está descargado")?;
        let folder = out.join(id);
        fs::create_dir_all(&folder)?;
        let dest = folder.join(&offer.name);
        let tmp = folder.join(".descifrando");
        let mut opts = fs::OpenOptions::new();
        opts.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        let mut f = opts.open(&tmp)?;
        let mut buf = vec![0; CHUNK + TAG];
        for index in 0..offer.chunks() {
            let len = offer.chunk_len(index) + TAG;
            if read_full(&mut src, &mut buf[..len])? != len {
                bail!("archivo local incompleto");
            }
            f.write_all(&offer.open(index, &buf[..len])?)?;
        }
        f.sync_all()?;
        drop(f);
        fs::rename(&tmp, &dest)?;
        Ok(dest)
    }

    /// El archivo está entero en este dispositivo.
    pub fn complete(&mut self, id: &str) -> bool {
        self.path(id).is_ok_and(|(done, _)| done.exists())
    }

    /// Trozo cifrado para servirlo a otro miembro del grupo.
    pub fn serve(&mut self, id: &str, index: u32, token: &[u8; 32]) -> Result<Vec<u8>> {
        let offer = self.get(id)?.offer;
        // Comparación en tiempo constante.
        if blake3::Hash::from(offer.token()) != blake3::Hash::from(*token) {
            bail!("archivo desconocido");
        }
        if index >= offer.chunks() {
            bail!("trozo fuera de rango");
        }
        let (done, _) = self.path(id)?;
        let mut f = File::open(&done).context("no tengo ese archivo")?;
        let mut buf = vec![0; offer.chunk_len(index) + TAG];
        f.seek(SeekFrom::Start(FileOffer::sealed_offset(index)))?;
        if read_full(&mut f, &mut buf)? != buf.len() {
            bail!("archivo local incompleto");
        }
        // Lo guardado ya es el trozo cifrado; se comprueba por si el disco
        // se corrompió.
        if Some(blake3::hash(&buf).as_bytes().as_slice()) != offer.hash(index) {
            bail!("archivo local corrupto");
        }
        Ok(buf)
    }

    /// Pasa al formato cifrado los archivos guardados en claro por versiones
    /// anteriores (`<nombre>` en lugar de `contenido`). Devuelve cuántos.
    pub fn migrate(&mut self) -> Result<usize> {
        let Ok(entries) = fs::read_dir(&self.dir) else { return Ok(0) };
        let mut n = 0;
        for e in entries.flatten() {
            let id = e.file_name().to_string_lossy().into_owned();
            let Ok(stored) = self.get(&id) else { continue };
            let folder = self.folder(&id)?;
            let name = &stored.offer.name;
            let _ = fs::remove_file(folder.join(format!("{name}.part")));
            let old = folder.join(name);
            let Ok(meta) = fs::metadata(&old) else { continue };
            // Un archivo llamado «contenido» ya puede estar cifrado: en claro
            // mide exactamente lo que dice la oferta.
            if !meta.is_file() || (name == CONTENT && (stored.offer.chunks() == 0 || meta.len() != stored.offer.size)) {
                continue;
            }
            seal_file(&old, &folder.join(CONTENT), &stored.offer)?;
            if name != CONTENT {
                fs::remove_file(&old)?;
            }
            n += 1;
        }
        Ok(n)
    }
}

/// Nombre del archivo cifrado completo dentro de su carpeta.
const CONTENT: &str = "contenido";
/// Descarga a medias.
const PART: &str = "contenido.part";

/// Cifra `src` (en claro) trozo a trozo en `dest` y comprueba que coincide
/// con los hashes de la oferta. Devuelve los hashes calculados.
fn seal_into(src: &mut File, dest: &Path, offer: &FileOffer, size: u64) -> Result<Vec<u8>> {
    let tmp = dest.with_extension("tmp");
    let mut out = File::create(&tmp)?;
    let mut buf = vec![0; CHUNK];
    let mut total = 0u64;
    let mut hashes = Vec::new();
    for index in 0.. {
        let n = read_full(src, &mut buf)?;
        if n == 0 {
            break;
        }
        total += n as u64;
        if total > size {
            bail!("el archivo cambió mientras se leía");
        }
        let sealed = offer.seal(index, &buf[..n]);
        hashes.extend_from_slice(blake3::hash(&sealed).as_bytes());
        out.write_all(&sealed)?;
    }
    if total != size {
        bail!("el archivo cambió mientras se leía");
    }
    out.sync_all()?;
    drop(out);
    fs::rename(&tmp, dest)?;
    Ok(hashes)
}

/// Migración: cifra un archivo en claro ya conocido.
fn seal_file(src: &Path, dest: &Path, offer: &FileOffer) -> Result<()> {
    let hashes = seal_into(&mut File::open(src)?, dest, offer, offer.size)?;
    if hashes != offer.hashes {
        let _ = fs::remove_file(dest);
        bail!("{} no coincide con su oferta", src.display());
    }
    Ok(())
}

/// Prepara el envío de un archivo: lo cifra para calcular los hashes y lo
/// copia al almacén (el original puede borrarse después). Lento con
/// archivos grandes: llamar fuera del runtime asíncrono.
pub fn prepare(files_dir: &Path, path: &Path) -> Result<FileOffer> {
    let size = fs::metadata(path).with_context(|| format!("no se pudo leer {}", path.display()))?.len();
    if size > MAX_FILE {
        bail!("el archivo ocupa {} MB (máximo {} MB)", size >> 20, MAX_FILE >> 20);
    }
    let name = safe_name(&path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default());
    let mut key = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut key);
    let mut offer = FileOffer { name, size, key, hashes: Vec::new() };
    let folder = files_dir.join(offer.id());
    fs::create_dir_all(&folder)?;
    offer.hashes = seal_into(&mut File::open(path)?, &folder.join(CONTENT), &offer, size)?;
    Ok(offer)
}

/// Descarga a medias: escribe los trozos cifrados ya comprobados en
/// `contenido.part`.
pub struct Partial {
    file: File,
    offer: FileOffer,
}

impl Partial {
    /// Abre (o crea) la descarga y devuelve los trozos que faltan. Los que
    /// ya están se reconocen por su hash, sin guardar aparte qué se
    /// descargó.
    pub fn open(part: &Path, offer: FileOffer) -> Result<(Self, Vec<u32>)> {
        let mut opts = fs::OpenOptions::new();
        opts.read(true).write(true).create(true).truncate(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        let mut file = opts.open(part)?;
        let have = file.metadata()?.len();
        let mut missing = Vec::new();
        let mut buf = vec![0; CHUNK + TAG];
        for index in 0..offer.chunks() {
            let len = offer.chunk_len(index) + TAG;
            let start = FileOffer::sealed_offset(index);
            let ok = start + len as u64 <= have && {
                file.seek(SeekFrom::Start(start))?;
                read_full(&mut file, &mut buf[..len])? == len
                    && Some(blake3::hash(&buf[..len]).as_bytes().as_slice()) == offer.hash(index)
            };
            if !ok {
                missing.push(index);
            }
        }
        file.set_len(offer.sealed_size())?;
        Ok((Self { file, offer }, missing))
    }

    /// Comprueba un trozo recibido (hash y descifrado) y lo guarda cifrado.
    pub fn write(&mut self, index: u32, sealed: &[u8]) -> Result<()> {
        self.offer.open(index, sealed)?;
        self.file.seek(SeekFrom::Start(FileOffer::sealed_offset(index)))?;
        self.file.write_all(sealed)?;
        Ok(())
    }

    pub fn finish(self, part: &Path, done: &Path) -> Result<()> {
        self.file.sync_all()?;
        drop(self.file);
        fs::rename(part, done)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("lacre-files-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn names() {
        assert_eq!(safe_name("../../etc/passwd"), "passwd");
        assert_eq!(safe_name("C:\\x\\informe.pdf"), "informe.pdf");
        assert_eq!(safe_name(".bashrc"), "bashrc");
        assert_eq!(safe_name(""), "archivo");
        assert_eq!(safe_name("a\u{0}b?.txt"), "ab.txt");
        assert_eq!(safe_name("con.txt"), "_con.txt");
        assert_eq!(safe_name("COM1"), "_COM1");
        assert_eq!(safe_name("lpt9.tar.gz"), "_lpt9.tar.gz");
        assert_eq!(safe_name("informe. . "), "informe");
        assert_eq!(safe_name("console.txt"), "console.txt");
    }

    #[test]
    fn send_serve_and_download() {
        let dir = tmp("ok");
        let src = dir.join("datos.bin");
        let data: Vec<u8> = (0..(CHUNK * 2 + 1000)).map(|i| (i * 7 % 251) as u8).collect();
        fs::write(&src, &data).unwrap();

        // Emisor.
        let a = dir.join("a");
        let offer = prepare(&a.join("files"), &src).unwrap();
        assert_eq!(offer.chunks(), 3);
        offer.check().unwrap();
        let id = offer.id();
        let mut fa = Files::open(&a);
        fa.register(StoredOffer { offer: offer.clone(), group_id: vec![1], sender: [0; 32] }).unwrap();
        assert!(fa.complete(&id));

        // Sin el testigo no se sirve nada.
        assert!(fa.serve(&id, 0, &[0; 32]).is_err());

        // Receptor: descarga a medias, se interrumpe y sigue.
        let b = dir.join("b");
        let mut fb = Files::open(&b);
        fb.register(StoredOffer { offer: offer.clone(), group_id: vec![1], sender: [0; 32] }).unwrap();
        let (done, part) = fb.path(&id).unwrap();
        let (mut p, missing) = Partial::open(&part, offer.clone()).unwrap();
        assert_eq!(missing, [0, 1, 2]);
        p.write(1, &fa.serve(&id, 1, &offer.token()).unwrap()).unwrap();
        // Un trozo cambiado se rechaza.
        let mut bad = fa.serve(&id, 0, &offer.token()).unwrap();
        bad[0] ^= 1;
        assert!(p.write(0, &bad).is_err());
        assert!(p.write(0, &fa.serve(&id, 2, &offer.token()).unwrap()).is_err());
        drop(p);
        let (mut p, missing) = Partial::open(&part, offer.clone()).unwrap();
        assert_eq!(missing, [0, 2]);
        for i in missing {
            p.write(i, &fa.serve(&id, i, &offer.token()).unwrap()).unwrap();
        }
        p.finish(&part, &done).unwrap();
        // En disco solo hay trozos cifrados; al exportar sale el original.
        let raw = fs::read(&done).unwrap();
        assert_eq!(raw.len() as u64, offer.sealed_size());
        assert!(!raw.windows(64).any(|w| data.windows(64).next() == Some(w)));
        let out = fb.export(&id, &dir.join("abiertos")).unwrap();
        assert_eq!(out.file_name().unwrap(), "datos.bin");
        assert_eq!(fs::read(&out).unwrap(), data);
        assert!(fb.complete(&id));
        // El emisor tampoco guarda la copia en claro.
        assert_eq!(fs::read(a.join("files").join(&id).join(CONTENT)).unwrap(), raw);

        // Migración de un archivo guardado en claro (versión anterior).
        let c = dir.join("c");
        let mut fc = Files::open(&c);
        fc.register(StoredOffer { offer: offer.clone(), group_id: vec![1], sender: [0; 32] }).unwrap();
        fs::write(c.join("files").join(&id).join("datos.bin"), &data).unwrap();
        assert_eq!(fc.migrate().unwrap(), 1);
        assert_eq!(fs::read(c.join("files").join(&id).join(CONTENT)).unwrap(), raw);
        assert!(!c.join("files").join(&id).join("datos.bin").exists());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn limits() {
        let dir = tmp("lim");
        let empty = dir.join("vacío.txt");
        fs::write(&empty, b"").unwrap();
        let offer = prepare(&dir.join("files"), &empty).unwrap();
        assert_eq!(offer.chunks(), 0);
        offer.check().unwrap();
        let mut big = offer.clone();
        big.size = MAX_FILE + 1;
        assert!(big.check().is_err());
        let mut lying = offer.clone();
        lying.size = 10;
        assert!(lying.check().is_err());
        let mut evil = offer;
        evil.name = "../x".into();
        assert!(evil.check().is_err());
        fs::remove_dir_all(&dir).unwrap();
    }
}
