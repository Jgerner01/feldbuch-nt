// Zweck:    Polarpunktberechnung: Neupunkte aus den Messungen der {Projekt}-dat.csv. Orientierung aus gespeicherter
//           Station oder neu aus den Anschlusspunkten (Ampel, einzelne Beobachtungen schaltbar, Gewichtung bei nahen
//           Zielen), Längs-/Querexzentrizität je Punkt, Maßstab nach Rechenparameter, Speichern nach Neupunkte.json
//           und als KOR-CSV
// Hardware: keine
// Autor:    Johann Gerner

use std::path::{Path, PathBuf};

use chrono::NaiveDateTime;
use eframe::egui::{self, Color32, RichText};

use super::rechenparameter_ansicht::RechenparameterAnsicht;
use super::tabelle::{Raster, Zellen};
use crate::daten::kor_csv::{self, KorPunkt};
use crate::daten::messdaten;
use crate::daten::modelle::{NeupunktErgebnis, NeupunktRohdaten, PunktResiduum, StationierungsErgebnis, StationierungsPunkt, TYP_NEUPUNKT, jetzt};
use crate::daten::polar_messdaten::{self, PolarMessdaten, PolarMessung};
use crate::daten::projekt::Projekt;
use crate::daten::punktlisten::QUELLE_POLAR;
use crate::daten::stationierung_csv::{self, StationierungsImport};
use crate::grundlagen::format::{f, parse_zahl};
use crate::grundlagen::{fehler_log, protokoll};
use crate::protokolle::polar::{self as prot_polar, PolarAnschluss, PolarDaten, PolarPunkt, STATUS_ERSETZT, STATUS_NEU};
use crate::protokolle::{ausgabeverzeichnis, letztes_protokoll, oeffne, schreibe_fuer_projekt};
use crate::rechnen::freie_stationierung::{self, Optionen};
use crate::rechnen::neupunkt;
use crate::rechnen::polar::{self, EXZ_INFO_GRENZE_M, FESTPUNKT_ABWEICHUNG_INFO_M, MIN_ANSCHLUSSPUNKTE_INFO, MassstabErgebnis, UEBERSCHREIBEN_RUECKFRAGE_M};
use crate::tachymeter::modell::TachymeterModell;

/// Gruppenname für Messungen ohne Standpunktangabe (keine erfundene Standpunktnummer).
const OHNE_STANDPUNKT: &str = "(ohne Standpunkt)";
/// Platzhalter für nicht vorhandene Werte.
const STRICH: &str = "–";
/// Prozent.
const PROZENT: f64 = 100.0;
/// Anzahl der angezeigten stark abweichenden Punkte in der Rückfrage.
const MAX_RUECKFRAGE_PUNKTE: usize = 10;
/// Füllgewichte der Anschlusspunkttabelle: Punkt, Str.h, v Winkel, v Quer, Hz, v Längs, Str, v Höhe, Hoe, Gewicht.
const GEWICHTE_ANSCHLUSS: [f32; 10] = [14.0, 11.0, 12.0, 11.0, 6.0, 11.0, 6.0, 11.0, 6.0, 10.0];
/// Füllgewichte der Polartabelle: Berechnen, Punkt, Hz, V, Strecke, ZH, Exz längs, Exz quer, R, H, Höhe, Code, Zeit.
const GEWICHTE_POLAR: [f32; 13] = [8.0, 9.0, 9.0, 9.0, 9.0, 6.0, 8.0, 8.0, 11.0, 11.0, 8.0, 6.0, 11.0];
/// Zeilenhöhe der Tabellen (RowTemplate.Height).
const ZEILEN_HOEHE: f32 = 26.0;
/// Höhe der Kopfzeile (ColumnHeadersHeight).
const KOPF_HOEHE: f32 = 32.0;
/// Platz für den senkrechten Rollbalken und die Ränder der Gruppen.
const TABELLEN_RAND: f32 = 40.0;
/// Kleinste Spaltenbreite.
const MIN_SPALTENBREITE: f32 = 36.0;
/// Höhe des Fußbereichs: Schaltflächen, darunter die Meldung (bis zwei Zeilen) und der Rand zum Fensterrand.
const UNTEN_HOEHE: f32 = 112.0;
/// Abstand zwischen Schaltflächen und Meldung sowie Rand unter der Meldung.
const UNTEN_ABSTAND: f32 = 6.0;
const UNTEN_RAND: f32 = 16.0;
/// Höhe der Schaltflächen.
const KNOPF_HOEHE: f32 = 36.0;
/// Höhe der Kopfschaltflächen.
const KOPF_KNOPF_HOEHE: f32 = 30.0;
/// Breite der Schaltflächen zum Laden der stp.csv und dat.csv.
const DATEI_KNOPF_BREITE: f32 = 260.0;
/// Höhe des Ergebnisfelds unter der Anschlusspunkttabelle.
const ERGEBNIS_HOEHE: f32 = 46.0;
/// Mindesthöhe jedes Teils des geteilten Hauptbereichs.
const MIN_TEIL_HOEHE: f32 = 120.0;
/// Höhe des Teilers und Vorgabe der Teilung (je zur Hälfte).
const TEILER_HOEHE: f32 = 6.0;
const TEILUNG_VORGABE: f32 = 0.5;
/// Platz über der Tabelle in einer Gruppe (Titel, Ränder) und unter dem Ergebnisfeld.
const GRUPPE_KOPF: f32 = 34.0;
/// Breite des Standpunkt-Auswahlfelds und des Höhenfelds.
const STANDPUNKT_BREITE: f32 = 130.0;
const HOEHEN_FELD_BREITE: f32 = 80.0;
/// Farben.
pub(super) const FARBE_GRAU: Color32 = Color32::from_rgb(211, 211, 211);
const FARBE_EXZ_GROSS: Color32 = Color32::from_rgb(255, 230, 100);
const FARBE_KOPF: Color32 = Color32::from_rgb(60, 100, 160);
const FARBE_HINWEIS_HINTERGRUND: Color32 = Color32::from_rgb(255, 244, 200);
const FARBE_ERGEBNIS_HINTERGRUND: Color32 = Color32::from_rgb(232, 238, 248);
const FARBE_SPEICHERN: Color32 = Color32::from_rgb(46, 125, 50);
const FARBE_RAHMEN: Color32 = Color32::from_rgb(160, 160, 160);
const FARBE_GRUEN: Color32 = Color32::from_rgb(180, 230, 160);
const FARBE_GELB: Color32 = Color32::from_rgb(255, 230, 100);
const FARBE_ROT: Color32 = Color32::from_rgb(255, 110, 100);
/// Ampel: grün bis Grenze − 1, gelb bis Grenze + 5.
const AMPEL_GRUEN_ABZUG: f64 = 1.0;
const AMPEL_GELB_ZUSCHLAG: f64 = 5.0;
/// Schriftfarben der Meldungen: Fehler rot (fett), Warnung orange, Information schwarz.
const FARBE_MELDUNG_FEHLER: Color32 = Color32::from_rgb(200, 0, 0);
const FARBE_MELDUNG_WARNUNG: Color32 = Color32::from_rgb(215, 110, 0);
const FARBE_MELDUNG_INFO: Color32 = Color32::BLACK;

