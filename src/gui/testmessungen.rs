// Zweck:    Fenster „Messwerte / Testmessungen": farbiges Terminal der Gerätekommunikation, Messwertanzeige,
//           Messung, EDM-Modus (dreistufig), Winkel-Dauerübertragung, Signaltöne, Laserpointer und animierte
//           Dosenlibelle (GeoCOM)
// Hardware: Tachymeter über gui::geraet (Leica GeoCOM, GSI Online, Topcon)
// Autor:    Johann Gerner

use std::time::{Duration, Instant};

use eframe::egui::{self, Align2, Color32, FontId, Painter, Pos2, Shape, Stroke, Vec2};

use super::geraet::Geraet;
use crate::daten::projekt::Projekt;
use crate::grundlagen::format::f;
use crate::tachymeter::befehle::Befehlsgeber;
use crate::tachymeter::geocom::{RPC_BMM_BEEP_ALARM, RPC_BMM_BEEP_NORMAL, RPC_TMC_GET_ATM_CORR, RPC_TMC_SET_ATM_CORR};
use crate::tachymeter::konfiguration::{Konfiguration, MessModus};
use crate::tachymeter::messablauf::MessErgebnis;
use crate::tachymeter::messung::{MessungsTyp, TachymeterMessung};

/// Abstand der Winkelabfragen und der Libellenabfragen.
const WINKEL_INTERVALL: Duration = Duration::from_millis(500);
const LIBELLE_INTERVALL: Duration = Duration::from_millis(800);
/// Pause zwischen Erfolgs- und Fehlerton des Signaltests.
const SIGNAL_PAUSE: Duration = Duration::from_millis(2000);
/// Auffrischintervall, solange Abfragen laufen.
const REPAINT: Duration = Duration::from_millis(100);
/// Plausibilitätsgrenzen der Atmosphärendaten.
const DRUCK_MIN_MBAR: f64 = 800.0;
const DRUCK_MAX_MBAR: f64 = 1100.0;
const TEMP_MIN_C: f64 = -30.0;
const TEMP_MAX_C: f64 = 55.0;
/// ICAO-Standardatmosphäre und Standard-Wellenlänge.
const ICAO_DRUCK_MBAR: f64 = 1013.25;
const ICAO_TEMP_C: f64 = 15.0;
const STANDARD_LAMBDA_M: f64 = 6.58e-7;
/// Rückgabecode „Kompensator außerhalb des Messbereichs".
const RC_KOMPENSATOR_AUSSERHALB: i32 = 1283;
/// Vollausschlag der Libelle, Kompensatorbereich und Toleranzring.
const MAX_NEIGUNG_RAD: f64 = 0.005;
const KOMP_BEREICH_RAD: f64 = 0.003;
const TOLERANZ_REL: f64 = 0.15;
const KOMP_ANTEIL: f32 = 0.75;
/// Umrechnung Radiant → mgon.
const RAD_ZU_MGON: f64 = 200.0 / std::f64::consts::PI * 1000.0;
/// Höchstzahl der Terminalzeilen, die gezeichnet werden.
const MAX_ZEILEN: usize = 400;
/// Farben des Terminals.
const FARBE_ROH: Color32 = Color32::from_rgb(180, 220, 160);
const FARBE_SENDEN: Color32 = Color32::from_rgb(220, 200, 130);
const FARBE_GEPARST: Color32 = Color32::from_rgb(100, 210, 255);
const FARBE_INFO: Color32 = Color32::from_rgb(140, 150, 170);
const FARBE_FEHLER: Color32 = Color32::from_rgb(255, 110, 110);
const FARBE_TERMINAL: Color32 = Color32::from_rgb(22, 25, 34);

/// Ablauf der Libellen-Startsequenz.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum LibelleZustand {
    Bereit,
    LesePpm,
    SetzePpm,
    Aktiv,
}

/// Fenster der Testmessungen.
pub struct TestmessungenAnsicht {
    winkel_aktiv: bool,
    winkel_naechste: Instant,
    libelle: LibelleZustand,
    libelle_naechste: Instant,
    signal_zweiter: Option<Instant>,
    gesehen_nr: u64,
    /// Neigung: (Quer, Längs) [rad] und Gültigkeit.
    neigung: Option<(f64, f64, bool)>,
    ppm_text: String,
    hz: String,
    v: String,
    sd: String,
    hd: String,
    dh: String,
    modus: MessModus,
    meldung: String,
}

