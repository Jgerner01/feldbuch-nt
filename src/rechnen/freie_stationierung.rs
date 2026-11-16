// Zweck:    Freie Stationierung nach Grillmayer & Blauensteiner (VGI 4/2022): Helmert-Näherung
//           (Methode 3.1) für Näherungskoordinaten, Orientierung und Maßstab, danach gewichtete Lageausgleichung
//           (Methode 3.2) mit den Unbekannten R, H und Orientierung. Der Maßstab wird NICHT mitausgeglichen,
//           er bleibt der Wert der Helmert-Näherung. Festpunktzwang.
//           Aus der Normalmatrix der Lageausgleichung folgt die Konfidenzellipse des Standpunkts.
//           Beobachtungen (Richtung/Strecke/Höhe) sind je Punkt einzeln aktivierbar.
//           Sonderfall n = 2 mit freiem Maßstab: direkte Helmert-Lösung ohne Ausgleichung (r = 0, keine Konfidenzellipse);
//           sie braucht alle 4 Beobachtungen. Mit festem Maßstab gilt auch bei n = 2 die Ausgleichung, damit
//           abgeschaltete Beobachtungen die Standpunktlage nicht beeinflussen.
// Hardware: keine
// Autor:    Johann Gerner

use std::f64::consts::PI;

use crate::daten::modelle::{InstrumentParameter, Konfidenzellipse, PunktResiduum, StationierungsErgebnis, StationierungsPunkt};

/// Umrechnung gon → rad.
const GON2RAD: f64 = PI / 200.0;
/// Umrechnung rad → gon.
const RAD2GON: f64 = 200.0 / PI;
/// Umrechnung cc → rad.
const CC2RAD: f64 = PI / 2_000_000.0;
/// Umrechnung rad → cc.
const RAD2CC: f64 = 2_000_000.0 / PI;
/// Vollkreis in gon.
const VOLLKREIS_GON: f64 = 400.0;
/// Mindestanzahl Beobachtungen bei n ≥ 3.
const MIN_BEOBACHTUNGEN: usize = 3;
/// Maximale Iterationen der Ausgleichung.
const MAX_ITERATIONEN: usize = 50;
/// Konvergenzschwelle für Koordinatenänderungen [m].
const KONVERGENZ_KOORDINATE_M: f64 = 1e-7;
/// Konvergenzschwelle für die Orientierungsänderung [rad].
const KONVERGENZ_ORIENTIERUNG_RAD: f64 = 1e-9;
/// Untergrenze der Varianz (verhindert Division durch 0).
const MIN_VARIANZ: f64 = 1e-20;
/// Untergrenze der Winkelvarianz bei den relativen Gewichten.
const MIN_SIGMA2: f64 = 1e-30;
/// Kleinste Strecke, ab der ein Winkelresiduum bestimmt wird [m].
const MIN_STRECKE_M: f64 = 1e-6;
/// Pivot-Schwelle der Gauß-Elimination.
const PIVOT_SCHWELLE: f64 = 1e-14;
/// Nennerschwelle der Helmert-Transformation.
const HELMERT_NENNER_MIN: f64 = 1e-12;
/// Stabilisierung der Höhengewichte.
const HOEHEN_GEWICHT_EPS: f64 = 1e-9;
/// Faktor der Fehlergrenze für den Rückfall auf 2D.
const HOEHEN_GRENZE_FAKTOR: f64 = 3.0;
/// Millimeter pro Meter.
const MM_PRO_M: f64 = 1000.0;
/// Konfidenzniveau der Standpunkt-Ellipse.
const KONFIDENZ_NIVEAU: f64 = 0.95;

/// Ergebnis der Lageausgleichung: Quer- und Längsresiduen [m], Iterationen, Konvergenz, letzte Normalmatrix.
type Ausgleichung = (Vec<f64>, Vec<f64>, usize, bool, Option<[[f64; 3]; 3]>);

/// Optionen der Berechnung (Standardwerte wie in der C#-Version).
#[derive(Clone, Debug)]
pub struct Optionen {
    pub param: InstrumentParameter,
    pub freier_massstab: bool,
    pub aktiv_richtung: Option<Vec<bool>>,
    pub aktiv_strecke: Option<Vec<bool>>,
    pub aktiv_hoehe: Option<Vec<bool>>,
    pub berechnung_3d: bool,
    pub fehlergrenze_mm_hoehe: f64,
    pub zentrierfehler_ziel_mm: f64,
    pub zentrierfehler_standpunkt_mm: f64,
}

