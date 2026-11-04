// Zweck:    Protokolle der Schnittberechnungen: Rückwärtsschnitt, Vorwärtsschnitt, Bogenschnitt, Hochpunktherablegung
// Hardware: keine
// Autor:    Johann Gerner

use chrono::NaiveDateTime;

use super::rtf::{Tabellen, Zeile};
use super::{
    Protokoll, STRICH, VORLAGE_BOGENSCHNITT, VORLAGE_HOCHPUNKT, VORLAGE_RUECKWAERTSSCHNITT, VORLAGE_VORWAERTSSCHNITT, ampel_stufe, felder, s0_info, signiert,
    zeile, zeitpunkt_text,
};
use crate::grundlagen::format::f;
use crate::rechnen::bogenschnitt::{BogenschnittErgebnis, BogenschnittMessung};
use crate::rechnen::hochpunkt::{HochpunktErgebnis, HochpunktMessung};
use crate::rechnen::rueckwaertsschnitt::{RueckwaertsErgebnis, RueckwaertsPunkt};
use crate::rechnen::vorwaertsschnitt::{VorwaertsErgebnis, VorwaertsMessung};

/// Ampelschwellen für Richtungsresiduen [cc] und Streckenresiduen [mm].
const SCHWELLEN_RICHTUNG: [f64; 3] = [60.0, 20.0, 5.0];
const SCHWELLEN_STRECKE: [f64; 3] = [30.0, 10.0, 3.0];

fn nur_tabelle(zeilen: Vec<Zeile>) -> Tabellen {
    Tabellen::from([(String::new(), zeilen)])
}

fn mit_stern(nr: &str, aktiv: bool) -> String {
    if aktiv { nr.to_string() } else { format!("{nr}*") }
}

/// Protokoll des Rückwärtsschnitts.
pub fn rueckwaertsschnitt(erg: &RueckwaertsErgebnis, punkte: &[RueckwaertsPunkt], standpunkt: &str, bearbeiter: &str, jetzt: NaiveDateTime) -> Protokoll {
    let felder = felder(&[
        ("Bearbeiter", bearbeiter.to_string()),
        ("Zeitpunkt", zeitpunkt_text(jetzt)),
        ("Standpunktnr", standpunkt.to_string()),
        ("R", f(erg.r, 3)),
        ("H", f(erg.h, 3)),
        ("Orientierung", f(erg.orientierung_gon, 4)),
        ("S0Info", s0_info(erg.redundanz, erg.s0_mm, erg.iterationen, erg.konvergiert)),
        ("KritKreis", erg.kritischer_kreis.clone()),
        ("HatKritKreis", if erg.kritischer_kreis.is_empty() { "0" } else { "1" }.to_string()),
    ]);
    let zeilen = punkte
        .iter()
        .zip(&erg.residuen)
        .map(|(p, res)| {
            let v = if res.aktiv { signiert(res.v_winkel_cc, 1) } else { STRICH.to_string() };
            let ampel = if res.aktiv { ampel_stufe(res.v_winkel_cc.abs(), SCHWELLEN_RICHTUNG) } else { String::new() };
            zeile(&[
                ("PunktNr", mit_stern(&p.punkt_nr, res.aktiv)),
                ("R", f(p.r, 3)),
                ("H", f(p.h, 3)),
                ("HZ", f(p.hz, 4)),
                ("s", f(res.strecke_h, 1)),
                ("v", v),
                ("_ampel", ampel),
            ])
        })
        .collect();
    Protokoll { vorlage: VORLAGE_RUECKWAERTSSCHNITT, praefix: "Rueckwaertsschnitt", felder, tabellen: nur_tabelle(zeilen), zeitpunkt: jetzt }
}

/// Protokoll des Vorwärtsschnitts.
pub fn vorwaertsschnitt(erg: &VorwaertsErgebnis, messungen: &[VorwaertsMessung], neupunkt: &str, bearbeiter: &str, jetzt: NaiveDateTime) -> Protokoll {
    let felder = felder(&[
        ("Bearbeiter", bearbeiter.to_string()),
        ("Zeitpunkt", zeitpunkt_text(jetzt)),
        ("Neupunkt", neupunkt.to_string()),
        ("R", f(erg.r, 3)),
        ("H", f(erg.h, 3)),
        ("S0Info", s0_info(erg.redundanz, erg.s0_mm, erg.iterationen, erg.konvergiert)),
    ]);
    let zeilen = messungen
        .iter()
        .zip(&erg.residuen)
        .map(|(m, res)| {
            let v = if res.aktiv { signiert(res.v_winkel_cc, 1) } else { STRICH.to_string() };
            let ampel = if res.aktiv { ampel_stufe(res.v_winkel_cc.abs(), SCHWELLEN_RICHTUNG) } else { String::new() };
            zeile(&[
                ("PunktNr", mit_stern(&m.punkt_nr, res.aktiv)),
                ("R", f(m.r, 3)),
                ("H", f(m.h, 3)),
                ("Hz", f(m.hz, 4)),
                ("z", f(m.z, 4)),
                ("t", f((m.hz + m.z).rem_euclid(400.0), 4)),
                ("s", f(res.strecke_h, 1)),
                ("v", v),
                ("_ampel", ampel),
            ])
        })
        .collect();
    Protokoll { vorlage: VORLAGE_VORWAERTSSCHNITT, praefix: "Vorwaertsschnitt", felder, tabellen: nur_tabelle(zeilen), zeitpunkt: jetzt }
}