fn terminal_farbe(zeile: &str) -> Color32 {
    if zeile.starts_with(">>") {
        FARBE_SENDEN
    } else if zeile.starts_with("<<") {
        FARBE_ROH
    } else if zeile.contains("!!") || zeile.contains("FEHLER") || zeile.contains("fehlgeschlagen") {
        FARBE_FEHLER
    } else if zeile.starts_with("   ") {
        FARBE_GEPARST
    } else {
        FARBE_INFO
    }
}

/// Zahl in wissenschaftlicher Schreibweise wie .NET „G6" (z. B. 6.58E-07).
fn wissenschaftlich(wert: f64) -> String {
    let text = format!("{wert:.5e}");
    let (mantisse, exponent) = text.split_once('e').unwrap_or((&text, "0"));
    let mantisse = if mantisse.contains('.') { mantisse.trim_end_matches('0').trim_end_matches('.') } else { mantisse };
    let exp: i32 = exponent.parse().unwrap_or(0);
    format!("{mantisse}E{}{:02}", if exp < 0 { '-' } else { '+' }, exp.abs())
}

fn mit_vorzeichen(wert: f64, stellen: usize) -> String {
    if wert < 0.0 { f(wert, stellen) } else { format!("+{}", f(wert, stellen)) }
}

/// Atmosphärische Daten plausibel?
fn atmosphaere_plausibel(m: &TachymeterMessung) -> bool {
    let p = m.atm_druck_mbar.unwrap_or(0.0);
    let t = m.atm_temp_trock_c.unwrap_or(0.0);
    p > DRUCK_MIN_MBAR && p < DRUCK_MAX_MBAR && t > TEMP_MIN_C && t < TEMP_MAX_C && m.atm_ppm.is_some()
}

impl TestmessungenAnsicht {
    pub fn neu(projekt: &Projekt) -> Self {
        Self {
            winkel_aktiv: false,
            winkel_naechste: Instant::now(),
            libelle: LibelleZustand::Bereit,
            libelle_naechste: Instant::now(),
            signal_zweiter: None,
            gesehen_nr: 0,
            neigung: None,
            ppm_text: "PPM: –".to_string(),
            hz: "–".to_string(),
            v: "–".to_string(),
            sd: "–".to_string(),
            hd: "–".to_string(),
            dh: "–".to_string(),
            modus: Konfiguration::aus_einstellungen(&projekt.einstellungen).modus,
            meldung: String::new(),
        }
    }

    fn stoppe_alles(&mut self) {
        self.winkel_aktiv = false;
        self.libelle = LibelleZustand::Bereit;
        self.signal_zweiter = None;
        self.neigung = None;
        self.ppm_text = "PPM: –".to_string();
    }

    // ── Verarbeitung ──────────────────────────────────────────────────────────

    fn aktualisiere(&mut self, geraet: &mut Geraet) {
        if !geraet.ist_verbunden() && (self.winkel_aktiv || self.libelle != LibelleZustand::Bereit) {
            self.stoppe_alles();
        }
        // Messung aus dem Messablauf (Knopf „Messung auslösen")
        match geraet.nimm_messergebnis() {
            Some(MessErgebnis::Messung(m)) => self.zeige_werte(&m),
            Some(MessErgebnis::Fehler(text)) => self.meldung = format!("Messung fehlgeschlagen: {text}"),
            None => {}
        }
        // Neue Antwort des Geräts
        if geraet.messung_nr != self.gesehen_nr {
            self.gesehen_nr = geraet.messung_nr;
            if let Some(m) = geraet.letzte_messung.clone() {
                self.verarbeite(geraet, &m);
            }
        }
        let jetzt = Instant::now();
        let ruhe = geraet.beschaeftigt();
        if let Some(t) = self.signal_zweiter {
            if jetzt >= t {
                self.signal_zweiter = None;
                if geraet.ist_verbunden() {
                    self.meldung = "Signaltest 2/2: Fehlerton (dreifacher Piepton)".to_string();
                    let _ = geraet.sende(&format!("%R1Q,{RPC_BMM_BEEP_ALARM},0:"));
                }
            }
        }
        if self.winkel_aktiv && !ruhe && jetzt >= self.winkel_naechste {
            self.winkel_naechste = jetzt + WINKEL_INTERVALL;
            if let Some(befehl) = Befehlsgeber::fuer(geraet.protokoll()).winkel_befehl() {
                if let Err(fehler) = geraet.sende(&befehl) {
                    self.meldung = fehler;
                    self.winkel_aktiv = false;
                }
            }
        }
        if self.libelle == LibelleZustand::Aktiv && !ruhe && jetzt >= self.libelle_naechste {
            self.libelle_naechste = jetzt + LIBELLE_INTERVALL;
            if let Some(befehl) = Befehlsgeber::fuer(geraet.protokoll()).libelle_befehl() {
                if let Err(fehler) = geraet.sende(&befehl) {
                    self.meldung = fehler;
                    self.libelle = LibelleZustand::Bereit;
                }
            }
        }
    }

