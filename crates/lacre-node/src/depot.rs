//! Depósito de trozos (D-054): trozos cifrados de archivos ajenos que este
//! nodo guarda un tiempo, para que se puedan descargar aunque quien los
//! mandó esté desconectado.
//!
//! Aquí solo se ve el id del archivo, el número y el tamaño de los trozos y
//! el hash del testigo. Sin la clave del archivo (que solo va en la oferta,
//! dentro de MLS) el contenido no se puede leer, y sin el testigo no se
//! puede pedir. Quien descarga comprueba cada trozo con el hash de la
//! oferta: un depósito no puede colar datos.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use tracing::{debug, warn};

use crate::app::now;
use crate::files::CHUNK;

/// Archivos más grandes no se depositan (en bytes cifrados).
pub const MAX_FILE: u64 = 32 << 20;
/// Trozos como mucho por archivo.
const MAX_CHUNKS: u32 = (MAX_FILE / CHUNK as u64) as u32;
/// Un trozo cifrado: el trozo más la etiqueta de autenticación.
const MAX_CHUNK_BYTES: usize = CHUNK + 16;
/// Bytes en total que guarda este nodo.
const MAX_TOTAL: u64 = 512 << 20;
/// Tiempo que se guarda un archivo desde su primer trozo.
const TTL_SECS: u64 = 7 * 24 * 3600;
const META: &str = "meta";

/// Clave de la DHT junto a la que se depositan los trozos de un archivo.
pub fn key(file: &str) -> Vec<u8> {
    [b"/p2m/v1/depot/".as_slice(), file.as_bytes()].concat()
}

/// Lo que hay que guardar para comprobar las peticiones: el hash del
/// testigo, no el testigo.
pub fn check_of(token: &[u8; 32]) -> [u8; 32] {
    *blake3::hash(token).as_bytes()
}

struct Entry {
    check: [u8; 32],
    at: u64,
    bytes: u64,
}

pub struct Depot {
    dir: PathBuf,
    entries: HashMap<String, Entry>,
    total: u64,
}

