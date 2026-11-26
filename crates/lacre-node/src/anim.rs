//! Stickers animados (D-055): un GIF (o WebP animado) que no cabe en
//! [`MAX_STICKER`](crate::sticker::MAX_STICKER) se convierte a WebP animado
//! sin pérdida, reduciendo el tamaño y, si hace falta, los fotogramas.
//!
//! Todo en Rust, sin libwebp: cada fotograma se codifica con `image-webp`
//! (VP8L) y el contenedor animado (`VP8X`, `ANIM`, `ANMF`) se arma aquí. De
//! cada fotograma solo se guarda el rectángulo que cambia respecto al
//! anterior, como hace el propio GIF.

use std::io::Cursor;

use anyhow::{Context, Result, bail};
use image::codecs::gif::GifDecoder;
use image::codecs::webp::WebPDecoder;
use image::imageops::{self, FilterType};
use image::{AnimationDecoder, RgbaImage};
use image_webp::{ColorType, WebPEncoder};

use crate::sticker::{MAX_SIDE, dimensions, image_ext};

/// Entrada máxima: más que esto no es un sticker.
const MAX_INPUT: usize = 16 << 20;
/// Fotogramas como mucho (cada uno se guarda decodificado mientras tanto).
const MAX_FRAMES: usize = 150;
/// Lado máximo del resultado: se muestran a unos 120 px.
const SIDE: u32 = 320;
/// Intentos, de mejor a peor calidad: escala y uno de cada cuántos
/// fotogramas se conserva.
const ATTEMPTS: [(f32, usize); 7] = [(1.0, 1), (0.75, 1), (0.75, 2), (0.5, 2), (0.5, 3), (0.375, 3), (0.25, 4)];

struct Frame {
    img: RgbaImage,
    ms: u32,
}

/// Convierte una imagen animada en un WebP animado de como mucho `max`
/// bytes.
pub fn fit_animated(bytes: &[u8], max: usize) -> Result<Vec<u8>> {
    let frames = decode(bytes)?;
    for (scale, step) in ATTEMPTS {
        let out = if scale == 1.0 {
            encode(&frames, step)?
        } else {
            let (w, h) = frames[0].img.dimensions();
            let (w, h) = (((w as f32 * scale) as u32).max(1), ((h as f32 * scale) as u32).max(1));
            let small: Vec<Frame> = frames
                .iter()
                .map(|f| Frame { img: imageops::resize(&f.img, w, h, FilterType::Triangle), ms: f.ms })
                .collect();
            encode(&small, step)?
        };
        if out.len() <= max {
            return Ok(out);
        }
    }
    bail!("no se pudo reducir el sticker animado a {} KB", max / 1024)
}

fn decode(bytes: &[u8]) -> Result<Vec<Frame>> {
    if bytes.len() > MAX_INPUT {
        bail!("imagen demasiado grande");
    }
    // Las dimensiones se leen de la cabecera antes de decodificar (D-025).
    let (w, h) = dimensions(bytes).context("no es una imagen válida")?;
    if w == 0 || h == 0 || w > MAX_SIDE || h > MAX_SIDE {
        bail!("imagen de {w}×{h} px (máximo {MAX_SIDE} px por lado)");
    }
    let frames = match image_ext(bytes) {
        Some("gif") => GifDecoder::new(Cursor::new(bytes))?.into_frames(),
        Some("webp") => WebPDecoder::new(Cursor::new(bytes))?.into_frames(),
        _ => bail!("solo se convierten GIF y WebP animados"),
    };
    let scale = (SIDE as f32 / w.max(h) as f32).min(1.0);
    let (tw, th) = (((w as f32 * scale) as u32).max(1), ((h as f32 * scale) as u32).max(1));
    let mut canvas = RgbaImage::new(w, h);
    let mut out = Vec::new();
    for f in frames {
        if out.len() == MAX_FRAMES {
            bail!("demasiados fotogramas (máximo {MAX_FRAMES})");
        }
        let f = f.context("imagen animada dañada")?;
        let (num, den) = f.delay().numer_denom_ms();
        // Los navegadores tratan los retardos muy cortos como 100 ms.
        let ms = if den == 0 || num / den < 20 { 100 } else { num / den };
        let (left, top) = (f.left(), f.top());
        let buf = f.into_buffer();
        if buf.dimensions() == (w, h) {
            canvas = buf;
        } else {
            imageops::replace(&mut canvas, &buf, left as i64, top as i64);
        }
        let img = if (tw, th) == (w, h) { canvas.clone() } else { imageops::resize(&canvas, tw, th, FilterType::Triangle) };
        out.push(Frame { img, ms });
    }
    if out.is_empty() {
        bail!("imagen sin fotogramas");
    }
    Ok(out)
}

