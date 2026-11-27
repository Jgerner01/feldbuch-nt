// Zweck:    GNSS-Betrieb im DXF-Viewer: Live-Position der Antenne mit Spur und Fehlerellipse, Statusanzeige,
//           Werkzeugleiste ohne Tachymeterfunktionen, Messen (Einfachmessung oder Mittelung), GNSS-Punkte im Plan,
//           Startprüfung der Einpassung und Plausibilitätsprüfung des Koordinatenbezugs (GNSS.md Abschnitte 5 und 6);
//           Ergänzungen (Rückfragen, Linie, Exzentrum, Höhenanschluss, Vorhersage …) in gnss_extras.rs
// Hardware: GNSS-Empfänger u-blox ZED-F9P (über das GNSS-Modul)
// Autor:    Johann Gerner

use std::collections::VecDeque;

use chrono::Local;
use eframe::egui::{self, Align, Align2, Color32, FontId, Layout, Pos2, Rect, RichText, Stroke, Vec2};

use super::zeichnen::{self, FeldArt, trenner};
use super::gnss_extras::{ExzentrumFenster, GnssDialog, Linie, VorhersageFenster};
use super::{DxfViewer, Dialog};
use crate::daten::messprotokoll::{self, Eintrag, Ereignis, GnssWerte};
use crate::daten::projekt::Projekt;
use crate::dxf::entity::Ansicht;
use crate::gnss::ansicht::app::{App, ExternProgress, ExternResult, LinkStatus, Live};
use crate::gnss::bewegung::Bewegung;
use crate::gnss::einpassung::{EinpassungsDatei, GnssBezug};
use crate::gnss::kern::config::UTM_ZONE;
use crate::gnss::kern::geo::{Geographic, geo_to_utm};
use crate::gnss::kern::measure::Criteria;
use crate::gnss::kern::model::Fix;
use crate::gnss::punkte::{self as gp, GnssPunkt};
use crate::grundlagen::format::{f, parse_zahl};
use crate::grundlagen::{protokoll, ton};

pub(super) use super::gnss_absteckung::GnssAbsteckung;
pub(super) use super::gnss_einpassung::EinpassFenster;

/// Meter je Millimeter bzw. Millimeter je Meter.
pub(super) const MM_PRO_M: f64 = 1000.0;
/// Zonenkennzahl als Vielfaches im Ostwert.
const ZONEN_FAKTOR: f64 = 1e6;
/// Höchstzahl der Spurpunkte im Plan.
const SPUR_MAX: usize = 600;
/// Ab dieser Entfernung zur Zeichnung wird nach dem Koordinatenbezug gefragt [m].
const PLAUSIBEL_GRENZE_M: f64 = 10_000.0;
/// Größe des Antennensymbols [px].
const ANTENNE_PX: f32 = 9.0;
/// Fehlerellipse: Anzahl Stützpunkte und kleinste sichtbare Halbachse [px].
const ELLIPSE_PUNKTE: usize = 36;
const ELLIPSE_MIN_PX: f64 = 3.0;
/// Größe der GNSS-Punktsymbole [px].
const PUNKT_PX: f32 = 5.0;
/// Farben nach Lösungsart.
pub(super) const FARBE_FIX: Color32 = Color32::from_rgb(40, 200, 70);
pub(super) const FARBE_FLOAT: Color32 = Color32::from_rgb(240, 190, 30);
pub(super) const FARBE_DGPS: Color32 = Color32::from_rgb(240, 130, 30);
pub(super) const FARBE_OHNE: Color32 = Color32::from_rgb(220, 50, 40);
pub(super) const FARBE_ALT: Color32 = Color32::from_gray(140);
/// Farbe der GNSS-Punkte und T-Punkte im Plan.
const FARBE_GNSS_PUNKT: Color32 = Color32::from_rgb(0, 170, 170);
const FARBE_TPUNKT: Color32 = Color32::from_rgb(70, 110, 255);
/// Hintergrund der Infoanzeige.
const INFO_HINTERGRUND: Color32 = Color32::from_rgba_premultiplied(20, 24, 32, 215);
const INFO_RAND: f32 = 8.0;
const INFO_ZEILE: f32 = 17.0;
/// Breiten der Toolbar-Knöpfe.
const MESSEN_BREITE: f32 = 120.0;
const TEXTKNOPF_BREITE: f32 = 86.0;
const KNOPF_HOEHE: f32 = 34.0;

/// Farbe einer Lösungsart.
pub(super) fn fix_farbe(fix: Option<Fix>, veraltet: bool) -> Color32 {
    match fix {
        _ if veraltet => FARBE_ALT,
        Some(Fix::Fixed) => FARBE_FIX,
        Some(Fix::Float) => FARBE_FLOAT,
        Some(Fix::Dgnss) => FARBE_DGPS,
        Some(_) => FARBE_OHNE,
        None => FARBE_ALT,
    }
}

/// Sollpunkt aus dem Plan (Absteckziel, T-Punkt, Kontrollpunkt).
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Soll {
    pub nr: String,
    pub r: f64,
    pub h: f64,
    pub hoehe: Option<f64>,
}

/// Wofür läuft die GNSS-Messung?
#[derive(Clone, Debug)]
pub(super) enum Zweck {
    Neupunkt { nr: String, code: String },
    /// automatisch ausgelöster Punkt der Linienaufnahme
    Linie { nr: String, code: String },
    TPunkt(Soll),
    Kontrolle(Soll),
    Absteckung,
    /// Messung auf einem Höhenfestpunkt
    Hoehenanschluss(Soll),
}

impl Zweck {
    /// Punktnummer der Messung (für Protokolleinträge).
    pub fn nummer(&self) -> String {
        match self {
            Self::Neupunkt { nr, .. } | Self::Linie { nr, .. } => nr.clone(),
            Self::TPunkt(s) | Self::Kontrolle(s) | Self::Hoehenanschluss(s) => s.nr.clone(),
            Self::Absteckung => String::new(),
        }
    }
}

/// Auftrag an die GNSS-Sitzung, ausgeführt nach dem Zeichnen.
pub(super) enum Auftrag {
    Messen { nr: String, code: String, kriterien: Criteria },
    Abbrechen,
    Stab(f64),
}

/// Position im Plansystem.
#[derive(Clone, Copy, Debug)]
pub(super) struct PlanPos {
    pub r: f64,
    pub h: f64,
    pub hoehe: Option<f64>,
    /// Bezug Einpassung, aber keine Einpassung aktiv: UTM-Werte
    pub ohne_einpassung: bool,
    /// außerhalb des Bereichs der T-Punkte
    pub ausserhalb: bool,
}

