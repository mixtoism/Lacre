//! Clave de datos en macOS (D-033, D-042): en el Llavero del usuario, una por
//! directorio de datos.

use std::path::Path;

use anyhow::{Context, Result, anyhow, bail};
use security_framework::passwords::{get_generic_password, set_generic_password};

const SERVICE: &str = "Lacre";
/// `errSecItemNotFound`.
const NOT_FOUND: i32 = -25300;

pub fn load_or_create(data: &Path, create: bool) -> Result<[u8; 32]> {
    let dir = data.canonicalize().unwrap_or_else(|_| data.to_path_buf());
    let account = dir.to_string_lossy();
    match get_generic_password(SERVICE, &account) {
        Ok(key) => key.try_into().map_err(|_| anyhow!("la clave del Llavero no mide 32 bytes")),
        Err(e) if e.code() == NOT_FOUND => {
            if !create {
                bail!("la clave de datos no está en el Llavero");
            }
            let mut key = [0u8; 32];
            use rand::RngCore;
            rand::rngs::OsRng.fill_bytes(&mut key);
            set_generic_password(SERVICE, &account, &key).context("no se pudo guardar la clave en el Llavero")?;
            Ok(key)
        }
        Err(e) => Err(anyhow!("no se pudo leer el Llavero: {e}")),
    }
}
