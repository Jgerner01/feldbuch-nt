// Zweck:    Bearbeitung der Rechenparameter (Fehlergrenzen, Maßstab, Gewichtung, Zentrierfehler)
// Hardware: keine
// Autor:    Johann Gerner

use eframe::egui;

use crate::daten::projekt::Projekt;
use crate::gui::bildschirmtastatur;
use crate::daten::rechenparameter::{DXF_GROESSE_BEREICH_MM, DXF_MASSSTAB_BEREICH};
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

        let hoehe = (ui.available_height() - 72.0).max(80.0);
        egui::ScrollArea::vertical().id_salt("rechenparameter_bereiche").max_height(hoehe).show(ui, |ui| {
        ui.columns(3, |spalten| {
            {
                let ui = &mut spalten[0];
                bereich(ui, "Fehlergrenzen und Qualitätsprüfung", egui::Color32::from_rgb(65, 105, 155), |ui| {
                    zahl(ui, "Fehlergrenze Winkel [cc]", &mut p.fehlergrenze_cc_winkel);
                    zahl(ui, "Fehlergrenze Strecke [mm]", &mut p.fehlergrenze_mm_strecke);
                    zahl(ui, "Fehlergrenze Höhe [mm]", &mut p.fehlergrenze_mm_hoehe);
                    zahl(ui, "s0-Grenze Auto-Match [mm]", &mut p.fehlergrenze_mm_s0_auto_match);
                });
                bereich(ui, "Stationierung und Standardabweichung", egui::Color32::from_rgb(55, 120, 80), |ui| {
                    ui.label("Freier Maßstab (Helmert-Näherung, nicht ausgeglichen)");
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

                });
                bereich(ui, "Polarpunkte – Maßstab", egui::Color32::from_rgb(135, 85, 155), |ui| {
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
                    bildschirmtastatur::text_feld(&ui.text_edit_singleline(&mut p.polar_massstab_quelle));
                    ui.end_row();
                });
            }
            {
                let ui = &mut spalten[1];
                bereich(ui, "Gewichtung und Zentrierfehler", egui::Color32::from_rgb(155, 105, 35), |ui| {
                    ui.label("Gewichtung naher Ziele");
                    ui.checkbox(&mut p.gewichtung_nahe_ziele, "");
                    ui.end_row();
                    zahl(ui, "Zentrierfehler Ziel [mm]", &mut p.zentrierfehler_ziel_mm);
                    zahl(ui, "Zentrierfehler Standpunkt [mm]", &mut p.zentrierfehler_standpunkt_mm);
                });
                bereich(ui, "DXF-Viewer – DXF-Export", egui::Color32::from_rgb(60, 100, 160), |ui| {
                    ui.label("Zielmaßstab 1 :");
                    ui.add(
                        egui::DragValue::new(&mut p.dxf.massstab)
                            .range(DXF_MASSSTAB_BEREICH.0..=DXF_MASSSTAB_BEREICH.1)
                            .speed(10.0)
                            .fixed_decimals(0),
                    );
                    ui.end_row();
                    ui.label("Symbolgröße auf dem Papier [mm]");
                    ui.add(
                        egui::DragValue::new(&mut p.dxf.symbol_mm)
                            .range(DXF_GROESSE_BEREICH_MM.0..=DXF_GROESSE_BEREICH_MM.1)
                            .speed(0.1)
                            .fixed_decimals(1),
                    );
                    ui.end_row();
                    ui.label("Textgröße auf dem Papier [mm]");
                    ui.add(
                        egui::DragValue::new(&mut p.dxf.text_mm)
                            .range(DXF_GROESSE_BEREICH_MM.0..=DXF_GROESSE_BEREICH_MM.1)
                            .speed(0.1)
                            .fixed_decimals(1),
                    );
                    ui.end_row();
                });
                bereich(ui, "DXF-Viewer – Flächenberechnung", egui::Color32::from_rgb(0, 130, 150), |ui| {
                    ui.label("Füllfarbe der Fläche");
                    ui.color_edit_button_srgb(&mut p.flaeche_farbe);
                    ui.end_row();
                    ui.label("Grafik im Protokoll");
                    ui.checkbox(&mut p.flaeche_grafik_im_protokoll, "Lageplan mit Punktnummern ergänzen");
                    ui.end_row();
                });
            }
            {
                let ui = &mut spalten[2];
                bereich(ui, "DXF-Viewer – Vektor-PDF", egui::Color32::from_rgb(40, 120, 135), |ui| {
                    ui.label("Papierformat");
                    ui.horizontal(|ui| {
                        ui.radio_value(&mut p.pdf.querformat, false, "A4 Hochformat");
                        ui.radio_value(&mut p.pdf.querformat, true, "A4 Querformat");
                    });
                    ui.end_row();
                    ui.label("Ausgabemaßstab 1 :");
                    ui.add(egui::DragValue::new(&mut p.pdf.massstab).range(1.0..=1_000_000.0).speed(1.0).fixed_decimals(0));
                    ui.end_row();
                    ui.label("Unterer Seitenrand [mm]");
                    ui.add(egui::DragValue::new(&mut p.pdf.rand_mm).range(0.0..=50.0).speed(0.5).fixed_decimals(1));
                    ui.end_row();
                    ui.label("Mindestlinienbreite [mm]");
                    ui.add(egui::DragValue::new(&mut p.pdf.linie_min_mm).range(0.25..=10.0).speed(0.05).fixed_decimals(2));
                    ui.end_row();
                    ui.label("Mindestgröße Punktsymbole [mm]");
                    ui.add(egui::DragValue::new(&mut p.pdf.symbol_min_mm).range(2.0..=20.0).speed(0.1).fixed_decimals(1))
                        .on_hover_text("Äußere Gesamtgröße, bei Kreissymbolen der Durchmesser.");
                    ui.end_row();
                    ui.label("Maßstabsleiste");
                    ui.label("Wird immer mit ausgegeben");
                    ui.end_row();
                });
                bereich(ui, "Absteckung (DXF-Viewer)", egui::Color32::from_rgb(170, 70, 40), |ui| {
                    zahl(ui, "Toleranz Lage (längs/quer) [mm]", &mut p.absteck_toleranz_lage_mm);
                    zahl(ui, "Toleranz Höhe [mm]", &mut p.absteck_toleranz_hoehe_mm);
                });
            }
        });
        ui.label("PDF-Ausschnitt im DXF-Viewer einschalten. Mindestgrößen gelten auf dem Papier.");
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

fn bereich(ui: &mut egui::Ui, titel: &str, farbe: egui::Color32, inhalt: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(egui::Color32::from_rgb(
            230 + farbe.r() / 12,
            230 + farbe.g() / 12,
            230 + farbe.b() / 12,
        ))
        .stroke(egui::Stroke::new(1.0, farbe))
        .corner_radius(5.0)
        .inner_margin(8.0)
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.label(egui::RichText::new(titel).strong().color(farbe));
            ui.add_space(4.0);
            egui::Grid::new(titel).num_columns(2).spacing([12.0, 6.0]).show(ui, inhalt);
        });
    ui.add_space(8.0);
}

fn zahl(ui: &mut egui::Ui, bezeichnung: &str, wert: &mut f64) {
    ui.label(bezeichnung);
    ui.add(egui::DragValue::new(wert).speed(SCHRITT).fixed_decimals(1));
    ui.end_row();
}