/// Ausgewertete GNSS-Messung (UTM, Plan, Genauigkeit).
#[derive(Clone, Debug)]
pub(super) struct Messwert {
    pub e: f64,
    pub n: f64,
    pub h_ell: f64,
    pub h_bezug: Option<f64>,
    pub pos: PlanPos,
    pub s_e_mm: f64,
    pub s_n_mm: f64,
    pub s_h_mm: f64,
    pub epochen: usize,
    pub dauer_s: f64,
    pub stab: f64,
    pub pco: f64,
    pub werte: GnssWerte,
    pub quelle: String,
    /// verworfene Epochen
    pub verworfen: usize,
    /// UTC-Zeit des Empfängers am Ende der Messung
    pub zeit_gnss: String,
    /// Grenzen der Messvorgaben für die Streuung [mm]
    pub grenze_lage_mm: f64,
    pub grenze_hoehe_mm: f64,
}

impl Messwert {
    pub fn s_lage_mm(&self) -> f64 {
        self.s_e_mm.hypot(self.s_n_mm)
    }
}

/// Zustand des GNSS-Betriebs im Viewer.
#[derive(Default)]
pub(super) struct GnssZustand {
    /// Messquelle ist GNSS (Gerätetyp „GNSS – SAPOS“)
    pub aktiv: bool,
    initialisiert: bool,
    pub live: Option<Live>,
    pub status: LinkStatus,
    pub fortschritt: Option<ExternProgress>,
    pub kriterien: Option<Criteria>,
    pub stab: f64,
    pub pco: f64,
    pub hoehenbezug: String,
    pub auftraege: Vec<Auftrag>,
    pub laufend: Option<Zweck>,
    pub pos: Option<PlanPos>,
    spur: VecDeque<(f64, f64, Option<Fix>)>,
    letzte_epoche: u32,
    pub folgen: bool,
    /// Mittelung (sonst Einfachmessung)
    pub mittel: bool,
    pub txt_epochen: String,
    pub punkte: Vec<GnssPunkt>,
    pub einpassungen: EinpassungsDatei,
    pub fenster: Option<EinpassFenster>,
    pub absteckung: Option<GnssAbsteckung>,
    plausibel_geprueft: bool,
    startfrage: bool,
    /// UTM-Maßstab am Ort (für die Einpassung mit festem Maßstab)
    pub utm_k: Option<f64>,
    /// Bewegungsrichtung für den Pfeil an der Antenne
    pub bewegung: Bewegung,
    /// zuletzt gespeicherter Neupunkt (Rückfrage zur Stabhöhenkorrektur)
    pub letzte: Option<String>,
    /// letzte gestartete Messung (für „Wiederholen“)
    pub wiederholung: Option<(Zweck, String, String, Criteria)>,
    pub linie: Option<Linie>,
    /// Sitzungsbeginn erkennen (Kontrolle, Rohdaten)
    pub war_verbunden: bool,
    pub aufzeichnung_versucht: bool,
    pub aufzeichnung: Option<(std::path::PathBuf, u64)>,
    /// Höhenbezug in dieser Sitzung bestätigt
    pub hoehenbezug_ok: bool,
    pub exzentrum: Option<ExzentrumFenster>,
    pub vorhersage: Option<VorhersageFenster>,
    /// Höhenanschluss: nächster Klick wählt den Höhenfestpunkt bzw. gewählter Punkt
    pub anschluss_wahl: bool,
    pub anschluss_soll: Option<Soll>,
}

impl GnssZustand {
    /// Plankoordinaten aus UTM (ohne Zone) und Höhe im GNSS-Höhenbezug.
    pub fn plan(&self, bezug: GnssBezug, puffer_prozent: f64, e: f64, n: f64, hoehe: Option<f64>) -> PlanPos {
        // Höhenanschluss gilt, solange keine Höheneinpassung aktiv ist (GNSS.md 12.10)
        let hoeheneinpassung = bezug == GnssBezug::Einpassung && self.einpassungen.aktive().and_then(|ep| ep.ergebnis.as_ref()).is_some_and(|erg| erg.hoehe.is_some());
        let hoehe = match &self.einpassungen.hoehen_anschluss {
            Some(a) if !hoeheneinpassung => hoehe.map(|h| h + a.dz_m),
            _ => hoehe,
        };
        let utm = |zone: bool| PlanPos {
            r: if zone { e + f64::from(UTM_ZONE) * ZONEN_FAKTOR } else { e },
            h: n,
            hoehe,
            ohne_einpassung: false,
            ausserhalb: false,
        };
        match bezug {
            GnssBezug::UtmOhneZone => utm(false),
            GnssBezug::UtmMitZone => utm(true),
            GnssBezug::Einpassung => match self.einpassungen.aktive().and_then(|ep| ep.ergebnis.as_ref()) {
                Some(erg) => {
                    let (r, h, hoehe) = erg.transformiere(e, n, hoehe);
                    PlanPos { r, h, hoehe, ohne_einpassung: false, ausserhalb: erg.ausserhalb(e, n, puffer_prozent) }
                }
                None => PlanPos { ohne_einpassung: true, ..utm(false) },
            },
        }
    }

    /// Id der aktiven Einpassung (leer ohne).
    pub fn einpassung_id(&self, bezug: GnssBezug) -> String {
        if bezug != GnssBezug::Einpassung {
            return String::new();
        }
        self.einpassungen.aktive().filter(|e| e.ergebnis.is_some()).map(|e| e.id.clone()).unwrap_or_default()
    }

    pub fn misst(&self) -> bool {
        self.laufend.is_some()
    }
}

impl DxfViewer {
    // ── Abgleich mit der GNSS-Sitzung ────────────────────────────────────────