impl Default for Optionen {
    fn default() -> Self {
        Self {
            param: InstrumentParameter::default(),
            freier_massstab: true,
            aktiv_richtung: None,
            aktiv_strecke: None,
            aktiv_hoehe: None,
            berechnung_3d: true,
            fehlergrenze_mm_hoehe: 10.0,
            zentrierfehler_ziel_mm: 0.0,
            zentrierfehler_standpunkt_mm: 0.0,
        }
    }
}

/// Berechnet die freie Stationierung.
pub fn berechnen(punkte: &[StationierungsPunkt], instrumentenhoehe: f64, optionen: &Optionen) -> Result<StationierungsErgebnis, String> {
    let mut param = optionen.param.clone();
    // Gewichtung bei nahen Zielen (nur Polarmodul): Zentrierfehler gehen in die Varianz ein.
    if optionen.zentrierfehler_ziel_mm > 0.0 || optionen.zentrierfehler_standpunkt_mm > 0.0 {
        param.m_spz_mm = optionen.zentrierfehler_standpunkt_mm;
        param.m_zpz_mm = optionen.zentrierfehler_ziel_mm;
    }
    let n = punkte.len();
    if n < 2 {
        return Err("Mindestens 2 Anschlusspunkte erforderlich.".to_string());
    }

    let akt_hz = optionen.aktiv_richtung.clone().unwrap_or_else(|| vec![true; n]);
    let akt_str = optionen.aktiv_strecke.clone().unwrap_or_else(|| vec![true; n]);
    let mut akt_hoe = optionen.aktiv_hoehe.clone().unwrap_or_else(|| akt_str.clone());
    if akt_hz.len() != n || akt_str.len() != n || akt_hoe.len() != n {
        return Err("Die Aktivierungslisten passen nicht zur Punktanzahl.".to_string());
    }

    for (aktiv, p) in akt_hoe.iter_mut().zip(punkte) {
        *aktiv &= p.hoehe.is_finite();
    }
    let n_dir = akt_hz.iter().filter(|b| **b).count();
    let n_dist = akt_str.iter().filter(|b| **b).count();
    if n_dir < 1 {
        return Err("Mindestens 1 aktive Richtungsbeobachtung erforderlich \
                    (Orientierungsunbekannte sonst unbestimmt)."
            .to_string());
    }
    if n == 2 {
        let min_obs = if optionen.freier_massstab { 4 } else { 3 };
        if n_dir + n_dist < min_obs {
            return Err(format!(
                "Für 2 Punkte {}sind mindestens {min_obs} aktive Beobachtungen erforderlich \
                 (aktiv: {n_dir} Richtungen + {n_dist} Strecken = {}).",
                if optionen.freier_massstab { "mit freiem Maßstab " } else { "" },
                n_dir + n_dist
            ));
        }
    } else if n_dir + n_dist < MIN_BEOBACHTUNGEN {
        return Err(format!("Mindestens 3 aktive Beobachtungen erforderlich (aktiv: {n_dir} Richtungen + {n_dist} Strecken = {}).", n_dir + n_dist));
    }

    let hz_rad: Vec<f64> = punkte.iter().map(|p| p.hz * GON2RAD).collect();
    let v_rad: Vec<f64> = punkte.iter().map(|p| p.v * GON2RAD).collect();
    let dh: Vec<f64> = punkte.iter().zip(&v_rad).map(|(p, v)| p.strecke * v.sin()).collect();

    // Schritt 1: Helmert-Näherungslösung
    let (r0, h0, mut o0, massstab) = helmert_transformation(punkte, &hz_rad, &dh, optionen.freier_massstab)?;

    // Schritt 2: Ausgleichung oder Direktlösung. Die Direktlösung (2 Punkte, freier Maßstab) verwendet alle 4 Beobachtungen
    // (siehe Prüfung oben); in allen anderen Fällen beachtet die Ausgleichung die Aktivierung je Beobachtung.
    let direkt = n == 2 && optionen.freier_massstab;
    let (mut r_fs, mut h_fs) = (r0, h0);
    let (v_quer_m, v_str_m, iterationen, konvergiert, kov_normal) = if !direkt {
        voll_ausgleichung(punkte, &hz_rad, &dh, &param, &akt_hz, &akt_str, &mut r_fs, &mut h_fs, &mut o0)
    } else {
        let mut v_quer = vec![0.0; n];
        let mut v_str = vec![0.0; n];
        for i in 0..n {
            let d_r = punkte[i].r - r_fs;
            let d_h = punkte[i].h - h_fs;
            let s0i = d_r.hypot(d_h);
            v_quer[i] = norm_winkel(d_r.atan2(d_h) - hz_rad[i] - o0) * s0i;
            v_str[i] = dh[i] - s0i;
        }
        (v_quer, v_str, 0, true, None)
    };

    // Schritt 3: Standardabweichung der Gewichtseinheit
    let m0_m = param.m0_mm / MM_PRO_M;
    let m_spz = param.m_spz_mm / MM_PRO_M;
    let m_zpz = param.m_zpz_mm / MM_PRO_M;
    let r: i32 = if direkt { 0 } else { (n_dir + n_dist) as i32 - MIN_BEOBACHTUNGEN as i32 };
    let mut vtpv = 0.0;
    for i in 0..n {
        let s0_i = (punkte[i].r - r_fs).hypot(punkte[i].h - h_fs);
        if akt_hz[i] {
            let m_r_m = param.m_r_cc * CC2RAD * s0_i;
            let mq2 = m_r_m * m_r_m + m_spz * m_spz + m_zpz * m_zpz;
            let p_d = m0_m * m0_m / mq2.max(MIN_VARIANZ);
            vtpv += p_d * (v_quer_m[i] * MM_PRO_M).powi(2);
        }
        if akt_str[i] {
            let ms_m = (param.msk_mm + s0_i / MM_PRO_M * param.msv_ppm) / MM_PRO_M;
            let ms2 = ms_m * ms_m + m_spz * m_spz + m_zpz * m_zpz;
            let p_s = m0_m * m0_m / ms2.max(MIN_VARIANZ);
            vtpv += p_s * (v_str_m[i] * MM_PRO_M).powi(2);
        }
    }
    let s0_mm = if r > 0 { (vtpv / f64::from(r)).sqrt() } else { 0.0 };
    let konfidenz = kov_normal.and_then(|normal| konfidenz_ellipse(&normal, param.m0_mm, s0_mm, r));

    // Schritt 4: Höhenberechnung
    let mut h_mittel = 0.0;
    let mut ist_3d = optionen.berechnung_3d && akt_hoe.iter().any(|b| *b);
    let mut warn_hoehe = String::new();
    if ist_3d {
        let mut h_station = vec![0.0; n];
        let (mut summe_zaehler, mut summe_nenner) = (0.0, 0.0);
        for i in 0..n {
            if !akt_hoe[i] {
                continue;
            }
            h_station[i] = punkte[i].hoehe + punkte[i].zielhoehe - instrumentenhoehe - punkte[i].strecke * v_rad[i].cos();
            let w = 1.0 / (dh[i] * dh[i] + HOEHEN_GEWICHT_EPS);
            summe_zaehler += w * h_station[i];
            summe_nenner += w;
        }
        h_mittel = if summe_nenner > 0.0 { summe_zaehler / summe_nenner } else { 0.0 };

        let grenze = HOEHEN_GRENZE_FAKTOR * optionen.fehlergrenze_mm_hoehe;
        for i in 0..n {
            if !akt_hoe[i] {
                continue;
            }
            let v_h_mm = (h_station[i] - h_mittel) * MM_PRO_M;
            if v_h_mm.abs() > grenze {
                warn_hoehe = format!(
                    "Höhenabweichung an Punkt {}: {v_h_mm:.1} mm überschreitet {grenze:.1} mm. \
                     Ergebnis nur in 2D; Höhe nicht bestimmt.",
                    punkte[i].punkt_nr
                );
                ist_3d = false;
                h_mittel = 0.0;
                break;
            }
        }
    }

    // Schritt 5: Residuen zusammenstellen
    let gewicht_rel = berechne_richtungsgewichte(punkte, &akt_hz, r_fs, h_fs, &param);
    let mut residuen = Vec::with_capacity(n);
    for i in 0..n {
        let s0_i = (punkte[i].r - r_fs).hypot(punkte[i].h - h_fs);
        let v_w_cc = if akt_hz[i] && s0_i > MIN_STRECKE_M { (v_quer_m[i] / s0_i) * RAD2CC } else { f64::NAN };
        let v_q_mm = if akt_hz[i] { v_quer_m[i] * MM_PRO_M } else { f64::NAN };
        let mut v_h_mm = f64::NAN;
        if ist_3d && akt_hoe[i] {
            let h_station = punkte[i].hoehe + punkte[i].zielhoehe - instrumentenhoehe - punkte[i].strecke * v_rad[i].cos();
            v_h_mm = (h_station - h_mittel) * MM_PRO_M;
        }
        residuen.push(PunktResiduum {
            punkt_nr: punkte[i].punkt_nr.clone(),
            strecke_h: dh[i],
            v_winkel_cc: v_w_cc,
            v_quer_mm: v_q_mm,
            v_strecke_mm: if akt_str[i] { v_str_m[i] * MM_PRO_M } else { f64::NAN },
            v_hoehe_mm: v_h_mm,
            richtung_aktiv: akt_hz[i],
            strecke_aktiv: akt_str[i],
            hoehe_aktiv: akt_hoe[i],
            gewicht_richtung: gewicht_rel[i],
        });
    }

    let orient_gon = (((o0 * RAD2GON) % VOLLKREIS_GON) + VOLLKREIS_GON) % VOLLKREIS_GON;

    Ok(StationierungsErgebnis {
        r: r_fs,
        h: h_fs,
        hoehe: h_mittel,
        orientierung_gon: orient_gon,
        massstab,
        s0_mm,
        redundanz: r,
        iterationen: iterationen as i32,
        konvergiert,
        berechnung_3d: ist_3d,
        ist_lokales_system: false,
        warnung_hoehe: warn_hoehe,
        residuen,
        konfidenz,
    })
}

