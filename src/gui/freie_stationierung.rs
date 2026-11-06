// Zweck:    Freie Stationierung mit Handeingabe der Anschlusspunkte – Maske und Ergebnisdialog
//           nach dem Vorbild der Originalversion (FormFreieStationierung / FormErgebnis)
// Hardware: keine
// Autor:    Johann Gerner

use std::path::PathBuf;
use std::time::SystemTime;

use eframe::egui::{self, Align, Color32, Layout, RichText, Vec2};

use super::polarpunkte::{ist_gruen, wert_zelle};
use super::rechenparameter_ansicht::RechenparameterAnsicht;
use super::tabelle::Raster;
use crate::daten::modelle::{StationierungsErgebnis, StationierungsPunkt, jetzt};
use super::stationierungs_hinweis;
use crate::daten::projekt::{Aktivierung, Projekt, StationierungsAenderung};
use crate::daten::stationierung_csv;
use crate::grundlagen::format::{de, f, parse_zahl};
use crate::grundlagen::{datei_speicher, fehler_log, pfade, protokoll};
use crate::protokolle::{ausgabeverzeichnis, letztes_protokoll, oeffne, schreibe_fuer_projekt, stationierung};
use crate::rechnen::freie_stationierung::{self, Optionen};
use crate::rechnen::neupunkt::{ist_verwendbar, pruefe_station};

/// Anzahl der Tabellenzeilen = Messzeilen (Mehrfachmessungen derselben Anschlusspunkte sind möglich).
const MAX_ROWS: usize = 3000;
/// Höchstzahl verschiedener Anschlusspunkte in der Tabelle.
const MAX_ANSCHLUSSPUNKTE: usize = 40;
/// Anzahl der Eingabespalten.
const SPALTEN: usize = 8;
/// Spaltenüberschriften der Eingabetabelle.
const KOPF_EINGABE: [&str; SPALTEN] = ["PunktNr", "R", "H", "Höhe", "HZ [gon]", "V [gon]", "Strecke [m]", "Zielhöhe [m]"];
/// Kopfzeile der CSV-Dateien (Anschlusspunkte, Standpunkt).
const KOPFZEILE_CSV: &str = "PunktNr,R,H,Hoehe,HZ,V,Strecke,Zielhoehe";
/// Nachkommastellen beim Rückschreiben geladener Stationierungsdateien: R, H, Höhe.
const STELLEN_KOORDINATE: usize = 3;
/// Nachkommastellen: HZ und V [gon].
const STELLEN_WINKEL: usize = 4;
/// Nachkommastellen: Strecke und Zielhöhe [m].
const STELLEN_STRECKE: usize = 3;
/// Nachkommastellen der Instrumentenhöhe in den Metadaten.
const STELLEN_IH: usize = 3;
/// Breite der Zeilenköpfe (RowHeadersWidth = 32).
const ZEILENKOPF_BREITE: f32 = 32.0;
/// Platzbedarf des senkrechten Rollbalkens der Tabelle.
const ROLLBALKEN_BREITE: f32 = 18.0;
/// Kleinste Spaltenbreite der Eingabetabelle.
const MIN_SPALTENBREITE: f32 = 70.0;
/// Platz unterhalb der Tabelle (Auto-Match-Leiste und zwei Knopfzeilen).
const PLATZ_UNTER_TABELLE: f32 = 190.0;
/// Kleinste Höhe der Tabelle.
const MIN_TABELLENHOEHE: f32 = 160.0;

/// Instrumentenhöhe: Vorgabe, Grenzen und Schrittweite [m].
const IH_VORGABE: f64 = 1.5;
const IH_MAX: f64 = 10.0;
const IH_SCHRITT: f64 = 0.001;
const IH_STELLEN: usize = 3;

/// Dateiname der Testdaten neben der EXE.
const MUSTER_DATEI: &str = "Muster.csv";
/// Dateinamenspräfix der Berechnungsprotokolle.
const PROTOKOLL_PRAEFIX: &str = "FreieStationierung";
/// Präfix automatisch vergebener Standpunktnummern.
const STANDPUNKT_PRAEFIX: &str = "STP";

/// Farben und Maße der Originalversion.
const FARBE_AUTO_MATCH_HINTERGRUND: Color32 = Color32::from_rgb(240, 248, 255);
const FARBE_RAHMEN: Color32 = Color32::from_rgb(160, 160, 160);
const FARBE_STATUS: Color32 = Color32::from_rgb(80, 80, 80);
const FARBE_NEUER_STANDPUNKT: Color32 = Color32::from_rgb(220, 120, 20);
const FARBE_BERECHNEN: Color32 = Color32::from_rgb(70, 130, 180);
const FARBE_NEU_BERECHNEN: Color32 = Color32::from_rgb(60, 130, 60);
const FARBE_HOEHE_WARNUNG: Color32 = Color32::from_rgb(139, 0, 0);
const KNOPF_HOEHE: f32 = 36.0;
const KOPF_KNOPF_HOEHE: f32 = 30.0;
const KNOPF_BREITE_CSV: f32 = 120.0;
const KNOPF_BREITE_NEUER_STANDPUNKT: f32 = 195.0;
const KNOPF_BREITE_AUTO_MATCH_PROTOKOLL: f32 = 195.0;
const KNOPF_BREITE_RECHENPARAMETER: f32 = 230.0;
const KNOPF_BREITE_TESTDATEN: f32 = 240.0;
const KNOPF_BREITE_SPEICHERN: f32 = 200.0;
const KNOPF_BREITE_BERECHNEN: f32 = 250.0;
const KNOPF_BREITE_NEU_BERECHNEN: f32 = 160.0;
const KNOPF_BREITE_SCHLIESSEN: f32 = 126.0;
const SCHRIFT_KOPF: f32 = 15.0;
const SCHRIFT_TITEL: f32 = 20.0;
const SCHRIFT_UEBERSCHRIFT: f32 = 15.0;

/// Breite des Ergebnisdialogs und Höhe seiner Residuentabelle.
const DIALOG_BREITE: f32 = 820.0;
const DIALOG_TABELLE_HOEHE: f32 = 220.0;
/// Breite der Haken-Spalten im Ergebnisdialog.
const HAKEN_SPALTE: f32 = 38.0;
/// Füllgewichte der Residuenspalten (Punkt, Str.h, v Winkel, v Quer, v Längs, v Höhe).
const GEWICHTE_RESIDUEN: [f32; 6] = [15.0, 13.0, 14.0, 13.0, 13.0, 13.0];
/// Breite der Beschriftung vor dem Wert in den Kopfzeilen des Ergebnisdialogs.
const BESCHRIFTUNG_BREITE: usize = 25;
/// Mindestanzahl grüner Quer- und Längsresiduen für eine gültige Berechnung.
const MIN_GRUENE_RESIDUEN: usize = 3;
/// Farben der Bewertung „Übernahme freigegeben / nur nach Bestätigung / gesperrt“.
const FARBE_FREIGABE_FREI: Color32 = Color32::from_rgb(40, 150, 60);
const FARBE_FREIGABE_BESTAETIGUNG: Color32 = Color32::from_rgb(200, 130, 40);
const FARBE_FREIGABE_GESPERRT: Color32 = Color32::from_rgb(200, 60, 40);
/// Grau für Nebenhinweise.
const FARBE_HINWEIS_GRAU: Color32 = Color32::from_rgb(120, 120, 120);
/// Umrechnung rad → gon und Anteil → Prozent.
const RAD_ZU_GON: f64 = 200.0 / std::f64::consts::PI;
const PROZENT: f64 = 100.0;
/// Mindestanzahl von Zeilen für die Berechnung.
const MIN_ZEILEN: usize = 2;
/// Wiederholrate der Dateiüberwachung [s].
const ABFRAGE_SEKUNDEN: f32 = 1.0;
/// Platzhalter für nicht vorhandene Werte.
const STRICH: &str = "–";

