// Zweck:    RTF-Protokolle: gemeinsame Ausgabe (Vorlagen, Dateinamen, Zielordner) und die Protokolle der Module
// Hardware: keine
// Autor:    Johann Gerner

pub mod absteckung;
pub mod berechnungen;
pub mod fl_teilung;
pub mod polar;
pub mod rtf;
pub mod stationierung;
pub mod transformation;

use std::fs;
use std::path::{Path, PathBuf};

use chrono::NaiveDateTime;

use crate::daten::projekt::Projekt;
use crate::grundlagen::datei_speicher;
use crate::grundlagen::format::f;
use crate::grundlagen::pfade;
use rtf::{Felder, Tabellen, Zeile};

/// Eingebettete Vorlagen (die Originale liegen im Ordner `vorlagen`).
pub const VORLAGE_FREIE_STATIONIERUNG: &str = include_str!("../../vorlagen/FreieStationierung_Protokoll.xml");
pub const VORLAGE_RUECKWAERTSSCHNITT: &str = include_str!("../../vorlagen/Rueckwaertsschnitt_Protokoll.xml");
pub const VORLAGE_VORWAERTSSCHNITT: &str = include_str!("../../vorlagen/Vorwaertsschnitt_Protokoll.xml");
pub const VORLAGE_BOGENSCHNITT: &str = include_str!("../../vorlagen/Bogenschnitt_Protokoll.xml");
pub const VORLAGE_HOCHPUNKT: &str = include_str!("../../vorlagen/Hochpunktherablegung_Protokoll.xml");
pub const VORLAGE_TRANSFORMATION: &str = include_str!("../../vorlagen/KoordTransformation_Protokoll.xml");
pub const VORLAGE_FLAECHENTEILUNG: &str = include_str!("../../vorlagen/Flaechenteilung_Protokoll.xml");
pub const VORLAGE_POLARPUNKTE: &str = include_str!("../../vorlagen/Polarpunkte_Protokoll.xml");
pub const VORLAGE_PUNKTABSTECKUNG: &str = include_str!("../../vorlagen/Punktabsteckung_Protokoll.xml");
pub const VORLAGE_ACHSABSTECKUNG: &str = include_str!("../../vorlagen/Achsabsteckung_Protokoll.xml");
pub const VORLAGE_RASTERABSTECKUNG: &str = include_str!("../../vorlagen/Rasterabsteckung_Protokoll.xml");
pub const VORLAGE_PROFILABSTECKUNG: &str = include_str!("../../vorlagen/Profilabsteckung_Protokoll.xml");
pub const VORLAGE_SCHNURGERUEST: &str = include_str!("../../vorlagen/Schnurgeruest_Protokoll.xml");

/// Format des Zeitpunkts im Protokollkopf.
const ZEITPUNKT_FORMAT: &str = "%d.%m.%Y    %H:%M:%S";
/// Format des Zeitstempels im Dateinamen.
const DATEI_ZEIT_FORMAT: &str = "%Y-%m-%d_%H-%M-%S";
/// Platzhalter für nicht vorhandene Werte.
pub const STRICH: &str = "-";

/// Ein vollständig aufbereitetes Protokoll, bereit zum Schreiben.
#[derive(Clone, Debug)]
pub struct Protokoll {
    pub vorlage: &'static str,
    /// Dateinamenpräfix, z. B. „Rueckwaertsschnitt".
    pub praefix: &'static str,
    pub felder: Felder,
    pub tabellen: Tabellen,
    /// Zeitpunkt der Erstellung (für den Dateinamen).
    pub zeitpunkt: NaiveDateTime,
}

impl Protokoll {
    /// Erzeugt den RTF-Text.
    pub fn rtf(&self) -> Result<String, String> {
        rtf::erzeuge(self.vorlage, &self.felder, &self.tabellen)
    }

    /// Dateipfad im Zielordner.
    pub fn pfad(&self, verzeichnis: &Path) -> PathBuf {
        verzeichnis.join(format!("{}_{}.rtf", self.praefix, self.zeitpunkt.format(DATEI_ZEIT_FORMAT)))
    }

    /// Schreibt die RTF-Datei und liefert deren Pfad.
    pub fn schreibe(&self, verzeichnis: &Path) -> Result<PathBuf, String> {
        let text = self.rtf()?;
        let pfad = self.pfad(verzeichnis);
        datei_speicher::schreibe_text(&pfad, &text).map_err(|e| format!("Protokoll konnte nicht geschrieben werden: {e}"))?;
        Ok(pfad)
    }
}

