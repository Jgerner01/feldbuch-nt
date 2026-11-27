// Zweck:    Satellitenvorhersage für den Messort aus den empfangenen GPS-Bahndaten: sichtbare Satelliten und PDOP für
//           die nächsten Stunden, damit enge Lagen (Hauswände, Wald) in Zeiten mit guter Geometrie gelegt werden können
//           (GNSS.md 12.13)
// Hardware: GNSS-Empfänger u-blox ZED-F9P (Bahndaten aus UBX-RXM-SFRBX)
// Autor:    Johann Gerner
//
// Es werden nur GPS-Satelliten vorhergesagt (für andere Systeme liegen keine Bahndaten vor). Der Empfänger nutzt
// zusätzlich Galileo, GLONASS und BeiDou; der tatsächliche PDOP ist deshalb meist deutlich besser. Die Vorhersage
// zeigt daher vor allem, wann die GPS-Geometrie schwach ist. Broadcast-Bahndaten taugen für einige Stunden.

use std::collections::BTreeMap;

use crate::gnss::kern::dop::{self, Direction};
use crate::gnss::kern::geo::{Geographic, geo_to_ecef};
use crate::gnss::kern::spp::{self, Ephemeris};

/// Sekunden je Minute.
const S_PRO_MIN: f64 = 60.0;
/// Systemkennung GPS (wie UBX).
const SYSTEM_GPS: u8 = 0;
/// Ab diesem PDOP gilt die Geometrie als schwach bzw. schlecht.
pub const PDOP_SCHWACH: f64 = 3.0;
pub const PDOP_SCHLECHT: f64 = 6.0;

/// Ein Zeitpunkt der Vorhersage.
#[derive(Clone, Debug, PartialEq)]
pub struct Zeitpunkt {
    /// Minuten ab jetzt
    pub minuten: f64,
    pub satelliten: usize,
    /// `None` bei weniger als vier Satelliten
    pub pdop: Option<f64>,
}

/// Vorhersage ab der GPS-Wochensekunde `tow_s` über `dauer_min` in Schritten von `schritt_min`.
pub fn berechne(bahndaten: &BTreeMap<u8, Ephemeris>, ort: &Geographic, tow_s: f64, dauer_min: f64, schritt_min: f64, maske_grad: f64) -> Vec<Zeitpunkt> {
    if schritt_min <= 0.0 || dauer_min < 0.0 {
        return Vec::new();
    }
    let empfaenger = geo_to_ecef(ort);
    let schritte = (dauer_min / schritt_min).floor() as usize;
    (0..=schritte)
        .map(|i| {
            let minuten = i as f64 * schritt_min;
            let t = tow_s + minuten * S_PRO_MIN;
            let richtungen: Vec<Direction> = bahndaten
                .values()
                .filter(|e| e.health == 0)
                .filter_map(|e| {
                    let (sat, _) = spp::position(e, t);
                    let [o, n, h] = spp::enu(empfaenger, sat);
                    let elev = h.atan2(o.hypot(n)).to_degrees();
                    (elev >= maske_grad).then(|| Direction { azim_deg: o.atan2(n).to_degrees().rem_euclid(360.0), elev_deg: elev, system: SYSTEM_GPS })
                })
                .collect();
            let pdop = dop::compute(&richtungen, false).map(|d| d.p);
            Zeitpunkt { minuten, satelliten: richtungen.len(), pdop }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Kreisbahn ohne Störungen mit gegebener Bahnebene und Startanomalie.
    fn bahn(omega0: f64, m0: f64) -> Ephemeris {
        Ephemeris { sqrt_a: 26_560_000.0_f64.sqrt(), i0: 55.0_f64.to_radians(), omega0, m0, ..Default::default() }
    }

    #[test]
    fn vorhersage_zaehlt_sichtbare_satelliten() {
        let mut daten = BTreeMap::new();
        let mut prn = 1;
        for ebene in 0..6 {
            for platz in 0..4 {
                daten.insert(prn, bahn(f64::from(ebene) * 1.047, f64::from(platz) * 1.571 + f64::from(ebene) * 0.3));
                prn += 1;
            }
        }
        let ort = Geographic { lat: 48.4, lon: 10.0, h: 500.0 };
        let v = berechne(&daten, &ort, 0.0, 120.0, 30.0, 10.0);
        assert_eq!(v.len(), 5);
        assert_eq!(v[2].minuten, 60.0);
        // bei 24 Satelliten sind immer einige über dem Horizont, nie alle
        assert!(v.iter().all(|z| z.satelliten > 3 && z.satelliten < 24));
        assert!(v.iter().all(|z| z.pdop.is_some_and(|p| p > 0.5)));
        assert!(berechne(&daten, &ort, 0.0, 60.0, 0.0, 10.0).is_empty());
    }
}
