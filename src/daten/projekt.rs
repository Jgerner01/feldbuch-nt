// Zweck:    Aktives Projekt: bündelt Einstellungen, Rechenparameter, Projektdaten, Punktlisten und die
//           aktive Stationierung (ersetzt die statischen Manager der C#-Version)
// Hardware: keine
// Autor:    Johann Gerner

use std::fs;
use std::io;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::daten::einstellungen::Einstellungen;
use crate::daten::json_speicher;
use crate::daten::modelle::{FeldbuchPunkt, NeupunktErgebnis, StationierungsErgebnis, StationierungsPunkt, TYP_NEUPUNKT, TYP_STANDPUNKT, heute_text};
use crate::daten::projektdaten::ProjektdatenManager;
use crate::daten::punktlisten::{FeldbuchpunkteManager, ImportPunkteManager, NeupunkteManager};
use crate::daten::rechenparameter::RechenparameterManager;
use crate::grundlagen::format::{f, parse_zahl};
use crate::grundlagen::{datei_speicher, fehler_log, pfade, protokoll};
use crate::rechnen::freie_stationierung::{self, Optionen};
use crate::rechnen::neupunkt::{ist_verwendbar, pruefe_station};

/// Kategorie der Stationierung in den Projektdaten.
const KATEGORIE_STATIONIERUNG: &str = "Freie Stationierung";
/// Kategorie der aktiven Aufstellung in den Projektdaten (je Projekt, nicht global).
pub const KATEGORIE_AKTIV: &str = "Aktiver Standpunkt";
/// Präfix der vorgeschlagenen Standpunktnummern; jedes Projekt beginnt mit STP1.
pub const STANDPUNKT_PRAEFIX: &str = "STP";
/// Zeichen, die in einer Standpunktnummer nicht erlaubt sind (Teil des Dateinamens).
const UNGUELTIGE_ZEICHEN: &str = r#"\/:*?"<>|"#;
/// Unterschied der Instrumentenhöhe, ab dem sie als geändert gilt [m].
const IH_TOLERANZ_M: f64 = 0.0005;
/// Zeitformat des Aufstellungsbeginns.
const BEGINN_FORMAT: &str = "%Y-%m-%dT%H:%M:%S";
/// Unterordner für Sicherungen vor dem Umbenennen von Standpunkten.
const SICHERUNG_ORDNER: &str = "Sicherung";
/// Quelle für live berechnete Standpunkte.
#[cfg(test)]
const QUELLE_LIVE: &str = "Freie Stationierung (Live)";
/// Quelle für manuell berechnete und übernommene Standpunkte.
const QUELLE_MANUELL: &str = "Freie Stationierung";
/// Quelle für übernommene Neupunkte.
pub const QUELLE_LOKAL: &str = "Lokal";
/// Mindestanzahl verschiedener Anschlusspunkte für eine gültige Station.
const MIN_ANSCHLUSSPUNKTE: usize = 2;
/// Mindestanzahl Spalten der Anschlusspunktedatei (Nr, R, H).
/// Dateiname der Anschlusspunkte.
const ANSCHLUSSPUNKTE_DATEI: &str = "Anschlusspunkte.csv";

/// Inhalt der Datei Station-{Nr}.json.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct StationsdatenDatei {
    #[serde(rename = "AufstellungId", default)]
    pub aufstellung_id: String,
    #[serde(rename = "StandpunktNr", default)]
    pub standpunkt_nr: String,
    #[serde(rename = "InstrumentenHoehe", default)]
    pub instrumenten_hoehe: f64,
    #[serde(rename = "Datum", default)]
    pub datum: String,
    #[serde(rename = "Messungen")]
    pub messungen: Vec<StationierungsPunkt>,
    #[serde(rename = "Ergebnis", default)]
    pub ergebnis: Option<StationierungsErgebnis>,
    /// Aktivierung der Richtungen je Messung (gleiche Reihenfolge wie `messungen`); leer in alten Dateien = alle aktiv.
    #[serde(rename = "AktivRichtung", default)]
    pub aktiv_richtung: Vec<bool>,
    /// Aktivierung der Strecken je Messung.
    #[serde(rename = "AktivStrecke", default)]
    pub aktiv_strecke: Vec<bool>,
    /// Aktivierung der Höhen je Messung.
    #[serde(rename = "AktivHoehe", default)]
    pub aktiv_hoehe: Vec<bool>,
    /// Standpunktcode (z. B. vermarkt / temporär).
    #[serde(rename = "Code", default)]
    pub code: String,
    /// Beginn der Aufstellung (yyyy-MM-ddTHH:mm:ss); leer in alten Dateien.
    #[serde(rename = "Beginn", default)]
    pub beginn: String,
    /// Instrument steht auf einem bekannten Punkt (dann genügt ein Anschlussziel).
    #[serde(rename = "BekannterStandpunkt", default)]
    pub bekannt: Option<BekannterStandpunkt>,
    /// Gerätezustand beim Anlegen der Aufstellung.
    #[serde(rename = "Geraet", default)]
    pub geraet: Option<Geraetezustand>,
}

/// Bekannter Standpunkt: Koordinaten stehen fest, nur die Orientierung wird bestimmt.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct BekannterStandpunkt {
    #[serde(rename = "PunktNr")]
    pub punkt_nr: String,
    #[serde(rename = "R")]
    pub r: f64,
    #[serde(rename = "H")]
    pub h: f64,
    /// NaN bzw. null, wenn keine Höhe bekannt ist.
    #[serde(rename = "Hoehe", with = "crate::grundlagen::serde_hilfen::f64_text")]
    pub hoehe: f64,
}

/// Gerätezustand beim Anlegen der Aufstellung (für Protokoll und Fehlersuche).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Geraetezustand {
    #[serde(rename = "Modell")]
    pub modell: String,
    #[serde(rename = "Verbunden")]
    pub verbunden: bool,
    #[serde(rename = "EdmModus")]
    pub edm_modus: String,
    #[serde(rename = "Prisma")]
    pub prisma: String,
    #[serde(rename = "Prismenkonstante_mm")]
    pub prismenkonstante_mm: f64,
    /// Atmosphärische Korrektur der letzten Messung [ppm].
    #[serde(rename = "Atm_ppm")]
    pub atm_ppm: Option<f64>,
    /// Längs- und Querneigung der letzten Messung [mgon].
    #[serde(rename = "Laengsneigung_mgon")]
    pub laengsneigung_mgon: Option<f64>,
    #[serde(rename = "Querneigung_mgon")]
    pub querneigung_mgon: Option<f64>,
}

impl Geraetezustand {
    /// Einzeilige Beschreibung für Protokolle.
    pub fn text(&self) -> String {
        let mut teile = vec![
            if self.modell.is_empty() { "Gerät unbekannt".to_string() } else { self.modell.clone() },
            if self.verbunden { "verbunden".to_string() } else { "nicht verbunden".to_string() },
            format!("EDM {}", self.edm_modus),
            format!("Prisma {} ({} mm)", self.prisma, f(self.prismenkonstante_mm, 1)),
        ];
        if let Some(ppm) = self.atm_ppm {
            teile.push(format!("{} ppm", f(ppm, 1)));
        }
        if let (Some(l), Some(q)) = (self.laengsneigung_mgon, self.querneigung_mgon) {
            teile.push(format!("Neigung längs {} / quer {} mgon", f(l, 1), f(q, 1)));
        }
        teile.join(", ")
    }
}

/// Eingaben für eine neue Aufstellung.
#[derive(Clone, Debug, Default)]
pub struct NeuerStandpunkt {
    pub nr: String,
    pub ih: f64,
    pub code: String,
    pub bekannt: Option<BekannterStandpunkt>,
    pub geraet: Option<Geraetezustand>,
}

/// Zeile der Standpunktübersicht.
#[derive(Clone, Debug, PartialEq)]
pub struct StandpunktZeile {
    pub nr: String,
    pub aufstellung_id: String,
    pub beginn: String,
    pub instr_hoehe: f64,
    pub anschluss: usize,
    pub s0_mm: Option<f64>,
    pub gueltig: bool,
    pub aktiv: bool,
}

/// Aktivierung der Beobachtungen je Anschlusspunkt (Reihenfolge wie die Messungen).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Aktivierung {
    pub richtung: Vec<bool>,
    pub strecke: Vec<bool>,
    pub hoehe: Vec<bool>,
}

impl Aktivierung {
    /// Anzahl der abgeschalteten Beobachtungen (Richtungen, Strecken und Höhen zusammen).
    pub fn anzahl_abgeschaltet(&self) -> usize {
        [&self.richtung, &self.strecke, &self.hoehe].iter().map(|liste| liste.iter().filter(|aktiv| !**aktiv).count()).sum()
    }
}

impl StationsdatenDatei {
    /// Alte Dateien (vor Version 0.5.0) haben keine Listen, nur die Aktivierungsflags in den Residuen des Ergebnisses.
    /// Daraus werden die Listen abgeleitet, wenn Residuen und Messungen Punkt für Punkt zusammenpassen.
    fn uebernimm_aktivierung_aus_ergebnis(&mut self) {
        if !(self.aktiv_richtung.is_empty() && self.aktiv_strecke.is_empty() && self.aktiv_hoehe.is_empty()) {
            return;
        }
        let Some(ergebnis) = &self.ergebnis else {
            return;
        };
        let passt = ergebnis.residuen.len() == self.messungen.len() && ergebnis.residuen.iter().zip(&self.messungen).all(|(r, m)| r.punkt_nr == m.punkt_nr);
        if passt {
            self.aktiv_richtung = ergebnis.residuen.iter().map(|r| r.richtung_aktiv).collect();
            self.aktiv_strecke = ergebnis.residuen.iter().map(|r| r.strecke_aktiv).collect();
            self.aktiv_hoehe = ergebnis.residuen.iter().map(|r| r.hoehe_aktiv).collect();
        }
    }

    /// Fügt eine Messung hinzu; ihre Beobachtungen sind aktiv. Die bisherige Auswahl bleibt erhalten.
    #[cfg(test)]
    pub fn fuege_messung_hinzu(&mut self, punkt: StationierungsPunkt) {
        self.gleiche_aktivierung_an();
        self.messungen.push(punkt);
        self.aktiv_richtung.push(true);
        self.aktiv_strecke.push(true);
        self.aktiv_hoehe.push(true);
    }

    /// Bringt die Aktivierungslisten auf die Zahl der Messungen: neue Messungen sind aktiv, überzählige Einträge entfallen.
    /// Liefert `true`, wenn eine bereits gespeicherte, nicht leere Liste nicht zur Messungszahl passte.
    pub fn gleiche_aktivierung_an(&mut self) -> bool {
        self.uebernimm_aktivierung_aus_ergebnis();
        let n = self.messungen.len();
        let mut abweichung = false;
        for liste in [&mut self.aktiv_richtung, &mut self.aktiv_strecke, &mut self.aktiv_hoehe] {
            if !liste.is_empty() && liste.len() != n {
                abweichung = true;
            }
            liste.resize(n, true);
        }
        abweichung
    }

    /// Aktivierung aller Beobachtungen (nach Angleichung an die Messungszahl).
    pub fn aktivierung(&self) -> Aktivierung {
        let mut kopie = self.clone();
        kopie.gleiche_aktivierung_an();
        Aktivierung { richtung: kopie.aktiv_richtung, strecke: kopie.aktiv_strecke, hoehe: kopie.aktiv_hoehe }
    }

    /// Entfernt alle Messungen mit der Punktnummer samt ihrer Aktivierungseinträge.
    #[cfg(test)]
    pub fn entferne_messung(&mut self, punkt_nr: &str) {
        self.gleiche_aktivierung_an();
        let mut i = 0;
        while i < self.messungen.len() {
            if self.messungen[i].punkt_nr == punkt_nr {
                self.messungen.remove(i);
                self.aktiv_richtung.remove(i);
                self.aktiv_strecke.remove(i);
                self.aktiv_hoehe.remove(i);
            } else {
                i += 1;
            }
        }
    }
}

