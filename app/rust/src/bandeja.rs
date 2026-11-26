//! Implementación de la bandeja (D-057), fuera de `api/` para que
//! flutter_rust_bridge no la tome por API.

#[cfg(target_os = "linux")]
mod imp {
    use std::sync::Mutex;

    use ksni::menu::StandardItem;
    use ksni::{Icon, MenuItem, ToolTip, Tray, TrayMethods};

    use anyhow::Result;

    use crate::api::tray::{TrayAction, TrayLabels};
    use crate::frb_generated::StreamSink;
    use crate::api::lacre::RT;

    /// Icono de 64×64 en RGBA, sacado de `docs/brand/lacre-icon-flat.svg`.
    const ICON: &[u8] = include_bytes!("../assets/bandeja-64.rgba");

    static HANDLE: Mutex<Option<ksni::Handle<LacreTray>>> = Mutex::new(None);

    struct LacreTray {
        labels: TrayLabels,
        sink: StreamSink<TrayAction>,
    }

    impl Tray for LacreTray {
        fn id(&self) -> String {
            "org.lacre.app".into()
        }

        fn title(&self) -> String {
            self.labels.title.clone()
        }

        fn tool_tip(&self) -> ToolTip {
            ToolTip { title: self.labels.title.clone(), ..Default::default() }
        }

        fn icon_pixmap(&self) -> Vec<Icon> {
            // RGBA → ARGB.
            let data = ICON.as_chunks::<4>().0.iter().flat_map(|&[r, g, b, a]| [a, r, g, b]).collect();
            vec![Icon { width: 64, height: 64, data }]
        }

        fn activate(&mut self, _x: i32, _y: i32) {
            let _ = self.sink.add(TrayAction::Open);
        }

        fn menu(&self) -> Vec<MenuItem<Self>> {
            vec![
                StandardItem {
                    label: self.labels.open.clone(),
                    activate: Box::new(|t: &mut Self| {
                        let _ = t.sink.add(TrayAction::Open);
                    }),
                    ..Default::default()
                }
                .into(),
                MenuItem::Separator,
                StandardItem {
                    label: self.labels.quit.clone(),
                    activate: Box::new(|t: &mut Self| {
                        let _ = t.sink.add(TrayAction::Quit);
                    }),
                    ..Default::default()
                }
                .into(),
            ]
        }
    }

    pub fn start(labels: TrayLabels, sink: StreamSink<TrayAction>) -> Result<()> {
        let mut handle = HANDLE.lock().unwrap();
        // Un suscriptor nuevo (la interfaz se ha reiniciado): se cambia el
        // icono viejo por uno nuevo.
        if let Some(old) = handle.take() {
            RT.block_on(old.shutdown());
        }
        match RT.block_on(LacreTray { labels, sink: sink.clone() }.spawn()) {
            Ok(h) => {
                *handle = Some(h);
                let _ = sink.add(TrayAction::Shown);
            }
            Err(e) => tracing::info!("no system tray: {e}"),
        }
        Ok(())
    }
}

#[cfg(not(target_os = "linux"))]
mod imp {
    use anyhow::Result;

    use crate::api::tray::{TrayAction, TrayLabels};
    use crate::frb_generated::StreamSink;

    pub fn start(_labels: TrayLabels, _sink: StreamSink<TrayAction>) -> Result<()> {
        Ok(())
    }
}

pub use imp::start;