/// Zielordner der Protokolle: Projektverzeichnis, sonst der Programmordner.
pub fn ausgabeverzeichnis(projekt: &Projekt) -> PathBuf {
    if projekt.einstellungen.ist_geladen() { projekt.einstellungen.projekt_verzeichnis.clone() } else { pfade::basis() }
}

/// Schreibt ein Protokoll ins Projektverzeichnis, sofern die Protokollierung eingeschaltet ist.
pub fn schreibe_fuer_projekt(protokoll: &Protokoll, projekt: &Projekt) -> Result<PathBuf, String> {
    if !projekt.einstellungen.protokoll_aktiv {
        return Err("Die Protokollierung ist ausgeschaltet (Hauptmenü: „Protokollieren“).".to_string());
    }
    protokoll.schreibe(&ausgabeverzeichnis(projekt))
}

/// Zuletzt erzeugtes Protokoll eines Moduls (nach Dateiname sortiert).
pub fn letztes_protokoll(verzeichnis: &Path, praefix: &str) -> Option<PathBuf> {
    let muster = format!("{praefix}_");
    fs::read_dir(verzeichnis)
        .ok()?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.extension().is_some_and(|e| e.eq_ignore_ascii_case("rtf"))
                && p.file_name().is_some_and(|n| n.to_string_lossy().starts_with(&muster))
        })
        .max_by_key(|p| p.file_name().map(|n| n.to_os_string()))
}

/// Öffnet eine Datei mit dem Standardprogramm.
pub fn oeffne(pfad: &Path) -> Result<(), String> {
    std::process::Command::new("cmd").args(["/C", "start", ""]).arg(pfad).spawn().map(|_| ()).map_err(|e| format!("Datei konnte nicht geöffnet werden: {e}"))
}

// ── Gemeinsame Formatierung ──────────────────────────────────────────────────

/// Zeitpunkt für den Protokollkopf.
pub fn zeitpunkt_text(zeit: NaiveDateTime) -> String {
    zeit.format(ZEITPUNKT_FORMAT).to_string()
}

/// Wert mit Vorzeichen, Null ohne Vorzeichen („+0.0;-0.0;0.0").
pub fn signiert(wert: f64, stellen: usize) -> String {
    if wert == 0.0 {
        f(0.0, stellen)
    } else if wert < 0.0 {
        f(wert, stellen)
    } else {
        format!("+{}", f(wert, stellen))
    }
}

/// Wert mit Vorzeichen, Null mit Pluszeichen („+0.000;-0.000").
pub fn signiert_plus(wert: f64, stellen: usize) -> String {
    if wert < 0.0 { f(wert, stellen) } else { format!("+{}", f(wert, stellen)) }
}

/// Wie `signiert`, aber „-" bei NaN.
pub fn signiert_oder_strich(wert: f64, stellen: usize) -> String {
    if wert.is_nan() { STRICH.to_string() } else { signiert(wert, stellen) }
}

/// Felder aus Paaren.
pub fn felder(paare: &[(&str, String)]) -> Felder {
    paare.iter().map(|(k, v)| ((*k).to_string(), v.clone())).collect()
}

/// Tabellenzeile aus Paaren.
pub fn zeile(paare: &[(&str, String)]) -> Zeile {
    felder(paare)
}

/// Standardabweichungszeile der Schnitte: s0, Redundanz, Iterationen, Konvergenz.
pub fn s0_info(redundanz: i32, s0_mm: f64, iterationen: usize, konvergiert: bool) -> String {
    if redundanz > 0 {
        let konv = if konvergiert {
            format!("Iterationen:  {iterationen}  (konvergiert)")
        } else {
            format!("Iterationen:  {iterationen}  !! NICHT konvergiert !!")
        };
        format!("Standardabw. s0:  {} mm     Redundanz r:  {redundanz}     {konv}", f(s0_mm, 2))
    } else {
        "Standardabw. s0:  -  (Redundanz r = 0, eindeutig bestimmt)".to_string()
    }
}

/// Ampelstufe eines Betrags nach drei Schwellen: "3" über der höchsten, "2", "1", sonst leer.
pub fn ampel_stufe(betrag: f64, schwellen: [f64; 3]) -> String {
    if betrag > schwellen[0] {
        "3"
    } else if betrag > schwellen[1] {
        "2"
    } else if betrag > schwellen[2] {
        "1"
    } else {
        ""
    }
    .to_string()
}

