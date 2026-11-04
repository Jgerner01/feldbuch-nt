// Zweck:    Messdatenmodell für GNSS-Empfänger (NMEA 0183: GGA, GSA, GST)
// Hardware: GNSS-Empfänger über COM-Port (RS-232 oder Bluetooth-SPP)
// Autor:    Johann Gerner

use chrono::NaiveDateTime;

use crate::daten::modelle::jetzt;
use crate::grundlagen::format::f;

/// Fix-Typ einer GNSS-Messung (NMEA-Codes).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GnssFixTyp {
    NoFix = 0,
    Gps = 1,
    /// Inklusive SAPOS EPS / EGNOS.
    Dgps = 2,
    /// SAPOS HEPS (cm-Genauigkeit).
    RtkFixed = 4,
    RtkFloat = 5,
    Unbekannt = 9,
}

impl GnssFixTyp {
    /// Lesbarer Name für das Protokoll.
    pub fn name(self) -> &'static str {
        match self {
            Self::Gps => "GPS",
            Self::Dgps => "DGPS",
            Self::RtkFixed => "RTK_Fixed",
            Self::RtkFloat => "RTK_Float",
            Self::NoFix => "NoFix",
            Self::Unbekannt => "Unknown",
        }
    }
}

/// Vollständiger GNSS-Messdatensatz.
#[derive(Clone, Debug)]
pub struct GnssMessung {
    pub zeitstempel: NaiveDateTime,
    pub quelle: String,
    pub breite_deg: Option<f64>,
    pub laenge_deg: Option<f64>,
    pub hoehe_m: Option<f64>,
    pub horiz_genauigkeit_m: Option<f64>,
    pub vert_genauigkeit_m: Option<f64>,
    pub hdop: Option<f64>,
    pub pdop: Option<f64>,
    pub fix_typ: GnssFixTyp,
    pub satelliten: Option<u32>,
}

impl Default for GnssMessung {
    fn default() -> Self {
        Self {
            zeitstempel: jetzt(),
            quelle: "NMEA".to_string(),
            breite_deg: None,
            laenge_deg: None,
            hoehe_m: None,
            horiz_genauigkeit_m: None,
            vert_genauigkeit_m: None,
            hdop: None,
            pdop: None,
            fix_typ: GnssFixTyp::NoFix,
            satelliten: None,
        }
    }
}

impl GnssMessung {
    /// Liegt eine verwertbare Position vor?
    pub fn hat_position(&self) -> bool {
        self.breite_deg.is_some() && self.laenge_deg.is_some() && self.fix_typ != GnssFixTyp::NoFix
    }

    /// Qualitätshinweis für die Statuszeile.
    pub fn qualitaet_info(&self) -> String {
        if !self.hat_position() {
            return "Kein Fix".to_string();
        }
        let sat = self.satelliten.map_or_else(|| "?".to_string(), |s| s.to_string());
        let mut basis = format!("{}  Sat={sat}", self.fix_typ.name());
        if let Some(h) = self.horiz_genauigkeit_m {
            basis.push_str(&format!("  σH={} m", f(h, 3)));
        } else if let Some(d) = self.hdop {
            basis.push_str(&format!("  HDOP={}", f(d, 1)));
        }
        basis
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qualitaetsinfo() {
        let mut g = GnssMessung::default();
        assert_eq!(g.qualitaet_info(), "Kein Fix");
        g.breite_deg = Some(49.0);
        g.laenge_deg = Some(11.0);
        g.fix_typ = GnssFixTyp::RtkFixed;
        g.satelliten = Some(14);
        g.horiz_genauigkeit_m = Some(0.012);
        assert_eq!(g.qualitaet_info(), "RTK_Fixed  Sat=14  σH=0.012 m");
        g.horiz_genauigkeit_m = None;
        g.hdop = Some(0.9);
        assert_eq!(g.qualitaet_info(), "RTK_Fixed  Sat=14  HDOP=0.9");
    }
}
