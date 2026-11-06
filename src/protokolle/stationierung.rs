// Zweck:    Protokoll der Freien Stationierung (Beobachtungstabelle mit Residuen, Ergebnis, Genauigkeit)
// Hardware: keine
// Autor:    Johann Gerner

use chrono::NaiveDateTime;

use super::rtf::Tabellen;
use super::{Protokoll, STRICH, VORLAGE_FREIE_STATIONIERUNG, felder, signiert, zeile, zeitpunkt_text};
use crate::daten::modelle::{StationierungsErgebnis, StationierungsPunkt};
use crate::daten::rechenparameter::Rechenparameter;
use crate::grundlagen::format::f;

/// Umrechnung des Konfidenzniveaus (Anteil) in Prozent.
const PROZENT: f64 = 100.0;
/// Anzahl der Punkte, bei der die Direktlösung (Ähnlichkeitstransformation) gilt.
const ZWEI_PUNKTE: usize = 2;

fn s0_info(erg: &StationierungsErgebnis, punkte: usize, par: &Rechenparameter) -> String {
    if punkte == ZWEI_PUNKTE {
        "Aehnlichkeitstransformation  (2 Punkte, Direktloesung, r = 0)".to_string()
    } else if erg.redundanz > 0 {
        let bez = if par.sigma_faktor_wirksam() == 1 { "s0".to_string() } else { format!("{}*s0", par.sigma_faktor_wirksam()) };
        let konv = if erg.konvergiert {
            format!("Iterationen:  {}  (konvergiert)", erg.iterationen)
        } else {
            format!("Iterationen:  {}  !! NICHT konvergiert !!", erg.iterationen)
        };
        format!("Standardabw. {bez}:  {} mm     Redundanz r:  {}     {konv}", f(par.sigma_wert(erg.s0_mm), 2), erg.redundanz)
    } else {
        "Standardabw. s0:  -  (Redundanz r = 0, eindeutig bestimmt)".to_string()
    }
}

/// Konfidenzellipse des Standpunkts für das Protokoll (leer, wenn nicht bestimmt).
fn konfidenz_info(erg: &StationierungsErgebnis) -> String {
    erg.konfidenz
        .as_ref()
        .map(|k| {
            format!(
                "     Konfidenzellipse {} %:  a = {} mm  b = {} mm  [Massstab nicht ausgeglichen]",
                f(k.niveau * PROZENT, 0),
                f(k.a_mm, 2),
                f(k.b_mm, 2)
            )
        })
        .unwrap_or_default()
}