type Zeile = [String; SPALTEN];

/// Maske der Freien Stationierung.
pub struct FreieStationierungAnsicht {
    standpunkt: String,
    instr_hoehe: f64,
    zeilen: Vec<Zeile>,
    /// Speichern ist erst nach einer gültigen Berechnung ohne spätere Änderung der Punktdaten frei.
    speichern_frei: bool,
    /// Standpunktnummer der letzten gültigen Berechnung (Dateiname des Speicherns).
    speicher_standpunkt: String,
    auto_match: bool,
    distanz_pflicht: bool,
    ergebnis_dialog: Option<ErgebnisDialog>,
    rechenparameter: Option<RechenparameterAnsicht>,
    /// Meldungsfenster (Titel, Text) wie eine MessageBox.
    meldung: Option<(String, String)>,
    /// Zeitstempel der Anschlusspunktdatei beim letzten Lesen oder Schreiben.
    anschluss_zeit: Option<SystemTime>,
    /// Berechnete, noch nicht ins Projekt übernommene Stationierung (wird erst beim Speichern übernommen).
    ausstehend: Option<Ausstehend>,
    /// Rückfrage „Stationierung übernehmen?“ (Warnung bei fehlender Kontrolle und/oder Änderungen gegenüber der bisherigen).
    bestaetigung: Option<Bestaetigung>,
}

/// Inhalt der Rückfrage vor dem Übernehmen.
#[derive(Clone)]
struct Bestaetigung {
    standpunkt: String,
    warnung: Option<String>,
    aenderung: Option<StationierungsAenderung>,
}

/// Bewertung einer berechneten Stationierung. „Berechnung möglich“ heißt noch nicht „zur Übernahme freigegeben“.
#[derive(Clone, Debug, PartialEq)]
enum Freigabe {
    /// Genug grüne Residuen: Übernahme ohne Rückfrage.
    Frei,
    /// Übernahme erst nach ausdrücklicher Bestätigung (Grund im Text).
    NurMitBestaetigung(String),
    /// Übernahme nicht möglich (Grund im Text).
    Gesperrt(String),
}

impl Freigabe {
    /// Kurzbezeichnung des Zustands.
    fn status(&self) -> &'static str {
        match self {
            Self::Frei => "freigegeben",
            Self::NurMitBestaetigung(_) => "nur nach Bestätigung",
            Self::Gesperrt(_) => "gesperrt",
        }
    }

    fn grund(&self) -> Option<&str> {
        match self {
            Self::Frei => None,
            Self::NurMitBestaetigung(g) | Self::Gesperrt(g) => Some(g),
        }
    }

    fn farbe(&self) -> Color32 {
        match self {
            Self::Frei => FARBE_FREIGABE_FREI,
            Self::NurMitBestaetigung(_) => FARBE_FREIGABE_BESTAETIGUNG,
            Self::Gesperrt(_) => FARBE_FREIGABE_GESPERRT,
        }
    }

    /// Einzeiliger Text für die Maske: Berechnung möglich, Übernahme je nach Bewertung.
    fn zeile(&self) -> String {
        match self.grund() {
            Some(grund) => format!("Berechnung möglich – Übernahme {}: {grund}", self.status()),
            None => format!("Berechnung möglich – Übernahme {}.", self.status()),
        }
    }
}

/// Berechnete Stationierung, die auf das Speichern wartet.
#[derive(Clone)]
struct Ausstehend {
    standpunkt: String,
    ih: f64,
    punkte: Vec<StationierungsPunkt>,
    aktivierung: Aktivierung,
    ergebnis: StationierungsErgebnis,
    freigabe: Freigabe,
    /// Stand der Eingaben (Tabelle, Instrumentenhöhe) bei der Berechnung; Änderungen sperren das Speichern.
    eingaben: String,
}

fn leere_zeile() -> Zeile {
    Default::default()
}

fn dateizeit(pfad: &std::path::Path) -> Option<SystemTime> {
    std::fs::metadata(pfad).and_then(|m| m.modified()).ok()
}

impl FreieStationierungAnsicht {
    /// Öffnet die Maske: Anschlusspunkte laden, Standpunktnummer und Instrumentenhöhe vorbelegen.
    pub fn neu(projekt: &Projekt) -> Self {
        let e = &projekt.einstellungen;
        let standpunkt = if !projekt.station.standpunkt_nr.is_empty() {
            projekt.station.standpunkt_nr.clone()
        } else if !e.letzte_standpunkt_nr.is_empty() {
            e.letzte_standpunkt_nr.clone()
        } else {
            format!("{STANDPUNKT_PRAEFIX}{}", e.stationierung_zaehler)
        };
        let ih = if projekt.station.instrumenten_hoehe > 0.0 { projekt.station.instrumenten_hoehe } else { e.instrumenten_hoehe };
        let mut ansicht = Self {
            standpunkt,
            instr_hoehe: if ih > 0.0 { ih.clamp(0.0, IH_MAX) } else { IH_VORGABE },
            zeilen: vec![leere_zeile(); MAX_ROWS],
            speichern_frei: false,
            speicher_standpunkt: String::new(),
            auto_match: false,
            distanz_pflicht: true,
            ergebnis_dialog: None,
            rechenparameter: None,
            meldung: None,
            anschluss_zeit: None,
            ausstehend: None,
            bestaetigung: None,
        };
        ansicht.lade_anschlusspunkte(projekt);
        ansicht
    }

    // ── Dateien ───────────────────────────────────────────────────────────────

    /// Liest die Anschlusspunkte aus der Projektdatei (vom DXF-Viewer geschrieben) in die Tabelle.
    fn lade_anschlusspunkte(&mut self, projekt: &Projekt) {
        let pfad = projekt.anschlusspunkte_pfad();
        self.anschluss_zeit = dateizeit(&pfad);
        let Ok(text) = std::fs::read_to_string(&pfad) else {
            return;
        };
        let zeilen: Vec<&str> = text.lines().skip(1).filter(|z| !z.trim().is_empty()).collect();
        if zeilen.is_empty() {
            return;
        }
        self.fuelle_zeilen(&zeilen);
    }

    /// Leert die Tabelle und füllt sie mit CSV-Zeilen (Zeilen mit weniger als 8 Spalten bleiben leer).
    fn fuelle_zeilen(&mut self, csv: &[&str]) {
        self.zeilen = vec![leere_zeile(); MAX_ROWS];
        for (i, zeile) in csv.iter().take(MAX_ROWS).enumerate() {
            let teile: Vec<&str> = zeile.split(',').collect();
            if teile.len() < SPALTEN {
                continue;
            }
            for (j, teil) in teile.iter().take(SPALTEN).enumerate() {
                self.zeilen[i][j] = teil.trim().to_string();
            }
        }
    }

    /// Prüft, ob der DXF-Viewer die Anschlusspunktdatei geändert hat, und lädt sie dann neu.
    fn beobachte_anschlusspunkte(&mut self, projekt: &Projekt) {
        let zeit = dateizeit(&projekt.anschlusspunkte_pfad());
        if zeit.is_some() && zeit != self.anschluss_zeit {
            self.lade_anschlusspunkte(projekt);
        }
    }

    /// Kopfzeile und alle belegten Tabellenzeilen als CSV-Zeilen.
    fn tabellen_csv(&self) -> Vec<String> {
        let mut csv = vec![KOPFZEILE_CSV.to_string()];
        for z in self.zeilen.iter().filter(|z| !z[0].trim().is_empty()) {
            csv.push(z.iter().map(|w| w.trim()).collect::<Vec<_>>().join(","));
        }
        csv
    }

