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
mod aufnahmetest;
mod testmessungen;
mod update_dialog;
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
use crate::update;

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
    Aufnahmetest(Box<aufnahmetest::AufnahmetestAnsicht>),
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
    /// Laufende Suche nach Updates (Ergebnisempfänger, vom Benutzer ausgelöst).
    update_suche: Option<(std::sync::mpsc::Receiver<update_dialog::SuchErgebnis>, bool)>,
    update_dialog: Option<update_dialog::UpdateDialog>,
    /// Die Info-Seite wünscht eine Suche.
    update_pruefen_wunsch: bool,
    /// Die Startgröße (Full HD oder maximiert) wurde bereits festgelegt.
    startgroesse_gesetzt: bool,
}

/// Gewünschte Fenstergröße (Innenfläche) beim Start in Punkten: Full HD.
const START_GROESSE: egui::Vec2 = egui::Vec2::new(1920.0, 1080.0);
/// Platz für Fensterrahmen und Titelleiste, solange die tatsächlichen Maße noch unbekannt sind.
const RAHMEN_RESERVE: egui::Vec2 = egui::Vec2::new(16.0, 40.0);
/// Platz für die Taskleiste.
const TASKLEISTE_RESERVE: f32 = 48.0;

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
        // Beim Start still nach Updates suchen (nur in veröffentlichten Versionen); Reste früherer Updates aufräumen
        let update_suche = update::ist_veroeffentlicht().then(|| {
            update::raeume_auf(&update::update_ordner());
            (update_dialog::starte_suche(), false)
        });
        Self {
            projekt,
            geraet,
            ansicht: Ansicht::Start,
            dxf_viewer: None,
            meldungen,
            update_suche,
            update_dialog: None,
            update_pruefen_wunsch: false,
            startgroesse_gesetzt: false,
        }
    }

    /// Legt beim Start die Fenstergröße fest: Full HD, wenn es samt Rahmen und Taskleiste auf den Bildschirm passt,
    /// sonst maximiert. Gilt für das Hauptfenster und damit für alle darin angezeigten Module.
    fn setze_startgroesse(&mut self, ctx: &egui::Context) {
        if self.startgroesse_gesetzt {
            return;
        }
        let (monitor, innen, aussen) = ctx.input(|i| {
            let v = i.viewport();
            (v.monitor_size, v.inner_rect, v.outer_rect)
        });
        let Some(monitor) = monitor else {
            // Bildschirmgröße noch nicht bekannt: im nächsten Frame erneut versuchen
            ctx.request_repaint();
            return;
        };
        self.startgroesse_gesetzt = true;
        let rahmen = match (innen, aussen) {
            (Some(innen), Some(aussen)) if aussen.size().x >= innen.size().x => aussen.size() - innen.size(),
            _ => RAHMEN_RESERVE,
        };
        let aussen_groesse = START_GROESSE + rahmen;
        if monitor.x >= aussen_groesse.x && monitor.y - TASKLEISTE_RESERVE >= aussen_groesse.y {
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(START_GROESSE));
            let links = ((monitor.x - aussen_groesse.x) / 2.0).max(0.0);
            let oben = ((monitor.y - TASKLEISTE_RESERVE - aussen_groesse.y) / 2.0).max(0.0);
            ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(links, oben)));
        } else {
            ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(true));
        }
    }

    /// Fenstertitel passend zum Projekt.
    fn titel(&self) -> String {
        if self.projekt.einstellungen.ist_geladen() {
            format!("Feldbuch  –  {}", self.projekt.einstellungen.projekt_name)
        } else {
            "Feldbuch".to_string()
        }
    }

    /// Öffnet den Aufnahmetest (oder bleibt in ihm) und fährt auf Wunsch den Punkt mit dieser Nummer an.
    fn oeffne_aufnahmetest(&mut self, anfahren_nach: Option<&str>) {
        if !matches!(self.ansicht, Ansicht::Aufnahmetest(_)) {
            self.ansicht = Ansicht::Aufnahmetest(Box::new(aufnahmetest::AufnahmetestAnsicht::neu(&self.projekt)));
        }
        if let (Ansicht::Aufnahmetest(a), Some(nr)) = (&mut self.ansicht, anfahren_nach) {
            a.anfahren_nach(nr, &mut self.geraet);
        }
    }

    /// Läuft gerade etwas, das ein Update nicht unterbrechen darf (DXF-Viewer oder ein anderes Modul als Hauptmenü/Info)?
    fn update_blockiert(&self) -> bool {
        self.dxf_viewer.is_some() || !matches!(self.ansicht, Ansicht::Start | Ansicht::Info)
    }

    /// Wertet das Ergebnis einer Update-Suche aus. Beim Start bleibt alles still, bei manueller Suche gibt es immer eine Rückmeldung.
    fn update_suche_fertig(&mut self, ergebnis: update_dialog::SuchErgebnis, manuell: bool) {
        match ergebnis {
            Ok(Some(info)) => {
                let basis = crate::grundlagen::pfade::basis();
                if !update::ist_ordner_schreibbar(&basis) {
                    if manuell {
                        self.meldungen.push(format!(
                            "Version {} ist verfügbar, aber der Programmordner ist nicht beschreibbar:\n{}\n\nBitte die neue Version von der GitHub-Release-Seite herunterladen:\n{}",
                            info.version,
                            basis.display(),
                            update::RELEASE_SEITE
                        ));
                    }
                    return;
                }
                // Beim Start nur fragen, solange nichts anderes offen ist, damit keine laufende Arbeit gestört wird
                if (manuell || !self.update_blockiert()) && self.update_dialog.is_none() {
                    self.update_dialog = Some(update_dialog::UpdateDialog::neu(info));
                }
            }
            Ok(None) => {
                if manuell {
                    self.meldungen.push(format!("Version {} ist aktuell.", update::Version::aktuell()));
                }
            }
            Err(fehler) => {
                crate::grundlagen::fehler_log::log("Update-Prüfung", &fehler);
                if manuell {
                    self.meldungen.push("Die Suche nach Updates ist fehlgeschlagen. Bitte die Internetverbindung prüfen.".to_string());
                }
            }
        }
    }

    /// Treibt Suche und Update-Fenster voran.
    fn bearbeite_update(&mut self, ctx: &egui::Context) {
        if std::mem::take(&mut self.update_pruefen_wunsch) && self.update_suche.is_none() {
            if update::ist_veroeffentlicht() {
                self.update_suche = Some((update_dialog::starte_suche(), true));
            } else {
                self.meldungen.push("Updates sind nur in der veröffentlichten Programmversion möglich.".to_string());
            }
        }
        if let Some((empfaenger, manuell)) = self.update_suche.take() {
            match empfaenger.try_recv() {
                Ok(ergebnis) => self.update_suche_fertig(ergebnis, manuell),
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    self.update_suche = Some((empfaenger, manuell));
                    ctx.request_repaint_after(std::time::Duration::from_millis(200));
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {}
            }
        }
        let blockiert = self.update_blockiert();
        let Some(dialog) = self.update_dialog.as_mut() else {
            return;
        };
        match dialog.zeige(ctx, blockiert) {
            update_dialog::Aktion::Weiter => {}
            update_dialog::Aktion::Schliessen => self.update_dialog = None,
            update_dialog::Aktion::Neustart(ordner) => {
                self.update_dialog = None;
                match update::starte_installation(&ordner, &crate::grundlagen::pfade::basis()) {
                    Ok(()) => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
                    Err(fehler) => self.meldungen.push(format!("Das Update konnte nicht gestartet werden:\n{fehler}")),
                }
            }
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
        self.setze_startgroesse(&ctx);
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
        self.bearbeite_update(&ctx);

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
        match wunsch {
            Some(dxf_viewer::Wunsch::FreieStationierung) => {
                self.ansicht = Ansicht::FreieStationierung(Box::new(freie_stationierung::FreieStationierungAnsicht::neu(&self.projekt)));
            }
            Some(dxf_viewer::Wunsch::Aufnahmetest) => self.oeffne_aufnahmetest(None),
            Some(dxf_viewer::Wunsch::Anfahren(nr)) => self.oeffne_aufnahmetest(Some(&nr)),
            None => {}
        }
    }

    fn on_exit(&mut self) {
        protokoll::log("ENDE", "Programm beendet");
    }
}