/// Ergebnis einer Festpunktübernahme.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UebernahmeErgebnis {
    pub neu: usize,
    pub ersetzt: usize,
}

/// Änderung der Stationierung eines Standpunkts samt Folgen für die Neupunkte (für die Rückmeldung an den Benutzer).
#[derive(Clone, Debug)]
pub struct StationierungsAenderung {
    pub standpunkt_nr: String,
    pub alt: StationierungsErgebnis,
    pub neu: StationierungsErgebnis,
    /// Anzahl der Neupunkte des Standpunkts, die beim Übernehmen neu berechnet werden.
    pub neupunkte: usize,
    /// Davon im Polarmodul berechnete Punkte.
    pub davon_polarpunkte: usize,
}

/// Das aktive Projekt mit allen Datenverwaltungen.
pub struct Projekt {
    pub einstellungen: Einstellungen,
    pub rechenparameter: RechenparameterManager,
    pub projektdaten: ProjektdatenManager,
    pub feldbuchpunkte: FeldbuchpunkteManager,
    pub neupunkte: NeupunkteManager,
    pub importpunkte: ImportPunkteManager,
    pub station: StationsdatenDatei,
    station_pfad: PathBuf,
    /// Zähler, der bei jeder Neuberechnung der Neupunkte nach einer Stationierung steigt (Anzeigen aktualisieren).
    pub station_version: u64,
}

impl Projekt {
    /// Startet wie das Hauptfenster der C#-Version: Einstellungen laden, Manager im Projektordner anlegen.
    pub fn starte() -> Self {
        pfade::initialisiere_benutzerdaten();
        let mut einstellungen = Einstellungen::neu();
        einstellungen.laden();
        datei_speicher::setze_auto_backup(einstellungen.auto_backup);
        let rechenparameter = RechenparameterManager::initialisiere(&pfade::benutzer_get("Freie-Stationierung.xml"));
        let mut projekt = Self {
            einstellungen,
            rechenparameter,
            projektdaten: ProjektdatenManager::leer(),
            feldbuchpunkte: FeldbuchpunkteManager::default(),
            neupunkte: NeupunkteManager::default(),
            importpunkte: ImportPunkteManager::default(),
            station: StationsdatenDatei::default(),
            station_pfad: PathBuf::new(),
            station_version: 0,
        };
        projekt.manager_initialisieren();
        projekt.messprotokoll_oeffnen();
        projekt
    }

    /// Initialisiert alle projektbezogenen Manager im aktuellen Projektverzeichnis.
    pub fn manager_initialisieren(&mut self) {
        let e = &self.einstellungen;
        self.projektdaten = ProjektdatenManager::initialisiere(&e.get_pfad("Projektdaten.csv"));
        self.feldbuchpunkte = FeldbuchpunkteManager::initialisiere(&e.get_pfad("Feldbuchpunkte.json"));
        self.neupunkte = NeupunkteManager::initialisiere(&e.get_pfad(&format!("{}-Neupunkte.json", e.projekt_name)));
        self.importpunkte = ImportPunkteManager::initialisiere(&e.get_pfad("ImportPunkte.json"));
        self.station_zuruecksetzen();
        self.lade_aktuellen_standpunkt();
        crate::daten::messprotokoll::konfiguriere(&self.einstellungen);
        protokoll::konfiguriere(self.einstellungen.protokoll_aktiv, self.einstellungen.ist_geladen().then(|| self.einstellungen.projekt_verzeichnis.clone()));
    }

    /// Beim Öffnen eines Projekts: einmalig Start-Protokoll aus Altdaten (P7), dann Ereignis PROJEKT.
    pub fn messprotokoll_oeffnen(&self) {
        use crate::daten::messprotokoll::{self as mp, Eintrag, Ereignis};
        if !self.einstellungen.ist_geladen() {
            return;
        }
        match mp::erzeuge_startprotokoll(&self.einstellungen) {
            Ok(n) if n > 0 => protokoll::log("PROJEKT", &format!("Messprotokoll aus Altdaten erzeugt ({n} Einträge)")),
            Ok(_) => {}
            Err(e) => fehler_log::log("Projekt.messprotokoll_oeffnen", &e.to_string()),
        }
        let mut e = Eintrag::neu(Ereignis::Projekt).quelle(mp::QUELLE_PROGRAMM).mit_prisma(&self.einstellungen).text(format!(
            "Projekt geöffnet: {} (Programm {})",
            self.einstellungen.projekt_name,
            env!("CARGO_PKG_VERSION")
        ));
        if self.hat_aktive_aufstellung() {
            e = e.mit_aufstellung(&self.station);
        }
        mp::protokolliere(&self.einstellungen, e);
    }

    /// Wechselt in ein (neues oder bestehendes) Projekt.
    pub fn projekt_wechseln(&mut self, name: &str, verzeichnis: &std::path::Path, leer_anlegen: bool) -> Result<(), String> {
        if leer_anlegen {
            self.einstellungen.neues_leeres_projekt(name, verzeichnis)?;
        } else {
            self.einstellungen.set_projekt(name, verzeichnis)?;
        }
        self.manager_initialisieren();
        protokoll::log("PROJEKT", &format!("Projekt gewechselt: {name}"));
        self.messprotokoll_oeffnen();
        Ok(())
    }

    // ── Anschlusspunktdatei ───────────────────────────────────────────────────

    /// Pfad der Anschlusspunktedatei im Projektverzeichnis.
    pub fn anschlusspunkte_pfad(&self) -> PathBuf {
        let neu = self.anschlusspunkte_zielpfad();
        let alt = self.einstellungen.get_pfad(ANSCHLUSSPUNKTE_DATEI);
        if neu.exists() || !alt.exists() { neu } else { alt }
    }

    /// Sucht Koordinaten (R, H, Höhe) eines Anschlusspunkts: Importpunkte, Feldbuchpunkte, Anschlusspunkte.csv.
    /// Standpunkte (Feldbuchpunkte vom Typ Standpunkt) zählen nicht: ein Standpunkt gleicher Nummer
    /// darf nie als Anschlusskoordinate dienen.
    pub fn finde_anschlusspunkt(&self, punkt_nr: &str) -> Option<(f64, f64, f64)> {
        self.finde_koordinate(punkt_nr, false)
    }

    /// Wie `finde_anschlusspunkt`, aber auch berechnete Standpunkte werden gefunden (z. B. für einen bekannten Standpunkt).
    pub fn finde_punkt_mit_standpunkten(&self, punkt_nr: &str) -> Option<(f64, f64, f64)> {
        self.finde_koordinate(punkt_nr, true)
    }

    fn finde_koordinate(&self, punkt_nr: &str, mit_standpunkten: bool) -> Option<(f64, f64, f64)> {
        if punkt_nr.trim().is_empty() {
            return None;
        }
        if let Some(p) = self.importpunkte.punkte().iter().find(|p| p.punkt_nr.eq_ignore_ascii_case(punkt_nr)) {
            return Some((p.r, p.h, if p.hoehe_vorhanden == Some(false) { f64::NAN } else { p.hoehe }));
        }
        if let Some(p) = self.feldbuchpunkte.punkte().iter().find(|p| p.punkt_nr.eq_ignore_ascii_case(punkt_nr) && (mit_standpunkten || p.typ != TYP_STANDPUNKT)) {
            return Some((p.r, p.h, p.hoehe));
        }
        let d = crate::daten::csv_datei::CsvDatei::laden(&self.anschlusspunkte_pfad()).ok()?;
        let i = d.zeilen.iter().position(|z| d.zelle(z, "PunktNr").eq_ignore_ascii_case(punkt_nr))?;
        Some((d.zahl(i, "X").ok()?, d.zahl(i, "Y").ok()?, if d.zelle(&d.zeilen[i], "Z").is_empty() { f64::NAN } else { d.zahl(i, "Z").ok()? }))
    }

    pub fn anschlusspunkte_zielpfad(&self) -> PathBuf {
        let name = if self.einstellungen.projekt_name.is_empty() { "Anschlusspunkte".to_string() } else { format!("{}-Anschlusspunkte", self.einstellungen.projekt_name) };
        self.einstellungen.get_pfad(&format!("{name}-stp.csv"))
    }

    // ── Festpunktübernahme ────────────────────────────────────────────────────

    /// Gibt es unter dieser Nummer bereits einen Festpunkt vom Typ Neupunkt?
    pub fn festpunkt_existiert(&self, punkt_nr: &str) -> bool {
        self.feldbuchpunkte.punkte().iter().any(|p| p.typ == TYP_NEUPUNKT && p.punkt_nr == punkt_nr)
    }

    /// Gemessene Neupunkte, die übernommen werden können (Koordinaten vorhanden), sortiert.
    pub fn festpunkt_kandidaten(&self) -> Vec<NeupunktErgebnis> {
        let mut liste: Vec<NeupunktErgebnis> = self.neupunkte.koordinaten().iter().filter(|e| !e.punkt_nr.trim().is_empty() && (e.r != 0.0 || e.h != 0.0)).cloned().collect();
        liste.sort_by(|a, b| a.punkt_nr.to_lowercase().cmp(&b.punkt_nr.to_lowercase()).then_with(|| a.standpunkt_nr.to_lowercase().cmp(&b.standpunkt_nr.to_lowercase())));
        liste
    }

    /// Kopiert Neupunkte als feste Koordinaten nach Feldbuchpunkte.json.
    pub fn festpunkte_uebernehmen(&mut self, punkte: &[NeupunktErgebnis]) -> io::Result<UebernahmeErgebnis> {
        let (mut neu, mut ersetzt) = (0, 0);
        let datum = heute_text();
        for e in punkte {
            if self.festpunkt_existiert(&e.punkt_nr) {
                ersetzt += 1;
            } else {
                neu += 1;
            }
            self.feldbuchpunkte.add_or_update(FeldbuchPunkt {
                punkt_nr: e.punkt_nr.clone(),
                typ: TYP_NEUPUNKT.to_string(),
                r: e.r,
                h: e.h,
                hoehe: e.hoehe,
                orientierung_gon: 0.0,
                ist_berechnung_3d: e.ist_3d,
                datum: datum.clone(),
                quelle: QUELLE_LOKAL.to_string(),
            })?;
        }
        Ok(UebernahmeErgebnis { neu, ersetzt })
    }

    // ── Stationsdaten ─────────────────────────────────────────────────────────

    fn neue_station(nr: &str) -> StationsdatenDatei {
        StationsdatenDatei { aufstellung_id: crate::daten::messdaten::neue_aufstellung_id(nr), standpunkt_nr: nr.to_string(), datum: heute_text(), ..StationsdatenDatei::default() }
    }

    /// Instrumentenhöhe der aktiven Station [m].
    pub fn instrumenten_hoehe(&self) -> f64 {
        self.station.instrumenten_hoehe
    }