    /// Vor dem Zeichnen: Momentaufnahme der Sitzung holen, Ergebnisse verarbeiten, Position und Spur nachführen.
    pub(super) fn gnss_vorher(&mut self, app: &mut App, projekt: &mut Projekt) {
        if !self.gnss.initialisiert {
            self.gnss_initialisiere(app, projekt);
        }
        let stab_alt = self.gnss.stab;
        self.gnss.live = app.live();
        self.gnss.status = app.status();
        self.gnss.fortschritt = app.extern_progress();
        self.gnss.kriterien = Some(app.criteria());
        self.gnss.stab = app.pole_height();
        self.gnss.pco = app.antenna_offset();
        self.gnss.hoehenbezug = app.height_text();
        if (stab_alt - self.gnss.stab).abs() > f64::EPSILON {
            self.txt_zielhoehe = f(self.gnss.stab, 3);
        }
        if let Some(erg) = app.take_extern_result() {
            self.gnss_ergebnis(projekt, &erg);
        } else if self.gnss.laufend.is_some() && self.gnss.fortschritt.is_none() && !self.gnss.status.measuring && self.gnss.auftraege.is_empty() {
            // Messung ohne Ergebnis beendet (Verbindung verloren, Punkt außerhalb des Höhenmodells …)
            self.gnss.laufend = None;
            if let Some((text, true)) = app.meldung() {
                self.status = format!("GNSS-Messung beendet: {text}");
            }
        }
        let Some(live) = self.gnss.live.clone() else {
            self.gnss.pos = None;
            self.gnss_extras_vorher(app, projekt, false);
            return;
        };
        self.gnss.utm_k = Some(geo_to_utm(&Geographic { lat: live.lat, lon: live.lon, h: 0.0 }, UTM_ZONE).k);
        let g = &projekt.rechenparameter.params.gnss;
        let pos = self.gnss.plan(g.bezug, g.extrapolation_prozent, live.east, live.north, live.h);
        self.gnss.pos = Some(pos);
        let neue_epoche = live.itow != self.gnss.letzte_epoche;
        if neue_epoche {
            self.gnss.letzte_epoche = live.itow;
            self.gnss.spur.push_back((pos.r, pos.h, Some(live.fix)));
            while self.gnss.spur.len() > SPUR_MAX {
                self.gnss.spur.pop_front();
            }
            let sekunden = f64::from(live.itow) / MM_PRO_M;
            let min_m = g.ausrichtung_min_m;
            self.gnss.bewegung.neue_position(pos.r, pos.h, sekunden, min_m, false);
            self.gnss_absteckung_epoche(projekt, pos, sekunden);
        }
        if self.gnss.folgen {
            self.mitte = [pos.r, pos.h];
        }
        self.gnss_extras_vorher(app, projekt, neue_epoche);
        self.gnss_pruefe_plausibel(pos);
    }

    /// Nach dem Zeichnen: gesammelte Aufträge an die Sitzung geben.
    pub(super) fn gnss_nachher(&mut self, app: &mut App) {
        for auftrag in std::mem::take(&mut self.gnss.auftraege) {
            match auftrag {
                Auftrag::Messen { nr, code, kriterien } => {
                    if let Err(e) = app.start_extern(&nr, &code, kriterien) {
                        self.gnss.laufend = None;
                        self.status = e;
                        ton::fehler();
                    }
                }
                Auftrag::Abbrechen => app.cancel_extern(),
                Auftrag::Stab(wert) => app.set_pole_height(wert),
            }
        }
    }

    fn gnss_initialisiere(&mut self, app: &App, projekt: &Projekt) {
        self.gnss.initialisiert = true;
        self.gnss.stab = app.pole_height();
        self.txt_zielhoehe = f(self.gnss.stab, 3);
        self.gnss.txt_epochen = app.criteria().epochs.to_string();
        self.gnss.mittel = true;
        self.gnss_lade(projekt);
        let ohne = projekt.rechenparameter.params.gnss.bezug == GnssBezug::Einpassung && self.gnss.einpassungen.aktive().is_none();
        self.gnss.startfrage = ohne && projekt.einstellungen.ist_geladen();
    }

    /// Lädt GNSS-Punkte und Einpassungen des Projekts.
    fn gnss_lade(&mut self, projekt: &Projekt) {
        if !projekt.einstellungen.ist_geladen() {
            return;
        }
        match gp::lese(&gp::pfad(&projekt.einstellungen)) {
            Ok(p) => self.gnss.punkte = p,
            Err(e) => self.status = format!("GNSS-Punkte nicht lesbar: {e}"),
        }
        match EinpassungsDatei::lade(&EinpassungsDatei::pfad(&projekt.einstellungen), self.gnss.utm_k) {
            Ok(d) => self.gnss.einpassungen = d,
            Err(e) => self.dialog = Some(Dialog::Fehler { titel: "Einpassung".into(), text: e }),
        }
    }

    /// Liegt die erste Position weit außerhalb der Zeichnung, wird nach dem Koordinatenbezug gefragt.
    fn gnss_pruefe_plausibel(&mut self, pos: PlanPos) {
        if self.gnss.plausibel_geprueft || self.entities.is_empty() {
            return;
        }
        self.gnss.plausibel_geprueft = true;
        let Some(bb) = self.entities.iter().map(|e| e.bounds()).reduce(|a, b| a.erweitert(&b)) else {
            return;
        };
        let dx = (bb.min_x - pos.r).max(pos.r - bb.max_x).max(0.0);
        let dy = (bb.min_y - pos.h).max(pos.h - bb.max_y).max(0.0);
        let abstand = dx.hypot(dy);
        if abstand > PLAUSIBEL_GRENZE_M && self.dialog.is_none() {
            let hinweis = if pos.ohne_einpassung {
                "Es ist keine Einpassung aktiv. Bitte über „Einpassung“ T-Punkte messen."
            } else {
                "Bitte den Koordinatenbezug (Rechenparameter → GNSS) prüfen: Zonenkennzahl 32 vergessen oder doppelt? \
                 Plan in Gauß-Krüger oder einem örtlichen System? Dann eine Einpassung einrichten."
            };
            self.dialog = Some(Dialog::Hinweis {
                titel: "GNSS-Position außerhalb des Plans".into(),
                text: format!("Die GNSS-Position liegt {} km außerhalb der Zeichnung.\n\n{hinweis}", f(abstand / MM_PRO_M, 1)),
            });
        }
    }

    // ── Werkzeugleiste ───────────────────────────────────────────────────────

