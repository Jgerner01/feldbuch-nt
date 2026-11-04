// Zweck:    Dialog „Als DXF exportieren" des Viewers: Zielmaßstab, Symbol- und Textgröße, Export aller Punktkategorien
// Hardware: keine
// Autor:    Johann Gerner

use eframe::egui::{self, Align2, RichText, Vec2};

use super::{Dialog, DxfViewer};
use crate::daten::projekt::Projekt;
use crate::dxf::entity::Art;
use crate::dxf::exporter::{self, Einstellung, Fehlerellipse};
use crate::grundlagen::datei_speicher;
use crate::grundlagen::format::{f, parse_zahl};

/// Grenzen der Eingaben (wie im Original).
const MASSSTAB_BEREICH: (f64, f64) = (100.0, 50_000.0);
const GROESSE_BEREICH_MM: (f64, f64) = (0.5, 10.0);

impl DxfViewer {
    pub(super) fn oeffne_export_dialog(&mut self) {
        let standard = Einstellung::default();
        self.dialog = Some(Dialog::DxfExport {
            massstab: f(standard.massstab, 0),
            symbol: f(standard.symbol_mm, 1),
            text: f(standard.text_mm, 1),
        });
    }

    /// Fehlerellipsen, die mit exportiert werden (nur bei sichtbaren Residuen).
    fn export_ellipsen(&self) -> Vec<Fehlerellipse> {
        if !self.residuen_sichtbar {
            return Vec::new();
        }
        self.ellipsen
            .iter()
            .filter_map(|e| match &e.art {
                Art::Ellipse { r, h, v_strecke_mm, v_quer_mm, phi, skalierung, .. } => {
                    Some(Fehlerellipse { r: *r, h: *h, v_strecke_mm: *v_strecke_mm, v_quer_mm: *v_quer_mm, phi_rad: *phi, skalierung: *skalierung })
                }
                _ => None,
            })
            .collect()
    }

    /// Zeichnet den Dialog; liefert den Dialog zurück, wenn er offen bleibt.
    pub(super) fn zeichne_export_dialog(&mut self, ctx: &egui::Context, projekt: &Projekt, mut massstab: String, mut symbol: String, mut text: String) {
        let einstellung = Einstellung {
            massstab: parse_zahl(&massstab).unwrap_or(1000.0).clamp(MASSSTAB_BEREICH.0, MASSSTAB_BEREICH.1).round(),
            symbol_mm: parse_zahl(&symbol).unwrap_or(1.5).clamp(GROESSE_BEREICH_MM.0, GROESSE_BEREICH_MM.1),
            text_mm: parse_zahl(&text).unwrap_or(2.0).clamp(GROESSE_BEREICH_MM.0, GROESSE_BEREICH_MM.1),
        };
        let mut aktion: Option<bool> = None;
        egui::Window::new("DXF-Export").collapsible(false).resizable(false).anchor(Align2::CENTER_CENTER, Vec2::ZERO).show(ctx, |ui| {
            ui.label("Alle Punkte (berechnet, eingelesen, gemessen) werden als DXF-Datei geschrieben.");
            egui::Grid::new("dxf_export_felder").num_columns(3).show(ui, |ui| {
                ui.label("Maßstab 1 :");
                ui.add(egui::TextEdit::singleline(&mut massstab).desired_width(80.0));
                ui.label(format!("({} … {})", f(MASSSTAB_BEREICH.0, 0), f(MASSSTAB_BEREICH.1, 0)));
                ui.end_row();
                ui.label("Symbolgröße:");
                ui.add(egui::TextEdit::singleline(&mut symbol).desired_width(80.0));
                ui.label("mm auf dem Papier");
                ui.end_row();
                ui.label("Textgröße:");
                ui.add(egui::TextEdit::singleline(&mut text).desired_width(80.0));
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
            Some(true) => self.exportiere_dxf(projekt, &einstellung),
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
                    if ellipsen.is_empty() { String::new() } else { format!("  {} Fehlerellipse(n)\n", ellipsen.len()) },
                    pfad.display()
                ),
            ),
            Err(fehler) => ("Fehler", format!("Fehler beim Export:\n{fehler}")),
        };
        self.dialog = Some(Dialog::Hinweis { titel: titel.into(), text });
    }
}
