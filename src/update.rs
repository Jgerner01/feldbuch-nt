// Zweck:    Update-Dienst: sucht neue Versionen in den GitHub-Releases, lädt Paket und Prüfsumme, prüft und entpackt sie und
//           tauscht die Programmdateien aus (die neue EXE installiert sich nach dem Ende der alten selbst)
// Hardware: keine
// Autor:    Johann Gerner
//
// Ein Release braucht zwei Dateien (siehe tools/Release-Paket.ps1):
//   Feldbuch-NT-v1.2.3-win-x64.zip  und  Feldbuch-NT-v1.2.3-win-x64.zip.sha256
// Die Prüfsumme erkennt beschädigte Downloads, ersetzt aber keine Signatur: Wer das GitHub-Konto übernimmt, könnte auch
// die Prüfsumme austauschen.
//
// Eine laufende EXE lässt sich nicht überschreiben. Deshalb startet die alte Version die neue EXE aus dem Download-Ordner
// mit dem Argument `--update-installieren`. Diese wartet auf das Ende der alten Version, kopiert sich in den Programmordner
// und startet dort neu. Dieser Aufruf ist die Schnittstelle zwischen zwei Versionen und darf sich nicht ändern, sonst können
// ältere Versionen nicht mehr aktualisieren. Ersetzt werden nur Dateien aus dem Paket; alles andere im Ordner bleibt
// unberührt. Die vorherige Version bleibt bis zum nächsten Update im Unterordner `_vorherige_Version` erhalten.

use std::fmt;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};

/// Adresse der neuesten Release.
pub const API_URL: &str = "https://api.github.com/repos/Jgerner01/feldbuch-nt/releases/latest";
/// Adresse der Release-Seite (für Hinweise).
pub const RELEASE_SEITE: &str = "https://github.com/Jgerner01/feldbuch-nt/releases";
/// Argument der neuen EXE im Installationsmodus.
pub const INSTALL_ARGUMENT: &str = "--update-installieren";
/// Dateiname der Programmdatei im Paket.
pub const EXE_NAME: &str = "feldbuch-nt.exe";
/// Unterordner der vorherigen Version.
pub const SICHERUNGSORDNER: &str = "_vorherige_Version";
/// Größtes zulässiges Paket.
const MAX_GROESSE: u64 = 500 * 1024 * 1024;
/// Wartezeit für die Suche.
const SUCH_TIMEOUT: Duration = Duration::from_secs(15);
/// Wartezeit für den Verbindungsaufbau beim Download (sonst ohne Zeitlimit: im Feld darf es dauern, der Nutzer kann abbrechen).
const VERBINDUNGS_TIMEOUT: Duration = Duration::from_secs(15);
/// Wie lange die neue EXE auf das Ende der alten wartet.
const WARTE_AUF_ENDE: Duration = Duration::from_secs(60);
/// Versuche und Wartezeit beim Verschieben (Virenscanner halten eine gerade beendete EXE oft noch kurz offen).
const VERSUCHE: u32 = 20;
const WARTEZEIT_STANDARD_MS: u64 = 500;
/// Länge einer SHA-256-Prüfsumme in Hexzeichen.
const PRUEFSUMME_LAENGE: usize = 64;

// ── Version ──────────────────────────────────────────────────────────────────

/// Programmversion aus 2 bis 4 Zahlen (wie `System.Version`); fehlende Stellen gelten beim Vergleich als 0.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Version(Vec<u64>);

impl Version {
    pub fn parse(text: &str) -> Option<Self> {
        let teile: Option<Vec<u64>> = text.trim().split('.').map(|t| t.parse::<u64>().ok()).collect();
        teile.filter(|t| (2..=4).contains(&t.len())).map(Self)
    }

    /// Version des laufenden Programms.
    pub fn aktuell() -> Self {
        Self::parse(env!("CARGO_PKG_VERSION")).unwrap_or(Self(vec![0, 0]))
    }
}

impl Ord for Version {
    fn cmp(&self, anderer: &Self) -> std::cmp::Ordering {
        let n = self.0.len().max(anderer.0.len());
        (0..n).map(|i| (self.0.get(i).copied().unwrap_or(0), anderer.0.get(i).copied().unwrap_or(0))).map(|(a, b)| a.cmp(&b)).find(|o| o.is_ne()).unwrap_or(std::cmp::Ordering::Equal)
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, anderer: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(anderer))
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text: Vec<String> = self.0.iter().map(u64::to_string).collect();
        write!(f, "{}", text.join("."))
    }
}

// ── Release auswerten ────────────────────────────────────────────────────────

/// Eine gefundene neuere Version.
#[derive(Clone, Debug, PartialEq)]
pub struct UpdateInfo {
    pub version: Version,
    pub tag: String,
    pub notizen: String,
    pub zip_url: String,
    pub pruefsummen_url: String,
    pub groesse: u64,
}

/// Fehler beim Laden.
#[derive(Debug, PartialEq, Eq)]
pub enum Fehler {
    Abgebrochen,
    Meldung(String),
}