    /// Werkzeugleiste im GNSS-Betrieb: ohne Standpunkt, Prisma, EDM, Laser und Instrumentenhöhe.
    pub(super) fn zeichne_toolbar_gnss(&mut self, ui: &mut egui::Ui, projekt: &mut Projekt) {
        ui.spacing_mut().item_spacing = Vec2::new(4.0, 0.0);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.add_space(8.0);
            self.toolbar_feld(ui, "Code", 40.0, 90.0, FeldArt::Code);
            ui.add_space(8.0);
            self.toolbar_feld(ui, "Nr", 22.0, 80.0, FeldArt::PunktNr);
            ui.add_space(8.0);
            let stab = self.toolbar_feld(ui, "Stab", 34.0, 58.0, FeldArt::Zielhoehe).on_hover_text("Lotstabhöhe Boden → Antennenbezugspunkt [m]");
            if stab.lost_focus() {
                match parse_zahl(&self.txt_zielhoehe).filter(|w| w.is_finite() && *w >= 0.0) {
                    Some(w) => {
                        if (w - self.gnss.stab).abs() > f64::EPSILON {
                            self.gnss.auftraege.push(Auftrag::Stab(w));
                            protokoll::log("GNSS", &format!("Stabhöhe {} m", f(w, 3)));
                            messprotokoll::protokolliere(
                                &projekt.einstellungen,
                                Eintrag::neu(Ereignis::Korrektur).aenderung("Stabhoehe_m", &f(self.gnss.stab, 3), &f(w, 3)).quelle(messprotokoll::QUELLE_HAND),
                            );
                            self.gnss.stab = w;
                            self.gnss_stab_geaendert(w);
                        }
                    }
                    None => self.txt_zielhoehe = f(self.gnss.stab, 3),
                }
            }
            ui.add_space(6.0);
            self.zeichne_messen_gnss(ui, projekt);
            ui.add_space(4.0);
            self.zeichne_messart_knopf(ui);
            ui.add_space(6.0);
            trenner(ui);

            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                ui.add_space(4.0);
                let ctx = ui.ctx().clone();
                let kachel = Vec2::splat(KNOPF_HOEHE + 6.0);
                let einpassung_offen = self.gnss.fenster.is_some();
                if textknopf(ui, "Einpassung", einpassung_offen).on_hover_text("Einpassung über T-Punkte: Punkt im Plan wählen und mit GNSS messen").clicked() {
                    self.einpassung_umschalten(projekt);
                }
                ui.add_space(4.0);
                let abstecken = self.gnss.absteckung.is_some();
                if zeichnen::bildknopf(ui, zeichnen::knopfbild("abstecken"), kachel, abstecken.then_some(true))
                    .on_hover_text(if abstecken { "GNSS-Absteckung beenden (Protokoll wird geschrieben)" } else { "GNSS-Absteckung: Zielpunkt im Plan antippen" })
                    .clicked()
                {
                    self.gnss_absteckung_umschalten(projekt);
                }
                ui.add_space(4.0);
                if textknopf(ui, "Folgen", self.gnss.folgen).on_hover_text("Plan mit der Antenne mitführen").clicked() {
                    self.gnss.folgen = !self.gnss.folgen;
                }
                ui.add_space(4.0);
                if textknopf(ui, "Linie", self.gnss.linie.is_some()).on_hover_text("Linienaufnahme: automatisch ein Punkt nach Weg oder Zeit (Rechenparameter → GNSS)").clicked() {
                    self.gnss_linie_umschalten(projekt);
                }
                ui.add_space(4.0);
                let extras = textknopf(ui, "Extras ▾", self.gnss.exzentrum.is_some() || self.gnss.vorhersage.is_some() || self.gnss.anschluss_wahl);
                zeichnen::ausklappmenue(&ctx, "toolbar_gnss_extras", &extras, false, |ui| {
                    ui.vertical(|ui| self.gnss_extras_menue(ui, projekt));
                });
                ui.add_space(6.0);
                let pdf_aktiv = self.pdf_mitte.is_some();
                let ausgabe = zeichnen::bildknopf(ui, zeichnen::knopfbild("pdf"), kachel, pdf_aktiv.then_some(true));
                zeichnen::ausklappmenue(&ctx, "toolbar_ausgabe_gnss", &ausgabe, false, |ui| {
                    let tip = if pdf_aktiv { "PDF-Ausschnitt AN – Klick zum Ausschalten" } else { "PDF-Ausschnitt AUS – Klick zum Einschalten" };
                    if zeichnen::bildknopf(ui, zeichnen::knopfbild("pdf"), kachel, Some(pdf_aktiv).filter(|a| *a)).on_hover_text(tip).clicked() {
                        self.pdf_mitte = if pdf_aktiv { None } else { Some(self.mitte) };
                        self.pdf_ziehen = false;
                    }
                    if zeichnen::bildknopf(ui, zeichnen::knopfbild("dxf_export"), kachel, None).on_hover_text("Als DXF exportieren").clicked() {
                        self.oeffne_export_dialog(projekt);
                    }
                });
            });
        });
    }

    fn zeichne_messen_gnss(&mut self, ui: &mut egui::Ui, projekt: &mut Projekt) {
        let fix = self.gnss.live.as_ref().filter(|l| !l.stale).map(|l| l.fix);
        let (text, tip) = match (&self.gnss.laufend, &self.gnss.fortschritt) {
            (Some(_), Some(p)) => (
                if p.min_seconds > 0.0 { format!("■ {}/{}  {}/{} s", p.accepted, p.epochs, f(p.seconds, 0), f(p.min_seconds, 0)) } else { format!("■ {}/{}  {} s", p.accepted, p.epochs, f(p.seconds, 0)) }, p.last_rejection.map_or("Abbrechen (Esc)".to_string(), |g| format!("Abbrechen – zuletzt verworfen: {g}"))),
            (Some(_), None) => ("■ startet …".to_string(), "Abbrechen".to_string()),
            (None, _) => {
                let ziel = if self.gnss.fenster.as_ref().is_some_and(|f| f.soll.is_some()) {
                    if self.gnss.fenster.as_ref().is_some_and(|f| f.kontrolle) { "► Kontrolle" } else { "► T-Punkt" }
                } else if self.gnss.absteckung.as_ref().is_some_and(|a| a.ziel.is_some()) {
                    "► Prüfen"
                } else if self.gnss.anschluss_soll.is_some() {
                    "► Höhe"
                } else {
                    "► Messen"
                };
                let tip = if fix == Some(Fix::Fixed) { "GNSS-Messung starten (A-Taste)".to_string() } else { "Kein RTK-Fix – Messen erst bei Fix möglich".to_string() };
                (ziel.to_string(), tip)
            }
        };
        let farbe = if self.gnss.laufend.is_some() {
            Color32::from_rgb(160, 60, 20)
        } else if fix == Some(Fix::Fixed) {
            Color32::from_rgb(30, 130, 60)
        } else {
            Color32::from_rgb(110, 110, 110)
        };
        let knopf = egui::Button::new(RichText::new(text).strong().size(12.0).color(Color32::WHITE)).fill(farbe).min_size(Vec2::new(MESSEN_BREITE, KNOPF_HOEHE));
        if ui.add(knopf).on_hover_text(tip).clicked() {
            self.gnss_messen_geklickt(projekt);
        }
    }

    fn zeichne_messart_knopf(&mut self, ui: &mut egui::Ui) {
        if self.gnss.mittel {
            let antwort = ui.add_sized([36.0, 24.0], egui::TextEdit::singleline(&mut self.gnss.txt_epochen).font(FontId::monospace(13.0)));
            antwort.on_hover_text("Anzahl Epochen der Mittelung");
        }
        let (text, tip) = if self.gnss.mittel { ("Mittel", "Mittelung über die eingestellten Epochen (Klick: Einfachmessung)") } else { ("Einfach", "Einfachmessung: eine gültige Epoche (Klick: Mittelung)") };
        if textknopf(ui, text, self.gnss.mittel).on_hover_text(tip).clicked() {
            self.gnss.mittel = !self.gnss.mittel;
        }
    }

    // ── Messen ───────────────────────────────────────────────────────────────

    /// Messen-Knopf oder A-Taste im GNSS-Betrieb.
    pub(super) fn gnss_messen_geklickt(&mut self, projekt: &mut Projekt) {
        if self.gnss.laufend.take().is_some() {
            self.gnss.auftraege.push(Auftrag::Abbrechen);
            self.status = "GNSS-Messung abgebrochen".into();
            return;
        }
        if !self.gnss.status.connected {
            self.status = "GNSS-Empfänger nicht verbunden (Messgerät-Kommunikation oder GNSS-Fenster)".into();
            ton::fehler();
            return;
        }
        // Fix-Wächter: gemessen wird nur mit RTK-Fix
        let fix = self.gnss.live.as_ref().filter(|l| !l.stale).map(|l| l.fix);
        if fix != Some(Fix::Fixed) {
            self.status = format!("Kein RTK-Fix ({}) – Messung nicht gestartet", fix.map_or("keine Position", |f| f.label()));
            ton::fehler();
            return;
        }
        let basis = self.gnss.kriterien.clone().unwrap_or_default();
        let g = projekt.rechenparameter.params.gnss.clone();
        let fenster_soll = self.gnss.fenster.as_ref().and_then(|f| f.soll.clone().map(|s| (s, f.kontrolle)));
        let absteck_ziel = self.gnss.absteckung.as_ref().and_then(|a| a.ziel.clone());
        // Höhenbezug sichtbar und erzwungen: vor jeder gespeicherten Messung (GNSS.md 12.9)
        if absteck_ziel.is_none() && !self.gnss_hoehenbezug_frei(false) {
            return;
        }
        let (zweck, nr, code, kriterien) = if let Some((soll, kontrolle)) = fenster_soll {
            let k = Criteria { epochs: basis.epochs.max(g.tpunkt_epochen), min_seconds: basis.min_seconds.max(g.tpunkt_sekunden), ..basis };
            let nr = soll.nr.clone();
            (if kontrolle { Zweck::Kontrolle(soll) } else { Zweck::TPunkt(soll) }, nr, String::new(), k)
        } else if let Some(ziel) = absteck_ziel {
            let k = Criteria { epochs: g.absteck_epochen.max(1), min_seconds: 0.0, ..basis };
            (Zweck::Absteckung, ziel.nr, String::new(), k)
        } else if let Some(soll) = self.gnss.anschluss_soll.clone() {
            let k = Criteria { epochs: basis.epochs.max(g.tpunkt_epochen), min_seconds: basis.min_seconds.max(g.tpunkt_sekunden), ..basis };
            let nr = soll.nr.clone();
            (Zweck::Hoehenanschluss(soll), nr, String::new(), k)
        } else {
            let nr = self.txt_punkt_nr.trim().to_string();
            if nr.is_empty() {
                self.status = "Punktnummer fehlt".into();
                ton::fehler();
                return;
            }
            let code = self.txt_code.trim().to_string();
            let k = if self.gnss.mittel {
                let epochen = self.gnss.txt_epochen.trim().parse().ok().filter(|n: &usize| *n > 0).unwrap_or(basis.epochs);
                Criteria { epochs: epochen, ..basis }
            } else {
                Criteria { epochs: 1, min_seconds: 0.0, ..basis }
            };
            (Zweck::Neupunkt { nr: nr.clone(), code: code.clone() }, nr, code, k)
        };
        self.status = format!("GNSS-Messung {nr}: {} Epochen{} …", kriterien.epochs, if kriterien.min_seconds > 0.0 { format!(", mindestens {} s", f(kriterien.min_seconds, 0)) } else { String::new() });
        self.gnss_starte(zweck, nr, code, kriterien);
    }

    /// Wertet ein Messergebnis aus und verteilt es nach dem Zweck der Messung.
    fn gnss_ergebnis(&mut self, projekt: &mut Projekt, erg: &ExternResult) {
        let Some(zweck) = self.gnss.laufend.take() else {
            return;
        };
        let m = self.gnss_messwert(projekt, erg);
        // Streuung über den Grenzen: „Trotzdem übernehmen / Wiederholen“ (GNSS.md 6.3)
        let pruefen = matches!(zweck, Zweck::Neupunkt { .. } | Zweck::TPunkt(_) | Zweck::Kontrolle(_) | Zweck::Hoehenanschluss(_));
        if pruefen && Self::gnss_streuung_zu_gross(&m) && self.dialog.is_none() {
            ton::fehler();
            self.status = format!("Streuung zu groß: sL {} mm, sH {} mm", f(m.s_lage_mm(), 1), f(m.s_h_mm, 1));
            self.dialog = Some(Dialog::Gnss(Box::new(GnssDialog::Streuung { zweck, m: Box::new(m) })));
            return;
        }
        self.gnss_verteile(projekt, zweck, m);
    }

    /// Verteilt ein angenommenes Messergebnis nach dem Zweck der Messung.
    pub(super) fn gnss_verteile(&mut self, projekt: &mut Projekt, zweck: Zweck, m: Messwert) {
        match zweck {
            Zweck::Neupunkt { nr, code } => {
                // gleiche Nummer schon gemessen: zweite Belegung, Ersetzen oder Verwerfen (GNSS.md 12.2)
                if self.gnss.punkte.iter().any(|p| p.nr.eq_ignore_ascii_case(&nr)) && self.dialog.is_none() {
                    self.dialog = Some(Dialog::Gnss(Box::new(GnssDialog::Zweitbelegung { nr, code, m: Box::new(m) })));
                } else {
                    self.gnss_neupunkt(projekt, &nr, &code, &m);
                }
            }
            Zweck::Linie { nr, code } => {
                self.gnss_neupunkt(projekt, &nr, &code, &m);
                self.gnss_linie_gemessen(&nr, &m);
            }
            Zweck::TPunkt(soll) => self.einpassung_tpunkt(projekt, &soll, &m),
            Zweck::Kontrolle(soll) => self.einpassung_kontrolle(projekt, &soll, &m),
            Zweck::Absteckung => self.gnss_absteckung_messung(projekt, m),
            Zweck::Hoehenanschluss(soll) => self.gnss_hoehenanschluss(projekt, &soll, &m),
        }
    }

    /// Bereitet das Ergebnis der GNSS-Sitzung auf.
    fn gnss_messwert(&self, projekt: &Projekt, erg: &ExternResult) -> Messwert {
        let r = &erg.result;
        let e = r.east - f64::from(UTM_ZONE) * ZONEN_FAKTOR;
        let h_ist = r.h_dhhn.unwrap_or(r.h_ell);
        let g = &projekt.rechenparameter.params.gnss;
        let pos = self.gnss.plan(g.bezug, g.extrapolation_prozent, e, r.north, Some(h_ist));
        let epochen = &erg.measurement.accepted;
        let anzahl = epochen.len().max(1) as f64;
        let breite = epochen.iter().map(|a| a.0.lat).sum::<f64>() / anzahl;
        let laenge = epochen.iter().map(|a| a.0.lon).sum::<f64>() / anzahl;
        let pdop = epochen.iter().map(|a| a.1.pdop).sum::<f64>() / anzahl;
        let sat = epochen.iter().map(|a| a.1.num_sv).min();
        let einpassung = self.gnss.einpassung_id(g.bezug);
        let mut zusatz: Vec<(String, String)> = vec![
            ("E_UTM".into(), f(e, 4)),
            ("N_UTM".into(), f(r.north, 4)),
            ("H_Bezug".into(), r.h_dhhn.map_or_else(String::new, |h| f(h, 4))),
            ("Bezug".into(), g.bezug.name().into()),
            ("Einpassung".into(), if pos.ohne_einpassung { "ohne".into() } else { einpassung }),
            ("PCO_m".into(), f(erg.measurement.request.antenna_offset, 4)),
        ];
        zusatz.extend(erg.protocol.iter().filter(|(k, _)| !matches!(*k, "h_ell" | "Stabhoehe_m" | "Antennenoffset_m" | "Programm")).map(|(k, v)| ((*k).to_string(), v.clone())));
        let werte = GnssWerte {
            breite,
            laenge,
            h_ell: r.h_ell,
            gen_lage_m: Some(r.sd_e.hypot(r.sd_n)),
            gen_hoehe_m: Some(r.sd_h),
            fix: "RTK_Fixed".into(),
            satelliten: sat,
            pdop: Some(pdop),
            stab_m: erg.measurement.request.pole_height,
            zusatz,
        };
        Messwert {
            e,
            n: r.north,
            h_ell: r.h_ell,
            h_bezug: r.h_dhhn,
            pos,
            s_e_mm: r.sd_e * MM_PRO_M,
            s_n_mm: r.sd_n * MM_PRO_M,
            s_h_mm: r.sd_h * MM_PRO_M,
            epochen: r.n,
            dauer_s: erg.measurement.seconds(),
            stab: erg.measurement.request.pole_height,
            pco: erg.measurement.request.antenna_offset,
            werte,
            quelle: if erg.replay { "WIEDERGABE".into() } else { "GNSS RTK".into() },
            verworfen: r.rejected,
            zeit_gnss: erg.protocol.iter().find(|(k, _)| *k == "GNSS_Zeit_UTC").map(|(_, v)| v.clone()).unwrap_or_default(),
            grenze_lage_mm: erg.measurement.criteria.max_sigma_2d * MM_PRO_M,
            grenze_hoehe_mm: erg.measurement.criteria.max_sigma_h * MM_PRO_M,
        }
    }

    /// Speichert einen GNSS-Neupunkt, schreibt das Messprotokoll und zählt die Nummer weiter.
    pub(super) fn gnss_neupunkt(&mut self, projekt: &mut Projekt, nr: &str, code: &str, m: &Messwert) {
        let jetzt = messprotokoll::zeit_text(Local::now().naive_local());
        let g = &projekt.rechenparameter.params.gnss;
        let punkt = GnssPunkt {
            nr: nr.to_string(),
            code: code.to_string(),
            r: m.pos.r,
            h: m.pos.h,
            hoehe: m.pos.hoehe,
            e_utm: m.e,
            n_utm: m.n,
            h_ell: m.h_ell,
            h_bezug: m.h_bezug,
            s_r_mm: m.s_e_mm,
            s_h_mm: m.s_n_mm,
            s_hoehe_mm: m.s_h_mm,
            epochen: m.epochen,
            dauer_s: m.dauer_s,
            stab_m: m.stab,
            pco_m: m.pco,
            hoehenbezug: self.gnss.hoehenbezug.clone(),
            einpassung_id: if m.pos.ohne_einpassung { String::new() } else { self.gnss.einpassung_id(g.bezug) },
            zeit: jetzt,
            quelle: m.quelle.clone(),
            zeit_gnss: m.zeit_gnss.clone(),
            belegungen: 1,
        };
        let mut eintrag = Eintrag::neu(Ereignis::Messung).punkt(nr, TYP_GNSS, code).koordinaten(m.pos.r, m.pos.h, m.pos.hoehe.unwrap_or(f64::NAN)).mit_gnss(&m.werte);
        if m.pos.ohne_einpassung {
            eintrag = eintrag.text("ohne Einpassung (UTM-Koordinaten)");
        }
        if m.pos.ausserhalb {
            eintrag = eintrag.text("außerhalb des Bereichs der T-Punkte");
        }
        if m.quelle == "WIEDERGABE" {
            eintrag = eintrag.text("Wiedergabe (Simulation)");
        }
        let lfd = messprotokoll::protokolliere(&projekt.einstellungen, eintrag);
        let ersetzt = gp::setze(&mut self.gnss.punkte, punkt);
        if ersetzt {
            messprotokoll::protokolliere(
                &projekt.einstellungen,
                Eintrag::neu(Ereignis::Ersetzt).punkt(nr, TYP_GNSS, "").bezug(lfd).quelle(messprotokoll::QUELLE_PROGRAMM).text("gleiche Nummer erneut gemessen, ersetzt"),
            );
        }
        if let Err(e) = gp::schreibe(&gp::pfad(&projekt.einstellungen), &self.gnss.punkte) {
            self.status = format!("GNSS-Punkt konnte nicht gespeichert werden: {e}");
            ton::fehler();
            return;
        }
        ton::bestaetigung();
        self.gnss.letzte = Some(nr.to_string());
        let mut info = format!("GNSS {nr}: R={}  H={}", f(m.pos.r, 3), f(m.pos.h, 3));
        if let Some(hh) = m.pos.hoehe {
            info.push_str(&format!("  Höhe={}", f(hh, 3)));
        }
        info.push_str(&format!("  sL={} mm  n={}", f(m.s_lage_mm(), 0), m.epochen));
        if m.pos.ohne_einpassung {
            info.push_str("  ⚠ ohne Einpassung");
        }
        if ersetzt {
            info.push_str("  (ersetzt)");
        }
        self.status = info;
        if let Ok(nummer) = nr.parse::<i32>() {
            self.neupunkt_zaehler = nummer.saturating_add(1);
            projekt.einstellungen.neupunkt_zaehler = self.neupunkt_zaehler;
            let _ = projekt.einstellungen.speichern();
            self.txt_punkt_nr = self.neupunkt_zaehler.to_string();
        }
    }

    /// Klick in den Plan im GNSS-Betrieb: Soll für Einpassung oder Absteckung, sonst nur Anzeige.
    pub(super) fn gnss_punkt_gewaehlt(&mut self, projekt: &Projekt, x: f64, y: f64, nr: Option<String>, hoehe: f64) {
        let nr = nr.unwrap_or_else(|| self.txt_punkt_nr.trim().to_string());
        // Höhe 0 bedeutet im Plan „ohne Höhe“
        let soll = Soll { nr, r: x, h: y, hoehe: (hoehe != 0.0).then_some(hoehe) };
        if self.gnss.anschluss_wahl {
            self.gnss_hoehenanschluss_punkt(soll);
            return;
        }
        if let Some(fenster) = self.gnss.fenster.as_mut() {
            if soll.nr.is_empty() {
                fenster.meldung = "Der gewählte Punkt hat keine Nummer – bitte im Feld „Nr“ eintragen und erneut wählen.".into();
                return;
            }
            fenster.meldung = format!("Soll {}: R={}  H={}  Höhe={}", soll.nr, f(soll.r, 3), f(soll.h, 3), soll.hoehe.map_or("–".into(), |h| f(h, 3)));
            fenster.soll = Some(soll);
            return;
        }
        if self.gnss.absteckung.is_some() {
            self.setze_gnss_absteckziel(projekt, soll);
        }
    }

    // ── Zeichnen ─────────────────────────────────────────────────────────────

    /// GNSS-Punkte, T-Punkte, Spur, Absteckziel und Antenne im Plan.
    pub(super) fn zeichne_gnss(&self, maler: &egui::Painter, a: &Ansicht) {
        if self.neupunkte_sichtbar {
            for p in &self.gnss.punkte {
                let q = a.zu_bildschirm(p.r, p.h);
                let raute = [q + Vec2::new(0.0, -PUNKT_PX), q + Vec2::new(PUNKT_PX, 0.0), q + Vec2::new(0.0, PUNKT_PX), q + Vec2::new(-PUNKT_PX, 0.0)];
                maler.add(egui::Shape::convex_polygon(raute.to_vec(), FARBE_GNSS_PUNKT, Stroke::new(1.0, Color32::BLACK)));
                if !self.bewegung_aktiv {
                    maler.text(q + Vec2::new(PUNKT_PX + 2.0, -PUNKT_PX), Align2::LEFT_BOTTOM, &p.nr, FontId::proportional(11.0), FARBE_GNSS_PUNKT);
                }
            }
        }
        // T-Punkte der Einpassung (Fenster hat Vorrang vor der aktiven)
        let tpunkte = self.gnss.fenster.as_ref().map(|f| &f.ep).or_else(|| self.gnss.einpassungen.aktive());
        if let Some(ep) = tpunkte {
            for p in &ep.punkte {
                let q = a.zu_bildschirm(p.soll_r, p.soll_h);
                let farbe = if p.lage_aktiv { FARBE_TPUNKT } else { FARBE_ALT };
                let dreieck = vec![q + Vec2::new(0.0, -8.0), q + Vec2::new(7.0, 5.0), q + Vec2::new(-7.0, 5.0)];
                maler.add(egui::Shape::closed_line(dreieck, Stroke::new(2.0, farbe)));
                maler.text(q + Vec2::new(8.0, 4.0), Align2::LEFT_TOP, format!("T {}", p.nr), FontId::proportional(11.0), farbe);
            }
        }
        if let Some(soll) = self.gnss.fenster.as_ref().and_then(|f| f.soll.as_ref()) {
            let q = a.zu_bildschirm(soll.r, soll.h);
            maler.circle_stroke(q, 11.0, Stroke::new(2.0, Color32::from_rgb(255, 120, 200)));
        }
        if let Some(ziel) = self.gnss.absteckung.as_ref().and_then(|ab| ab.ziel.as_ref()) {
            let q = a.zu_bildschirm(ziel.r, ziel.h);
            maler.circle_stroke(q, 12.0, Stroke::new(2.5, Color32::from_rgb(255, 60, 60)));
            maler.circle_stroke(q, 4.0, Stroke::new(2.0, Color32::from_rgb(255, 60, 60)));
            if let Some(pos) = self.gnss.pos {
                maler.line_segment([a.zu_bildschirm(pos.r, pos.h), q], Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 60, 60, 160)));
            }
        }
        if !self.bewegung_aktiv {
            for &(r, h, fix) in &self.gnss.spur {
                maler.circle_filled(a.zu_bildschirm(r, h), 1.6, fix_farbe(fix, false));
            }
        }
        self.zeichne_gnss_extras(maler, a);
        let (Some(pos), Some(live)) = (self.gnss.pos, self.gnss.live.as_ref()) else {
            return;
        };
        let farbe = fix_farbe(Some(live.fix), live.stale);
        let q = a.zu_bildschirm(pos.r, pos.h);
        // Fehlerellipse (Standardellipse, im Planmaßstab)
        if let Some((gross, klein, azimut_grad)) = live.ellipse {
            if gross * self.scale >= ELLIPSE_MIN_PX {
                let t = azimut_grad.to_radians();
                let punkte: Vec<Pos2> = (0..ELLIPSE_PUNKTE)
                    .map(|i| {
                        let w = std::f64::consts::TAU * i as f64 / ELLIPSE_PUNKTE as f64;
                        let (u, v) = (gross * w.cos(), klein * w.sin());
                        // u entlang der großen Achse (Azimut ab Nord)
                        let (de, dn) = (u * t.sin() - v * t.cos(), u * t.cos() + v * t.sin());
                        a.zu_bildschirm(pos.r + de, pos.h + dn)
                    })
                    .collect();
                maler.add(egui::Shape::closed_line(punkte, Stroke::new(1.5, farbe)));
            }
        }
        maler.circle_stroke(q, ANTENNE_PX, Stroke::new(2.5, farbe));
        maler.line_segment([q - Vec2::new(ANTENNE_PX * 1.6, 0.0), q + Vec2::new(ANTENNE_PX * 1.6, 0.0)], Stroke::new(1.5, farbe));
        maler.line_segment([q - Vec2::new(0.0, ANTENNE_PX * 1.6), q + Vec2::new(0.0, ANTENNE_PX * 1.6)], Stroke::new(1.5, farbe));
        if let Some(richtung) = self.gnss.bewegung.richtung() {
            let laenge = 34.0;
            let v = Vec2::new(richtung.sin() as f32, -(richtung.cos() as f32)) * laenge;
            maler.arrow(q + v * 0.35, v * 0.65, Stroke::new(2.5, farbe));
        }
    }

    /// Statusanzeige oben links (statt Standpunktinfo).
    pub(super) fn zeichne_gnss_info(&self, maler: &egui::Painter, rect: Rect, projekt: &Projekt) {
        let mut zeilen: Vec<(String, Color32)> = Vec::new();
        let s = &self.gnss.status;
        if !s.connected {
            zeilen.push(("GNSS nicht verbunden".into(), FARBE_OHNE));
        } else {
            match self.gnss.live.as_ref() {
                Some(l) => {
                    let seit = s.fix_seconds.filter(|_| l.fix == Fix::Fixed).map(|t| format!(" {}", dauer_text(t))).unwrap_or_default();
                    let sig = |w: Option<f64>| w.map_or("–".into(), |v| f(v * MM_PRO_M, 0));
                    zeilen.push((
                        format!(
                            "{}{seit} · {} Sat · PDOP {} · σL {} mm · σH {} mm · Alter {} s · {}",
                            l.fix.label(),
                            l.num_sv,
                            f(l.pdop, 1),
                            sig(l.sigma_e.zip(l.sigma_n).map(|(e, n)| e.hypot(n))),
                            sig(l.sigma_h),
                            l.age.map_or("–".into(), |a| f(a, 1)),
                            s.mountpoint
                        ),
                        fix_farbe(Some(l.fix), l.stale),
                    ));
                }
                None => zeilen.push(("GNSS verbunden – keine Position".into(), FARBE_OHNE)),
            }
            if s.replay {
                zeilen.push(("WIEDERGABE (Simulation)".into(), Color32::from_rgb(200, 120, 255)));
            }
        }
        let g = &projekt.rechenparameter.params.gnss;
        let bezug = match (g.bezug, self.gnss.einpassungen.aktive()) {
            (GnssBezug::Einpassung, Some(ep)) => match &ep.ergebnis {
                Some(e) => format!("Einpassung {} ({}, {} T-Punkte, s0 {} mm)", ep.id, ep.methode.anzeige(), ep.aktive_lage(), f(e.lage.s0_mm, 0)),
                None => format!("Einpassung {}: {}", ep.id, ep.fehler.clone().unwrap_or_default()),
            },
            (GnssBezug::Einpassung, None) => "⚠ keine Einpassung aktiv – Koordinaten ETRS89/UTM32".into(),
            (b, _) => format!("Koordinaten: {}", b.anzeige()),
        };
        let warn = g.bezug == GnssBezug::Einpassung && self.gnss.einpassungen.aktive().is_none();
        zeilen.push((bezug, if warn { FARBE_FLOAT } else { Color32::from_gray(220) }));
        if self.gnss.pos.is_some_and(|p| p.ausserhalb) {
            zeilen.push(("⚠ außerhalb des Bereichs der T-Punkte".into(), FARBE_FLOAT));
        }
        zeilen.extend(self.gnss_extras_info());
        zeilen.push((format!("Stab {} + PCO {} m · H: {}", f(self.gnss.stab, 3), f(self.gnss.pco, 3), self.gnss.hoehenbezug), Color32::from_gray(200)));
        let breite = zeilen.iter().map(|(t, _)| t.chars().count()).max().unwrap_or(0) as f32 * 7.0 + 2.0 * INFO_RAND;
        let kasten = Rect::from_min_size(rect.left_top() + Vec2::splat(INFO_RAND), Vec2::new(breite, zeilen.len() as f32 * INFO_ZEILE + INFO_RAND));
        maler.rect_filled(kasten, 6.0, INFO_HINTERGRUND);
        for (i, (text, farbe)) in zeilen.iter().enumerate() {
            maler.text(kasten.left_top() + Vec2::new(INFO_RAND, INFO_RAND / 2.0 + i as f32 * INFO_ZEILE), Align2::LEFT_TOP, text, FontId::monospace(12.0), *farbe);
        }
    }

    /// Rückfrage beim Öffnen, wenn der Bezug „Einpassung“ ist, aber keine Einpassung aktiv ist.
    pub(super) fn zeige_gnss_startfrage(&mut self, ctx: &egui::Context, projekt: &mut Projekt) {
        if !self.gnss.startfrage || self.dialog.is_some() {
            return;
        }
        let mut wahl = None;
        egui::Window::new("GNSS – Einpassung")
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                ui.label("Für dieses Projekt ist keine Einpassung aktiv.\nOhne Einpassung werden GNSS-Punkte in ETRS89/UTM32 gespeichert und gekennzeichnet.");
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button(RichText::new("Einpassung einrichten").strong()).clicked() {
                        wahl = Some(1);
                    }
                    let vorhanden = !self.gnss.einpassungen.einpassungen.is_empty();
                    if ui.add_enabled(vorhanden, egui::Button::new("Vorhandene fortsetzen")).on_hover_text("Letzte Einpassung laden; danach einen Kontrollpunkt messen").clicked() {
                        wahl = Some(2);
                    }
                    if ui.button("Ohne Einpassung (UTM32)").clicked() {
                        wahl = Some(3);
                    }
                });
            });
        match wahl {
            Some(1) => {
                self.gnss.startfrage = false;
                self.einpassung_umschalten(projekt);
            }
            Some(2) => {
                self.gnss.startfrage = false;
                self.einpassung_umschalten(projekt);
                if let (Some(fenster), Some(letzte)) = (self.gnss.fenster.as_mut(), self.gnss.einpassungen.einpassungen.last()) {
                    fenster.ep = letzte.clone();
                    fenster.kontrolle = true;
                    fenster.meldung = "Einpassung geladen – bitte einen Kontrollpunkt im Plan wählen und messen, dann übernehmen.".into();
                }
            }
            Some(_) => self.gnss.startfrage = false,
            None => {}
        }
    }
}

