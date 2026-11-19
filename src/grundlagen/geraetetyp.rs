// Zweck:    Erkennung, ob das Programm auf einem Panasonic Toughpad/Toughbook (Feldrechner) läuft
// Hardware: Panasonic Toughpad FZ-xx / Toughbook CF-xx (Auslesen der BIOS-Angaben aus der Registry)
// Autor:    Johann Gerner

use std::sync::OnceLock;

/// Registry-Schlüssel mit den BIOS-Angaben (Hersteller, Produktname).
#[cfg(windows)]
const BIOS_SCHLUESSEL: &str = r"HKLM\HARDWARE\DESCRIPTION\System\BIOS";
/// Namen der Registry-Werte.
const WERT_HERSTELLER: &str = "SystemManufacturer";
const WERT_PRODUKT: &str = "SystemProductName";
/// Herstellername (Kleinschreibung) und Modellpräfixe (Kleinschreibung) der Toughpad-/Toughbook-Reihen.
const HERSTELLER_PANASONIC: &str = "panasonic";
const MODELL_PRAEFIXE: [&str; 2] = ["fz-", "cf-"];

/// Liest den Wert `name` aus der Ausgabe von `reg query` (Zeilenformat: `Name  REG_SZ  Wert`).
fn registry_wert(ausgabe: &str, name: &str) -> Option<String> {
    ausgabe.lines().find_map(|zeile| {
        let rest = zeile.trim().strip_prefix(name)?;
        let wert = rest.trim_start().strip_prefix("REG_SZ")?;
        Some(wert.trim().to_string())
    })
}

/// Wertet die BIOS-Angaben aus: Hersteller Panasonic und Modell FZ-… oder CF-….
pub fn ist_toughpad_aus_bios(ausgabe: &str) -> bool {
    let hersteller = registry_wert(ausgabe, WERT_HERSTELLER).unwrap_or_default().to_lowercase();
    let produkt = registry_wert(ausgabe, WERT_PRODUKT).unwrap_or_default().to_lowercase();
    hersteller.contains(HERSTELLER_PANASONIC) && MODELL_PRAEFIXE.iter().any(|p| produkt.starts_with(p))
}

#[cfg(windows)]
fn lese_bios() -> Option<String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let ausgabe = std::process::Command::new("reg").args(["query", BIOS_SCHLUESSEL]).creation_flags(CREATE_NO_WINDOW).output().ok()?;
    Some(String::from_utf8_lossy(&ausgabe.stdout).into_owned())
}

#[cfg(not(windows))]
fn lese_bios() -> Option<String> {
    None
}

/// Läuft das Programm auf einem Panasonic Toughpad/Toughbook? Das Ergebnis wird einmalig ermittelt.
pub fn ist_panasonic_toughpad() -> bool {
    static ERGEBNIS: OnceLock<bool> = OnceLock::new();
    *ERGEBNIS.get_or_init(|| lese_bios().is_some_and(|text| ist_toughpad_aus_bios(&text)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bios(hersteller: &str, produkt: &str) -> String {
        format!("\nHKEY_LOCAL_MACHINE\\HARDWARE\\DESCRIPTION\\System\\BIOS\n    BIOSVendor    REG_SZ    X\n    SystemManufacturer    REG_SZ    {hersteller}\n    SystemProductName    REG_SZ    {produkt}\n")
    }

    #[test]
    fn toughpad_wird_erkannt() {
        assert!(ist_toughpad_aus_bios(&bios("Panasonic Corporation", "FZ-M1CDAAZBA")));
        assert!(ist_toughpad_aus_bios(&bios("Panasonic Corporation", "CF-54G1234")));
    }

    #[test]
    fn andere_geraete_werden_abgelehnt() {
        assert!(!ist_toughpad_aus_bios(&bios("Dell Inc.", "Latitude 5420")));
        assert!(!ist_toughpad_aus_bios(&bios("Panasonic Corporation", "Lumix")));
        assert!(!ist_toughpad_aus_bios(""));
    }
}
