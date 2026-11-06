// Zweck:    Reine Rechenlogik der Polarpunktberechnung: Lage mit Maßstab und Exzentrizitäten,
//           Maßstabsermittlung nach Rechenparameter, Orientierung aus einem einzelnen Anschlussziel.
//           Keine Abhängigkeit zu Oberfläche oder Dateien, daher vollständig testbar.
// Hardware: keine
// Autor:    Johann Gerner

use std::f64::consts::PI;

use crate::daten::modelle::{PunktResiduum, StationierungsErgebnis, StationierungsPunkt};
use crate::daten::rechenparameter::Rechenparameter;
use crate::grundlagen::format::f;

/// Umrechnung gon → rad.
const GON2RAD: f64 = PI / 200.0;
/// Vollkreis in gon.
const VOLLKREIS_GON: f64 = 400.0;
/// Exzentrizitäten, die betragsmäßig größer sind, lösen eine Information aus [m].
pub const EXZ_INFO_GRENZE_M: f64 = 2.0;
/// Plausibler Maßstabsbereich (untere Grenze).
pub const MASSSTAB_MIN: f64 = 0.9;
/// Plausibler Maßstabsbereich (obere Grenze).
pub const MASSSTAB_MAX: f64 = 1.1;
/// Betrag, unter dem ein Maßstab als 1 gilt.
pub const MASSSTAB_EINS_TOLERANZ: f64 = 1e-9;
/// Unterhalb dieser Anzahl Anschlusspunkte wird eine Information ausgegeben.
pub const MIN_ANSCHLUSSPUNKTE_INFO: usize = 3;
/// Abweichung beim Überschreiben eines vorhandenen Polarpunkts, ab der nachgefragt wird [m].
pub const UEBERSCHREIBEN_RUECKFRAGE_M: f64 = 0.05;
/// Abweichung zu einem Festpunkt gleicher Nummer, ab der ein Hinweis erscheint [m].
pub const FESTPUNKT_ABWEICHUNG_INFO_M: f64 = 0.05;
/// Kleinste Strecke, die als Abstand gilt [m].
const MIN_ABSTAND_M: f64 = 1e-9;
/// Millimeter pro Meter.
const MM_PRO_M: f64 = 1000.0;
/// Stellen bei der Anzeige des Maßstabs.
const MASSSTAB_STELLEN: usize = 6;

/// Auswahl des Maßstabs, der bei Polarpunkten auf die Horizontalstrecke wirkt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MassstabModus {
    /// Kein Maßstab (m = 1) – Standard.
    Eins,
    /// Maßstab der Stationierung.
    Berechnet,
    /// Fest vorgegebener Wert.
    Vorgegeben,
}

impl MassstabModus {
    /// Name wie in der XML-Datei.
    pub fn name(self) -> &'static str {
        match self {
            Self::Eins => "Eins",
            Self::Berechnet => "Berechnet",
            Self::Vorgegeben => "Vorgegeben",
        }
    }

    /// Findet den Modus anhand des Namens (ohne Beachtung der Groß-/Kleinschreibung).
    pub fn aus_name(name: &str) -> Option<Self> {
        [Self::Eins, Self::Berechnet, Self::Vorgegeben]
            .into_iter()
            .find(|m| m.name().eq_ignore_ascii_case(name))
    }
}

/// Verwendeter Maßstab samt Hinweistext (leer, wenn nichts zu melden ist).
#[derive(Clone, Debug, PartialEq)]
pub struct MassstabErgebnis {
    pub wert: f64,
    pub hinweis: String,
}

impl MassstabErgebnis {
    fn neu(wert: f64, hinweis: impl Into<String>) -> Self {
        Self { wert, hinweis: hinweis.into() }
    }
}

/// Gilt der Maßstab als 1?
pub fn ist_massstab_eins(m: f64) -> bool {
    (m - 1.0).abs() < MASSSTAB_EINS_TOLERANZ
}

/// Liegt der Maßstab im plausiblen Bereich?
pub fn massstab_plausibel(m: f64) -> bool {
    m > MASSSTAB_MIN && m < MASSSTAB_MAX
}

/// Ist die Längs- oder Querexzentrizität betragsmäßig größer als 2 m?
pub fn exzentrizitaet_gross(e_laengs: f64, e_quer: f64) -> bool {
    e_laengs.abs() > EXZ_INFO_GRENZE_M || e_quer.abs() > EXZ_INFO_GRENZE_M
}

/// Horizontaler Abstand aus Koordinatendifferenzen [m].
pub fn abstand(d_r: f64, d_h: f64) -> f64 {
    d_r.hypot(d_h)
}

