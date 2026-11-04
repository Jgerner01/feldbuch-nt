// Zweck:    Zahlenformatierung und -parsen (invariant für Dateien, deutsch für die Anzeige)
// Hardware: keine
// Autor:    Johann Gerner

/// Zusätzliche Stellen zur Erkennung einer exakten Rundungsmitte.
const PRUEF_STELLEN: usize = 40;

/// Ist der Betrag exakt die Mitte zwischen zwei Werten mit `stellen` Nachkommastellen (z. B. 1,25 bei einer Stelle)?
fn ist_exakte_mitte(wert: f64, stellen: usize) -> bool {
    let text = format!("{:.*}", stellen + PRUEF_STELLEN, wert.abs());
    let Some((_, nachkomma)) = text.split_once('.') else {
        return false;
    };
    let ziffern = nachkomma.as_bytes();
    ziffern.get(stellen) == Some(&b'5') && ziffern[stellen + 1..].iter().all(|z| *z == b'0')
}

/// Formatiert mit fester Nachkommastellenzahl und Punkt (Dateiformat, wie C# "F3" invariant).
/// Exakte Rundungsmitten werden wie in .NET von null weg gerundet (1,25 → 1,3), Rust rundet sie auf gerade Ziffern.
pub fn f(wert: f64, stellen: usize) -> String {
    let wert = if wert.is_finite() && wert != 0.0 && ist_exakte_mitte(wert, stellen) {
        // Ein Schritt in der letzten Binärstelle weg von null genügt, um die Mitte zu überschreiten
        f64::from_bits(wert.to_bits() + 1)
    } else {
        wert
    };
    format!("{wert:.stellen$}")
}

/// Wert mit Vorzeichen, Null mit Pluszeichen (wie .NET „+0.0000;-0.0000“).
pub fn plus_minus(wert: f64, stellen: usize) -> String {
    if wert < 0.0 { f(wert, stellen) } else { format!("+{}", f(wert, stellen)) }
}

/// Formatiert mit Dezimalkomma für die Anzeige.
pub fn de(wert: f64, stellen: usize) -> String {
    f(wert, stellen).replace('.', ",")
}

/// Liest eine Dezimalzahl mit Punkt oder Komma; leere oder ungültige Eingaben ergeben `None`.
pub fn parse_zahl(text: &str) -> Option<f64> {
    let bereinigt = text.trim().replace(',', ".");
    if bereinigt.is_empty() {
        return None;
    }
    bereinigt.parse::<f64>().ok().filter(|v| v.is_finite())
}

/// Wie `parse_zahl`, aber mit Ersatzwert.
pub fn parse_zahl_oder(text: &str, ersatz: f64) -> f64 {
    parse_zahl(text).unwrap_or(ersatz)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plus_minus_zeigt_immer_ein_vorzeichen() {
        assert_eq!(plus_minus(1.5, 2), "+1.50");
        assert_eq!(plus_minus(-1.5, 2), "-1.50");
        assert_eq!(plus_minus(0.0, 1), "+0.0");
    }

    #[test]
    fn rundet_exakte_mitten_wie_dotnet() {
        assert_eq!(f(1.25, 1), "1.3");
        assert_eq!(f(-1.25, 1), "-1.3");
        assert_eq!(f(0.125, 2), "0.13");
        assert_eq!(f(2.5, 0), "3");
        assert_eq!(f(-0.5, 0), "-1");
        // Keine exakten Mitten: normales Runden
        assert_eq!(f(1.35, 1), "1.4");
        assert_eq!(f(1.249, 1), "1.2");
        assert_eq!(f(0.0, 3), "0.000");
        assert_eq!(f(100.0, 2), "100.00");
        assert_eq!(f(f64::NAN, 2), "NaN");
    }

    #[test]
    fn formatiert_und_liest() {
        assert_eq!(f(1.23456, 3), "1.235");
        assert_eq!(de(1.5, 2), "1,50");
        assert_eq!(parse_zahl("12,5"), Some(12.5));
        assert_eq!(parse_zahl(" 7.25 "), Some(7.25));
        assert_eq!(parse_zahl(""), None);
        assert_eq!(parse_zahl("abc"), None);
        assert_eq!(parse_zahl("NaN"), None);
    }
}
