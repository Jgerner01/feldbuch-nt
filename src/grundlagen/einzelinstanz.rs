// Zweck:    Verhindert, dass Feldbuch-NT zweimal gestartet wird: zwei Instanzen würden dieselben Projektdateien und
//           dieselbe serielle Schnittstelle (Tachymeter, GNSS-Empfänger) gleichzeitig benutzen. Läuft das Programm
//           bereits, wird dessen Fenster in den Vordergrund geholt und ein Hinweis gezeigt.
// Hardware: keine (Windows: benannter Mutex, Fenstersuche über user32)
// Autor:    Johann Gerner
//
// Der Mutex gehört dem Prozess und wird von Windows beim Programmende freigegeben, auch nach einem Absturz.
// Der Installationsmodus nach einem Update (`--update-installieren`) prüft nicht: er wartet ohnehin auf das Ende
// der alten Version und startet danach die neue.

/// Name des Mutex (je Windows-Sitzung).
#[cfg(windows)]
const MUTEX_NAME: &str = "Local\\Feldbuch-NT-Einzelinstanz";
/// Anfang des Titels der Programmfenster.
#[cfg(windows)]
const TITEL_ANFANG: &str = "Feldbuch";
/// Hinweis, wenn das Programm bereits läuft.
#[cfg(windows)]
const HINWEIS_TITEL: &str = "Feldbuch-NT";
#[cfg(windows)]
const HINWEIS_TEXT: &str = "Feldbuch-NT läuft bereits.\n\nDas vorhandene Fenster wurde in den Vordergrund geholt.";

/// Belegt die Einzelinstanz. `false`: das Programm läuft bereits (Fenster wurde aktiviert, Hinweis gezeigt).
pub fn belegen() -> bool {
    #[cfg(windows)]
    {
        windows::belegen()
    }
    #[cfg(not(windows))]
    {
        true
    }
}

#[cfg(windows)]
mod windows {
    use super::{HINWEIS_TEXT, HINWEIS_TITEL, MUTEX_NAME, TITEL_ANFANG};
    use std::ffi::c_void;
    use std::path::Path;

    type Handle = *mut c_void;
    type Hwnd = *mut c_void;