    /// Korrigiert die Instrumentenhöhe der aktiven Aufstellung. Die Aufstellung bleibt dieselbe (keine neue Id);
    /// bei gültiger Stationierung werden ihre Neupunkte mit der neuen Höhe neu berechnet.
    pub fn set_instrumenten_hoehe(&mut self, wert: f64) -> io::Result<()> {
        if !wert.is_finite() || wert < 0.0 {
            return Err(crate::daten::csv_basis::fehler("Ungültige Instrumentenhöhe."));
        }
        if !self.hat_aktive_aufstellung() {
            return Err(crate::daten::csv_basis::fehler("Kein Standpunkt aktiv – bitte „Neuer Stp“."));
        }
        datei_speicher::pruefe_schreibzugriff(&self.station_pfad)?;
        let alt = self.station.instrumenten_hoehe;
        if (alt - wert).abs() <= IH_TOLERANZ_M {
            return Ok(());
        }
        // Höhenkette (audit.md S3): Eine aus Anschlusszielen abgeleitete Stationshöhe (Bodenpunkt) enthält −IH;
        // sie ändert sich deshalb um −ΔIH, die Neupunkthöhen bleiben gleich. Eine fest vorgegebene Höhe eines
        // bekannten Standpunkts bleibt fest; dann ändern sich die Neupunkthöhen fachlich korrekt um +ΔIH.
        let hoehe_vorher = self.station.ergebnis.as_ref().map(|e| e.hoehe);
        if self.stationshoehe_aus_zielen() {
            if let Some(e) = self.station.ergebnis.as_mut() {
                e.hoehe -= wert - alt;
            }
        }
        self.station.instrumenten_hoehe = wert;
        self.station_speichern()?;
        self.projektdaten.set_value(KATEGORIE_AKTIV, "Instrumentenhoehe [m]", &f(wert, 3))?;
        let mut wirkung = String::new();
        if let (Some(vorher), Some(e)) = (hoehe_vorher, self.station.ergebnis.clone()) {
            if (vorher - e.hoehe).abs() > 1e-9 {
                wirkung = format!("Stationshöhe {} → {} m", f(vorher, 3), f(e.hoehe, 3));
                self.aktualisiere_stationshoehe(e.hoehe)?;
            }
        }
        protokoll::log("STANDPKT", &format!("Instrumentenhöhe {} korrigiert: {} → {} m {wirkung}", self.station.standpunkt_nr, f(alt, 3), f(wert, 3)));
        let neu_berechnet = self.neupunkte_neu_berechnen();
        {
            use crate::daten::messprotokoll::{self as mp, Eintrag, Ereignis};
            let mut text = if wirkung.is_empty() { String::new() } else { format!("{wirkung}; ") };
            text.push_str(match &neu_berechnet {
                Ok(true) => "Neupunkte der Aufstellung neu berechnet",
                Ok(false) => "keine gültige Stationierung, Neupunkte nicht neu berechnet",
                Err(_) => "Neuberechnung der Neupunkte fehlgeschlagen",
            });
            mp::protokolliere(&self.einstellungen, Eintrag::neu(Ereignis::Ih).mit_aufstellung(&self.station).aenderung("IH_m", &f(alt, 3), &f(wert, 3)).quelle(mp::QUELLE_HAND).text(text));
        }
        self.station_version += 1;
        neu_berechnet.map(|_| ()).map_err(|e| crate::daten::csv_basis::fehler(format!("Instrumentenhöhe gespeichert, aber Neupunkte nicht neu berechnet: {e}")))
    }

    /// Wurde die Stationshöhe aus den Anschlusszielen abgeleitet (und hängt damit von der Instrumentenhöhe ab)?
    fn stationshoehe_aus_zielen(&self) -> bool {
        let Some(e) = &self.station.ergebnis else {
            return false;
        };
        if !e.berechnung_3d || e.hoehe == 0.0 || e.ist_lokales_system {
            return false;
        }
        // Bekannter Standpunkt mit fester Höhe: Höhe nicht aus Zielen
        !self.station.bekannt.as_ref().is_some_and(|b| b.hoehe.is_finite() && b.hoehe != 0.0)
    }

    /// Text für die Rückfrage: was eine IH-Korrektur auf `neu` bewirkt.
    pub fn ih_korrektur_wirkung(&self, neu: f64) -> String {
        let delta = neu - self.station.instrumenten_hoehe;
        let (anzahl, _) = self.neupunkte.zaehle(&self.station.standpunkt_nr);
        let neupunkte = if anzahl > 0 { format!("{anzahl} Neupunkt(e) dieser Aufstellung werden neu berechnet") } else { "Es gibt noch keine Neupunkte".to_string() };
        match self.station.ergebnis.as_ref().filter(|_| self.hat_gueltige_station()) {
            Some(e) if self.stationshoehe_aus_zielen() => format!(
                "Die Stationshöhe wurde aus den Anschlusszielen bestimmt und ändert sich von {} auf {} m. {neupunkte}; ihre Höhen bleiben gleich.",
                f(e.hoehe, 3),
                f(e.hoehe - delta, 3)
            ),
            Some(e) if e.berechnung_3d && e.hoehe != 0.0 => {
                format!("Die Stationshöhe ist fest vorgegeben ({} m). {neupunkte}; ihre Höhen ändern sich um {} m.", f(e.hoehe, 3), crate::grundlagen::format::plus_minus(delta, 3))
            }
            Some(_) => format!("Die Stationierung ist zweidimensional. {neupunkte}."),
            None => "Noch keine gültige Stationierung – es ändert sich nur die gespeicherte Instrumentenhöhe.".to_string(),
        }
    }

    /// Schreibt eine geänderte Stationshöhe in Feldbuchpunkt und Projektdaten des Standpunkts.
    fn aktualisiere_stationshoehe(&mut self, hoehe: f64) -> io::Result<()> {
        let nr = self.station.standpunkt_nr.clone();
        if let Some(mut p) = self.feldbuchpunkte.punkte().iter().find(|p| p.typ == TYP_STANDPUNKT && p.punkt_nr == nr).cloned() {
            p.hoehe = hoehe;
            self.feldbuchpunkte.add_or_update(p)?;
        }
        if self.projektdaten.get_value(KATEGORIE_STATIONIERUNG, "Standpunkt") == Some(nr.as_str()) {
            self.projektdaten.set_value(KATEGORIE_STATIONIERUNG, "Hoehe [m]", &f(hoehe, 3))?;
        }
        Ok(())
    }

    /// Mindestzahl verschiedener Anschlusspunkte der aktiven Aufstellung (bekannter Standpunkt: einer).
    pub fn min_anschlusspunkte(&self) -> usize {
        if self.station.bekannt.is_some() { 1 } else { MIN_ANSCHLUSSPUNKTE }
    }

    /// Konvergierte Stationierung mit genügend verschiedenen Anschlusspunkten.
    pub fn hat_gueltige_station(&self) -> bool {
        let verschiedene: std::collections::HashSet<&str> = self.station.messungen.iter().map(|p| p.punkt_nr.as_str()).filter(|nr| !nr.trim().is_empty()).collect();
        ist_verwendbar(self.station.ergebnis.as_ref()) && verschiedene.len() >= self.min_anschlusspunkte()
    }

    /// Ist eine Aufstellung aktiv (über „Neuer Stp“ angelegt oder fortgesetzt)?
    pub fn hat_aktive_aufstellung(&self) -> bool {
        !self.station.standpunkt_nr.trim().is_empty() && !self.station.aufstellung_id.is_empty() && !self.station_pfad.as_os_str().is_empty()
    }

    /// Beginn der aktiven Aufstellung: gespeicherter Beginn, sonst Zeitstempel der AufstellungId, sonst Datum.
    pub fn aufstellung_beginn(&self) -> Option<chrono::NaiveDateTime> {
        beginn_der_station(&self.station)
    }

    /// Hinweis zur Instrumentenhöhe (nur bei 3D-Berechnung relevant).
    pub fn ih_hinweis(&self, ih: f64) -> Option<String> {
        ih_hinweis(ih, self.rechenparameter.params.berechnung_3d)
    }

    /// Anzahl der Neupunkte der aktiven Aufstellung, die noch keine Koordinaten haben.
    pub fn offene_neupunkte(&self) -> usize {
        if self.station.aufstellung_id.is_empty() {
            return 0;
        }
        self.neupunkte.offene(&self.station.standpunkt_nr, &self.station.aufstellung_id)
    }

    /// Berechnet eine Stationierung aus Anschlussmessungen mit den Rechenparametern des Projekts.
    /// Bei bekanntem Standpunkt wird nur die Orientierung bestimmt, sonst frei stationiert.
    pub fn berechne_station(&self, punkte: &[StationierungsPunkt], ih: f64, aktivierung: &Aktivierung) -> Result<StationierungsErgebnis, String> {
        let par = &self.rechenparameter.params;
        if let Some(b) = &self.station.bekannt {
            return crate::rechnen::polar::station_bekannter_punkt(b.r, b.h, b.hoehe, punkte, ih, aktivierung, par.berechnung_3d, par.freier_massstab);
        }
        let optionen = Optionen {
            freier_massstab: par.freier_massstab,
            aktiv_richtung: Some(aktivierung.richtung.clone()),
            aktiv_strecke: Some(aktivierung.strecke.clone()),
            aktiv_hoehe: Some(aktivierung.hoehe.clone()),
            berechnung_3d: par.berechnung_3d,
            fehlergrenze_mm_hoehe: par.fehlergrenze_mm_hoehe,
            ..Optionen::default()
        };
        freie_stationierung::berechnen(punkte, ih, &optionen)
    }

    // ── Neuer Standpunkt, Fortsetzen, Umbenennen ──────────────────────────────

    /// Prüft eine Standpunktnummer: Fehler bei leerer oder ungültiger Nummer, sonst Liste von Hinweisen
    /// (Nummer ist bereits Punktnummer eines anderen Punkts).
    pub fn pruefe_standpunkt_nr(&self, nr: &str) -> Result<Vec<String>, String> {
        let nr = nr.trim();
        if nr.is_empty() {
            return Err("Bitte eine Standpunktnummer eingeben.".into());
        }
        if nr.chars().any(|c| UNGUELTIGE_ZEICHEN.contains(c) || c.is_control()) {
            return Err(format!("Die Standpunktnummer darf keines der Zeichen {UNGUELTIGE_ZEICHEN} enthalten."));
        }
        let mut hinweise = Vec::new();
        let mut quellen = Vec::new();
        if self.importpunkte.punkte().iter().any(|p| p.punkt_nr.eq_ignore_ascii_case(nr)) {
            quellen.push("Importpunkt");
        }
        if self.feldbuchpunkte.punkte().iter().any(|p| p.punkt_nr.eq_ignore_ascii_case(nr) && p.typ != TYP_STANDPUNKT) {
            quellen.push("Feldbuchpunkt");
        }
        if self.neupunkte.koordinaten().iter().any(|p| p.punkt_nr.eq_ignore_ascii_case(nr)) {
            quellen.push("Neupunkt");
        }
        if let Ok(d) = crate::daten::csv_datei::CsvDatei::laden(&self.anschlusspunkte_pfad()) {
            if d.zeilen.iter().any(|z| d.zelle(z, "PunktNr").eq_ignore_ascii_case(nr)) {
                quellen.push("Anschlusspunkt");
            }
        }
        if !quellen.is_empty() {
            hinweise.push(format!("„{nr}“ ist bereits Punktnummer ({}). Eine andere Standpunktnummer vermeidet Verwechslungen.", quellen.join(", ")));
        }
        Ok(hinweise)
    }

    /// Gibt es schon eine Stationsdatei dieser Nummer?
    pub fn standpunkt_existiert(&self, nr: &str) -> bool {
        self.einstellungen.ist_geladen() && self.station_pfad_fuer(nr.trim()).exists()
    }

    fn station_pfad_fuer(&self, nr: &str) -> PathBuf {
        self.einstellungen.get_pfad(&format!("Station-{nr}.json"))
    }

    /// Nummernvorschlag für einen neuen Standpunkt: höchstes `STPn` des Projekts + 1 (STP1 im leeren Projekt).
    /// Gezählt werden Stationsdateien, Standpunkte der Messdatei und Feldbuchpunkte vom Typ Standpunkt.
    pub fn naechste_standpunkt_nr(&self) -> String {
        let mut namen: Vec<String> = self.standpunkt_dateien().into_iter().map(|(nr, _)| nr).collect();
        namen.extend(self.feldbuchpunkte.punkte().iter().filter(|p| p.typ == TYP_STANDPUNKT).map(|p| p.punkt_nr.clone()));
        if let Ok(d) = crate::daten::csv_datei::CsvDatei::laden(&crate::daten::messdaten::messdatei_pfad(&self.einstellungen)) {
            namen.extend(d.zeilen.iter().map(|z| d.zelle(z, "StandpunktNr").to_string()));
        }
        naechste_nummer(&namen)
    }

