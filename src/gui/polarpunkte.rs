// Zweck:    Polarpunktberechnung: Neupunkte aus den Messungen der {Projekt}-dat.csv. Orientierung aus gespeicherter
//           Station oder neu aus den Anschlusspunkten (Ampel, einzelne Beobachtungen schaltbar, Gewichtung bei nahen
//           Zielen), Längs-/Querexzentrizität je Punkt, Maßstab nach Rechenparameter, Speichern nach Neupunkte.json
//           und als KOR-CSV
// Hardware: keine
// Autor:    Johann Gerner

use std::path::{Path, PathBuf};

use chrono::NaiveDateTime;
use eframe::egui::{self, Color32, RichText};

use super::tabelle::{Raster, Zellen};
use crate::daten::kor_csv::{self, KorPunkt};
use crate::daten::messdaten;
use crate::daten::modelle::{NeupunktErgebnis, NeupunktRohdaten, PunktResiduum, StationierungsErgebnis, StationierungsPunkt, TYP_NEUPUNKT, jetzt};
use crate::daten::polar_messdaten::{self, PolarMessdaten, PolarMessung};
use crate::daten::projekt::Projekt;
use crate::daten::punktlisten::QUELLE_POLAR;
use crate::grundlagen::format::{f, parse_zahl};
use crate::grundlagen::{fehler_log, protokoll};
use crate::protokolle::polar::{self as prot_polar, PolarAnschluss, PolarDaten, PolarPunkt, STATUS_ERSETZT, STATUS_NEU};
use crate::protokolle::{ausgabeverzeichnis, letztes_protokoll, oeffne, schreibe_fuer_projekt};
use crate::rechnen::freie_stationierung::{self, Optionen};
use crate::rechnen::neupunkt;
use crate::rechnen::polar::{self, EXZ_INFO_GRENZE_M, FESTPUNKT_ABWEICHUNG_INFO_M, MIN_ANSCHLUSSPUNKTE_INFO, MassstabErgebnis, UEBERSCHREIBEN_RUECKFRAGE_M};
use crate::tachymeter::modell::TachymeterModell;

/// Standardstandpunkt für Messungen ohne Standpunktangabe.
const STANDPUNKT_STANDARD: &str = "SP01";
/// Platzhalter für nicht vorhandene Werte.
const STRICH: &str = "–";
/// Prozent.
const PROZENT: f64 = 100.0;
/// Anzahl der angezeigten stark abweichenden Punkte in der Rückfrage.
const MAX_RUECKFRAGE_PUNKTE: usize = 10;
/// Spaltenbreiten der Anschlusspunkttabelle: Nr, s, vHz, vQuer, vLängs, vHöhe, Gewicht, Hz, Str, Höhe.
const BREITEN_ANSCHLUSS: [f32; 10] = [90.0, 80.0, 70.0, 70.0, 70.0, 70.0, 70.0, 44.0, 44.0, 52.0];
/// Spaltenbreiten der Polartabelle: Berechnen, Nr, Hz, V, Strecke, ZH, eL, eQ, R, H, Höhe, Code, Zeit.
const BREITEN_POLAR: [f32; 13] = [60.0, 70.0, 80.0, 80.0, 80.0, 60.0, 60.0, 60.0, 90.0, 90.0, 70.0, 80.0, 130.0];
/// Breite der Zeilenköpfe.
const ZEILENKOPF_BREITE: f32 = 30.0;
/// Farben.
const FARBE_GRAU: Color32 = Color32::from_rgb(211, 211, 211);
const FARBE_EXZ_GROSS: Color32 = Color32::from_rgb(255, 230, 100);
const FARBE_FEHLER: Color32 = Color32::from_rgb(170, 30, 30);
const FARBE_GRUEN: Color32 = Color32::from_rgb(180, 230, 160);
const FARBE_GELB: Color32 = Color32::from_rgb(255, 230, 100);
const FARBE_ROT: Color32 = Color32::from_rgb(255, 110, 100);
/// Ampel: grün bis Grenze − 1, gelb bis Grenze + 5.
const AMPEL_GRUEN_ABZUG: f64 = 1.0;
const AMPEL_GELB_ZUSCHLAG: f64 = 5.0;

/// Offene Rückfrage.
enum Frage {
    Ueberschreiben(String),
    Kor(PathBuf),
}

/// Formular der Polarpunkte.
pub struct PolarpunkteAnsicht {
    daten: Option<PolarMessdaten>,
    standpunkte: Vec<String>,
    sp_index: usize,
    instr_hoehe: String,
    quelle_station: bool,
    ap: Vec<StationierungsPunkt>,
    akt: Vec<[bool; 3]>,
    np: Vec<PolarMessung>,
    np_aktiv: Vec<bool>,
    exz_text: Vec<(String, String)>,
    exz_original: Vec<(f64, f64)>,
    ergebnisse: Vec<Option<NeupunktErgebnis>>,
    residuen: Vec<Option<PunktResiduum>>,
    station: Option<StationierungsErgebnis>,
    massstab: MassstabErgebnis,
    lokal: bool,
    lade_hinweise: Vec<String>,
    hinweise: Vec<String>,
    station_fehler: String,
    fehler: String,
    ergebnis_text: String,
    status: String,
    frage: Option<Frage>,
}

/// Eigene Fläche für jede Tabelle; breite Inhalte bleiben im Scrollbereich.
fn tabellen_bereich<R>(ui: &mut egui::Ui, titel: &str, hintergrund: Color32, akzent: Color32, inhalt: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::new()
        .fill(hintergrund)
        .stroke(egui::Stroke::new(1.0, akzent))
        .corner_radius(5.0)
        .inner_margin(8.0)
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.label(RichText::new(titel).strong().color(akzent));
            ui.add_space(4.0);
            inhalt(ui)
        }).inner
}

fn ist_gruen(betrag: f64, grenze: f64) -> bool {
    betrag <= grenze - AMPEL_GRUEN_ABZUG
}

fn ampel(betrag: f64, grenze: f64) -> Color32 {
    if ist_gruen(betrag, grenze) {
        FARBE_GRUEN
    } else if betrag <= grenze + AMPEL_GELB_ZUSCHLAG {
        FARBE_GELB
    } else {
        FARBE_ROT
    }
}

