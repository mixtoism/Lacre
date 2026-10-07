//! Stickers (D-025): imágenes pequeñas que viajan dentro del mensaje cifrado,
//! sin servicios externos.
//!
//! Los bytes de cada sticker (enviado, recibido o de la colección) se guardan
//! una sola vez en `media/`, con el hash del contenido como nombre. La
//! colección propia es solo la lista de esos nombres (`stickers.cbor`).

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use p2m_core::{cbor_from, cbor_to};

use crate::app::{read_private, write_plain, write_private};

/// Tamaño máximo de un sticker. Cabe con holgura en un sobre de buzón
/// (`MAX_ENVELOPE`) junto con el cifrado MLS.
pub const MAX_STICKER: usize = 200 * 1024;

/// Lado máximo, en píxeles. Una imagen pequeña en bytes puede declarar
/// dimensiones enormes y agotar la memoria al decodificarla (bomba de
/// descompresión): se comprueba en la cabecera antes de mostrarla.
pub const MAX_SIDE: u32 = 1024;

/// Formato de imagen según los primeros bytes; solo se aceptan estos.
pub fn image_ext(bytes: &[u8]) -> Option<&'static str> {
    match bytes {
        [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, ..] => Some("png"),
        [b'G', b'I', b'F', b'8', b'7' | b'9', b'a', ..] => Some("gif"),
        [b'R', b'I', b'F', b'F', _, _, _, _, b'W', b'E', b'B', b'P', ..] => Some("webp"),
        [0xff, 0xd8, 0xff, ..] => Some("jpg"),
        _ => None,
    }
}

/// Ancho y alto declarados en la cabecera de la imagen.
pub fn dimensions(b: &[u8]) -> Option<(u32, u32)> {
    let be16 = |i: usize| Some(u16::from_be_bytes(b.get(i..i + 2)?.try_into().ok()?) as u32);
    let le16 = |i: usize| Some(u16::from_le_bytes(b.get(i..i + 2)?.try_into().ok()?) as u32);
    let le24 = |i: usize| Some(u32::from_le_bytes([*b.get(i)?, *b.get(i + 1)?, *b.get(i + 2)?, 0]));
    match image_ext(b)? {
        "png" if b.get(12..16)? == b"IHDR" => {
            let w = u32::from_be_bytes(b.get(16..20)?.try_into().ok()?);
            Some((w, u32::from_be_bytes(b.get(20..24)?.try_into().ok()?)))
        }
        "gif" => Some((le16(6)?, le16(8)?)),
        "webp" => match b.get(12..16)? {
            b"VP8 " => Some((le16(26)? & 0x3fff, le16(28)? & 0x3fff)),
            b"VP8L" => {
                let v = u32::from_le_bytes(b.get(21..25)?.try_into().ok()?);
                Some(((v & 0x3fff) + 1, ((v >> 14) & 0x3fff) + 1))
            }
            b"VP8X" => Some((le24(24)? + 1, le24(27)? + 1)),
            _ => None,
        },
        "jpg" => {
            // Recorre los segmentos hasta el de inicio de fotograma (SOFn).
            let mut i = 2;
            loop {
                if *b.get(i)? != 0xff {
                    return None;
                }
                let marker = *b.get(i + 1)?;
                if (0xc0..=0xcf).contains(&marker) && !matches!(marker, 0xc4 | 0xc8 | 0xcc) {
                    return Some((be16(i + 7)?, be16(i + 5)?));
                }
                i += 2 + be16(i + 2)? as usize;
            }
        }
        _ => None,
    }
}

/// Comprueba tamaño, formato y dimensiones; devuelve la extensión.
pub fn check(bytes: &[u8]) -> Result<&'static str> {
    if bytes.len() > MAX_STICKER {
        bail!("el sticker ocupa {} KB (máximo {} KB)", bytes.len() / 1024, MAX_STICKER / 1024);
    }
    let ext = image_ext(bytes).context("el sticker no es una imagen PNG, GIF, WebP o JPEG")?;
    let (w, h) = dimensions(bytes).context("cabecera de imagen no válida")?;
    if w == 0 || h == 0 || w > MAX_SIDE || h > MAX_SIDE {
        bail!("el sticker mide {w}×{h} px (máximo {MAX_SIDE}×{MAX_SIDE})");
    }
    Ok(ext)
}

/// Almacén de stickers por contenido y colección propia.
pub struct Stickers {
    dir: PathBuf,
    list_path: PathBuf,
    /// Colección propia, del más reciente al más antiguo.
    list: Vec<String>,
}

impl Stickers {
    pub fn open(data_dir: &Path) -> Result<Self> {
        let dir = data_dir.join("media");
        let list_path = data_dir.join("stickers.cbor");
        let list = match read_private(&list_path)? {
            Some(b) => cbor_from(&b)?,
            None => Vec::new(),
        };
        Ok(Self { dir, list_path, list })
    }