/// Konfidenzfaktor k der zweidimensionalen Ellipse beim Niveau `KONFIDENZ_NIVEAU`.
/// A priori (`None`, σ0 bekannt): k² = χ²(2; 1−α) = −2·ln α.
/// A posteriori (`Some(r)`, s0 aus r Freiheitsgraden): k² = 2·F(2, r; 1−α) = r·(α^(−2/r) − 1).
fn konfidenzfaktor(redundanz: Option<i32>) -> f64 {
    let alpha = 1.0 - KONFIDENZ_NIVEAU;
    match redundanz {
        None => (-2.0 * alpha.ln()).sqrt(),
        Some(r) => {
            let r = f64::from(r.max(1));
            (r * (alpha.powf(-2.0 / r) - 1.0)).sqrt()
        }
    }
}

/// Konfidenzellipse der Standpunktlage aus der Normalmatrix N der Lageausgleichung (Unbekannte R, H, Orientierung).
/// Kovarianzmatrix K = σ0² · N⁻¹; σ0 = s0 (a posteriori) bei Redundanz > 0, sonst die a-priori-Genauigkeit m0.
fn konfidenz_ellipse(normal: &[[f64; 3]; 3], m0_mm: f64, s0_mm: f64, redundanz: i32) -> Option<Konfidenzellipse> {
    let spalte = |j: usize| {
        let mut einheit = [0.0_f64; 3];
        einheit[j] = 1.0;
        gauss_elim(normal, &einheit)
    };
    let (c0, c1) = (spalte(0)?, spalte(1)?);
    let (q_rr, q_hh, q_rh) = (c0[0], c1[1], 0.5 * (c0[1] + c1[0]));
    if !(q_rr > 0.0 && q_hh > 0.0) {
        return None;
    }
    let a_priori = redundanz <= 0;
    let sigma0_mm = if a_priori { m0_mm } else { s0_mm };
    let varianz0 = sigma0_mm * sigma0_mm;
    let (k_rr, k_hh, k_rh) = (varianz0 * q_rr, varianz0 * q_hh, varianz0 * q_rh);
    let mittel = 0.5 * (k_rr + k_hh);
    let radius = (0.25 * (k_rr - k_hh).powi(2) + k_rh * k_rh).sqrt();
    let faktor = konfidenzfaktor(if a_priori { None } else { Some(redundanz) });
    Some(Konfidenzellipse {
        a_mm: faktor * (mittel + radius).sqrt(),
        b_mm: faktor * (mittel - radius).max(0.0).sqrt(),
        theta_rad: 0.5 * (2.0 * k_rh).atan2(k_rr - k_hh),
        sigma_r_mm: k_rr.sqrt(),
        sigma_h_mm: k_hh.sqrt(),
        niveau: KONFIDENZ_NIVEAU,
        a_priori,
    })
}