impl From<String> for Fehler {
    fn from(text: String) -> Self {
        Self::Meldung(text)
    }
}

impl fmt::Display for Fehler {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Abgebrochen => write!(f, "Abgebrochen."),
            Self::Meldung(text) => write!(f, "{text}"),
        }
    }
}

/// Nur https-Adressen auf github.com (ohne Benutzerangabe und Port) sind erlaubt.
fn ist_github_url(url: &str) -> bool {
    url.strip_prefix("https://").is_some_and(|rest| rest.split(['/', '?', '#']).next() == Some("github.com"))
}

/// Dateiname des Pakets zu einem Tag.
pub fn paket_name(tag: &str) -> String {
    format!("Feldbuch-NT-{tag}-win-x64.zip")
}

/// Wertet die Release-Antwort aus: `None`, wenn nichts Neueres vorliegt oder das Paket unvollständig ist.
pub fn lese_release(json: &str, aktuell: &Version) -> Result<Option<UpdateInfo>, String> {
    let wurzel: serde_json::Value = serde_json::from_str(json).map_err(|e| format!("Ungültige Antwort: {e}"))?;
    if wurzel["draft"].as_bool() == Some(true) || wurzel["prerelease"].as_bool() == Some(true) {
        return Ok(None);
    }
    let tag = wurzel["tag_name"].as_str().ok_or("Ungültige Antwort: tag_name fehlt")?.to_string();
    let Some(version) = Version::parse(tag.trim_start_matches('v')).filter(|v| v > aktuell) else {
        return Ok(None);
    };
    let zip_name = paket_name(&tag);
    let (mut zip_url, mut pruef_url, mut groesse) = (None, None, 0);
    for asset in wurzel["assets"].as_array().ok_or("Ungültige Antwort: assets fehlt")? {
        let name = asset["name"].as_str().unwrap_or("");
        let url = asset["browser_download_url"].as_str().unwrap_or("");
        if name == zip_name {
            zip_url = Some(url.to_string());
            groesse = asset["size"].as_u64().unwrap_or(0);
        } else if name == format!("{zip_name}.sha256") {
            pruef_url = Some(url.to_string());
        }
    }
    let (Some(zip_url), Some(pruefsummen_url)) = (zip_url, pruef_url) else {
        return Ok(None);
    };
    if !ist_github_url(&zip_url) || !ist_github_url(&pruefsummen_url) || groesse == 0 || groesse > MAX_GROESSE {
        return Ok(None);
    }
    let notizen = wurzel["body"].as_str().unwrap_or("").to_string();
    Ok(Some(UpdateInfo { version, tag, notizen, zip_url, pruefsummen_url, groesse }))
}

// ── Netzwerk ─────────────────────────────────────────────────────────────────

fn agent(globales_zeitlimit: Option<Duration>) -> ureq::Agent {
    let konfig = ureq::Agent::config_builder()
        .timeout_global(globales_zeitlimit)
        .timeout_connect(Some(VERBINDUNGS_TIMEOUT))
        .http_status_as_error(false)
        .user_agent(format!("Feldbuch-NT/{}", Version::aktuell()))
        .build();
    ureq::Agent::new_with_config(konfig)
}

/// Sucht die neueste Release unter `url`; 404 (keine Release) ergibt `None`. Netzwerkfehler werden gemeldet.
pub fn suche_von(url: &str, aktuell: &Version) -> Result<Option<UpdateInfo>, String> {
    let mut antwort = agent(Some(SUCH_TIMEOUT)).get(url).call().map_err(|e| format!("Verbindung fehlgeschlagen: {e}"))?;
    let status = antwort.status().as_u16();
    if status == 404 {
        return Ok(None);
    }
    if !(200..300).contains(&status) {
        return Err(format!("Der Server antwortete mit Status {status}."));
    }
    let text = antwort.body_mut().read_to_string().map_err(|e| format!("Antwort nicht lesbar: {e}"))?;
    lese_release(&text, aktuell)
}

/// Sucht auf GitHub nach einer neueren Version.
pub fn suche(aktuell: &Version) -> Result<Option<UpdateInfo>, String> {
    suche_von(API_URL, aktuell)
}

fn hole_text(agent: &ureq::Agent, url: &str) -> Result<String, String> {
    let mut antwort = agent.get(url).call().map_err(|e| format!("Verbindung fehlgeschlagen: {e}"))?;
    let status = antwort.status().as_u16();
    if !(200..300).contains(&status) {
        return Err(format!("Der Server antwortete mit Status {status}."));
    }
    antwort.body_mut().read_to_string().map_err(|e| format!("Antwort nicht lesbar: {e}"))
}