    fn zeige_werte(&mut self, m: &TachymeterMessung) {
        if let Some(v) = m.hz_gon {
            self.hz = format!("{} gon", f(v, 4));
        }
        if let Some(v) = m.v_gon {
            self.v = format!("{} gon", f(v, 4));
        }
        if let Some(v) = m.schraegstrecke_m {
            self.sd = format!("{} m", f(v, 4));
        }
        if let Some(v) = m.horizontalstrecke_m {
            self.hd = format!("{} m", f(v, 4));
        }
        if let Some(v) = m.hoehenunterschied_m {
            self.dh = format!("{} m", f(v, 4));
        }
        if let Some(v) = m.e_m {
            self.hz = format!("E: {}", f(v, 3));
        }
        if let Some(v) = m.n_m {
            self.v = format!("N: {}", f(v, 3));
        }
        if let Some(v) = m.h_m {
            self.sd = format!("H: {}", f(v, 3));
        }
    }

    fn verarbeite(&mut self, geraet: &mut Geraet, m: &TachymeterMessung) {
        let rpc = geraet.parser.letzter_rpc;
        match m.typ {
            MessungsTyp::AtmKorrektur => self.verarbeite_atmosphaere(geraet, m),
            MessungsTyp::Fehler | MessungsTyp::Status | MessungsTyp::EdmModusInfo => {}
            _ => {
                if m.hat_winkel() || m.ist_vollmessung() || m.typ == MessungsTyp::Koordinate {
                    self.zeige_werte(m);
                }
                if m.kreuz_neigung_rad.is_some() || m.laengs_neigung_rad.is_some() {
                    self.setze_neigung(m);
                }
            }
        }
        // Startsequenz der Libelle: Atmosphäre gesetzt → Libelle starten
        if self.libelle == LibelleZustand::SetzePpm && m.typ == MessungsTyp::Status && rpc == RPC_TMC_SET_ATM_CORR {
            self.meldung = "Atmosphäre gesetzt. Starte Dosenlibelle Live …".to_string();
            self.libelle = LibelleZustand::Aktiv;
        }
    }

    fn setze_neigung(&mut self, m: &TachymeterMessung) {
        let (kreuz, laengs) = (m.kreuz_neigung_rad.unwrap_or(0.0), m.laengs_neigung_rad.unwrap_or(0.0));
        let ausserhalb = m.return_code == Some(RC_KOMPENSATOR_AUSSERHALB) && kreuz.abs() < 1e-12 && laengs.abs() < 1e-12;
        self.neigung = Some(if ausserhalb { (self.neigung.map_or(0.0, |n| n.0), self.neigung.map_or(0.0, |n| n.1), false) } else { (kreuz, laengs, true) });
    }

    fn verarbeite_atmosphaere(&mut self, geraet: &mut Geraet, m: &TachymeterMessung) {
        if self.libelle != LibelleZustand::LesePpm {
            return;
        }
        if atmosphaere_plausibel(m) {
            let ppm = m.atm_ppm.unwrap_or(0.0);
            let p = m.atm_druck_mbar.unwrap_or(0.0);
            let t = m.atm_temp_trock_c.unwrap_or(0.0);
            self.ppm_text = format!("PPM: {} ppm  (P={} mbar, T={} °C)", mit_vorzeichen(ppm, 2), f(p, 1), f(t, 1));
            self.meldung = format!("Atmosphäre OK: PPM={}  P={} mbar  T={} °C", mit_vorzeichen(ppm, 1), f(p, 1), f(t, 1));
            self.libelle = LibelleZustand::Aktiv;
        } else {
            self.meldung = "Atmosphäre unplausibel → setze ICAO-Standard (P=1013.25 mbar, T=15 °C).".to_string();
            self.ppm_text = "PPM: wird gesetzt (ICAO-Standard)".to_string();
            self.libelle = LibelleZustand::SetzePpm;
            let lambda = m.atm_lambda_m.unwrap_or(STANDARD_LAMBDA_M);
            let befehl = format!("%R1Q,{RPC_TMC_SET_ATM_CORR},0:{},{},{},{}", wissenschaftlich(lambda), f(ICAO_DRUCK_MBAR, 2), f(ICAO_TEMP_C, 1), f(ICAO_TEMP_C, 1));
            let _ = geraet.sende(&befehl);
        }
    }