/// Relatives Richtungsgewicht je Punkt (1 = größtes Gewicht aller aktiven Richtungen).
/// Bei n = 2 (Direktlösung) unwirksam → NaN.
fn berechne_richtungsgewichte(punkte: &[StationierungsPunkt], akt_hz: &[bool], r_stand: f64, h_stand: f64, param: &InstrumentParameter) -> Vec<f64> {
    let n = punkte.len();
    let mut rel = vec![f64::NAN; n];
    if n < 3 {
        return rel;
    }
    let m_r_rad = param.m_r_cc * CC2RAD;
    let c2 = (param.m_spz_mm / MM_PRO_M).powi(2) + (param.m_zpz_mm / MM_PRO_M).powi(2);

    let mut w = vec![f64::NAN; n];
    let mut max = 0.0_f64;
    for i in 0..n {
        if !akt_hz[i] {
            continue;
        }
        let s = (punkte[i].r - r_stand).hypot(punkte[i].h - h_stand);
        let sigma2 = m_r_rad * m_r_rad + if s > MIN_STRECKE_M { c2 / (s * s) } else { c2 / 1e-12 };
        w[i] = 1.0 / sigma2.max(MIN_SIGMA2);
        if w[i] > max {
            max = w[i];
        }
    }
    if max <= 0.0 {
        return rel;
    }
    for i in 0..n {
        if !w[i].is_nan() {
            rel[i] = w[i] / max;
        }
    }
    rel
}

