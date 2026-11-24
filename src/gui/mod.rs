// Zweck:    Oberfläche des Feldbuchs (egui): Hauptfenster, Modulauswahl, Meldungen
// Hardware: keine
// Autor:    Johann Gerner

mod absteckung;
pub mod bildschirmtastatur;
mod berechnungen;
mod daten_editor;
mod extraktion_ansicht;
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
mod libelle;
mod stationierungs_hinweis;
mod standpunkt_dialog;
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
    Projektdaten(projektdaten_ansicht::ProjektdatenAnsicht),
    Rechenparameter(rechenparameter_ansicht::RechenparameterAnsicht),
    Kommunikation(kommunikation::KommunikationAnsicht),
    Berechnungen(berechnungen::BerechnungenAnsicht),
    Absteckung(absteckung::AbsteckungAnsicht),
    Datenmanager(Box<daten_editor::DatenEditorAnsicht>),
    Extraktion(extraktion_ansicht::ExtraktionAnsicht),
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
    /// Freie Stationierung als eigenes Fenster.
    freie_stationierung: Option<Box<freie_stationierung::FreieStationierungAnsicht>>,
    /// Polarpunkte als eigenes Fenster.
    polarpunkte: Option<Box<polarpunkte::PolarpunkteAnsicht>>,
    /// Die Rückfrage "Programm beenden?" ist geöffnet.
    beenden_rueckfrage: bool,
    /// Das Beenden wurde bestätigt.
    beenden_bestaetigt: bool,
    meldungen: Vec<String>,
    /// Laufende Suche nach Updates (Ergebnisempfänger, vom Benutzer ausgelöst).
    update_suche: Option<(std::sync::mpsc::Receiver<update_dialog::SuchErgebnis>, bool)>,
    update_dialog: Option<update_dialog::UpdateDialog>,
    /// Die Info-Seite wünscht eine Suche.
    update_pruefen_wunsch: bool,
    /// Die Startgröße (Full HD oder maximiert) wurde bereits festgelegt.
    startgroesse_gesetzt: bool,
    /// Das Hauptfenster wurde für den geöffneten DXF-Viewer minimiert (wird beim Schließen wiederhergestellt).
    hauptfenster_minimiert: bool,
    /// Zuletzt verlassene Konvertierungstabelle; wird beim erneuten Öffnen im selben Projekt weiterverwendet.
    konvertierung_merker: Option<Box<konvertierung::KonvertierungAnsicht>>,
    /// Eigene DXF-Viewer-Instanz zur Punktauswahl für die Konvertierung.
    auswahl_viewer: Option<Box<dxf_viewer::DxfViewer>>,
    /// Erkennt Änderungen von Verbindung, EDM-Modus und Prisma für das Messprotokoll.
    messprotokoll_beobachter: crate::daten::messprotokoll::Beobachter,
}

/// Gewünschte Fenstergröße (Innenfläche) beim Start in Punkten: Full HD.
const START_GROESSE: egui::Vec2 = egui::Vec2::new(1920.0, 1080.0);
/// Platz für Fensterrahmen und Titelleiste, solange die tatsächlichen Maße noch unbekannt sind.
const RAHMEN_RESERVE: egui::Vec2 = egui::Vec2::new(16.0, 40.0);
/// Platz für die Taskleiste.
const TASKLEISTE_RESERVE: f32 = 48.0;

/// Fenstergröße der Freien Stationierung (Innenfläche) in Punkten.
const FENSTER_FREIE_STATIONIERUNG: [f32; 2] = [1000.0, 780.0];
/// Fenstergröße der Polarpunkte (Innenfläche) in Punkten.
const FENSTER_POLARPUNKTE: [f32; 2] = [1150.0, 780.0];
/// Mindestgröße der Modulfenster.
const FENSTER_MIN: [f32; 2] = [760.0, 480.0];