    /// Inhalt der Standpunkt-Datei: Metadaten (Standpunktnummer, Instrumentenhöhe) und die Tabellenzeilen.
    /// Die Datenzeilen entsprechen dem Original; die Metadaten stehen in Kommentar- bzw. Schlüssel-Zeilen davor.
    fn standpunkt_datei_zeilen(&self, standpunkt: &str, ih: f64) -> Vec<String> {
        let mut zeilen = vec![
            "# METADATA".to_string(),
            format!("StandpunktNr: {}", standpunkt.trim()),
            format!("Instrumentenhoehe: {}", f(ih, STELLEN_IH)),
            "# DATEN".to_string(),
        ];
        zeilen.extend(self.tabellen_csv());
        zeilen
    }

    fn standpunkt_csv_pfad(&self, standpunkt: &str, projekt: &Projekt) -> PathBuf {
        let nr = standpunkt.trim();
        let sicher: String = if nr.is_empty() {
            "Unbekannt".to_string()
        } else {
            nr.chars().map(|c| if "\\/:*?\"<>|".contains(c) { '_' } else { c }).collect()
        };
        let name = format!("Standpunkt-{sicher}.csv");
        if projekt.einstellungen.ist_geladen() { projekt.einstellungen.get_pfad(&name) } else { pfade::get(&name) }
    }

    fn melde(&mut self, titel: &str, text: impl Into<String>) {
        self.meldung = Some((titel.to_string(), text.into()));
    }

    /// Merkt Standpunktnummer und Instrumentenhöhe für den nächsten Aufruf.
    fn merke(&self, projekt: &mut Projekt) {
        let nr = self.standpunkt.trim();
        if !nr.is_empty() {
            projekt.einstellungen.letzte_standpunkt_nr = nr.to_string();
        }
        projekt.einstellungen.instrumenten_hoehe = self.instr_hoehe;
        if let Err(fehler) = projekt.einstellungen.speichern() {
            fehler_log::log("FreieStationierung.merke", &fehler.to_string());
        }
    }

    // ── Aktionen ──────────────────────────────────────────────────────────────

    /// „Neuer Standpunkt“: Anschlusspunktdatei mit Zeitstempel archivieren und die Tabelle leeren.
    fn neuer_standpunkt(&mut self, projekt: &Projekt) {
        let pfad = projekt.anschlusspunkte_pfad();
        if pfad.exists() {
            let zeitstempel = chrono::Local::now().format("%Y-%m-%d_%H-%M-%S");
            let archiv = pfad.with_file_name(format!("Anschlusspunkte_{zeitstempel}.csv"));
            if let Err(fehler) = std::fs::rename(&pfad, &archiv) {
                self.melde("Fehler", format!("Die Anschlusspunktdatei konnte nicht archiviert werden: {fehler}"));
                return;
            }
        }
        self.zeilen = vec![leere_zeile(); MAX_ROWS];
        self.speichern_frei = false;
        self.ausstehend = None;
        self.anschluss_zeit = None;
    }

    /// Entfernt eine Zeile, schließt die Lücke und schreibt die CSV-Dateien neu.
    fn zeile_entfernen(&mut self, index: usize, projekt: &Projekt) {
        self.zeilen.remove(index);
        self.zeilen.push(leere_zeile());
        self.speichern_frei = false;
        self.ausstehend = None;
        let csv = self.tabellen_csv();
        let anschluss = projekt.anschlusspunkte_pfad();
        let standpunkt = self.standpunkt_csv_pfad(&self.standpunkt, projekt);
        let ergebnis = datei_speicher::schreibe_zeilen(&anschluss, &csv).and_then(|()| {
            if standpunkt.exists() {
                datei_speicher::schreibe_zeilen(&standpunkt, self.standpunkt_datei_zeilen(&self.standpunkt, self.instr_hoehe))
            } else {
                Ok(())
            }
        });
        self.anschluss_zeit = dateizeit(&anschluss);
        if let Err(fehler) = ergebnis {
            self.melde("Fehler", format!("Fehler beim Schreiben der CSV-Dateien: {fehler}"));
        }
    }

    fn lade_testdaten(&mut self) {
        self.lade_csv(&pfade::get(MUSTER_DATEI));
    }

    /// „CSV laden“: Dateiauswahl, dann die Zeilen (PunktNr, R, H, Höhe, HZ, V, Strecke, Zielhöhe) in die Tabelle übernehmen.
    fn waehle_csv(&mut self, projekt: &Projekt) {
        let mut dialog = rfd::FileDialog::new().set_title("CSV mit Anschlusspunkten laden").add_filter("CSV-Dateien (*.csv)", &["csv"]);
        if projekt.einstellungen.ist_geladen() {
            dialog = dialog.set_directory(&projekt.einstellungen.projekt_verzeichnis);
        }
        if let Some(pfad) = dialog.pick_file() {
            self.lade_csv(&pfad);
        }
    }

    fn lade_csv(&mut self, pfad: &std::path::Path) {
        let Ok(text) = std::fs::read_to_string(pfad) else {
            let name = pfad.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            self.melde("Datei nicht gefunden", format!("{name} nicht gefunden.\nErwartet in: {}", pfad.display()));
            return;
        };
        let beginnt_mit_metadaten = text.trim_start_matches('\u{feff}').trim_start().starts_with('#');
        if !beginnt_mit_metadaten {
            // Originalformat: Kopfzeile, danach die Punktzeilen
            let zeilen: Vec<&str> = text.lines().skip(1).take(MAX_ROWS).collect();
            self.fuelle_zeilen(&zeilen);
            self.speichern_frei = false;
            self.ausstehend = None;
            return;
        }
        match stationierung_csv::parse(&text) {
            Ok(import) => {
                if let Some(nr) = import.standpunkt.filter(|nr| !nr.is_empty()) {
                    self.standpunkt = nr;
                }
                if let Some(ih) = import.instrumentenhoehe {
                    self.instr_hoehe = ih.clamp(0.0, IH_MAX);
                }
                let zeilen: Vec<String> = import
                    .punkte
                    .iter()
                    .map(|p| {
                        [
                            p.punkt_nr.clone(),
                            f(p.r, STELLEN_KOORDINATE),
                            f(p.h, STELLEN_KOORDINATE),
                            f(p.hoehe, STELLEN_KOORDINATE),
                            f(p.hz, STELLEN_WINKEL),
                            f(p.v, STELLEN_WINKEL),
                            f(p.strecke, STELLEN_STRECKE),
                            f(p.zielhoehe, STELLEN_STRECKE),
                        ]
                        .join(",")
                    })
                    .collect();
                let refs: Vec<&str> = zeilen.iter().map(String::as_str).collect();
                self.fuelle_zeilen(&refs);
                if import.punkte.len() > MAX_ROWS {
                    self.melde("CSV laden", format!("Die Datei enthält {} Punkte, es werden nur die ersten {MAX_ROWS} übernommen.", import.punkte.len()));
                }
                self.speichern_frei = false;
                self.ausstehend = None;
            }
            Err(fehler) => self.melde("CSV laden", format!("Die Datei konnte nicht gelesen werden:\n{fehler}")),
        }
    }