    // ── Aktionen ──────────────────────────────────────────────────────────────

    fn messen(&mut self, geraet: &mut Geraet) {
        match geraet.starte_messung(0.0) {
            Ok(()) => self.meldung = "Messung läuft …".to_string(),
            Err(fehler) => self.meldung = fehler,
        }
    }

    fn modus_wechseln(&mut self, geraet: &mut Geraet, projekt: &mut Projekt) {
        let neu = self.modus.weiterschalten(true);
        self.modus = neu;
        let mut k = Konfiguration::aus_einstellungen(&projekt.einstellungen);
        k.modus = neu;
        k.in_einstellungen(&mut projekt.einstellungen);
        let _ = projekt.einstellungen.speichern();
        if geraet.ist_verbunden() {
            if let Err(fehler) = geraet.starte_modus_umschaltung(neu) {
                self.meldung = fehler;
            }
        } else {
            self.meldung = "Modus lokal geändert (kein Gerät verbunden).".to_string();
        }
    }

    fn signal_test(&mut self, geraet: &mut Geraet) {
        if !geraet.ist_verbunden() {
            self.meldung = "Kein Tachymeter verbunden.".to_string();
            return;
        }
        if !geraet.protokoll().ist_geocom() {
            self.meldung = "Signaltöne am Instrument gibt es nur mit Leica GeoCOM.".to_string();
            return;
        }
        self.meldung = "Signaltest 1/2: Erfolgston (ein Piepton)".to_string();
        if geraet.sende(&format!("%R1Q,{RPC_BMM_BEEP_NORMAL},0:")).is_ok() {
            self.signal_zweiter = Some(Instant::now() + SIGNAL_PAUSE);
        }
    }

    fn libelle_umschalten(&mut self, geraet: &mut Geraet) {
        if self.libelle != LibelleZustand::Bereit {
            self.libelle = LibelleZustand::Bereit;
            self.neigung = None;
            self.ppm_text = "PPM: –".to_string();
            self.meldung = "Dosenlibelle Live gestoppt.".to_string();
        } else if !geraet.ist_verbunden() {
            self.meldung = "Kein Tachymeter verbunden.".to_string();
        } else {
            self.meldung = "Lese atmosphärische Korrekturdaten (RPC 2029) …".to_string();
            self.libelle = LibelleZustand::LesePpm;
            let _ = geraet.sende(&format!("%R1Q,{RPC_TMC_GET_ATM_CORR},0:"));
        }
    }

    fn winkel_umschalten(&mut self, geraet: &Geraet) {
        if self.winkel_aktiv {
            self.winkel_aktiv = false;
            self.meldung = "Winkel-Dauerübertragung gestoppt.".to_string();
        } else if !geraet.ist_verbunden() {
            self.meldung = "Kein Tachymeter verbunden.".to_string();
        } else {
            self.winkel_aktiv = true;
            self.winkel_naechste = Instant::now();
            self.meldung = "Winkel-Dauerübertragung gestartet (alle 500 ms).".to_string();
        }
    }

    // ── Darstellung ───────────────────────────────────────────────────────────