/// Lädt und prüft das Paket und liefert den entpackten Ordner mit der neuen EXE.
/// `fortschritt` erhält den Fortschritt in Prozent, `abbruch` bricht den Download ab.
pub fn lade(info: &UpdateInfo, update_ordner: &Path, fortschritt: &AtomicU64, abbruch: &AtomicBool) -> Result<PathBuf, Fehler> {
    let agent = agent(None);
    let ordner = update_ordner.join(&info.tag);
    if ordner.exists() {
        fs::remove_dir_all(&ordner).map_err(|e| format!("Alter Update-Ordner konnte nicht gelöscht werden: {e}"))?;
    }
    fs::create_dir_all(&ordner).map_err(|e| format!("Update-Ordner konnte nicht angelegt werden: {e}"))?;
    let zip_pfad = ordner.join("paket.zip");

    let pruef_text = hole_text(&agent, &info.pruefsummen_url)?;
    let erwartet = pruef_text.split_whitespace().next().unwrap_or("").to_string();
    if erwartet.len() != PRUEFSUMME_LAENGE {
        return Err("Die Prüfsummendatei des Updates ist ungültig.".to_string().into());
    }

    let antwort = agent.get(&info.zip_url).call().map_err(|e| format!("Verbindung fehlgeschlagen: {e}"))?;
    if !antwort.status().is_success() {
        return Err(format!("Der Server antwortete mit Status {}.", antwort.status().as_u16()).into());
    }
    let mut quelle = antwort.into_body().into_reader();
    let mut ziel = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&zip_pfad)
        .map_err(|e| format!("Paketdatei konnte nicht angelegt werden: {e}"))?;
    let mut puffer = vec![0u8; 81_920];
    let mut gelesen: u64 = 0;
    loop {
        if abbruch.load(Ordering::Relaxed) {
            return Err(Fehler::Abgebrochen);
        }
        let n = quelle.read(&mut puffer).map_err(|e| format!("Download unterbrochen: {e}"))?;
        if n == 0 {
            break;
        }
        gelesen += n as u64;
        if gelesen > info.groesse {
            return Err("Das Update ist größer als angekündigt.".to_string().into());
        }
        ziel.write_all(&puffer[..n]).map_err(|e| format!("Paketdatei konnte nicht geschrieben werden: {e}"))?;
        fortschritt.store(gelesen * 100 / info.groesse, Ordering::Relaxed);
    }
    drop(ziel);
    if gelesen != info.groesse {
        return Err("Das Update wurde nicht vollständig heruntergeladen.".to_string().into());
    }

    let tatsaechlich = sha256_hex(&zip_pfad).map_err(|e| format!("Prüfsumme konnte nicht berechnet werden: {e}"))?;
    if !tatsaechlich.eq_ignore_ascii_case(&erwartet) {
        return Err("Die Prüfsumme des Updates stimmt nicht.".to_string().into());
    }
    let entpackt = ordner.join("entpackt");
    entpacke(&zip_pfad, &entpackt)?;
    finde_exe(&entpackt).ok_or_else(|| "Das Update enthält keine eindeutige Programmdatei.".to_string().into())
}

/// SHA-256 einer Datei in Hexschreibweise.
pub fn sha256_hex(pfad: &Path) -> std::io::Result<String> {
    let mut datei = fs::File::open(pfad)?;
    let mut hash = Sha256::new();
    let mut puffer = vec![0u8; 81_920];
    loop {
        let n = datei.read(&mut puffer)?;
        if n == 0 {
            break;
        }
        hash.update(&puffer[..n]);
    }
    Ok(hash.finalize().iter().map(|b| format!("{b:02X}")).collect())
}

/// Entpackt ein Zip; Einträge mit Pfaden außerhalb des Zielordners werden abgelehnt.
fn entpacke(zip_pfad: &Path, ziel: &Path) -> Result<(), String> {
    let datei = fs::File::open(zip_pfad).map_err(|e| format!("Paket nicht lesbar: {e}"))?;
    let mut archiv = zip::ZipArchive::new(datei).map_err(|e| format!("Das Update-Paket ist ungültig: {e}"))?;
    fs::create_dir_all(ziel).map_err(|e| format!("Zielordner konnte nicht angelegt werden: {e}"))?;
    for i in 0..archiv.len() {
        let mut eintrag = archiv.by_index(i).map_err(|e| format!("Das Update-Paket ist ungültig: {e}"))?;
        let Some(relativ) = eintrag.enclosed_name() else {
            return Err(format!("Das Update-Paket enthält einen unzulässigen Pfad: {}", eintrag.name().map_or_else(|_| "?".to_string(), |n| n.into_owned())));
        };
        let pfad = ziel.join(relativ);
        if eintrag.is_dir() {
            fs::create_dir_all(&pfad).map_err(|e| format!("Ordner konnte nicht angelegt werden: {e}"))?;
            continue;
        }
        if let Some(eltern) = pfad.parent() {
            fs::create_dir_all(eltern).map_err(|e| format!("Ordner konnte nicht angelegt werden: {e}"))?;
        }
        let mut ausgabe = fs::File::create(&pfad).map_err(|e| format!("Datei konnte nicht angelegt werden: {e}"))?;
        std::io::copy(&mut eintrag, &mut ausgabe).map_err(|e| format!("Datei konnte nicht entpackt werden: {e}"))?;
    }
    Ok(())
}