    /// Stationsdateien des Projekts (ohne Archive): (Standpunktnummer, Pfad), sortiert.
    fn standpunkt_dateien(&self) -> Vec<(String, PathBuf)> {
        if !self.einstellungen.ist_geladen() {
            return Vec::new();
        }
        let Ok(eintraege) = fs::read_dir(&self.einstellungen.projekt_verzeichnis) else {
            return Vec::new();
        };
        let mut liste: Vec<(String, PathBuf)> = eintraege
            .filter_map(Result::ok)
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().into_owned();
                let nr = name.strip_prefix("Station-")?.strip_suffix(".json")?.to_string();
                (!nr.is_empty() && !ist_archiv_name(&nr)).then(|| (nr, e.path()))
            })
            .collect();
        liste.sort_by(|a, b| natuerlich(&a.0, &b.0));
        liste
    }

    /// Übersicht aller Standpunkte des Projekts.
    pub fn standpunkte_uebersicht(&self) -> Vec<StandpunktZeile> {
        self.standpunkt_dateien()
            .into_iter()
            .filter_map(|(nr, pfad)| {
                let d = json_speicher::lade::<StationsdatenDatei>(&pfad).ok()?;
                let verschiedene: std::collections::HashSet<&str> = d.messungen.iter().map(|p| p.punkt_nr.as_str()).collect();
                let minimum = if d.bekannt.is_some() { 1 } else { MIN_ANSCHLUSSPUNKTE };
                let beginn = beginn_der_station(&d).map(|z| z.format("%d.%m.%Y %H:%M").to_string()).unwrap_or_else(|| d.datum.clone());
                Some(StandpunktZeile {
                    aktiv: nr == self.station.standpunkt_nr && d.aufstellung_id == self.station.aufstellung_id,
                    gueltig: ist_verwendbar(d.ergebnis.as_ref()) && verschiedene.len() >= minimum,
                    s0_mm: d.ergebnis.as_ref().filter(|e| e.redundanz > 0).map(|e| e.s0_mm),
                    anschluss: verschiedene.len(),
                    instr_hoehe: d.instrumenten_hoehe,
                    beginn,
                    aufstellung_id: d.aufstellung_id,
                    nr,
                })
            })
            .collect()
    }

    /// Beginnt eine neue Aufstellung. Einziger Weg, einen Standpunkt anzulegen:
    /// bisherige Station gleicher Nummer und Anschlusspunktdatei archivieren, neue AufstellungId,
    /// Stationsdatei, Anschlusspunktdatei und Projektdaten mit derselben Id schreiben, Wechsel protokollieren.
    pub fn standpunkt_beginnen(&mut self, neu: NeuerStandpunkt) -> Result<(), String> {
        let nr = neu.nr.trim().to_string();
        self.pruefe_standpunkt_nr(&nr)?;
        if !neu.ih.is_finite() || neu.ih < 0.0 {
            return Err("Ungültige Instrumentenhöhe.".into());
        }
        if !self.einstellungen.ist_geladen() {
            return Err("Kein Projekt geladen.".into());
        }
        let pfad = self.station_pfad_fuer(&nr);
        datei_speicher::pruefe_schreibzugriff(&pfad).map_err(|e| e.to_string())?;
        if pfad.exists() {
            archiviere_station(&pfad).map_err(|e| format!("Die bisherige Station {nr} konnte nicht archiviert werden: {e}"))?;
        }
        self.archiviere_anschlusspunktdatei().map_err(|e| format!("Die Anschlusspunktdatei konnte nicht archiviert werden: {e}"))?;

        let jetzt = chrono::Local::now().naive_local();
        let station = StationsdatenDatei {
            aufstellung_id: crate::daten::messdaten::neue_aufstellung_id(&nr),
            standpunkt_nr: nr.clone(),
            instrumenten_hoehe: neu.ih,
            datum: heute_text(),
            code: neu.code.trim().to_string(),
            beginn: jetzt.format(BEGINN_FORMAT).to_string(),
            bekannt: neu.bekannt,
            geraet: neu.geraet,
            ..StationsdatenDatei::default()
        };
        let vorher = (std::mem::replace(&mut self.station, station), std::mem::replace(&mut self.station_pfad, pfad));
        if let Err(e) = self.station_speichern() {
            (self.station, self.station_pfad) = vorher;
            return Err(e.to_string());
        }
        self.schreibe_anschlusspunktdatei(&[]).map_err(|e| e.to_string())?;
        self.merke_aktive_aufstellung().map_err(|e| e.to_string())?;
        crate::daten::messdaten::schreibe_standpunktwechsel(&self.einstellungen, &self.station, crate::daten::messprotokoll::Ereignis::StpNeu);
        self.station_version += 1;
        Ok(())
    }

    /// Setzt eine vorhandene Aufstellung fort (z. B. nach einem Neustart): Station laden, ohne neue Id.
    /// Passt die Anschlusspunktdatei nicht zur Aufstellung, wird sie archiviert und aus der Station neu geschrieben.
    pub fn standpunkt_fortsetzen(&mut self, nr: &str) -> Result<(), String> {
        let nr = nr.trim();
        if !self.standpunkt_existiert(nr) {
            return Err(format!("Zu {nr} gibt es keine gespeicherte Station."));
        }
        self.station_laden(nr);
        if self.station.beginn.is_empty() {
            if let Some(b) = self.aufstellung_beginn() {
                self.station.beginn = b.format(BEGINN_FORMAT).to_string();
            }
        }
        self.station_speichern().map_err(|e| e.to_string())?;
        let passt = crate::daten::csv_datei::CsvDatei::laden(&self.anschlusspunkte_pfad())
            .ok()
            .is_some_and(|d| d.meta("AufstellungId") == Some(self.station.aufstellung_id.as_str()) && d.standpunkt_nr == self.station.standpunkt_nr);
        if !passt {
            self.archiviere_anschlusspunktdatei().map_err(|e| e.to_string())?;
            let messungen = self.station.messungen.clone();
            self.schreibe_anschlusspunktdatei(&messungen).map_err(|e| e.to_string())?;
        }
        self.merke_aktive_aufstellung().map_err(|e| e.to_string())?;
        crate::daten::messdaten::schreibe_standpunktwechsel(&self.einstellungen, &self.station, crate::daten::messprotokoll::Ereignis::StpFort);
        self.station_version += 1;
        Ok(())
    }

    /// Vermerkt die aktive Aufstellung in den Projektdaten des Projekts.
    fn merke_aktive_aufstellung(&mut self) -> io::Result<()> {
        self.projektdaten.set_values(
            KATEGORIE_AKTIV,
            &[
                ("Standpunkt", self.station.standpunkt_nr.clone()),
                ("AufstellungId", self.station.aufstellung_id.clone()),
                ("Instrumentenhoehe [m]", f(self.station.instrumenten_hoehe, 3)),
            ],
        )
    }

    /// Benennt die bisherige Anschlusspunktdatei mit Zeitstempel um (wenn vorhanden).
    fn archiviere_anschlusspunktdatei(&self) -> io::Result<()> {
        let pfad = self.anschlusspunkte_pfad();
        if !pfad.exists() {
            return Ok(());
        }
        let zeitstempel = chrono::Local::now().format("%Y-%m-%d_%H-%M-%S");
        let name = if self.einstellungen.projekt_name.is_empty() { "Anschlusspunkte".to_string() } else { format!("{}-Anschlusspunkte", self.einstellungen.projekt_name) };
        fs::rename(&pfad, pfad.with_file_name(format!("{name}_{zeitstempel}-stp.csv")))
    }

    /// Schreibt die Anschlusspunktdatei der aktiven Aufstellung (Kopf mit Nummer, Höhe, Id und Code).
    pub fn schreibe_anschlusspunktdatei(&self, punkte: &[StationierungsPunkt]) -> io::Result<()> {
        let mut d = crate::daten::stationierung_csv::datei(&self.station.standpunkt_nr, self.station.instrumenten_hoehe, punkte);
        d.projekt = self.einstellungen.projekt_name.clone();
        d.standpunkt_code = self.station.code.clone();
        d.set_meta("AufstellungId", &self.station.aufstellung_id);
        d.pfad = self.anschlusspunkte_zielpfad();
        d.speichern()
    }

    /// Benennt einen Standpunkt um bzw. führt ihn mit einem vorhandenen Standpunkt derselben Aufstellung zusammen.
    /// Geändert werden Stationsdatei, Messdatei, Neupunkte, Feldbuchpunkt, Anschlusspunktdatei und Projektdaten;
    /// vorher werden alle betroffenen Dateien in `Sicherung/` kopiert. Liefert einen Bericht.
    pub fn standpunkt_umbenennen(&mut self, alt: &str, neu: &str) -> Result<String, String> {
        let (alt, neu) = (alt.trim().to_string(), neu.trim().to_string());
        self.pruefe_standpunkt_nr(&neu)?;
        if alt == neu {
            return Err("Alte und neue Nummer sind gleich.".into());
        }
        let alt_pfad = self.station_pfad_fuer(&alt);
        let neu_pfad = self.station_pfad_fuer(&neu);
        let alt_station = if alt_pfad.exists() { Some(json_speicher::lade::<StationsdatenDatei>(&alt_pfad).map_err(|e| e.to_string())?) } else { None };
        let neu_station = if neu_pfad.exists() { Some(json_speicher::lade::<StationsdatenDatei>(&neu_pfad).map_err(|e| e.to_string())?) } else { None };
        let zusammenfuehren = neu_station.is_some();
        if let (Some(a), Some(n)) = (&alt_station, &neu_station) {
            if a.aufstellung_id != n.aufstellung_id {
                return Err(format!(
                    "{neu} existiert bereits mit einer anderen Aufstellung ({}).\nZusammenführen ist nur bei gleicher Aufstellung möglich.",
                    n.aufstellung_id
                ));
            }
        }
        let messdatei = crate::daten::messdaten::messdatei_pfad(&self.einstellungen);
        let neupunkt_datei = self.einstellungen.get_pfad(&format!("{}-Neupunkte.json", self.einstellungen.projekt_name));
        let sicherung = self.sichere_dateien(
            &format!("Umbenennen-{alt}-{neu}"),
            &[&alt_pfad, &neu_pfad, &messdatei, &neupunkt_datei, &self.einstellungen.get_pfad("Feldbuchpunkte.json"), &self.einstellungen.get_pfad("Projektdaten.csv"), &self.anschlusspunkte_pfad()],
        )?;
        let mut bericht = vec![format!("Sicherung: {}", sicherung.display())];

        // Stationsdatei
        match (alt_station, zusammenfuehren) {
            (Some(_), true) => {
                fs::remove_file(&alt_pfad).map_err(|e| e.to_string())?;
                bericht.push(format!("Station-{alt}.json entfernt (gleiche Aufstellung wie {neu})."));
            }
            (Some(mut a), false) => {
                a.standpunkt_nr.clone_from(&neu);
                json_speicher::speichere(&neu_pfad, &a, "Projekt.standpunkt_umbenennen").map_err(|e| e.to_string())?;
                fs::remove_file(&alt_pfad).map_err(|e| e.to_string())?;
                bericht.push(format!("Station-{alt}.json → Station-{neu}.json"));
            }
            (None, _) => {}
        }

        // Messdatei
        if let Ok(mut d) = crate::daten::csv_datei::CsvDatei::laden(&messdatei) {
            let mut zeilen = 0;
            for i in 0..d.zeilen.len() {
                if d.zelle(&d.zeilen[i], "StandpunktNr") == alt {
                    d.set_zelle(i, "StandpunktNr", neu.clone());
                    zeilen += 1;
                }
            }
            let muster = format!("SP={alt} ");
            for (_, k) in &mut d.kommentare {
                if k.contains(&muster) {
                    *k = k.replace(&muster, &format!("SP={neu} "));
                }
            }
            if d.standpunkt_nr == alt {
                d.standpunkt_nr.clone_from(&neu);
            }
            d.speichern().map_err(|e| e.to_string())?;
            bericht.push(format!("Messdatei: {zeilen} Zeile(n) umgestellt."));
        }

        // Neupunkte
        let n = self.neupunkte.benenne_standpunkt_um(&alt, &neu).map_err(|e| e.to_string())?;
        bericht.push(format!("Neupunkte: {n} Eintrag/Einträge umgestellt."));

        // Feldbuchpunkt des Standpunkts
        if let Some(mut p) = self.feldbuchpunkte.punkte().iter().find(|p| p.typ == TYP_STANDPUNKT && p.punkt_nr == alt).cloned() {
            self.feldbuchpunkte.remove(&alt, TYP_STANDPUNKT).map_err(|e| e.to_string())?;
            if !self.feldbuchpunkte.punkte().iter().any(|q| q.typ == TYP_STANDPUNKT && q.punkt_nr == neu) {
                p.punkt_nr.clone_from(&neu);
                self.feldbuchpunkte.add_or_update(p).map_err(|e| e.to_string())?;
            }
            bericht.push("Feldbuchpunkt des Standpunkts umgestellt.".into());
        }

        // Anschlusspunktdatei und aktive Aufstellung
        if let Ok(mut d) = crate::daten::csv_datei::CsvDatei::laden(&self.anschlusspunkte_pfad()) {
            if d.standpunkt_nr == alt {
                d.standpunkt_nr.clone_from(&neu);
                d.speichern().map_err(|e| e.to_string())?;
            }
        }
        if self.station.standpunkt_nr == alt {
            self.station_laden(&neu);
            self.merke_aktive_aufstellung().map_err(|e| e.to_string())?;
            if self.projektdaten.get_value(KATEGORIE_STATIONIERUNG, "Standpunkt") == Some(alt.as_str()) {
                self.projektdaten.set_value(KATEGORIE_STATIONIERUNG, "Standpunkt", &neu).map_err(|e| e.to_string())?;
            }
            bericht.push(format!("Aktive Aufstellung ist jetzt {neu}."));
        }
        protokoll::log("STANDPKT", &format!("Standpunkt {alt} {} {neu}", if zusammenfuehren { "zusammengeführt mit" } else { "umbenannt in" }));
        {
            use crate::daten::messprotokoll::{self as mp, Eintrag, Ereignis};
            let mut e = Eintrag::neu(Ereignis::StpUmbenannt).aenderung("StandpunktNr", &alt, &neu).quelle(mp::QUELLE_HAND).text(if zusammenfuehren { "zusammengeführt" } else { "umbenannt" });
            if self.station.standpunkt_nr == neu {
                e.aufstellung_id.clone_from(&self.station.aufstellung_id);
            }
            mp::protokolliere(&self.einstellungen, e);
        }
        self.station_version += 1;
        Ok(bericht.join("\n"))
    }

    /// Kopiert Dateien in `Sicherung/{Zeitstempel}-{anlass}/`; liefert den Ordner.
    fn sichere_dateien(&self, anlass: &str, dateien: &[&PathBuf]) -> Result<PathBuf, String> {
        let sicher: String = anlass.chars().map(|c| if UNGUELTIGE_ZEICHEN.contains(c) { '_' } else { c }).collect();
        let ordner = self.einstellungen.get_pfad(SICHERUNG_ORDNER).join(format!("{}-{sicher}", chrono::Local::now().format("%Y%m%d-%H%M%S")));
        fs::create_dir_all(&ordner).map_err(|e| e.to_string())?;
        for pfad in dateien.iter().filter(|p| p.exists()) {
            if let Some(name) = pfad.file_name() {
                fs::copy(pfad, ordner.join(name)).map_err(|e| format!("Sicherung von {}: {e}", pfad.display()))?;
            }
        }
        Ok(ordner)
    }

    /// Lädt die Stationsdaten eines Standpunkts (legt sie neu an, wenn nötig).
    pub fn station_laden(&mut self, standpunkt_nr: &str) {
        let pfad = self.einstellungen.get_pfad(&format!("Station-{standpunkt_nr}.json"));
        self.station_pfad = pfad.clone();
        self.station = Self::neue_station(standpunkt_nr);
        if pfad.exists() {
            match json_speicher::lade::<StationsdatenDatei>(&pfad) {
                Ok(mut daten) => {
                    datei_speicher::laden_erfolgreich(&pfad);
                    daten.standpunkt_nr = standpunkt_nr.to_string();
                    if daten.aufstellung_id.is_empty() {
                        daten.aufstellung_id = self.station.aufstellung_id.clone();
                    }
                    self.station = daten;
                }
                Err(fehler) => {
                    datei_speicher::melde_ladefehler(&pfad, &fehler);
                    self.station = Self::neue_station(standpunkt_nr);
                }
            }
        } else {
            datei_speicher::laden_erfolgreich(&pfad);
        }
    }

    /// Lädt die aktive Aufstellung des Projekts aus den Projektdaten
    /// (ältere Projekte: zuletzt übernommene Freie Stationierung).
    pub fn lade_aktuellen_standpunkt(&mut self) {
        self.station_zuruecksetzen();
        let nr = self
            .projektdaten
            .get_value(KATEGORIE_AKTIV, "Standpunkt")
            .or_else(|| self.projektdaten.get_value(KATEGORIE_STATIONIERUNG, "Standpunkt"))
            .map(str::to_string);
        if let Some(nr) = nr.filter(|n| !n.is_empty() && self.standpunkt_existiert(n)) {
            self.station_laden(&nr);
            if self.station.instrumenten_hoehe == 0.0 {
                if let Some(ih) = self.projektdaten.get_value(KATEGORIE_STATIONIERUNG, "Instrumentenhoehe [m]").and_then(parse_zahl) {
                    self.station.instrumenten_hoehe = ih;
                }
            }
        }
    }

    pub fn station_zuruecksetzen(&mut self) {
        self.station = StationsdatenDatei::default();
        self.station_pfad = PathBuf::new();
    }

    /// Liest die Stationsdatei eines Standpunkts, ohne den aktiven Stand zu verändern.
    pub fn lese_station_datei(&self, standpunkt_nr: &str) -> Option<StationsdatenDatei> {
        if !self.einstellungen.ist_geladen() || standpunkt_nr.trim().is_empty() {
            return None;
        }
        let pfad = self.einstellungen.get_pfad(&format!("Station-{standpunkt_nr}.json"));
        if !pfad.exists() {
            return None;
        }
        match json_speicher::lade::<StationsdatenDatei>(&pfad) {
            Ok(mut daten) => {
                datei_speicher::laden_erfolgreich(&pfad);
                daten.standpunkt_nr = standpunkt_nr.to_string();
                Some(daten)
            }
            Err(fehler) => {
                datei_speicher::melde_ladefehler(&pfad, &fehler);
                None
            }
        }
    }

    /// Fügt eine Messung hinzu und berechnet die Stationierung neu.
    /// Liefert das neue Ergebnis oder `None`, wenn noch zu wenige Messungen vorliegen.
    #[cfg(test)]
    pub fn hinzufuegen_und_berechnen(&mut self, punkt: StationierungsPunkt) -> Result<Option<StationierungsErgebnis>, String> {
        datei_speicher::pruefe_schreibzugriff(&self.station_pfad).map_err(|e| e.to_string())?;
        // Die Auswahl wird vor dem Löschen des Ergebnisses gesichert (alte Dateien: Ableitung aus den Residuen)
        self.station.gleiche_aktivierung_an();
        self.station.ergebnis = None;
        self.station.fuege_messung_hinzu(punkt);
        let ergebnis = self.neuberechnen()?;
        if ergebnis.is_some() {
            self.neupunkte_nach_stationierung();
        }
        Ok(ergebnis)
    }

    /// Berechnet die Neupunkte des aktiven Standpunkts mit der neuen Stationierung neu und speichert sie.
    fn neupunkte_nach_stationierung(&mut self) {
        if let Err(e) = self.neupunkte_neu_berechnen() {
            fehler_log::log("Projekt.neupunkte_nach_stationierung", &format!("Neuberechnung der Neupunkte: {e}"));
        }
    }

    /// Berechnet die Neupunkte der aktiven Aufstellung neu. `Ok(false)` ohne gültige Stationierung.
    fn neupunkte_neu_berechnen(&mut self) -> Result<bool, String> {
        if !self.hat_gueltige_station() {
            return Ok(false);
        }
        let Some(neu) = self.station.ergebnis.clone() else {
            return Ok(false);
        };
        let standpunkt_nr = self.station.standpunkt_nr.clone();
        let params = self.rechenparameter.params.clone();
        let ergebnis = self.neupunkte.berechne_neu(&neu, &standpunkt_nr, &params, self.station.instrumenten_hoehe, &self.station.aufstellung_id);
        self.station_version += 1;
        ergebnis.map(|_| true).map_err(|e| e.to_string())
    }

    /// Vergleicht ein noch nicht übernommenes Stationierungsergebnis mit der bisherigen Stationierung des Standpunkts.
    /// `None`, wenn es keine brauchbare bisherige Stationierung gibt (erste Stationierung).
    pub fn aenderung_vorschau(&self, standpunkt_nr: &str, neu: &StationierungsErgebnis) -> Option<StationierungsAenderung> {
        let alt = self.alte_stationierung(standpunkt_nr).filter(|a| !a.ist_lokales_system && pruefe_station(a).is_ok())?;
        let (neupunkte, davon_polarpunkte) = self.neupunkte.zaehle(standpunkt_nr);
        Some(StationierungsAenderung { standpunkt_nr: standpunkt_nr.to_string(), alt, neu: neu.clone(), neupunkte, davon_polarpunkte })
    }

    /// Bisheriges Stationierungsergebnis des Standpunkts (aktiver Stand oder gespeicherte Stationsdatei).
    fn alte_stationierung(&self, standpunkt_nr: &str) -> Option<StationierungsErgebnis> {
        if self.station.standpunkt_nr == standpunkt_nr { self.station.ergebnis.clone() } else { self.lese_station_datei(standpunkt_nr).and_then(|d| d.ergebnis) }
    }

    /// Entfernt einen Anschlusspunkt aus dem Stack und berechnet neu.
    #[cfg(test)]
    pub fn entfernen_und_berechnen(&mut self, punkt_nr: &str) -> Result<Option<StationierungsErgebnis>, String> {
        datei_speicher::pruefe_schreibzugriff(&self.station_pfad).map_err(|e| e.to_string())?;
        self.station.entferne_messung(punkt_nr);
        self.station.ergebnis = None;
        if self.station.messungen.len() < self.min_anschlusspunkte() {
            self.station_speichern().map_err(|e| e.to_string())?;
            return Ok(None);
        }
        let ergebnis = self.neuberechnen()?;
        if ergebnis.is_some() {
            self.neupunkte_nach_stationierung();
        }
        Ok(ergebnis)
    }

    #[cfg(test)]
    fn neuberechnen(&mut self) -> Result<Option<StationierungsErgebnis>, String> {
        if self.station.messungen.len() < self.min_anschlusspunkte() {
            self.station_speichern().map_err(|e| e.to_string())?;
            return Ok(None);
        }
        // Die gespeicherte Beobachtungsauswahl gilt auch für spätere Neuberechnungen; passt sie nicht, wird sie angeglichen
        if self.station.gleiche_aktivierung_an() {
            fehler_log::log("Projekt.neuberechnen", "Aktivierungslisten passten nicht zur Zahl der Messungen: angeglichen (neue Beobachtungen aktiv).");
        }
        let aktivierung = self.station.aktivierung();
        let ergebnis = self.berechne_station(&self.station.messungen, self.station.instrumenten_hoehe, &aktivierung)?;
        self.station.ergebnis = Some(ergebnis.clone());
        self.station_speichern().map_err(|e| e.to_string())?;
        if !self.hat_gueltige_station() {
            return Ok(Some(ergebnis));
        }

        // Projektdaten synchronisieren
        let ih = self.station.instrumenten_hoehe;
        self.projektdaten
            .set_values(
                KATEGORIE_STATIONIERUNG,
                &[
                    ("Standpunkt", self.station.standpunkt_nr.clone()),
                    ("R [m]", f(ergebnis.r, 3)),
                    ("H [m]", f(ergebnis.h, 3)),
                    ("Hoehe [m]", f(ergebnis.hoehe, 3)),
                    ("Orientierung [gon]", f(ergebnis.orientierung_gon, 4)),
                    ("s0 [mm]", f(ergebnis.s0_mm, 2)),
                    ("Massstab", f(ergebnis.massstab, 6)),
                    ("Instrumentenhoehe [m]", f(ih, 3)),
                ],
            )
            .map_err(|e| e.to_string())?;
        self.schreibe_sigma_projektdaten(ergebnis.s0_mm).map_err(|e| e.to_string())?;

        // Feldbuchpunkte synchronisieren
        self.feldbuchpunkte
            .add_or_update(FeldbuchPunkt {
                punkt_nr: self.station.standpunkt_nr.clone(),
                typ: TYP_STANDPUNKT.to_string(),
                r: ergebnis.r,
                h: ergebnis.h,
                hoehe: ergebnis.hoehe,
                orientierung_gon: ergebnis.orientierung_gon,
                ist_berechnung_3d: ergebnis.berechnung_3d,
                datum: heute_text(),
                quelle: QUELLE_LIVE.to_string(),
            })
            .map_err(|e| e.to_string())?;
        Ok(Some(ergebnis))
    }

    /// Schreibt die Standardabweichung (mit Faktor) in die Projektdaten.
    pub fn schreibe_sigma_projektdaten(&mut self, s0_mm: f64) -> io::Result<()> {
        let p = &self.rechenparameter.params;
        let text = format!("{}σ = {} mm", p.sigma_faktor_wirksam(), f(p.sigma_wert(s0_mm), 2));
        self.projektdaten.set_value(KATEGORIE_STATIONIERUNG, "Standardabweichung", &text)
    }

    /// Übernimmt das Ergebnis einer manuellen Stationierung samt Beobachtungsauswahl in die **aktive** Aufstellung.
    /// Gehört die Berechnung zu einer anderen Standpunktnummer, wird abgelehnt (keine stille Neuanlage).
    pub fn set_ergebnis_manuell(
        &mut self,
        standpunkt_nr: &str,
        instr_hoehe: f64,
        punkte: Vec<StationierungsPunkt>,
        aktivierung: Aktivierung,
        ergebnis: StationierungsErgebnis,
    ) -> Result<(), String> {
        pruefe_station(&ergebnis)?;
        if !self.hat_aktive_aufstellung() {
            return Err("Kein Standpunkt aktiv – bitte zuerst „Neuer Stp“.".into());
        }
        if self.station.standpunkt_nr != standpunkt_nr {
            return Err(format!(
                "Die Berechnung gehört zu {standpunkt_nr}, aktiv ist {}. Bitte die Aufstellung über „Neuer Stp“ wählen.",
                self.station.standpunkt_nr
            ));
        }
        datei_speicher::pruefe_schreibzugriff(&self.station_pfad).map_err(|e| e.to_string())?;
        self.station.instrumenten_hoehe = instr_hoehe;
        self.station.messungen = punkte;
        self.station.aktiv_richtung = aktivierung.richtung;
        self.station.aktiv_strecke = aktivierung.strecke;
        self.station.aktiv_hoehe = aktivierung.hoehe;
        self.station.gleiche_aktivierung_an();
        self.station.ergebnis = Some(ergebnis);
        self.station.datum = heute_text();
        self.station_speichern().map_err(|e| e.to_string())
    }

    /// Übernimmt eine manuell berechnete Stationierung vollständig: Stationsdatei, Projektdaten, Feldbuchpunkt.
    pub fn uebernehme_manuelle_stationierung(
        &mut self,
        standpunkt_nr: &str,
        instr_hoehe: f64,
        punkte: Vec<StationierungsPunkt>,
        aktivierung: Aktivierung,
        ergebnis: StationierungsErgebnis,
    ) -> Result<(), String> {
        self.set_ergebnis_manuell(standpunkt_nr, instr_hoehe, punkte, aktivierung, ergebnis.clone())?;
        self.feldbuchpunkte
            .add_or_update(FeldbuchPunkt {
                punkt_nr: standpunkt_nr.to_string(),
                typ: TYP_STANDPUNKT.to_string(),
                r: ergebnis.r,
                h: ergebnis.h,
                hoehe: ergebnis.hoehe,
                orientierung_gon: ergebnis.orientierung_gon,
                ist_berechnung_3d: ergebnis.berechnung_3d,
                datum: heute_text(),
                quelle: QUELLE_MANUELL.to_string(),
            })
            .map_err(|e| e.to_string())?;
        self.projektdaten
            .set_values(
                KATEGORIE_STATIONIERUNG,
                &[
                    ("Standpunkt", standpunkt_nr.to_string()),
                    ("R [m]", f(ergebnis.r, 3)),
                    ("H [m]", f(ergebnis.h, 3)),
                    ("Hoehe [m]", f(ergebnis.hoehe, 3)),
                    ("Orientierung [gon]", f(ergebnis.orientierung_gon, 4)),
                    ("s0 [mm]", f(ergebnis.s0_mm, 2)),
                    ("Instrumentenhoehe [m]", f(instr_hoehe, 3)),
                ],
            )
            .map_err(|e| e.to_string())?;
        self.merke_aktive_aufstellung().map_err(|e| e.to_string())?;
        self.schreibe_sigma_projektdaten(ergebnis.s0_mm).map_err(|e| e.to_string())?;
        let p = &self.rechenparameter.params;
        crate::daten::messdaten::schreibe_standpunktinfo(&self.einstellungen, standpunkt_nr, instr_hoehe, &ergebnis, p.sigma_faktor_wirksam(), p.sigma_wert(ergebnis.s0_mm));
        crate::daten::messprotokoll::protokolliere(&self.einstellungen, crate::daten::messprotokoll::stationierung_eintrag(&self.station, &ergebnis).quelle(crate::daten::messprotokoll::QUELLE_PROGRAMM));
        let art = if self.station.bekannt.is_some() { "Stationierung (bekannter Standpunkt)" } else { "Freie Stationierung" };
        protokoll::log("RECHNUNG", &format!("{art} übernommen: {standpunkt_nr}, s0 = {} mm", f(ergebnis.s0_mm, 2)));
        if let Some(g) = &self.station.geraet {
            protokoll::log("RECHNUNG", &format!("Gerätezustand der Aufstellung: {}", g.text()));
        }
        self.neupunkte_nach_stationierung();
        Ok(())
    }

    /// Speichert die aktive Station.
    pub fn station_speichern(&self) -> io::Result<()> {
        if self.station_pfad.as_os_str().is_empty() {
            return Ok(());
        }
        json_speicher::speichere(&self.station_pfad, &self.station, "StationsdatenManager.speichern").inspect_err(|e| fehler_log::log("Projekt.station_speichern", &e.to_string()))
    }
}

