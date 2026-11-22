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

/// Normiert eine Winkeldifferenz auf −200 … +200 gon.
fn norm_200(gon: f64) -> f64 {
    let w = norm_400(gon + VOLLKREIS_GON / 2.0);
    w - VOLLKREIS_GON / 2.0
}

/// Umrechnung gon → cc.
const CC_JE_GON: f64 = 10_000.0;
/// Umrechnung gon → mgon.
const MGON_JE_GON: f64 = 1000.0;

/// Stationierung auf einem **bekannten Standpunkt**: Lage (und ggf. Höhe) stehen fest, bestimmt wird die
/// Orientierung als Mittel über alle aktiven Richtungen. Die Residuen zeigen die Abweichung jedes Ziels vom Mittel
/// (Winkel und Querabweichung) sowie die Streckendifferenz zur Koordinatenstrecke.
/// Redundanz = (aktive Richtungen − 1) + aktive Strecken (− 1 bei freiem Maßstab).
#[allow(clippy::too_many_arguments)]
pub fn station_bekannter_punkt(
    r_stand: f64,
    h_stand: f64,
    hoehe_stand: f64,
    ziele: &[StationierungsPunkt],
    instr_hoehe: f64,
    aktiv: &crate::daten::projekt::Aktivierung,
    berechnung_3d: bool,
    freier_massstab: bool,
) -> Result<StationierungsErgebnis, String> {
    let akt = |liste: &[bool], i: usize| liste.get(i).copied().unwrap_or(true);
    // Orientierungen je Ziel (nur aktive Richtungen mit Abstand zum Standpunkt)
    let mut o_i: Vec<Option<f64>> = Vec::with_capacity(ziele.len());
    for (i, z) in ziele.iter().enumerate() {
        let (d_r, d_h) = (z.r - r_stand, z.h - h_stand);
        let o = (akt(&aktiv.richtung, i) && d_r.hypot(d_h) >= MIN_ABSTAND_M).then(|| norm_400(d_r.atan2(d_h) / GON2RAD - z.hz));
        o_i.push(o);
    }
    let gueltige: Vec<f64> = o_i.iter().flatten().copied().collect();
    let Some(&bezug) = gueltige.first() else {
        return Err("Keine aktive Richtung zu einem Anschlussziel mit Koordinaten.".into());
    };
    let o = norm_400(bezug + gueltige.iter().map(|w| norm_200(w - bezug)).sum::<f64>() / gueltige.len() as f64);

    // Maßstab aus den aktiven Strecken (Koordinatenstrecke / gemessene Horizontalstrecke)
    let strecken: Vec<(usize, f64, f64)> = ziele
        .iter()
        .enumerate()
        .filter(|(i, _)| akt(&aktiv.strecke, *i))
        .map(|(i, z)| (i, (z.r - r_stand).hypot(z.h - h_stand), z.strecke * (z.v * GON2RAD).sin()))
        .filter(|(_, dist, dh)| *dist >= MIN_ABSTAND_M && *dh > MIN_ABSTAND_M)
        .collect();
    let massstab = if freier_massstab && !strecken.is_empty() { strecken.iter().map(|(_, d, s)| d / s).sum::<f64>() / strecken.len() as f64 } else { 1.0 };

    // Höhe: bekannt, sonst Mittel aus den aktiven Höhenzielen
    let hoehe_aus_ziel = |z: &StationierungsPunkt| z.hoehe + z.zielhoehe - instr_hoehe - z.strecke * (z.v * GON2RAD).cos();
    let hoehen_ziele: Vec<usize> = (0..ziele.len()).filter(|i| berechnung_3d && akt(&aktiv.hoehe, *i) && ziele[*i].hoehe.is_finite()).collect();
    let hoehe = if !berechnung_3d {
        0.0
    } else if hoehe_stand.is_finite() && hoehe_stand != 0.0 {
        hoehe_stand
    } else if !hoehen_ziele.is_empty() {
        hoehen_ziele.iter().map(|i| hoehe_aus_ziel(&ziele[*i])).sum::<f64>() / hoehen_ziele.len() as f64
    } else {
        0.0
    };

    let mut residuen = Vec::with_capacity(ziele.len());
    let mut summe_quer2 = 0.0;
    for (i, z) in ziele.iter().enumerate() {
        let dist = (z.r - r_stand).hypot(z.h - h_stand);
        let dh = z.strecke * (z.v * GON2RAD).sin();
        let v_winkel = o_i[i].map(|oi| norm_200(o - oi));
        if let Some(v) = v_winkel {
            summe_quer2 += (v * GON2RAD * dist * MM_PRO_M).powi(2);
        }
        let strecke_aktiv = akt(&aktiv.strecke, i) && dh > MIN_ABSTAND_M;
        let hoehe_aktiv = hoehen_ziele.contains(&i) && hoehe != 0.0;
        residuen.push(PunktResiduum {
            punkt_nr: z.punkt_nr.clone(),
            strecke_h: dh,
            v_winkel_cc: v_winkel.map_or(f64::NAN, |v| v * CC_JE_GON),
            v_quer_mm: v_winkel.map_or(f64::NAN, |v| v * GON2RAD * dist * MM_PRO_M),
            v_strecke_mm: if strecke_aktiv { (dist - massstab * dh) * MM_PRO_M } else { f64::NAN },
            v_hoehe_mm: if hoehe_aktiv { (hoehe - hoehe_aus_ziel(z)) * MM_PRO_M } else { f64::NAN },
            richtung_aktiv: v_winkel.is_some(),
            strecke_aktiv,
            hoehe_aktiv,
            gewicht_richtung: f64::NAN,
        });
    }
    let n_dir = gueltige.len() as i32;
    let redundanz = (n_dir - 1) + strecken.len() as i32 - i32::from(freier_massstab && !strecken.is_empty());
    let s0_mm = if n_dir > 1 { (summe_quer2 / f64::from(n_dir - 1)).sqrt() } else { 0.0 };
    Ok(StationierungsErgebnis {
        r: r_stand,
        h: h_stand,
        hoehe,
        orientierung_gon: o,
        massstab,
        s0_mm,
        redundanz: redundanz.max(0),
        iterationen: 0,
        konvergiert: true,
        berechnung_3d: berechnung_3d && hoehe != 0.0,
        ist_lokales_system: false,
        warnung_hoehe: String::new(),
        residuen,
        konfidenz: None,
    })
}