    /// Speichern: eine neue Stationierung wird erst nach Rückfrage (Vergleich mit der bisherigen) übernommen.
    fn speichern(&mut self, projekt: &mut Projekt) {
        let Some(a) = self.ausstehend.as_ref() else {
            self.schreibe_csv(projekt);
            return;
        };
        let warnung = match &a.freigabe {
            Freigabe::Gesperrt(_) => return,
            Freigabe::NurMitBestaetigung(grund) => Some(format!("{grund} Die Stationierung hat keine Kontrolle durch Residuen.")),
            Freigabe::Frei => None,
        };
        let aenderung = projekt.aenderung_vorschau(&a.standpunkt, &a.ergebnis);
        if warnung.is_some() || aenderung.is_some() {
            self.bestaetigung = Some(Bestaetigung { standpunkt: a.standpunkt.clone(), warnung, aenderung });
        } else {
            self.uebernimm_stationierung(projekt);
        }
    }

    /// Übernimmt die berechnete Stationierung ins Projekt (Neupunkte werden dabei neu berechnet) und schreibt die CSV.
    fn uebernimm_stationierung(&mut self, projekt: &mut Projekt) {
        let Some(a) = self.ausstehend.clone() else {
            return;
        };
        match projekt.uebernehme_manuelle_stationierung(&a.standpunkt, a.ih, a.punkte, a.aktivierung, a.ergebnis) {
            Ok(()) => {
                self.ausstehend = None;
                self.schreibe_csv(projekt);
            }
            Err(fehler) => self.melde("Fehler", format!("Stationierung konnte nicht übernommen werden:\n{fehler}")),
        }
    }