fn mit_vorzeichen(wert: f64) -> String {
    if wert == 0.0 {
        "0.0".to_string()
    } else if wert < 0.0 {
        f(wert, 1)
    } else {
        format!("+{}", f(wert, 1))
    }
}

fn lokale_station() -> StationierungsErgebnis {
    StationierungsErgebnis {
        massstab: 1.0,
        konvergiert: true,
        berechnung_3d: true,
        ist_lokales_system: true,
        ..StationierungsErgebnis::default()
    }
}

fn zeitstempel(text: &str) -> NaiveDateTime {
    ["%Y-%m-%d %H:%M:%S", "%Y-%m-%d %H:%M"]
        .iter()
        .find_map(|format| NaiveDateTime::parse_from_str(text, format).ok())
        .unwrap_or_else(jetzt)
}

impl PolarpunkteAnsicht {
    pub fn neu(projekt: &Projekt) -> Self {
        let mut a = Self {
            daten: None,
            standpunkte: Vec::new(),
            sp_index: 0,
            instr_hoehe: String::new(),
            quelle_station: false,
            ap: Vec::new(),
            akt: Vec::new(),
            np: Vec::new(),
            np_aktiv: Vec::new(),
            exz_text: Vec::new(),
            exz_original: Vec::new(),
            ergebnisse: Vec::new(),
            residuen: Vec::new(),
            station: None,
            massstab: MassstabErgebnis { wert: 1.0, hinweis: String::new() },
            lokal: false,
            lade_hinweise: Vec::new(),
            hinweise: Vec::new(),
            station_fehler: String::new(),
            fehler: String::new(),
            ergebnis_text: String::new(),
            status: String::new(),
            frage: None,
        };
        a.lade_datei(projekt);
        a
    }

    fn sp_name(&self) -> String {
        self.standpunkte.get(self.sp_index).cloned().unwrap_or_default()
    }

    fn zeige_leer(&mut self, meldung: &str) {
        self.fehler.clear();
        self.ergebnis_text.clear();
        self.status = meldung.to_string();
        self.hinweise.clear();
        self.station = None;
    }

    // ── Laden ─────────────────────────────────────────────────────────────────

    fn lade_datei(&mut self, projekt: &Projekt) {
        self.daten = None;
        self.standpunkte.clear();
        self.station = None;
        self.ap.clear();
        self.np.clear();
        self.ergebnisse.clear();
        if !projekt.einstellungen.ist_geladen() {
            self.zeige_leer("Kein Projekt geladen.");
            return;
        }
        let pfad = messdaten::messdatei_pfad(&projekt.einstellungen);
        if !pfad.exists() {
            let name = pfad.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            self.zeige_leer(&format!("Keine Messdaten-Datei vorhanden ({name})."));
            return;
        }
        let mut daten = match polar_messdaten::lese(&pfad, |nr| projekt.finde_anschlusspunkt(nr).is_some()) {
            Ok(d) => d,
            Err(fehler) => {
                fehler_log::log("Polarpunkte.lade_datei", &fehler.to_string());
                self.zeige_leer(&format!("Messdaten konnten nicht gelesen werden: {fehler}"));
                return;
            }
        };
        for m in daten.messungen.iter_mut().filter(|m| m.standpunkt_nr.is_empty()) {
            m.standpunkt_nr = STANDPUNKT_STANDARD.to_string();
        }
        self.standpunkte = daten.standpunkte();
        self.daten = Some(daten);
        let aktuell = &projekt.station.standpunkt_nr;
        match self.standpunkte.iter().position(|s| s == aktuell).or_else(|| self.standpunkte.len().checked_sub(1)) {
            Some(i) => {
                self.sp_index = i;
                self.standpunkt_gewaehlt(projekt);
            }
            None => self.zeige_leer("Die Messdaten-Datei enthält keine Messungen."),
        }
    }

    fn standpunkt_gewaehlt(&mut self, projekt: &Projekt) {
        let sp = self.sp_name();
        let Some(daten) = &self.daten else {
            return;
        };
        if sp.is_empty() {
            return;
        }
        let ih = if &projekt.station.standpunkt_nr == &sp && projekt.station.instrumenten_hoehe > 0.0 {
            projekt.station.instrumenten_hoehe
        } else if daten.kopf_instr_hoehe > 0.0 {
            daten.kopf_instr_hoehe
        } else {
            projekt.einstellungen.instrumenten_hoehe
        };
        self.instr_hoehe = f(ih, 3);
        self.quelle_station = projekt.lese_station_datei(&sp).is_some_and(|d| !d.messungen.is_empty());
        self.fuelle_tabellen(projekt);
    }

    fn fuelle_tabellen(&mut self, projekt: &Projekt) {
        let sp = self.sp_name();
        self.lade_hinweise.clear();
        self.station_fehler.clear();
        self.ap.clear();
        self.np.clear();
        if let Some(daten) = self.daten.clone().filter(|_| !sp.is_empty()) {
            if self.quelle_station {
                self.lade_anschluss_aus_station(projekt, &sp);
            } else {
                self.lade_anschluss_aus_dat(projekt, &daten, &sp);
            }
            self.akt = self.ap.iter().map(|p| [true, true, p.hoehe != 0.0]).collect();
            self.np = daten.messungen.iter().filter(|m| !m.ist_anschlusspunkt() && m.standpunkt_nr == sp).cloned().collect();
            self.exz_original = self.np.iter().map(|m| (m.exz_laengs_m, m.exz_quer_m)).collect();
            self.exz_text = self.np.iter().map(|m| (f(m.exz_laengs_m, 3), f(m.exz_quer_m, 3))).collect();
            self.np_aktiv = vec![true; self.np.len()];
        }
        self.neuberechnen(projekt);
    }

    fn lade_anschluss_aus_station(&mut self, projekt: &Projekt, sp: &str) {
        match projekt.lese_station_datei(sp) {
            Some(datei) => self.ap = datei.messungen,
            None => self.station_fehler = format!("Keine gespeicherte Stationierung für {sp}. Quelle „neu aus dat.csv“ wählen."),
        }
    }