/// Ergebnis einer Orientierungskontrolle (Messung zu einem Anschlusspunkt bei bestehender Stationierung).
#[derive(Clone, Debug, PartialEq)]
pub struct KontrollErgebnis {
    /// Richtungsabweichung gemessen − Soll [mgon].
    pub d_hz_mgon: f64,
    /// Querabweichung im Zielpunkt [mm].
    pub quer_mm: f64,
    /// Streckenabweichung (gemessen × Maßstab − Soll) [mm]; NaN ohne Strecke.
    pub laengs_mm: f64,
    /// Koordinatenstrecke Standpunkt → Ziel [m].
    pub strecke_soll_m: f64,
}

/// Vergleicht eine Kontrollmessung mit der Stationierung. `None`, wenn Standpunkt und Ziel zusammenfallen.
pub fn kontrolle(station: &StationierungsErgebnis, ziel_r: f64, ziel_h: f64, hz: f64, v: f64, strecke: f64) -> Option<KontrollErgebnis> {
    let (d_r, d_h) = (ziel_r - station.r, ziel_h - station.h);
    let dist = d_r.hypot(d_h);
    if dist < MIN_ABSTAND_M {
        return None;
    }
    let t = norm_400(d_r.atan2(d_h) / GON2RAD);
    let d_hz = norm_200(norm_400(hz + station.orientierung_gon) - t);
    let m = if station.massstab > 0.0 { station.massstab } else { 1.0 };
    let laengs_mm = if strecke > 0.0 { (m * strecke * (v * GON2RAD).sin() - dist) * MM_PRO_M } else { f64::NAN };
    Some(KontrollErgebnis { d_hz_mgon: d_hz * MGON_JE_GON, quer_mm: d_hz * GON2RAD * dist * MM_PRO_M, laengs_mm, strecke_soll_m: dist })
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

    fn ziel(nr: &str, r: f64, h: f64, hz: f64, strecke: f64) -> StationierungsPunkt {
        StationierungsPunkt { punkt_nr: nr.into(), r, h, hoehe: 0.0, hz, v: 100.0, strecke, zielhoehe: 0.0 }
    }

    #[test]
    fn bekannter_standpunkt_mittelt_die_orientierung() {
        use crate::daten::projekt::Aktivierung;
        // Standpunkt (0,0), wahre Orientierung 50 gon; Ziel B mit +2 mgon Richtungsfehler
        let ziele = vec![ziel("A", 0.0, 100.0, 350.0, 100.0), ziel("B", 100.0, 0.0, 50.002, 100.0)];
        let akt = Aktivierung { richtung: vec![true; 2], strecke: vec![true; 2], hoehe: vec![false; 2] };
        let e = station_bekannter_punkt(0.0, 0.0, f64::NAN, &ziele, 1.5, &akt, false, false).unwrap();
        assert!((e.orientierung_gon - 49.999).abs() < 1e-9);
        assert!((e.residuen[0].v_winkel_cc + 10.0).abs() < 1e-6);
        assert!((e.residuen[1].v_winkel_cc - 10.0).abs() < 1e-6);
        assert_eq!(e.redundanz, 3);
        assert!(e.konvergiert);
        // Ein Ziel genügt, ohne Kontrolle
        let e = station_bekannter_punkt(0.0, 0.0, f64::NAN, &ziele[..1], 1.5, &Aktivierung { richtung: vec![true], strecke: vec![false], hoehe: vec![false] }, false, false).unwrap();
        assert!((e.orientierung_gon - 50.0).abs() < 1e-9);
        assert_eq!(e.redundanz, 0);
        // Orientierungen beiderseits von 0 gon werden richtig gemittelt
        let um_null = vec![ziel("A", 0.0, 100.0, 0.001, 100.0), ziel("B", 100.0, 0.0, 99.999, 100.0)];
        let e = station_bekannter_punkt(0.0, 0.0, f64::NAN, &um_null, 0.0, &Aktivierung { richtung: vec![true; 2], strecke: vec![false; 2], hoehe: vec![false; 2] }, false, false).unwrap();
        assert!(e.orientierung_gon.abs() < 1e-9 || (e.orientierung_gon - 400.0).abs() < 1e-9);
        assert!(station_bekannter_punkt(0.0, 0.0, 0.0, &ziele, 0.0, &Aktivierung { richtung: vec![false; 2], ..akt }, false, false).is_err());
    }

    #[test]
    fn kontrolle_zeigt_abweichungen() {
        let st = StationierungsErgebnis { r: 0.0, h: 0.0, orientierung_gon: 10.0, massstab: 1.0, ..StationierungsErgebnis::default() };
        // Ziel genau östlich (t = 100 gon): erwartetes Hz = 90 gon; gemessen 90.001 gon und 100.004 m
        let k = kontrolle(&st, 100.0, 0.0, 90.001, 100.0, 100.004).unwrap();
        assert!((k.d_hz_mgon - 1.0).abs() < 1e-6);
        assert!((k.quer_mm - 1.0 * GON2RAD / 1000.0 * 100.0 * 1000.0).abs() < 1e-6);
        assert!((k.laengs_mm - 4.0).abs() < 1e-6);
        assert!(kontrolle(&st, 0.0, 0.0, 0.0, 100.0, 1.0).is_none());
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