/// Untere und obere Grenze einer üblichen Instrumentenhöhe auf dem Stativ [m].
const IH_UEBLICH_MIN: f64 = 0.8;
const IH_UEBLICH_MAX: f64 = 2.2;

/// Hinweis zur Instrumentenhöhe bei 3D-Berechnung: 0 oder außerhalb des üblichen Stativbereichs.
pub fn ih_hinweis(ih: f64, berechnung_3d: bool) -> Option<String> {
    if !berechnung_3d {
        return None;
    }
    if ih.abs() < IH_TOLERANZ_M {
        return Some("Instrumentenhöhe 0.000 m bei 3D-Berechnung – die Höhen der Neupunkte werden um die Instrumentenhöhe falsch.".into());
    }
    (!(IH_UEBLICH_MIN..=IH_UEBLICH_MAX).contains(&ih)).then(|| {
        format!("Instrumentenhöhe {} m ist ungewöhnlich (Stativ üblich {}–{} m) – bitte prüfen.", f(ih, 3), f(IH_UEBLICH_MIN, 1), f(IH_UEBLICH_MAX, 1))
    })
}

/// Beginn einer Aufstellung: gespeicherter Beginn, sonst Zeitstempel der AufstellungId, sonst Datum (00:00).
pub fn beginn_der_station(s: &StationsdatenDatei) -> Option<chrono::NaiveDateTime> {
    if let Ok(z) = chrono::NaiveDateTime::parse_from_str(&s.beginn, BEGINN_FORMAT) {
        return Some(z);
    }
    if let Some(z) = zeit_aus_aufstellung_id(&s.aufstellung_id) {
        return Some(z);
    }
    chrono::NaiveDate::parse_from_str(&s.datum, "%Y-%m-%d").ok().and_then(|d| d.and_hms_opt(0, 0, 0))
}

