//! Personas: bloqueos (D-029) y fotos de perfil (D-030).
//!
//! Las dos cosas van por sufijo de handle (derivado de la clave raíz), que
//! no cambia aunque la persona cambie de nombre o de dispositivos.

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use lacre_core::{cbor_from, cbor_to};
use serde::{Deserialize, Serialize};

use crate::app::{read_private, write_private};
use crate::sticker::{dimensions, image_ext};

/// Tamaño máximo de una foto de perfil. La app la reduce antes de enviarla.
pub const MAX_AVATAR: usize = 128 * 1024;
/// Lado máximo de una foto de perfil, en píxeles.
pub const AVATAR_SIDE: u32 = 512;

/// Comprueba formato, tamaño y dimensiones declaradas de una foto de perfil.
pub fn check_avatar(bytes: &[u8]) -> Result<&'static str> {
    if bytes.len() > MAX_AVATAR {
        bail!("la foto ocupa {} KB (máximo {} KB)", bytes.len() / 1024, MAX_AVATAR / 1024);
    }
    let ext = image_ext(bytes).context("la foto no es una imagen PNG, GIF, WebP o JPEG")?;
    let (w, h) = dimensions(bytes).context("cabecera de imagen no válida")?;
    if w == 0 || h == 0 || w > AVATAR_SIDE || h > AVATAR_SIDE {
        bail!("la foto mide {w}×{h} px (máximo {AVATAR_SIDE}×{AVATAR_SIDE})");
    }
    Ok(ext)
}

/// Un bloqueo o desbloqueo de una persona, con su hora: entre nuestros
/// dispositivos gana el cambio más reciente (D-031).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct BlockEntry {
    pub handle: String,
    pub blocked: bool,
    /// Segundos desde 1970 (el siguiente cambio local siempre es posterior).
    pub at: u64,
}

/// Personas bloqueadas por sufijo. Los desbloqueos se recuerdan para que no
/// los deshaga una lista vieja de otro dispositivo nuestro.
pub struct Blocked {
    path: PathBuf,
    map: BTreeMap<String, BlockEntry>,
}

impl Blocked {
    pub fn open(dir: &Path) -> Result<Self> {
        let path = dir.join("blocked.cbor");
        let map = match read_private(&path)? {
            // Formato anterior: sufijo → handle, todos bloqueados.
            Some(b) => cbor_from(&b).or_else(|_| {
                cbor_from::<BTreeMap<String, String>>(&b).map(|old| {
                    old.into_iter()
                        .map(|(s, handle)| (s, BlockEntry { handle, blocked: true, at: 0 }))
                        .collect()
                })
            })?,
            None => BTreeMap::new(),
        };
        Ok(Self { path, map })
    }

    fn save(&self) -> Result<()> {
        write_private(&self.path, &cbor_to(&self.map)?)
    }

    pub fn contains(&self, suffix: &str) -> bool {
        self.map.get(suffix).is_some_and(|e| e.blocked)
    }

    /// Bloquea o desbloquea. Devuelve si cambió algo.
    pub fn set(&mut self, suffix: String, handle: String, blocked: bool, at: u64) -> Result<bool> {
        let changed = self.contains(&suffix) != blocked;
        let at = self.map.get(&suffix).map_or(at, |e| at.max(e.at + 1));
        self.map.insert(suffix, BlockEntry { handle, blocked, at });
        self.save()?;
        Ok(changed)
    }

    /// Une la lista de otro dispositivo nuestro. Devuelve si cambió algo.
    pub fn merge(&mut self, other: BTreeMap<String, BlockEntry>) -> Result<bool> {
        let mut changed = false;
        for (suffix, e) in other {
            if self.map.get(&suffix).is_some_and(|mine| mine.at >= e.at) {
                continue;
            }
            changed |= self.contains(&suffix) != e.blocked;
            self.map.insert(suffix, e);
        }
        if changed {
            self.save()?;
        }
        Ok(changed)
    }

    pub fn entries(&self) -> BTreeMap<String, BlockEntry> {
        self.map.clone()
    }

    pub fn handles(&self) -> Vec<String> {
        self.map.values().filter(|e| e.blocked).map(|e| e.handle.clone()).collect()
    }
}

#[derive(Serialize, Deserialize, Default, Clone)]
struct Entry {
    /// Versión que tenemos (0: ninguna).
    have: u64,
    /// Archivo en `avatars/` (sin foto si la versión la quitó).
    file: Option<String>,
    /// Versión más reciente que hemos visto anunciada.
    latest: u64,
    /// Dispositivo que la anunció: a él se le pide.
    device: Option<[u8; 32]>,
}

/// Fotos de perfil por sufijo de handle, la nuestra incluida.
pub struct Avatars {
    dir: PathBuf,
    map: HashMap<String, Entry>,
}

impl Avatars {
    pub fn open(dir: &Path) -> Result<Self> {
        let dir = dir.join("avatars");
        fs::create_dir_all(&dir)?;
        let map = match read_private(&dir.join("index.cbor"))? {
            Some(b) => cbor_from(&b)?,
            None => HashMap::new(),
        };
        Ok(Self { dir, map })
    }

    fn save(&self) -> Result<()> {
        write_private(&self.dir.join("index.cbor"), &cbor_to(&self.map)?)
    }

    /// Ruta de la foto, si la tenemos.
    pub fn path(&self, suffix: &str) -> Option<PathBuf> {
        Some(self.dir.join(self.map.get(suffix)?.file.as_ref()?))
    }