/// Alle Dateien unter einem Ordner (sortiert, mit relativen Pfaden).
fn alle_dateien(wurzel: &Path) -> std::io::Result<Vec<PathBuf>> {
    fn sammle(ordner: &Path, liste: &mut Vec<PathBuf>) -> std::io::Result<()> {
        for eintrag in fs::read_dir(ordner)? {
            let pfad = eintrag?.path();
            if pfad.is_dir() {
                sammle(&pfad, liste)?;
            } else {
                liste.push(pfad);
            }
        }
        Ok(())
    }
    let mut liste = Vec::new();
    sammle(wurzel, &mut liste)?;
    liste.sort();
    Ok(liste)
}

/// Ordner mit der einzigen Programmdatei im entpackten Paket.
fn finde_exe(wurzel: &Path) -> Option<PathBuf> {
    let treffer: Vec<PathBuf> = alle_dateien(wurzel)
        .ok()?
        .into_iter()
        .filter(|p| p.file_name().is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(EXE_NAME)))
        .collect();
    match treffer.as_slice() {
        [exe] => exe.parent().map(Path::to_path_buf),
        _ => None,
    }
}

/// Entfernt Reste früherer Updates. Noch gesperrte Dateien bleiben bis zum nächsten Start liegen.
pub fn raeume_auf(update_ordner: &Path) {
    if update_ordner.exists() {
        let _ = fs::remove_dir_all(update_ordner);
    }
}

/// Ordner für heruntergeladene Updates: `%LOCALAPPDATA%\Feldbuch\Update`.
pub fn update_ordner() -> PathBuf {
    std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir).join("Feldbuch").join("Update")
}

// ── Installation ─────────────────────────────────────────────────────────────

/// Nur veröffentlichte Versionen lassen sich austauschen, keine Entwicklungs-Builds (Ordner `target`).
pub fn ist_veroeffentlicht() -> bool {
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    exe.file_name().is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(EXE_NAME))
        && !exe.ancestors().any(|a| a.file_name().is_some_and(|n| n == "target"))
}

/// Kann im Ordner geschrieben werden?
pub fn ist_ordner_schreibbar(ordner: &Path) -> bool {
    let probe = ordner.join(format!(".update-probe-{}", std::process::id()));
    let ok = fs::write(&probe, b"").is_ok();
    let _ = fs::remove_file(&probe);
    ok
}

/// Startet die neue Version im Installationsmodus; die laufende Version muss sich danach beenden.
pub fn starte_installation(neuer_ordner: &Path, ziel_ordner: &Path) -> std::io::Result<()> {
    std::process::Command::new(neuer_ordner.join(EXE_NAME))
        .arg(INSTALL_ARGUMENT)
        .arg(std::process::id().to_string())
        .arg(neuer_ordner)
        .arg(ziel_ordner)
        .current_dir(neuer_ordner)
        .spawn()
        .map(|_| ())
}

#[cfg(windows)]
fn ohne_fenster(befehl: &mut std::process::Command) -> &mut std::process::Command {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    befehl.creation_flags(CREATE_NO_WINDOW)
}

#[cfg(not(windows))]
fn ohne_fenster(befehl: &mut std::process::Command) -> &mut std::process::Command {
    befehl
}

/// Läuft der Prozess noch?
fn prozess_laeuft(pid: u32) -> bool {
    let mut befehl = std::process::Command::new("tasklist");
    befehl.args(["/FI", &format!("PID eq {pid}"), "/NH", "/FO", "CSV"]);
    match ohne_fenster(&mut befehl).output() {
        Ok(ausgabe) => String::from_utf8_lossy(&ausgabe.stdout).contains(&format!("\"{pid}\"")),
        Err(_) => false,
    }
}

