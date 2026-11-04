// Zweck:    Berechnung von Neupunktkoordinaten aus polaren Messungen (Rechtssystem R/H, Winkel in gon),
//           Prüfung der Stationierungsqualität
// Hardware: keine
// Autor:    Johann Gerner

use std::f64::consts::PI;

use crate::daten::modelle::{NeupunktErgebnis, NeupunktRohdaten, StationierungsErgebnis};
use crate::daten::rechenparameter::Rechenparameter;
use crate::rechnen::polar::{self, ermittle_massstab};

/// Umrechnung gon → rad.
const GON2RAD: f64 = PI / 200.0;
/// Vollkreis in gon.
const VOLLKREIS_GON: f64 = 400.0;

/// Ist die Stationierung konvergiert und enthält sie nur gültige Werte?
pub fn ist_verwendbar(e: Option<&StationierungsErgebnis>) -> bool {
    e.is_some_and(|e| {
        e.konvergiert
            && e.r.is_finite()
            && e.h.is_finite()
            && e.hoehe.is_finite()
            && e.orientierung_gon.is_finite()
            && e.massstab.is_finite()
            && e.massstab > 0.0
            && e.s0_mm.is_finite()
            && e.s0_mm >= 0.0
    })
}

/// Prüft die Stationierung und liefert einen Fehlertext, wenn sie nicht verwendbar ist.
pub fn pruefe_station(e: &StationierungsErgebnis) -> Result<(), String> {
    if ist_verwendbar(Some(e)) {
        Ok(())
    } else {
        Err("Die Stationierung ist nicht konvergiert oder enthält ungültige Werte. \
             Bitte Anschlusspunkte und Messungen prüfen."
            .to_string())
    }
}

/// Berechnet die Koordinaten eines Neupunktes aus einer Polarmessung und der Stationierung.
/// Liefert `None`, wenn keine Strecke vorliegt.
///
/// `massstab`: fester Maßstab oder `None` (dann nach Rechenparameter).
/// `instr_hoehe`: Instrumentenhöhe [m] der aktiven Station.
pub fn berechnen(
    rohdaten: &NeupunktRohdaten,
    station: &StationierungsErgebnis,
    params: &Rechenparameter,
    massstab: Option<f64>,
    instr_hoehe: f64,
) -> Option<NeupunktErgebnis> {
    if rohdaten.strecke_m <= 0.0 {
        return None;
    }
    let hz = rohdaten.hz_gon;
    let v = rohdaten.v_gon;
    let strecke = rohdaten.strecke_m;
    let zh = rohdaten.zielhoehe_m;

    // Orientierte Richtung [gon, 0..400]
    let mut hz_ori = (hz + station.orientierung_gon) % VOLLKREIS_GON;
    if hz_ori < 0.0 {
        hz_ori += VOLLKREIS_GON;
    }

    // Horizontaldistanz: D_h = S * sin(V)
    let d_horiz = strecke * (v * GON2RAD).sin();

    let m = massstab.unwrap_or_else(|| ermittle_massstab(params, Some(station)).wert);
    let (r, h) = polar::lage(
        station.r,
        station.h,
        hz_ori,
        d_horiz,
        m,
        rohdaten.exz_laengs_m,
        rohdaten.exz_quer_m,
    );

    // Höhe: nur bei 3D-Berechnung mit gültiger Standpunkthöhe
    let ist_3d = params.berechnung_3d && (station.hoehe != 0.0 || station.ist_lokales_system);
    let hoehe = if ist_3d {
        // Δh = S * cos(V) + ih − zh
        station.hoehe + strecke * (v * GON2RAD).cos() + instr_hoehe - zh
    } else {
        0.0
    };

    Some(NeupunktErgebnis {
        punkt_nr: rohdaten.punkt_nr.clone(),
        code: rohdaten.code.clone(),
        r,
        h,
        hoehe,
        ist_3d,
        hz_orientiert: hz_ori,
        hz_roh: hz,
        v_gon: v,
        strecke_m: strecke,
        zielhoehe_m: zh,
        zeitstempel: rohdaten.zeitstempel,
        standpunkt_nr: rohdaten.standpunkt_nr.clone(),
        verwendete_orientierung: station.orientierung_gon,
        exz_laengs_m: rohdaten.exz_laengs_m,
        exz_quer_m: rohdaten.exz_quer_m,
        verwendeter_massstab: m,
        quelle: String::new(),
    })
}