    fn lade_anschluss_aus_dat(&mut self, projekt: &Projekt, daten: &PolarMessdaten, sp: &str) {
        for m in daten.messungen.iter().filter(|m| m.ist_anschlusspunkt() && m.standpunkt_nr == sp) {
            match projekt.finde_anschlusspunkt(&m.punkt_nr) {
                None => self.lade_hinweise.push(format!("Anschlusspunkt {}: keine Koordinaten gefunden – nicht verwendet.", m.punkt_nr)),
                Some((r, h, hoehe)) => self.ap.push(StationierungsPunkt {
                    punkt_nr: m.punkt_nr.clone(),
                    r,
                    h,
                    hoehe,
                    hz: m.hz_gon,
                    v: m.v_gon,
                    strecke: m.strecke_m,
                    zielhoehe: m.zielhoehe_m,
                }),
            }
        }
    }

    // ── Berechnung ────────────────────────────────────────────────────────────

    fn lese_instr_hoehe(&self) -> f64 {
        parse_zahl(&self.instr_hoehe).unwrap_or(0.0)
    }

    fn neuberechnen(&mut self, projekt: &Projekt) {
        let par = projekt.rechenparameter.params.clone();
        let sp = self.sp_name();
        let ih = self.lese_instr_hoehe();
        let n = self.ap.len();
        let mut hinweise = self.lade_hinweise.clone();
        let akt_hz: Vec<bool> = self.akt.iter().map(|a| a[0]).collect();
        let akt_str: Vec<bool> = self.akt.iter().map(|a| a[1]).collect();
        let akt_hoe: Vec<bool> = self.akt.iter().map(|a| a[2]).collect();

        self.station = None;
        self.lokal = false;
        let mut fehler = String::new();
        let mut je_zeile: Vec<Option<PunktResiduum>> = vec![None; n];

        let verschiedene = {
            let mut namen: Vec<String> = self.ap.iter().map(|p| p.punkt_nr.to_lowercase()).collect();
            namen.sort();
            namen.dedup();
            namen.len()
        };
        if sp.is_empty() || self.daten.is_none() {
        } else if n == 0 && !self.station_fehler.is_empty() {
            fehler.clone_from(&self.station_fehler);
        } else if n == 0 {
            self.station = Some(lokale_station());
            self.lokal = true;
        } else if verschiedene == 1 {
            match self.berechne_ein_ziel(projekt, &sp, ih, (&akt_hz, &akt_str, &akt_hoe), &mut je_zeile) {
                Ok(station) => self.station = Some(station),
                Err(text) => fehler = text,
            }
        } else {
            let optionen = Optionen {
                freier_massstab: par.freier_massstab,
                aktiv_richtung: Some(akt_hz.clone()),
                aktiv_strecke: Some(akt_str.clone()),
                aktiv_hoehe: Some(akt_hoe.clone()),
                berechnung_3d: par.berechnung_3d,
                fehlergrenze_mm_hoehe: par.fehlergrenze_mm_hoehe,
                zentrierfehler_ziel_mm: if par.gewichtung_nahe_ziele { par.zentrierfehler_ziel_mm } else { 0.0 },
                zentrierfehler_standpunkt_mm: if par.gewichtung_nahe_ziele { par.zentrierfehler_standpunkt_mm } else { 0.0 },
                ..Optionen::default()
            };
            match freie_stationierung::berechnen(&self.ap, ih, &optionen).and_then(|e| neupunkt::pruefe_station(&e).map(|()| e)) {
                Ok(erg) => {
                    for (i, r) in erg.residuen.iter().enumerate().take(n) {
                        je_zeile[i] = Some(r.clone());
                    }
                    self.station = Some(erg);
                }
                Err(text) => fehler = text,
            }
        }

        self.residuen = je_zeile;
        if let Some(h) = self.station.as_ref().map(|s| s.warnung_hoehe.clone()).filter(|h| !h.is_empty()) {
            hinweise.push(h);
        }
        let anschluss = {
            let mut namen: Vec<String> = (0..n).filter(|i| akt_hz[*i] || akt_str[*i]).map(|i| self.ap[i].punkt_nr.to_lowercase()).collect();
            namen.sort();
            namen.dedup();
            namen.len()
        };
        self.massstab = polar::ermittle_massstab(&par, self.station.as_ref());

        if let Some(station) = self.station.clone().filter(|_| !self.lokal) {
            if anschluss < MIN_ANSCHLUSSPUNKTE_INFO {
                hinweise.push(format!(
                    "Nur {anschluss} Anschlusspunkt(e): Orientierung nicht überbestimmt, Qualität kann nicht beurteilt werden \
                     (empfohlen: mindestens {MIN_ANSCHLUSSPUNKTE_INFO})."
                ));
            }
            let nicht_gruen = self.zaehle_nicht_gruen(par.fehlergrenze_mm_strecke);
            if nicht_gruen > 0 {
                hinweise.push(format!("{nicht_gruen} Abweichung(en) (Quer/Längs) nicht im grünen Bereich."));
            }
            if par.berechnung_3d && station.hoehe == 0.0 {
                hinweise.push("Station ohne Höhe – Polarpunkte nur in der Lage (2D).".to_string());
            }
        }
        if !self.massstab.hinweis.is_empty() {
            hinweise.push(self.massstab.hinweis.clone());
        }

        let berechnet = self.berechne_polarpunkte(projekt, ih, &mut hinweise);
        self.hinweise = hinweise;
        self.zeige_ergebnis(projekt, &fehler, anschluss, berechnet);
    }

    /// Ein Anschlussziel bei bekanntem Standpunkt: Orientierung aus dieser einen Richtung.
    fn berechne_ein_ziel(
        &self,
        projekt: &Projekt,
        sp: &str,
        ih: f64,
        (akt_hz, akt_str, akt_hoe): (&[bool], &[bool], &[bool]),
        je_zeile: &mut [Option<PunktResiduum>],
    ) -> Result<StationierungsErgebnis, String> {
        let (r, h, hoehe) = projekt
            .finde_anschlusspunkt(sp)
            .ok_or_else(|| format!("Nur ein Anschlussziel: Der Standpunkt {sp} hat keine bekannten Koordinaten."))?;
        let idx = akt_hz.iter().position(|a| *a).ok_or("Keine aktive Richtung zum Anschlussziel.")?;
        let erg = polar::orientierung_aus_einem_ziel(r, h, hoehe, &self.ap[idx], ih, akt_hz[idx], akt_str[idx], akt_hoe[idx], projekt.rechenparameter.params.berechnung_3d)
            .ok_or("Orientierung aus dem Anschlussziel nicht berechenbar (Standpunkt und Ziel identisch?).")?;
        je_zeile[idx] = erg.residuen.first().cloned();
        Ok(erg)
    }