/// Zeitstempel aus einer AufstellungId `{Nr}-{yyyyMMddTHHmmss…}`.
fn zeit_aus_aufstellung_id(id: &str) -> Option<chrono::NaiveDateTime> {
    let teil = id.rsplit('-').next()?;
    chrono::NaiveDateTime::parse_from_str(teil.get(..15)?, "%Y%m%dT%H%M%S").ok()
}

/// Archivname einer Stationsdatei (`Station-{AufstellungId}.json`): endet auf `-{yyyyMMddTHHmmss…}`.
fn ist_archiv_name(nr: &str) -> bool {
    nr.contains('-') && zeit_aus_aufstellung_id(nr).is_some()
}

/// Kopiert eine Stationsdatei unter dem Namen ihrer AufstellungId (`Station-{Id}.json`) als Archiv.
fn archiviere_station(pfad: &std::path::Path) -> io::Result<()> {
    let d = json_speicher::lade::<StationsdatenDatei>(pfad).unwrap_or_default();
    let id = if d.aufstellung_id.is_empty() { format!("{}-{}", d.standpunkt_nr, chrono::Local::now().format("%Y%m%dT%H%M%S")) } else { d.aufstellung_id };
    let sicher: String = id.chars().map(|c| if UNGUELTIGE_ZEICHEN.contains(c) { '_' } else { c }).collect();
    let archiv = pfad.with_file_name(format!("Station-{sicher}.json"));
    if !archiv.exists() {
        fs::copy(pfad, archiv)?;
    }
    Ok(())
}

/// Nächste Nummer `STP{n+1}` zu den vorhandenen Namen (nur Namen der Form STP + Zahl zählen).
fn naechste_nummer(namen: &[String]) -> String {
    let hoechste = namen
        .iter()
        .filter_map(|n| {
            let n = n.trim();
            let rest = n.get(..STANDPUNKT_PRAEFIX.len()).filter(|p| p.eq_ignore_ascii_case(STANDPUNKT_PRAEFIX)).and_then(|_| n.get(STANDPUNKT_PRAEFIX.len()..))?;
            rest.parse::<u32>().ok()
        })
        .max()
        .unwrap_or(0);
    format!("{STANDPUNKT_PRAEFIX}{}", hoechste + 1)
}