/// Lagekoordinaten eines Polarpunkts.
///
/// `hz_orientiert_gon` ist die orientierte Messrichtung, `d_horiz` die Horizontalstrecke zum Reflektor.
/// `e_laengs` positiv = weiter vom Standpunkt weg, `e_quer` positiv = rechts der Messrichtung.
/// Der Maßstab wirkt nur auf die Strecke, nicht auf die Exzentrizitäten.
pub fn lage(
    r_stand: f64,
    h_stand: f64,
    hz_orientiert_gon: f64,
    d_horiz: f64,
    massstab: f64,
    e_laengs: f64,
    e_quer: f64,
) -> (f64, f64) {
    let (sin, cos) = (hz_orientiert_gon * GON2RAD).sin_cos();
    let r = r_stand + massstab * d_horiz * sin + e_laengs * sin + e_quer * cos;
    let h = h_stand + massstab * d_horiz * cos + e_laengs * cos - e_quer * sin;
    (r, h)
}

/// Ermittelt den anzuwendenden Maßstab. Bei m ≠ 1 oder Rückfall auf 1 enthält das Ergebnis einen Hinweis.
pub fn ermittle_massstab(p: &Rechenparameter, station: Option<&StationierungsErgebnis>) -> MassstabErgebnis {
    match p.polar_massstab_modus {
        MassstabModus::Vorgegeben => {
            let m = p.polar_massstab_wert;
            if !massstab_plausibel(m) {
                return MassstabErgebnis::neu(
                    1.0,
                    format!("Vorgegebener Maßstab {} unplausibel – m = 1 verwendet", f(m, MASSSTAB_STELLEN)),
                );
            }
            if ist_massstab_eins(m) {
                return MassstabErgebnis::neu(1.0, "");
            }
            let quelle = if p.polar_massstab_quelle.trim().is_empty() {
                "vorgegeben".to_string()
            } else {
                format!("vorgegeben, {}", p.polar_massstab_quelle)
            };
            MassstabErgebnis::neu(
                m,
                format!("Berechnung mit Maßstab m = {} ({quelle})", f(m, MASSSTAB_STELLEN)),
            )
        }
        MassstabModus::Berechnet => {
            let Some(station) = station.filter(|s| !s.ist_lokales_system) else {
                return MassstabErgebnis::neu(1.0, "Maßstab nicht berechenbar – m = 1 verwendet");
            };
            let m = station.massstab;
            if !massstab_plausibel(m) {
                return MassstabErgebnis::neu(1.0, "Maßstab nicht berechenbar – m = 1 verwendet");
            }
            if ist_massstab_eins(m) {
                return if p.freier_massstab {
                    MassstabErgebnis::neu(1.0, "")
                } else {
                    MassstabErgebnis::neu(1.0, "Maßstab nicht berechenbar (freier Maßstab aus) – m = 1 verwendet")
                };
            }
            MassstabErgebnis::neu(
                m,
                format!("Berechnung mit Maßstab m = {} (berechnet)", f(m, MASSSTAB_STELLEN)),
            )
        }
        MassstabModus::Eins => MassstabErgebnis::neu(1.0, ""),
    }
}

/// Orientierung bei bekanntem Standpunkt und genau einem Anschlussziel: o = t(Standpunkt→Ziel) − Hz.
///
/// Der Maßstab (Koordinatenstrecke / gemessene Strecke) wird mitgeliefert, sofern die Strecke aktiv ist.
/// Winkel- und Querabweichung sind nicht bestimmbar (NaN).
/// Liefert `None`, wenn die Richtung nicht aktiv oder Standpunkt und Ziel identisch sind.
#[allow(clippy::too_many_arguments)]
pub fn orientierung_aus_einem_ziel(
    r_stand: f64,
    h_stand: f64,
    hoehe_stand: f64,
    ziel: &StationierungsPunkt,
    instr_hoehe: f64,
    akt_hz: bool,
    akt_str: bool,
    akt_hoe: bool,
    berechnung_3d: bool,
) -> Option<StationierungsErgebnis> {
    if !akt_hz {
        return None;
    }
    let d_r = ziel.r - r_stand;
    let d_h = ziel.h - h_stand;
    let dist = d_r.hypot(d_h);
    if dist < MIN_ABSTAND_M {
        return None;
    }

    let t_gon = d_r.atan2(d_h) / GON2RAD;
    let o = norm_400(t_gon - ziel.hz);

    let dh = ziel.strecke * (ziel.v * GON2RAD).sin();
    let m = if akt_str && dh > MIN_ABSTAND_M { dist / dh } else { 1.0 };

    let hat_hoehe = berechnung_3d && akt_hoe && ziel.hoehe != 0.0;
    let hoehe = standpunkt_hoehe(hat_hoehe, ziel, instr_hoehe, hoehe_stand, berechnung_3d);

    let residuum = PunktResiduum {
        punkt_nr: ziel.punkt_nr.clone(),
        strecke_h: dh,
        v_winkel_cc: f64::NAN,
        v_quer_mm: f64::NAN,
        v_strecke_mm: if akt_str { (dh - dist) * MM_PRO_M } else { f64::NAN },
        v_hoehe_mm: if hat_hoehe { 0.0 } else { f64::NAN },
        richtung_aktiv: true,
        strecke_aktiv: akt_str,
        hoehe_aktiv: hat_hoehe,
        gewicht_richtung: f64::NAN,
    };

    Some(StationierungsErgebnis {
        r: r_stand,
        h: h_stand,
        hoehe,
        orientierung_gon: o,
        massstab: m,
        s0_mm: 0.0,
        redundanz: 0,
        iterationen: 0,
        konvergiert: true,
        berechnung_3d: berechnung_3d && hoehe != 0.0,
        ist_lokales_system: false,
        warnung_hoehe: String::new(),
        residuen: vec![residuum],
        konfidenz: None,
    })
}

