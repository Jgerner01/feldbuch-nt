// Zweck:    Protokoll der Koordinatentransformation (Parameter, Güte, benutzte und nicht benutzte Passpunkte,
//           transformierte Punkte)
// Hardware: keine
// Autor:    Johann Gerner

use std::collections::{HashMap, HashSet};

use chrono::NaiveDateTime;

use super::rtf::Tabellen;
use super::{Protokoll, VORLAGE_TRANSFORMATION, ampel_stufe, felder, signiert, signiert_plus, zeile, zeitpunkt_text};
use crate::grundlagen::format::f;
use crate::rechnen::koord_transformation::{Ergebnis, TransformPunkt, TransformationsTyp};

/// Umrechnung Radiant → „cc" wie im Original (Faktor 206264,806).
const RAD_ZU_CC_ORIGINAL: f64 = 206_264.806;
/// Millionstel.
const PPM: f64 = 1e6;
/// Millimeter je Zentimeter.
const MM_PRO_CM: f64 = 10.0;
/// Ampelschwellen für die Gesamtabweichung [cm].
const SCHWELLEN_CM: [f64; 3] = [9.0, 3.0, 1.0];

fn schluessel(nr: &str) -> String {
    nr.to_lowercase()
}

fn spalte14(wert: f64, stellen: usize) -> String {
    format!("{:>14}", f(wert, stellen))
}

/// Mehrzeiliger Parametertext.
fn parameter_text(erg: &Ergebnis) -> String {
    let p = &erg.parameter;
    let mut z: Vec<String> = Vec::new();
    let drehung = |name: &str, rad: f64| format!("  {name}    =  {} rad  ({} cc)", spalte14(rad, 8), signiert_plus(rad * RAD_ZU_CC_ORIGINAL, 3));
    match erg.typ {
        TransformationsTyp::Helmert2D => {
            z.push(format!("  dx          =  {} m", spalte14(p.dx, 4)));
            z.push(format!("  dy          =  {} m", spalte14(p.dy, 4)));
            z.push(format!("  Rotation α  =  {} gon", spalte14(p.alpha_gon, 6)));
            if erg.fester_massstab {
                z.push("  Maßstab m   =           1.00000000  (fest)".to_string());
            } else {
                z.push(format!("  Maßstab m   =  {}", spalte14(p.massstab_2d, 8)));
                z.push(format!("  (a = {},  b = {})", f(p.a, 8), f(p.b, 8)));
            }
        }
        TransformationsTyp::Helmert3D | TransformationsTyp::Parameter7 | TransformationsTyp::Parameter9 => {
            z.push(format!("  dx    =  {} m", spalte14(p.dx, 4)));
            z.push(format!("  dy    =  {} m", spalte14(p.dy, 4)));
            z.push(format!("  dz    =  {} m", spalte14(p.dz, 4)));
            z.push(drehung("rx", p.rx_rad));
            z.push(drehung("ry", p.ry_rad));
            z.push(drehung("rz", p.rz_rad));
            let massstab = |name: &str, m: f64| {
                if erg.fester_massstab {
                    format!("  {name}    =              0.00000000  (fest, m = 1)")
                } else {
                    format!("  {name}    =  {}       ({} ppm)", spalte14(m, 8), signiert_plus(m * PPM, 2))
                }
            };
            if erg.typ == TransformationsTyp::Parameter9 {
                z.push(massstab("mx", p.mx));
                z.push(massstab("my", p.my));
                z.push(massstab("mz", p.mz));
            } else {
                z.push(massstab("m ", p.m));
            }
        }
    }
    z.join("\n")
}

