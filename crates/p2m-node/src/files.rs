//! Archivos (D-026): se cifran en trozos con una clave aleatoria propia del
//! archivo; la clave y los hashes de los trozos cifrados viajan en un
//! mensaje MLS (la «oferta») y los trozos se piden directamente a los
//! dispositivos del grupo que ya tienen el archivo entero.
//!
//! Fuera del grupo nadie ve el nombre, el tipo ni la clave; para pedir un
//! trozo hay que presentar un testigo derivado de la clave y ser miembro
//! actual del grupo. Cada trozo se comprueba con su hash antes de
//! descifrarlo: quien lo sirve no puede colar datos.

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use chacha20poly1305::aead::Aead;
use chacha20poly1305::{ChaCha20Poly1305, KeyInit, Nonce};
use p2m_core::{cbor_from, cbor_to};
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

    /// Comprueba que la oferta es coherente (viene de otro dispositivo).
    pub fn check(&self) -> Result<()> {
        if self.size > MAX_FILE {
            bail!("el archivo ocupa {} MB (máximo {} MB)", self.size >> 20, MAX_FILE >> 20);
        }
        if self.hashes.len() % 32 != 0 || self.chunks() as u64 != self.size.div_ceil(CHUNK as u64) {
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
    let clean = clean.trim().trim_start_matches('.').to_owned();
    if clean.is_empty() { "archivo".into() } else { clean }
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

/// Almacén de archivos: `files/<id>/oferta.cbor` y el archivo con su
/// nombre (o `<nombre>.part` mientras se descarga).
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

    /// Ruta del archivo completo, o de la descarga a medias.
    pub fn path(&mut self, id: &str) -> Result<(PathBuf, PathBuf)> {
        let name = self.get(id)?.offer.name;
        let folder = self.folder(id)?;
        Ok((folder.join(&name), folder.join(format!("{name}.part"))))
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
        let mut buf = vec![0; offer.chunk_len(index)];
        f.seek(SeekFrom::Start(index as u64 * CHUNK as u64))?;
        if read_full(&mut f, &mut buf)? != buf.len() {
            bail!("archivo local incompleto");
        }
        Ok(offer.seal(index, &buf))
    }
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
    let copy_path = folder.join(format!("{}.part", offer.name));
    let mut src = File::open(path)?;
    let mut copy = File::create(&copy_path)?;
    let mut buf = vec![0; CHUNK];
    let mut total = 0u64;
    for index in 0.. {
        let n = read_full(&mut src, &mut buf)?;
        if n == 0 {
            break;
        }
        total += n as u64;
        if total > size {
            bail!("el archivo cambió mientras se leía");
        }
        copy.write_all(&buf[..n])?;
        offer.hashes.extend_from_slice(blake3::hash(&offer.seal(index, &buf[..n])).as_bytes());
    }
    if total != size {
        bail!("el archivo cambió mientras se leía");
    }
    copy.sync_all()?;
    fs::rename(&copy_path, folder.join(&offer.name))?;
    Ok(offer)
}

/// Descarga a medias: escribe trozos ya comprobados en `<nombre>.part`.
pub struct Partial {
    file: File,
    offer: FileOffer,
}

impl Partial {
    /// Abre (o crea) la descarga y devuelve los trozos que faltan. Los que
    /// ya están se reconocen volviendo a cifrarlos y comparando el hash, sin
    /// guardar aparte qué se descargó.
    pub fn open(part: &Path, offer: FileOffer) -> Result<(Self, Vec<u32>)> {
        let mut file = fs::OpenOptions::new().read(true).write(true).create(true).truncate(false).open(part)?;
        let have = file.metadata()?.len();
        let mut missing = Vec::new();
        let mut buf = vec![0; CHUNK];
        for index in 0..offer.chunks() {
            let len = offer.chunk_len(index);
            let start = index as u64 * CHUNK as u64;
            let ok = start + len as u64 <= have && {
                file.seek(SeekFrom::Start(start))?;
                read_full(&mut file, &mut buf[..len])? == len
                    && Some(blake3::hash(&offer.seal(index, &buf[..len])).as_bytes().as_slice()) == offer.hash(index)
            };
            if !ok {
                missing.push(index);
            }
        }
        file.set_len(offer.size)?;
        Ok((Self { file, offer }, missing))
    }

    /// Comprueba, descifra y escribe un trozo recibido.
    pub fn write(&mut self, index: u32, sealed: &[u8]) -> Result<()> {
        let plain = self.offer.open(index, sealed)?;
        self.file.seek(SeekFrom::Start(index as u64 * CHUNK as u64))?;
        self.file.write_all(&plain)?;
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
        let dir = std::env::temp_dir().join(format!("p2m-files-{name}-{}", std::process::id()));
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
        assert_eq!(fs::read(&done).unwrap(), data);
        assert!(fb.complete(&id));
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