/// WebP animado con uno de cada `step` fotogramas.
fn encode(frames: &[Frame], step: usize) -> Result<Vec<u8>> {
    let (w, h) = frames[0].img.dimensions();
    let mut body = Vec::new();
    // VP8X: animación (bit 1) y alfa (bit 4); lienzo de w×h.
    chunk(&mut body, b"VP8X", &[&[0x12, 0, 0, 0][..], &le24(w - 1), &le24(h - 1)].concat());
    // ANIM: fondo transparente y repetición infinita.
    chunk(&mut body, b"ANIM", &[0, 0, 0, 0, 0, 0]);
    let mut prev: Option<&RgbaImage> = None;
    for group in frames.chunks(step) {
        let img = &group[0].img;
        let ms: u32 = group.iter().map(|f| f.ms).sum();
        let (x0, y0, x1, y1) = match prev {
            None => (0, 0, w, h),
            Some(p) => changed(p, img).unwrap_or((0, 0, 1, 1)),
        };
        // El desplazamiento de un fotograma va en unidades de 2 px.
        let (x0, y0) = (x0 & !1, y0 & !1);
        let sub = imageops::crop_imm(img, x0, y0, x1 - x0, y1 - y0).to_image();
        let mut anmf = [le24(x0 / 2), le24(y0 / 2), le24(sub.width() - 1), le24(sub.height() - 1), le24(ms.min(0xff_ffff))]
            .concat();
        // Sin mezcla (bit 1): el rectángulo sustituye a lo que había.
        anmf.push(0x02);
        chunk(&mut anmf, b"VP8L", &lossless(&sub)?);
        chunk(&mut body, b"ANMF", &anmf);
        prev = Some(img);
    }
    let mut out = Vec::with_capacity(body.len() + 12);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(body.len() as u32 + 4).to_le_bytes());
    out.extend_from_slice(b"WEBP");
    out.extend_from_slice(&body);
    Ok(out)
}

/// Rectángulo (x0, y0, x1, y1) de los píxeles que cambian; `None` si
/// ninguno.
fn changed(a: &RgbaImage, b: &RgbaImage) -> Option<(u32, u32, u32, u32)> {
    let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0, 0);
    for (x, y, p) in b.enumerate_pixels() {
        if a.get_pixel(x, y) != p {
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x + 1);
            y1 = y1.max(y + 1);
        }
    }
    (x0 != u32::MAX).then_some((x0, y0, x1, y1))
}

/// Flujo VP8L de una imagen (sin el contenedor RIFF).
fn lossless(img: &RgbaImage) -> Result<Vec<u8>> {
    let mut riff = Vec::new();
    WebPEncoder::new(&mut riff).encode(img.as_raw(), img.width(), img.height(), ColorType::Rgba8)?;
    // RIFF <tamaño> WEBP VP8L <tamaño> <datos>
    if riff.get(12..16) != Some(b"VP8L") {
        bail!("formato inesperado del codificador WebP");
    }
    let len = u32::from_le_bytes(riff[16..20].try_into()?) as usize;
    Ok(riff.get(20..20 + len).context("WebP truncado")?.to_vec())
}