    /// Rückfrage-Fenster: „Übernehmen“ setzt die neue Stationierung, „Abbrechen“ belässt die bisherigen Daten.
    fn zeige_bestaetigung(&mut self, ctx: &egui::Context, projekt: &mut Projekt) {
        let Some(frage) = self.bestaetigung.clone() else {
            return;
        };
        let mut antwort = None;
        egui::Window::new("Stationierung übernehmen?")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                antwort = stationierungs_hinweis::zeichne(ui, &frage.standpunkt, frage.warnung.as_deref(), frage.aenderung.as_ref());
            });
        match antwort {
            Some(true) => {
                self.bestaetigung = None;
                self.uebernimm_stationierung(projekt);
            }
            Some(false) => self.bestaetigung = None,
            None => {}
        }
    }

    fn schreibe_csv(&mut self, projekt: &Projekt) {
        let pfad = self.standpunkt_csv_pfad(&self.speicher_standpunkt, projekt);
        let anzahl = self.tabellen_csv().len() - 1;
        let zeilen = self.standpunkt_datei_zeilen(&self.speicher_standpunkt, self.instr_hoehe);
        match datei_speicher::schreibe_zeilen(&pfad, &zeilen) {
            Ok(()) => self.melde("Gespeichert", format!("Gespeichert: {anzahl} Punkte\n{}", pfad.display())),
            Err(fehler) => self.melde("Fehler", format!("Fehler beim Speichern: {fehler}")),
        }
    }

    fn oeffne_protokoll(&mut self, projekt: &Projekt) {
        match letztes_protokoll(&ausgabeverzeichnis(projekt), PROTOKOLL_PRAEFIX) {
            None => self.melde(
                "Kein Protokoll",
                "Kein Berechnungsprotokoll gefunden.\nEs wird beim Schließen des Berechnungsdialogs erzeugt.",
            ),
            Some(pfad) => {
                if let Err(fehler) = oeffne(&pfad) {
                    self.melde("Fehler", format!("Protokoll konnte nicht geöffnet werden:\n{fehler}"));
                }
            }
        }
    }

    /// Liest die Tabelle; Fehlertext mit Zeilennummer, wenn ein Wert ungültig ist.
    fn lese_punkte(&self) -> Result<Vec<StationierungsPunkt>, String> {
        let mut punkte = Vec::new();
        for (i, z) in self.zeilen.iter().enumerate() {
            if z[0].trim().is_empty() {
                continue;
            }
            let zahl = |index: usize, name: &str| {
                parse_zahl(&z[index]).ok_or_else(|| format!("Fehler in Zeile {}: {name} ungültig", i + 1))
            };
            punkte.push(StationierungsPunkt {
                punkt_nr: z[0].trim().to_string(),
                r: zahl(1, "R")?,
                h: zahl(2, "H")?,
                hoehe: zahl(3, "Höhe")?,
                hz: zahl(4, "HZ")?,
                v: zahl(5, "V")?,
                strecke: zahl(6, "Strecke")?,
                zielhoehe: zahl(7, "Zielhöhe")?,
            });
        }
        Ok(punkte)
    }

    fn berechnen(&mut self, projekt: &mut Projekt) {
        self.merke(projekt);
        let punkte = match self.lese_punkte() {
            Ok(p) => p,
            Err(text) => {
                self.melde("Eingabefehler", text);
                return;
            }
        };
        if punkte.len() < MIN_ZEILEN {
            self.melde("Zu wenig Punkte", "Mindestens 2 vollständige Zeilen erforderlich.");
            return;
        }
        let verschiedene = anzahl_verschiedene(&punkte);
        if verschiedene < MIN_ZEILEN {
            self.melde("Zu wenig Punkte", format!("Mindestens {MIN_ZEILEN} verschiedene Anschlusspunkte erforderlich (vorhanden: {verschiedene})."));
            return;
        }
        if verschiedene > MAX_ANSCHLUSSPUNKTE {
            self.melde(
                "Zu viele Anschlusspunkte",
                format!("Höchstens {MAX_ANSCHLUSSPUNKTE} verschiedene Anschlusspunkte sind erlaubt (vorhanden: {verschiedene}). Messzeilen: bis zu {MAX_ROWS}."),
            );
            return;
        }
        let standpunkt = self.standpunkt.trim().to_string();
        let vorgabe = gespeicherte_aktivierung(projekt, &standpunkt, &punkte);
        let hinweis = verworfene_auswahl(projekt, &standpunkt, vorgabe.is_some());
        match ErgebnisDialog::neu(standpunkt, self.instr_hoehe, punkte, vorgabe, projekt) {
            Ok(mut dialog) => {
                dialog.hinweis = hinweis;
                self.ergebnis_dialog = Some(dialog);
            }
            Err(fehler) => self.melde("Fehler", format!("Berechnungsfehler: {fehler}")),
        }
    }

    /// Nach dem Schließen des Ergebnisdialogs: Protokoll schreiben, Speichern freigeben, Standpunktzähler erhöhen.
    fn nach_dialog(&mut self, dialog: &ErgebnisDialog, projekt: &mut Projekt) {
        // Scheiterte die letzte Neuberechnung (nach geänderten Haken), passt das angezeigte Ergebnis nicht zur Auswahl:
        // weder Protokoll noch Übernahme
        if !dialog.gueltig {
            self.speichern_frei = false;
            self.ausstehend = None;
            self.melde(
                "Berechnung fehlgeschlagen",
                "Die letzte Berechnung mit den geänderten Haken ist fehlgeschlagen. Das angezeigte Ergebnis gehört zur vorherigen Auswahl \
                 und wird weder protokolliert noch zur Übernahme freigegeben. Bitte erneut berechnen.",
            );
            return;
        }
        if let Err(text) = dialog.schreibe_protokoll(projekt) {
            if projekt.einstellungen.protokoll_aktiv {
                self.melde("Protokoll", text);
            }
        }
        let freigabe = bewerte_freigabe(&dialog.ergebnis, anzahl_verschiedene(&dialog.punkte), projekt);
        self.speichern_frei = !matches!(freigabe, Freigabe::Gesperrt(_));
        self.speicher_standpunkt = dialog.standpunkt.clone();
        self.ausstehend = Some(Ausstehend {
            standpunkt: dialog.standpunkt.clone(),
            ih: dialog.ih,
            punkte: dialog.punkte.clone(),
            aktivierung: Aktivierung {
                richtung: dialog.akt_hz.clone(),
                strecke: dialog.akt_str.clone(),
                hoehe: dialog.akt_hoe.clone(),
            },
            ergebnis: dialog.ergebnis.clone(),
            freigabe,
            eingaben: self.eingaben_stand(),
        });

        let nr = dialog.standpunkt.as_str();
        if nr.len() > STANDPUNKT_PRAEFIX.len() && nr[..STANDPUNKT_PRAEFIX.len()].eq_ignore_ascii_case(STANDPUNKT_PRAEFIX) {
            if let Ok(n) = nr[STANDPUNKT_PRAEFIX.len()..].parse::<i32>() {
                projekt.einstellungen.stationierung_zaehler = n + 1;
                self.standpunkt = format!("{STANDPUNKT_PRAEFIX}{}", n + 1);
            }
        }
        self.merke(projekt);
    }

    // ── Darstellung ───────────────────────────────────────────────────────────

    /// Stand der Eingaben, auf denen eine Berechnung beruht: Instrumentenhöhe und alle belegten Tabellenzeilen.
    fn eingaben_stand(&self) -> String {
        format!("{:.6}\n{}", self.instr_hoehe, self.tabellen_csv().join("\n"))
    }

    /// Änderungen nach dem Berechnen (Tabelle, Instrumentenhöhe, nachgeladene Anschlusspunktdatei) sperren das Speichern.
    fn pruefe_eingaben(&mut self) {
        if self.ausstehend.as_ref().is_some_and(|a| a.eingaben != self.eingaben_stand()) {
            self.ausstehend = None;
            self.speichern_frei = false;
        }
    }

    pub fn zeichne(&mut self, ui: &mut egui::Ui, projekt: &mut Projekt) {
        let ctx = ui.ctx().clone();
        self.beobachte_anschlusspunkte(projekt);
        self.pruefe_eingaben();
        ctx.request_repaint_after(std::time::Duration::from_secs_f32(ABFRAGE_SEKUNDEN));

        let gesperrt = self.ergebnis_dialog.is_some() || self.meldung.is_some() || self.bestaetigung.is_some();
        ui.add_enabled_ui(!gesperrt, |ui| self.zeichne_maske(ui, projekt));

        if let Some(mut dialog) = self.ergebnis_dialog.take() {
            if dialog.zeige(&ctx, projekt) {
                self.nach_dialog(&dialog, projekt);
            } else {
                self.ergebnis_dialog = Some(dialog);
            }
        }
        self.zeige_rechenparameter(&ctx, projekt);
        self.zeige_bestaetigung(&ctx, projekt);
        self.zeige_meldung(&ctx);
    }

    fn zeichne_maske(&mut self, ui: &mut egui::Ui, projekt: &mut Projekt) {
        ui.heading("Freie Stationierung");
        ui.add_space(4.0);

        // ── Kopfzeile ────────────────────────────────────────────────────────
        let mut csv_laden = false;
        ui.horizontal(|ui| {
            ui.label("Standpunktnummer:");
            let nr = ui.add(egui::TextEdit::singleline(&mut self.standpunkt).desired_width(110.0));
            ui.add_space(24.0);
            ui.label("Instrumentenhöhe (m):");
            let ih = ui.add(
                egui::DragValue::new(&mut self.instr_hoehe)
                    .range(0.0..=IH_MAX)
                    .speed(IH_SCHRITT)
                    .fixed_decimals(IH_STELLEN),
            );
            if nr.lost_focus() || ih.lost_focus() || ih.drag_stopped() {
                self.merke(projekt);
            }
            ui.add_space(24.0);
            csv_laden = ui.add(egui::Button::new("CSV laden …").min_size(Vec2::new(KNOPF_BREITE_CSV, KOPF_KNOPF_HOEHE))).clicked();
        });
        if csv_laden {
            self.waehle_csv(projekt);
        }
        ui.add_space(6.0);

        self.zeichne_tabelle(ui, projekt);
        ui.add_space(8.0);
        self.zeichne_auto_match_leiste(ui, projekt);
        ui.add_space(8.0);
        self.zeichne_knoepfe(ui, projekt);
    }

    fn zeichne_tabelle(&mut self, ui: &mut egui::Ui, projekt: &Projekt) {
        let breite = ((ui.available_width() - ZEILENKOPF_BREITE - ROLLBALKEN_BREITE) / SPALTEN as f32).max(MIN_SPALTENBREITE);
        let raster = Raster::neu(&[breite; SPALTEN]).mit_zeilenkopf(ZEILENKOPF_BREITE);
        let hoehe = (ui.available_height() - PLATZ_UNTER_TABELLE).max(MIN_TABELLENHOEHE);
        let mut geaendert = false;
        let mut entfernen = None;
        egui::ScrollArea::horizontal().id_salt("fs_horizontal").auto_shrink([false, true]).show(ui, |ui| {
            ui.spacing_mut().item_spacing = Vec2::ZERO;
            raster.kopf(ui, &KOPF_EINGABE);
            // Bis zu 3000 Messzeilen: nur die sichtbaren Zeilen werden gezeichnet
            let zeilen_hoehe = raster.zeilen_hoehe();
            egui::ScrollArea::vertical().id_salt("fs_vertikal").max_height(hoehe).auto_shrink([false, true]).show_rows(ui, zeilen_hoehe, MAX_ROWS, |ui, sichtbar| {
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                for i in sichtbar {
                    let nr = self.zeilen[i][0].trim().to_string();
                    let zeile = &mut self.zeilen[i];
                    raster.zeile(ui, i, &(i + 1).to_string(), |zellen| {
                        for wert in zeile.iter_mut() {
                            let Some(antwort) = zellen.eingabe_antwort(wert) else {
                                continue;
                            };
                            geaendert |= antwort.changed();
                            if !nr.is_empty() {
                                antwort.context_menu(|ui| {
                                    if ui.button(format!("Zeile '{nr}' entfernen")).clicked() {
                                        entfernen = Some(i);
                                        ui.close();
                                    }
                                });
                            }
                        }
                    });
                }
            });
        });
        if geaendert {
            self.speichern_frei = false;
            self.ausstehend = None;
        }
        if let Some(i) = entfernen {
            self.zeile_entfernen(i, projekt);
        }
    }

    fn zeichne_auto_match_leiste(&mut self, ui: &mut egui::Ui, projekt: &Projekt) {
        let grenze = projekt.rechenparameter.params.fehlergrenze_mm_s0_auto_match;
        let ergebnis = projekt.station.ergebnis.as_ref();
        let brauchbar = ergebnis.is_some_and(|e| e.redundanz > 0 && e.s0_mm <= grenze);
        let status = match ergebnis {
            None => "Keine Stationierung vorhanden".to_string(),
            Some(e) if !brauchbar => {
                format!("Station zu ungenau (s₀={} mm > {} mm) – Auto-Match gesperrt", de(e.s0_mm, 1), de(grenze, 0))
            }
            Some(e) => format!(
                "Station: {}  r={}  – Auto-Match bereit",
                projekt.rechenparameter.params.sigma_text(e.s0_mm, 2),
                e.redundanz
            ),
        };
        if !brauchbar {
            self.auto_match = false;
        }
        let mut protokoll_wunsch = false;
        egui::Frame::new()
            .fill(FARBE_AUTO_MATCH_HINTERGRUND)
            .stroke(egui::Stroke::new(1.0, FARBE_RAHMEN))
            .inner_margin(egui::Margin::symmetric(8, 8))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    // Die Live-Punktsuche über den Tachymeter ist in Feldbuch-NT noch nicht portiert.
                    ui.add_enabled(false, egui::Checkbox::new(&mut self.auto_match, RichText::new("Auto-Punktsuche aktiv").strong()))
                        .on_disabled_hover_text("Die automatische Punktsuche ist in Feldbuch-NT noch nicht umgesetzt.");
                    ui.add_space(12.0);
                    ui.checkbox(&mut self.distanz_pflicht, "Distanz-Pflicht");
                    ui.add_space(12.0);
                    ui.label(RichText::new(status).color(FARBE_STATUS));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.button("Berechnungsprotokoll").clicked() {
                            protokoll_wunsch = true;
                        }
                    });
                });
            });
        if protokoll_wunsch {
            self.oeffne_protokoll(projekt);
        }
    }

    fn zeichne_knoepfe(&mut self, ui: &mut egui::Ui, projekt: &mut Projekt) {
        let mut neuer_standpunkt = false;
        let mut testdaten = false;
        let mut speichern = false;
        let mut berechnen = false;

        // Zeile 1: [Neuer Standpunkt] [Auto-Match-Protokoll] … [Rechenparameter]
        ui.horizontal(|ui| {
            if projekt.anschlusspunkte_pfad().exists() {
                let knopf = egui::Button::new(RichText::new("Neuer Standpunkt").strong().size(SCHRIFT_KOPF).color(Color32::WHITE))
                    .fill(FARBE_NEUER_STANDPUNKT)
                    .stroke(egui::Stroke::new(1.0, Color32::from_rgb(180, 90, 10)))
                    .min_size(Vec2::new(KNOPF_BREITE_NEUER_STANDPUNKT, KNOPF_HOEHE));
                neuer_standpunkt = ui.add(knopf).clicked();
            }
            ui.add_enabled(false, egui::Button::new("Auto-Match-Protokoll").min_size(Vec2::new(KNOPF_BREITE_AUTO_MATCH_PROTOKOLL, KNOPF_HOEHE)))
                .on_disabled_hover_text("Die automatische Punktsuche ist in Feldbuch-NT noch nicht umgesetzt.");
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let knopf = egui::Button::new("⚙ Rechenparameter").min_size(Vec2::new(KNOPF_BREITE_RECHENPARAMETER, KNOPF_HOEHE));
                if ui.add(knopf).clicked() {
                    self.rechenparameter = Some(RechenparameterAnsicht::default());
                }
            });
        });
        ui.add_space(4.0);

        // Bewertung der letzten Berechnung: „Berechnung möglich“ und „Übernahme freigegeben“ getrennt benennen
        if let Some(a) = &self.ausstehend {
            ui.label(RichText::new(a.freigabe.zeile()).color(a.freigabe.farbe()));
            ui.add_space(2.0);
        }

        // Zeile 2: [Testdaten laden] … [Speichern] [Berechnen]
        ui.horizontal(|ui| {
            let knopf = egui::Button::new("Testdaten laden (Muster.csv)").min_size(Vec2::new(KNOPF_BREITE_TESTDATEN, KNOPF_HOEHE));
            testdaten = ui.add(knopf).clicked();
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let knopf = egui::Button::new(RichText::new("Berechnen").strong().size(SCHRIFT_KOPF).color(Color32::WHITE))
                    .fill(FARBE_BERECHNEN)
                    .min_size(Vec2::new(KNOPF_BREITE_BERECHNEN, KNOPF_HOEHE));
                berechnen = ui.add(knopf).clicked();
                let knopf = egui::Button::new("Speichern").min_size(Vec2::new(KNOPF_BREITE_SPEICHERN, KNOPF_HOEHE));
                let antwort = ui.add_enabled(self.speichern_frei, knopf);
                let antwort = match self.ausstehend.as_ref().and_then(|a| a.freigabe.grund()) {
                    Some(grund) => antwort.on_hover_text(grund).on_disabled_hover_text(grund),
                    None => antwort,
                };
                speichern = antwort.clicked();
            });
        });

        if neuer_standpunkt {
            self.neuer_standpunkt(projekt);
        }
        if testdaten {
            self.lade_testdaten();
        }
        if speichern {
            self.speichern(projekt);
        }
        if berechnen {
            self.berechnen(projekt);
        }
    }

    /// Fenster mit den Rechenparametern; beim Schließen wird die Standardabweichung neu in die Projektdaten geschrieben.
    fn zeige_rechenparameter(&mut self, ctx: &egui::Context, projekt: &mut Projekt) {
        let Some(mut ansicht) = self.rechenparameter.take() else {
            return;
        };
        let mut offen = true;
        let mut schliessen = false;
        egui::Window::new("Rechenparameter")
            .open(&mut offen)
            .collapsible(false)
            .default_size(Vec2::new(620.0, 560.0))
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                ansicht.zeichne(ui, projekt);
                if ui.button("Schliessen").clicked() {
                    schliessen = true;
                }
            });
        if offen && !schliessen {
            self.rechenparameter = Some(ansicht);
            return;
        }
        if let Some(s0) = projekt.station.ergebnis.as_ref().map(|e| e.s0_mm) {
            if let Err(fehler) = projekt.schreibe_sigma_projektdaten(s0) {
                fehler_log::log("FreieStationierung.rechenparameter", &fehler.to_string());
            }
        }
    }

    fn zeige_meldung(&mut self, ctx: &egui::Context) {
        let Some((titel, text)) = self.meldung.clone() else {
            return;
        };
        let mut ok = false;
        egui::Window::new(titel).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO).show(ctx, |ui| {
            ui.label(text);
            ui.add_space(6.0);
            ok = ui.button("OK").clicked();
        });
        if ok {
            self.meldung = None;
        }
    }
}