    /// Guarda los bytes (ya comprobados con [`check`]) y devuelve su nombre.
    pub fn put(&self, bytes: &[u8]) -> Result<String> {
        let id = format!("{}.{}", blake3::hash(bytes).to_hex(), check(bytes)?);
        let path = self.dir.join(&id);
        if !path.exists() {
            fs::create_dir_all(&self.dir)?;
            write_plain(&path, bytes)?;
        }
        Ok(id)
    }

    /// Ruta de un sticker guardado. El nombre viene del historial o de la
    /// interfaz: se valida para no salir de `media/`.
    pub fn path(&self, id: &str) -> Result<PathBuf> {
        let (hash, ext) = id.split_once('.').context("sticker desconocido")?;
        let valid = hash.len() == 64
            && hash.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
            && matches!(ext, "png" | "gif" | "webp" | "jpg");
        if !valid {
            bail!("sticker desconocido");
        }
        Ok(self.dir.join(id))
    }

    pub fn read(&self, id: &str) -> Result<Vec<u8>> {
        let path = self.path(id)?;
        fs::read(&path).with_context(|| format!("no se pudo leer {}", path.display()))
    }

    pub fn list(&self) -> &[String] {
        &self.list
    }

    /// Añade un sticker ya guardado a la colección (al principio).
    pub fn keep(&mut self, id: &str) -> Result<()> {
        if !self.path(id)?.exists() {
            bail!("sticker desconocido");
        }
        self.list.retain(|s| s != id);
        self.list.insert(0, id.to_owned());
        self.save()
    }

    /// Lo quita de la colección; los bytes se quedan (los usa el historial).
    pub fn remove(&mut self, id: &str) -> Result<()> {
        self.list.retain(|s| s != id);
        self.save()
    }

    fn save(&self) -> Result<()> {
        write_private(&self.list_path, &cbor_to(&self.list)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// PNG de 512×256 (solo la cabecera: no se decodifica).
    const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR\0\0\x02\0\0\0\x01\0\x08\x06\0\0\0";

    #[test]
    fn formats() {
        assert_eq!(image_ext(PNG), Some("png"));
        assert_eq!(image_ext(b"GIF89a..."), Some("gif"));
        assert_eq!(image_ext(b"RIFF\0\0\0\0WEBPVP8 "), Some("webp"));
        assert_eq!(image_ext(&[0xff, 0xd8, 0xff, 0xe0]), Some("jpg"));
        assert_eq!(image_ext(b"<svg></svg>"), None);
        assert!(check(&[PNG, &vec![0; MAX_STICKER]].concat()).is_err());
    }

    #[test]
    fn dimensions_from_headers() {
        assert_eq!(dimensions(PNG), Some((512, 256)));
        assert_eq!(dimensions(b"GIF89a\x40\x01\xf0\x00"), Some((320, 240)));
        // VP8L: ancho-1 y alto-1 en 14 bits cada uno.
        let v: u32 = 99 | (49 << 14);
        let webp = [b"RIFF\0\0\0\0WEBPVP8L\0\0\0\0\x2f".as_slice(), &v.to_le_bytes()].concat();
        assert_eq!(dimensions(&webp), Some((100, 50)));
        let vp8x = b"RIFF\0\0\0\0WEBPVP8X\0\0\0\0\0\0\0\0\xff\x01\0\xff\x00\0";
        assert_eq!(dimensions(vp8x), Some((512, 256)));
        // JPEG: APP0 de 4 bytes y luego SOF0 (alto 300, ancho 400).
        let jpg = b"\xff\xd8\xff\xe0\0\x04ab\xff\xc0\0\x11\x08\x01\x2c\x01\x90";
        assert_eq!(dimensions(jpg), Some((400, 300)));
        assert_eq!(dimensions(b"\xff\xd8\xff\xe0\0\x04"), None);
        // Bomba: 30000×30000 en pocos bytes.
        let bomb = [&PNG[..16], &30000u32.to_be_bytes(), &30000u32.to_be_bytes()].concat();
        assert!(check(&bomb).unwrap_err().to_string().contains("30000×30000"));
        assert!(check(PNG).is_ok());
    }

    #[test]
    fn store_and_collection() {
        let dir = std::env::temp_dir().join(format!("p2m-stickers-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let mut s = Stickers::open(&dir).unwrap();
        let id = s.put(PNG).unwrap();
        assert!(id.ends_with(".png"));
        assert_eq!(s.read(&id).unwrap(), PNG);
        assert!(s.path("../../etc/passwd.png").is_err());
        assert!(s.path(&format!("{}.svg", "a".repeat(64))).is_err());
        s.keep(&id).unwrap();
        assert_eq!(Stickers::open(&dir).unwrap().list(), [id.clone()]);
        s.remove(&id).unwrap();
        assert!(Stickers::open(&dir).unwrap().list().is_empty());
        assert!(s.read(&id).is_ok());
        fs::remove_dir_all(&dir).unwrap();
    }
}