/// Protokoll des Bogenschnitts.
pub fn bogenschnitt(erg: &BogenschnittErgebnis, messungen: &[BogenschnittMessung], neupunkt: &str, bearbeiter: &str, jetzt: NaiveDateTime) -> Protokoll {
    let zweite = if erg.zwei_loesungen { format!("2. Lösung: R = {} m   H = {} m", f(erg.r2, 3), f(erg.h2, 3)) } else { String::new() };
    let felder = felder(&[
        ("Bearbeiter", bearbeiter.to_string()),
        ("Zeitpunkt", zeitpunkt_text(jetzt)),
        ("Neupunkt", neupunkt.to_string()),
        ("R", f(erg.r, 3)),
        ("H", f(erg.h, 3)),
        ("S0Info", s0_info(erg.redundanz, erg.s0_mm, erg.iterationen, erg.konvergiert)),
        ("ZweiteLoesung", zweite),
        ("HatZwei", if erg.zwei_loesungen { "1" } else { "0" }.to_string()),
    ]);
    let zeilen = messungen
        .iter()
        .zip(&erg.residuen)
        .map(|(m, res)| {
            let v = if res.aktiv { signiert(res.v_strecke_mm, 1) } else { STRICH.to_string() };
            let ampel = if res.aktiv { ampel_stufe(res.v_strecke_mm.abs(), SCHWELLEN_STRECKE) } else { String::new() };
            zeile(&[
                ("PunktNr", mit_stern(&m.punkt_nr, res.aktiv)),
                ("R", f(m.r, 3)),
                ("H", f(m.h, 3)),
                ("Strecke", f(m.strecke, 3)),
                ("v", v),
                ("_ampel", ampel),
            ])
        })
        .collect();
    Protokoll { vorlage: VORLAGE_BOGENSCHNITT, praefix: "Bogenschnitt", felder, tabellen: nur_tabelle(zeilen), zeitpunkt: jetzt }
}