/// Bewertet eine berechnete Stationierung:
/// - nicht verwendbar (z. B. nicht konvergiert): gesperrt;
/// - genau 2 Punkte (Direktlösung, r = 0, keine Residuenkontrolle): nur nach ausdrücklicher Bestätigung;
/// - sonst frei bei mindestens 3 grünen Quer- und 3 grünen Längsresiduen, andernfalls gesperrt.
fn bewerte_freigabe(e: &StationierungsErgebnis, verschiedene: usize, projekt: &Projekt) -> Freigabe {
    if !ist_verwendbar(Some(e)) {
        return Freigabe::Gesperrt("Die Berechnung ist nicht verwendbar (nicht konvergiert oder ungültige Werte).".to_string());
    }
    if verschiedene <= MIN_ZEILEN {
        return Freigabe::NurMitBestaetigung(format!(
            "Nur {verschiedene} verschiedene Anschlusspunkte: geringe Kontrolle (Redundanz r = {}).",
            e.redundanz
        ));
    }
    let par = &projekt.rechenparameter.params;
    let (mut quer, mut laengs) = (0, 0);
    for r in &e.residuen {
        if r.richtung_aktiv && !r.v_quer_mm.is_nan() && ist_gruen(r.v_quer_mm.abs(), par.fehlergrenze_mm_strecke) {
            quer += 1;
        }
        if r.strecke_aktiv && !r.v_strecke_mm.is_nan() && ist_gruen(r.v_strecke_mm.abs(), par.fehlergrenze_mm_strecke) {
            laengs += 1;
        }
    }
    if quer >= MIN_GRUENE_RESIDUEN && laengs >= MIN_GRUENE_RESIDUEN {
        Freigabe::Frei
    } else {
        Freigabe::Gesperrt(format!(
            "{quer} von {MIN_GRUENE_RESIDUEN} grünen Quer- und {laengs} von {MIN_GRUENE_RESIDUEN} grünen Längsresiduen."
        ))
    }
}