/// Protokoll der Transformation.
pub fn protokoll(
    erg: &Ergebnis,
    quelle: &[TransformPunkt],
    ziel: &[TransformPunkt],
    alle_transformiert: Option<&[TransformPunkt]>,
    projektname: &str,
    bearbeiter: &str,
    jetzt: NaiveDateTime,
) -> Protokoll {
    let ist_3d = erg.typ != TransformationsTyp::Helmert2D;
    let benutzt: HashSet<String> = erg.residuen.iter().map(|r| schluessel(&r.punkt_nr)).collect();
    let residuen: HashMap<String, &crate::rechnen::koord_transformation::Residuum> = erg.residuen.iter().map(|r| (schluessel(&r.punkt_nr), r)).collect();
    let nicht_benutzt = quelle.iter().filter(|p| !benutzt.contains(&schluessel(&p.punkt_nr))).count();

    let massstab = if erg.fester_massstab { "[Maßstab fest m = 1]" } else { "[Maßstab frei]" };
    let guete = if erg.redundanz > 0 {
        format!("Anzahl Punktpaare: {}     Redundanz: {}     Standardabw: {} mm", quelle.len(), erg.redundanz, f(erg.s0_mm, 2))
    } else {
        format!("Anzahl Punktpaare: {}     Redundanz: 0  (eindeutig bestimmt)", quelle.len())
    };
    let felder = felder(&[
        ("TypUndMassstab", format!("Typ: {}   {massstab}", erg.typ.name())),
        ("Bearbeiter", bearbeiter.to_string()),
        ("Projekt", projektname.to_string()),
        ("Zeitpunkt", zeitpunkt_text(jetzt)),
        ("Parameter", parameter_text(erg)),
        ("Gueteinfo", guete),
        ("Ist3D", if ist_3d { "1" } else { "0" }.to_string()),
        ("HatNichtBenutzte", if nicht_benutzt > 0 { "1" } else { "0" }.to_string()),
    ]);

    let z_text = |wert: f64| if ist_3d { f(wert, 3) } else { String::new() };
    let mut benutzte = Vec::new();
    let mut nicht_benutzte = Vec::new();
    for (src, tgt) in quelle.iter().zip(ziel) {
        match residuen.get(&schluessel(&src.punkt_nr)) {
            Some(r) => {
                let (vx, vy, vz, vg) = (r.vx_mm / MM_PRO_CM, r.vy_mm / MM_PRO_CM, r.vz_mm / MM_PRO_CM, r.v_gesamt_mm / MM_PRO_CM);
                benutzte.push(zeile(&[
                    ("PunktNr", src.punkt_nr.clone()),
                    ("XAus", f(src.x, 3)),
                    ("YAus", f(src.y, 3)),
                    ("ZAus", z_text(src.z)),
                    ("XZiel", f(tgt.x, 3)),
                    ("YZiel", f(tgt.y, 3)),
                    ("ZZiel", z_text(tgt.z)),
                    ("vX", signiert(vx, 1)),
                    ("vY", signiert(vy, 1)),
                    ("vZ", if ist_3d { signiert(vz, 1) } else { String::new() }),
                    ("vGes", f(vg, 1)),
                    ("_ampel", ampel_stufe(vg, SCHWELLEN_CM)),
                ]));
            }
            None => nicht_benutzte.push(zeile(&[
                ("PunktNr", src.punkt_nr.clone()),
                ("XAus", f(src.x, 3)),
                ("YAus", f(src.y, 3)),
                ("ZAus", z_text(src.z)),
                ("XZiel", f(tgt.x, 3)),
                ("YZiel", f(tgt.y, 3)),
                ("ZZiel", z_text(tgt.z)),
            ])),
        }
    }

    let pass_nr: HashSet<String> = quelle.iter().map(|p| schluessel(&p.punkt_nr)).collect();
    let transformiert = alle_transformiert.unwrap_or(&erg.transformierte_punkte)
        .iter()
        .map(|p| {
            zeile(&[
                ("PunktNr", p.punkt_nr.clone()),
                ("X", f(p.x, 3)),
                ("Y", f(p.y, 3)),
                ("Z", z_text(p.z)),
                ("Typ", if pass_nr.contains(&schluessel(&p.punkt_nr)) { "Passpunkt" } else { "Neupunkt" }.to_string()),
            ])
        })
        .collect();

    let tabellen = Tabellen::from([
        ("BenutztePasspunkte".to_string(), benutzte),
        ("NichtBenutztePasspunkte".to_string(), nicht_benutzte),
        ("TransformiertePunkte".to_string(), transformiert),
    ]);
    Protokoll { vorlage: VORLAGE_TRANSFORMATION, praefix: "KoordTransformation", felder, tabellen, zeitpunkt: jetzt }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daten::modelle::jetzt;
    use crate::protokolle::pruefe_vollstaendig;
    use crate::rechnen::koord_transformation::{self as kt, Parameter};

    fn quelle() -> Vec<TransformPunkt> {
        vec![
            TransformPunkt::neu("A", 100.0, 200.0, 10.0),
            TransformPunkt::neu("B", 400.0, 250.0, 25.0),
            TransformPunkt::neu("C", 300.0, 500.0, 5.0),
            TransformPunkt::neu("D", 50.0, 450.0, 40.0),
        ]
    }

    #[test]
    fn protokoll_3d_mit_nicht_benutztem_punkt() {
        let p = Parameter { dx: 10.0, dy: 20.0, dz: 1.0, m: 5e-6, rz_rad: 2e-5, ..Parameter::default() };
        let q = quelle();
        let z: Vec<TransformPunkt> = q.iter().map(|s| kt::transformiere(TransformationsTyp::Helmert3D, &p, s)).collect();
        let erg = kt::berechnen(TransformationsTyp::Helmert3D, &q[..3], &z[..3], false).unwrap();
        let mut alle_quelle = q.clone();
        alle_quelle.push(TransformPunkt::neu("N", 200.0, 300.0, 12.0));
        let alle: Vec<TransformPunkt> = alle_quelle.iter().map(|s| kt::transformiere(erg.typ, &erg.parameter, s)).collect();
        let prot = protokoll(&erg, &q, &z, Some(&alle), "Demo", "Tester", jetzt());
        let rtf = pruefe_vollstaendig(&prot);
        assert_eq!(prot.felder["Ist3D"], "1");
        assert_eq!(prot.felder["HatNichtBenutzte"], "1");
        assert_eq!(prot.tabellen["BenutztePasspunkte"].len(), 3);
        assert_eq!(prot.tabellen["NichtBenutztePasspunkte"].len(), 1);
        assert_eq!(prot.tabellen["TransformiertePunkte"].len(), 5);
        assert_eq!(prot.tabellen["TransformiertePunkte"][3]["Typ"], "Passpunkt", "auch abgewählte Passpunkte bleiben Passpunkte");
        assert_eq!(prot.tabellen["TransformiertePunkte"][4]["Typ"], "Neupunkt");
        assert!(rtf.contains("Nicht benutzte identische Punkte"));
        assert!(prot.felder["Parameter"].contains("ppm"));
    }

    #[test]
    fn protokoll_2d_ohne_hoehenspalten() {
        let p = Parameter { a: 1.0, b: 0.1, dx: 5.0, dy: 6.0, ..Parameter::default() };
        let q = quelle();
        let z: Vec<TransformPunkt> = q.iter().map(|s| kt::transformiere(TransformationsTyp::Helmert2D, &p, s)).collect();
        let erg = kt::berechnen(TransformationsTyp::Helmert2D, &q, &z, false).unwrap();
        let prot = protokoll(&erg, &q, &z, None, "Demo", "Tester", jetzt());
        let rtf = pruefe_vollstaendig(&prot);
        assert_eq!(prot.felder["Ist3D"], "0");
        assert_eq!(prot.felder["HatNichtBenutzte"], "0");
        assert!(!rtf.contains("Nicht benutzte"));
        assert!(prot.felder["Parameter"].contains("Rotation"));
        assert!(!rtf.contains("Z-Aus"));
    }
}