    fn zaehle_nicht_gruen(&self, grenze: f64) -> usize {
        self.residuen
            .iter()
            .flatten()
            .map(|r| {
                usize::from(r.richtung_aktiv && !r.v_quer_mm.is_nan() && !ist_gruen(r.v_quer_mm.abs(), grenze))
                    + usize::from(r.strecke_aktiv && !r.v_strecke_mm.is_nan() && !ist_gruen(r.v_strecke_mm.abs(), grenze))
            })
            .sum()
    }

    fn baue_rohdaten(&self, m: &PolarMessung) -> NeupunktRohdaten {
        NeupunktRohdaten {
            punkt_nr: m.punkt_nr.clone(),
            code: m.code.clone(),
            hz_gon: m.hz_gon,
            v_gon: m.v_gon,
            strecke_m: m.strecke_m,
            zielhoehe_m: m.zielhoehe_m,
            zeitstempel: zeitstempel(&m.zeitstempel),
            standpunkt_nr: self.sp_name(),
            exz_laengs_m: m.exz_laengs_m,
            exz_quer_m: m.exz_quer_m,
        }
    }

    fn berechne_polarpunkte(&mut self, projekt: &Projekt, ih: f64, hinweise: &mut Vec<String>) -> usize {
        let params = projekt.rechenparameter.params.clone();
        let mut ergebnisse: Vec<Option<NeupunktErgebnis>> = Vec::with_capacity(self.np.len());
        let (mut berechnet, mut grosse_exz, mut fest_abweichung) = (0, Vec::new(), Vec::new());
        for (i, m) in self.np.iter().enumerate() {
            let aktiv = self.np_aktiv[i];
            let gross = m.exz_laengs_m.abs() > EXZ_INFO_GRENZE_M || m.exz_quer_m.abs() > EXZ_INFO_GRENZE_M;
            if aktiv && gross {
                grosse_exz.push(m.punkt_nr.clone());
            }
            let ergebnis = match (&self.station, aktiv) {
                (Some(station), true) => neupunkt::berechnen(&self.baue_rohdaten(m), station, &params, Some(self.massstab.wert), ih),
                _ => None,
            };
            if let Some(e) = &ergebnis {
                berechnet += 1;
                if let Some(fest) = projekt.feldbuchpunkte.punkte().iter().find(|p| p.typ == TYP_NEUPUNKT && p.punkt_nr == e.punkt_nr) {
                    let abw = polar::abstand(e.r - fest.r, e.h - fest.h);
                    if abw > FESTPUNKT_ABWEICHUNG_INFO_M {
                        fest_abweichung.push(format!("{} ({} m)", e.punkt_nr, f(abw, 3)));
                    }
                }
            }
            ergebnisse.push(ergebnis);
        }
        self.ergebnisse = ergebnisse;
        if !grosse_exz.is_empty() {
            hinweise.push(format!("Exzentrizität größer als {} m bei Punkt(en): {}", f(EXZ_INFO_GRENZE_M, 0), grosse_exz.join(", ")));
        }
        if !fest_abweichung.is_empty() {
            hinweise.push(format!("Abweichung zum Festpunkt (> {} m): {}", f(FESTPUNKT_ABWEICHUNG_INFO_M, 2), fest_abweichung.join(", ")));
        }
        berechnet
    }

    fn zeige_ergebnis(&mut self, projekt: &Projekt, fehler: &str, anschluss: usize, berechnet: usize) {
        let par = &projekt.rechenparameter.params;
        self.fehler = fehler.to_string();
        self.ergebnis_text.clear();
        if !fehler.is_empty() {
            self.status = "Keine Orientierung berechenbar.".to_string();
            return;
        }
        let Some(station) = &self.station else {
            self.status.clear();
            return;
        };
        let gewichtung = if par.gewichtung_nahe_ziele {
            format!("ein (Zielzentrierung {} mm)", f(par.zentrierfehler_ziel_mm, 1))
        } else {
            "aus".to_string()
        };
        if self.lokal {
            self.ergebnis_text = "Lokales System: Standpunkt 0/0/0, Orientierung 0 gon – keine Anschlusspunkte erforderlich.".to_string();
        } else {
            let hoehe = if par.berechnung_3d && station.hoehe != 0.0 { f(station.hoehe, 3) } else { STRICH.to_string() };
            let s0 = if station.redundanz > 0 { par.sigma_text(station.s0_mm, 2) } else { "s₀ = –".to_string() };
            self.ergebnis_text = format!(
                "R {}  H {}  Höhe {hoehe}  Orientierung {} gon  m {}  {s0}  r = {}\nGewichtung nahe Ziele: {gewichtung}",
                f(station.r, 3),
                f(station.h, 3),
                f(station.orientierung_gon, 4),
                f(self.massstab.wert, 6),
                station.redundanz
            );
        }
        let drei_d = par.berechnung_3d && (self.lokal || station.hoehe != 0.0);
        let herkunft = if self.lokal { "lokales System".to_string() } else { format!("{anschluss} Anschlusspunkt(e)") };
        self.status = format!(
            "{} · m = {} · {herkunft} · {berechnet} Polarpunkt(e) berechnet",
            if drei_d { "3D" } else { "2D" },
            f(self.massstab.wert, 6)
        );
    }

    // ── Speichern ─────────────────────────────────────────────────────────────

