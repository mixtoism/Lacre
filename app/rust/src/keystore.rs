//! Clave de datos del escritorio (D-033): se guarda en el Secret Service
//! (GNOME Keyring, KWallet…), una por directorio de datos.

use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result, anyhow};
use rand::RngCore;
use secret_service::{EncryptionType, SecretService};

/// Busca la clave de `data` en el llavero o crea una nueva. `Err` si no hay
/// llavero o está bloqueado y no se pudo abrir.
pub async fn load_or_create(data: &Path, create: bool) -> Result<[u8; 32]> {
    let ss = SecretService::connect(EncryptionType::Dh)
        .await
        .context("no hay llavero del sistema (Secret Service)")?;
    let dir = data.canonicalize().unwrap_or_else(|_| data.to_path_buf());
    let dir = dir.to_string_lossy();
    let attrs = HashMap::from([("application", "lacre"), ("data-dir", dir.as_ref())]);
    let found = ss.search_items(attrs.clone()).await?;
    if let Some(item) = found.unlocked.first().or(found.locked.first()) {
        item.ensure_unlocked().await.context("el llavero está bloqueado")?;
        let secret = item.get_secret().await?;
        return secret.try_into().map_err(|_| anyhow!("la clave del llavero no mide 32 bytes"));
    }
    if !create {
        anyhow::bail!("la clave de datos no está en el llavero");
    }
    let mut key = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut key);
    let collection = ss.get_default_collection().await.context("el llavero no tiene colección por defecto")?;
    collection.ensure_unlocked().await.context("el llavero está bloqueado")?;
    collection
        .create_item(&format!("Lacre ({dir})"), attrs, &key, true, "application/octet-stream")
        .await?;
    Ok(key)
}
