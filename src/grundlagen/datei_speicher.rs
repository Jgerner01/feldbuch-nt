// Zweck:    Atomare Dateiersetzung und Schutz vor Überschreiben nach einem Ladefehler
// Hardware: keine
// Autor:    Johann Gerner

use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

use chrono::Local;

use super::fehler_log;

/// Unterordner für Sicherungskopien beim Ersetzen von Dateien.
const BACKUP_ORDNER: &str = "Backup";
/// Dateiendung der Sicherungskopien.
const BACKUP_ENDUNG: &str = "bak";
/// Zeitstempelformat für Sicherungskopien.
const BACKUP_ZEIT_FORMAT: &str = "%Y%m%d-%H%M%S-%6f";
/// Zeitstempelformat für beiseitegelegte defekte Dateien.
const DEFEKT_ZEIT_FORMAT: &str = "%Y%m%d-%H%M%S-%3f";

static AUTO_BACKUP: AtomicBool = AtomicBool::new(false);

fn gesperrte_pfade() -> &'static Mutex<HashSet<String>> {
    static PFADE: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    PFADE.get_or_init(|| Mutex::new(HashSet::new()))
}

fn meldungen() -> &'static Mutex<Vec<String>> {
    static MELDUNGEN: OnceLock<Mutex<Vec<String>>> = OnceLock::new();
    MELDUNGEN.get_or_init(|| Mutex::new(Vec::new()))
}

/// Schalter: vor dem Ersetzen einer Datei die vorherige Version sichern.
pub fn setze_auto_backup(aktiv: bool) {
    AUTO_BACKUP.store(aktiv, Ordering::Relaxed);
}

/// Schlüssel für die Sperrliste (ohne Groß-/Kleinschreibung, wie unter Windows üblich).
fn schluessel(pfad: &Path) -> String {
    let voll = std::path::absolute(pfad).unwrap_or_else(|_| pfad.to_path_buf());
    voll.to_string_lossy().to_lowercase()
}

/// Holt alle aufgelaufenen Ladefehler-Meldungen für die Anzeige in der Oberfläche ab.
pub fn hole_meldungen() -> Vec<String> {
    meldungen()
        .lock()
        .map(|mut m| std::mem::take(&mut *m))
        .unwrap_or_default()
}

fn melde(text: String) {
    if let Ok(mut m) = meldungen().lock() {
        m.push(text);
    }
}

/// Sperrt die Datei für Schreibzugriffe und meldet den Fehler.
pub fn melde_ladefehler(pfad: &Path, fehler: &dyn std::fmt::Display) {
    if let Ok(mut s) = gesperrte_pfade().lock() {
        s.insert(schluessel(pfad));
    }
    fehler_log::log(&format!("Laden: {}", pfad.display()), &fehler.to_string());
    melde(format!(
        "Die Datei konnte nicht geladen werden:\n{}\n\n{}\n\nDie Datei bleibt erhalten und ist für \
         Schreibzugriffe gesperrt. Bitte eine intakte Datei wiederherstellen und das Projekt erneut \
         öffnen oder das Programm neu starten.",
        pfad.display(),
        fehler
    ));
}

/// Benennt eine neu aufbaubare, defekte Datei um und gibt ihren Pfad frei.
pub fn lege_defekte_datei_beiseite(
    pfad: &Path,
    fehler: &dyn std::fmt::Display,
    hinweis: &str,
) -> io::Result<PathBuf> {
    let sicherung = PathBuf::from(format!(
        "{}.defekt-{}",
        pfad.display(),
        Local::now().format(DEFEKT_ZEIT_FORMAT)
    ));
    fs::rename(pfad, &sicherung)?;
    laden_erfolgreich(pfad);
    fehler_log::log(&format!("Laden: {}", pfad.display()), &fehler.to_string());
    melde(format!(
        "Die Datei war defekt und wird neu angelegt:\n{}\n\n{}\n\nDie defekte Datei wurde aufbewahrt:\n{}\n\n{}",
        pfad.display(),
        fehler,
        sicherung.display(),
        hinweis
    ));
    Ok(sicherung)
}

/// Hebt die Schreibsperre auf, nachdem eine Datei erfolgreich geladen wurde.
pub fn laden_erfolgreich(pfad: &Path) {
    if let Ok(mut s) = gesperrte_pfade().lock() {
        s.remove(&schluessel(pfad));
    }
}

