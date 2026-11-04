// Zweck:    Anzeige und Bearbeitung der Projektdaten (Projektdaten.csv)
// Hardware: keine
// Autor:    Johann Gerner

use eframe::egui;

use super::tabelle::Raster;
use crate::daten::projekt::Projekt;
use crate::daten::projektdaten::ProjektEintrag;

/// Breiten der Spalten (Datum, Uhrzeit, Bearbeiter, Kategorie, Parameter, Wert, Aktion).
const SPALTEN_BREITEN: [f32; 7] = [90.0, 80.0, 110.0, 160.0, 190.0, 260.0, 40.0];
/// Breite der Zeilenköpfe.
const ZEILENKOPF_BREITE: f32 = 32.0;

/// Tabelle der Projektdaten mit Bearbeitung.
#[derive(Default)]
pub struct ProjektdatenAnsicht {
    zeilen: Option<Vec<ProjektEintrag>>,
    status: String,
}

impl ProjektdatenAnsicht {
    pub fn zeichne(&mut self, ui: &mut egui::Ui, projekt: &mut Projekt) {
        let zeilen = self.zeilen.get_or_insert_with(|| projekt.projektdaten.get_all().to_vec());
        ui.heading("Projektdaten");
        ui.horizontal(|ui| {
            if ui.button("Zeile hinzufügen").clicked() {
                zeilen.push(ProjektEintrag {
                    datum: String::new(),
                    uhrzeit: String::new(),
                    bearbeiter: projekt.projektdaten.bearbeiter.clone(),
                    kategorie: String::new(),
                    parameter: String::new(),
                    wert: String::new(),
                });
            }
            if ui.button("Speichern").clicked() {
                self.status = match projekt.projektdaten.replace_all(zeilen.clone()) {
                    Ok(()) => "Gespeichert.".to_string(),
                    Err(fehler) => format!("Fehler beim Speichern: {fehler}"),
                };
            }
            if ui.button("Neu laden").clicked() {
                *zeilen = projekt.projektdaten.get_all().to_vec();
                self.status.clear();
            }
        });
        if !self.status.is_empty() {
            ui.label(&self.status);
        }
        ui.separator();

        let mut loeschen = None;
        let raster = Raster::neu(&SPALTEN_BREITEN).mit_zeilenkopf(ZEILENKOPF_BREITE);
        egui::ScrollArea::both().show(ui, |ui| {
            ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
            raster.kopf(ui, &["Datum", "Uhrzeit", "Bearbeiter", "Kategorie", "Parameter", "Wert", ""]);
            for (i, z) in zeilen.iter_mut().enumerate() {
                raster.zeile(ui, i, &(i + 1).to_string(), |zellen| {
                    zellen.eingabe(&mut z.datum);
                    zellen.eingabe(&mut z.uhrzeit);
                    zellen.eingabe(&mut z.bearbeiter);
                    zellen.eingabe(&mut z.kategorie);
                    zellen.eingabe(&mut z.parameter);
                    zellen.eingabe(&mut z.wert);
                    if zellen.knopf("✕") {
                        loeschen = Some(i);
                    }
                });
            }
        });
        if let Some(i) = loeschen {
            zeilen.remove(i);
        }
    }
}
