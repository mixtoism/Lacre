//! Arma un paquete `.lacre-stickers` con imágenes ya preparadas (D-055).
//!
//! `cargo run -p lacre-node --example empaquetar_stickers -- <nombre> <salida> <imagen>…`

use anyhow::{Context, Result, bail};

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [name, out, images @ ..] = args.as_slice() else {
        bail!("uso: empaquetar_stickers <nombre> <salida> <imagen>…");
    };
    let mut stickers = Vec::new();
    for path in images {
        let bytes = std::fs::read(path).with_context(|| format!("no se pudo leer {path}"))?;
        lacre_node::sticker::check(&bytes).with_context(|| format!("{path} no vale como sticker"))?;
        stickers.push(bytes);
    }
    let pack = lacre_node::sticker::pack(name, stickers)?;
    std::fs::write(out, &pack).with_context(|| format!("no se pudo escribir {out}"))?;
    println!("{out}: {} stickers, {} KB", images.len(), pack.len() / 1024);
    Ok(())
}