/// Berechnet alle Neupunkte eines Standpunktes mit der aktuellen Stationierung neu.
pub fn berechne_alle<'a>(
    rohdaten: impl IntoIterator<Item = &'a NeupunktRohdaten>,
    station: &StationierungsErgebnis,
    standpunkt_nr: &str,
    params: &Rechenparameter,
    instr_hoehe: f64,
) -> Vec<NeupunktErgebnis> {
    rohdaten
        .into_iter()
        .filter(|r| r.standpunkt_nr == standpunkt_nr)
        .filter_map(|r| berechnen(r, station, params, None, instr_hoehe))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daten::modelle::jetzt;

    fn station() -> StationierungsErgebnis {
        StationierungsErgebnis {
            r: 1000.0,
            h: 2000.0,
            hoehe: 100.0,
            orientierung_gon: 50.0,
            massstab: 1.0,
            konvergiert: true,
            ..StationierungsErgebnis::default()
        }
    }

    fn roh(hz: f64, v: f64, s: f64) -> NeupunktRohdaten {
        NeupunktRohdaten {
            punkt_nr: "N1".into(),
            hz_gon: hz,
            v_gon: v,
            strecke_m: s,
            zielhoehe_m: 1.5,
            zeitstempel: jetzt(),
            standpunkt_nr: "S1".into(),
            ..NeupunktRohdaten::default()
        }
    }

    #[test]
    fn neupunkt_lage_und_hoehe() {
        // Hz roh 50 + Orientierung 50 = 100 gon (Osten), horizontal gemessen (V = 100 gon)
        let e = berechnen(&roh(50.0, 100.0, 40.0), &station(), &Rechenparameter::default(), None, 1.6).unwrap();
        assert!((e.r - 1040.0).abs() < 1e-9);
        assert!((e.h - 2000.0).abs() < 1e-9);
        assert!(e.ist_3d);
        // Δh = 0 + 1,6 − 1,5
        assert!((e.hoehe - 100.1).abs() < 1e-9);
        assert!((e.hz_orientiert - 100.0).abs() < 1e-9);
    }

    #[test]
    fn ohne_strecke_kein_ergebnis() {
        assert!(berechnen(&roh(0.0, 100.0, 0.0), &station(), &Rechenparameter::default(), None, 0.0).is_none());
    }

    #[test]
    fn richtung_wird_auf_vollkreis_normiert() {
        let e = berechnen(&roh(380.0, 100.0, 10.0), &station(), &Rechenparameter::default(), None, 0.0).unwrap();
        assert!((e.hz_orientiert - 30.0).abs() < 1e-9);
    }

    #[test]
    fn zweidimensional_ohne_standpunkthoehe() {
        let mut s = station();
        s.hoehe = 0.0;
        let e = berechnen(&roh(0.0, 100.0, 10.0), &s, &Rechenparameter::default(), None, 1.0).unwrap();
        assert!(!e.ist_3d);
        assert_eq!(e.hoehe, 0.0);
    }

    #[test]
    fn qualitaetspruefung() {
        assert!(ist_verwendbar(Some(&station())));
        let mut s = station();
        s.konvergiert = false;
        assert!(!ist_verwendbar(Some(&s)));
        assert!(pruefe_station(&s).is_err());
        s = station();
        s.massstab = f64::NAN;
        assert!(!ist_verwendbar(Some(&s)));
        assert!(!ist_verwendbar(None));
    }

    #[test]
    fn berechne_alle_filtert_nach_standpunkt() {
        let a = roh(0.0, 100.0, 10.0);
        let mut b = roh(0.0, 100.0, 10.0);
        b.standpunkt_nr = "S2".into();
        let liste = [a, b];
        let erg = berechne_alle(&liste, &station(), "S1", &Rechenparameter::default(), 0.0);
        assert_eq!(erg.len(), 1);
    }
}
