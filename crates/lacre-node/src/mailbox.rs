//! Buzones: sobres que este nodo guarda para dispositivos desconectados
//! (D-016). El contenido, remitente incluido, va sellado para el
//! destinatario (D-034); aquí solo se ve destinatario, tamaño y hora.

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;

use anyhow::{Result, bail};
use tracing::warn;
use lacre_core::envelope::Envelope;
use lacre_core::{cbor_from, cbor_to};
use serde::{Deserialize, Serialize};

use crate::app::{now, read_private, write_private};
use crate::proto::MAX_ENVELOPE;

/// Sobres por destinatario.
const MAX_PER_RECIPIENT: usize = 500;
/// Destinatarios distintos.
const MAX_RECIPIENTS: usize = 2000;
/// Bytes por destinatario y en total: sin ellos el peor caso era de
/// cientos de GB (D-042).
const MAX_BYTES_PER_RECIPIENT: usize = 16 << 20;
const MAX_TOTAL_BYTES: usize = 256 << 20;
/// Segundos mínimos entre escrituras del buzón a disco: cada una reescribe
/// el archivo entero. Lo pendiente se guarda con [`Mailbox::flush`].
const SAVE_EVERY: u64 = 10;
/// Tiempo máximo que se guarda un sobre.
const TTL_SECS: u64 = 14 * 24 * 3600;
/// Bytes máximos por respuesta a `Fetch` (el límite del protocolo es 10 MiB).
const MAX_FETCH_BYTES: usize = 4 << 20;
/// Bytes máximos por `Deliver` (el límite de una petición es 1 MiB).
pub const MAX_DELIVER_BYTES: usize = 768 << 10;
/// Entrega activa (D-034): primer intento al minuto (más un azar de hasta
/// 30 s, para no delatar el momento del depósito) y después cada vez más
/// espaciados, hasta que caduque el sobre.
const PUSH_FIRST: u64 = 60;
const PUSH_JITTER: u64 = 30;
const PUSH_BACKOFF: [u64; 3] = [5 * 60, 30 * 60, 2 * 3600];

/// Cuándo toca intentar entregar los sobres de un destinatario.
struct Push {
    next: u64,
    tries: usize,
}

fn first_push() -> u64 {
    now() + PUSH_FIRST + rand::random::<u64>() % PUSH_JITTER
}

#[derive(Serialize, Deserialize)]
struct Stored {
    at: u64,
    env: Envelope,
}

#[derive(Default, Serialize, Deserialize)]
struct Boxes(HashMap<[u8; 32], VecDeque<Stored>>);

pub struct Mailbox {
    path: PathBuf,
    boxes: Boxes,
    /// No se guarda: tras reiniciar se empieza de nuevo por el primer intento.
    pushes: HashMap<[u8; 32], Push>,
    /// Bytes de sobres guardados.
    bytes: usize,
    /// Hay cambios sin escribir y cuándo se escribió por última vez.
    dirty: bool,
    saved_at: u64,
}

impl Mailbox {
    pub fn open(path: PathBuf) -> Result<Self> {
        let boxes = match read_private(&path)? {
            // Un formato antiguo no impide arrancar: son copias, el
            // remitente guarda otras.
            Some(b) => cbor_from(&b).unwrap_or_else(|e| {
                warn!("unreadable mailbox ({e}); starting empty");
                Boxes::default()
            }),
            None => Boxes::default(),
        };
        let pushes = boxes.0.keys().map(|to| (*to, Push { next: first_push(), tries: 0 })).collect();
        let mut mb = Self { path, boxes, pushes, bytes: 0, dirty: false, saved_at: now() };
        mb.prune();
        Ok(mb)
    }

    /// Marca cambios y los escribe si hace `SAVE_EVERY` que no se escribía.
    fn save(&mut self) -> Result<()> {
        self.dirty = true;
        if now() >= self.saved_at + SAVE_EVERY {
            self.flush()?;
        }
        Ok(())
    }

    /// Escribe a disco lo pendiente.
    pub fn flush(&mut self) -> Result<()> {
        if self.dirty {
            write_private(&self.path, &cbor_to(&self.boxes)?)?;
            self.dirty = false;
            self.saved_at = now();
        }
        Ok(())
    }

    fn queue_bytes(q: &VecDeque<Stored>) -> usize {
        q.iter().map(|s| s.env.sealed.len()).sum()
    }

    fn prune(&mut self) {
        let limit = now().saturating_sub(TTL_SECS);
        for q in self.boxes.0.values_mut() {
            q.retain(|s| s.at >= limit);
        }
        self.boxes.0.retain(|_, q| !q.is_empty());
        let boxes = &self.boxes.0;
        self.pushes.retain(|to, _| boxes.contains_key(to));
        self.bytes = boxes.values().map(Self::queue_bytes).sum();
    }