/// Helmert-Transformation (Methode 3.1): Näherungskoordinaten, Orientierung und Maßstab.
fn helmert_transformation(punkte: &[StationierungsPunkt], hz_rad: &[f64], dh: &[f64], freier_massstab: bool) -> Result<(f64, f64, f64, f64), String> {
    let n = punkte.len();
    let nf = n as f64;
    let xi: Vec<f64> = (0..n).map(|i| dh[i] * hz_rad[i].cos()).collect();
    let eta: Vec<f64> = (0..n).map(|i| dh[i] * hz_rad[i].sin()).collect();

    let xi_s = xi.iter().sum::<f64>() / nf;
    let eta_s = eta.iter().sum::<f64>() / nf;
    let r_s = punkte.iter().map(|p| p.r).sum::<f64>() / nf;
    let h_s = punkte.iter().map(|p| p.h).sum::<f64>() / nf;

    let (mut nenner, mut summe_a, mut summe_b) = (0.0, 0.0, 0.0);
    for i in 0..n {
        let xi_r = xi[i] - xi_s;
        let eta_r = eta[i] - eta_s;
        let r_r = punkte[i].r - r_s;
        let h_r = punkte[i].h - h_s;
        nenner += eta_r * eta_r + xi_r * xi_r;
        summe_a += xi_r * r_r - eta_r * h_r;
        summe_b += eta_r * r_r + xi_r * h_r;
    }
    if nenner.abs() < HELMERT_NENNER_MIN {
        return Err("Helmert: Nenner zu klein (alle Punkte kollinear?).".to_string());
    }

    let a = summe_a / nenner;
    let b = summe_b / nenner;
    let m_frei = a.hypot(b);
    let o0 = a.atan2(b);

    let (a_eff, b_eff) = if freier_massstab { (a, b) } else { (o0.sin(), o0.cos()) };
    let r_fs = r_s - b_eff * eta_s - a_eff * xi_s;
    let h_fs = h_s - b_eff * xi_s + a_eff * eta_s;
    let m = if freier_massstab { m_frei } else { 1.0 };
    Ok((r_fs, h_fs, o0, m))
}

