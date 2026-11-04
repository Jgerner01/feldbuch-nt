// Zweck:    Tagesprotokoll (Protokoll_YYYY-MM-DD.txt) mit Ereigniszeilen
// Hardware: keine
// Autor:    Johann Gerner

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use chrono::Local;

use super::pfade;

/// Breite der Kategoriespalte im Protokoll.
const KATEGORIE_BREITE: usize = 8;
/// Länge der Trennlinie unter der Kopfzeile.
const TRENNLINIE_LAENGE: usize = 60;

struct LogKontext {
    aktiv: bool,
    verzeichnis: Option<PathBuf>,
}

fn kontext() -> &'static Mutex<LogKontext> {
    static KONTEXT: OnceLock<Mutex<LogKontext>> = OnceLock::new();
    KONTEXT.get_or_init(|| {
        Mutex::new(LogKontext {
            aktiv: true,
            verzeichnis: None,
        })
    })
}

/// Setzt, ob protokolliert wird und in welches Projektverzeichnis geschrieben wird.
pub fn konfiguriere(aktiv: bool, projekt_verzeichnis: Option<PathBuf>) {
    if let Ok(mut k) = kontext().lock() {
        k.aktiv = aktiv;
        k.verzeichnis = projekt_verzeichnis;
    }
}

/// Schreibt eine Zeile ins Tagesprotokoll (nur wenn die Protokollierung aktiv ist).
pub fn log(kategorie: &str, nachricht: &str) {
    let (aktiv, verzeichnis) = match kontext().lock() {
        Ok(k) => (k.aktiv, k.verzeichnis.clone()),
        Err(_) => return,
    };
    if !aktiv {
        return;
    }
    let verzeichnis = verzeichnis.unwrap_or_else(pfade::benutzer_basis);
    let jetzt = Local::now();
    let pfad = verzeichnis.join(format!("Protokoll_{}.txt", jetzt.format("%Y-%m-%d")));
    let ist_neu = !pfad.exists();

    let schreiben = || -> std::io::Result<()> {
        let _ = fs::create_dir_all(&verzeichnis);
        let mut datei = OpenOptions::new().create(true).append(true).open(&pfad)?;
        if ist_neu {
            writeln!(datei, "Feldbuch Protokoll – {}", jetzt.format("%d.%m.%Y"))?;
            writeln!(datei, "{}", "=".repeat(TRENNLINIE_LAENGE))?;
        }
        writeln!(
            datei,
            "{}  [{kategorie:<KATEGORIE_BREITE$}]  {nachricht}",
            jetzt.format("%H:%M:%S")
        )
    };
    if let Err(e) = schreiben() {
        eprintln!("[Protokoll] Schreibfehler: {e}");
    }
}
