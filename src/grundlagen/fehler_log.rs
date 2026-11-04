// Zweck:    Protokolliert unerwartete Fehler in Fehler.log und im Tagesprotokoll
// Hardware: keine
// Autor:    Johann Gerner

use std::fs::OpenOptions;
use std::io::Write;

use chrono::Local;

use super::{pfade, protokoll};

/// Name der Fehlerdatei im Benutzerverzeichnis.
const FEHLER_LOG_DATEI: &str = "Fehler.log";

/// Protokolliert einen Fehler mit Kontext (immer in Fehler.log, optional im Tagesprotokoll).
pub fn log(kontext: &str, nachricht: &str) {
    protokoll::log("FEHLER", &format!("{kontext} – {nachricht}"));
    let eintrag = format!(
        "{}  [{kontext}]  {nachricht}\r\n",
        Local::now().format("%Y-%m-%d %H:%M:%S")
    );
    let pfad = pfade::benutzer_get(FEHLER_LOG_DATEI);
    let ergebnis = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&pfad)
        .and_then(|mut f| f.write_all(eintrag.as_bytes()));
    if ergebnis.is_err() {
        eprintln!("[Feldbuch FEHLER] {kontext}: {nachricht}");
    }
}