    /// Versión que tenemos (0 si ninguna).
    pub fn version(&self, suffix: &str) -> u64 {
        self.map.get(suffix).map_or(0, |e| e.have)
    }

    /// Versión y bytes de la foto (`None`: sin foto).
    pub fn read(&self, suffix: &str) -> Result<(u64, Option<Vec<u8>>)> {
        let version = self.version(suffix);
        let bytes = match self.path(suffix) {
            Some(p) => read_private(&p)?,
            None => None,
        };
        Ok((version, bytes))
    }

    /// Anota una versión anunciada. Devuelve si hay que pedirla.
    pub fn saw(&mut self, suffix: &str, version: u64, device: [u8; 32]) -> Result<bool> {
        let e = self.map.entry(suffix.to_owned()).or_default();
        let want = version > e.have;
        if version > e.latest {
            e.latest = version;
            e.device = Some(device);
            self.save()?;
        }
        Ok(want)
    }

    /// Fotos anunciadas que aún no tenemos: (sufijo, versión, dispositivo).
    pub fn wanted(&self) -> Vec<(String, u64, [u8; 32])> {
        self.map
            .iter()
            .filter(|(_, e)| e.latest > e.have)
            .filter_map(|(s, e)| Some((s.clone(), e.latest, e.device?)))
            .collect()
    }

    /// Guarda una versión (`None`: la persona quitó su foto).
    pub fn put(&mut self, suffix: &str, version: u64, image: Option<&[u8]>) -> Result<()> {
        let file = match image {
            Some(b) => {
                let name = format!("{suffix}-{version}.{}", check_avatar(b)?);
                write_private(&self.dir.join(&name), b)?;
                Some(name)
            }
            None => None,
        };
        let e = self.map.entry(suffix.to_owned()).or_default();
        if version < e.have {
            return Ok(());
        }
        if let Some(old) = e.file.take()
            && Some(&old) != file.as_ref()
        {
            let _ = fs::remove_file(self.dir.join(old));
        }
        e.file = file;
        e.have = version;
        e.latest = e.latest.max(version);
        self.save()
    }
}

/// Texto normalizado para buscar: minúsculas y sin tildes ni diéresis.
pub fn fold(s: &str) -> String {
    s.chars()
        .flat_map(char::to_lowercase)
        .map(|c| match c {
            'á' | 'à' | 'â' | 'ä' | 'ã' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'í' | 'ì' | 'î' | 'ï' => 'i',
            'ó' | 'ò' | 'ô' | 'ö' | 'õ' => 'o',
            'ú' | 'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            c => c,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fold_ignores_case_and_accents() {
        assert_eq!(fold("Canción ÚNICA"), "cancion unica");
        assert!(fold("Pingüino").contains(&fold("PINGUI")));
        assert_eq!(fold("año"), "año");
    }

    #[test]
    fn blocks_merge_latest_wins() {
        let dir = std::env::temp_dir().join(format!("lacre-blocked-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let mut a = Blocked::open(&dir).unwrap();
        assert!(a.set("x".into(), "x#x".into(), true, 10).unwrap());
        let old = a.entries();
        assert!(a.set("x".into(), "x#x".into(), false, 20).unwrap());
        // Una lista vieja no deshace el desbloqueo; una nueva sí bloquea.
        assert!(!a.merge(old).unwrap() && !a.contains("x"));
        let newer = BTreeMap::from([("x".to_owned(), BlockEntry { handle: "x#x".into(), blocked: true, at: 30 })]);
        assert!(a.merge(newer).unwrap() && a.contains("x"));
        assert_eq!(Blocked::open(&dir).unwrap().handles(), vec!["x#x".to_owned()]);
        // Formato anterior.
        let legacy = BTreeMap::from([("y".to_owned(), "y#y".to_owned())]);
        write_private(&dir.join("blocked.cbor"), &cbor_to(&legacy).unwrap()).unwrap();
        assert!(Blocked::open(&dir).unwrap().contains("y"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn avatars_track_versions() {
        let dir = std::env::temp_dir().join(format!("lacre-avatars-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let mut a = Avatars::open(&dir).unwrap();
        let png = {
            let mut b = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 13];
            b.extend_from_slice(b"IHDR");
            b.extend_from_slice(&64u32.to_be_bytes());
            b.extend_from_slice(&64u32.to_be_bytes());
            b
        };
        assert!(a.saw("abc", 5, [1; 32]).unwrap());
        assert_eq!(a.wanted(), vec![("abc".to_owned(), 5, [1; 32])]);
        a.put("abc", 5, Some(&png)).unwrap();
        assert!(a.wanted().is_empty());
        assert!(!a.saw("abc", 5, [1; 32]).unwrap());
        let first = a.path("abc").unwrap();
        assert!(first.exists());
        // Una versión vieja no sustituye a la nueva.
        a.put("abc", 3, None).unwrap();
        assert_eq!(a.version("abc"), 5);
        // Quitar la foto borra el archivo.
        a.put("abc", 6, None).unwrap();
        assert!(a.path("abc").is_none() && !first.exists());
        // Se reabre igual.
        assert_eq!(Avatars::open(&dir).unwrap().version("abc"), 6);
        let mut too_big = png.clone();
        too_big[16..20].copy_from_slice(&4000u32.to_be_bytes());
        assert!(check_avatar(&too_big).is_err());
        let _ = fs::remove_dir_all(&dir);
    }
}
