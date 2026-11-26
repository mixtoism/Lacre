//! Icono en la bandeja del escritorio (D-057): con él, cerrar la ventana la
//! esconde y el nodo sigue recibiendo mensajes.
//!
//! En Linux se habla el protocolo StatusNotifierItem por D-Bus con `ksni`,
//! sin libayatana. Si el escritorio no tiene bandeja (GNOME sin extensión),
//! no llega [`TrayAction::Shown`] y la ventana se cierra como siempre.

use anyhow::Result;

use crate::frb_generated::StreamSink;

#[derive(Clone, Copy)]
pub enum TrayAction {
    /// El icono está en la bandeja: cerrar la ventana ya puede esconderla.
    Shown,
    /// Mostrar la ventana (clic en el icono o «Abrir»).
    Open,
    /// Cerrar la app del todo.
    Quit,
}

/// Textos del icono, en el idioma de la app.
pub struct TrayLabels {
    pub title: String,
    pub open: String,
    pub quit: String,
}

/// Pone el icono en la bandeja; si se puede, el primer aviso es `Shown`.
pub fn tray_start(labels: TrayLabels, sink: StreamSink<TrayAction>) -> Result<()> {
    crate::bandeja::start(labels, sink)
}