    fn speichern(&mut self, projekt: &mut Projekt) {
        self.neuberechnen(projekt);
        let anzahl = self.ergebnisse.iter().flatten().count();
        if self.station.is_none() || anzahl == 0 {
            self.status = "Es sind keine berechneten Polarpunkte zum Speichern vorhanden.".to_string();
            return;
        }
        let sp = self.sp_name();
        let abweichend: Vec<String> = self
            .ergebnisse
            .iter()
            .flatten()
            .filter_map(|e| {
                let alt = projekt.neupunkte.finde_koordinate(&e.punkt_nr, &sp)?;
                let abw = polar::abstand(e.r - alt.r, e.h - alt.h);
                (abw > UEBERSCHREIBEN_RUECKFRAGE_M).then(|| format!("{}: {} m", e.punkt_nr, f(abw, 3)))
            })
            .collect();
        if abweichend.is_empty() {
            self.speichere_jetzt(projekt);
            return;
        }
        let mut text = format!(
            "{} vorhandene(r) Punkt(e) von {sp} weichen um mehr als {} m vom neuen Ergebnis ab:\n\n{}",
            abweichend.len(),
            f(UEBERSCHREIBEN_RUECKFRAGE_M, 2),
            abweichend.iter().take(MAX_RUECKFRAGE_PUNKTE).cloned().collect::<Vec<_>>().join("\n")
        );
        if abweichend.len() > MAX_RUECKFRAGE_PUNKTE {
            text.push_str("\n…");
        }
        text.push_str("\n\nTrotzdem überschreiben?");
        self.frage = Some(Frage::Ueberschreiben(text));
    }

    fn speichere_jetzt(&mut self, projekt: &mut Projekt) {
        let sp = self.sp_name();
        let ih = self.lese_instr_hoehe();
        let anzahl = self.ergebnisse.iter().flatten().count();
        let (mut neu, mut ersetzt) = (0, 0);
        let mut protokoll_punkte: Vec<PolarPunkt> = Vec::new();
        for i in 0..self.np.len() {
            let Some(mut erg) = self.ergebnisse[i].clone() else {
                continue;
            };
            erg.standpunkt_nr.clone_from(&sp);
            erg.quelle = QUELLE_POLAR.to_string();
            let alt = projekt.neupunkte.finde_koordinate(&erg.punkt_nr, &sp).map(|a| (a.r, a.h));
            if alt.is_none() {
                neu += 1;
            } else {
                ersetzt += 1;
            }
            let m = &self.np[i];
            protokoll_punkte.push(PolarPunkt {
                punkt_nr: erg.punkt_nr.clone(),
                hz: m.hz_gon,
                v: m.v_gon,
                strecke: m.strecke_m,
                zielhoehe: m.zielhoehe_m,
                exz_laengs: erg.exz_laengs_m,
                exz_quer: erg.exz_quer_m,
                r: erg.r,
                h: erg.h,
                hoehe: erg.ist_3d.then_some(erg.hoehe),
                status: if alt.is_none() { STATUS_NEU } else { STATUS_ERSETZT }.to_string(),
                d_r: alt.map_or(f64::NAN, |a| erg.r - a.0),
                d_h: alt.map_or(f64::NAN, |a| erg.h - a.1),
            });
            let roh = self.baue_rohdaten(&self.np[i]);
            if let Err(fehler) = projekt.neupunkte.hinzufuegen_oder_ersetzen(roh, erg) {
                self.status = format!("Speichern fehlgeschlagen: {fehler}");
                return;
            }
        }
        self.schreibe_geaenderte_exzentrizitaeten(projekt);
        if let Some(station) = &self.station {
            let par = &projekt.rechenparameter.params;
            protokoll::log(
                "POLAR",
                &format!(
                    "SP={sp}  IH={} m  Orientierung={} gon  m={}  r={}  Punkte={anzahl}  Gewichtung nahe Ziele: {}",
                    f(ih, 3),
                    f(station.orientierung_gon, 4),
                    f(self.massstab.wert, 6),
                    station.redundanz,
                    if par.gewichtung_nahe_ziele { "ein" } else { "aus" }
                ),
            );
        }
        for h in &self.hinweise {
            protokoll::log("POLAR", &format!("Hinweis: {h}"));
        }
        for e in self.ergebnisse.iter().flatten() {
            let hoehe = if e.ist_3d { format!(" Höhe={}", f(e.hoehe, 3)) } else { String::new() };
            protokoll::log("POLAR", &format!("Punkt {}: R={} H={}{hoehe}  eL={} eQ={}", e.punkt_nr, f(e.r, 3), f(e.h, 3), f(e.exz_laengs_m, 3), f(e.exz_quer_m, 3)));
        }
        let protokoll_text = match self.schreibe_protokoll(projekt, protokoll_punkte) {
            Ok(Some(pfad)) => format!(", Protokoll {}", pfad.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()),
            Ok(None) => String::new(),
            Err(fehler) => format!(" (Protokoll fehlgeschlagen: {fehler})"),
        };
        self.status = format!("{anzahl} Polarpunkt(e) gespeichert ({neu} neu, {ersetzt} ersetzt) – Neupunkte.json{protokoll_text}.");
    }

