// Zweck:    Dialog „Als DXF exportieren" des Viewers: Zielmaßstab, Symbol- und Textgröße, Export aller Punktkategorien
// Hardware: keine
// Autor:    Johann Gerner

use eframe::egui::{self, Align2, RichText, Vec2};

use super::{Dialog, DxfViewer};
use crate::daten::projekt::Projekt;
use crate::daten::rechenparameter::{DXF_GROESSE_BEREICH_MM as GROESSE_BEREICH_MM, DXF_MASSSTAB_BEREICH as MASSSTAB_BEREICH, DxfParameter};
use crate::dxf::entity::Art;
use crate::dxf::exporter::{self, Einstellung, Ellipse};
use crate::grundlagen::datei_speicher;
use crate::grundlagen::format::{f, parse_zahl};

/// Breite des Maßstab-Eingabefelds (vierstellige Zahlen passen bequem hinein).
const MASSSTAB_FELD_BREITE: f32 = 90.0;
/// Höhe der Eingabefelder.
const FELD_HOEHE: f32 = 24.0;
/// Breite der Eingabefelder für Symbol- und Textgröße.
const GROESSE_FELD_BREITE: f32 = 80.0;

impl DxfViewer {
    /// Öffnet den Dialog mit den zuletzt in den Rechenparametern abgelegten Werten (sonst Standardwerte).
    pub(super) fn oeffne_export_dialog(&mut self, projekt: &Projekt) {
        let standard = projekt.rechenparameter.params.dxf.normalisiert();
        self.dialog = Some(Dialog::DxfExport {
            massstab: f(standard.massstab, 0),
            symbol: f(standard.symbol_mm, 1),
            text: f(standard.text_mm, 1),
        });
    }

    /// Residuen- und Konfidenzellipsen, die mit exportiert werden (nur bei sichtbaren Residuen).
    fn export_ellipsen(&self) -> Vec<Ellipse> {
        if !self.residuen_sichtbar {
            return Vec::new();
        }
        self.ellipsen
            .iter()
            .filter_map(|e| match &e.art {
                Art::Ellipse { r, h, v_strecke_mm, v_quer_mm, phi, skalierung, konfidenz, .. } => Some(Ellipse {
                    r: *r,
                    h: *h,
                    v_strecke_mm: *v_strecke_mm,
                    v_quer_mm: *v_quer_mm,
                    phi_rad: *phi,
                    skalierung: *skalierung,
                    konfidenz: *konfidenz,
                }),
                _ => None,
            })
            .collect()
    }