/// Hinweistext, wenn für den Standpunkt abgeschaltete Beobachtungen gespeichert sind, die Auswahl aber nicht zu den
/// aktuellen Punkten passt und deshalb verworfen wird (alle Beobachtungen aktiv).
fn verworfene_auswahl(projekt: &Projekt, standpunkt: &str, wird_angewendet: bool) -> String {
    let gespeichert = projekt.station.standpunkt_nr == standpunkt && projekt.station.aktivierung().anzahl_abgeschaltet() > 0;
    if gespeichert && !wird_angewendet {
        "Die gespeicherte Beobachtungsauswahl passt nicht zu den Punkten (Anzahl oder Reihenfolge) und wurde verworfen: \
         alle Beobachtungen sind aktiv. Bitte die Haken prüfen."
            .to_string()
    } else if gespeichert {
        "Die gespeicherte Beobachtungsauswahl wurde übernommen. Bitte die Haken prüfen.".to_string()
    } else {
        String::new()
    }
}

/// Anzahl verschiedener Anschlusspunkte (Punktnummern) unter den Messzeilen.
fn anzahl_verschiedene(punkte: &[StationierungsPunkt]) -> usize {
    punkte.iter().map(|p| p.punkt_nr.trim()).collect::<std::collections::HashSet<_>>().len()
}

/// Gespeicherte Beobachtungsauswahl, wenn der aktive Standpunkt derselbe ist und die Punkte in gleicher Reihenfolge vorliegen.
fn gespeicherte_aktivierung(projekt: &Projekt, standpunkt: &str, punkte: &[StationierungsPunkt]) -> Option<Aktivierung> {
    let station = &projekt.station;
    let gleich = station.standpunkt_nr == standpunkt
        && station.messungen.len() == punkte.len()
        && station.messungen.iter().zip(punkte).all(|(a, b)| a.punkt_nr == b.punkt_nr);
    gleich.then(|| station.aktivierung())
}

/// Ergebnisdialog „Ergebnis – Freie Stationierung“ mit Residuentabelle und Aktivierung je Beobachtung.
struct ErgebnisDialog {
    standpunkt: String,
    ih: f64,
    punkte: Vec<StationierungsPunkt>,
    ergebnis: StationierungsErgebnis,
    akt_hz: Vec<bool>,
    akt_str: Vec<bool>,
    akt_hoe: Vec<bool>,
    fehler: String,
    /// Hinweis zur Beobachtungsauswahl (orange).
    hinweis: String,
    /// Das angezeigte Ergebnis gehört zur aktuellen Haken-Auswahl (falsch nach einer fehlgeschlagenen Neuberechnung).
    gueltig: bool,
}

impl ErgebnisDialog {
    /// Rechnet mit allen Richtungen und Strecken; Höhen mit Höhe 0 sind zunächst abgeschaltet.
    /// `vorgabe`: gespeicherte Beobachtungsauswahl desselben Standpunkts mit denselben Punkten (sonst alles aktiv).
    fn neu(
        standpunkt: String,
        ih: f64,
        punkte: Vec<StationierungsPunkt>,
        vorgabe: Option<Aktivierung>,
        projekt: &Projekt,
    ) -> Result<Self, String> {
        let n = punkte.len();
        let (akt_hz, akt_str, akt_hoe) = match vorgabe {
            Some(a) => (a.richtung, a.strecke, a.hoehe),
            None => (vec![true; n], vec![true; n], punkte.iter().map(|p| p.hoehe != 0.0).collect()),
        };
        let mut dialog = Self {
            standpunkt,
            ih,
            punkte,
            ergebnis: StationierungsErgebnis::default(),
            akt_hz,
            akt_str,
            akt_hoe,
            fehler: String::new(),
            hinweis: String::new(),
            gueltig: true,
        };
        dialog.rechne(projekt)?;
        Ok(dialog)
    }

    /// Berechnet mit der aktuellen Aktivierung. Das Ergebnis wird erst beim Speichern ins Projekt übernommen.
    fn rechne(&mut self, projekt: &Projekt) -> Result<(), String> {
        // Bis zum erfolgreichen Abschluss gilt das angezeigte Ergebnis nicht mehr für die aktuelle Auswahl
        self.gueltig = false;
        let optionen = {
            let par = &projekt.rechenparameter.params;
            Optionen {
                freier_massstab: par.freier_massstab,
                aktiv_richtung: Some(self.akt_hz.clone()),
                aktiv_strecke: Some(self.akt_str.clone()),
                aktiv_hoehe: Some(self.akt_hoe.clone()),
                berechnung_3d: par.berechnung_3d,
                fehlergrenze_mm_hoehe: par.fehlergrenze_mm_hoehe,
                ..Optionen::default()
            }
        };
        let ergebnis = freie_stationierung::berechnen(&self.punkte, self.ih, &optionen)?;
        pruefe_station(&ergebnis)?;
        protokoll::log("RECHNUNG", &format!("Freie Stationierung berechnet, s0 = {} mm", de(ergebnis.s0_mm, 2)));
        self.ergebnis = ergebnis;
        self.gueltig = true;
        Ok(())
    }

    /// Schreibt das RTF-Protokoll mit der zuletzt berechneten Aktivierung.
    fn schreibe_protokoll(&self, projekt: &Projekt) -> Result<(), String> {
        let p = stationierung::protokoll(
            &self.ergebnis,
            &self.punkte,
            (&self.akt_hz, &self.akt_str, &self.akt_hoe),
            &self.standpunkt,
            self.ih,
            &projekt.rechenparameter.params,
            &projekt.projektdaten.bearbeiter,
            jetzt(),
        );
        schreibe_fuer_projekt(&p, projekt).map(|_| ())
    }

