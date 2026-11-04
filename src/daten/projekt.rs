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
use crate::daten::modelle::{
    FeldbuchPunkt, NeupunktErgebnis, StationierungsErgebnis, StationierungsPunkt, TYP_NEUPUNKT, TYP_STANDPUNKT,
    heute_text,
};
use crate::daten::projektdaten::ProjektdatenManager;
use crate::daten::punktlisten::{FeldbuchpunkteManager, ImportPunkteManager, NeupunkteManager};
use crate::daten::rechenparameter::RechenparameterManager;
use crate::grundlagen::format::{f, parse_zahl};
use crate::grundlagen::{datei_speicher, fehler_log, pfade, protokoll};
use crate::rechnen::freie_stationierung::{self, Optionen};
use crate::rechnen::neupunkt::{ist_verwendbar, pruefe_station};

/// Kategorie der Stationierung in den Projektdaten.
const KATEGORIE_STATIONIERUNG: &str = "Freie Stationierung";
/// Quelle für live berechnete Standpunkte.
const QUELLE_LIVE: &str = "Freie Stationierung (Live)";
/// Quelle für manuell berechnete und übernommene Standpunkte.
const QUELLE_MANUELL: &str = "Freie Stationierung";
/// Quelle für übernommene Neupunkte.
pub const QUELLE_LOKAL: &str = "Lokal";
/// Mindestanzahl verschiedener Anschlusspunkte für eine gültige Station.
const MIN_ANSCHLUSSPUNKTE: usize = 2;
/// Mindestanzahl Spalten der Anschlusspunktedatei (Nr, R, H).
const MIN_SPALTEN_ANSCHLUSSDATEI: usize = 3;
/// Dateiname der Anschlusspunkte.
const ANSCHLUSSPUNKTE_DATEI: &str = "Anschlusspunkte.csv";

/// Inhalt der Datei Station-{Nr}.json.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct StationsdatenDatei {
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
}