    pub fn zeichne(&mut self, ui: &mut egui::Ui, projekt: &mut Projekt, geraet: &mut Geraet) {
        self.aktualisiere(geraet);
        if self.winkel_aktiv || self.libelle != LibelleZustand::Bereit || self.signal_zweiter.is_some() || geraet.beschaeftigt() {
            ui.ctx().request_repaint_after(REPAINT);
        }
        let bg = Befehlsgeber::fuer(geraet.protokoll());
        ui.heading(format!("Messwerte / Testmessungen  –  {}", bg.name()));
        ui.horizontal(|ui| {
            let (farbe, text) = if geraet.ist_verbunden() {
                (Color32::from_rgb(40, 160, 60), format!("Verbunden  ({})  –  {} Baud", geraet.einstellung.port, geraet.einstellung.baud))
            } else {
                (Color32::from_rgb(200, 50, 50), "Kein Tachymeter verbunden  –  bitte zuerst im Kommunikationsfenster verbinden".to_string())
            };
            ui.colored_label(farbe, "●");
            ui.colored_label(farbe, text);
        });
        ui.add_space(4.0);

        ui.horizontal_wrapped(|ui| {
            let verbunden = geraet.ist_verbunden();
            let frei = verbunden && !geraet.beschaeftigt();
            if ui.add_enabled(frei, egui::Button::new("► Messung auslösen")).clicked() {
                self.messen(geraet);
            }
            let modus_text = match self.modus {
                MessModus::Prisma => "Modus: Prisma / Reflektor  →  weiter: RL Kurz",
                MessModus::ReflektorlosKurz => "Modus: Reflektorlos Kurz  →  weiter: RL Lang",
                MessModus::ReflektorlosLang => "Modus: Reflektorlos Lang  →  weiter: Prisma",
            };
            let modus_text = if bg.unterstuetzt_edm_modus() { modus_text } else { "EDM-Modus: am Gerät einstellen" };
            if ui.add_enabled(bg.unterstuetzt_edm_modus() && !geraet.beschaeftigt(), egui::Button::new(modus_text)).clicked() {
                self.modus_wechseln(geraet, projekt);
            }
            let winkel_text = if self.winkel_aktiv { "Winkel-Live stoppen" } else { "Winkel-Live starten" };
            if ui.add_enabled(bg.unterstuetzt_winkel_live(), egui::Button::new(winkel_text)).clicked() {
                self.winkel_umschalten(geraet);
            }
            if ui.add_enabled(bg.unterstuetzt_libelle_live(), egui::Button::new(if self.libelle != LibelleZustand::Bereit { "Libelle Live stoppen" } else { "Libelle Live starten" })).clicked() {
                self.libelle_umschalten(geraet);
            }
            if ui.add_enabled(frei && self.signal_zweiter.is_none(), egui::Button::new("Signaltöne testen")).clicked() {
                self.signal_test(geraet);
            }
            let laser_text = if geraet.laser_an { "Laserpointer AUS" } else { "Laserpointer EIN" };
            if ui.add_enabled(bg.unterstuetzt_laserpointer() && frei, egui::Button::new(laser_text)).clicked() {
                if let Err(fehler) = geraet.starte_laser_umschaltung() {
                    self.meldung = fehler;
                }
            }
            if ui.button("Löschen").clicked() {
                geraet.protokoll.clear();
            }
        });
        if !self.meldung.is_empty() {
            ui.colored_label(FARBE_INFO, &self.meldung);
        }
        ui.add_space(4.0);

        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width(300.0);
                self.zeichne_werte(ui);
                ui.add_space(8.0);
                self.zeichne_libelle(ui);
            });
            ui.vertical(|ui| self.zeichne_terminal(ui, geraet));
        });
    }

    fn zeichne_werte(&self, ui: &mut egui::Ui) {
        egui::Grid::new("testwerte").num_columns(2).spacing([12.0, 4.0]).show(ui, |ui| {
            for (name, wert) in [("Hz", &self.hz), ("V", &self.v), ("Schrägstrecke", &self.sd), ("Horizontalstrecke", &self.hd), ("Höhenunterschied", &self.dh)] {
                ui.label(name);
                ui.label(egui::RichText::new(wert).monospace().strong());
                ui.end_row();
            }
        });
    }

    fn zeichne_libelle(&self, ui: &mut egui::Ui) {
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(200.0), egui::Sense::hover());
        zeichne_dosenlibelle(&ui.painter_at(rect), rect, self.neigung);
        let text = match self.neigung {
            None => "Quer:    –\nLängs:   –".to_string(),
            Some((_, _, false)) => "Quer:    außerh. Messbereich\nLängs:   außerh. Messbereich".to_string(),
            Some((kreuz, laengs, true)) => format!(
                "Quer:  {} mgon  ({} mm/m)\nLängs: {} mgon  ({} mm/m)",
                mit_vorzeichen(kreuz * RAD_ZU_MGON, 2),
                mit_vorzeichen(kreuz * 1000.0, 3),
                mit_vorzeichen(laengs * RAD_ZU_MGON, 2),
                mit_vorzeichen(laengs * 1000.0, 3)
            ),
        };
        ui.label(egui::RichText::new(text).monospace());
        ui.label(&self.ppm_text);
    }

    fn zeichne_terminal(&self, ui: &mut egui::Ui, geraet: &Geraet) {
        egui::Frame::new().fill(FARBE_TERMINAL).inner_margin(6.0).show(ui, |ui| {
            egui::ScrollArea::vertical().stick_to_bottom(true).max_height(520.0).auto_shrink([false, false]).show(ui, |ui| {
                ui.set_min_width(560.0);
                let start = geraet.protokoll.len().saturating_sub(MAX_ZEILEN);
                for zeile in geraet.protokoll.iter().skip(start) {
                    ui.label(egui::RichText::new(zeile).monospace().color(terminal_farbe(zeile)));
                }
            });
        });
    }
}

