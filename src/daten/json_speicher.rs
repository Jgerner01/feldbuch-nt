// Zweck:    JSON-Dateien laden und speichern (C#-kompatible Formatierung, atomares Schreiben)
// Hardware: keine
// Autor:    Johann Gerner

use std::fs;
use std::io;
use std::path::Path;

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::grundlagen::{datei_speicher, fehler_log};

/// Liest und deserialisiert eine JSON-Datei (UTF-8, auch mit BOM).
pub fn lade<T: DeserializeOwned>(pfad: &Path) -> Result<T, String> {
    let text = fs::read_to_string(pfad).map_err(|e| e.to_string())?;
    serde_json::from_str(text.trim_start_matches('\u{feff}')).map_err(|e| e.to_string())
}

/// Serialisiert (eingerückt) und schreibt atomar.
pub fn speichere<T: Serialize>(pfad: &Path, daten: &T, kontext: &str) -> io::Result<()> {
    if pfad.as_os_str().is_empty() {
        return Ok(());
    }
    let json = serde_json::to_string_pretty(daten).map_err(io::Error::other)?;
    datei_speicher::schreibe_text(pfad, &json).inspect_err(|e| fehler_log::log(kontext, &e.to_string()))
}

/// Lädt eine Liste mit Fehlerbehandlung wie in der C#-Version: Fehler sperren die Datei, die Liste bleibt leer.
pub fn lade_oder_melde<T: DeserializeOwned + Default>(pfad: &Path) -> T {
    match lade::<T>(pfad) {
        Ok(daten) => {
            datei_speicher::laden_erfolgreich(pfad);
            daten
        }
        Err(fehler) => {
            datei_speicher::melde_ladefehler(pfad, &fehler);
            T::default()
        }
    }
}