    /// Zeichnet den Dialog; liefert den Dialog zurück, wenn er offen bleibt.
    pub(super) fn zeichne_export_dialog(&mut self, ctx: &egui::Context, projekt: &mut Projekt, mut massstab: String, mut symbol: String, mut text: String) {
        let standard = DxfParameter::default();
        let einstellung = Einstellung {
            massstab: parse_zahl(&massstab).unwrap_or(standard.massstab).clamp(MASSSTAB_BEREICH.0, MASSSTAB_BEREICH.1).round(),
            symbol_mm: parse_zahl(&symbol).unwrap_or(standard.symbol_mm).clamp(GROESSE_BEREICH_MM.0, GROESSE_BEREICH_MM.1),
            text_mm: parse_zahl(&text).unwrap_or(standard.text_mm).clamp(GROESSE_BEREICH_MM.0, GROESSE_BEREICH_MM.1),
        };
        let mut aktion: Option<bool> = None;
        egui::Window::new("DXF-Export").collapsible(false).resizable(false).anchor(Align2::CENTER_CENTER, Vec2::ZERO).show(ctx, |ui| {
            ui.label("Alle Punkte (berechnet, eingelesen, gemessen) werden als DXF-Datei geschrieben.");
            egui::Grid::new("dxf_export_felder").num_columns(3).show(ui, |ui| {
                ui.label("Maßstab 1 :");
                ui.add_sized([MASSSTAB_FELD_BREITE, FELD_HOEHE], egui::TextEdit::singleline(&mut massstab));
                ui.label(format!("({} … {})", f(MASSSTAB_BEREICH.0, 0), f(MASSSTAB_BEREICH.1, 0)));
                ui.end_row();
                ui.label("Symbolgröße:");
                ui.add(egui::TextEdit::singleline(&mut symbol).desired_width(GROESSE_FELD_BREITE));
                ui.label("mm auf dem Papier");
                ui.end_row();
                ui.label("Textgröße:");
                ui.add(egui::TextEdit::singleline(&mut text).desired_width(GROESSE_FELD_BREITE));
                ui.label("mm auf dem Papier");
                ui.end_row();
            });
            ui.label(format!(
                "Symbolradius {} m · Texthöhe {} m",
                f(einstellung.symbol_radius(), 3),
                f(einstellung.text_hoehe(), 3)
            ));
            ui.horizontal(|ui| {
                if ui.button(RichText::new("Exportieren").strong()).clicked() {
                    aktion = Some(true);
                }
                if ui.button("Abbrechen").clicked() {
                    aktion = Some(false);
                }
            });
        });
        match aktion {
            None => self.dialog = Some(Dialog::DxfExport { massstab, symbol, text }),
            Some(false) => {}
            Some(true) => {
                // Werte in den Rechenparametern ablegen, damit sie beim nächsten Export wieder vorbelegt sind
                projekt.rechenparameter.params.dxf =
                    DxfParameter { massstab: einstellung.massstab, symbol_mm: einstellung.symbol_mm, text_mm: einstellung.text_mm };
                if let Err(e) = projekt.rechenparameter.speichern() {
                    self.status = format!("DXF-Vorgaben konnten nicht gespeichert werden: {e}");
                }
                self.exportiere_dxf(projekt, &einstellung);
            }
        }
    }

    fn exportiere_dxf(&mut self, projekt: &Projekt, einstellung: &Einstellung) {
        let feldbuch = projekt.feldbuchpunkte.punkte();
        let import = projekt.importpunkte.punkte();
        let gemessen = projekt.neupunkte.koordinaten();
        if feldbuch.len() + import.len() + gemessen.len() == 0 {
            self.dialog = Some(Dialog::Hinweis {
                titel: "Keine Daten".into(),
                text: "Keine Punkte vorhanden.\nBitte zuerst Koordinaten importieren, Punkte messen\noder eine Freie Stationierung berechnen.".into(),
            });
            return;
        }
        let vorschlag = if projekt.einstellungen.ist_geladen() {
            format!("{}_Feldbuch.dxf", projekt.einstellungen.projekt_name)
        } else {
            "Feldbuch.dxf".to_string()
        };
        let mut dialog = rfd::FileDialog::new().set_title("Feldbuchpunkte exportieren").add_filter("DXF-Dateien", &["dxf"]).set_file_name(vorschlag);
        if projekt.einstellungen.ist_geladen() {
            dialog = dialog.set_directory(&projekt.einstellungen.projekt_verzeichnis);
        }
        let Some(pfad) = dialog.save_file() else {
            return;
        };
        let ellipsen = self.export_ellipsen();
        let dxf = exporter::erzeuge(feldbuch, import, gemessen, einstellung, &ellipsen);
        let (titel, text) = match datei_speicher::schreibe_text(&pfad, &dxf) {
            Ok(()) => (
                "Export erfolgreich",
                format!(
                    "Export erfolgreich:\n  {} berechnete Punkt(e)\n  {} eingelesene Koordinate(n)\n  {} gemessene Punkt(e)\n{}\n{}",
                    feldbuch.len(),
                    import.len(),
                    gemessen.len(),
                    if ellipsen.is_empty() { String::new() } else { format!("  {} Ellipse(n) (Residuen- bzw. Konfidenzellipsen)\n", ellipsen.len()) },
                    pfad.display()
                ),
            ),
            Err(fehler) => ("Fehler", format!("Fehler beim Export:\n{fehler}")),
        };
        self.dialog = Some(Dialog::Hinweis { titel: titel.into(), text });
    }
}
