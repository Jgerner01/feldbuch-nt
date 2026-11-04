// Zweck:    Oberfläche des Feldbuchs (egui): Hauptfenster, Modulauswahl, Meldungen
// Hardware: keine
// Autor:    Johann Gerner

mod absteckung;
mod berechnungen;
mod daten_editor;
mod flaechenteilung;
mod dxf_viewer;
mod konvertierung;
mod koord_transformation;
mod polarpunkte;
mod testmessungen;
mod freie_stationierung;
mod geraet;
mod hauptfenster;
mod kommunikation;
mod projekt_dialog;
mod projektdaten_ansicht;
mod rechenparameter_ansicht;
mod tabelle;

use eframe::egui;

use crate::daten::projekt::Projekt;
use crate::grundlagen::{datei_speicher, protokoll};

/// Aktuell angezeigtes Modul.
enum Ansicht {
    Start,
    Projekt(projekt_dialog::ProjektDialog),
    FreieStationierung(Box<freie_stationierung::FreieStationierungAnsicht>),
    Projektdaten(projektdaten_ansicht::ProjektdatenAnsicht),
    Rechenparameter(rechenparameter_ansicht::RechenparameterAnsicht),
    Kommunikation(kommunikation::KommunikationAnsicht),
    Berechnungen(berechnungen::BerechnungenAnsicht),
    Absteckung(absteckung::AbsteckungAnsicht),
    Polarpunkte(Box<polarpunkte::PolarpunkteAnsicht>),
    Datenmanager(Box<daten_editor::DatenEditorAnsicht>),
    Konvertierung(Box<konvertierung::KonvertierungAnsicht>),
    Testmessungen(Box<testmessungen::TestmessungenAnsicht>),
    Info,
    NochNichtPortiert(&'static str),
}

/// Die Anwendung.
pub struct FeldbuchApp {
    projekt: Projekt,
    geraet: geraet::Geraet,
    ansicht: Ansicht,
    /// Der DXF-Viewer läuft als eigenes Fenster (wie das Dialogfenster der Originalversion).
    dxf_viewer: Option<Box<dxf_viewer::DxfViewer>>,
    meldungen: Vec<String>,
}

impl FeldbuchApp {
    pub fn neu() -> Self {
        let projekt = Projekt::starte();
        protokoll::log(
            "START",
            &format!(
                "Programm gestartet{}",
                if projekt.einstellungen.ist_geladen() {
                    format!(" | Projekt: {}", projekt.einstellungen.projekt_name)
                } else {
                    String::new()
                }
            ),
        );
        let mut geraet = geraet::Geraet::aus_einstellungen(&projekt.einstellungen);
        let mut meldungen = Vec::new();
        // Gespeichertes Gerät einmalig verbinden (nicht bei GNSS/manuell ohne Port)
        // (im Hintergrund, höchstens 10 s – das Fenster bleibt bedienbar)
        if !geraet.einstellung.port.is_empty() {
            if let Err(fehler) = geraet.starte_verbinden() {
                meldungen.push(format!(
                    "Tachymeter ({}) konnte nicht verbunden werden:\n{fehler}",
                    geraet.einstellung.port
                ));
            }
        }
        Self { projekt, geraet, ansicht: Ansicht::Start, dxf_viewer: None, meldungen }
    }

    /// Fenstertitel passend zum Projekt.
    fn titel(&self) -> String {
        if self.projekt.einstellungen.ist_geladen() {
            format!("Feldbuch  –  {}", self.projekt.einstellungen.projekt_name)
        } else {
            "Feldbuch".to_string()
        }
    }

    /// Zeigt aufgelaufene Fehlermeldungen als Fenster.
    fn zeige_meldungen(&mut self, ctx: &egui::Context) {
        self.meldungen.extend(datei_speicher::hole_meldungen());
        if self.meldungen.is_empty() {
            return;
        }
        let mut schliessen = false;
        egui::Window::new("Hinweis")
            .collapsible(false)
            .resizable(true)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                    for text in &self.meldungen {
                        ui.label(text);
                        ui.separator();
                    }
                });
                if ui.button("OK").clicked() {
                    schliessen = true;
                }
            });
        if schliessen {
            self.meldungen.clear();
        }
    }
}

impl eframe::App for FeldbuchApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(self.titel()));
        // Gerät: Verbindungsversuch und Empfang verarbeiten, auch wenn das Kommunikationsfenster zu ist
        self.geraet.aktualisiere();
        if self.geraet.verbindet() || self.geraet.test.is_some() || self.geraet.schalter.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        if let Some(fehler) = self.geraet.verbinde_fehler.take() {
            self.meldungen.push(fehler);
        }
        self.zeige_meldungen(&ctx);
        hauptfenster::zeichne(self, ui);

        // DXF-Viewer als eigenes Fenster
        let mut wunsch = None;
        if let Some(viewer) = self.dxf_viewer.as_mut() {
            let offen = viewer.zeige(&ctx, &mut self.projekt, &mut self.geraet);
            wunsch = viewer.nimm_wunsch();
            if !offen {
                self.dxf_viewer = None;
            }
        }
        if self.dxf_viewer.is_none() {
            self.geraet.live_hz_aktiv = false;
        }
        if let Some(dxf_viewer::Wunsch::FreieStationierung) = wunsch {
            self.ansicht = Ansicht::FreieStationierung(Box::default());
        }
    }

    fn on_exit(&mut self) {
        protokoll::log("ENDE", "Programm beendet");
    }
}