/// Zeichnet die Dosenlibelle mit Dreifuß und Blase.
fn zeichne_dosenlibelle(maler: &Painter, rect: egui::Rect, neigung: Option<(f64, f64, bool)>) {
    let (kreuz, laengs, gueltig) = neigung.unwrap_or((0.0, 0.0, false));
    maler.rect_filled(rect, 4.0, Color32::from_rgb(22, 25, 34));
    let mitte = rect.center();
    let r_aussen = rect.width().min(rect.height()) / 2.0 - 5.0;
    let ecken: Vec<Pos2> = (0..3)
        .map(|i| {
            let w = -std::f32::consts::FRAC_PI_2 + i as f32 * 2.0 * std::f32::consts::PI / 3.0;
            mitte + Vec2::new(w.cos(), w.sin()) * r_aussen
        })
        .collect();
    let kontur = Stroke::new(2.0, Color32::from_rgb(110, 145, 190));
    maler.add(Shape::convex_polygon(ecken.clone(), Color32::from_rgb(32, 38, 52), kontur));
    for p in &ecken {
        maler.circle(*p, 5.0, Color32::from_rgb(80, 120, 170), kontur);
    }
    let r_libelle = r_aussen * 0.68;
    maler.circle_filled(mitte, r_libelle, Color32::from_rgb(18, 30, 50));
    maler.circle_stroke(mitte, r_libelle, Stroke::new(2.5, Color32::from_rgb(80, 150, 230)));
    maler.circle_stroke(mitte, r_libelle * TOLERANZ_REL as f32, Stroke::new(1.0, Color32::from_rgb(120, 160, 180)));
    let r_blase = r_libelle * 0.19;
    let spiel = r_libelle - r_blase * 1.1;
    let r_komp = spiel * KOMP_ANTEIL;
    maler.circle_stroke(mitte, r_komp, Stroke::new(1.3, Color32::from_rgba_unmultiplied(60, 200, 180, 80)));
    let kreuz_stift = Stroke::new(1.0, Color32::from_rgb(55, 80, 110));
    let k = r_libelle * 0.9;
    maler.line_segment([mitte - Vec2::new(k, 0.0), mitte + Vec2::new(k, 0.0)], kreuz_stift);
    maler.line_segment([mitte - Vec2::new(0.0, k), mitte + Vec2::new(0.0, k)], kreuz_stift);

    // Blase: Quer+ → links, Längs+ → unten; zwei Empfindlichkeitszonen
    let abstand = kreuz.hypot(laengs);
    let begrenzt = abstand.min(MAX_NEIGUNG_RAD);
    let pixel = if begrenzt <= KOMP_BEREICH_RAD {
        (begrenzt / KOMP_BEREICH_RAD) as f32 * r_komp
    } else {
        r_komp + ((begrenzt - KOMP_BEREICH_RAD) / (MAX_NEIGUNG_RAD - KOMP_BEREICH_RAD)) as f32 * (spiel - r_komp)
    };
    let pos = if abstand < 1e-12 { mitte } else { mitte + Vec2::new((-kreuz / abstand) as f32, (laengs / abstand) as f32) * pixel };
    let normiert = (abstand / MAX_NEIGUNG_RAD).min(1.0);
    let farbe = if normiert < TOLERANZ_REL {
        Color32::from_rgb(30, 210, 80)
    } else if abstand <= KOMP_BEREICH_RAD {
        Color32::from_rgb(230, 200, 20)
    } else {
        Color32::from_rgb(230, 60, 40)
    };
    maler.circle(pos, r_blase, farbe, Stroke::new(1.2, farbe));
    maler.circle_filled(pos - Vec2::splat(r_blase * 0.3), r_blase * 0.35, Color32::from_rgba_unmultiplied(255, 255, 255, 150));
    let richtung = FontId::proportional(11.0);
    let gedimmt = Color32::from_rgb(90, 120, 160);
    maler.text(mitte - Vec2::new(0.0, r_aussen + 1.0), Align2::CENTER_TOP, "N", richtung.clone(), gedimmt);
    maler.text(mitte + Vec2::new(0.0, r_aussen + 1.0), Align2::CENTER_BOTTOM, "S", richtung, gedimmt);

    if !gueltig && neigung.is_some() {
        maler.circle_stroke(mitte, r_libelle + 2.0, Stroke::new(3.5, Color32::from_rgba_unmultiplied(220, 100, 20, 200)));
        maler.circle_filled(mitte, r_libelle, Color32::from_rgba_unmultiplied(10, 10, 10, 100));
        maler.text(mitte - Vec2::new(0.0, 8.0), Align2::CENTER_CENTER, "!", FontId::proportional(20.0), Color32::from_rgb(220, 180, 60));
        maler.text(mitte + Vec2::new(0.0, 8.0), Align2::CENTER_CENTER, "außerh. Bereich", FontId::proportional(10.0), Color32::from_rgb(200, 120, 40));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tachymeter::geocom;

    #[test]
    fn wissenschaftliche_schreibweise_wie_dotnet() {
        assert_eq!(wissenschaftlich(6.58e-7), "6.58E-07");
        assert_eq!(wissenschaftlich(1.5e3), "1.5E+03");
        assert_eq!(wissenschaftlich(4.0e-10), "4E-10");
    }

    #[test]
    fn terminalfarben() {
        assert_eq!(terminal_farbe(">> %R1Q,2007,0:"), FARBE_SENDEN);
        assert_eq!(terminal_farbe("<< %R1P,0,0:0"), FARBE_ROH);
        assert_eq!(terminal_farbe("   Hz 1.0"), FARBE_GEPARST);
        assert_eq!(terminal_farbe("   !! GRC=3"), FARBE_FEHLER);
        assert_eq!(terminal_farbe("Verbunden"), FARBE_INFO);
    }

    #[test]
    fn atmosphaere_wird_geprueft() {
        let mut m = TachymeterMessung::neu("GeoCOM", "");
        assert!(!atmosphaere_plausibel(&m));
        m.atm_druck_mbar = Some(1000.0);
        m.atm_temp_trock_c = Some(12.0);
        assert!(!atmosphaere_plausibel(&m), "ohne PPM");
        m.atm_ppm = Some(1.5);
        assert!(atmosphaere_plausibel(&m));
        m.atm_druck_mbar = Some(500.0);
        assert!(!atmosphaere_plausibel(&m));
        assert_eq!(mit_vorzeichen(0.0, 2), "+0.00");
        assert_eq!(geocom::rpc_aus_anfrage("%R1Q,11003,0:"), RPC_BMM_BEEP_NORMAL);
    }

    #[test]
    fn neigung_ausserhalb_bleibt_an_letzter_position() {
        let mut a = TestmessungenAnsicht {
            winkel_aktiv: false,
            winkel_naechste: Instant::now(),
            libelle: LibelleZustand::Aktiv,
            libelle_naechste: Instant::now(),
            signal_zweiter: None,
            gesehen_nr: 0,
            neigung: None,
            ppm_text: String::new(),
            hz: String::new(),
            v: String::new(),
            sd: String::new(),
            hd: String::new(),
            dh: String::new(),
            modus: MessModus::Prisma,
            meldung: String::new(),
        };
        let mut m = TachymeterMessung::neu("GeoCOM", "");
        m.kreuz_neigung_rad = Some(0.001);
        m.laengs_neigung_rad = Some(-0.002);
        m.return_code = Some(0);
        a.setze_neigung(&m);
        assert_eq!(a.neigung, Some((0.001, -0.002, true)));
        m.kreuz_neigung_rad = Some(0.0);
        m.laengs_neigung_rad = Some(0.0);
        m.return_code = Some(RC_KOMPENSATOR_AUSSERHALB);
        a.setze_neigung(&m);
        assert_eq!(a.neigung, Some((0.001, -0.002, false)));
    }
}