/// Zeigt ein Modul als eigenes Fenster. `inhalt` liefert `true`, wenn das Modul sich selbst schließen will.
/// Der Rückgabewert ist `false`, wenn das Fenster geschlossen wurde.
fn zeige_fenster(
    ctx: &egui::Context,
    kennung: &str,
    titel: &str,
    groesse: [f32; 2],
    mut inhalt: impl FnMut(&mut egui::Ui) -> bool,
) -> bool {
    let id = egui::ViewportId::from_hash_of(kennung);
    let builder = egui::ViewportBuilder::default()
        .with_title(titel)
        .with_inner_size(groesse)
        .with_min_inner_size(FENSTER_MIN);
    let mut offen = true;
    ctx.show_viewport_immediate(id, builder, |ui, _klasse| {
        egui::CentralPanel::default().show(ui, |ui| {
            if inhalt(ui) {
                offen = false;
            }
        });
        if ui.ctx().input(|i| i.viewport().close_requested()) {
            offen = false;
        }
    });
    offen
}

/// Legt die Größe des aktuellen Fensters (Viewport) fest: Full HD, wenn es samt Rahmen und Taskleiste auf den
/// Bildschirm passt, sonst maximiert. Gibt `false` zurück, solange die Bildschirmgröße noch unbekannt ist
/// (dann im nächsten Frame erneut aufrufen). Gilt für das Hauptfenster und den DXF-Viewer.
pub(crate) fn setze_startgroesse_fenster(ctx: &egui::Context) -> bool {
    let (monitor, innen, aussen) = ctx.input(|i| {
        let v = i.viewport();
        (v.monitor_size, v.inner_rect, v.outer_rect)
    });
    let Some(monitor) = monitor else {
        ctx.request_repaint();
        return false;
    };
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
    true
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
            freie_stationierung: None,
            polarpunkte: None,
            beenden_rueckfrage: false,
            beenden_bestaetigt: false,
            meldungen,
            update_suche,
            update_dialog: None,
            update_pruefen_wunsch: false,
            startgroesse_gesetzt: false,
            hauptfenster_minimiert: false,
            konvertierung_merker: None,
            auswahl_viewer: None,
            messprotokoll_beobachter: crate::daten::messprotokoll::Beobachter::default(),
        }
    }

    /// Legt beim Start die Fenstergröße des Hauptfensters fest (siehe [`setze_startgroesse_fenster`]).
    fn setze_startgroesse(&mut self, ctx: &egui::Context) {
        if !self.startgroesse_gesetzt {
            self.startgroesse_gesetzt = setze_startgroesse_fenster(ctx);
        }
    }

    /// Holt das minimierte Hauptfenster zurück und gibt ihm den Fokus.
    fn stelle_hauptfenster_wieder_her(ctx: &egui::Context) {
        ctx.send_viewport_cmd_to(egui::ViewportId::ROOT, egui::ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd_to(egui::ViewportId::ROOT, egui::ViewportCommand::Focus);
    }

    /// Fenstertitel passend zum Projekt.
    fn titel(&self) -> String {
        if self.projekt.einstellungen.ist_geladen() {
            format!("Feldbuch  –  {}", self.projekt.einstellungen.projekt_name)
        } else {
            "Feldbuch".to_string()
        }
    }

    /// Öffnet das Fenster der Freien Stationierung (falls es nicht schon offen ist).
    pub(crate) fn oeffne_freie_stationierung(&mut self) {
        if self.freie_stationierung.is_none() {
            self.freie_stationierung = Some(Box::new(freie_stationierung::FreieStationierungAnsicht::neu(&self.projekt)));
        }
    }

    /// Öffnet das Fenster der Polarpunkte (falls es nicht schon offen ist).
    pub(crate) fn oeffne_polarpunkte(&mut self) {
        if self.polarpunkte.is_none() {
            self.polarpunkte = Some(Box::new(polarpunkte::PolarpunkteAnsicht::neu(&self.projekt)));
        }
    }

    /// Zeigt Freie Stationierung und Polarpunkte als eigene Fenster; Schließen des Fensters beendet nur das Modul.
    fn zeige_modulfenster(&mut self, ctx: &egui::Context) {
        if let Some(ansicht) = self.freie_stationierung.as_mut() {
            let projekt = &mut self.projekt;
            let offen = zeige_fenster(ctx, "freie_stationierung", "Freie Stationierung", FENSTER_FREIE_STATIONIERUNG, |ui| {
                ansicht.zeichne(ui, projekt);
                false
            });
            if !offen {
                self.freie_stationierung = None;
            }
        }
        if let Some(ansicht) = self.polarpunkte.as_mut() {
            let projekt = &mut self.projekt;
            let offen = zeige_fenster(ctx, "polarpunkte", "Polarpunkte", FENSTER_POLARPUNKTE, |ui| ansicht.zeichne(ui, projekt));
            if !offen {
                self.polarpunkte = None;
            }
        }
    }

    /// Fragt beim Schließen des Hauptfensters nach, ob das Programm wirklich beendet werden soll.
    fn pruefe_beenden(&mut self, ctx: &egui::Context) {
        let schliessen_gewuenscht = ctx.input(|i| i.viewport().close_requested());
        if schliessen_gewuenscht && !self.beenden_bestaetigt {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.beenden_rueckfrage = true;
            Self::stelle_hauptfenster_wieder_her(ctx);
        }
        if !self.beenden_rueckfrage {
            return;
        }
        let mut antwort = None;
        egui::Window::new("Programm beenden?")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                ui.label("Soll Feldbuch-NT wirklich beendet werden?");
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button(egui::RichText::new("Ja, beenden").strong()).clicked() {
                        antwort = Some(true);
                    }
                    if ui.button("Nein").clicked() {
                        antwort = Some(false);
                    }
                });
            });
        match antwort {
            Some(true) => {
                self.beenden_bestaetigt = true;
                self.beenden_rueckfrage = false;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            Some(false) => self.beenden_rueckfrage = false,
            None => {}
        }
    }

    /// Wechselt die Ansicht. Eine verlassene Konvertierung wird aufbewahrt, damit ihre Tabelle erhalten bleibt.
    fn setze_ansicht(&mut self, neu: Ansicht) {
        if let Ansicht::Konvertierung(alt) = std::mem::replace(&mut self.ansicht, neu) {
            self.konvertierung_merker = Some(alt);
        }
    }

    /// Die Konvertierung zum Öffnen: die gemerkte Tabelle, wenn sie zum aktuellen Projekt gehört, sonst neu.
    fn konvertierung_oeffnen(&mut self) -> Ansicht {
        let ansicht = match self.konvertierung_merker.take() {
            Some(a) if a.gehoert_zu(&self.projekt) => a,
            _ => Box::new(konvertierung::KonvertierungAnsicht::neu(&self.projekt)),
        };
        Ansicht::Konvertierung(ansicht)
    }

    /// Öffnet den Aufnahmetest (oder bleibt in ihm) und fährt auf Wunsch den Punkt mit dieser Nummer an.
    fn oeffne_aufnahmetest(&mut self, anfahren_nach: Option<&str>) {
        if !matches!(self.ansicht, Ansicht::Aufnahmetest(_)) {
            let neu = Ansicht::Aufnahmetest(Box::new(aufnahmetest::AufnahmetestAnsicht::neu(&self.projekt)));
            self.setze_ansicht(neu);
        }
        if let (Ansicht::Aufnahmetest(a), Some(nr)) = (&mut self.ansicht, anfahren_nach) {
            a.anfahren_nach(nr, &mut self.geraet);
        }
    }

    /// Läuft gerade etwas, das ein Update nicht unterbrechen darf (DXF-Viewer oder ein anderes Modul als Hauptmenü/Info)?
    /// Die Konvertierung, ob gerade angezeigt oder gemerkt.
    fn konvertierung_mut(&mut self) -> Option<&mut konvertierung::KonvertierungAnsicht> {
        match &mut self.ansicht {
            Ansicht::Konvertierung(k) => Some(k),
            _ => self.konvertierung_merker.as_deref_mut(),
        }
    }

    /// DXF-Viewer zur Auswahl für die Konvertierung: eigene Instanz neben einem evtl. offenen Haupt-Viewer.
    /// Öffnet auf Wunsch der Konvertierung und gibt beim Schließen das Ergebnis an sie zurück.
    fn bediene_auswahl_viewer(&mut self, ctx: &egui::Context) {
        if self.auswahl_viewer.is_none() {
            if let Some(anfrage) = self.konvertierung_mut().and_then(|k| k.nimm_auswahl_anfrage()) {
                self.auswahl_viewer = Some(Box::new(dxf_viewer::DxfViewer::fuer_auswahl(&self.projekt, anfrage)));
            }
        }
        let Some(viewer) = self.auswahl_viewer.as_mut() else {
            return;
        };
        if viewer.zeige(ctx, &mut self.projekt, &mut self.geraet) {
            return;
        }
        let ergebnis = viewer.nimm_auswahl_ergebnis();
        self.auswahl_viewer = None;
        if let (Some(ergebnis), Some(k)) = (ergebnis, self.konvertierung_mut()) {
            k.uebernimm_dxf_auswahl(ergebnis);
        }
    }

    fn update_blockiert(&self) -> bool {
        self.auswahl_viewer.is_some()
            || self.dxf_viewer.is_some()
            || self.freie_stationierung.is_some()
            || self.polarpunkte.is_some()
            || !matches!(self.ansicht, Ansicht::Start | Ansicht::Info)
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
        self.meldungen.extend(crate::daten::messprotokoll::hole_fehler());
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
        // Bestätigte Schaltvorgänge (EDM, Laserpointer) als gewünschten Zustand speichern; die Vorgabe wird vor der
        // ersten Messung jeder Verbindung ans Gerät übertragen (nicht beim Programmstart)
        if let Some(z) = self.geraet.nimm_bestaetigung() {
            let e = &mut self.projekt.einstellungen;
            match z {
                geraet::SchalterZiel::Reflektorlos(rl) => {
                    if rl != (e.mess_modus_name != "Prisma") {
                        e.mess_modus_name = if rl { "ReflektorlosKurz" } else { "Prisma" }.to_string();
                    }
                }
                geraet::SchalterZiel::Laser(an) => e.laser_an = an,
            }
            let _ = e.speichern();
        }
        self.geraet.setze_vorgabe(crate::tachymeter::einrichtung::Vorgabe::aus_einstellungen(&self.projekt.einstellungen));
        self.messprotokoll_beobachter.pruefe(&self.projekt.einstellungen, self.geraet.ist_verbunden(), self.geraet.reflektorlos, self.geraet.modell.name());
        if self.geraet.verbindet() || self.geraet.test.is_some() || self.geraet.schalter.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        if let Some(fehler) = self.geraet.verbinde_fehler.take() {
            self.meldungen.push(fehler);
        }
        self.zeige_meldungen(&ctx);
        // Die Neigungsabfrage (Libelle) läuft nur, solange die Kommunikationsmaske sichtbar ist
        self.geraet.live_neigung_aktiv = matches!(self.ansicht, Ansicht::Kommunikation(_));
        hauptfenster::zeichne(self, ui);
        self.bearbeite_update(&ctx);
        self.bediene_auswahl_viewer(&ctx);

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
        // Hauptfenster: beim Öffnen des Viewers minimieren, nach dem Schließen wiederherstellen
        if self.dxf_viewer.is_some() && !self.hauptfenster_minimiert {
            self.hauptfenster_minimiert = true;
            ctx.send_viewport_cmd_to(egui::ViewportId::ROOT, egui::ViewportCommand::Minimized(true));
        } else if self.dxf_viewer.is_none() && self.hauptfenster_minimiert {
            self.hauptfenster_minimiert = false;
            Self::stelle_hauptfenster_wieder_her(&ctx);
        }
        match wunsch {
            Some(dxf_viewer::Wunsch::FreieStationierung) => self.oeffne_freie_stationierung(),
            Some(dxf_viewer::Wunsch::Polarpunkte) => self.oeffne_polarpunkte(),
            Some(dxf_viewer::Wunsch::Aufnahmetest) => {
                // Der Aufnahmetest läuft im Hauptfenster: dieses sichtbar machen
                Self::stelle_hauptfenster_wieder_her(&ctx);
                self.oeffne_aufnahmetest(None);
            }
            Some(dxf_viewer::Wunsch::Anfahren(nr)) => {
                Self::stelle_hauptfenster_wieder_her(&ctx);
                self.oeffne_aufnahmetest(Some(&nr));
            }
            None => {}
        }

        self.zeige_modulfenster(&ctx);
        self.pruefe_beenden(&ctx);
    }

    fn on_exit(&mut self) {
        protokoll::log("ENDE", "Programm beendet");
    }
}
