// Zweck:    Protokoll der Polarpunktberechnung (Querformat): Station, Anschlusspunkte, berechnete Polarpunkte
// Hardware: keine
// Autor:    Johann Gerner

use chrono::NaiveDateTime;

use super::rtf::Tabellen;
use super::{Protokoll, STRICH, VORLAGE_POLARPUNKTE, felder, signiert_oder_strich, zeile, zeitpunkt_text};
use crate::grundlagen::format::f;

/// Prozent.
const PROZENT: f64 = 100.0;
/// Status eines neu angelegten Punkts.
pub const STATUS_NEU: &str = "neu";
/// Status eines ersetzten Punkts.
pub const STATUS_ERSETZT: &str = "ersetzt";

/// Anschlusspunktzeile (NaN = nicht vorhanden).
#[derive(Clone, Debug)]
pub struct PolarAnschluss {
    pub punkt_nr: String,
    pub inaktiv: bool,
    pub strecke_h: f64,
    pub v_winkel_cc: f64,
    pub v_quer_mm: f64,
    pub v_laengs_mm: f64,
    pub v_hoehe_mm: f64,
    pub gewicht_richtung: f64,
}

/// Polarpunktzeile; `d_r`/`d_h` = neu − alt (NaN bei „neu").
#[derive(Clone, Debug)]
pub struct PolarPunkt {
    pub punkt_nr: String,
    pub hz: f64,
    pub v: f64,
    pub strecke: f64,
    pub zielhoehe: f64,
    pub exz_laengs: f64,
    pub exz_quer: f64,
    pub r: f64,
    pub h: f64,
    pub hoehe: Option<f64>,
    pub status: String,
    pub d_r: f64,
    pub d_h: f64,
}

/// Alle Angaben des Protokolls.
#[derive(Clone, Debug)]
pub struct PolarDaten {
    pub standpunkt: String,
    pub instr_hoehe: f64,
    pub quelle: String,
    pub r: f64,
    pub h: f64,
    pub hoehe: Option<f64>,
    pub orientierung_gon: f64,
    pub massstab: f64,
    pub s0_info: String,
    pub gewichtung: String,
    pub hinweise: Vec<String>,
    pub anschluss: Vec<PolarAnschluss>,
    pub punkte: Vec<PolarPunkt>,
}