/// Protokoll der Freien Stationierung.
#[allow(clippy::too_many_arguments)]
pub fn protokoll(
    erg: &StationierungsErgebnis,
    punkte: &[StationierungsPunkt],
    akt: (&[bool], &[bool], &[bool]),
    standpunkt: &str,
    instr_hoehe: f64,
    par: &Rechenparameter,
    bearbeiter: &str,
    jetzt: NaiveDateTime,
) -> Protokoll {
    let felder = felder(&[
        ("Bearbeiter", bearbeiter.to_string()),
        ("Zeitpunkt", zeitpunkt_text(jetzt)),
        ("Standpunktnr", standpunkt.to_string()),
        ("IH", f(instr_hoehe, 3)),
        ("R", f(erg.r, 3)),
        ("H", f(erg.h, 3)),
        ("Hoehe", if erg.berechnung_3d { f(erg.hoehe, 3) } else { STRICH.to_string() }),
        ("Orientierung", f(erg.orientierung_gon, 4)),
        (
            "Massstab",
            if par.freier_massstab {
                format!("{}  [Helmert-Naeherung, nicht ausgeglichen]", f(erg.massstab, 6))
            } else {
                "1.000000  [fixiert]".to_string()
            },
        ),
        ("S0Info", format!("{}{}", s0_info(erg, punkte.len(), par), konfidenz_info(erg))),
    ]);
    let wert = |aktiv: bool, v: f64| if aktiv && !v.is_nan() { signiert(v, 1) } else { STRICH.to_string() };
    let nicht_null = |v: f64, stellen: usize| if v != 0.0 { f(v, stellen) } else { STRICH.to_string() };
    let zeilen = punkte
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let res = erg.residuen.get(i);
            let inaktiv = !akt.0.get(i).copied().unwrap_or(true) || !akt.1.get(i).copied().unwrap_or(true) || !akt.2.get(i).copied().unwrap_or(true);
            zeile(&[
                ("PunktNr", format!("{}{}", p.punkt_nr, if inaktiv { "*" } else { "" })),
                ("R", f(p.r, 3)),
                ("H", f(p.h, 3)),
                ("Hoehe", nicht_null(p.hoehe, 3)),
                ("HZ", f(p.hz, 4)),
                ("V", f(p.v, 4)),
                ("Strecke", nicht_null(p.strecke, 3)),
                ("Zielhoehe", nicht_null(p.zielhoehe, 3)),
                ("vQuer", res.map_or_else(|| STRICH.to_string(), |r| wert(r.richtung_aktiv, r.v_quer_mm))),
                ("vLaengs", res.map_or_else(|| STRICH.to_string(), |r| wert(r.strecke_aktiv, r.v_strecke_mm))),
                ("vHoehe", res.map_or_else(|| STRICH.to_string(), |r| wert(r.hoehe_aktiv, r.v_hoehe_mm))),
            ])
        })
        .collect();
    Protokoll {
        vorlage: VORLAGE_FREIE_STATIONIERUNG,
        praefix: "FreieStationierung",
        felder,
        tabellen: Tabellen::from([(String::new(), zeilen)]),
        zeitpunkt: jetzt,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daten::modelle::{PunktResiduum, jetzt};
    use crate::protokolle::pruefe_vollstaendig;

    fn punkt(nr: &str, r: f64, h: f64) -> StationierungsPunkt {
        StationierungsPunkt { punkt_nr: nr.into(), r, h, hoehe: 0.0, hz: 10.0, v: 100.0, strecke: 25.0, zielhoehe: 0.0 }
    }

    #[test]
    fn protokoll_mit_residuen_und_stern() {
        let erg = StationierungsErgebnis {
            r: 100.0,
            h: 200.0,
            berechnung_3d: false,
            orientierung_gon: 12.0,
            massstab: 1.0000123,
            s0_mm: 1.5,
            redundanz: 3,
            iterationen: 4,
            konvergiert: true,
            residuen: vec![
                PunktResiduum { punkt_nr: "A".into(), v_quer_mm: 1.2, v_strecke_mm: -0.8, ..PunktResiduum::default() },
                PunktResiduum { punkt_nr: "B".into(), richtung_aktiv: false, v_quer_mm: f64::NAN, ..PunktResiduum::default() },
            ],
            ..StationierungsErgebnis::default()
        };
        let punkte = [punkt("A", 0.0, 0.0), punkt("B", 50.0, 0.0), punkt("C", 0.0, 50.0)];
        let par = Rechenparameter::default();
        let p = protokoll(&erg, &punkte, (&[true, false, true], &[true, true, true], &[true, true, true]), "SP1", 1.5, &par, "Tester", jetzt());
        pruefe_vollstaendig(&p);
        let z = &p.tabellen[""];
        assert_eq!(z[0]["vQuer"], "+1.2");
        assert_eq!(z[0]["vLaengs"], "-0.8");
        assert_eq!(z[1]["PunktNr"], "B*");
        assert_eq!(z[1]["vQuer"], "-");
        assert_eq!(z[2]["vQuer"], "-", "ohne Residuum");
        assert_eq!(z[0]["Hoehe"], "-");
        assert!(p.felder["S0Info"].contains("Redundanz r:  3") && p.felder["S0Info"].contains("konvergiert"));
        assert_eq!(p.felder["Massstab"], "1.000012  [Helmert-Naeherung, nicht ausgeglichen]");
        assert_eq!(p.felder["Hoehe"], "-", "2D");
    }

    #[test]
    fn zwei_punkte_direktloesung_und_fester_massstab() {
        let erg = StationierungsErgebnis { berechnung_3d: true, hoehe: 5.0, ..StationierungsErgebnis::default() };
        let par = Rechenparameter { freier_massstab: false, ..Rechenparameter::default() };
        let punkte = [punkt("A", 0.0, 0.0), punkt("B", 50.0, 0.0)];
        let p = protokoll(&erg, &punkte, (&[true, true], &[true, true], &[true, true]), "SP", 1.0, &par, "T", jetzt());
        assert!(p.felder["S0Info"].starts_with("Aehnlichkeitstransformation"));
        assert_eq!(p.felder["Massstab"], "1.000000  [fixiert]");
        assert_eq!(p.felder["Hoehe"], "5.000");
    }
}