/// Natürliche Sortierung (STP2 vor STP10).
fn natuerlich(a: &str, b: &str) -> std::cmp::Ordering {
    let schluessel = |s: &str| {
        let ziffern: String = s.chars().rev().take_while(char::is_ascii_digit).collect::<Vec<_>>().into_iter().rev().collect();
        let stamm = s[..s.len() - ziffern.len()].to_lowercase();
        (stamm, ziffern.parse::<u64>().unwrap_or(0), s.to_string())
    };
    schluessel(a).cmp(&schluessel(b))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn projekt_in(name: &str) -> (Projekt, PathBuf) {
        let dir = std::env::temp_dir().join(format!("fb-proj-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let mut e = Einstellungen::mit_pfad(dir.join("Einstellungen.xml"));
        e.set_projekt("Demo", &dir).unwrap();
        let mut p = Projekt {
            einstellungen: e,
            rechenparameter: RechenparameterManager::initialisiere(&dir.join("Freie-Stationierung.xml")),
            projektdaten: ProjektdatenManager::leer(),
            feldbuchpunkte: FeldbuchpunkteManager::default(),
            neupunkte: NeupunkteManager::default(),
            importpunkte: ImportPunkteManager::default(),
            station: StationsdatenDatei::default(),
            station_pfad: PathBuf::new(),
            station_version: 0,
        };
        p.manager_initialisieren();
        (p, dir)
    }

    fn anschluss(nr: &str, r: f64, h: f64, hz: f64, strecke: f64) -> StationierungsPunkt {
        StationierungsPunkt { punkt_nr: nr.into(), r, h, hz, v: 100.0, strecke, ..Default::default() }
    }

    #[test]
    fn live_stationierung_synchronisiert_alle_dateien() {
        let (mut p, dir) = projekt_in("live");
        p.standpunkt_beginnen(NeuerStandpunkt { nr: "S1".into(), ih: 1.5, ..Default::default() }).unwrap();
        // Station bei (0,0), Orientierung 0: Ziel A im Norden, B im Osten, C im Süden
        assert!(p.hinzufuegen_und_berechnen(anschluss("A", 0.0, 100.0, 0.0, 100.0)).unwrap().is_none());
        let erg = p.hinzufuegen_und_berechnen(anschluss("B", 100.0, 0.0, 100.0, 100.0)).unwrap().unwrap();
        assert!(erg.konvergiert);
        // zwei Punkte, zwei Richtungen + zwei Strecken = 4 Beobachtungen: gültig
        assert!(p.hat_gueltige_station());
        assert!(erg.r.abs() < 1e-6 && erg.h.abs() < 1e-6);
        assert_eq!(p.feldbuchpunkte.punkte().len(), 1);
        assert_eq!(p.feldbuchpunkte.punkte()[0].typ, TYP_STANDPUNKT);
        assert_eq!(p.projektdaten.get_value("Freie Stationierung", "Standpunkt"), Some("S1"));
        assert!(p.projektdaten.get_value("Freie Stationierung", "Standardabweichung").unwrap().starts_with("1σ"));

        // Neustart: aktiver Standpunkt wird aus den Projektdaten wiederhergestellt
        p.manager_initialisieren();
        assert_eq!(p.station.standpunkt_nr, "S1");
        assert_eq!(p.station.messungen.len(), 2);
        assert!(p.hat_gueltige_station());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn weiterer_anschlusspunkt_rechnet_neupunkte_neu_und_vorschau_vergleicht() {
        let (mut p, dir) = projekt_in("neuberechnung");
        p.standpunkt_beginnen(NeuerStandpunkt { nr: "S1".into(), ih: 1.5, ..Default::default() }).unwrap();
        p.hinzufuegen_und_berechnen(anschluss("A", 0.0, 100.0, 0.0, 100.0)).unwrap();
        p.hinzufuegen_und_berechnen(anschluss("B", 100.0, 0.0, 100.0, 100.0)).unwrap();
        let version = p.station_version;
        // Ein ungenau gemessener dritter Punkt verschiebt den Standpunkt leicht
        p.hinzufuegen_und_berechnen(anschluss("C", 0.0, -100.0, 200.0, 100.05)).unwrap().unwrap();
        assert!(p.station_version > version);
        // Vorschau für eine noch nicht übernommene Stationierung: Vergleich mit der gespeicherten
        let mut neu = p.station.ergebnis.clone().unwrap();
        neu.h += 0.01;
        let vorschau = p.aenderung_vorschau("S1", &neu).expect("Vergleich mit bisheriger Stationierung");
        assert!((vorschau.neu.h - vorschau.alt.h - 0.01).abs() < 1e-9);
        assert!(p.aenderung_vorschau("ANDERER", &neu).is_none());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn aktivierung_wird_gespeichert_und_bei_neuberechnung_angewendet() {
        let (mut p, dir) = projekt_in("aktiv");
        p.standpunkt_beginnen(NeuerStandpunkt { nr: "S1".into(), ih: 1.5, ..Default::default() }).unwrap();
        let punkte = vec![
            anschluss("A", 0.0, 100.0, 0.0, 100.0),
            anschluss("B", 100.0, 0.0, 100.0, 100.0),
            anschluss("C", 0.0, -100.0, 200.0, 100.0),
            anschluss("D", -100.0, 0.0, 300.0, 100.0),
        ];
        let akt = Aktivierung { richtung: vec![true; 4], strecke: vec![true, true, false, true], hoehe: vec![true; 4] };
        let optionen = Optionen {
            aktiv_richtung: Some(akt.richtung.clone()),
            aktiv_strecke: Some(akt.strecke.clone()),
            aktiv_hoehe: Some(akt.hoehe.clone()),
            berechnung_3d: false,
            ..Optionen::default()
        };
        let erg = freie_stationierung::berechnen(&punkte, 1.5, &optionen).unwrap();
        assert!(erg.konfidenz.is_some(), "Konfidenzellipse bei n >= 3");
        p.uebernehme_manuelle_stationierung("S1", 1.5, punkte, akt.clone(), erg).unwrap();

        // Die Auswahl steht in der gespeicherten Stationsdatei
        assert_eq!(p.lese_station_datei("S1").unwrap().aktivierung(), akt);

        // Ein weiterer Punkt: die frühere Auswahl bleibt erhalten, der neue Punkt ist aktiv
        let neu = p.hinzufuegen_und_berechnen(anschluss("E", 70.7107, 70.7107, 50.0, 100.0)).unwrap().unwrap();
        assert!(!neu.residuen[2].strecke_aktiv);
        assert!(neu.residuen[4].strecke_aktiv);
        assert_eq!(p.station.aktiv_strecke, vec![true, true, false, true, true]);

        // Punkt entfernen: Auswahl wird mit entfernt
        p.entfernen_und_berechnen("A").unwrap();
        assert_eq!(p.station.aktiv_strecke, vec![true, false, true, true]);
        assert_eq!(p.station.messungen.len(), 4);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn aktivierung_alter_dateien_wird_aus_den_residuen_abgeleitet() {
        let res = |nr: &str, hz: bool, str_: bool| crate::daten::modelle::PunktResiduum { punkt_nr: nr.into(), richtung_aktiv: hz, strecke_aktiv: str_, ..Default::default() };
        let mut daten = StationsdatenDatei {
            messungen: vec![anschluss("A", 0.0, 0.0, 0.0, 1.0), anschluss("B", 1.0, 1.0, 0.0, 1.0), anschluss("C", 2.0, 2.0, 0.0, 1.0)],
            ergebnis: Some(StationierungsErgebnis { residuen: vec![res("A", true, true), res("B", true, false), res("C", false, true)], ..Default::default() }),
            ..Default::default()
        };
        let akt = daten.aktivierung();
        assert_eq!(akt.strecke, vec![true, false, true]);
        assert_eq!(akt.richtung, vec![true, true, false]);
        assert_eq!(akt.anzahl_abgeschaltet(), 2);
        // Passen die Punkte nicht zu den Residuen, bleibt alles aktiv
        daten.aktiv_richtung.clear();
        daten.aktiv_strecke.clear();
        daten.aktiv_hoehe.clear();
        daten.messungen[1].punkt_nr = "X".into();
        assert_eq!(daten.aktivierung().anzahl_abgeschaltet(), 0);
    }

    #[test]
    fn aktivierung_alter_dateien_ohne_listen_ist_komplett_aktiv() {
        let mut daten = StationsdatenDatei { messungen: vec![anschluss("A", 0.0, 0.0, 0.0, 1.0), anschluss("B", 1.0, 1.0, 0.0, 1.0)], ..Default::default() };
        assert!(!daten.gleiche_aktivierung_an());
        assert_eq!(daten.aktivierung().richtung, vec![true, true]);
        // Passt eine gespeicherte Liste nicht zur Messungszahl, wird das gemeldet und angeglichen
        daten.aktiv_strecke = vec![false];
        assert!(daten.gleiche_aktivierung_an());
        assert_eq!(daten.aktiv_strecke, vec![false, true]);
    }

    #[test]
    fn anschlusspunktsuche_reihenfolge() {
        let (mut p, dir) = projekt_in("suche");
        fs::write(p.anschlusspunkte_pfad(), "Nr,R,H,Hoehe\nCSV1,1,2,3\nBEIDE,10,20,30\n").unwrap();
        p.feldbuchpunkte.add_or_update(FeldbuchPunkt { punkt_nr: "BEIDE".into(), r: 100.0, h: 200.0, hoehe: 300.0, ..Default::default() }).unwrap();
        assert_eq!(p.finde_anschlusspunkt("csv1"), Some((1.0, 2.0, 3.0)));
        assert_eq!(p.finde_anschlusspunkt("BEIDE"), Some((100.0, 200.0, 300.0)));
        assert_eq!(p.finde_anschlusspunkt("fehlt"), None);
        assert_eq!(p.finde_anschlusspunkt("  "), None);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn festpunkte_werden_uebernommen_und_gezaehlt() {
        let (mut p, dir) = projekt_in("fest");
        let n = NeupunktErgebnis { punkt_nr: "N1".into(), r: 5.0, h: 6.0, hoehe: 7.0, ist_3d: true, ..Default::default() };
        let erg = p.festpunkte_uebernehmen(std::slice::from_ref(&n)).unwrap();
        assert_eq!(erg, UebernahmeErgebnis { neu: 1, ersetzt: 0 });
        let erg = p.festpunkte_uebernehmen(&[n]).unwrap();
        assert_eq!(erg, UebernahmeErgebnis { neu: 0, ersetzt: 1 });
        assert!(p.festpunkt_existiert("N1"));
        assert_eq!(p.feldbuchpunkte.punkte()[0].quelle, QUELLE_LOKAL);
        let _ = fs::remove_dir_all(dir);
    }

    fn neu(nr: &str, ih: f64) -> NeuerStandpunkt {
        NeuerStandpunkt { nr: nr.into(), ih, ..Default::default() }
    }

    #[test]
    fn neuer_standpunkt_schreibt_ueberall_dieselbe_aufstellung() {
        let (mut p, dir) = projekt_in("neu");
        assert!(!p.hat_aktive_aufstellung());
        assert_eq!(p.naechste_standpunkt_nr(), "STP1");
        p.standpunkt_beginnen(neu("STP1", 1.6)).unwrap();
        assert!(p.hat_aktive_aufstellung());
        let id = p.station.aufstellung_id.clone();
        assert!(id.starts_with("STP1-"));
        assert_eq!(p.projektdaten.get_value(KATEGORIE_AKTIV, "AufstellungId"), Some(id.as_str()));
        let stp = crate::daten::csv_datei::CsvDatei::laden(&p.anschlusspunkte_pfad()).unwrap();
        assert_eq!(stp.meta("AufstellungId"), Some(id.as_str()));
        assert_eq!(stp.standpunkt_nr, "STP1");
        assert!(p.aufstellung_beginn().is_some());
        // Vorschlag zählt weiter, auch ohne Zähler in den Einstellungen
        assert_eq!(p.naechste_standpunkt_nr(), "STP2");
        // Gleiche Nummer erneut: alte Station wird archiviert, neue Id, Archiv zählt nicht als Standpunkt
        std::thread::sleep(std::time::Duration::from_millis(5));
        p.standpunkt_beginnen(neu("STP1", 1.6)).unwrap();
        assert_ne!(p.station.aufstellung_id, id);
        assert!(dir.join(format!("Station-{id}.json")).exists());
        assert_eq!(p.standpunkte_uebersicht().len(), 1);
        assert_eq!(p.naechste_standpunkt_nr(), "STP2");
        // Neustart stellt die aktive Aufstellung wieder her
        let aktiv = p.station.aufstellung_id.clone();
        p.manager_initialisieren();
        assert_eq!(p.station.aufstellung_id, aktiv);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn ungueltige_nummern_und_punktkonflikt() {
        let (mut p, dir) = projekt_in("nummer");
        assert!(p.standpunkt_beginnen(neu("  ", 1.5)).is_err());
        assert!(p.standpunkt_beginnen(neu("A/B", 1.5)).is_err());
        p.feldbuchpunkte.add_or_update(FeldbuchPunkt { punkt_nr: "100".into(), r: 1.0, h: 2.0, ..Default::default() }).unwrap();
        assert_eq!(p.pruefe_standpunkt_nr("100").unwrap().len(), 1);
        assert!(p.pruefe_standpunkt_nr("STP1").unwrap().is_empty());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn standpunkt_gleicher_nummer_ist_kein_anschlusspunkt() {
        let (mut p, dir) = projekt_in("v8");
        p.feldbuchpunkte.add_or_update(FeldbuchPunkt { punkt_nr: "100".into(), typ: TYP_STANDPUNKT.into(), r: 5.0, h: 6.0, ..Default::default() }).unwrap();
        assert_eq!(p.finde_anschlusspunkt("100"), None);
        assert_eq!(p.finde_punkt_mit_standpunkten("100"), Some((5.0, 6.0, 0.0)));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn instrumentenhoehe_korrigiert_ohne_neue_aufstellung() {
        let (mut p, dir) = projekt_in("ih");
        assert!(p.set_instrumenten_hoehe(1.5).is_err(), "ohne aktive Aufstellung");
        p.standpunkt_beginnen(neu("STP1", 1.5)).unwrap();
        let id = p.station.aufstellung_id.clone();
        p.set_instrumenten_hoehe(1.65).unwrap();
        assert_eq!(p.station.aufstellung_id, id);
        assert_eq!(p.lese_station_datei("STP1").unwrap().instrumenten_hoehe, 1.65);
        let _ = fs::remove_dir_all(dir);
    }

    /// Station bei (0,0) mit drei horizontalen Zielen der Höhe 100 m; Neupunkt 7 horizontal in 50 m.
    fn ih_kette(name: &str, bekannt: Option<BekannterStandpunkt>) -> (Projekt, PathBuf) {
        let (mut p, dir) = projekt_in(name);
        p.rechenparameter.params.berechnung_3d = true;
        p.standpunkt_beginnen(NeuerStandpunkt { nr: "S1".into(), ih: 1.5, bekannt, ..Default::default() }).unwrap();
        let ap = |nr: &str, r: f64, h: f64, hz: f64| StationierungsPunkt { punkt_nr: nr.into(), r, h, hoehe: 100.0, hz, v: 100.0, strecke: 100.0, zielhoehe: 0.0 };
        let punkte = vec![ap("A", 0.0, 100.0, 0.0), ap("B", 100.0, 0.0, 100.0), ap("C", 0.0, -100.0, 200.0)];
        let akt = Aktivierung { richtung: vec![true; 3], strecke: vec![true; 3], hoehe: vec![true; 3] };
        let erg = p.berechne_station(&punkte, 1.5, &akt).unwrap();
        assert!((erg.hoehe - 98.5).abs() < 1e-9, "Stationshöhe {}", erg.hoehe);
        p.uebernehme_manuelle_stationierung("S1", 1.5, punkte, akt, erg).unwrap();
        let roh = crate::daten::modelle::NeupunktRohdaten {
            aufstellung_id: p.station.aufstellung_id.clone(),
            punkt_nr: "7".into(),
            standpunkt_nr: "S1".into(),
            hz_gon: 50.0,
            v_gon: 100.0,
            strecke_m: 50.0,
            zeitstempel: crate::daten::modelle::jetzt(),
            ..Default::default()
        };
        p.neupunkte.rohdaten_hinzufuegen(roh).unwrap();
        p.neupunkte_nach_stationierung();
        (p, dir)
    }

    fn np_hoehe(p: &Projekt) -> f64 {
        p.neupunkte.koordinaten().iter().find(|k| k.punkt_nr == "7").unwrap().hoehe
    }

    #[test]
    fn ih_korrektur_erhaelt_neupunkthoehen_bei_freier_station() {
        // Audit S3: Stationshöhe aus Zielen → sie ändert sich um −ΔIH, die Neupunkthöhe bleibt
        let (mut p, dir) = ih_kette("ih_frei", None);
        assert!((np_hoehe(&p) - 100.0).abs() < 1e-6);
        p.set_instrumenten_hoehe(1.65).unwrap();
        assert!((p.station.ergebnis.as_ref().unwrap().hoehe - 98.35).abs() < 1e-9);
        assert!((np_hoehe(&p) - 100.0).abs() < 1e-6, "NP-Höhe {}", np_hoehe(&p));
        assert!((p.lese_station_datei("S1").unwrap().ergebnis.unwrap().hoehe - 98.35).abs() < 1e-9);
        let fb = p.feldbuchpunkte.punkte().iter().find(|q| q.typ == TYP_STANDPUNKT && q.punkt_nr == "S1").unwrap().hoehe;
        assert!((fb - 98.35).abs() < 1e-9);
        // Rück- und Mehrfachkorrektur ohne kumulierte Verschiebung
        p.set_instrumenten_hoehe(1.7).unwrap();
        p.set_instrumenten_hoehe(1.5).unwrap();
        assert!((p.station.ergebnis.as_ref().unwrap().hoehe - 98.5).abs() < 1e-9);
        assert!((np_hoehe(&p) - 100.0).abs() < 1e-6);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn ih_korrektur_bei_fester_standpunkthoehe() {
        // Bekannter Standpunkt mit fester Höhe: Stationshöhe bleibt, Neupunkte ändern sich um +ΔIH
        let bekannt = BekannterStandpunkt { punkt_nr: "P0".into(), r: 0.0, h: 0.0, hoehe: 98.5 };
        let (mut p, dir) = ih_kette("ih_fest", Some(bekannt));
        assert!((np_hoehe(&p) - 100.0).abs() < 1e-6);
        assert!(p.ih_korrektur_wirkung(1.65).contains("fest vorgegeben"));
        p.set_instrumenten_hoehe(1.65).unwrap();
        assert!((p.station.ergebnis.as_ref().unwrap().hoehe - 98.5).abs() < 1e-9);
        assert!((np_hoehe(&p) - 100.15).abs() < 1e-6, "NP-Höhe {}", np_hoehe(&p));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn fortsetzen_behaelt_die_aufstellung() {
        let (mut p, dir) = projekt_in("fort");
        p.standpunkt_beginnen(neu("STP1", 1.5)).unwrap();
        let id = p.station.aufstellung_id.clone();
        p.standpunkt_beginnen(neu("STP2", 1.6)).unwrap();
        p.standpunkt_fortsetzen("STP1").unwrap();
        assert_eq!(p.station.aufstellung_id, id);
        let stp = crate::daten::csv_datei::CsvDatei::laden(&p.anschlusspunkte_pfad()).unwrap();
        assert_eq!(stp.meta("AufstellungId"), Some(id.as_str()));
        assert!(p.standpunkt_fortsetzen("FEHLT").is_err());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn uebernahme_nur_in_die_aktive_aufstellung() {
        let (mut p, dir) = projekt_in("aktivpflicht");
        let punkte = vec![anschluss("A", 0.0, 100.0, 0.0, 100.0), anschluss("B", 100.0, 0.0, 100.0, 100.0), anschluss("C", 0.0, -100.0, 200.0, 100.0)];
        let akt = Aktivierung { richtung: vec![true; 3], strecke: vec![true; 3], hoehe: vec![false; 3] };
        let erg = freie_stationierung::berechnen(&punkte, 1.5, &Optionen { berechnung_3d: false, ..Optionen::default() }).unwrap();
        assert!(p.uebernehme_manuelle_stationierung("S1", 1.5, punkte.clone(), akt.clone(), erg.clone()).is_err());
        p.standpunkt_beginnen(neu("S1", 1.5)).unwrap();
        assert!(p.uebernehme_manuelle_stationierung("S2", 1.5, punkte.clone(), akt.clone(), erg.clone()).is_err());
        p.uebernehme_manuelle_stationierung("S1", 1.5, punkte, akt, erg).unwrap();
        assert!(p.hat_gueltige_station());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn umbenennen_und_zusammenfuehren() {
        let (mut p, dir) = projekt_in("umbenennen");
        p.standpunkt_beginnen(neu("STP1", 1.5)).unwrap();
        let id = p.station.aufstellung_id.clone();
        let roh = crate::daten::modelle::NeupunktRohdaten { punkt_nr: "7".into(), standpunkt_nr: "STP1".into(), aufstellung_id: id.clone(), strecke_m: 10.0, v_gon: 100.0, ..Default::default() };
        crate::daten::messdaten::schreibe_neupunktmessung(&p.einstellungen, &roh, "STP1", 1.5).unwrap();
        p.neupunkte.rohdaten_hinzufuegen(roh).unwrap();
        let bericht = p.standpunkt_umbenennen("STP1", "STP9").unwrap();
        assert!(bericht.contains("Sicherung"));
        assert_eq!(p.station.standpunkt_nr, "STP9");
        assert_eq!(p.station.aufstellung_id, id);
        assert!(!dir.join("Station-STP1.json").exists());
        assert!(p.neupunkte.rohdaten().iter().all(|r| r.standpunkt_nr == "STP9"));
        let dat = crate::daten::csv_datei::CsvDatei::laden(&crate::daten::messdaten::messdatei_pfad(&p.einstellungen)).unwrap();
        assert!(dat.zeilen.iter().all(|z| dat.zelle(z, "StandpunktNr") == "STP9"));
        assert_eq!(p.projektdaten.get_value(KATEGORIE_AKTIV, "Standpunkt"), Some("STP9"));
        // Zusammenführen mit einer fremden Aufstellung wird abgelehnt
        p.standpunkt_beginnen(neu("STP10", 1.5)).unwrap();
        assert!(p.standpunkt_umbenennen("STP9", "STP10").is_err());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn nummernvorschlag_und_hilfen() {
        assert_eq!(naechste_nummer(&[]), "STP1");
        assert_eq!(naechste_nummer(&["STP2".into(), "stp10".into(), "SP01".into(), "STPX".into()]), "STP11");
        assert!(ist_archiv_name("STP1-20261122T101530123"));
        assert!(!ist_archiv_name("STP1"));
        assert!(!ist_archiv_name("S-1"));
        assert_eq!(natuerlich("STP2", "STP10"), std::cmp::Ordering::Less);
        assert!(ih_hinweis(0.0, true).is_some());
        assert!(ih_hinweis(3.0, true).is_some());
        assert!(ih_hinweis(1.6, true).is_none());
        assert!(ih_hinweis(0.0, false).is_none());
        let s = StationsdatenDatei { aufstellung_id: "STP1-20261122T101530123".into(), ..Default::default() };
        assert_eq!(beginn_der_station(&s).unwrap().format("%H:%M").to_string(), "10:15");
    }
}