/// Prüft, ob in die Datei geschrieben werden darf.
pub fn pruefe_schreibzugriff(pfad: &Path) -> io::Result<()> {
    if pfad.as_os_str().is_empty() {
        return Ok(());
    }
    let gesperrt = gesperrte_pfade()
        .lock()
        .map(|s| s.contains(&schluessel(pfad)))
        .unwrap_or(false);
    if gesperrt {
        return Err(io::Error::other(format!(
            "Schreiben gesperrt nach einem Ladefehler: {}. Bitte die Datei wiederherstellen und das \
             Projekt erneut öffnen oder das Programm neu starten.",
            pfad.display()
        )));
    }
    Ok(())
}

/// Schreibt Text atomar (temporäre Datei, Flush, Ersetzen).
pub fn schreibe_text(pfad: &Path, text: &str) -> io::Result<()> {
    pruefe_schreibzugriff(pfad)?;
    let ziel = std::path::absolute(pfad)?;
    let verzeichnis = ziel
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    let dateiname = ziel
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let temp = verzeichnis.join(format!(".{dateiname}.{}.tmp", eindeutige_kennung()));

    let ergebnis = schreibe_und_ersetze(&temp, &ziel, &verzeichnis, &dateiname, text);
    if temp.exists() {
        if let Err(e) = fs::remove_file(&temp) {
            fehler_log::log(&format!("Temporäre Datei: {}", temp.display()), &e.to_string());
        }
    }
    ergebnis.map_err(|e| {
        fehler_log::log(&format!("Speichern: {}", ziel.display()), &e.to_string());
        io::Error::new(
            e.kind(),
            format!("Die Datei wurde nicht gespeichert: {}. {e}", ziel.display()),
        )
    })
}

fn schreibe_und_ersetze(
    temp: &Path,
    ziel: &Path,
    verzeichnis: &Path,
    dateiname: &str,
    text: &str,
) -> io::Result<()> {
    {
        let mut datei = File::options().write(true).create_new(true).open(temp)?;
        datei.write_all(text.as_bytes())?;
        datei.sync_all()?;
    }
    if ziel.exists() && AUTO_BACKUP.load(Ordering::Relaxed) {
        let backup_dir = verzeichnis.join(BACKUP_ORDNER);
        fs::create_dir_all(&backup_dir)?;
        let backup = backup_dir.join(format!(
            "{dateiname}.{}.{}.{BACKUP_ENDUNG}",
            Local::now().format(BACKUP_ZEIT_FORMAT),
            eindeutige_kennung()
        ));
        fs::copy(ziel, backup)?;
    }
    fs::rename(temp, ziel)
}

/// Eindeutige Kennung für temporäre Dateien und Sicherungen.
fn eindeutige_kennung() -> String {
    use std::sync::atomic::AtomicU64;
    use std::time::{SystemTime, UNIX_EPOCH};
    static ZAEHLER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!(
        "{nanos:x}{:x}{:x}",
        std::process::id(),
        ZAEHLER.fetch_add(1, Ordering::Relaxed)
    )
}

/// Schreibt Zeilen (jeweils mit Zeilenende) atomar.
pub fn schreibe_zeilen<I, S>(pfad: &Path, zeilen: I) -> io::Result<()>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut text = String::new();
    for zeile in zeilen {
        text.push_str(zeile.as_ref());
        text.push_str("\r\n");
    }
    schreibe_text(pfad, &text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schreibt_und_ersetzt_atomar() {
        let dir = std::env::temp_dir().join(format!("fb-test-{}", eindeutige_kennung()));
        fs::create_dir_all(&dir).unwrap();
        let datei = dir.join("test.txt");
        schreibe_text(&datei, "eins").unwrap();
        schreibe_text(&datei, "zwei").unwrap();
        assert_eq!(fs::read_to_string(&datei).unwrap(), "zwei");
        let reste: Vec<_> = fs::read_dir(&dir).unwrap().collect();
        assert_eq!(reste.len(), 1, "keine temporären Dateien übrig");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn ladefehler_sperrt_schreiben() {
        let dir = std::env::temp_dir().join(format!("fb-sperre-{}", eindeutige_kennung()));
        fs::create_dir_all(&dir).unwrap();
        let datei = dir.join("x.json");
        melde_ladefehler(&datei, &"kaputt");
        assert!(schreibe_text(&datei, "a").is_err());
        laden_erfolgreich(&datei);
        assert!(schreibe_text(&datei, "a").is_ok());
        let _ = fs::remove_dir_all(&dir);
    }
}
