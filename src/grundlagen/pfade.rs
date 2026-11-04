// Zweck:    Pfade der Anwendung: Verzeichnis der EXE und beschreibbare Benutzerdaten
// Hardware: keine
// Autor:    Johann Gerner

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

/// Unterordner in %LOCALAPPDATA% für die Benutzerdaten.
const BENUTZER_ORDNER: &str = "Feldbuch";
/// Dateien, die einmalig aus dem EXE-Ordner in die Benutzerdaten übernommen werden.
const ZU_UEBERNEHMENDE_DATEIEN: [&str; 2] = ["Einstellungen.xml", "Freie-Stationierung.xml"];

fn benutzer_basis_speicher() -> &'static Mutex<PathBuf> {
    static BASIS: OnceLock<Mutex<PathBuf>> = OnceLock::new();
    BASIS.get_or_init(|| Mutex::new(basis()))
}

/// Verzeichnis der laufenden EXE.
pub fn basis() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Pfad zu einer Datei neben der EXE.
pub fn get(dateiname: &str) -> PathBuf {
    basis().join(dateiname)
}

/// Beschreibbares Benutzerverzeichnis (nach `initialisiere_benutzerdaten`).
pub fn benutzer_basis() -> PathBuf {
    benutzer_basis_speicher()
        .lock()
        .map(|p| p.clone())
        .unwrap_or_else(|_| basis())
}

/// Pfad zu einer Datei im Benutzerverzeichnis.
pub fn benutzer_get(dateiname: &str) -> PathBuf {
    benutzer_basis().join(dateiname)
}

/// Legt das Benutzerverzeichnis an und übernimmt vorhandene Einstellungen einmalig.
pub fn initialisiere_benutzerdaten() {
    let lokal = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(basis);
    let ziel = lokal.join(BENUTZER_ORDNER);
    if fs::create_dir_all(&ziel).is_err() {
        return;
    }
    for name in ZU_UEBERNEHMENDE_DATEIEN {
        let quelle = get(name);
        let neu = ziel.join(name);
        if !neu.exists() && quelle.exists() {
            let _ = fs::copy(&quelle, &neu);
        }
    }
    if let Ok(mut basis) = benutzer_basis_speicher().lock() {
        *basis = ziel;
    }
}