/// Typ der GNSS-Messungen im Messprotokoll.
pub(super) const TYP_GNSS: &str = "GNSS";
pub(super) const TYP_GNSS_TPUNKT: &str = "GNSS-TP";
pub(super) const TYP_GNSS_KONTROLLE: &str = "GNSS-KP";
pub(super) const TYP_GNSS_ABSTECKUNG: &str = "GNSS-AB";

/// Textknopf der Werkzeugleiste, hervorgehoben wenn aktiv.
pub(super) fn textknopf(ui: &mut egui::Ui, text: &str, an: bool) -> egui::Response {
    let (fill, rand) = if an { (Color32::from_rgb(210, 120, 20), Color32::from_rgb(255, 170, 40)) } else { (Color32::from_rgb(55, 65, 80), Color32::from_rgb(90, 100, 120)) };
    ui.add(egui::Button::new(RichText::new(text).strong().size(11.0).color(Color32::WHITE)).fill(fill).stroke(Stroke::new(1.0, rand)).min_size(Vec2::new(TEXTKNOPF_BREITE, KNOPF_HOEHE)))
}

/// Dauer als „12 s“, „3 min“ oder „1 h 5 min“.
fn dauer_text(sekunden: f64) -> String {
    const MINUTE: f64 = 60.0;
    const STUNDE: f64 = 3600.0;
    if sekunden < MINUTE {
        format!("{} s", f(sekunden, 0))
    } else if sekunden < STUNDE {
        format!("{} min", f((sekunden / MINUTE).floor(), 0))
    } else {
        format!("{} h {} min", f((sekunden / STUNDE).floor(), 0), f(((sekunden % STUNDE) / MINUTE).floor(), 0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_ohne_und_mit_zone() {
        let z = GnssZustand::default();
        let p = z.plan(GnssBezug::UtmMitZone, 20.0, 690_000.0, 5_430_000.0, Some(400.0));
        assert_eq!(p.r, 32_690_000.0);
        let p = z.plan(GnssBezug::Einpassung, 20.0, 690_000.0, 5_430_000.0, None);
        assert!(p.ohne_einpassung && p.r == 690_000.0);
        assert_eq!(dauer_text(3700.0), "1 h 1 min");
    }
}