/// Gewichtete Lageausgleichung (Methode 3.2) mit den Unbekannten R, H und Orientierung; der Maßstab ist nicht dabei.
/// Inaktive Beobachtungen erhalten das Gewicht 0, ihre Residuen werden dennoch berechnet.
/// Liefert zusätzlich die Normalmatrix der letzten Iteration (für die Kovarianz).
#[allow(clippy::too_many_arguments)]
fn voll_ausgleichung(
    punkte: &[StationierungsPunkt],
    hz_rad: &[f64],
    dh: &[f64],
    param: &InstrumentParameter,
    akt_hz: &[bool],
    akt_str: &[bool],
    r_fs: &mut f64,
    h_fs: &mut f64,
    o0: &mut f64,
) -> Ausgleichung {
    let n = punkte.len();
    let m_obs = 2 * n;
    let m0_m = param.m0_mm / MM_PRO_M;
    let m_spz = param.m_spz_mm / MM_PRO_M;
    let m_zpz = param.m_zpz_mm / MM_PRO_M;

    let mut iterationen = 0;
    let mut konvergiert = false;
    let mut letzte_normal: Option<[[f64; 3]; 3]> = None;

    for iter in 0..MAX_ITERATIONEN {
        let mut a = vec![[0.0_f64; 3]; m_obs];
        let mut l = vec![0.0_f64; m_obs];
        let mut p = vec![0.0_f64; m_obs];

        for i in 0..n {
            let d_r = punkte[i].r - *r_fs;
            let d_h = punkte[i].h - *h_fs;
            let s0 = d_r.hypot(d_h);
            let t_i = d_r.atan2(d_h);
            let (sin_t, cos_t) = t_i.sin_cos();

            // Richtungsbeobachtung
            let i_d = 2 * i;
            l[i_d] = norm_winkel(t_i - hz_rad[i] - *o0) * s0;
            a[i_d] = [cos_t, -sin_t, s0];
            if akt_hz[i] {
                let m_r_m = param.m_r_cc * CC2RAD * s0;
                let mq2 = m_r_m * m_r_m + m_spz * m_spz + m_zpz * m_zpz;
                p[i_d] = (m0_m * m0_m) / mq2.max(MIN_VARIANZ);
            }

            // Streckenbeobachtung
            let i_s = 2 * i + 1;
            l[i_s] = dh[i] - s0;
            a[i_s] = [-sin_t, -cos_t, 0.0];
            if akt_str[i] {
                let ms_m = (param.msk_mm + s0 / MM_PRO_M * param.msv_ppm) / MM_PRO_M;
                let ms2 = ms_m * ms_m + m_spz * m_spz + m_zpz * m_zpz;
                p[i_s] = (m0_m * m0_m) / ms2.max(MIN_VARIANZ);
            }
        }

        let mut normal = [[0.0_f64; 3]; 3];
        let mut b_vec = [0.0_f64; 3];
        for i in 0..m_obs {
            for j in 0..3 {
                b_vec[j] += a[i][j] * p[i] * l[i];
                for k in 0..3 {
                    normal[j][k] += a[i][j] * p[i] * a[i][k];
                }
            }
        }

        let Some(dx) = gauss_elim(&normal, &b_vec) else {
            letzte_normal = None;
            break;
        };
        letzte_normal = Some(normal);
        *r_fs += dx[0];
        *h_fs += dx[1];
        *o0 = norm_winkel(*o0 + dx[2]);
        iterationen = iter + 1;

        if dx[0].abs() < KONVERGENZ_KOORDINATE_M && dx[1].abs() < KONVERGENZ_KOORDINATE_M && dx[2].abs() < KONVERGENZ_ORIENTIERUNG_RAD {
            konvergiert = true;
            break;
        }
    }

    // Endresiduen für alle Punkte (auch inaktive, als Information)
    let mut v_quer = vec![0.0; n];
    let mut v_str = vec![0.0; n];
    for i in 0..n {
        let d_r = punkte[i].r - *r_fs;
        let d_h = punkte[i].h - *h_fs;
        let s0 = d_r.hypot(d_h);
        v_quer[i] = norm_winkel(d_r.atan2(d_h) - hz_rad[i] - *o0) * s0;
        v_str[i] = dh[i] - s0;
    }
    (v_quer, v_str, iterationen, konvergiert, letzte_normal)
}

/// Normiert einen Winkel auf (−π, π].
fn norm_winkel(mut winkel: f64) -> f64 {
    while winkel > PI {
        winkel -= 2.0 * PI;
    }
    while winkel < -PI {
        winkel += 2.0 * PI;
    }
    winkel
}