    const ERROR_ALREADY_EXISTS: u32 = 183;
    const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
    const SW_RESTORE: i32 = 9;
    const MB_OK: u32 = 0x0000_0000;
    const MB_ICONINFORMATION: u32 = 0x0000_0040;
    const MB_SETFOREGROUND: u32 = 0x0001_0000;
    /// Puffergröße für Fenstertitel und Pfade (UTF-16-Zeichen).
    const PUFFER: usize = 512;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn CreateMutexW(attribute: *const c_void, besitzer: i32, name: *const u16) -> Handle;
        fn GetLastError() -> u32;
        fn GetCurrentProcessId() -> u32;
        fn OpenProcess(zugriff: u32, erben: i32, pid: u32) -> Handle;
        fn QueryFullProcessImageNameW(prozess: Handle, flags: u32, pfad: *mut u16, laenge: *mut u32) -> i32;
        fn CloseHandle(handle: Handle) -> i32;
    }

    #[link(name = "user32")]
    unsafe extern "system" {
        fn EnumWindows(rueckruf: unsafe extern "system" fn(Hwnd, isize) -> i32, daten: isize) -> i32;
        fn GetWindowTextW(fenster: Hwnd, text: *mut u16, laenge: i32) -> i32;
        fn GetWindowThreadProcessId(fenster: Hwnd, pid: *mut u32) -> u32;
        fn IsWindowVisible(fenster: Hwnd) -> i32;
        fn IsIconic(fenster: Hwnd) -> i32;
        fn ShowWindow(fenster: Hwnd, befehl: i32) -> i32;
        fn SetForegroundWindow(fenster: Hwnd) -> i32;
        fn MessageBoxW(fenster: Hwnd, text: *const u16, titel: *const u16, art: u32) -> i32;
    }

    fn breit(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(std::iter::once(0)).collect()
    }

    pub fn belegen() -> bool {
        let name = breit(MUTEX_NAME);
        // SAFETY: gültiger, nullterminierter Name; ohne Sicherheitsattribute. Das Handle bleibt bis zum
        // Programmende offen (absichtlich nicht geschlossen), damit der Mutex belegt bleibt.
        let handle = unsafe { CreateMutexW(std::ptr::null(), 0, name.as_ptr()) };
        // SAFETY: unmittelbar nach CreateMutexW gelesen
        let schon_da = unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;
        if handle.is_null() || !schon_da {
            // ohne Mutex (Fehler beim Anlegen) lieber starten als den Benutzer aussperren
            return true;
        }
        let gefunden = aktiviere_vorhandenes_fenster();
        let text = if gefunden { HINWEIS_TEXT.to_string() } else { HINWEIS_TEXT.lines().next().unwrap_or_default().to_string() };
        let (text, titel) = (breit(&text), breit(HINWEIS_TITEL));
        // SAFETY: gültige, nullterminierte Zeichenketten; ohne Elternfenster
        unsafe { MessageBoxW(std::ptr::null_mut(), text.as_ptr(), titel.as_ptr(), MB_OK | MB_ICONINFORMATION | MB_SETFOREGROUND) };
        false
    }

    /// Dateiname der Programmdatei eines Prozesses (klein geschrieben).
    fn programmname(pid: u32) -> Option<String> {
        // SAFETY: nur lesender Zugriff; das Handle wird unten geschlossen
        let prozess = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        if prozess.is_null() {
            return None;
        }
        let mut puffer = [0u16; PUFFER];
        let mut laenge = PUFFER as u32;
        // SAFETY: Puffer und Länge passen zusammen
        let ok = unsafe { QueryFullProcessImageNameW(prozess, 0, puffer.as_mut_ptr(), &mut laenge) } != 0;
        // SAFETY: gültiges Handle aus OpenProcess
        unsafe { CloseHandle(prozess) };
        ok.then(|| {
            let pfad = String::from_utf16_lossy(&puffer[..laenge as usize]);
            Path::new(&pfad).file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default()
        })
    }

    /// Sucht das sichtbare Hauptfenster der anderen Instanz (Titel beginnt mit „Feldbuch“, gleiche Programmdatei).
    unsafe extern "system" fn pruefe_fenster(fenster: Hwnd, daten: isize) -> i32 {
        // SAFETY: `daten` ist der Zeiger auf das Suchergebnis aus `aktiviere_vorhandenes_fenster`
        let ziel = unsafe { &mut *(daten as *mut Option<Hwnd>) };
        // SAFETY: `fenster` stammt von EnumWindows
        if unsafe { IsWindowVisible(fenster) } == 0 {
            return 1;
        }
        let mut puffer = [0u16; PUFFER];
        // SAFETY: Puffer und Länge passen zusammen
        let n = unsafe { GetWindowTextW(fenster, puffer.as_mut_ptr(), PUFFER as i32) };
        let titel = String::from_utf16_lossy(&puffer[..n.max(0) as usize]);
        if !titel.starts_with(TITEL_ANFANG) {
            return 1;
        }
        let mut pid = 0u32;
        // SAFETY: gültiges Fenster und Zeiger auf eine lokale Variable
        unsafe { GetWindowThreadProcessId(fenster, &mut pid) };
        // SAFETY: liest nur die eigene Prozesskennung
        let eigene = unsafe { GetCurrentProcessId() };
        let eigener_name = std::env::current_exe().ok().and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_lowercase()));
        if pid != eigene && programmname(pid).is_some_and(|n| Some(n) == eigener_name) {
            *ziel = Some(fenster);
            return 0;
        }
        1
    }

    /// Holt das Fenster der laufenden Instanz nach vorn; `true`, wenn eines gefunden wurde.
    fn aktiviere_vorhandenes_fenster() -> bool {
        let mut gefunden: Option<Hwnd> = None;
        // SAFETY: der Rückruf schreibt nur in `gefunden`, das während des Aufrufs lebt
        unsafe { EnumWindows(pruefe_fenster, &mut gefunden as *mut Option<Hwnd> as isize) };
        let Some(fenster) = gefunden else {
            return false;
        };
        // SAFETY: gültiges Fenster aus EnumWindows
        unsafe {
            if IsIconic(fenster) != 0 {
                ShowWindow(fenster, SW_RESTORE);
            }
            SetForegroundWindow(fenster);
        }
        true
    }
}
