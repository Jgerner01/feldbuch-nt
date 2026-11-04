// Zweck:    Projektauswahl: neues oder bestehendes Projekt wählen (Name und Verzeichnis)
// Hardware: keine
// Autor:    Johann Gerner

use std::path::PathBuf;

use eframe::egui;

use crate::daten::projekt::Projekt;

/// Eingabemaske für das Projekt.
pub struct ProjektDialog {
    name: String,
    verzeichnis: String,
    leer_anlegen: bool,
    fehler: String,
}

impl ProjektDialog {
    /// Vorbelegt mit dem aktuellen Projekt.
    pub fn neu() -> Self {
        Self { name: String::new(), verzeichnis: String::new(), leer_anlegen: false, fehler: String::new() }
    }

    /// Zeichnet die Maske; liefert `true`, wenn das Projekt gewechselt wurde.
    pub fn zeichne(&mut self, ui: &mut egui::Ui, projekt: &mut Projekt) -> bool {
        if self.name.is_empty() && self.verzeichnis.is_empty() {
            self.name = projekt.einstellungen.projekt_name.clone();
            self.verzeichnis = projekt.einstellungen.projekt_verzeichnis.to_string_lossy().into_owned();
        }
        ui.heading("Projekt");
        egui::Grid::new("projekt").num_columns(2).spacing([8.0, 8.0]).show(ui, |ui| {
            ui.label("Projektname:");
            ui.text_edit_singleline(&mut self.name);
            ui.end_row();
            ui.label("Verzeichnis:");
            ui.horizontal(|ui| {
                ui.text_edit_singleline(&mut self.verzeichnis);
                if ui.button("Durchsuchen…").clicked() {
                    if let Some(ordner) = rfd::FileDialog::new().pick_folder() {
                        self.verzeichnis = ordner.to_string_lossy().into_owned();
                    }
                }
            });
            ui.end_row();
        });
        ui.checkbox(
            &mut self.leer_anlegen,
            "Leeres neues Projekt (Neupunkt- und Stationierungszähler zurücksetzen)",
        );
        if !self.fehler.is_empty() {
            ui.colored_label(egui::Color32::RED, &self.fehler);
        }
        ui.add_space(8.0);
        if ui.button("Übernehmen").clicked() {
            let verzeichnis = PathBuf::from(self.verzeichnis.trim());
            match projekt.projekt_wechseln(&self.name, &verzeichnis, self.leer_anlegen) {
                Ok(()) => return true,
                Err(fehler) => self.fehler = fehler,
            }
        }
        false
    }
}