/// Standpunkthöhe: aus dem Anschlussziel, sonst die bekannte Höhe des Standpunkts.
fn standpunkt_hoehe(aus_ziel: bool, z: &StationierungsPunkt, ih: f64, hoehe_stand: f64, berechnung_3d: bool) -> f64 {
    if !berechnung_3d {
        return 0.0;
    }
    if aus_ziel {
        return z.hoehe + z.zielhoehe - ih - z.strecke * (z.v * GON2RAD).cos();
    }
    hoehe_stand
}

/// Normiert einen Winkel auf 0..400 gon.
fn norm_400(gon: f64) -> f64 {
    let w = gon % VOLLKREIS_GON;
    if w < 0.0 { w + VOLLKREIS_GON } else { w }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lage_ohne_exzentrizitaet() {
        // Richtung 100 gon = Osten
        let (r, h) = lage(1000.0, 2000.0, 100.0, 50.0, 1.0, 0.0, 0.0);
        assert!((r - 1050.0).abs() < 1e-9);
        assert!((h - 2000.0).abs() < 1e-9);
    }

    #[test]
    fn exzentrizitaeten_wirken_laengs_und_quer() {
        // Messrichtung Norden (0 gon): längs = +H, quer rechts = +R
        let (r, h) = lage(0.0, 0.0, 0.0, 10.0, 1.0, 2.0, 0.5);
        assert!((r - 0.5).abs() < 1e-9);
        assert!((h - 12.0).abs() < 1e-9);
    }

    #[test]
    fn massstab_wirkt_nur_auf_die_strecke() {
        let (r, h) = lage(0.0, 0.0, 0.0, 100.0, 1.001, 1.0, 0.0);
        assert!((h - (100.1 + 1.0)).abs() < 1e-9);
        assert!(r.abs() < 1e-9);
    }

    #[test]
    fn massstab_nach_modus() {
        let mut p = Rechenparameter::default();
        assert_eq!(ermittle_massstab(&p, None), MassstabErgebnis::neu(1.0, ""));

        p.polar_massstab_modus = MassstabModus::Vorgegeben;
        p.polar_massstab_wert = 1.0002;
        let e = ermittle_massstab(&p, None);
        assert_eq!(e.wert, 1.0002);
        assert!(e.hinweis.contains("1.000200"));

        p.polar_massstab_wert = 2.0;
        assert_eq!(ermittle_massstab(&p, None).wert, 1.0);

        p.polar_massstab_modus = MassstabModus::Berechnet;
        let station = StationierungsErgebnis { massstab: 0.9997, ..StationierungsErgebnis::default() };
        assert_eq!(ermittle_massstab(&p, Some(&station)).wert, 0.9997);
        assert_eq!(ermittle_massstab(&p, None).wert, 1.0);
        let lokal = StationierungsErgebnis { ist_lokales_system: true, massstab: 0.9997, ..StationierungsErgebnis::default() };
        assert_eq!(ermittle_massstab(&p, Some(&lokal)).wert, 1.0);
    }

    #[test]
    fn orientierung_bei_einem_ziel() {
        // Ziel genau östlich (t = 100 gon), gemessene Richtung 40 gon → Orientierung 60 gon
        let ziel = StationierungsPunkt {
            punkt_nr: "Z".into(),
            r: 100.0,
            h: 0.0,
            hoehe: 0.0,
            hz: 40.0,
            v: 100.0,
            strecke: 100.0,
            zielhoehe: 0.0,
        };
        let e = orientierung_aus_einem_ziel(0.0, 0.0, 0.0, &ziel, 0.0, true, true, false, false).unwrap();
        assert!((e.orientierung_gon - 60.0).abs() < 1e-9);
        assert!((e.massstab - 1.0).abs() < 1e-12);
        assert!(e.konvergiert);
        assert!(orientierung_aus_einem_ziel(0.0, 0.0, 0.0, &ziel, 0.0, false, true, false, false).is_none());
        assert!(orientierung_aus_einem_ziel(100.0, 0.0, 0.0, &ziel, 0.0, true, true, false, false).is_none());
    }
}
