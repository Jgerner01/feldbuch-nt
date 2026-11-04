// Zweck:    Kommunikationsprotokoll mit dem Messgerät in einer eigenen Datei (Kommunikation.log im
//           Benutzerverzeichnis): jede Zeile mit Zeitstempel, gesendet und empfangen auch als Hex. Zur Auswertung
//           bei Verbindungsproblemen. Die Datei wird bei mehr als 2 MB zu Kommunikation.alt.log gerollt.
// Hardware: Tachymeter und GNSS über serielle Schnittstelle
// Autor:    Johann Gerner

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use chrono::Local;

use super::pfade;

/// Name der Protokolldatei.
const DATEINAME: &str = "Kommunikation.log";
/// Name der gerollten Datei.
const DATEINAME_ALT: &str = "Kommunikation.alt.log";
/// Ab dieser Größe wird die Datei gerollt.
const MAX_BYTES: u64 = 2 * 1024 * 1024;

static SPERRE: Mutex<()> = Mutex::new(());

/// Pfad der Protokolldatei.
pub fn pfad() -> PathBuf {
    pfade::benutzer_get(DATEINAME)
}

/// Hängt eine Zeile mit Zeitstempel an die Protokolldatei an.
pub fn schreibe(zeile: &str) {
    schreibe_in(&pfad(), zeile);
}

/// Wie `schreibe`, mit frei wählbarer Datei (für Tests).
pub fn schreibe_in(datei: &Path, zeile: &str) {
    let _wache = SPERRE.lock();
    if fs::metadata(datei).is_ok_and(|m| m.len() > MAX_BYTES) {
        let alt = datei.with_file_name(DATEINAME_ALT);
        let _ = fs::remove_file(&alt);
        let _ = fs::rename(datei, alt);
    }
    if let Some(ordner) = datei.parent() {
        let _ = fs::create_dir_all(ordner);
    }
    let ergebnis = OpenOptions::new().create(true).append(true).open(datei).and_then(|mut f| {
        writeln!(f, "{}  {}", Local::now().format("%Y-%m-%d %H:%M:%S%.3f"), zeile.replace(['\r', '\n'], " "))
    });
    if let Err(e) = ergebnis {
        eprintln!("[Kommunikationslog] {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_datei(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("fb-komlog-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir.join(DATEINAME)
    }

    #[test]
    fn schreibt_zeilen_mit_zeitstempel_und_ohne_umbrueche() {
        let d = test_datei("a");
        schreibe_in(&d, "erste");
        schreibe_in(&d, "zweite\r\nmit Umbruch");
        let text = fs::read_to_string(&d).unwrap();
        let zeilen: Vec<&str> = text.lines().collect();
        assert_eq!(zeilen.len(), 2);
        assert!(zeilen[0].ends_with("erste"));
        assert!(zeilen[1].ends_with("zweite  mit Umbruch"));
        // Zeitstempel: "2026-11-04 12:00:00.123"
        assert_eq!(zeilen[0].as_bytes()[4], b'-');
        assert_eq!(zeilen[0].as_bytes()[19], b'.');
        let _ = fs::remove_dir_all(d.parent().unwrap());
    }

    #[test]
    fn grosse_datei_wird_gerollt() {
        let d = test_datei("b");
        fs::create_dir_all(d.parent().unwrap()).unwrap();
        fs::write(&d, vec![b'x'; (MAX_BYTES + 10) as usize]).unwrap();
        schreibe_in(&d, "neu");
        assert!(d.with_file_name(DATEINAME_ALT).exists());
        assert_eq!(fs::read_to_string(&d).unwrap().lines().count(), 1);
        let _ = fs::remove_dir_all(d.parent().unwrap());
    }
}