    /// Zeichnet den Dialog; `true`, wenn er geschlossen wurde.
    fn zeige(&mut self, ctx: &egui::Context, projekt: &mut Projekt) -> bool {
        let mut schliessen = false;
        let mut neu_rechnen = false;
        egui::Window::new("Ergebnis – Freie Stationierung")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                ui.set_width(DIALOG_BREITE);
                ui.label(RichText::new("Freie Stationierung  –  Ergebnis").strong().size(SCHRIFT_TITEL));
                ui.separator();
                self.zeichne_kopfdaten(ui, projekt);
                self.zeichne_bewertung(ui, projekt);
                ui.separator();
                ui.add_space(4.0);
                ui.label(RichText::new("Residuen pro Anschlusspunkt:").strong().size(SCHRIFT_UEBERSCHRIFT));
                ui.add_space(4.0);
                neu_rechnen |= self.zeichne_residuen(ui, projekt);
                if !self.fehler.is_empty() {
                    ui.colored_label(Color32::RED, &self.fehler);
                }
                if !self.hinweis.is_empty() {
                    ui.colored_label(FARBE_FREIGABE_BESTAETIGUNG, &self.hinweis);
                }
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    let knopf = egui::Button::new(RichText::new("↻ Neu berechnen").strong().size(SCHRIFT_KOPF).color(Color32::WHITE))
                        .fill(FARBE_NEU_BERECHNEN)
                        .min_size(Vec2::new(KNOPF_BREITE_NEU_BERECHNEN, KNOPF_HOEHE));
                    neu_rechnen |= ui.add(knopf).clicked();
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let knopf = egui::Button::new("Schliessen").min_size(Vec2::new(KNOPF_BREITE_SCHLIESSEN, KNOPF_HOEHE));
                        schliessen = ui.add(knopf).clicked();
                    });
                });
            });
        if neu_rechnen {
            self.fehler = match self.rechne(projekt) {
                Ok(()) => String::new(),
                Err(fehler) => format!(
                    "{fehler}\nDas angezeigte Ergebnis gehört zur vorherigen Auswahl und wird nicht zur Übernahme freigegeben."
                ),
            };
        }
        schliessen
    }

    fn zeichne_kopfdaten(&self, ui: &mut egui::Ui, projekt: &Projekt) {
        let e = &self.ergebnis;
        let p = &projekt.rechenparameter.params;
        let zeile = |ui: &mut egui::Ui, text: String| {
            ui.label(RichText::new(text).monospace());
        };
        zeile(ui, format!("Standpunktnummer:        {}", self.standpunkt));
        zeile(ui, format!("Instrumentenhöhe:        {} m", de(self.ih, 3)));
        zeile(ui, format!("R  (Rechtswert):         {} m", de(e.r, 3)));
        zeile(ui, format!("H  (Hochwert):           {} m", de(e.h, 3)));
        let hoehe = if e.berechnung_3d {
            format!("Höhe:                    {} m", de(e.hoehe, 3))
        } else if !e.warnung_hoehe.is_empty() {
            "Höhe:                    –  (Höhenabweichung zu groß; Ergebnis nur 2D)".to_string()
        } else {
            "Höhe:                    –  (keine aktiven Höhenmessungen)".to_string()
        };
        let farbe = if e.warnung_hoehe.is_empty() { ui.visuals().text_color() } else { FARBE_HOEHE_WARNUNG };
        ui.label(RichText::new(hoehe).monospace().color(farbe));
        zeile(ui, format!("Orientierungsunbekannte: {} gon", de(e.orientierung_gon, 4)));
        let fixiert = if p.freier_massstab { "" } else { "  [fixiert]" };
        zeile(ui, format!("Maßstab (Näherung):      {}{fixiert}", de(e.massstab, 6)));
        if p.freier_massstab {
            ui.label(
                RichText::new("Der Maßstab stammt aus der Helmert-Näherung und wird nicht mitausgeglichen (Ausgleichung: R, H, Orientierung).")
                    .small()
                    .color(FARBE_HINWEIS_GRAU),
            );
        }
        let beschriftung = format!("{:<w$}", format!("Standardabw. {}:", p.sigma_bez()), w = BESCHRIFTUNG_BREITE);
        zeile(
            ui,
            if e.redundanz > 0 {
                format!("{beschriftung}{} mm   (Redundanz r = {})", de(p.sigma_wert(e.s0_mm), 2), e.redundanz)
            } else {
                format!("{beschriftung}–   (Redundanz r = 0, eindeutig bestimmt)")
            },
        );
        zeile(
            ui,
            if self.punkte.len() == 2 && e.iterationen == 0 && e.redundanz == 0 {
                "Methode:                 Ähnlichkeitstransformation  (2 Punkte, Direktlösung)".to_string()
            } else {
                format!(
                    "Iterationen:             {}{}",
                    e.iterationen,
                    if e.konvergiert { "  (konvergiert)" } else { "  !! NICHT konvergiert !!" }
                )
            },
        );
    }

    /// Konfidenzellipse des Standpunkts sowie die getrennte Bewertung „Berechnung möglich“ / „Übernahme“.
    fn zeichne_bewertung(&self, ui: &mut egui::Ui, projekt: &Projekt) {
        let e = &self.ergebnis;
        let zeile = |ui: &mut egui::Ui, text: String| {
            ui.label(RichText::new(text).monospace());
        };
        match &e.konfidenz {
            Some(k) => {
                let theta_gon = k.theta_rad.rem_euclid(std::f64::consts::PI) * RAD_ZU_GON;
                zeile(
                    ui,
                    format!(
                        "Konfidenzellipse {} %:   a = {} mm   b = {} mm   Richtung der großen Achse = {} gon",
                        de(k.niveau * PROZENT, 0),
                        de(k.a_mm, 2),
                        de(k.b_mm, 2),
                        de(theta_gon, 2)
                    ),
                );
                zeile(
                    ui,
                    format!(
                        "Lagegenauigkeit:         σR = {} mm   σH = {} mm   ({}, Maßstab fest)",
                        de(k.sigma_r_mm, 2),
                        de(k.sigma_h_mm, 2),
                        if k.a_priori { "a priori m0" } else { "a posteriori s0" }
                    ),
                );
            }
            None => zeile(ui, "Konfidenzellipse:        nicht bestimmbar (Direktlösung oder singuläre Normalmatrix)".to_string()),
        }
        let freigabe = bewerte_freigabe(e, anzahl_verschiedene(&self.punkte), projekt);
        ui.label(RichText::new("Berechnung:             möglich").monospace().color(FARBE_FREIGABE_FREI));
        let text = match freigabe.grund() {
            Some(grund) => format!("Übernahme:              {} – {grund}", freigabe.status()),
            None => format!("Übernahme:              {}", freigabe.status()),
        };
        ui.label(RichText::new(text).monospace().strong().color(freigabe.farbe()));
    }

    /// Residuentabelle; `true`, wenn ein Haken geändert wurde (dann ist neu zu rechnen).
    fn zeichne_residuen(&mut self, ui: &mut egui::Ui, projekt: &Projekt) -> bool {
        let par = &projekt.rechenparameter.params;
        let frei = DIALOG_BREITE - 3.0 * HAKEN_SPALTE;
        let summe: f32 = GEWICHTE_RESIDUEN.iter().sum();
        let b: Vec<f32> = GEWICHTE_RESIDUEN.iter().map(|g| frei * g / summe).collect();
        // Reihenfolge: Punkt | Str.h | v Winkel | v Quer | Hz | v Längs | Str | v Höhe | Hoe
        let raster = Raster::neu(&[b[0], b[1], b[2], b[3], HAKEN_SPALTE, b[4], HAKEN_SPALTE, b[5], HAKEN_SPALTE]);
        let mut geaendert = false;
        ui.spacing_mut().item_spacing = Vec2::ZERO;
        raster.kopf(ui, &["Punkt", "Str.h [m]", "v Winkel [cc]", "v Quer [mm]", "Hz", "v Längs [mm]", "Str", "v Höhe [mm]", "Hoe"]);
        let zeilen_hoehe = raster.zeilen_hoehe();
        let anzahl = self.punkte.len();
        egui::ScrollArea::vertical().id_salt("fs_residuen").max_height(DIALOG_TABELLE_HOEHE).auto_shrink([false, true]).show_rows(ui, zeilen_hoehe, anzahl, |ui, sichtbar| {
            ui.spacing_mut().item_spacing = Vec2::ZERO;
            for i in sichtbar {
                let res = self.ergebnis.residuen.get(i);
                let nr = self.punkte[i].punkt_nr.clone();
                let (hz, st, hoe) = (&mut self.akt_hz[i], &mut self.akt_str[i], &mut self.akt_hoe[i]);
                raster.zeile(ui, i, "", |z| {
                    z.zelle(|ui| {
                        ui.label(RichText::new(nr).monospace());
                    });
                    z.zelle(|ui| {
                        ui.label(RichText::new(res.map_or_else(|| STRICH.to_string(), |r| de(r.strecke_h, 3))).monospace());
                    });
                    wert_zelle(z, res.filter(|r| r.richtung_aktiv).map(|r| r.v_winkel_cc), par.fehlergrenze_cc_winkel);
                    wert_zelle(z, res.filter(|r| r.richtung_aktiv).map(|r| r.v_quer_mm), par.fehlergrenze_mm_strecke);
                    let vorher = *hz;
                    z.haken(hz);
                    geaendert |= vorher != *hz;
                    wert_zelle(z, res.filter(|r| r.strecke_aktiv).map(|r| r.v_strecke_mm), par.fehlergrenze_mm_strecke);
                    let vorher = *st;
                    z.haken(st);
                    geaendert |= vorher != *st;
                    wert_zelle(z, res.filter(|r| r.hoehe_aktiv).map(|r| r.v_hoehe_mm), par.fehlergrenze_mm_hoehe);
                    let vorher = *hoe;
                    z.haken(hoe);
                    geaendert |= vorher != *hoe;
                });
            }
        });
        geaendert
    }
}