/// Art einer Meldung im Fußbereich.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stufe {
    Info,
    Warnung,
    Fehler,
}

/// Offene Rückfrage.
enum Frage {
    /// Hinweise zur Berechnung vor dem Speichern (nur Information).
    Hinweise(String),
    Ueberschreiben(String),
    Kor(PathBuf),
}

/// Herkunft der Anschlussmessungen (obere Tabelle).
#[derive(Clone, Copy, PartialEq, Eq)]
enum AnschlussQuelle {
    /// Gespeicherte Stationierung (Station-{SP}.json).
    Station,
    /// AP-Zeilen der Messdaten-Datei (dat.csv).
    Dat,
    /// Vom Benutzer gewählte Standpunkt-Datei (stp.csv).
    Stp,
}

/// Formular der Polarpunkte.
pub struct PolarpunkteAnsicht {
    daten: Option<PolarMessdaten>,
    standpunkte: Vec<String>,
    sp_index: usize,
    instr_hoehe: String,
    quelle: AnschlussQuelle,
    /// Gewählte Standpunkt-Datei (stp.csv) für die Anschlusspunkte.
    stp_datei: Option<(PathBuf, StationierungsImport)>,
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
    status_stufe: Stufe,
    frage: Option<Frage>,
    /// Meldungsfenster (Titel, Text) wie eine MessageBox.
    meldung: Option<(String, String)>,
    /// Geöffnetes Fenster mit den Rechenparametern.
    rechenparameter: Option<RechenparameterAnsicht>,
    /// Anteil der oberen Tabelle am geteilten Hauptbereich.
    teilung: f32,
    /// Vom Benutzer gewählte Messdaten-Datei (statt der dat.csv des Projekts).
    eigene_datei: Option<PathBuf>,
}

/// Gruppenfeld mit fettem Titel (GroupBox der Originalversion).
fn gruppe<R>(ui: &mut egui::Ui, titel: &str, inhalt: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::new()
        .stroke(egui::Stroke::new(1.0, FARBE_RAHMEN))
        .inner_margin(6.0)
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.label(RichText::new(titel).strong());
            ui.add_space(2.0);
            inhalt(ui)
        })
        .inner
}

pub(super) fn ist_gruen(betrag: f64, grenze: f64) -> bool {
    betrag <= grenze - AMPEL_GRUEN_ABZUG
}

pub(super) fn ampel(betrag: f64, grenze: f64) -> Color32 {
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
    StationierungsErgebnis { massstab: 1.0, konvergiert: true, berechnung_3d: true, ist_lokales_system: true, ..StationierungsErgebnis::default() }
}

fn zeitstempel(text: &str) -> NaiveDateTime {
    if let Ok(z) = chrono::DateTime::parse_from_rfc3339(text) {
        return z.with_timezone(&chrono::Local).naive_local();
    }
    ["%Y-%m-%d %H:%M:%S", "%Y-%m-%d %H:%M"].iter().find_map(|format| NaiveDateTime::parse_from_str(text, format).ok()).unwrap_or_else(jetzt)
}

impl PolarpunkteAnsicht {
    pub fn neu(projekt: &Projekt) -> Self {
        let mut a = Self {
            daten: None,
            standpunkte: Vec::new(),
            sp_index: 0,
            instr_hoehe: String::new(),
            quelle: AnschlussQuelle::Dat,
            stp_datei: None,
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
            status_stufe: Stufe::Info,
            frage: None,
            meldung: None,
            rechenparameter: None,
            teilung: TEILUNG_VORGABE,
            eigene_datei: None,
        };
        a.lade_datei(projekt);
        a
    }

    /// Pfad der Messdaten: gewählte Datei oder dat.csv des Projekts.
    fn datei_pfad(&self, projekt: &Projekt) -> PathBuf {
        self.eigene_datei.clone().unwrap_or_else(|| messdaten::messdatei_pfad(&projekt.einstellungen))
    }

    /// Dateidialog für CSV-Dateien im Projektverzeichnis.
    fn csv_dialog(projekt: &Projekt, titel: &str) -> Option<PathBuf> {
        let mut dialog = rfd::FileDialog::new().set_title(titel).add_filter("CSV-Dateien (*.csv)", &["csv"]);
        if projekt.einstellungen.ist_geladen() {
            dialog = dialog.set_directory(&projekt.einstellungen.projekt_verzeichnis);
        }
        dialog.pick_file()
    }

    /// „Messdaten (dat.csv) laden“: Messdaten-Datei für die Polarpunkte wählen und einlesen.
    fn waehle_dat(&mut self, projekt: &Projekt) {
        if let Some(pfad) = Self::csv_dialog(projekt, "Messdaten (dat.csv) laden") {
            self.eigene_datei = Some(pfad);
            self.lade_datei(projekt);
        }
    }

    /// „Standpunktdaten (stp.csv) laden“: Anschlussmessungen für die Orientierung wählen und einlesen.
    fn waehle_stp(&mut self, projekt: &Projekt) {
        let Some(pfad) = Self::csv_dialog(projekt, "Standpunktdaten (stp.csv) laden") else {
            return;
        };
        let import = std::fs::read_to_string(&pfad).map_err(|e| e.to_string()).and_then(|text| stationierung_csv::parse(&text));
        match import {
            Ok(import) => {
                self.stp_datei = Some((pfad, import));
                self.quelle = AnschlussQuelle::Stp;
                self.lade_datei(projekt);
            }
            Err(fehler) => {
                fehler_log::log("Polarpunkte.waehle_stp", &fehler);
                self.setze_status(&format!("Standpunktdaten konnten nicht gelesen werden: {fehler}"), Stufe::Fehler);
                self.melde("Standpunktdaten laden", format!("Die Datei konnte nicht gelesen werden:\n{}\n\n{fehler}", pfad.display()));
            }
        }
    }

    /// AufstellungId aus dem Kopf der gewählten stp.csv.
    fn stp_aufstellung_id(&self) -> Option<&str> {
        self.stp_datei.as_ref().and_then(|(_, i)| i.dokument.meta("AufstellungId")).filter(|id| !id.trim().is_empty())
    }

    /// Standpunktgruppe der Messdaten, die zur stp.csv gehört: zuerst gleiche AufstellungId
    /// (bei mehreren Gruppen die mit den meisten Polarmessungen), sonst gleiche Standpunktnummer.
    fn passender_standpunkt_zur_stp(&self) -> Option<usize> {
        let daten = self.daten.as_ref()?;
        let anzahl_np = |g: &String, bedingung: &dyn Fn(&PolarMessung) -> bool| {
            daten.messungen.iter().filter(|m| m.gruppe() == *g && !m.ist_anschlusspunkt() && bedingung(m)).count()
        };
        let beste = |bedingung: &dyn Fn(&PolarMessung) -> bool| {
            self.standpunkte
                .iter()
                .enumerate()
                .filter(|(_, g)| daten.messungen.iter().any(|m| m.gruppe() == **g && bedingung(m)))
                .max_by_key(|(i, g)| (anzahl_np(g, bedingung), std::cmp::Reverse(*i)))
                .map(|(i, _)| i)
        };
        if let Some(id) = self.stp_aufstellung_id() {
            if let Some(i) = beste(&|m: &PolarMessung| m.aufstellung_id == id) {
                return Some(i);
            }
        }
        let sp = self.stp_kopf().0?;
        beste(&|m: &PolarMessung| m.standpunkt_nr.eq_ignore_ascii_case(sp))
    }