fn warte_auf_ende(pid: u32) -> Result<(), String> {
    let start = Instant::now();
    while prozess_laeuft(pid) {
        if start.elapsed() >= WARTE_AUF_ENDE {
            return Err("Die laufende Feldbuch-Version wurde nicht beendet.".to_string());
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    Ok(())
}

fn mit_wiederholung(mut aktion: impl FnMut() -> std::io::Result<()>, wartezeit_ms: u64) -> std::io::Result<()> {
    let mut versuch = 1;
    loop {
        match aktion() {
            Ok(()) => return Ok(()),
            Err(e) if versuch >= VERSUCHE => return Err(e),
            Err(_) => {
                versuch += 1;
                std::thread::sleep(Duration::from_millis(wartezeit_ms));
            }
        }
    }
}

/// Ersetzt die Dateien im Zielordner durch die des Pakets. Bei einem Fehler wird der vorherige Stand wiederhergestellt.
pub fn anwenden(quelle: &Path, ziel: &Path, wartezeit_ms: u64) -> Result<(), String> {
    if !quelle.join(EXE_NAME).is_file() {
        return Err("Das Update enthält keine Programmdatei.".to_string());
    }
    let sicherung = ziel.join(SICHERUNGSORDNER);
    if sicherung.exists() {
        fs::remove_dir_all(&sicherung).map_err(|e| format!("Die alte Sicherung konnte nicht gelöscht werden: {e}"))?;
    }
    let mut gesichert: Vec<(PathBuf, PathBuf)> = Vec::new();
    let mut neu: Vec<PathBuf> = Vec::new();
    let ergebnis = (|| -> Result<(), String> {
        for datei in alle_dateien(quelle).map_err(|e| e.to_string())? {
            let relativ = datei.strip_prefix(quelle).map_err(|e| e.to_string())?.to_path_buf();
            let ziel_datei = ziel.join(&relativ);
            if let Some(eltern) = ziel_datei.parent() {
                fs::create_dir_all(eltern).map_err(|e| format!("{}: {e}", eltern.display()))?;
            }
            if ziel_datei.exists() {
                let kopie = sicherung.join(&relativ);
                if let Some(eltern) = kopie.parent() {
                    fs::create_dir_all(eltern).map_err(|e| format!("{}: {e}", eltern.display()))?;
                }
                mit_wiederholung(|| fs::rename(&ziel_datei, &kopie), wartezeit_ms).map_err(|e| format!("{}: {e}", ziel_datei.display()))?;
                gesichert.push((ziel_datei.clone(), kopie));
            }
            neu.push(ziel_datei.clone());
            fs::copy(&datei, &ziel_datei).map_err(|e| format!("{}: {e}", ziel_datei.display()))?;
        }
        Ok(())
    })();
    if let Err(fehler) = ergebnis {
        for datei in &neu {
            if datei.is_file() {
                let _ = fs::remove_file(datei);
            }
        }
        for (original, kopie) in &gesichert {
            let _ = fs::rename(kopie, original);
        }
        return Err(fehler);
    }
    Ok(())
}

/// Einstieg der neuen EXE im Installationsmodus (`--update-installieren <pid> <neuer Ordner> <Zielordner>`).
/// Startet am Ende immer eine Version im Zielordner.
pub fn fuehre_aus(argumente: &[String]) {
    let (Some(pid), Some(quelle), Some(ziel)) = (argumente.get(2).and_then(|p| p.parse::<u32>().ok()), argumente.get(3), argumente.get(4)) else {
        return;
    };
    let (quelle, ziel) = (PathBuf::from(quelle), PathBuf::from(ziel));
    let ergebnis = warte_auf_ende(pid).and_then(|()| anwenden(&quelle, &ziel, WARTEZEIT_STANDARD_MS));
    if let Err(fehler) = ergebnis {
        crate::grundlagen::fehler_log::log("Update", &fehler);
        rfd::MessageDialog::new()
            .set_level(rfd::MessageLevel::Error)
            .set_title("Update fehlgeschlagen")
            .set_description(format!("Das Update konnte nicht installiert werden. Die bisherige Version bleibt erhalten.\n\n{fehler}"))
            .show();
    }
    let _ = std::process::Command::new(ziel.join(EXE_NAME)).current_dir(&ziel).spawn();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader};
    use std::net::TcpListener;

    fn v(text: &str) -> Version {
        Version::parse(text).unwrap()
    }

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("fb-upd-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    // ── Version ──

    #[test]
    fn versionen_werden_gelesen_und_verglichen() {
        assert!(Version::parse("1.2.3").is_some() && Version::parse("1.2").is_some() && Version::parse("1.2.3.4").is_some());
        for falsch in ["", "1", "1.2.3.4.5", "a.b", "1..2", "1.2.x", "-1.2"] {
            assert!(Version::parse(falsch).is_none(), "{falsch}");
        }
        assert!(v("1.10.0") > v("1.9.9"));
        assert!(v("2.0") > v("1.99.99"));
        assert_eq!(v("1.2"), v("1.2")); // gleiche Schreibweise
        assert_eq!(v("1.2").cmp(&v("1.2.0")), std::cmp::Ordering::Equal);
        assert!(v("1.2.0.1") > v("1.2"));
        assert_eq!(v("1.2.3").to_string(), "1.2.3");
        assert!(Version::aktuell() > v("0.0"));
    }

    // ── Release ──

    fn release(tag: &str, zip_url: &str, pruef_url: &str, groesse: u64, extra: &str) -> String {
        let zip = paket_name(tag);
        format!(
            r#"{{"tag_name":"{tag}","body":"Neu:\n- Absteckung","draft":false,"prerelease":false{extra},
            "assets":[{{"name":"{zip}","browser_download_url":"{zip_url}","size":{groesse}}},
                      {{"name":"{zip}.sha256","browser_download_url":"{pruef_url}","size":64}}]}}"#
        )
    }

    const ZIP_URL: &str = "https://github.com/Jgerner01/feldbuch-nt/releases/download/v1.2.3/Feldbuch-NT-v1.2.3-win-x64.zip";

    #[test]
    fn neuere_release_wird_erkannt() {
        let json = release("v1.2.3", ZIP_URL, &format!("{ZIP_URL}.sha256"), 1000, "");
        let info = lese_release(&json, &v("0.1.0")).unwrap().unwrap();
        assert_eq!((info.version.to_string().as_str(), info.tag.as_str(), info.groesse), ("1.2.3", "v1.2.3", 1000));
        assert!(info.notizen.contains("Absteckung"));
        assert_eq!(info.zip_url, ZIP_URL);
        // Tag ohne „v“ geht auch
        let ohne_v = release("1.2.3", ZIP_URL, &format!("{ZIP_URL}.sha256"), 1000, "");
        assert!(lese_release(&ohne_v, &v("1.2.2")).unwrap().is_some());
    }

    #[test]
    fn nichts_zu_tun_bei_gleicher_oder_aelterer_version_entwurf_und_vorabversion() {
        let json = release("v1.2.3", ZIP_URL, &format!("{ZIP_URL}.sha256"), 1000, "");
        assert!(lese_release(&json, &v("1.2.3")).unwrap().is_none());
        assert!(lese_release(&json, &v("2.0.0")).unwrap().is_none());
        for flagge in ["draft", "prerelease"] {
            let j = json.replace(&format!("\"{flagge}\":false"), &format!("\"{flagge}\":true"));
            assert!(lese_release(&j, &v("0.1.0")).unwrap().is_none(), "{flagge}");
        }
        // Kein Versions-Tag (z. B. „nightly“)
        let j = json.replace("v1.2.3\",\"body", "nightly\",\"body");
        assert!(lese_release(&j, &v("0.1.0")).unwrap().is_none());
    }

    #[test]
    fn unvollstaendige_oder_unsichere_pakete_werden_abgelehnt() {
        let pruef = format!("{ZIP_URL}.sha256");
        // Prüfsumme fehlt
        let ohne = release("v1.2.3", ZIP_URL, &pruef, 1000, "").replace(".zip.sha256\"", ".zip.sha257\"");
        assert!(lese_release(&ohne, &v("0.1.0")).unwrap().is_none());
        // Fremder Host, http statt https, Host mit Zusatz, Größe 0 oder zu groß
        for url in [
            "https://evil.example/Feldbuch-NT-v1.2.3-win-x64.zip",
            "http://github.com/x.zip",
            "https://github.com.evil.example/x.zip",
            "https://github.com@evil.example/x.zip",
            "https://github.com:8443/x.zip",
        ] {
            let j = release("v1.2.3", url, &pruef, 1000, "");
            assert!(lese_release(&j, &v("0.1.0")).unwrap().is_none(), "{url}");
        }
        for groesse in [0, MAX_GROESSE + 1] {
            let j = release("v1.2.3", ZIP_URL, &pruef, groesse, "");
            assert!(lese_release(&j, &v("0.1.0")).unwrap().is_none(), "{groesse}");
        }
        assert!(lese_release("kein json", &v("0.1.0")).is_err());
        assert!(lese_release("{}", &v("0.1.0")).is_err());
    }

    #[test]
    fn github_adressen() {
        assert!(ist_github_url("https://github.com/a/b"));
        assert!(ist_github_url("https://github.com"));
        assert!(!ist_github_url("https://api.github.com/a"));
        assert!(!ist_github_url("ftp://github.com/a"));
    }

    // ── Mini-Server für die Netzwerktests ──

    /// Beantwortet Anfragen nach Pfad (GET); unbekannte Pfade mit 404. Läuft, bis der Test endet.
    fn starte_server(routen: Vec<(&'static str, Vec<u8>)>) -> String {
        let hoerer = TcpListener::bind("127.0.0.1:0").unwrap();
        let adresse = format!("http://{}", hoerer.local_addr().unwrap());
        std::thread::spawn(move || {
            for strom in hoerer.incoming().flatten() {
                let routen = routen.clone();
                std::thread::spawn(move || {
                    let mut strom = strom;
                    let mut leser = BufReader::new(strom.try_clone().unwrap());
                    let mut zeile = String::new();
                    if leser.read_line(&mut zeile).is_err() {
                        return;
                    }
                    let pfad = zeile.split_whitespace().nth(1).unwrap_or("/").to_string();
                    loop {
                        let mut kopf = String::new();
                        if leser.read_line(&mut kopf).unwrap_or(0) <= 2 {
                            break;
                        }
                    }
                    let antwort = match routen.iter().find(|(p, _)| *p == pfad) {
                        Some((_, inhalt)) => {
                            let mut a = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", inhalt.len()).into_bytes();
                            a.extend_from_slice(inhalt);
                            a
                        }
                        None => b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec(),
                    };
                    let _ = strom.write_all(&antwort);
                });
            }
        });
        adresse
    }

    fn baue_zip(dateien: &[(&str, &[u8])]) -> Vec<u8> {
        let mut puffer = std::io::Cursor::new(Vec::new());
        let mut schreiber = zip::ZipWriter::new(&mut puffer);
        let optionen = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        for (name, inhalt) in dateien {
            schreiber.start_file(*name, optionen).unwrap();
            schreiber.write_all(inhalt).unwrap();
        }
        schreiber.finish().unwrap();
        puffer.into_inner()
    }

    fn sha_von(daten: &[u8]) -> String {
        Sha256::digest(daten).iter().map(|b| format!("{b:02X}")).collect()
    }

    fn info_fuer(basis: &str, zip: &[u8]) -> UpdateInfo {
        UpdateInfo {
            version: v("9.9.9"),
            tag: "v9.9.9".into(),
            notizen: String::new(),
            zip_url: format!("{basis}/paket.zip"),
            pruefsummen_url: format!("{basis}/paket.zip.sha256"),
            groesse: zip.len() as u64,
        }
    }

    #[test]
    fn suche_behandelt_404_und_serverfehler() {
        let basis = starte_server(vec![("/ok", b"{\"tag_name\":\"v1.0.0\",\"assets\":[]}".to_vec())]);
        assert_eq!(suche_von(&format!("{basis}/fehlt"), &v("0.1.0")), Ok(None));
        // Gefunden, aber ohne Paket-Dateien: nichts anzubieten
        assert_eq!(suche_von(&format!("{basis}/ok"), &v("0.1.0")), Ok(None));
        // Nicht erreichbar
        assert!(suche_von("http://127.0.0.1:1/x", &v("0.1.0")).is_err());
    }

    #[test]
    fn download_pruefung_und_entpacken() {
        let zip = baue_zip(&[("feldbuch-nt.exe", b"EXE"), ("hilfe/liesmich.txt", b"Hallo")]);
        let basis = starte_server(vec![("/paket.zip", zip.clone()), ("/paket.zip.sha256", format!("{}  paket.zip\n", sha_von(&zip)).into_bytes())]);
        let ordner = temp("lade");
        let fortschritt = AtomicU64::new(0);
        let ordner_neu = lade(&info_fuer(&basis, &zip), &ordner, &fortschritt, &AtomicBool::new(false)).unwrap();
        assert_eq!(fs::read(ordner_neu.join(EXE_NAME)).unwrap(), b"EXE");
        assert_eq!(fs::read(ordner_neu.join("hilfe").join("liesmich.txt")).unwrap(), b"Hallo");
        assert_eq!(fortschritt.load(Ordering::Relaxed), 100);
        // Ein zweiter Lauf räumt den alten Ordner des Tags weg und klappt erneut
        assert!(lade(&info_fuer(&basis, &zip), &ordner, &fortschritt, &AtomicBool::new(false)).is_ok());
        raeume_auf(&ordner);
        assert!(!ordner.exists());
    }

    #[test]
    fn falsche_pruefsumme_groesse_und_abbruch() {
        let zip = baue_zip(&[("feldbuch-nt.exe", b"EXE")]);
        let ordner = temp("fehler");
        let nein = AtomicBool::new(false);
        let fortschritt = AtomicU64::new(0);
        // Falsche Prüfsumme
        let basis = starte_server(vec![("/paket.zip", zip.clone()), ("/paket.zip.sha256", "0".repeat(64).into_bytes())]);
        assert_eq!(lade(&info_fuer(&basis, &zip), &ordner, &fortschritt, &nein), Err(Fehler::Meldung("Die Prüfsumme des Updates stimmt nicht.".into())));
        // Ungültige Prüfsummendatei
        let basis = starte_server(vec![("/paket.zip", zip.clone()), ("/paket.zip.sha256", b"zu kurz".to_vec())]);
        assert_eq!(
            lade(&info_fuer(&basis, &zip), &ordner, &fortschritt, &nein),
            Err(Fehler::Meldung("Die Prüfsummendatei des Updates ist ungültig.".into()))
        );
        // Angekündigte Größe kleiner bzw. größer als geliefert
        let basis = starte_server(vec![("/paket.zip", zip.clone()), ("/paket.zip.sha256", sha_von(&zip).into_bytes())]);
        let mut info = info_fuer(&basis, &zip);
        info.groesse -= 1;
        assert_eq!(lade(&info, &ordner, &fortschritt, &nein), Err(Fehler::Meldung("Das Update ist größer als angekündigt.".into())));
        info.groesse += 5;
        assert_eq!(lade(&info, &ordner, &fortschritt, &nein), Err(Fehler::Meldung("Das Update wurde nicht vollständig heruntergeladen.".into())));
        // Abbruch durch den Benutzer
        assert_eq!(lade(&info_fuer(&basis, &zip), &ordner, &fortschritt, &AtomicBool::new(true)), Err(Fehler::Abgebrochen));
        // Fehlende Datei auf dem Server
        let basis = starte_server(vec![]);
        assert!(matches!(lade(&info_fuer(&basis, &zip), &ordner, &fortschritt, &nein), Err(Fehler::Meldung(m)) if m.contains("404")));
        raeume_auf(&ordner);
    }

    #[test]
    fn paket_ohne_eindeutige_exe_oder_mit_fremdem_pfad() {
        let ordner = temp("exe");
        let nein = AtomicBool::new(false);
        let fortschritt = AtomicU64::new(0);
        for (dateien, erwartet) in [
            (vec![("liesmich.txt", &b"x"[..])], "Das Update enthält keine eindeutige Programmdatei."),
            (vec![("a/feldbuch-nt.exe", &b"1"[..]), ("b/Feldbuch-NT.EXE", &b"2"[..])], "Das Update enthält keine eindeutige Programmdatei."),
            (vec![("../boese.txt", &b"x"[..])], "Das Update-Paket enthält einen unzulässigen Pfad: ../boese.txt"),
        ] {
            let zip = baue_zip(&dateien);
            let basis = starte_server(vec![("/paket.zip", zip.clone()), ("/paket.zip.sha256", sha_von(&zip).into_bytes())]);
            assert_eq!(lade(&info_fuer(&basis, &zip), &ordner, &fortschritt, &nein), Err(Fehler::Meldung(erwartet.into())));
        }
        assert!(!ordner.parent().unwrap().join("boese.txt").exists());
        raeume_auf(&ordner);
    }

    // ── Installation ──

    fn schreibe(pfad: &Path, inhalt: &str) {
        fs::create_dir_all(pfad.parent().unwrap()).unwrap();
        fs::write(pfad, inhalt).unwrap();
    }

    #[test]
    fn anwenden_ersetzt_nur_paketdateien_und_sichert_das_alte() {
        let wurzel = temp("anwenden");
        let (quelle, ziel) = (wurzel.join("neu"), wurzel.join("prog"));
        schreibe(&quelle.join(EXE_NAME), "neue exe");
        schreibe(&quelle.join("vorlagen").join("a.xml"), "neu a");
        schreibe(&ziel.join(EXE_NAME), "alte exe");
        schreibe(&ziel.join("Einstellungen.xml"), "meine Einstellungen");
        // Eine ältere Sicherung wird beim nächsten Update ersetzt
        schreibe(&ziel.join(SICHERUNGSORDNER).join("uralt.txt"), "x");
        anwenden(&quelle, &ziel, 1).unwrap();
        assert_eq!(fs::read_to_string(ziel.join(EXE_NAME)).unwrap(), "neue exe");
        assert_eq!(fs::read_to_string(ziel.join("vorlagen").join("a.xml")).unwrap(), "neu a");
        assert_eq!(fs::read_to_string(ziel.join("Einstellungen.xml")).unwrap(), "meine Einstellungen");
        assert_eq!(fs::read_to_string(ziel.join(SICHERUNGSORDNER).join(EXE_NAME)).unwrap(), "alte exe");
        assert!(!ziel.join(SICHERUNGSORDNER).join("uralt.txt").exists());
        assert!(!ziel.join(SICHERUNGSORDNER).join("vorlagen").exists(), "nur Vorhandenes wird gesichert");
        let _ = fs::remove_dir_all(wurzel);
    }

    #[test]
    fn anwenden_rollt_bei_fehler_zurueck() {
        let wurzel = temp("rollback");
        let (quelle, ziel) = (wurzel.join("neu"), wurzel.join("prog"));
        schreibe(&quelle.join(EXE_NAME), "neue exe");
        schreibe(&quelle.join("x").join("y.txt"), "neu"); // „x“ ist im Ziel eine Datei: das Anlegen scheitert
        schreibe(&ziel.join(EXE_NAME), "alte exe");
        schreibe(&ziel.join("x"), "ich bin eine Datei");
        let fehler = anwenden(&quelle, &ziel, 1).unwrap_err();
        assert!(!fehler.is_empty());
        assert_eq!(fs::read_to_string(ziel.join(EXE_NAME)).unwrap(), "alte exe", "vorheriger Stand wiederhergestellt");
        assert_eq!(fs::read_to_string(ziel.join("x")).unwrap(), "ich bin eine Datei");
        assert!(!ziel.join("x").join("y.txt").exists());
        let _ = fs::remove_dir_all(wurzel);
    }

    #[test]
    fn anwenden_braucht_eine_exe_im_paket() {
        let wurzel = temp("ohneexe");
        schreibe(&wurzel.join("neu").join("liesmich.txt"), "x");
        assert_eq!(anwenden(&wurzel.join("neu"), &wurzel.join("prog"), 1), Err("Das Update enthält keine Programmdatei.".to_string()));
        let _ = fs::remove_dir_all(wurzel);
    }

    #[test]
    fn schreibbarkeit_und_entwicklungsbuild() {
        let d = temp("schreib");
        assert!(ist_ordner_schreibbar(&d));
        assert!(!ist_ordner_schreibbar(&d.join("gibt-es-nicht")));
        // Die Testprogramme liegen unter „target“ und sind keine veröffentlichte Version
        assert!(!ist_veroeffentlicht());
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn prozess_pruefung() {
        assert!(prozess_laeuft(std::process::id()), "der eigene Prozess läuft");
        assert!(!prozess_laeuft(4_000_000_000), "unbekannte Prozess-ID");
        assert!(warte_auf_ende(4_000_000_000).is_ok());
    }
}