fn chunk(out: &mut Vec<u8>, fourcc: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(fourcc);
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(data);
    if data.len() % 2 == 1 {
        out.push(0);
    }
}

fn le24(v: u32) -> [u8; 3] {
    let b = v.to_le_bytes();
    [b[0], b[1], b[2]]
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::codecs::gif::{GifEncoder, Repeat};
    use image::{Delay, Rgba};

    /// GIF de 400×300 con 30 fotogramas: un cuadrado que se mueve sobre un
    /// fondo de franjas o sobre ruido (que no se comprime).
    fn gif(noisy: bool) -> Vec<u8> {
        let mut out = Vec::new();
        {
            let mut enc = GifEncoder::new_with_speed(&mut out, 30);
            enc.set_repeat(Repeat::Infinite).unwrap();
            let mut seed = 1u32;
            let noise = RgbaImage::from_fn(400, 300, |x, y| {
                seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
                if noisy {
                    Rgba([(seed >> 16) as u8, (seed >> 8) as u8, seed as u8, 255])
                } else {
                    Rgba([((x / 20) * 12) as u8, ((y / 15) * 12) as u8, 128, 255])
                }
            });
            for i in 0..30u32 {
                let mut img = noise.clone();
                for y in 100..160 {
                    for x in (i * 10)..(i * 10 + 60) {
                        img.put_pixel(x, y, Rgba([255, 0, 0, 255]));
                    }
                }
                let frame = image::Frame::from_parts(img, 0, 0, Delay::from_numer_denom_ms(50, 1));
                enc.encode_frame(frame).unwrap();
            }
        }
        out
    }

    #[test]
    fn gif_to_animated_webp() {
        let src = gif(false);
        let max = 200 * 1024;
        let out = fit_animated(&src, max).unwrap();
        assert!(out.len() <= max, "{} bytes", out.len());
        assert_eq!(image_ext(&out), Some("webp"));
        // Para validarlo con herramientas externas (webpinfo, navegadores).
        if let Ok(path) = std::env::var("LACRE_DUMP_WEBP") {
            std::fs::write(path, &out).unwrap();
        }
        // Lado reducido a 320 px como mucho, y lo acepta `check`.
        assert_eq!(dimensions(&out), Some((320, 240)));
        crate::sticker::check(&out).unwrap();
        // Se decodifica como animación con los fotogramas y la duración.
        let frames: Vec<_> = WebPDecoder::new(Cursor::new(&out)).unwrap().into_frames().collect_frames().unwrap();
        assert!(frames.len() >= 10, "{} fotogramas", frames.len());
        let total: u32 = frames
            .iter()
            .map(|f| {
                let (n, d) = f.delay().numer_denom_ms();
                n / d
            })
            .sum();
        assert_eq!(total, 30 * 50);
        // El último fotograma (sobre el lienzo) coincide con el original.
        let orig: Vec<_> = GifDecoder::new(Cursor::new(&src)).unwrap().into_frames().collect_frames().unwrap();
        let last = imageops::resize(orig.last().unwrap().buffer(), 320, 240, FilterType::Triangle);
        if frames.len() == 30 {
            assert_eq!(frames.last().unwrap().buffer(), &last);
        }
    }

    #[test]
    fn rejects() {
        assert!(fit_animated(b"no es una imagen", 1000).is_err());
        // Cabecera GIF que declara 30000×30000.
        assert!(fit_animated(b"GIF89a\x30\x75\x30\x75\0\0\0", 1000).is_err());
        // Imposible de reducir a 100 bytes.
        assert!(fit_animated(&gif(false), 100).is_err());
    }

    #[test]
    fn noise_is_shrunk_until_it_fits() {
        let out = fit_animated(&gif(true), 200 * 1024).unwrap();
        assert!(out.len() <= 200 * 1024);
        let (w, _) = dimensions(&out).unwrap();
        assert!(w < 320);
    }
}