    /// Schreibt das Protokoll der Berechnung; `None`, wenn die Protokollierung ausgeschaltet ist.
    fn schreibe_protokoll(&self, projekt: &Projekt, punkte: Vec<PolarPunkt>) -> Result<Option<PathBuf>, String> {
        if !projekt.einstellungen.protokoll_aktiv {
            return Ok(None);
        }
        let Some(station) = &self.station else {
            return Ok(None);
        };
        let par = &projekt.rechenparameter.params;
        let anschluss = self
            .ap
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let r = self.residuen.get(i).and_then(Option::as_ref);
                let akt = self.akt.get(i).copied().unwrap_or([true; 3]);
                PolarAnschluss {
                    punkt_nr: p.punkt_nr.clone(),
                    inaktiv: !akt[0] || !akt[1] || !akt[2],
                    strecke_h: r.map_or(0.0, |r| r.strecke_h),
                    v_winkel_cc: r.filter(|r| r.richtung_aktiv).map_or(f64::NAN, |r| r.v_winkel_cc),
                    v_quer_mm: r.filter(|r| r.richtung_aktiv).map_or(f64::NAN, |r| r.v_quer_mm),
                    v_laengs_mm: r.filter(|r| r.strecke_aktiv).map_or(f64::NAN, |r| r.v_strecke_mm),
                    v_hoehe_mm: r.filter(|r| r.hoehe_aktiv).map_or(f64::NAN, |r| r.v_hoehe_mm),
                    gewicht_richtung: r.map_or(f64::NAN, |r| r.gewicht_richtung),
                }
            })
            .collect();
        let s0_info = if self.lokal {
            "Lokales System: Standpunkt 0/0/0, Orientierung 0 gon, keine Anschlusspunkte".to_string()
        } else if station.redundanz > 0 {
            let bez = if par.sigma_faktor_wirksam() == 1 { "s0".to_string() } else { format!("{}*s0", par.sigma_faktor_wirksam()) };
            format!("Standardabw. {bez}:  {} mm     Redundanz r:  {}", f(par.sigma_wert(station.s0_mm), 2), station.redundanz)
        } else {
            "Standardabw. s0:  -  (Redundanz r = 0, eindeutig bestimmt)".to_string()
        };
        let gewichtung = if par.gewichtung_nahe_ziele {
            format!("ein (Zielzentrierung {} mm, Standpunkt {} mm)", f(par.zentrierfehler_ziel_mm, 1), f(par.zentrierfehler_standpunkt_mm, 1))
        } else {
            "aus".to_string()
        };
        let quelle = if self.lokal {
            "lokales System"
        } else if self.quelle_station {
            "gespeicherter Stationierung"
        } else {
            "Anschlusspunkten der dat.csv"
        };
        let drei_d = par.berechnung_3d && (self.lokal || station.hoehe != 0.0);
        let daten = PolarDaten {
            standpunkt: self.sp_name(),
            instr_hoehe: self.lese_instr_hoehe(),
            quelle: quelle.to_string(),
            r: station.r,
            h: station.h,
            hoehe: drei_d.then_some(station.hoehe),
            orientierung_gon: station.orientierung_gon,
            massstab: self.massstab.wert,
            s0_info,
            gewichtung,
            hinweise: self.hinweise.clone(),
            anschluss,
            punkte,
        };
        let p = prot_polar::protokoll(&daten, &projekt.projektdaten.bearbeiter, jetzt());
        schreibe_fuer_projekt(&p, projekt).map(Some)
    }

    /// Öffnet das zuletzt erzeugte Polarpunkte-Protokoll.
    fn oeffne_letztes_protokoll(&mut self, projekt: &Projekt) {
        match letztes_protokoll(&ausgabeverzeichnis(projekt), "Polarpunkte") {
            None => {
                self.status = "Kein Polarpunkte-Protokoll gefunden. Es wird beim Speichern erzeugt (sofern die Protokollierung eingeschaltet ist).".to_string();
            }
            Some(pfad) => {
                if let Err(fehler) = oeffne(&pfad) {
                    self.status = format!("{fehler}\nDatei: {}", pfad.display());
                }
            }
        }
    }

    fn schreibe_geaenderte_exzentrizitaeten(&mut self, projekt: &Projekt) {
        let Some(daten) = &self.daten else {
            return;
        };
        let _ = daten;
        let aenderungen: Vec<(usize, f64, f64)> = self
            .np
            .iter()
            .zip(&self.exz_original)
            .filter(|(m, (l, q))| (l - m.exz_laengs_m).abs() > 1e-9 || (q - m.exz_quer_m).abs() > 1e-9)
            .map(|(m, _)| (m.zeilen_index, m.exz_laengs_m, m.exz_quer_m))
            .collect();
        if aenderungen.is_empty() {
            return;
        }
        let pfad = messdaten::messdatei_pfad(&projekt.einstellungen);
        match messdaten::schreibe_exzentrizitaeten(&pfad, &aenderungen) {
            Ok(()) => self.exz_original = self.np.iter().map(|m| (m.exz_laengs_m, m.exz_quer_m)).collect(),
            Err(fehler) => {
                fehler_log::log("Polarpunkte.schreibe_exzentrizitaeten", &fehler.to_string());
                self.status = format!("Die geänderten Exzentrizitäten konnten nicht in die dat.csv geschrieben werden: {fehler}");
            }
        }
    }

    // ── KOR-CSV ───────────────────────────────────────────────────────────────

    fn kor_csv(&mut self, projekt: &Projekt) {
        self.neuberechnen(projekt);
        if self.ergebnisse.iter().flatten().count() == 0 {
            self.status = "Es sind keine berechneten Polarpunkte vorhanden.".to_string();
            return;
        }
        let mut dialog = rfd::FileDialog::new()
            .set_title("Polarpunkte als KOR-CSV speichern")
            .add_filter("KOR-CSV (*-kor.csv)", &["csv"])
            .set_file_name(format!("{}-kor.csv", projekt.einstellungen.projekt_name));
        if projekt.einstellungen.ist_geladen() {
            dialog = dialog.set_directory(&projekt.einstellungen.projekt_verzeichnis);
        }
        let Some(pfad) = dialog.save_file() else {
            return;
        };
        if pfad.exists() {
            self.frage = Some(Frage::Kor(pfad));
        } else {
            self.schreibe_kor(projekt, &pfad, false);
        }
    }

    fn schreibe_kor(&mut self, projekt: &Projekt, pfad: &Path, erhalten: bool) {
        let punkte: Vec<KorPunkt> = self
            .ergebnisse
            .iter()
            .flatten()
            .map(|e| KorPunkt { punkt_nr: e.punkt_nr.clone(), r: e.r, h: e.h, hoehe: e.ist_3d.then_some(e.hoehe), code: e.code.clone() })
            .collect();
        let sensor = if projekt.einstellungen.tachymeter_modell == TachymeterModell::Manuell { "" } else { projekt.einstellungen.tachymeter_modell.name() };
        let name = pfad.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        match kor_csv::schreibe(pfad, &projekt.einstellungen.projekt_name, sensor, &punkte, erhalten) {
            Ok(gesamt) => {
                protokoll::log("POLAR", &format!("KOR-CSV: {} Punkt(e) nach {name} ({gesamt} gesamt)", punkte.len()));
                self.status = format!("{} Punkt(e) in {name} gespeichert ({gesamt} gesamt).", punkte.len());
            }
            Err(fehler) => {
                fehler_log::log("Polarpunkte.kor_csv", &fehler.to_string());
                self.status = format!("Die Datei konnte nicht geschrieben werden: {fehler}");
            }
        }
    }

    // ── Darstellung ───────────────────────────────────────────────────────────

    pub fn zeichne(&mut self, ui: &mut egui::Ui, projekt: &mut Projekt) {
        ui.heading("Polarpunkte");
        let mut neu_rechnen = false;
        let mut standpunkt_neu = false;
        ui.horizontal_wrapped(|ui| {
            ui.label("Standpunkt:");
            let vorher = self.sp_index;
            egui::ComboBox::from_id_salt("polar_standpunkt").selected_text(self.sp_name()).show_ui(ui, |ui| {
                for (i, sp) in self.standpunkte.iter().enumerate() {
                    ui.selectable_value(&mut self.sp_index, i, sp);
                }
            });
            standpunkt_neu = vorher != self.sp_index;
            ui.label("IH [m]:");
            let antwort = ui.add(egui::TextEdit::singleline(&mut self.instr_hoehe).desired_width(60.0));
            if antwort.lost_focus() {
                neu_rechnen = true;
            }
            ui.label("Anschluss aus:");
            let station = ui.radio_value(&mut self.quelle_station, true, "gespeicherter Stationierung").changed();
            let dat = ui.radio_value(&mut self.quelle_station, false, "neu aus dat.csv").changed();
            if station || dat {
                self.fuelle_tabellen(projekt);
            }
        });
        ui.horizontal_wrapped(|ui| {
            if ui.button("Neu laden").clicked() {
                self.lade_datei(projekt);
            }
            if ui.button("Neu berechnen").clicked() {
                neu_rechnen = true;
            }
            if ui.button(RichText::new("Speichern").strong()).clicked() {
                self.speichern(projekt);
            }
            if ui.button("Als KOR-CSV …").clicked() {
                self.kor_csv(projekt);
            }
            if ui.button("Protokoll öffnen").clicked() {
                self.oeffne_letztes_protokoll(projekt);
            }
        });
        if standpunkt_neu {
            self.standpunkt_gewaehlt(projekt);
        }

        if !self.hinweise.is_empty() {
            ui.add_space(4.0);
            for h in &self.hinweise {
                ui.colored_label(Color32::from_rgb(150, 90, 0), format!("⚠ {h}"));
            }
        }
        if !self.fehler.is_empty() {
            ui.colored_label(FARBE_FEHLER, &self.fehler);
        }
        if !self.ergebnis_text.is_empty() {
            ui.label(RichText::new(&self.ergebnis_text).monospace());
        }
        ui.add_space(6.0);

        let geaendert_ap = tabellen_bereich(ui, "Anschlusspunkte", Color32::from_rgb(232, 241, 252), Color32::from_rgb(65, 105, 155), |ui| self.zeichne_anschluss(ui, projekt));
        ui.add_space(10.0);
        let geaendert_np = tabellen_bereich(ui, "Polarpunkte", Color32::from_rgb(232, 246, 237), Color32::from_rgb(55, 120, 80), |ui| self.zeichne_polar(ui));
        if neu_rechnen || geaendert_ap || geaendert_np {
            self.neuberechnen(projekt);
        }
        ui.add_space(4.0);
        ui.label(&self.status);
        self.zeichne_frage(ui.ctx(), projekt);
    }

    fn zeichne_anschluss(&mut self, ui: &mut egui::Ui, projekt: &Projekt) -> bool {
        let par = &projekt.rechenparameter.params;
        let raster = Raster::neu(&BREITEN_ANSCHLUSS).mit_zeilenkopf(ZEILENKOPF_BREITE);
        egui::ScrollArea::horizontal().id_salt("polar_ap_horizontal").auto_shrink([false, true]).show(ui, |ui| {
        raster.kopf(ui, &["PunktNr", "s [m]", "vHz [cc]", "vQuer [mm]", "vLängs [mm]", "vHöhe [mm]", "Gewicht", "Hz", "Str", "Höhe"]);
        let mut geaendert = false;
        egui::ScrollArea::vertical().id_salt("polar_ap").max_height(180.0).show(ui, |ui| {
            for (i, p) in self.ap.iter().enumerate() {
                let r = self.residuen.get(i).and_then(Option::as_ref);
                let akt = &mut self.akt[i];
                raster.zeile(ui, i, &(i + 1).to_string(), |z| {
                    z.text(&p.punkt_nr);
                    z.zahl(&r.map_or_else(|| STRICH.to_string(), |r| f(r.strecke_h, 3)));
                    wert_zelle(z, r.filter(|r| r.richtung_aktiv).map(|r| r.v_winkel_cc), par.fehlergrenze_cc_winkel);
                    wert_zelle(z, r.filter(|r| r.richtung_aktiv).map(|r| r.v_quer_mm), par.fehlergrenze_mm_strecke);
                    wert_zelle(z, r.filter(|r| r.strecke_aktiv).map(|r| r.v_strecke_mm), par.fehlergrenze_mm_strecke);
                    wert_zelle(z, r.filter(|r| r.hoehe_aktiv).map(|r| r.v_hoehe_mm), par.fehlergrenze_mm_hoehe);
                    let gewicht = r.map(|r| r.gewicht_richtung).filter(|w| !w.is_nan());
                    z.zahl(&gewicht.map_or_else(|| STRICH.to_string(), |w| format!("{} %", f(w * PROZENT, 1))));
                    let vorher = *akt;
                    z.haken(&mut akt[0]);
                    z.haken(&mut akt[1]);
                    z.haken(&mut akt[2]);
                    geaendert |= vorher != *akt;
                });
            }
        });
        geaendert
        }).inner
    }

    fn zeichne_polar(&mut self, ui: &mut egui::Ui) -> bool {
        let raster = Raster::neu(&BREITEN_POLAR).mit_zeilenkopf(ZEILENKOPF_BREITE);
        egui::ScrollArea::horizontal().id_salt("polar_np_horizontal").auto_shrink([false, true]).show(ui, |ui| {
        raster.kopf(ui, &["Berechnen", "PunktNr", "Hz [gon]", "V [gon]", "Strecke [m]", "ZH [m]", "eL [m]", "eQ [m]", "R [m]", "H [m]", "Höhe [m]", "Code", "Zeit"]);
        let mut geaendert = false;
        egui::ScrollArea::vertical().id_salt("polar_np").max_height(260.0).show(ui, |ui| {
            for i in 0..self.np.len() {
                let ergebnis = self.ergebnisse.get(i).and_then(Option::as_ref).cloned();
                let hintergrund = ergebnis.is_none().then_some(FARBE_GRAU);
                let m = &mut self.np[i];
                let (exz_l, exz_q) = &mut self.exz_text[i];
                let aktiv = &mut self.np_aktiv[i];
                let (gross_l, gross_q) = (m.exz_laengs_m.abs() > EXZ_INFO_GRENZE_M, m.exz_quer_m.abs() > EXZ_INFO_GRENZE_M);
                raster.zeile_farbig(ui, i, &(i + 1).to_string(), hintergrund, |z| {
                    let vorher = *aktiv;
                    z.haken(aktiv);
                    geaendert |= vorher != *aktiv;
                    z.text(&m.punkt_nr);
                    z.zahl(&f(m.hz_gon, 4));
                    z.zahl(&f(m.v_gon, 4));
                    z.zahl(&f(m.strecke_m, 3));
                    z.zahl(&f(m.zielhoehe_m, 3));
                    if exz_zelle(z, exz_l, gross_l) {
                        if let Some(w) = parse_zahl(exz_l) {
                            m.exz_laengs_m = w;
                        }
                        geaendert = true;
                    }
                    if exz_zelle(z, exz_q, gross_q) {
                        if let Some(w) = parse_zahl(exz_q) {
                            m.exz_quer_m = w;
                        }
                        geaendert = true;
                    }
                    z.zahl(&ergebnis.as_ref().map_or_else(|| STRICH.to_string(), |e| f(e.r, 3)));
                    z.zahl(&ergebnis.as_ref().map_or_else(|| STRICH.to_string(), |e| f(e.h, 3)));
                    z.zahl(&ergebnis.as_ref().filter(|e| e.ist_3d).map_or_else(|| STRICH.to_string(), |e| f(e.hoehe, 3)));
                    z.text(&m.code);
                    z.text(&m.zeitstempel);
                });
            }
        });
        geaendert
        }).inner
    }

    fn zeichne_frage(&mut self, ctx: &egui::Context, projekt: &mut Projekt) {
        let Some(frage) = self.frage.take() else {
            return;
        };
        let mut antwort: Option<u8> = None;
        let titel = match &frage {
            Frage::Ueberschreiben(_) => "Polarpunkte überschreiben",
            Frage::Kor(_) => "Als KOR-CSV speichern",
        };
        egui::Window::new(titel).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO).show(ctx, |ui| match &frage {
            Frage::Ueberschreiben(text) => {
                ui.label(text);
                ui.horizontal(|ui| {
                    if ui.button("Ja").clicked() {
                        antwort = Some(0);
                    }
                    if ui.button("Nein").clicked() {
                        antwort = Some(2);
                    }
                });
            }
            Frage::Kor(_) => {
                ui.label("Die Datei ist bereits vorhanden.\n\nErgänzen: Punkte ergänzen bzw. ersetzen (andere Punkte bleiben erhalten)\nNeu anlegen: Datei überschreiben");
                ui.horizontal(|ui| {
                    if ui.button("Ergänzen").clicked() {
                        antwort = Some(0);
                    }
                    if ui.button("Neu anlegen").clicked() {
                        antwort = Some(1);
                    }
                    if ui.button("Abbrechen").clicked() {
                        antwort = Some(2);
                    }
                });
            }
        });
        match (antwort, frage) {
            (None, frage) => self.frage = Some(frage),
            (Some(0), Frage::Ueberschreiben(_)) => self.speichere_jetzt(projekt),
            (Some(2), Frage::Ueberschreiben(_)) => self.status = "Speichern abgebrochen – vorhandene Punkte unverändert.".to_string(),
            (Some(a), Frage::Kor(pfad)) if a < 2 => self.schreibe_kor(projekt, &pfad, a == 0),
            _ => {}
        }
    }
}

