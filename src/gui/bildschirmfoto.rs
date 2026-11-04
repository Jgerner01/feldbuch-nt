// Zweck:    Prüfwerkzeug (nur Debug-Build): öffnet über die Umgebungsvariable FELDBUCH_FOTO="<ansicht>=<datei.png>" eine Ansicht,
//           macht nach einigen Frames ein Bildschirmfoto und beendet das Programm. Damit lässt sich die Oberfläche ohne
//           Mausbedienung begutachten. Optional legt FELDBUCH_FOTO_GROESSE="<breite>x<höhe>" die Fenstergröße fest.
// Hardware: keine
// Autor:    Johann Gerner

use std::path::PathBuf;

use eframe::egui;

/// Frames bis zum Foto (Layout und Bilder sollen sich gesetzt haben).
const FRAMES_BIS_FOTO: u32 = 20;

/// Ein Fotoauftrag.
pub struct Fotoauftrag {
    pub ansicht: String,
    datei: PathBuf,
    frames: u32,
    angefordert: bool,
}

impl Fotoauftrag {
    /// Liest den Auftrag aus der Umgebung.
    pub fn aus_umgebung() -> Option<Self> {
        let wert = std::env::var("FELDBUCH_FOTO").ok()?;
        let (ansicht, datei) = wert.split_once('=')?;
        Some(Self { ansicht: ansicht.trim().to_string(), datei: PathBuf::from(datei.trim()), frames: 0, angefordert: false })
    }

    /// Fenstergröße aus FELDBUCH_FOTO_GROESSE.
    pub fn fenstergroesse() -> Option<[f32; 2]> {
        let wert = std::env::var("FELDBUCH_FOTO_GROESSE").ok()?;
        let (b, h) = wert.split_once('x')?;
        Some([b.trim().parse().ok()?, h.trim().parse().ok()?])
    }

    /// Ein Frame weiter; liefert `true`, wenn das Foto gespeichert ist und das Programm enden soll.
    pub fn schritt(&mut self, ctx: &egui::Context) -> bool {
        let bild = ctx.input(|i| {
            i.raw.events.iter().find_map(|e| match e {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(bild) = bild {
            let [b, h] = bild.size;
            let rohdaten: Vec<u8> = bild.pixels.iter().flat_map(|p| p.to_array()).collect();
            if let Some(png) = image::RgbaImage::from_raw(b as u32, h as u32, rohdaten) {
                if let Err(fehler) = png.save(&self.datei) {
                    eprintln!("Foto konnte nicht gespeichert werden: {fehler}");
                }
            }
            return true;
        }
        self.frames += 1;
        if self.frames >= FRAMES_BIS_FOTO && !self.angefordert {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
            self.angefordert = true;
        }
        ctx.request_repaint();
        false
    }
}
