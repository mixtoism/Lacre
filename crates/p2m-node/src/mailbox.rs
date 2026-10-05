//! Buzones: sobres que este nodo guarda para dispositivos desconectados
//! (D-016). El contenido va sellado para el destinatario; aquí solo se ve
//! remitente, destinatario, tamaño y hora.

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;

use anyhow::{Result, bail};
use p2m_core::envelope::Envelope;
use p2m_core::{cbor_from, cbor_to};
use serde::{Deserialize, Serialize};

use crate::app::{now, write_private};
use crate::proto::MAX_ENVELOPE;

/// Sobres por destinatario.
const MAX_PER_RECIPIENT: usize = 500;
/// Destinatarios distintos.
const MAX_RECIPIENTS: usize = 2000;
/// Tiempo máximo que se guarda un sobre.
const TTL_SECS: u64 = 14 * 24 * 3600;
/// Bytes máximos por respuesta a `Fetch` (el límite del protocolo es 10 MiB).
const MAX_FETCH_BYTES: usize = 4 << 20;

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
}

impl Mailbox {
    pub fn open(path: PathBuf) -> Result<Self> {
        let boxes = match std::fs::read(&path) {
            Ok(b) => cbor_from(&b)?,
            Err(_) => Boxes::default(),
        };
        let mut mb = Self { path, boxes };
        mb.prune();
        Ok(mb)
    }

    fn save(&self) -> Result<()> {
        write_private(&self.path, &cbor_to(&self.boxes)?)
    }

    fn prune(&mut self) {
        let limit = now().saturating_sub(TTL_SECS);
        for q in self.boxes.0.values_mut() {
            q.retain(|s| s.at >= limit);
        }
        self.boxes.0.retain(|_, q| !q.is_empty());
    }

    /// Guarda un sobre ya recibido de la red (se comprueba la firma).
    pub fn deposit(&mut self, env: Envelope) -> Result<()> {
        if env.sealed.len() > MAX_ENVELOPE {
            bail!("sobre demasiado grande");
        }
        env.verify()?;
        self.prune();
        if !self.boxes.0.contains_key(&env.to) && self.boxes.0.len() >= MAX_RECIPIENTS {
            bail!("buzón lleno");
        }
        let q = self.boxes.0.entry(env.to).or_default();
        let id = env.id();
        if q.iter().any(|s| s.env.id() == id) {
            return Ok(());
        }
        if q.len() >= MAX_PER_RECIPIENT {
            // Se descarta el más antiguo: el remitente aún tiene otras copias.
            q.pop_front();
        }
        q.push_back(Stored { at: now(), env });
        self.save()
    }

    /// Sobres para `to`, del más antiguo al más reciente, sin borrarlos.
    pub fn fetch(&self, to: &[u8; 32]) -> (Vec<Envelope>, bool) {
        let Some(q) = self.boxes.0.get(to) else { return (Vec::new(), false) };
        let (mut out, mut bytes) = (Vec::new(), 0);
        for s in q {
            bytes += s.env.sealed.len();
            if bytes > MAX_FETCH_BYTES && !out.is_empty() {
                return (out, true);
            }
            out.push(s.env.clone());
        }
        (out, false)
    }

    /// Borra los sobres confirmados por su destinatario.
    pub fn ack(&mut self, to: &[u8; 32], ids: &[[u8; 32]]) -> Result<()> {
        let Some(q) = self.boxes.0.get_mut(to) else { return Ok(()) };
        q.retain(|s| !ids.contains(&s.env.id()));
        if q.is_empty() {
            self.boxes.0.remove(to);
        }
        self.save()
    }
}