/// Wertezelle mit Ampelfarbe (oder grau bei fehlendem Wert).
fn wert_zelle(z: &mut Zellen<'_>, wert: Option<f64>, grenze: f64) {
    let (text, farbe) = match wert.filter(|w| !w.is_nan()) {
        Some(w) => (mit_vorzeichen(w), ampel(w.abs(), grenze)),
        None => (STRICH.to_string(), FARBE_GRAU),
    };
    z.zelle(|ui| {
        ui.painter().rect_filled(ui.max_rect().expand(1.0), 0.0, farbe);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(RichText::new(text).monospace().color(Color32::BLACK));
        });
    });
}

/// Exzentrizitätszelle; bei großem Wert gelb hinterlegt. Liefert `true`, wenn der Text geändert wurde.
fn exz_zelle(z: &mut Zellen<'_>, text: &mut String, gross: bool) -> bool {
    if gross {
        let mut geaendert = false;
        z.zelle(|ui| {
            ui.painter().rect_filled(ui.max_rect().expand(1.0), 0.0, FARBE_EXZ_GROSS);
            let breite = ui.available_width();
            geaendert = ui.add(egui::TextEdit::singleline(text).frame(egui::Frame::NONE).desired_width(breite)).changed();
        });
        geaendert
    } else {
        z.eingabe(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ampel_stufen() {
        assert_eq!(ampel(5.0, 10.0), FARBE_GRUEN);
        assert_eq!(ampel(9.5, 10.0), FARBE_GELB);
        assert_eq!(ampel(15.0, 10.0), FARBE_GELB);
        assert_eq!(ampel(15.1, 10.0), FARBE_ROT);
        assert!(ist_gruen(9.0, 10.0) && !ist_gruen(9.1, 10.0));
    }

    #[test]
    fn vorzeichenformat() {
        assert_eq!(mit_vorzeichen(0.0), "0.0");
        assert_eq!(mit_vorzeichen(1.25), "+1.3");
        assert_eq!(mit_vorzeichen(-0.46), "-0.5");
    }

    #[test]
    fn zeitstempel_formate() {
        assert_eq!(zeitstempel("2026-03-04 10:11").format("%H:%M:%S").to_string(), "10:11:00");
        assert_eq!(zeitstempel("2026-03-04 10:11:12").format("%S").to_string(), "12");
    }

    #[test]
    fn lokale_station_ist_verwendbar() {
        assert!(neupunkt::ist_verwendbar(Some(&lokale_station())));
    }
}