    /// Guarda un sobre recibido de la red. No se sabe de quién es: solo se
    /// comprueba que esté bien formado, y los límites son por destinatario.
    pub fn deposit(&mut self, env: Envelope) -> Result<()> {
        if env.sealed.len() > MAX_ENVELOPE {
            bail!("sobre demasiado grande");
        }
        env.check()?;
        self.prune();
        if !self.boxes.0.contains_key(&env.to) && self.boxes.0.len() >= MAX_RECIPIENTS {
            bail!("buzón lleno");
        }
        let to = env.to;
        let size = env.sealed.len();
        let q = self.boxes.0.entry(to).or_default();
        let id = env.id();
        if q.iter().any(|s| s.env.id() == id) {
            return Ok(());
        }
        // Se descartan los más antiguos de ese destinatario: el remitente
        // aún tiene otras copias. Así un destinatario no llena el de otros.
        let mut used = Self::queue_bytes(q);
        let before = used;
        while !q.is_empty() && (q.len() >= MAX_PER_RECIPIENT || used + size > MAX_BYTES_PER_RECIPIENT) {
            used -= q.pop_front().map_or(0, |s| s.env.sealed.len());
        }
        if self.bytes - (before - used) + size > MAX_TOTAL_BYTES {
            if q.is_empty() {
                self.boxes.0.remove(&to);
            }
            // Lo descartado para hacer sitio ya no está.
            self.bytes -= before - used;
            self.dirty |= before != used;
            bail!("buzón lleno");
        }
        self.bytes = self.bytes - (before - used) + size;
        q.push_back(Stored { at: now(), env });
        self.pushes.entry(to).or_insert_with(|| Push { next: first_push(), tries: 0 });
        self.save()
    }

    /// Sobres para `to`, del más antiguo al más reciente, sin borrarlos.
    pub fn fetch(&self, to: &[u8; 32]) -> (Vec<Envelope>, bool) {
        self.take(to, MAX_FETCH_BYTES)
    }

    /// Como `fetch`, hasta `max_bytes`; `true` si quedan más.
    pub fn take(&self, to: &[u8; 32], max_bytes: usize) -> (Vec<Envelope>, bool) {
        let Some(q) = self.boxes.0.get(to) else { return (Vec::new(), false) };
        let (mut out, mut bytes) = (Vec::new(), 0);
        for s in q {
            bytes += s.env.sealed.len();
            if bytes > max_bytes && !out.is_empty() {
                return (out, true);
            }
            out.push(s.env.clone());
        }
        (out, false)
    }

    /// Destinatarios a los que toca intentar entregar ya.
    pub fn due(&mut self) -> Vec<[u8; 32]> {
        self.prune();
        let t = now();
        self.pushes.iter().filter(|(_, p)| p.next <= t).map(|(to, _)| *to).collect()
    }

    /// ¿Hay sobres para `to`? Entonces se le entrega en cuanto aparece.
    pub fn has(&self, to: &[u8; 32]) -> bool {
        self.boxes.0.contains_key(to)
    }

    /// Resultado de un intento de entrega a `to`.
    pub fn pushed(&mut self, to: &[u8; 32], ok: bool) {
        let Some(p) = self.pushes.get_mut(to) else { return };
        if ok {
            *p = Push { next: first_push(), tries: 0 };
        } else {
            p.next = now() + PUSH_BACKOFF[p.tries.min(PUSH_BACKOFF.len() - 1)];
            p.tries += 1;
        }
    }

    /// Borra los sobres confirmados por su destinatario.
    pub fn ack(&mut self, to: &[u8; 32], ids: &[[u8; 32]]) -> Result<()> {
        let Some(q) = self.boxes.0.get_mut(to) else { return Ok(()) };
        let before = Self::queue_bytes(q);
        q.retain(|s| !ids.contains(&s.env.id()));
        self.bytes -= before - Self::queue_bytes(q);
        if q.is_empty() {
            self.boxes.0.remove(to);
            self.pushes.remove(to);
        }
        self.save()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lacre_core::identity::generate_key;

    #[test]
    fn push_schedule() {
        let dir = std::env::temp_dir().join(format!("lacre-mailbox-{}", rand::random::<u64>()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut mb = Mailbox::open(dir.join("mailbox.cbor")).unwrap();
        let (alice, bob) = (generate_key(), generate_key());
        let to = bob.verifying_key().to_bytes();
        let env = Envelope::seal(&alice, &to, b"hola").unwrap();
        mb.deposit(env.clone()).unwrap();
        // Ni al instante (azar y espera) ni tras un fallo.
        assert!(mb.due().is_empty());
        mb.pushes.get_mut(&to).unwrap().next = 0;
        assert_eq!(mb.due(), vec![to]);
        mb.pushed(&to, false);
        assert!(mb.due().is_empty());
        assert!(mb.pushes[&to].next >= now() + PUSH_BACKOFF[0]);
        // Entregado: no queda nada que programar.
        mb.ack(&to, &[env.id()]).unwrap();
        assert!(!mb.has(&to) && mb.pushes.is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn byte_limits() {
        let dir = std::env::temp_dir().join(format!("lacre-mailbox-{}", rand::random::<u64>()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut mb = Mailbox::open(dir.join("mailbox.cbor")).unwrap();
        let alice = generate_key();
        let to = generate_key().verifying_key().to_bytes();
        let big = vec![0u8; MAX_ENVELOPE - 1024];
        let n = MAX_BYTES_PER_RECIPIENT / big.len() + 5;
        for _ in 0..n {
            mb.deposit(Envelope::seal(&alice, &to, &big).unwrap()).unwrap();
        }
        assert!(mb.bytes <= MAX_BYTES_PER_RECIPIENT);
        assert_eq!(mb.bytes, Mailbox::queue_bytes(&mb.boxes.0[&to]));
        mb.flush().unwrap();
        let again = Mailbox::open(dir.join("mailbox.cbor")).unwrap();
        assert_eq!(again.bytes, mb.bytes);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