/// Un id de archivo es hexadecimal de 64 caracteres (`FileOffer::id`): así
/// no puede salir del directorio.
fn valid_id(id: &str) -> bool {
    id.len() == 64 && id.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

impl Depot {
    pub fn open(data_dir: &Path) -> Self {
        let dir = data_dir.join("deposito");
        let mut depot = Self { dir, entries: HashMap::new(), total: 0 };
        let Ok(list) = fs::read_dir(&depot.dir) else { return depot };
        for e in list.flatten() {
            let id = e.file_name().to_string_lossy().into_owned();
            match depot.load(&id) {
                Some(entry) if valid_id(&id) => {
                    depot.total += entry.bytes;
                    depot.entries.insert(id, entry);
                }
                _ => {
                    debug!(%id, "dropping unreadable depot entry");
                    let _ = fs::remove_dir_all(e.path());
                }
            }
        }
        depot.prune();
        depot
    }

    fn load(&self, id: &str) -> Option<Entry> {
        let folder = self.dir.join(id);
        let meta = fs::read(folder.join(META)).ok()?;
        let check: [u8; 32] = meta.get(..32)?.try_into().ok()?;
        let at = u64::from_le_bytes(meta.get(32..40)?.try_into().ok()?);
        let mut bytes = 0;
        for e in fs::read_dir(&folder).ok()?.flatten() {
            if e.file_name() != META {
                bytes += e.metadata().ok()?.len();
            }
        }
        Some(Entry { check, at, bytes })
    }

    /// Borra los archivos caducados.
    pub fn prune(&mut self) {
        let t = now();
        let old: Vec<String> =
            self.entries.iter().filter(|(_, e)| e.at + TTL_SECS <= t).map(|(id, _)| id.clone()).collect();
        for id in old {
            self.remove(&id);
        }
    }

    fn remove(&mut self, id: &str) {
        if let Some(e) = self.entries.remove(id) {
            self.total -= e.bytes;
        }
        if let Err(e) = fs::remove_dir_all(self.dir.join(id)) {
            warn!(%id, "could not remove depot entry: {e}");
        }
    }

    /// Guarda un trozo. `check` es el hash del testigo: el primer trozo lo
    /// fija y los siguientes tienen que traer el mismo.
    pub fn put(&mut self, id: &str, index: u32, check: [u8; 32], data: &[u8]) -> Result<()> {
        if !valid_id(id) {
            bail!("id de archivo no válido");
        }
        if index >= MAX_CHUNKS {
            bail!("archivo demasiado grande para depositarlo");
        }
        if data.is_empty() || data.len() > MAX_CHUNK_BYTES {
            bail!("trozo de tamaño no válido");
        }
        self.prune();
        let folder = self.dir.join(id);
        let path = folder.join(index.to_string());
        let old = fs::metadata(&path).map_or(0, |m| m.len());
        let grow = (data.len() as u64).saturating_sub(old);
        if self.total + grow > MAX_TOTAL {
            bail!("depósito lleno");
        }
        match self.entries.get(id) {
            Some(e) if e.check != check => bail!("archivo desconocido"),
            Some(_) => {}
            None => {
                fs::create_dir_all(&folder)?;
                let at = now();
                fs::write(folder.join(META), [check.as_slice(), &at.to_le_bytes()].concat())?;
                self.entries.insert(id.to_owned(), Entry { check, at, bytes: 0 });
            }
        }
        let tmp = folder.join(format!("{index}.tmp"));
        fs::write(&tmp, data)?;
        fs::rename(&tmp, &path)?;
        let e = self.entries.get_mut(id).expect("insertada arriba");
        e.bytes = e.bytes - old + data.len() as u64;
        self.total = self.total - old + data.len() as u64;
        Ok(())
    }

    /// Un trozo guardado, para quien presente el testigo.
    pub fn get(&self, id: &str, index: u32, token: &[u8; 32]) -> Result<Vec<u8>> {
        let entry = self.entries.get(id).context("archivo desconocido")?;
        // Comparación en tiempo constante.
        if blake3::Hash::from(entry.check) != blake3::hash(token) {
            bail!("archivo desconocido");
        }
        if entry.at + TTL_SECS <= now() {
            bail!("archivo caducado");
        }
        fs::read(self.dir.join(id).join(index.to_string())).context("no tengo ese trozo")
    }

    pub fn total(&self) -> u64 {
        self.total
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("lacre-depot-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn put_get_and_reopen() {
        let dir = tmp("basic");
        let id = "ab".repeat(32);
        let token = [7u8; 32];
        let mut d = Depot::open(&dir);
        d.put(&id, 0, check_of(&token), b"trozo cero").unwrap();
        d.put(&id, 1, check_of(&token), b"uno").unwrap();
        assert_eq!(d.get(&id, 0, &token).unwrap(), b"trozo cero");
        // Testigo incorrecto, trozo que no está, otro testigo para el mismo id.
        assert!(d.get(&id, 0, &[8u8; 32]).is_err());
        assert!(d.get(&id, 2, &token).is_err());
        assert!(d.put(&id, 2, check_of(&[8u8; 32]), b"x").is_err());
        // Ids que saldrían del directorio, índices y tamaños fuera de rango.
        assert!(d.put("../../etc", 0, check_of(&token), b"x").is_err());
        assert!(d.put(&"AB".repeat(32), 0, check_of(&token), b"x").is_err());
        assert!(d.put(&id, MAX_CHUNKS, check_of(&token), b"x").is_err());
        assert!(d.put(&id, 3, check_of(&token), &vec![0; MAX_CHUNK_BYTES + 1]).is_err());
        // Reescribir un trozo no cuenta dos veces.
        d.put(&id, 1, check_of(&token), b"uno").unwrap();
        assert_eq!(d.total(), 13);
        let d = Depot::open(&dir);
        assert_eq!(d.total(), 13);
        assert_eq!(d.get(&id, 1, &token).unwrap(), b"uno");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn expires() {
        let dir = tmp("ttl");
        let id = "cd".repeat(32);
        let token = [1u8; 32];
        let mut d = Depot::open(&dir);
        d.put(&id, 0, check_of(&token), b"x").unwrap();
        d.entries.get_mut(&id).unwrap().at = now() - TTL_SECS;
        assert!(d.get(&id, 0, &token).is_err());
        d.prune();
        assert_eq!(d.total(), 0);
        assert!(!dir.join("deposito").join(&id).exists());
        let _ = fs::remove_dir_all(&dir);
    }
}