    /// Standpunktnummer und Instrumentenhöhe aus der gewählten stp.csv.
    fn stp_kopf(&self) -> (Option<&str>, Option<f64>) {
        self.stp_datei.as_ref().map_or((None, None), |(_, i)| (i.standpunkt.as_deref(), i.instrumentenhoehe))
    }

    fn sp_gruppe(&self) -> String {
        self.standpunkte.get(self.sp_index).cloned().unwrap_or_default()
    }
    fn sp_id(&self) -> String {
        self.daten.as_ref().and_then(|d| d.messungen.iter().find(|m| m.gruppe() == self.sp_gruppe())).map(|m| m.aufstellung_id.clone()).unwrap_or_default()
    }
    fn sp_name(&self) -> String {
        self.daten.as_ref().and_then(|d| d.messungen.iter().find(|m| m.gruppe() == self.sp_gruppe())).map(|m| m.standpunkt_nr.clone()).unwrap_or_default()
    }

    /// Meldung im Fußbereich mit ihrer Art (bestimmt Farbe und Schrift).
    fn setze_status(&mut self, text: &str, stufe: Stufe) {
        self.status = text.to_string();
        self.status_stufe = stufe;
    }

    fn zeige_leer(&mut self, meldung: &str, stufe: Stufe) {
        self.fehler.clear();
        self.ergebnis_text.clear();
        self.setze_status(meldung, stufe);
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
        if self.eigene_datei.is_none() && !projekt.einstellungen.ist_geladen() {
            self.zeige_leer("Kein Projekt geladen.", Stufe::Warnung);
            return;
        }
        let pfad = self.datei_pfad(projekt);
        if !pfad.exists() {
            let name = pfad.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            self.zeige_leer(&format!("Keine Messdaten-Datei vorhanden ({name})."), Stufe::Warnung);
            return;
        }
        let mut daten = match polar_messdaten::lese(&pfad, |nr| projekt.finde_anschlusspunkt(nr).is_some()) {
            Ok(d) => d,
            Err(fehler) => {
                fehler_log::log("Polarpunkte.lade_datei", &fehler.to_string());
                self.zeige_leer(&format!("Messdaten konnten nicht gelesen werden: {fehler}"), Stufe::Fehler);
                return;
            }
        };
        for m in daten.messungen.iter_mut().filter(|m| m.standpunkt_nr.is_empty()) {
            m.standpunkt_nr = OHNE_STANDPUNKT.to_string();
        }
        self.standpunkte = daten.standpunkte();
        self.daten = Some(daten);
        let aktuell = &projekt.station.standpunkt_nr;
        // Bei gewählter stp.csv den passenden Standpunkt der Messdaten wählen.
        let stp_index = if self.quelle == AnschlussQuelle::Stp { self.passender_standpunkt_zur_stp() } else { None };
        match stp_index
            .or_else(|| {
                self.standpunkte.iter().position(|s| s == aktuell || self.daten.as_ref().is_some_and(|d| d.messungen.iter().any(|m| m.gruppe() == **s && m.aufstellung_id == projekt.station.aufstellung_id)))
            })
            .or_else(|| self.standpunkte.len().checked_sub(1))
        {
            Some(i) => {
                self.sp_index = i;
                self.standpunkt_gewaehlt(projekt);
            }
            None => self.zeige_leer("Die Messdaten-Datei enthält keine Messungen.", Stufe::Warnung),
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
        let gruppe = self.sp_gruppe();
        let messung = daten.messungen.iter().find(|m| m.gruppe() == gruppe);
        // Instrumentenhöhe: gespeicherte Station derselben Aufstellung (enthält Korrekturen) vor der Höhe der
        // Messdaten; der Kopf einer stp.csv nur, wenn die Messdaten keine Höhe haben (Abweichung wird gemeldet).
        let gruppe_id = messung.map(|m| m.aufstellung_id.clone()).unwrap_or_default();
        let station_ih = projekt.lese_station_datei(&sp).filter(|d| !gruppe_id.is_empty() && d.aufstellung_id == gruppe_id).map(|d| d.instrumenten_hoehe);
        let stp_ih = self.stp_kopf().1.filter(|_| self.quelle == AnschlussQuelle::Stp);
        let ih = if let Some(ih) = station_ih {
            ih
        } else if let Some(ih) = messung.and_then(|m| m.instrumenten_hoehe) {
            ih
        } else if let Some(ih) = stp_ih {
            ih
        } else if &projekt.station.standpunkt_nr == &sp && projekt.station.instrumenten_hoehe > 0.0 {
            projekt.station.instrumenten_hoehe
        } else if daten.kopf_instr_hoehe > 0.0 {
            daten.kopf_instr_hoehe
        } else {
            projekt.einstellungen.instrumenten_hoehe
        };
        self.instr_hoehe = f(ih, 3);
        if self.quelle != AnschlussQuelle::Stp || self.stp_datei.is_none() {
            let gespeichert = projekt
                .lese_station_datei(&sp)
                .is_some_and(|d| !d.messungen.is_empty() && messung.is_some_and(|m| m.aufstellung_id.is_empty() || m.aufstellung_id == d.aufstellung_id));
            self.quelle = if gespeichert { AnschlussQuelle::Station } else { AnschlussQuelle::Dat };
        }
        self.fuelle_tabellen(projekt);
    }

    fn fuelle_tabellen(&mut self, projekt: &Projekt) {
        let sp = self.sp_name();
        self.lade_hinweise.clear();
        if let Some(d) = &self.daten {
            self.lade_hinweise.extend(d.hinweise.iter().cloned());
            let ohne_strecke: Vec<&str> =
                d.messungen.iter().filter(|m| m.gruppe() == self.sp_gruppe() && !m.ist_anschlusspunkt() && m.strecke_m <= 0.0).map(|m| m.punkt_nr.as_str()).collect();
            if !ohne_strecke.is_empty() {
                self.lade_hinweise.push(format!("Ohne Strecke, daher nicht berechenbar: {}", ohne_strecke.join(", ")));
            }
            if d.messungen.iter().any(|m| m.gruppe() == self.sp_gruppe() && m.typ_geschaetzt) {
                self.lade_hinweise.push("Altdatei: AP/NP wurde aus vorhandenen Koordinaten abgeleitet. Zuordnung prüfen.".into());
            }
            if d.messungen.iter().any(|m| m.gruppe() == self.sp_gruppe() && m.exz_unbekannt) {
                self.lade_hinweise.push("Exzentrizitäten fehlen: für die Berechnung wird 0 angenommen. Werte prüfen.".into());
            }
        }
        self.station_fehler.clear();
        self.ap.clear();
        self.np.clear();
        if let Some(daten) = self.daten.clone().filter(|_| !sp.is_empty()) {
            match self.quelle {
                AnschlussQuelle::Station => self.lade_anschluss_aus_station(projekt, &sp),
                AnschlussQuelle::Dat => self.lade_anschluss_aus_dat(projekt, &daten, &sp),
                AnschlussQuelle::Stp => self.lade_anschluss_aus_stp(&sp),
            }
            self.akt = self.ap.iter().map(|p| [true, true, p.hoehe.is_finite()]).collect();
            self.np = daten.messungen.iter().filter(|m| !m.ist_anschlusspunkt() && m.gruppe() == self.sp_gruppe()).cloned().collect();
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

    fn lade_anschluss_aus_stp(&mut self, sp: &str) {
        let Some((pfad, import)) = &self.stp_datei else {
            self.station_fehler = "Keine Standpunktdaten (stp.csv) geladen.".to_string();
            return;
        };
        self.ap = import.punkte.clone();
        let ih = parse_zahl(&self.instr_hoehe).unwrap_or(0.0);
        if let Some(stp_ih) = import.instrumentenhoehe.filter(|h| (h - ih).abs() > 0.0005) {
            let name = pfad.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            self.lade_hinweise.push(format!(
                "Instrumentenhöhe im Kopf der stp.csv ({name}: {} m) weicht ab; verwendet wird {} m (Station/Messdaten). Feld „IH“ prüfen.",
                f(stp_ih, 3),
                f(ih, 3)
            ));
        }
        if let Some(stp_sp) = import.standpunkt.as_deref().filter(|s| !s.eq_ignore_ascii_case(sp)) {
            let name = pfad.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let id = import.dokument.meta("AufstellungId").unwrap_or("");
            let gleiche_aufstellung = !id.is_empty() && id == self.sp_id();
            self.lade_hinweise.push(if gleiche_aufstellung {
                format!("stp.csv ({name}) gehört zur selben Aufstellung, aber mit Standpunktnummer {stp_sp} statt {sp}.")
            } else {
                format!("Standpunkt der stp.csv ({name}: {stp_sp}) weicht vom Standpunkt der Messdaten ({sp}) ab. Zuordnung prüfen.")
            });
        }
    }

    fn lade_anschluss_aus_dat(&mut self, projekt: &Projekt, daten: &PolarMessdaten, sp: &str) {
        for m in daten.messungen.iter().filter(|m| m.ist_anschlusspunkt() && m.standpunkt_nr == sp) {
            match projekt.finde_anschlusspunkt(&m.punkt_nr) {
                None => self.lade_hinweise.push(format!("Anschlusspunkt {}: keine Koordinaten gefunden – nicht verwendet.", m.punkt_nr)),
                Some((r, h, hoehe)) => {
                    self.ap.push(StationierungsPunkt { punkt_nr: m.punkt_nr.clone(), r, h, hoehe, hz: m.hz_gon, v: m.v_gon, strecke: m.strecke_m, zielhoehe: m.zielhoehe_m })
                }
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
        let (r, h, hoehe) = projekt.finde_punkt_mit_standpunkten(sp).ok_or_else(|| format!("Nur ein Anschlussziel: Der Standpunkt {sp} hat keine bekannten Koordinaten."))?;
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
            aufstellung_id: m.aufstellung_id.clone(),
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
            self.setze_status("Keine Orientierung berechenbar.", Stufe::Fehler);
            return;
        }
        let Some(station) = &self.station else {
            self.setze_status("", Stufe::Info);
            return;
        };
        let gewichtung = if par.gewichtung_nahe_ziele { format!("ein (Zielzentrierung {} mm)", f(par.zentrierfehler_ziel_mm, 1)) } else { "aus".to_string() };
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
        let text = format!("{} · m = {} · {herkunft} · {berechnet} Polarpunkt(e) berechnet", if drei_d { "3D" } else { "2D" }, f(self.massstab.wert, 6));
        self.setze_status(&text, Stufe::Info);
    }

    // ── Speichern ─────────────────────────────────────────────────────────────

    fn speichern(&mut self, projekt: &mut Projekt) {
        if self.sp_name() == OHNE_STANDPUNKT {
            self.melde(
                "Polarpunkte",
                "Diese Messungen haben keine Standpunktnummer und werden nicht gespeichert.\nBitte die Standpunktnummer in der Messdatei ergänzen (Daten-Editor).",
            );
            return;
        }
        self.neuberechnen(projekt);
        let anzahl = self.ergebnisse.iter().flatten().count();
        if self.station.is_none() || anzahl == 0 {
            self.melde("Polarpunkte", "Es sind keine berechneten Polarpunkte zum Speichern vorhanden.");
            return;
        }
        if self.hinweise.is_empty() {
            self.pruefe_ueberschreiben(projekt);
        } else {
            let liste: Vec<String> = self.hinweise.iter().map(|h| format!("• {h}")).collect();
            self.frage = Some(Frage::Hinweise(format!("Hinweise zu dieser Berechnung:\n\n{}", liste.join("\n"))));
        }
    }

    /// Fragt nach, wenn vorhandene Punkte stark vom neuen Ergebnis abweichen; sonst wird gespeichert.
    fn pruefe_ueberschreiben(&mut self, projekt: &mut Projekt) {
        let sp = self.sp_name();
        let abweichend: Vec<String> = self
            .ergebnisse
            .iter()
            .flatten()
            .filter_map(|e| {
                let alt = projekt.neupunkte.finde_koordinate_aufstellung(&e.punkt_nr, &sp, &self.sp_id())?;
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

    fn melde(&mut self, titel: &str, text: impl Into<String>) {
        self.meldung = Some((titel.to_string(), text.into()));
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
            let alt = projekt.neupunkte.finde_koordinate_aufstellung(&erg.punkt_nr, &sp, &self.sp_id()).map(|a| (a.r, a.h));
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
                self.setze_status(&format!("Speichern fehlgeschlagen: {fehler}"), Stufe::Fehler);
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
        self.status_stufe = Stufe::Info;
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
        } else {
            match self.quelle {
                AnschlussQuelle::Station => "gespeicherter Stationierung",
                AnschlussQuelle::Dat => "Anschlusspunkten der dat.csv",
                AnschlussQuelle::Stp => "Standpunktdaten der stp.csv",
            }
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
                self.melde("Kein Protokoll", "Kein Polarpunkte-Protokoll gefunden.\nEs wird beim Speichern erzeugt (sofern das Protokoll in den Projektoptionen aktiviert ist).")
            }
            Some(pfad) => {
                if let Err(fehler) = oeffne(&pfad) {
                    self.melde("Protokoll", format!("Protokoll konnte nicht geöffnet werden:\n{fehler}\n\nDatei: {}", pfad.display()));
                }
            }
        }
    }

    fn schreibe_geaenderte_exzentrizitaeten(&mut self, projekt: &Projekt) {
        if self.daten.is_none() {
            return;
        }
        let geaendert: Vec<(&crate::daten::polar_messdaten::PolarMessung, (f64, f64))> = self
            .np
            .iter()
            .zip(&self.exz_original)
            .filter(|(m, (l, q))| (l - m.exz_laengs_m).abs() > 1e-9 || (q - m.exz_quer_m).abs() > 1e-9)
            .map(|(m, alt)| (m, *alt))
            .collect();
        if geaendert.is_empty() {
            return;
        }
        // Zuordnung über den Datensatz der Datei, geprüft über Punktnummer und Zeitstempel (audit.md P3)
        let aenderungen: Vec<messdaten::ExzAenderung> = geaendert
            .iter()
            .map(|(m, _)| messdaten::ExzAenderung { datensatz: m.datensatz, punkt_nr: m.punkt_nr.clone(), zeitstempel: m.zeitstempel.clone(), laengs: m.exz_laengs_m, quer: m.exz_quer_m })
            .collect();
        // Für das Messprotokoll: Punkt, Aufstellung, Standpunkt, alte und neue Werte
        let protokoll_aenderungen: Vec<(String, String, String, (f64, f64), (f64, f64))> =
            geaendert.iter().map(|(m, alt)| (m.punkt_nr.clone(), m.aufstellung_id.clone(), m.standpunkt_nr.clone(), *alt, (m.exz_laengs_m, m.exz_quer_m))).collect();
        let pfad = self.datei_pfad(projekt);
        match messdaten::schreibe_exzentrizitaeten(&pfad, &aenderungen) {
            Ok(()) => {
                {
                    use crate::daten::messprotokoll::{self as mp, Eintrag, Ereignis};
                    let e = &projekt.einstellungen;
                    let eintraege = if mp::pfad(e).exists() { mp::lese(&mp::pfad(e)).unwrap_or_default() } else { Vec::new() };
                    for (nr, id, sp, (l0, q0), (l1, q1)) in &protokoll_aenderungen {
                        let mut z = Eintrag::neu(Ereignis::Exz)
                            .punkt(nr, messdaten::TYP_NEUPUNKT, "")
                            .exzentrizitaet(*l1, *q1)
                            .aenderung("Exz", &format!("{} / {}", f(*l0, 3), f(*q0, 3)), &format!("{} / {}", f(*l1, 3), f(*q1, 3)))
                            .bezug(mp::letzte_messung(&eintraege, id, nr, messdaten::TYP_NEUPUNKT))
                            .quelle(mp::QUELLE_HAND)
                            .text("Exzentrizität in der Polarpunktberechnung geändert (längs / quer)");
                        z.aufstellung_id.clone_from(id);
                        z.standpunkt_nr.clone_from(sp);
                        mp::protokolliere(e, z);
                    }
                }
                self.exz_original = self.np.iter().map(|m| (m.exz_laengs_m, m.exz_quer_m)).collect();
                // Die kanonische Ausgabe kann physische Zeilen verschieben: Anzeigeindizes über den Datensatz nachführen.
                if let (Some(daten), Ok(datei)) = (self.daten.as_mut(), crate::daten::csv_datei::CsvDatei::laden(&pfad)) {
                    for m in daten.messungen.iter_mut().chain(self.np.iter_mut()) {
                        if let Some(nr) = datei.zeilennummern.get(m.datensatz) {
                            m.zeilen_index = nr - 1;
                        }
                    }
                }
            }
            Err(fehler) => {
                fehler_log::log("Polarpunkte.schreibe_exzentrizitaeten", &fehler.to_string());
                self.melde("Polarpunkte", format!("Die geänderten Exzentrizitäten konnten nicht in die dat.csv geschrieben werden:\n{fehler}"));
            }
        }
    }

    // ── KOR-CSV ───────────────────────────────────────────────────────────────

    fn kor_csv(&mut self, projekt: &Projekt) {
        self.neuberechnen(projekt);
        if self.ergebnisse.iter().flatten().count() == 0 {
            self.melde("Als KOR-CSV speichern", "Es sind keine berechneten Polarpunkte vorhanden.");
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
        let pfad = crate::daten::csv_datei::CsvDatei::normalisiere_pfad(&pfad, crate::daten::csv_datei::TYP_KOORDINATEN);
        if pfad.exists() {
            self.frage = Some(Frage::Kor(pfad));
        } else {
            self.schreibe_kor(projekt, &pfad, false);
        }
    }

    fn schreibe_kor(&mut self, projekt: &Projekt, pfad: &Path, erhalten: bool) {
        let punkte: Vec<KorPunkt> =
            self.ergebnisse.iter().flatten().map(|e| KorPunkt { punkt_nr: e.punkt_nr.clone(), r: e.r, h: e.h, hoehe: e.ist_3d.then_some(e.hoehe), code: e.code.clone() }).collect();
        let sensor = if projekt.einstellungen.tachymeter_modell == TachymeterModell::Manuell { "" } else { projekt.einstellungen.tachymeter_modell.name() };
        let name = pfad.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        match kor_csv::schreibe(pfad, &projekt.einstellungen.projekt_name, sensor, &punkte, erhalten) {
            Ok(gesamt) => {
                protokoll::log("POLAR", &format!("KOR-CSV: {} Punkt(e) nach {name} ({gesamt} gesamt)", punkte.len()));
                self.setze_status(&format!("{} Punkt(e) in {name} gespeichert ({gesamt} gesamt).", punkte.len()), Stufe::Info);
            }
            Err(fehler) => {
                fehler_log::log("Polarpunkte.kor_csv", &fehler.to_string());
                self.melde("Als KOR-CSV speichern", format!("Die Datei konnte nicht geschrieben werden:\n{fehler}"));
            }
        }
    }

    // ── Darstellung ───────────────────────────────────────────────────────────

    /// Zeichnet die Maske; `true`, wenn „Schließen“ gewählt wurde.
    pub fn zeichne(&mut self, ui: &mut egui::Ui, projekt: &mut Projekt) -> bool {
        let ctx = ui.ctx().clone();
        let gesperrt = self.frage.is_some() || self.meldung.is_some() || self.rechenparameter.is_some();
        let mut schliessen = false;
        let mut neu_rechnen = false;
        ui.add_enabled_ui(!gesperrt, |ui| {
            neu_rechnen |= self.zeichne_kopf(ui, projekt);
            self.zeichne_hinweise(ui);
            ui.add_space(4.0);
            neu_rechnen |= self.zeichne_haupt(ui, projekt);
            ui.add_space(6.0);
            schliessen = self.zeichne_unten(ui, projekt);
        });
        neu_rechnen |= self.zeige_rechenparameter(&ctx, projekt);
        if neu_rechnen {
            self.neuberechnen(projekt);
        }
        self.zeichne_frage(&ctx, projekt);
        self.zeige_meldung(&ctx);
        schliessen
    }

    /// Kopfbereich: Standpunkt, Instrumentenhöhe, Orientierungsquelle, Rechenparameter, Neu laden.
    fn zeichne_kopf(&mut self, ui: &mut egui::Ui, projekt: &Projekt) -> bool {
        let mut neu_rechnen = false;
        let mut standpunkt_neu = false;
        let mut quelle_neu = false;
        let mut laden = false;
        let mut csv_laden = false;
        ui.horizontal(|ui| {
            ui.label("Standpunkt:");
            let vorher = self.sp_index;
            egui::ComboBox::from_id_salt("polar_standpunkt").width(STANDPUNKT_BREITE).selected_text(RichText::new(self.sp_gruppe()).monospace()).show_ui(ui, |ui| {
                for (i, sp) in self.standpunkte.iter().enumerate() {
                    ui.selectable_value(&mut self.sp_index, i, RichText::new(sp).monospace());
                }
            });
            standpunkt_neu = vorher != self.sp_index;
            ui.add_space(12.0);
            ui.label("Instrumentenhöhe [m]:");
            let antwort =
                ui.add(egui::TextEdit::singleline(&mut self.instr_hoehe).font(egui::TextStyle::Monospace).align(egui::Align2::RIGHT_CENTER).desired_width(HOEHEN_FELD_BREITE));
            neu_rechnen |= antwort.lost_focus();
            ui.add_space(12.0);
            if ui.add(egui::Button::new("Rechenparameter…").min_size(egui::Vec2::new(160.0, KOPF_KNOPF_HOEHE))).clicked() {
                self.rechenparameter = Some(RechenparameterAnsicht::default());
            }
            laden = ui.add(egui::Button::new("Neu laden").min_size(egui::Vec2::new(100.0, KOPF_KNOPF_HOEHE))).clicked();
        });
        let (mut stp_laden, mut stp_entfernen, mut projekt_datei) = (false, false, false);
        let knopf = egui::Vec2::new(DATEI_KNOPF_BREITE, KOPF_KNOPF_HOEHE);
        let dateiname = |pfad: &Path| pfad.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| STRICH.to_string());
        egui::Grid::new("polar_dateien").num_columns(4).spacing([12.0, 4.0]).show(ui, |ui| {
            // Obere Tabelle: Anschlussmessungen aus der stp.csv
            stp_laden = ui.add(egui::Button::new("Standpunktdaten (stp.csv) laden …").min_size(knopf)).clicked();
            match &self.stp_datei {
                Some((pfad, import)) => {
                    let text = format!("{}  ({} AP)", dateiname(pfad), import.punkte.len());
                    ui.label(RichText::new(text).monospace().strong()).on_hover_text(pfad.display().to_string());
                }
                None => {
                    ui.label(RichText::new("keine stp.csv gewählt").italics());
                }
            }
            stp_entfernen = ui.add_enabled(self.stp_datei.is_some(), egui::Button::new("stp.csv entfernen").min_size(egui::Vec2::new(140.0, KOPF_KNOPF_HOEHE))).clicked();
            ui.end_row();
            // Untere Tabelle: Polarmessungen aus der dat.csv
            csv_laden = ui.add(egui::Button::new("Messdaten (dat.csv) laden …").min_size(knopf)).clicked();
            let pfad = self.datei_pfad(projekt);
            ui.label(RichText::new(dateiname(&pfad)).monospace().strong()).on_hover_text(pfad.display().to_string());
            projekt_datei = ui
                .add_enabled(self.eigene_datei.is_some(), egui::Button::new("Projekt-dat.csv").min_size(egui::Vec2::new(140.0, KOPF_KNOPF_HOEHE)))
                .on_hover_text("Zurück zur Messdatei des Projekts")
                .clicked();
            ui.end_row();
        });
        if projekt_datei {
            self.eigene_datei = None;
            laden = true;
        }
        if stp_entfernen {
            self.stp_datei = None;
            self.quelle = AnschlussQuelle::Dat;
            laden = true;
        }
        ui.horizontal(|ui| {
            ui.label("Orientierung aus:");
            quelle_neu |= ui.radio_value(&mut self.quelle, AnschlussQuelle::Station, "gespeicherter Station").changed();
            quelle_neu |= ui.radio_value(&mut self.quelle, AnschlussQuelle::Dat, "neu aus dat.csv").changed();
            ui.add_enabled_ui(self.stp_datei.is_some(), |ui| {
                quelle_neu |= ui.radio_value(&mut self.quelle, AnschlussQuelle::Stp, "Standpunktdaten (stp.csv)").changed();
            });
        });
        if quelle_neu && self.quelle == AnschlussQuelle::Stp {
            if let Some(ih) = self.stp_kopf().1 {
                self.instr_hoehe = f(ih, 3);
            }
        }
        if stp_laden {
            self.waehle_stp(projekt);
        } else if csv_laden {
            self.waehle_dat(projekt);
        } else if laden {
            self.lade_datei(projekt);
        } else if standpunkt_neu {
            self.standpunkt_gewaehlt(projekt);
        } else if quelle_neu {
            self.fuelle_tabellen(projekt);
        }
        neu_rechnen
    }

    /// Gelbes Hinweisband über den Tabellen.
    fn zeichne_hinweise(&self, ui: &mut egui::Ui) {
        if self.hinweise.is_empty() {
            return;
        }
        egui::Frame::new().fill(FARBE_HINWEIS_HINTERGRUND).inner_margin(egui::Margin::symmetric(14, 4)).show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            for h in &self.hinweise {
                ui.label(RichText::new(format!("⚠ {h}")).color(FARBE_MELDUNG_WARNUNG));
            }
        });
    }

    /// Geteilter Hauptbereich: oben Orientierung / Anschlusspunkte, unten Polarpunkte; `true`, wenn neu zu rechnen ist.
    fn zeichne_haupt(&mut self, ui: &mut egui::Ui, projekt: &Projekt) -> bool {
        let gesamt = (ui.available_height() - UNTEN_HOEHE).max(2.0 * MIN_TEIL_HOEHE + TEILER_HOEHE);
        let nutzbar = gesamt - TEILER_HOEHE;
        let oben = (nutzbar * self.teilung).clamp(MIN_TEIL_HOEHE, nutzbar - MIN_TEIL_HOEHE);
        let unten = nutzbar - oben;
        let breite = ui.available_width();

        let mut geaendert = false;
        ui.allocate_ui(egui::Vec2::new(breite, oben), |ui| {
            geaendert |= gruppe(ui, "Orientierung / Anschlusspunkte", |ui| {
                let tabelle = (oben - GRUPPE_KOPF - ERGEBNIS_HOEHE - KOPF_HOEHE).max(ZEILEN_HOEHE);
                let geaendert = self.zeichne_anschluss(ui, projekt, breite, tabelle);
                self.zeichne_ergebnisfeld(ui);
                geaendert
            });
        });

        // Teiler zum Verschieben der Aufteilung
        let (rect, antwort) = ui.allocate_exact_size(egui::Vec2::new(breite, TEILER_HOEHE), egui::Sense::drag());
        let farbe = if antwort.hovered() || antwort.dragged() { FARBE_KOPF } else { FARBE_RAHMEN };
        ui.painter().line_segment([rect.left_center(), rect.right_center()], egui::Stroke::new(2.0, farbe));
        if antwort.hovered() || antwort.dragged() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical);
        }
        if antwort.dragged() {
            self.teilung = ((oben + antwort.drag_delta().y) / nutzbar).clamp(MIN_TEIL_HOEHE / nutzbar, 1.0 - MIN_TEIL_HOEHE / nutzbar);
        }

        ui.allocate_ui(egui::Vec2::new(breite, unten), |ui| {
            geaendert |= gruppe(ui, "Polarpunkte", |ui| {
                let tabelle = (unten - GRUPPE_KOPF - KOPF_HOEHE).max(ZEILEN_HOEHE);
                self.zeichne_polar(ui, breite, tabelle)
            });
        });
        geaendert
    }

    fn tabellen_raster(gewichte: &[f32], breite: f32) -> Raster {
        Raster::fuellend(gewichte, breite - TABELLEN_RAND, MIN_SPALTENBREITE).mit_zeilenhoehe(ZEILEN_HOEHE).mit_kopf_stil(FARBE_KOPF, Color32::WHITE).ohne_streifen()
    }

    fn zeichne_anschluss(&mut self, ui: &mut egui::Ui, projekt: &Projekt, breite: f32, hoehe: f32) -> bool {
        let par = &projekt.rechenparameter.params;
        let raster = Self::tabellen_raster(&GEWICHTE_ANSCHLUSS, breite);
        let mut geaendert = false;
        egui::Frame::new().fill(Color32::WHITE).show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            egui::ScrollArea::horizontal().id_salt("polar_ap_horizontal").auto_shrink([false, true]).show(ui, |ui| {
                ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
                raster.kopf(ui, &["Punkt", "Str.h [m]", "v Winkel [cc]", "v Quer [mm]", "Hz", "v Längs [mm]", "Str", "v Höhe [mm]", "Hoe", "Gewicht"]);
                egui::ScrollArea::vertical().id_salt("polar_ap").max_height(hoehe).auto_shrink([false, false]).show(ui, |ui| {
                    ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
                    for (i, p) in self.ap.iter().enumerate() {
                        let r = self.residuen.get(i).and_then(Option::as_ref);
                        let akt = &mut self.akt[i];
                        raster.zeile(ui, i, "", |z| {
                            let vorher = *akt;
                            z.rechts(&p.punkt_nr);
                            z.rechts(&r.map_or_else(|| STRICH.to_string(), |r| f(r.strecke_h, 3)));
                            wert_zelle(z, r.filter(|r| r.richtung_aktiv).map(|r| r.v_winkel_cc), par.fehlergrenze_cc_winkel);
                            wert_zelle(z, r.filter(|r| r.richtung_aktiv).map(|r| r.v_quer_mm), par.fehlergrenze_mm_strecke);
                            z.haken(&mut akt[0]);
                            wert_zelle(z, r.filter(|r| r.strecke_aktiv).map(|r| r.v_strecke_mm), par.fehlergrenze_mm_strecke);
                            z.haken(&mut akt[1]);
                            wert_zelle(z, r.filter(|r| r.hoehe_aktiv).map(|r| r.v_hoehe_mm), par.fehlergrenze_mm_hoehe);
                            z.haken(&mut akt[2]);
                            let gewicht = r.map(|r| r.gewicht_richtung).filter(|w| !w.is_nan());
                            z.rechts(&gewicht.map_or_else(|| STRICH.to_string(), |w| format!("{} %", f(w * PROZENT, 1))));
                            geaendert |= vorher != *akt;
                        });
                    }
                });
            });
        });
        geaendert
    }

    /// Ergebnisfeld unter der Anschlusspunkttabelle (Orientierung, Maßstab, s0) oder Fehlertext.
    fn zeichne_ergebnisfeld(&self, ui: &mut egui::Ui) {
        let text = if self.fehler.is_empty() { RichText::new(&self.ergebnis_text).color(Color32::BLACK) } else { RichText::new(&self.fehler).color(FARBE_MELDUNG_FEHLER).strong() };
        egui::Frame::new().fill(FARBE_ERGEBNIS_HINTERGRUND).inner_margin(egui::Margin::symmetric(6, 4)).show(ui, |ui| {
            ui.set_min_size(egui::Vec2::new(ui.available_width(), ERGEBNIS_HOEHE - 8.0));
            ui.label(text.monospace());
        });
    }

    fn zeichne_polar(&mut self, ui: &mut egui::Ui, breite: f32, hoehe: f32) -> bool {
        let raster = Self::tabellen_raster(&GEWICHTE_POLAR, breite);
        let mut geaendert = false;
        egui::Frame::new().fill(Color32::WHITE).show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            egui::ScrollArea::horizontal().id_salt("polar_np_horizontal").auto_shrink([false, true]).show(ui, |ui| {
                ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
                raster.kopf(
                    ui,
                    &["Berechnen", "Punkt", "Hz [gon]", "V [gon]", "Strecke [m]", "ZH [m]", "Exz längs [m]", "Exz quer [m]", "R [m]", "H [m]", "Höhe [m]", "Code", "Zeit"],
                );
                egui::ScrollArea::vertical().id_salt("polar_np").max_height(hoehe).auto_shrink([false, false]).show(ui, |ui| {
                    ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
                    for i in 0..self.np.len() {
                        let ergebnis = self.ergebnisse.get(i).and_then(Option::as_ref).cloned();
                        let hintergrund = ergebnis.is_none().then_some(FARBE_GRAU);
                        let m = &mut self.np[i];
                        let (exz_l, exz_q) = &mut self.exz_text[i];
                        let aktiv = &mut self.np_aktiv[i];
                        let (gross_l, gross_q) = (m.exz_laengs_m.abs() > EXZ_INFO_GRENZE_M, m.exz_quer_m.abs() > EXZ_INFO_GRENZE_M);
                        raster.zeile_farbig(ui, i, "", hintergrund, |z| {
                            let vorher = *aktiv;
                            z.haken(aktiv);
                            geaendert |= vorher != *aktiv;
                            z.rechts(&m.punkt_nr);
                            z.rechts(&f(m.hz_gon, 4));
                            z.rechts(&f(m.v_gon, 4));
                            z.rechts(&f(m.strecke_m, 3));
                            z.rechts(&f(m.zielhoehe_m, 3));
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
                            z.rechts(&ergebnis.as_ref().map_or_else(|| STRICH.to_string(), |e| f(e.r, 3)));
                            z.rechts(&ergebnis.as_ref().map_or_else(|| STRICH.to_string(), |e| f(e.h, 3)));
                            z.rechts(&ergebnis.as_ref().filter(|e| e.ist_3d).map_or_else(|| STRICH.to_string(), |e| f(e.hoehe, 3)));
                            z.rechts(&m.code);
                            z.rechts(&m.zeitstempel);
                        });
                    }
                });
            });
        });
        geaendert
    }

    /// Fußbereich: Schaltflächen rechts, darunter die Meldung über die volle Breite (Fehler rot und fett,
    /// Warnung orange, Information schwarz), darunter Rand zum Fensterrand; `true` bei „Schließen“.
    fn zeichne_unten(&mut self, ui: &mut egui::Ui, projekt: &mut Projekt) -> bool {
        let mut schliessen = false;
        let (mut neu, mut speichern, mut protokoll_zeigen, mut kor) = (false, false, false, false);
        ui.allocate_ui_with_layout(egui::Vec2::new(ui.available_width(), KNOPF_HOEHE), egui::Layout::right_to_left(egui::Align::Center), |ui| {
            {
                let knopf = |text: &str, breite: f32| egui::Button::new(text).min_size(egui::Vec2::new(breite, KNOPF_HOEHE));
                schliessen = ui.add(knopf("Schließen", 100.0)).clicked();
                kor = ui.add(knopf("Als KOR-CSV speichern", 180.0)).clicked();
                protokoll_zeigen = ui.add(knopf("Protokoll anzeigen", 150.0)).clicked();
                speichern =
                    ui.add(egui::Button::new(RichText::new("Speichern").color(Color32::WHITE)).fill(FARBE_SPEICHERN).min_size(egui::Vec2::new(110.0, KNOPF_HOEHE))).clicked();
                neu = ui.add(knopf("Neu berechnen", 130.0)).clicked();
            }
        });
        ui.add_space(UNTEN_ABSTAND);
        if !self.status.is_empty() {
            let text = match self.status_stufe {
                Stufe::Fehler => RichText::new(format!("✖ {}", self.status)).color(FARBE_MELDUNG_FEHLER).strong(),
                Stufe::Warnung => RichText::new(format!("⚠ {}", self.status)).color(FARBE_MELDUNG_WARNUNG),
                Stufe::Info => RichText::new(&self.status).color(FARBE_MELDUNG_INFO),
            };
            ui.add(egui::Label::new(text).wrap());
        }
        ui.add_space(UNTEN_RAND);
        if neu {
            self.neuberechnen(projekt);
        }
        if speichern {
            self.speichern(projekt);
        }
        if protokoll_zeigen {
            self.oeffne_letztes_protokoll(projekt);
        }
        if kor {
            self.kor_csv(projekt);
        }
        schliessen
    }

    /// Fenster mit den Rechenparametern; `true`, wenn es geschlossen wurde (dann ist neu zu rechnen).
    fn zeige_rechenparameter(&mut self, ctx: &egui::Context, projekt: &mut Projekt) -> bool {
        let Some(mut ansicht) = self.rechenparameter.take() else {
            return false;
        };
        let mut offen = true;
        let mut schliessen = false;
        egui::Window::new("Rechenparameter")
            .open(&mut offen)
            .collapsible(false)
            .default_size(egui::Vec2::new(620.0, 560.0))
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                ansicht.zeichne(ui, projekt);
                if ui.button("Schliessen").clicked() {
                    schliessen = true;
                }
            });
        if offen && !schliessen {
            self.rechenparameter = Some(ansicht);
            return false;
        }
        true
    }

    fn zeige_meldung(&mut self, ctx: &egui::Context) {
        let Some((titel, text)) = self.meldung.clone() else {
            return;
        };
        let mut ok = false;
        egui::Window::new(titel).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO).show(ctx, |ui| {
            ui.label(text);
            ui.add_space(6.0);
            ok = ui.button("OK").clicked();
        });
        if ok {
            self.meldung = None;
        }
    }

    fn zeichne_frage(&mut self, ctx: &egui::Context, projekt: &mut Projekt) {
        let Some(frage) = self.frage.take() else {
            return;
        };
        let mut antwort: Option<u8> = None;
        let titel = match &frage {
            Frage::Hinweise(_) => "Information",
            Frage::Ueberschreiben(_) => "Polarpunkte überschreiben",
            Frage::Kor(_) => "Als KOR-CSV speichern",
        };
        egui::Window::new(titel).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO).show(ctx, |ui| match &frage {
            Frage::Hinweise(text) => {
                ui.label(text);
                ui.add_space(6.0);
                if ui.button("OK").clicked() {
                    antwort = Some(0);
                }
            }
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
                ui.label("Die Datei ist bereits vorhanden.\n\nJa:  Punkte ergänzen bzw. ersetzen (andere Punkte bleiben erhalten)\nNein:  Datei neu anlegen (überschreiben)");
                ui.horizontal(|ui| {
                    if ui.button("Ja").clicked() {
                        antwort = Some(0);
                    }
                    if ui.button("Nein").clicked() {
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
            (Some(_), Frage::Hinweise(_)) => self.pruefe_ueberschreiben(projekt),
            (Some(2), Frage::Ueberschreiben(_)) => self.setze_status("Speichern abgebrochen – vorhandene Punkte unverändert.", Stufe::Warnung),
            (Some(a), Frage::Kor(pfad)) if a < 2 => self.schreibe_kor(projekt, &pfad, a == 0),
            _ => {}
        }
    }
}

/// Wertezelle mit Ampelfarbe (oder grau bei fehlendem Wert).
pub(super) fn wert_zelle(z: &mut Zellen<'_>, wert: Option<f64>, grenze: f64) {
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

/// Exzentrizitätszelle (rechtsbündig); bei großem Wert gelb hinterlegt. Liefert `true`, wenn der Text geändert wurde.
fn exz_zelle(z: &mut Zellen<'_>, text: &mut String, gross: bool) -> bool {
    let mut geaendert = false;
    z.zelle(|ui| {
        if gross {
            ui.painter().rect_filled(ui.max_rect().expand(1.0), 0.0, FARBE_EXZ_GROSS);
        }
        let breite = ui.available_width();
        geaendert = ui.add(egui::TextEdit::singleline(text).frame(egui::Frame::NONE).align(egui::Align2::RIGHT_CENTER).desired_width(breite)).changed();
    });
    geaendert
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

    /// Kopiert das Testprojekt Test5 (IH während der Aufstellung von 0,000 auf 1,500 m korrigiert) in einen Temp-Ordner.
    fn test5(name: &str) -> (Projekt, PathBuf) {
        let quelle = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("testdaten").join("polar-test5");
        let dir = std::env::temp_dir().join(format!("fb-polar-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for d in std::fs::read_dir(&quelle).unwrap().filter_map(Result::ok) {
            std::fs::copy(d.path(), dir.join(d.file_name())).unwrap();
        }
        (Projekt::fuer_test(&dir, "Test5"), dir)
    }

    #[test]
    fn dat_mit_korrigierter_instrumentenhoehe_wird_geladen_und_berechnet() {
        let (projekt, dir) = test5("dat");
        let a = PolarpunkteAnsicht::neu(&projekt);
        assert!(a.daten.is_some(), "Status: {}", a.status);
        assert_eq!(a.standpunkte.len(), 1);
        assert_eq!(a.instr_hoehe, "1.500");
        assert!(a.quelle == AnschlussQuelle::Station);
        assert_eq!(a.ap.len(), 4);
        assert_eq!(a.np.len(), 20);
        assert!(a.station.is_some(), "Fehler: {} {}", a.station_fehler, a.fehler);
        assert_eq!(a.ergebnisse.iter().flatten().count(), 20);
        assert!(a.hinweise.iter().any(|h| h.contains("0.000 → 1.500")), "{:?}", a.hinweise);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn stp_datei_liefert_die_anschlussmessungen() {
        let (projekt, dir) = test5("stp");
        let mut a = PolarpunkteAnsicht::neu(&projekt);
        let pfad = dir.join("Test5-Anschlusspunkte-stp.csv");
        let import = stationierung_csv::parse(&std::fs::read_to_string(&pfad).unwrap()).unwrap();
        a.stp_datei = Some((pfad, import));
        a.quelle = AnschlussQuelle::Stp;
        a.lade_datei(&projekt);
        assert!(a.quelle == AnschlussQuelle::Stp);
        assert_eq!(a.ap.len(), 4);
        assert_eq!(a.instr_hoehe, "1.500", "Kopf der stp.csv (0.000) ist veraltet");
        assert!(a.station.is_some(), "Fehler: {} {}", a.station_fehler, a.fehler);
        assert_eq!(a.ergebnisse.iter().flatten().count(), 20);
        assert!(a.hinweise.iter().any(|h| h.contains("Kopf der stp.csv")), "{:?}", a.hinweise);
        let _ = std::fs::remove_dir_all(dir);
    }
}