/// Ergebnis einer Festpunktübernahme.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UebernahmeErgebnis {
    pub neu: usize,
    pub ersetzt: usize,
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
        };
        projekt.manager_initialisieren();
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
        protokoll::konfiguriere(
            self.einstellungen.protokoll_aktiv,
            self.einstellungen.ist_geladen().then(|| self.einstellungen.projekt_verzeichnis.clone()),
        );
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
        Ok(())
    }

    // ── Anschlusspunktdatei ───────────────────────────────────────────────────

    /// Pfad der Anschlusspunktedatei im Projektverzeichnis.
    pub fn anschlusspunkte_pfad(&self) -> PathBuf {
        self.einstellungen.get_pfad(ANSCHLUSSPUNKTE_DATEI)
    }

    /// Sucht Koordinaten (R, H, Höhe) zu einer Punktnummer: Importpunkte, Feldbuchpunkte, Anschlusspunkte.csv.
    pub fn finde_anschlusspunkt(&self, punkt_nr: &str) -> Option<(f64, f64, f64)> {
        if punkt_nr.trim().is_empty() {
            return None;
        }
        if let Some(p) = self.importpunkte.punkte().iter().find(|p| p.punkt_nr.eq_ignore_ascii_case(punkt_nr)) {
            return Some((p.r, p.h, p.hoehe));
        }
        if let Some(p) = self.feldbuchpunkte.punkte().iter().find(|p| p.punkt_nr.eq_ignore_ascii_case(punkt_nr)) {
            return Some((p.r, p.h, p.hoehe));
        }
        let text = fs::read_to_string(self.anschlusspunkte_pfad()).ok()?;
        for zeile in text.lines().skip(1) {
            let t: Vec<&str> = zeile.split(',').collect();
            if t.len() < MIN_SPALTEN_ANSCHLUSSDATEI || !t[0].trim().eq_ignore_ascii_case(punkt_nr) {
                continue;
            }
            let (Some(r), Some(h)) = (parse_zahl(t[1]), parse_zahl(t[2])) else {
                continue;
            };
            let hoehe = t.get(3).and_then(|x| parse_zahl(x)).unwrap_or(0.0);
            return Some((r, h, hoehe));
        }
        None
    }

    // ── Festpunktübernahme ────────────────────────────────────────────────────

    /// Gibt es unter dieser Nummer bereits einen Festpunkt vom Typ Neupunkt?
    pub fn festpunkt_existiert(&self, punkt_nr: &str) -> bool {
        self.feldbuchpunkte.punkte().iter().any(|p| p.typ == TYP_NEUPUNKT && p.punkt_nr == punkt_nr)
    }

    /// Gemessene Neupunkte, die übernommen werden können (Koordinaten vorhanden), sortiert.
    pub fn festpunkt_kandidaten(&self) -> Vec<NeupunktErgebnis> {
        let mut liste: Vec<NeupunktErgebnis> = self
            .neupunkte
            .koordinaten()
            .iter()
            .filter(|e| !e.punkt_nr.trim().is_empty() && (e.r != 0.0 || e.h != 0.0))
            .cloned()
            .collect();
        liste.sort_by(|a, b| {
            a.punkt_nr
                .to_lowercase()
                .cmp(&b.punkt_nr.to_lowercase())
                .then_with(|| a.standpunkt_nr.to_lowercase().cmp(&b.standpunkt_nr.to_lowercase()))
        });
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
        StationsdatenDatei {
            standpunkt_nr: nr.to_string(),
            datum: heute_text(),
            ..StationsdatenDatei::default()
        }
    }

    /// Instrumentenhöhe der aktiven Station [m].
    pub fn instrumenten_hoehe(&self) -> f64 {
        self.station.instrumenten_hoehe
    }

    /// Setzt die Instrumentenhöhe und speichert die Station.
    pub fn set_instrumenten_hoehe(&mut self, wert: f64) -> io::Result<()> {
        datei_speicher::pruefe_schreibzugriff(&self.station_pfad)?;
        self.station.instrumenten_hoehe = wert;
        self.station_speichern()
    }

    /// Konvergierte Stationierung mit mindestens zwei verschiedenen Anschlusspunkten.
    pub fn hat_gueltige_station(&self) -> bool {
        let verschiedene: std::collections::HashSet<&str> = self
            .station
            .messungen
            .iter()
            .map(|p| p.punkt_nr.as_str())
            .filter(|nr| !nr.trim().is_empty())
            .collect();
        ist_verwendbar(self.station.ergebnis.as_ref()) && verschiedene.len() >= MIN_ANSCHLUSSPUNKTE
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

    /// Lädt den zuletzt benutzten Standpunkt aus den Projektdaten.
    pub fn lade_aktuellen_standpunkt(&mut self) {
        self.station_zuruecksetzen();
        let nr = self.projektdaten.get_value(KATEGORIE_STATIONIERUNG, "Standpunkt").map(str::to_string);
        if let Some(nr) = nr.filter(|n| !n.is_empty()) {
            self.station_laden(&nr);
            if self.station.instrumenten_hoehe == 0.0 {
                if let Some(ih) = self
                    .projektdaten
                    .get_value(KATEGORIE_STATIONIERUNG, "Instrumentenhoehe [m]")
                    .and_then(parse_zahl)
                {
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
    pub fn hinzufuegen_und_berechnen(&mut self, punkt: StationierungsPunkt) -> Result<Option<StationierungsErgebnis>, String> {
        datei_speicher::pruefe_schreibzugriff(&self.station_pfad).map_err(|e| e.to_string())?;
        self.station.ergebnis = None;
        self.station.messungen.push(punkt);
        self.neuberechnen()
    }

    /// Entfernt einen Anschlusspunkt aus dem Stack und berechnet neu.
    pub fn entfernen_und_berechnen(&mut self, punkt_nr: &str) -> Result<Option<StationierungsErgebnis>, String> {
        datei_speicher::pruefe_schreibzugriff(&self.station_pfad).map_err(|e| e.to_string())?;
        self.station.ergebnis = None;
        self.station.messungen.retain(|p| p.punkt_nr != punkt_nr);
        if self.station.messungen.len() < MIN_ANSCHLUSSPUNKTE {
            self.station_speichern().map_err(|e| e.to_string())?;
            return Ok(None);
        }
        self.neuberechnen()
    }

    fn neuberechnen(&mut self) -> Result<Option<StationierungsErgebnis>, String> {
        if self.station.messungen.len() < MIN_ANSCHLUSSPUNKTE {
            self.station_speichern().map_err(|e| e.to_string())?;
            return Ok(None);
        }
        let par = &self.rechenparameter.params;
        let optionen = Optionen {
            freier_massstab: par.freier_massstab,
            berechnung_3d: par.berechnung_3d,
            fehlergrenze_mm_hoehe: par.fehlergrenze_mm_hoehe,
            ..Optionen::default()
        };
        let ergebnis = freie_stationierung::berechnen(&self.station.messungen, self.station.instrumenten_hoehe, &optionen)?;
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

    /// Übernimmt das Ergebnis einer manuellen Stationierung in den aktiven Stationsdatensatz.
    pub fn set_ergebnis_manuell(
        &mut self,
        standpunkt_nr: &str,
        instr_hoehe: f64,
        punkte: Vec<StationierungsPunkt>,
        ergebnis: StationierungsErgebnis,
    ) -> Result<(), String> {
        let ziel = self.einstellungen.get_pfad(&format!("Station-{standpunkt_nr}.json"));
        pruefe_station(&ergebnis)?;
        if ziel.exists() && !self.station_pfad.to_string_lossy().eq_ignore_ascii_case(&ziel.to_string_lossy()) {
            self.station_laden(standpunkt_nr);
        }
        datei_speicher::pruefe_schreibzugriff(&ziel).map_err(|e| e.to_string())?;
        self.station.standpunkt_nr = standpunkt_nr.to_string();
        self.station.instrumenten_hoehe = instr_hoehe;
        self.station.messungen = punkte;
        self.station.ergebnis = Some(ergebnis);
        self.station.datum = heute_text();
        self.station_pfad = ziel;
        self.station_speichern().map_err(|e| e.to_string())
    }

    /// Übernimmt eine manuell berechnete Stationierung vollständig: Stationsdatei, Projektdaten, Feldbuchpunkt.
    pub fn uebernehme_manuelle_stationierung(
        &mut self,
        standpunkt_nr: &str,
        instr_hoehe: f64,
        punkte: Vec<StationierungsPunkt>,
        ergebnis: StationierungsErgebnis,
    ) -> Result<(), String> {
        self.set_ergebnis_manuell(standpunkt_nr, instr_hoehe, punkte, ergebnis.clone())?;
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
        self.schreibe_sigma_projektdaten(ergebnis.s0_mm).map_err(|e| e.to_string())?;
        protokoll::log(
            "RECHNUNG",
            &format!("Freie Stationierung übernommen: {standpunkt_nr}, s0 = {} mm", f(ergebnis.s0_mm, 2)),
        );
        Ok(())
    }

    /// Beginnt einen neuen Standpunkt (leer oder aus vorhandener Datei).
    pub fn neuer_standpunkt(&mut self, standpunkt_nr: &str, instr_hoehe: f64) -> io::Result<()> {
        self.station_laden(standpunkt_nr);
        datei_speicher::pruefe_schreibzugriff(&self.station_pfad)?;
        self.station.instrumenten_hoehe = instr_hoehe;
        self.station.datum = heute_text();
        self.station_speichern()
    }

    /// Speichert die aktive Station.
    pub fn station_speichern(&self) -> io::Result<()> {
        if self.station_pfad.as_os_str().is_empty() {
            return Ok(());
        }
        json_speicher::speichere(&self.station_pfad, &self.station, "StationsdatenManager.speichern")
            .inspect_err(|e| fehler_log::log("Projekt.station_speichern", &e.to_string()))
    }
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
        p.neuer_standpunkt("S1", 1.5).unwrap();
        // Station bei (0,0), Orientierung 0: Ziel A im Norden, B im Osten, C im Süden
        assert!(p.hinzufuegen_und_berechnen(anschluss("A", 0.0, 100.0, 0.0, 100.0)).unwrap().is_none());
        let erg = p
            .hinzufuegen_und_berechnen(anschluss("B", 100.0, 0.0, 100.0, 100.0))
            .unwrap()
            .unwrap();
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
    fn anschlusspunktsuche_reihenfolge() {
        let (mut p, dir) = projekt_in("suche");
        fs::write(p.anschlusspunkte_pfad(), "Nr,R,H,Hoehe\nCSV1,1,2,3\nBEIDE,10,20,30\n").unwrap();
        p.feldbuchpunkte
            .add_or_update(FeldbuchPunkt { punkt_nr: "BEIDE".into(), r: 100.0, h: 200.0, hoehe: 300.0, ..Default::default() })
            .unwrap();
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
}
