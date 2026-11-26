//! Clave de datos en Windows (D-033, D-042): se guarda en `<datos>/clave.dpapi`
//! protegida con DPAPI, ligada a la cuenta de usuario. Otro usuario del
//! equipo, o alguien que copie la carpeta a otro equipo, no puede descifrarla.

use std::path::Path;

use anyhow::{Context, Result, anyhow, bail};
use windows_sys::Win32::Foundation::LocalFree;
use windows_sys::Win32::Security::Cryptography::{
    CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData, CryptUnprotectData,
};

const FILE: &str = "clave.dpapi";
/// Entropía adicional: la clave solo se descifra pidiéndola para Lacre.
const ENTROPY: &[u8] = b"lacre/clave-de-datos/v1";

pub fn load_or_create(data: &Path, create: bool) -> Result<[u8; 32]> {
    let path = data.join(FILE);
    match std::fs::read(&path) {
        Ok(wrapped) => {
            let key = unprotect(&wrapped).context("DPAPI no pudo descifrar la clave de datos")?;
            key.try_into().map_err(|_| anyhow!("la clave de datos no mide 32 bytes"))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            if !create {
                bail!("falta {} y el estado ya está cifrado", path.display());
            }
            let mut key = [0u8; 32];
            getrandom(&mut key)?;
            let wrapped = protect(&key)?;
            std::fs::create_dir_all(data)?;
            let tmp = data.join(format!("{FILE}.tmp"));
            std::fs::write(&tmp, &wrapped)?;
            std::fs::rename(&tmp, &path)?;
            Ok(key)
        }
        Err(e) => Err(e).with_context(|| format!("no se pudo leer {}", path.display())),
    }
}

fn getrandom(buf: &mut [u8]) -> Result<()> {
    use rand::RngCore;
    rand::rngs::OsRng.try_fill_bytes(buf).map_err(|e| anyhow!("sin aleatoriedad del sistema: {e}"))
}

fn blob(b: &[u8]) -> CRYPT_INTEGER_BLOB {
    CRYPT_INTEGER_BLOB { cbData: b.len() as u32, pbData: b.as_ptr() as *mut u8 }
}

/// Copia la salida de DPAPI y la libera.
unsafe fn take(out: CRYPT_INTEGER_BLOB) -> Vec<u8> {
    let v = unsafe { std::slice::from_raw_parts(out.pbData, out.cbData as usize) }.to_vec();
    unsafe { LocalFree(out.pbData as _) };
    v
}

fn protect(plain: &[u8]) -> Result<Vec<u8>> {
    let input = blob(plain);
    let entropy = blob(ENTROPY);
    let mut out = CRYPT_INTEGER_BLOB { cbData: 0, pbData: std::ptr::null_mut() };
    // SAFETY: los punteros apuntan a búferes vivos durante la llamada; la
    // salida la reserva DPAPI y se libera en `take`.
    let ok = unsafe {
        CryptProtectData(&input, std::ptr::null(), &entropy, std::ptr::null(), std::ptr::null(), CRYPTPROTECT_UI_FORBIDDEN, &mut out)
    };
    if ok == 0 {
        bail!("CryptProtectData falló: {}", std::io::Error::last_os_error());
    }
    Ok(unsafe { take(out) })
}

fn unprotect(wrapped: &[u8]) -> Result<Vec<u8>> {
    let input = blob(wrapped);
    let entropy = blob(ENTROPY);
    let mut out = CRYPT_INTEGER_BLOB { cbData: 0, pbData: std::ptr::null_mut() };
    // SAFETY: como en `protect`.
    let ok = unsafe {
        CryptUnprotectData(&input, std::ptr::null_mut(), &entropy, std::ptr::null(), std::ptr::null(), CRYPTPROTECT_UI_FORBIDDEN, &mut out)
    };
    if ok == 0 {
        bail!("CryptUnprotectData falló: {}", std::io::Error::last_os_error());
    }
    Ok(unsafe { take(out) })
}