/// Protokoll der Hochpunktherablegung.
pub fn hochpunkt(erg: &HochpunktErgebnis, messungen: &[HochpunktMessung], punktnr: &str, bearbeiter: &str, jetzt: NaiveDateTime) -> Protokoll {
    let richtung = if erg.redundanz_dir > 0 {
        let mut t = format!("Richtungen – s0 = {} mm    r = {}    {} Iter.", f(erg.s0_dir_mm, 2), erg.redundanz_dir, erg.iterationen);
        if !erg.konvergiert {
            t.push_str("  !! NICHT konvergiert !!");
        }
        t
    } else {
        format!("Richtungen – r = 0  (eindeutig bestimmt, {} Iter.)", erg.iterationen)
    };
    let hoehe = if erg.redundanz_h > 0 { format!("Hoehe – s0 = {} mm    r = {}", f(erg.s0_h_mm, 2), erg.redundanz_h) } else { "Hoehe – r = 0  (eindeutig bestimmt)".to_string() };
    let felder = felder(&[
        ("Bearbeiter", bearbeiter.to_string()),
        ("Zeitpunkt", zeitpunkt_text(jetzt)),
        ("PunktNr", punktnr.to_string()),
        ("R", f(erg.r, 3)),
        ("H", f(erg.h, 3)),
        ("Hoehe", f(erg.hoehe, 3)),
        ("S0DirInfo", richtung),
        ("S0HInfo", hoehe),
    ]);
    let zeilen = messungen
        .iter()
        .zip(&erg.residuen)
        .map(|(m, res)| {
            let v_dir = if res.aktiv_dir { signiert(res.v_dir_cc, 1) } else { STRICH.to_string() };
            let h_pi = if res.aktiv_h { f(res.hoehe_pi, 3) } else { STRICH.to_string() };
            let v_h = if res.aktiv_h { signiert(res.v_h_mm, 1) } else { STRICH.to_string() };
            let amp_dir = if res.aktiv_dir { ampel_stufe(res.v_dir_cc.abs(), SCHWELLEN_RICHTUNG) } else { String::new() };
            let amp_h = if res.aktiv_h { ampel_stufe(res.v_h_mm.abs(), SCHWELLEN_STRECKE) } else { String::new() };
            zeile(&[
                ("PunktNr", mit_stern(&m.punkt_nr, res.aktiv_dir)),
                ("R", f(m.r, 3)),
                ("H", f(m.h, 3)),
                ("Hoehe", f(m.hoehe, 3)),
                ("iH", f(m.ih, 3)),
                ("Hz", f(m.hz, 4)),
                ("z", f(m.z, 4)),
                ("V", f(m.v, 4)),
                ("zh", f(m.zielhoehe, 3)),
                ("s", f(res.s_horiz, 1)),
                ("vDir", v_dir),
                ("HPi", h_pi),
                ("vH", v_h),
                ("_ampel", if amp_dir.is_empty() { amp_h } else { amp_dir }),
            ])
        })
        .collect();
    Protokoll { vorlage: VORLAGE_HOCHPUNKT, praefix: "Hochpunktherablegung", felder, tabellen: nur_tabelle(zeilen), zeitpunkt: jetzt }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daten::modelle::jetzt;
    use crate::protokolle::pruefe_vollstaendig;
    use crate::rechnen::{bogenschnitt as bs, hochpunkt as hp, rueckwaertsschnitt as rs, vorwaertsschnitt as vs};

    #[test]
    fn rueckwaertsschnitt_protokoll() {
        let punkte: Vec<RueckwaertsPunkt> = [("A", 10.0, 100.0, 30.0), ("B", 90.0, 110.0, 80.0), ("C", 60.0, 20.0, 150.0), ("D", 5.0, 40.0, 250.0)]
            .iter()
            .map(|(n, r, h, hz)| RueckwaertsPunkt { punkt_nr: (*n).to_string(), r: *r, h: *h, hz: *hz })
            .collect();
        let erg = rs::berechnen(&punkte, Some(&[true, true, true, false]), 9.0, 1.0).unwrap();
        let p = rueckwaertsschnitt(&erg, &punkte, "9000", "Tester", jetzt());
        let rtf = pruefe_vollstaendig(&p);
        assert!(rtf.contains("Standpunktnr.:  9000") && rtf.contains("Tester"));
        assert!(rtf.contains("D*"), "inaktiver Punkt trägt einen Stern");
        assert_eq!(p.tabellen[""].len(), 4);
        assert!(p.pfad(std::path::Path::new("x")).to_string_lossy().contains("Rueckwaertsschnitt_"));
    }

    #[test]
    fn vorwaertsschnitt_protokoll() {
        let m = [VorwaertsMessung { punkt_nr: "A".into(), r: 0.0, h: 0.0, hz: 50.0, z: 350.0 }, VorwaertsMessung { punkt_nr: "B".into(), r: 100.0, h: 0.0, hz: 350.0, z: 0.0 }];
        let erg = vs::berechnen(&m, None, 9.0, 1.0).unwrap();
        let p = vorwaertsschnitt(&erg, &m, "8000", "Tester", jetzt());
        pruefe_vollstaendig(&p);
        assert_eq!(p.tabellen[""][0]["t"], "0.0000", "Hz 50 + z 350 = 400 gon = 0 gon");
        assert_eq!(p.tabellen[""][1]["t"], "350.0000");
    }

    #[test]
    fn bogenschnitt_protokoll_mit_zweiter_loesung() {
        let m = [BogenschnittMessung { punkt_nr: "A".into(), r: 0.0, h: 0.0, strecke: 50.0 }, BogenschnittMessung { punkt_nr: "B".into(), r: 80.0, h: 0.0, strecke: 50.0 }];
        let erg = bs::berechnen(&m, None, bs::StreckenGenauigkeit::default()).unwrap();
        let p = bogenschnitt(&erg, &m, "8000", "Tester", jetzt());
        let rtf = pruefe_vollstaendig(&p);
        assert_eq!(p.felder["HatZwei"], "1");
        assert!(rtf.contains("2. L\\'f6sung"));
    }

    #[test]
    fn hochpunkt_protokoll() {
        let ziel = (50.0_f64, 80.0_f64);
        let mk = |nr: &str, r: f64, h: f64| {
            let t = (ziel.0 - r).atan2(ziel.1 - h) * 200.0 / std::f64::consts::PI;
            HochpunktMessung { punkt_nr: nr.into(), r, h, hoehe: 100.0, ih: 1.5, hz: t.rem_euclid(400.0), z: 0.0, v: 80.0, zielhoehe: 0.0 }
        };
        let m = [mk("A", 0.0, 0.0), mk("B", 100.0, 0.0), mk("C", 50.0, -50.0)];
        let erg = hp::berechnen(&m, None, 9.0, 1.0).unwrap();
        let p = hochpunkt(&erg, &m, "9001", "Tester", jetzt());
        pruefe_vollstaendig(&p);
        assert_eq!(p.tabellen[""].len(), 3);
    }
}