/// Gauß-Elimination mit Spaltenpivotsuche für ein 3×3-System; `None` bei singulärer Matrix.
fn gauss_elim(a: &[[f64; 3]; 3], b: &[f64; 3]) -> Option<[f64; 3]> {
    let mut m = [[0.0_f64; 4]; 3];
    for i in 0..3 {
        m[i][..3].copy_from_slice(&a[i]);
        m[i][3] = b[i];
    }
    for spalte in 0..3 {
        let mut pivot = spalte;
        for zeile in spalte + 1..3 {
            if m[zeile][spalte].abs() > m[pivot][spalte].abs() {
                pivot = zeile;
            }
        }
        m.swap(spalte, pivot);
        if m[spalte][spalte].abs() < PIVOT_SCHWELLE {
            return None;
        }
        for zeile in 0..3 {
            if zeile == spalte {
                continue;
            }
            let faktor = m[zeile][spalte] / m[spalte][spalte];
            for k in spalte..4 {
                m[zeile][k] -= faktor * m[spalte][k];
            }
        }
    }
    Some([m[0][3] / m[0][0], m[1][3] / m[1][1], m[2][3] / m[2][2]])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn konfidenzfaktor_a_priori_und_a_posteriori() {
        // χ²(2; 0,95) = 5,991 → k = 2,4477
        assert!((konfidenzfaktor(None) - 2.447_746_8).abs() < 1e-6);
        // F(2, 10; 0,95) = 4,103 → k = sqrt(2 · 4,103) = 2,8646
        assert!((konfidenzfaktor(Some(10)) - 2.864_6).abs() < 1e-3);
        // Viele Freiheitsgrade nähern sich dem a-priori-Wert
        assert!((konfidenzfaktor(Some(1_000_000)) - konfidenzfaktor(None)).abs() < 1e-3);
    }

    #[test]
    fn konfidenzellipse_aus_normalmatrix() {
        // N = diag(4, 1, 1) → Q = diag(0,25; 1; 1); m0 = 2 mm → σR = 1 mm, σH = 2 mm; große Achse in H-Richtung
        let normal = [[4.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
        let k = konfidenz_ellipse(&normal, 2.0, 0.0, 0).unwrap();
        assert!(k.a_priori);
        let faktor = konfidenzfaktor(None);
        assert!((k.sigma_r_mm - 1.0).abs() < 1e-9 && (k.sigma_h_mm - 2.0).abs() < 1e-9);
        assert!((k.a_mm - faktor * 2.0).abs() < 1e-9 && (k.b_mm - faktor).abs() < 1e-9);
        assert!((k.theta_rad - std::f64::consts::FRAC_PI_2).abs() < 1e-9);
        // Singuläre Normalmatrix: keine Ellipse
        assert!(konfidenz_ellipse(&[[0.0; 3]; 3], 2.0, 0.0, 0).is_none());
    }

    /// Wahre Station für die Testdaten.
    const STAND_R: f64 = 1000.0;
    const STAND_H: f64 = 2000.0;
    const STAND_HOEHE: f64 = 100.0;
    const ORIENTIERUNG_GON: f64 = 37.5;

    /// Erzeugt fehlerfreie Messungen zu bekannten Festpunkten.
    fn messung(nr: &str, r: f64, h: f64, hoehe: f64) -> StationierungsPunkt {
        let d_r = r - STAND_R;
        let d_h = h - STAND_H;
        let dist_h = d_r.hypot(d_h);
        let t_gon = d_r.atan2(d_h) * RAD2GON;
        let hz = ((t_gon - ORIENTIERUNG_GON) % 400.0 + 400.0) % 400.0;
        let d_hoehe = hoehe - STAND_HOEHE;
        let schraeg = dist_h.hypot(d_hoehe);
        let v = dist_h.atan2(d_hoehe) * RAD2GON;
        StationierungsPunkt { punkt_nr: nr.to_string(), r, h, hoehe, hz, v, strecke: schraeg, zielhoehe: 0.0 }
    }

    fn testpunkte() -> Vec<StationierungsPunkt> {
        vec![messung("A", 1100.0, 2150.0, 101.0), messung("B", 920.0, 2210.0, 99.5), messung("C", 1060.0, 1890.0, 100.4), messung("D", 880.0, 1940.0, 100.9)]
    }

    #[test]
    fn fehlerfreie_daten_ergeben_exakte_station() {
        let e = berechnen(&testpunkte(), 0.0, &Optionen::default()).unwrap();
        assert!(e.konvergiert);
        assert!((e.r - STAND_R).abs() < 1e-6, "R = {}", e.r);
        assert!((e.h - STAND_H).abs() < 1e-6, "H = {}", e.h);
        assert!((e.orientierung_gon - ORIENTIERUNG_GON).abs() < 1e-6);
        assert!((e.massstab - 1.0).abs() < 1e-9);
        assert!((e.hoehe - STAND_HOEHE).abs() < 1e-6);
        assert!(e.s0_mm < 1e-3);
        assert_eq!(e.redundanz, 8 - 3);
        assert!(e.berechnung_3d);
    }

    #[test]
    fn zwei_punkte_nutzen_die_direktloesung() {
        let punkte = testpunkte()[..2].to_vec();
        let e = berechnen(&punkte, 0.0, &Optionen::default()).unwrap();
        assert_eq!(e.redundanz, 0);
        assert_eq!(e.iterationen, 0);
        assert!((e.r - STAND_R).abs() < 1e-6);
        assert!((e.h - STAND_H).abs() < 1e-6);
        assert!(e.residuen[0].gewicht_richtung.is_nan());
    }

    #[test]
    fn zwei_punkte_fester_massstab_beachten_ausgeschaltete_strecken() {
        let mut punkte = testpunkte()[..2].to_vec();
        let optionen = Optionen { freier_massstab: false, aktiv_strecke: Some(vec![true, false]), berechnung_3d: false, ..Optionen::default() };
        let ohne = berechnen(&punkte, 0.0, &optionen).unwrap();
        assert!(ohne.konvergiert);
        assert_eq!(ohne.redundanz, 0);
        assert!((ohne.r - STAND_R).abs() < 1e-5 && (ohne.h - STAND_H).abs() < 1e-5, "R = {}, H = {}", ohne.r, ohne.h);

        // Eine um 10 m geänderte, ausgeschaltete Strecke darf die Standpunktlage nicht verschieben
        punkte[1].strecke += 10.0;
        let mit = berechnen(&punkte, 0.0, &optionen).unwrap();
        assert!(mit.konvergiert);
        assert!((mit.r - ohne.r).abs() < 1e-6 && (mit.h - ohne.h).abs() < 1e-6, "R {} → {}, H {} → {}", ohne.r, mit.r, ohne.h, mit.h);
        assert!((mit.orientierung_gon - ohne.orientierung_gon).abs() < 1e-6);

        // Auch eine ausgeschaltete Richtung wirkt nicht mehr (3 aktive Beobachtungen genügen bei festem Maßstab)
        let optionen = Optionen { aktiv_richtung: Some(vec![true, false]), aktiv_strecke: Some(vec![true, true]), ..optionen };
        let punkte_richtung = testpunkte()[..2].to_vec();
        let basis = berechnen(&punkte_richtung, 0.0, &optionen).unwrap();
        let mut verstellt = punkte_richtung;
        verstellt[1].hz += 5.0;
        let nach = berechnen(&verstellt, 0.0, &optionen).unwrap();
        assert!((nach.r - basis.r).abs() < 1e-6 && (nach.h - basis.h).abs() < 1e-6);
    }

    #[test]
    fn dreitausend_messzeilen_mit_40_anschlusspunkten() {
        // 40 verschiedene Punkte auf einem Ring, jeder 75-mal gemessen = 3000 Messzeilen
        let ring: Vec<StationierungsPunkt> = (0..40)
            .map(|i| {
                let winkel = f64::from(i) * std::f64::consts::TAU / 40.0;
                messung(&format!("P{i}"), STAND_R + 150.0 * winkel.cos(), STAND_H + 150.0 * winkel.sin(), 100.0 + f64::from(i % 5))
            })
            .collect();
        let punkte: Vec<StationierungsPunkt> = (0..3000).map(|i| ring[i % 40].clone()).collect();
        let start = std::time::Instant::now();
        let e = berechnen(&punkte, 0.0, &Optionen::default()).unwrap();
        assert!(start.elapsed().as_secs_f64() < 5.0, "Rechenzeit {:?}", start.elapsed());
        assert!(e.konvergiert);
        assert!((e.r - STAND_R).abs() < 1e-6 && (e.h - STAND_H).abs() < 1e-6);
        assert_eq!(e.residuen.len(), 3000);
        assert_eq!(e.redundanz, 6000 - 3);
        assert!(e.konfidenz.is_some());
    }

    #[test]
    fn zwei_punkte_freier_massstab_brauchen_alle_vier_beobachtungen() {
        let punkte = testpunkte()[..2].to_vec();
        let optionen = Optionen { aktiv_strecke: Some(vec![true, false]), ..Optionen::default() };
        let fehler = berechnen(&punkte, 0.0, &optionen).unwrap_err();
        assert!(fehler.contains("mindestens 4"), "{fehler}");
    }

    #[test]
    fn grober_fehler_wird_im_residuum_sichtbar() {
        let mut punkte = testpunkte();
        punkte[2].strecke += 0.050;
        let e = berechnen(&punkte, 0.0, &Optionen::default()).unwrap();
        assert!(e.s0_mm > 1.0);
        let groesstes = e.residuen.iter().map(|r| r.v_strecke_mm.abs()).fold(0.0_f64, f64::max);
        assert!(groesstes > 5.0);
    }

    #[test]
    fn hoehenfehler_faellt_auf_2d_zurueck() {
        let mut punkte = testpunkte();
        punkte[0].hoehe += 0.5;
        let e = berechnen(&punkte, 0.0, &Optionen::default()).unwrap();
        assert!(!e.berechnung_3d);
        assert_eq!(e.hoehe, 0.0);
        assert!(e.warnung_hoehe.contains("Höhenabweichung"));
    }

    #[test]
    fn zu_wenige_beobachtungen_werden_abgelehnt() {
        assert!(berechnen(&testpunkte()[..1], 0.0, &Optionen::default()).is_err());
        let opt = Optionen { aktiv_richtung: Some(vec![false; 4]), ..Optionen::default() };
        assert!(berechnen(&testpunkte(), 0.0, &opt).is_err());
        let opt = Optionen { aktiv_richtung: Some(vec![true, false, false, false]), aktiv_strecke: Some(vec![true, false, false, false]), ..Optionen::default() };
        assert!(berechnen(&testpunkte(), 0.0, &opt).is_err());
    }

    #[test]
    fn fester_massstab_ergibt_massstab_eins() {
        let opt = Optionen { freier_massstab: false, ..Optionen::default() };
        let e = berechnen(&testpunkte(), 0.0, &opt).unwrap();
        assert_eq!(e.massstab, 1.0);
        assert!((e.r - STAND_R).abs() < 1e-6);
    }

    #[test]
    fn kollineare_punkte_werden_erkannt() {
        let punkte = vec![messung("A", 1000.0, 2100.0, 100.0), messung("B", 1000.0, 2100.0, 100.0)];
        assert!(berechnen(&punkte, 0.0, &Optionen::default()).is_err());
    }
}