/// Protokoll der Polarpunkte.
pub fn protokoll(d: &PolarDaten, bearbeiter: &str, jetzt: NaiveDateTime) -> Protokoll {
    let neu = d.punkte.iter().filter(|p| p.status == STATUS_NEU).count();
    let ersetzt = d.punkte.len() - neu;
    let hinweise: Vec<String> = d.hinweise.iter().map(|h| format!("! {h}")).collect();
    let felder = felder(&[
        ("Bearbeiter", bearbeiter.to_string()),
        ("Zeitpunkt", zeitpunkt_text(jetzt)),
        ("Standpunktnr", d.standpunkt.clone()),
        ("IH", f(d.instr_hoehe, 3)),
        ("Quelle", d.quelle.clone()),
        ("R", f(d.r, 3)),
        ("H", f(d.h, 3)),
        ("Hoehe", d.hoehe.map_or_else(|| STRICH.to_string(), |h| f(h, 3))),
        ("Orientierung", f(d.orientierung_gon, 4)),
        ("Massstab", f(d.massstab, 6)),
        ("S0Info", d.s0_info.clone()),
        ("Gewichtung", d.gewichtung.clone()),
        ("Hinweise", hinweise.join("\n")),
        ("HatHinweise", if d.hinweise.is_empty() { "0" } else { "1" }.to_string()),
        ("HatAnschluss", if d.anschluss.is_empty() { "0" } else { "1" }.to_string()),
        ("Zusammenfassung", format!("{} Polarpunkt(e) in Neupunkte.json gespeichert: {neu} neu, {ersetzt} ersetzt.", d.punkte.len())),
    ]);
    let anschluss = d
        .anschluss
        .iter()
        .map(|a| {
            zeile(&[
                ("PunktNr", format!("{}{}", a.punkt_nr, if a.inaktiv { "*" } else { "" })),
                ("StreckeH", f(a.strecke_h, 3)),
                ("vWinkel", signiert_oder_strich(a.v_winkel_cc, 1)),
                ("vQuer", signiert_oder_strich(a.v_quer_mm, 1)),
                ("vLaengs", signiert_oder_strich(a.v_laengs_mm, 1)),
                ("vHoehe", signiert_oder_strich(a.v_hoehe_mm, 1)),
                ("Gewicht", if a.gewicht_richtung.is_nan() { STRICH.to_string() } else { format!("{}%", f(a.gewicht_richtung * PROZENT, 1)) }),
            ])
        })
        .collect();
    let polar = d
        .punkte
        .iter()
        .map(|p| {
            zeile(&[
                ("PunktNr", p.punkt_nr.clone()),
                ("HZ", f(p.hz, 4)),
                ("V", f(p.v, 4)),
                ("Strecke", f(p.strecke, 3)),
                ("Zielhoehe", if p.zielhoehe != 0.0 { f(p.zielhoehe, 3) } else { STRICH.to_string() }),
                ("ExzL", f(p.exz_laengs, 3)),
                ("ExzQ", f(p.exz_quer, 3)),
                ("R", f(p.r, 3)),
                ("H", f(p.h, 3)),
                ("Hoehe", p.hoehe.map_or_else(|| STRICH.to_string(), |h| f(h, 3))),
                ("Status", p.status.clone()),
                ("dR", signiert_oder_strich(p.d_r, 3)),
                ("dH", signiert_oder_strich(p.d_h, 3)),
            ])
        })
        .collect();
    let tabellen = Tabellen::from([("Anschluss".to_string(), anschluss), ("Polar".to_string(), polar)]);
    Protokoll { vorlage: VORLAGE_POLARPUNKTE, praefix: "Polarpunkte", felder, tabellen, zeitpunkt: jetzt }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daten::modelle::jetzt;
    use crate::protokolle::pruefe_vollstaendig;

    fn daten(mit_anschluss: bool, mit_hinweis: bool) -> PolarDaten {
        PolarDaten {
            standpunkt: "SP1".into(),
            instr_hoehe: 1.556,
            quelle: "Anschlusspunkten der dat.csv".into(),
            r: 100.0,
            h: 200.0,
            hoehe: Some(50.0),
            orientierung_gon: 12.3456,
            massstab: 1.0,
            s0_info: "Standardabw. s0:  1.20 mm     Redundanz r:  2".into(),
            gewichtung: "aus".into(),
            hinweise: if mit_hinweis { vec!["Nur 2 Anschlusspunkt(e)".into()] } else { vec![] },
            anschluss: if mit_anschluss {
                vec![PolarAnschluss {
                    punkt_nr: "A1".into(),
                    inaktiv: true,
                    strecke_h: 50.123,
                    v_winkel_cc: 3.2,
                    v_quer_mm: f64::NAN,
                    v_laengs_mm: -1.5,
                    v_hoehe_mm: 0.0,
                    gewicht_richtung: 0.5,
                }]
            } else {
                vec![]
            },
            punkte: vec![
                PolarPunkt {
                    punkt_nr: "7".into(),
                    hz: 10.0,
                    v: 100.0,
                    strecke: 20.0,
                    zielhoehe: 0.0,
                    exz_laengs: 0.0,
                    exz_quer: 0.0,
                    r: 110.0,
                    h: 215.0,
                    hoehe: None,
                    status: STATUS_NEU.into(),
                    d_r: f64::NAN,
                    d_h: f64::NAN,
                },
                PolarPunkt {
                    punkt_nr: "8".into(),
                    hz: 20.0,
                    v: 99.0,
                    strecke: 30.0,
                    zielhoehe: 1.5,
                    exz_laengs: 0.5,
                    exz_quer: -0.25,
                    r: 120.0,
                    h: 225.0,
                    hoehe: Some(49.5),
                    status: STATUS_ERSETZT.into(),
                    d_r: 0.012,
                    d_h: -0.003,
                },
            ],
        }
    }

    #[test]
    fn protokoll_mit_anschluss_und_hinweis() {
        let p = protokoll(&daten(true, true), "Tester", jetzt());
        let rtf = pruefe_vollstaendig(&p);
        assert_eq!(p.felder["HatAnschluss"], "1");
        assert_eq!(p.felder["HatHinweise"], "1");
        assert_eq!(p.felder["Hinweise"], "! Nur 2 Anschlusspunkt(e)");
        assert_eq!(p.felder["Zusammenfassung"], "2 Polarpunkt(e) in Neupunkte.json gespeichert: 1 neu, 1 ersetzt.");
        let a = &p.tabellen["Anschluss"][0];
        assert_eq!((a["PunktNr"].as_str(), a["vQuer"].as_str(), a["vHoehe"].as_str(), a["Gewicht"].as_str()), ("A1*", "-", "0.0", "50.0%"));
        let polar = &p.tabellen["Polar"];
        assert_eq!((polar[0]["Status"].as_str(), polar[0]["dR"].as_str(), polar[0]["Hoehe"].as_str(), polar[0]["Zielhoehe"].as_str()), ("neu", "-", "-", "-"));
        assert_eq!(polar[1]["dR"], "+0.012");
        assert_eq!(polar[1]["dH"], "-0.003");
        assert!(rtf.contains("Anschlusspunkte (* = Beobachtung abgeschaltet)"));
    }

    #[test]
    fn protokoll_ohne_anschluss_blendet_tabelle_aus() {
        let p = protokoll(&daten(false, false), "Tester", jetzt());
        let rtf = pruefe_vollstaendig(&p);
        assert_eq!(p.felder["HatAnschluss"], "0");
        assert!(!rtf.contains("Anschlusspunkte (*"));
        assert!(rtf.contains("Polarpunkte"));
    }
}
