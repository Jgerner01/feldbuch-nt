// Zweck:    Bearbeitung der Rechenparameter (Fehlergrenzen, Maßstab, Gewichtung, Zentrierfehler)
// Hardware: keine
// Autor:    Johann Gerner

use eframe::egui;

use crate::daten::projekt::Projekt;
use crate::rechnen::polar::MassstabModus;

/// Größte Schrittweite der Zahlenfelder.
const SCHRITT: f64 = 0.1;

/// Maske der Rechenparameter.
#[derive(Default)]
pub struct RechenparameterAnsicht {
    status: String,
}

impl RechenparameterAnsicht {
    pub fn zeichne(&mut self, ui: &mut egui::Ui, projekt: &mut Projekt) {
        ui.heading("Rechenparameter");
        let p = &mut projekt.rechenparameter.params;

        egui::Grid::new("rechenparameter").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
            zahl(ui, "Fehlergrenze Winkel [cc]", &mut p.fehlergrenze_cc_winkel);
            zahl(ui, "Fehlergrenze Strecke [mm]", &mut p.fehlergrenze_mm_strecke);
            zahl(ui, "Fehlergrenze Höhe [mm]", &mut p.fehlergrenze_mm_hoehe);
            zahl(ui, "s0-Grenze Auto-Match [mm]", &mut p.fehlergrenze_mm_s0_auto_match);

            ui.label("Freier Maßstab");
            ui.checkbox(&mut p.freier_massstab, "");
            ui.end_row();
            ui.label("3D-Berechnung");
            ui.checkbox(&mut p.berechnung_3d, "");
            ui.end_row();

            ui.label("Standardabweichung");
            egui::ComboBox::from_id_salt("sigma")
                .selected_text(p.sigma_bez())
                .show_ui(ui, |ui| {
                    for faktor in 1..=3 {
                        let text = if faktor == 1 { "s₀".to_string() } else { format!("{faktor}·s₀") };
                        ui.selectable_value(&mut p.sigma_faktor, faktor, text);
                    }
                });
            ui.end_row();

            ui.label("Polar: Maßstab");
            egui::ComboBox::from_id_salt("massstab")
                .selected_text(p.polar_massstab_modus.name())
                .show_ui(ui, |ui| {
                    for modus in [MassstabModus::Eins, MassstabModus::Berechnet, MassstabModus::Vorgegeben] {
                        ui.selectable_value(&mut p.polar_massstab_modus, modus, modus.name());
                    }
                });
            ui.end_row();
            ui.label("Polar: vorgegebener Maßstab");
            ui.add(egui::DragValue::new(&mut p.polar_massstab_wert).speed(1e-6).fixed_decimals(6));
            ui.end_row();
            ui.label("Polar: Quelle des Maßstabs");
            ui.text_edit_singleline(&mut p.polar_massstab_quelle);
            ui.end_row();

            ui.label("Gewichtung naher Ziele");
            ui.checkbox(&mut p.gewichtung_nahe_ziele, "");
            ui.end_row();
            zahl(ui, "Zentrierfehler Ziel [mm]", &mut p.zentrierfehler_ziel_mm);
            zahl(ui, "Zentrierfehler Standpunkt [mm]", &mut p.zentrierfehler_standpunkt_mm);
        });

        ui.add_space(8.0);
        if ui.button("Speichern").clicked() {
            self.status = match projekt.rechenparameter.speichern() {
                Ok(()) => "Gespeichert.".to_string(),
                Err(fehler) => format!("Fehler beim Speichern: {fehler}"),
            };
        }
        if !self.status.is_empty() {
            ui.label(&self.status);
        }
    }
}

fn zahl(ui: &mut egui::Ui, bezeichnung: &str, wert: &mut f64) {
    ui.label(bezeichnung);
    ui.add(egui::DragValue::new(wert).speed(SCHRITT).fixed_decimals(1));
    ui.end_row();
}
