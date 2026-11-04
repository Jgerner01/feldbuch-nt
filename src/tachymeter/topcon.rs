// Zweck:    Parser für Topcon GTS/GPT (Fernsteuerungsformat). Winkel kommen in Grad und werden in gon umgerechnet.
// Hardware: Topcon GTS/GPT
// Autor:    Johann Gerner

use super::messung::{MessungsTyp, TachymeterMessung};

/// Faktor Grad → gon.
const GRAD2GON: f64 = 400.0 / 360.0;
/// Quellname Topcon.
const QUELLE_TOPCON: &str = "Topcon";

/// Kann diese Zeile eine Topcon-Ausgabe sein?
pub fn kann_verarbeiten_topcon(zeile: &str) -> bool {
    let z = zeile.trim_start();
    if z.is_empty() {
        return false;
    }
    let gross = z.to_uppercase();
    gross.starts_with("SS-") || (gross.contains("H:") && gross.contains("V:"))
}

/// Parst eine Topcon-Zeile (H:, V:, D:).
pub fn parse_topcon_zeile(zeile: &str) -> Option<TachymeterMessung> {
    if !kann_verarbeiten_topcon(zeile) {
        return None;
    }
    let hz = extrahiere_wert(zeile, "H:");
    let v = extrahiere_wert(zeile, "V:");
    let sd = extrahiere_wert(zeile, "D:");
    if hz.is_none() && v.is_none() {
        return None;
    }
    let mut m = TachymeterMessung::neu(QUELLE_TOPCON, zeile);
    m.typ = if sd.is_some() { MessungsTyp::Vollmessung } else { MessungsTyp::Winkel };
    m.hz_gon = hz.map(|g| g * GRAD2GON);
    m.v_gon = v.map(|g| g * GRAD2GON);
    m.schraegstrecke_m = sd;
    Some(m)
}

/// Wert nach einem Schlüssel (ohne Beachtung der Groß-/Kleinschreibung).
fn extrahiere_wert(zeile: &str, schluessel: &str) -> Option<f64> {
    // Die Indizes gelten nur, solange Groß-/Kleinschreibung die Länge nicht ändert (ASCII)
    if !zeile.is_ascii() {
        return None;
    }
    let gross = zeile.to_uppercase();
    let idx = gross.find(&schluessel.to_uppercase())?;
    let rest = zeile[idx + schluessel.len()..].trim_start_matches(' ');
    let ende = rest.find(|c: char| !(c.is_ascii_digit() || matches!(c, '.' | '-' | '+'))).unwrap_or(rest.len());
    if ende == 0 {
        return None;
    }
    rest[..ende].parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn topcon_variante_a_und_b() {
        let m = parse_topcon_zeile("SS-H:90.0000 V:90.0000 D: 25.1234").unwrap();
        assert_eq!(m.typ, MessungsTyp::Vollmessung);
        assert!((m.hz_gon.unwrap() - 100.0).abs() < 1e-9);
        assert_eq!(m.schraegstrecke_m, Some(25.1234));
        let w = parse_topcon_zeile("h:180.0 v:90.0").unwrap();
        assert_eq!(w.typ, MessungsTyp::Winkel);
        assert!((w.hz_gon.unwrap() - 200.0).abs() < 1e-9);
        assert!(parse_topcon_zeile("irgendwas").is_none());
        assert!(!kann_verarbeiten_topcon(""));
    }
}