#[cfg(test)]
pub fn pruefe_vollstaendig(protokoll: &Protokoll) -> String {
    let rtf = protokoll.rtf().expect("Vorlage muss gültig sein");
    // Jede Variable der Vorlage muss belegt sein: Felder, Monoblöcke, bedingte Abschnitte und Tabellenspalten
    let dok = roxmltree::Document::parse(protokoll.vorlage.trim_start_matches('\u{feff}')).unwrap();
    let mut fehlend: Vec<String> = Vec::new();
    for knoten in dok.descendants().filter(|k| k.is_element()) {
        let name = knoten.tag_name().name();
        if let Some(v) = knoten.attribute("var").filter(|_| name == "Feld" || name == "Abschnitt") {
            if !protokoll.felder.contains_key(v) {
                fehlend.push(format!("Feld {v}"));
            }
        }
        if let Some(b) = knoten.attribute("zeigenWenn") {
            if !protokoll.felder.contains_key(b) {
                fehlend.push(format!("Bedingung {b}"));
            }
        }
        if name == "Abschnitt" && knoten.attribute("typ") == Some("Tabelle") {
            let tabelle = knoten.attribute("name").unwrap_or("");
            let Some(zeilen) = protokoll.tabellen.get(tabelle) else {
                fehlend.push(format!("Tabelle '{tabelle}'"));
                continue;
            };
            for spalte in knoten.children().filter(|k| k.is_element() && k.tag_name().name() == "Spalte") {
                let var = spalte.attribute("var").unwrap_or("");
                if zeilen.iter().any(|z| !z.contains_key(var)) {
                    fehlend.push(format!("Spalte {tabelle}.{var}"));
                }
            }
        }
    }
    assert!(fehlend.is_empty(), "Unbelegtes in {}: {fehlend:?}", protokoll.praefix);
    rtf
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formatierungshilfen() {
        assert_eq!(signiert(0.0, 1), "0.0");
        assert_eq!(signiert(2.34, 1), "+2.3");
        assert_eq!(signiert(-2.34, 1), "-2.3");
        assert_eq!(signiert_plus(0.0, 3), "+0.000");
        assert_eq!(signiert_oder_strich(f64::NAN, 1), "-");
        assert_eq!(ampel_stufe(61.0, [60.0, 20.0, 5.0]), "3");
        assert_eq!(ampel_stufe(21.0, [60.0, 20.0, 5.0]), "2");
        assert_eq!(ampel_stufe(6.0, [60.0, 20.0, 5.0]), "1");
        assert_eq!(ampel_stufe(5.0, [60.0, 20.0, 5.0]), "");
        assert!(s0_info(0, 1.0, 3, true).contains("eindeutig bestimmt"));
        assert!(s0_info(2, 1.234, 4, false).contains("NICHT konvergiert"));
    }

    #[test]
    fn alle_vorlagen_sind_gueltig() {
        for v in [
            VORLAGE_FREIE_STATIONIERUNG,
            VORLAGE_RUECKWAERTSSCHNITT,
            VORLAGE_VORWAERTSSCHNITT,
            VORLAGE_BOGENSCHNITT,
            VORLAGE_HOCHPUNKT,
            VORLAGE_TRANSFORMATION,
            VORLAGE_FLAECHENTEILUNG,
            VORLAGE_POLARPUNKTE,
            VORLAGE_PUNKTABSTECKUNG,
            VORLAGE_ACHSABSTECKUNG,
            VORLAGE_RASTERABSTECKUNG,
            VORLAGE_PROFILABSTECKUNG,
            VORLAGE_SCHNURGERUEST,
        ] {
            assert!(rtf::erzeuge(v, &Felder::new(), &Tabellen::new()).is_ok());
        }
    }

    #[test]
    fn letztes_protokoll_waehlt_juengstes() {
        let dir = std::env::temp_dir().join(format!("fb-rtf-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        for n in ["Polarpunkte_2026-01-01_10-00-00.rtf", "Polarpunkte_2026-02-01_09-00-00.rtf", "Anderes_2027-01-01_00-00-00.rtf", "Polarpunkte_x.txt"] {
            fs::write(dir.join(n), "x").unwrap();
        }
        let p = letztes_protokoll(&dir, "Polarpunkte").unwrap();
        assert!(p.to_string_lossy().ends_with("Polarpunkte_2026-02-01_09-00-00.rtf"));
        assert!(letztes_protokoll(&dir, "Fehlt").is_none());
        let _ = fs::remove_dir_all(dir);
    }
}
