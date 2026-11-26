//! Zweck:    GPS-Bahndaten (LNAV aus UBX-RXM-SFRBX), Satellitenpositionen und
//!           Einzelpunktlösung aus Codemessungen. Verwendet für die Messungen der
//!           SAPOS-Referenzstation, um deren Positionsfehler sichtbar zu machen.
//! Hardware: simpleRTK2B (u-blox ZED-F9P), SAPOS HEPS
//! Autor:    Johann Gerner
//!
//! Übernommen aus gnss-vermessung 0.4.0 (src/spp.rs), eigenständig gepflegt in Feldbuch-NT.
//!
//! Formeln nach IS-GPS-200 (Bahnberechnung, Satellitenuhr). Ohne Ionosphärenmodell,
//! Troposphäre nur grob: Die Lösung soll die Fehler zeigen, die SAPOS korrigiert.

use std::collections::BTreeMap;

pub const CLIGHT: f64 = 299_792_458.0;
/// Gravitationskonstante der Erde nach IS-GPS-200 [m³/s²]
const GM: f64 = 3.986_005e14;
/// Erdrotation [rad/s]
const OMEGA_E: f64 = 7.292_115_146_7e-5;
/// Kreiszahl in der Festlegung von IS-GPS-200 (Halbkreise in Radiant)
// bewusst der gerundete Wert aus IS-GPS-200, nicht std::f64::consts::PI
#[allow(clippy::approx_constant)]
const GPS_PI: f64 = 3.141_592_653_589_8;
const HALF_WEEK_S: f64 = 302_400.0;
const WEEK_S: f64 = 604_800.0;
const KEPLER_ITERATIONS: usize = 12;
const LSQ_ITERATIONS: usize = 10;
const CONVERGED_M: f64 = 1e-4;
pub const MIN_ELEVATION_DEG: f64 = 10.0;
/// Zenitverzögerung der Troposphäre (Näherung) [m]
const ZENITH_TROPO_M: f64 = 2.3;
/// Bahndaten gelten bis zu diesem Abstand von toe [s]
const MAX_EPH_AGE_S: f64 = 4.0 * 3600.0;
const SUBFRAME_BITS: usize = 240;
const DATA_BITS_PER_WORD: usize = 24;
const WORDS: usize = 10;
/// Zeichen für ungültige Gruppenlaufzeit
const TGD_INVALID: i64 = -128;

/// Broadcast-Bahndaten eines GPS-Satelliten (Winkel in Radiant, Zeiten in Sekunden der Woche).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Ephemeris {
    pub iode: u32,
    pub health: u32,
    pub toe: f64,
    pub toc: f64,
    pub sqrt_a: f64,
    pub e: f64,
    pub i0: f64,
    pub omega0: f64,
    pub omega: f64,
    pub m0: f64,
    pub delta_n: f64,
    pub omega_dot: f64,
    pub idot: f64,
    pub cuc: f64,
    pub cus: f64,
    pub crc: f64,
    pub crs: f64,
    pub cic: f64,
    pub cis: f64,
    pub af0: f64,
    pub af1: f64,
    pub af2: f64,
    pub tgd: f64,
}

/// Bitzugriff auf einen Unterrahmen aus 10 × 24 Datenbits (höchstwertiges Bit zuerst).
struct Bits<'a>(&'a [u8]);

impl Bits<'_> {
    fn u(&self, pos: usize, len: usize) -> u64 {
        (pos..pos + len).fold(0, |v, i| (v << 1) | u64::from((self.0[i / 8] >> (7 - i % 8)) & 1))
    }

    fn s(&self, pos: usize, len: usize) -> i64 {
        let raw = self.u(pos, len) as i64;
        (raw << (64 - len)) >> (64 - len)
    }
}

fn two_pow(exp: i32) -> f64 {
    2f64.powi(exp)
}

fn decode(sf1: &[u8], sf2: &[u8], sf3: &[u8]) -> Option<Ephemeris> {
    let (a, b, c) = (Bits(sf1), Bits(sf2), Bits(sf3));
    let iodc = (a.u(70, 2) << 8) | a.u(168, 8);
    let iode = b.u(48, 8);
    if iode != c.u(216, 8) || iode != (iodc & 0xFF) {
        return None;
    }
    let tgd = a.s(160, 8);
    Some(Ephemeris {
        iode: iode as u32,
        health: a.u(64, 6) as u32,
        tgd: if tgd == TGD_INVALID { 0.0 } else { tgd as f64 * two_pow(-31) },
        toc: a.u(176, 16) as f64 * 16.0,
        af2: a.s(192, 8) as f64 * two_pow(-55),
        af1: a.s(200, 16) as f64 * two_pow(-43),
        af0: a.s(216, 22) as f64 * two_pow(-31),
        crs: b.s(56, 16) as f64 * two_pow(-5),
        delta_n: b.s(72, 16) as f64 * two_pow(-43) * GPS_PI,
        m0: b.s(88, 32) as f64 * two_pow(-31) * GPS_PI,
        cuc: b.s(120, 16) as f64 * two_pow(-29),
        e: b.u(136, 32) as f64 * two_pow(-33),
        cus: b.s(168, 16) as f64 * two_pow(-29),
        sqrt_a: b.u(184, 32) as f64 * two_pow(-19),
        toe: b.u(216, 16) as f64 * 16.0,
        cic: c.s(48, 16) as f64 * two_pow(-29),
        omega0: c.s(64, 32) as f64 * two_pow(-31) * GPS_PI,
        cis: c.s(96, 16) as f64 * two_pow(-29),
        i0: c.s(112, 32) as f64 * two_pow(-31) * GPS_PI,
        crc: c.s(144, 16) as f64 * two_pow(-5),
        omega: c.s(160, 32) as f64 * two_pow(-31) * GPS_PI,
        omega_dot: c.s(192, 24) as f64 * two_pow(-43) * GPS_PI,
        idot: c.s(224, 14) as f64 * two_pow(-43) * GPS_PI,
    })
}

/// Bahndaten aller GPS-Satelliten, zusammengesetzt aus den Unterrahmen 1 bis 3.
#[derive(Default)]
pub struct Ephemerides {
    subframes: BTreeMap<u8, [Option<Vec<u8>>; 3]>,
    pub gps: BTreeMap<u8, Ephemeris>,
    pub subframes_received: u64,
}

impl Ephemerides {
    /// Nimmt einen GPS-L1C/A-Unterrahmen aus UBX-RXM-SFRBX auf (10 Worte, Datenbits 29..6).
    pub fn add_gps_words(&mut self, prn: u8, words: &[u32]) {
        if words.len() < WORDS {
            return;
        }
        let mut buf = vec![0u8; SUBFRAME_BITS / 8];
        for (i, w) in words.iter().take(WORDS).enumerate() {
            let data = (w >> 6) & 0x00FF_FFFF;
            for b in 0..DATA_BITS_PER_WORD {
                if (data >> (DATA_BITS_PER_WORD - 1 - b)) & 1 == 1 {
                    let pos = i * DATA_BITS_PER_WORD + b;
                    buf[pos / 8] |= 0x80 >> (pos % 8);
                }
            }
        }
        let id = Bits(&buf).u(43, 3) as usize;
        if !(1..=3).contains(&id) {
            return;
        }
        self.subframes_received += 1;
        let entry = self.subframes.entry(prn).or_default();
        entry[id - 1] = Some(buf);
        if let [Some(a), Some(b), Some(c)] = entry {
            if let Some(e) = decode(a, b, c) {
                self.gps.insert(prn, e);
            }
        }
    }
}

impl Ephemerides {
    /// Satelliten mit empfangenen Unterrahmen, aber noch ohne vollständige Ephemeride.
    pub fn pending(&self) -> Vec<u8> {
        self.subframes.keys().copied().filter(|p| !self.gps.contains_key(p)).collect()
    }
}

/// Zeitdifferenz mit Wochenwechsel.
fn wrap(dt: f64) -> f64 {
    if dt > HALF_WEEK_S {
        dt - WEEK_S
    } else if dt < -HALF_WEEK_S {
        dt + WEEK_S
    } else {
        dt
    }
}

/// Satellitenposition (ECEF zur Sendezeit) und Uhrkorrektur [s] zur GPS-Zeit `t`.
pub fn position(e: &Ephemeris, t: f64) -> ([f64; 3], f64) {
    let a = e.sqrt_a * e.sqrt_a;
    let tk = wrap(t - e.toe);
    let n = (GM / (a * a * a)).sqrt() + e.delta_n;
    let m = e.m0 + n * tk;
    let mut ecc = m;
    for _ in 0..KEPLER_ITERATIONS {
        ecc = m + e.e * ecc.sin();
    }
    let v = ((1.0 - e.e * e.e).sqrt() * ecc.sin()).atan2(ecc.cos() - e.e);
    let phi = v + e.omega;
    let (s2, c2) = (2.0 * phi).sin_cos();
    let u = phi + e.cus * s2 + e.cuc * c2;
    let r = a * (1.0 - e.e * ecc.cos()) + e.crs * s2 + e.crc * c2;
    let i = e.i0 + e.cis * s2 + e.cic * c2 + e.idot * tk;
    let (x, y) = (r * u.cos(), r * u.sin());
    let om = e.omega0 + (e.omega_dot - OMEGA_E) * tk - OMEGA_E * e.toe;
    let pos = [x * om.cos() - y * i.cos() * om.sin(), x * om.sin() + y * i.cos() * om.cos(), y * i.sin()];
    let tc = wrap(t - e.toc);
    let relativistic = -2.0 * GM.sqrt() * e.e * e.sqrt_a * ecc.sin() / (CLIGHT * CLIGHT);
    (pos, e.af0 + e.af1 * tc + e.af2 * tc * tc + relativistic - e.tgd)
}

#[derive(Debug, Clone, Copy)]
pub struct Observation {
    pub prn: u8,
    pub pseudorange: f64,
}

#[derive(Debug, Clone)]
pub struct Solution {
    pub ecef: [f64; 3],
    pub clock_m: f64,
    pub used: usize,
}

fn norm(v: [f64; 3]) -> f64 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}

/// Löst ein kleines Gleichungssystem (Gauß mit Spaltenpivot).
fn solve_linear(mut m: [[f64; 5]; 4]) -> Option<[f64; 4]> {
    for c in 0..4 {
        let p = (c..4).max_by(|&a, &b| m[a][c].abs().total_cmp(&m[b][c].abs()))?;
        if m[p][c].abs() < 1e-12 {
            return None;
        }
        m.swap(c, p);
        for r in 0..4 {
            if r != c {
                let f = m[r][c] / m[c][c];
                for j in c..5 {
                    m[r][j] -= f * m[c][j];
                }
            }
        }
    }
    Some([m[0][4] / m[0][0], m[1][4] / m[1][1], m[2][4] / m[2][2], m[3][4] / m[3][3]])
}

/// Einzelpunktlösung zur Empfangszeit `tow_s` (GPS-Sekunde der Woche) ausgehend von `approx`.
pub fn solve(obs: &[Observation], tow_s: f64, eph: &BTreeMap<u8, Ephemeris>, approx: [f64; 3]) -> Option<Solution> {
    // Satellitenposition zur Sendezeit
    let mut sats = Vec::new();
    for o in obs {
        let Some(e) = eph.get(&o.prn).filter(|e| e.health == 0 && wrap(tow_s - e.toe).abs() < MAX_EPH_AGE_S) else {
            continue;
        };
        let mut tx = tow_s - o.pseudorange / CLIGHT;
        let (_, dts) = position(e, tx);
        tx -= dts;
        let (pos, dts) = position(e, tx);
        sats.push((pos, dts, o.pseudorange));
    }
    let mut x = approx;
    let mut clock = 0.0;
    let mut used = 0;
    for _ in 0..LSQ_ITERATIONS {
        let up = {
            let n = norm(x);
            [x[0] / n, x[1] / n, x[2] / n]
        };
        let mut normal = [[0.0; 5]; 4];
        used = 0;
        for &(pos, dts, pr) in &sats {
            // Erddrehung während der Laufzeit
            let theta = OMEGA_E * pr / CLIGHT;
            let s = [
                theta.cos() * pos[0] + theta.sin() * pos[1],
                -theta.sin() * pos[0] + theta.cos() * pos[1],
                pos[2],
            ];
            let los = [s[0] - x[0], s[1] - x[1], s[2] - x[2]];
            let range = norm(los);
            let sin_el = (los[0] * up[0] + los[1] * up[1] + los[2] * up[2]) / range;
            if sin_el < MIN_ELEVATION_DEG.to_radians().sin() {
                continue;
            }
            let residual = pr - (range + clock - CLIGHT * dts + ZENITH_TROPO_M / sin_el);
            let row = [-los[0] / range, -los[1] / range, -los[2] / range, 1.0];
            for i in 0..4 {
                for j in 0..4 {
                    normal[i][j] += row[i] * row[j];
                }
                normal[i][4] += row[i] * residual;
            }
            used += 1;
        }
        if used < 4 {
            return None;
        }
        let d = solve_linear(normal)?;
        x = [x[0] + d[0], x[1] + d[1], x[2] + d[2]];
        clock += d[3];
        if norm([d[0], d[1], d[2]]) < CONVERGED_M {
            break;
        }
    }
    Some(Solution { ecef: x, clock_m: clock, used })
}

/// Abweichung `p − reference` im lokalen System (Ost, Nord, Hoch) am Bezugspunkt [m].
pub fn enu(reference: [f64; 3], p: [f64; 3]) -> [f64; 3] {
    let g = crate::gnss::kern::geo::ecef_to_geo(reference);
    let (lat, lon) = (g.lat.to_radians(), g.lon.to_radians());
    let d = [p[0] - reference[0], p[1] - reference[1], p[2] - reference[2]];
    [
        -lon.sin() * d[0] + lon.cos() * d[1],
        -lat.sin() * lon.cos() * d[0] - lat.sin() * lon.sin() * d[1] + lat.cos() * d[2],
        lat.cos() * lon.cos() * d[0] + lat.cos() * lon.sin() * d[1] + lat.sin() * d[2],
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gnss::kern::geo::{Geographic, geo_to_ecef};

    /// Schreibt Werte in einen 24-Bit-gepackten Unterrahmen und erzeugt die SFRBX-Worte.
    fn words(fields: &[(usize, usize, i64)]) -> Vec<u32> {
        let mut bits = vec![0u8; SUBFRAME_BITS];
        for &(pos, len, value) in fields {
            for i in 0..len {
                bits[pos + i] = ((value >> (len - 1 - i)) & 1) as u8;
            }
        }
        (0..WORDS)
            .map(|w| {
                let data = (0..DATA_BITS_PER_WORD).fold(0u32, |v, b| (v << 1) | u32::from(bits[w * DATA_BITS_PER_WORD + b]));
                data << 6
            })
            .collect()
    }

    fn q(v: f64, exp: i32) -> i64 {
        (v / two_pow(exp)).round() as i64
    }

    /// Bahndaten nahe realer GPS-Werte, ein Satellit je Bahnebene und Anomalie.
    fn sample(prn: u8) -> Ephemeris {
        let k = f64::from(prn);
        Ephemeris {
            iode: 40 + u32::from(prn),
            health: 0,
            toe: 388_800.0,
            toc: 388_800.0,
            sqrt_a: 5153.65,
            e: 0.006 + 0.0007 * k,
            i0: 0.96 + 0.002 * k,
            omega0: -2.9 + 1.05 * (k % 6.0),
            omega: 0.6 - 0.3 * k,
            m0: -2.8 + 0.71 * k,
            delta_n: 4.5e-9,
            omega_dot: -8.0e-9,
            idot: 3.0e-10,
            cuc: 1.2e-6,
            cus: 8.0e-6,
            crc: 210.0,
            crs: 25.0,
            cic: -5.0e-8,
            cis: 9.0e-8,
            af0: 1.0e-4 * k,
            af1: -2.0e-12,
            af2: 0.0,
            tgd: -5.1e-9,
        }
    }

    /// Kodiert Bahndaten in die drei Unterrahmen (Kehrwert von `decode`).
    fn encode(e: &Ephemeris) -> [Vec<u32>; 3] {
        let iodc = i64::from(e.iode);
        let sf = |id: i64, mut f: Vec<(usize, usize, i64)>| {
            f.push((43, 3, id));
            words(&f)
        };
        [
            sf(1, vec![(64, 6, 0), (70, 2, iodc >> 8), (160, 8, q(e.tgd, -31)), (168, 8, iodc & 0xFF), (176, 16, (e.toc / 16.0) as i64),
                (192, 8, q(e.af2, -55)), (200, 16, q(e.af1, -43)), (216, 22, q(e.af0, -31))]),
            sf(2, vec![(48, 8, i64::from(e.iode)), (56, 16, q(e.crs, -5)), (72, 16, q(e.delta_n / GPS_PI, -43)), (88, 32, q(e.m0 / GPS_PI, -31)),
                (120, 16, q(e.cuc, -29)), (136, 32, q(e.e, -33)), (168, 16, q(e.cus, -29)), (184, 32, q(e.sqrt_a, -19)), (216, 16, (e.toe / 16.0) as i64)]),
            sf(3, vec![(48, 16, q(e.cic, -29)), (64, 32, q(e.omega0 / GPS_PI, -31)), (96, 16, q(e.cis, -29)), (112, 32, q(e.i0 / GPS_PI, -31)),
                (144, 16, q(e.crc, -5)), (160, 32, q(e.omega / GPS_PI, -31)), (192, 24, q(e.omega_dot / GPS_PI, -43)), (216, 8, i64::from(e.iode)),
                (224, 14, q(e.idot / GPS_PI, -43))]),
        ]
    }

    #[test]
    fn subframes_roundtrip_and_orbit_radius() {
        let mut store = Ephemerides::default();
        let e = sample(5);
        for w in encode(&e) {
            store.add_gps_words(5, &w);
        }
        let d = &store.gps[&5];
        assert_eq!(d.iode, e.iode);
        assert!((d.sqrt_a - e.sqrt_a).abs() < 1e-5 && (d.e - e.e).abs() < 1e-9);
        assert!((d.m0 - e.m0).abs() < 1e-8 && (d.omega0 - e.omega0).abs() < 1e-8);
        assert!((d.tgd - e.tgd).abs() < 1e-9 && (d.toe - e.toe).abs() < 1e-9);
        let (p, _) = position(d, e.toe + 100.0);
        let r = norm(p);
        assert!((r - 26_560e3).abs() < 300e3, "Bahnradius {r}");
    }

    #[test]
    fn position_solution_recovers_station() {
        let truth = geo_to_ecef(&Geographic { lat: 48.83, lon: 12.95, h: 370.0 });
        let clock_m = 1234.5;
        let tow = 390_000.0;
        let eph: BTreeMap<u8, Ephemeris> = (1..=24).map(|p| (p, sample(p))).collect();
        // Codemessungen erzeugen: Lichtlaufzeit iterativ, gleiche Modelle wie der Löser
        let up = [truth[0] / norm(truth), truth[1] / norm(truth), truth[2] / norm(truth)];
        let mut obs = Vec::new();
        for (&prn, e) in &eph {
            let mut pr = 20_200e3;
            for _ in 0..5 {
                let mut tx = tow - pr / CLIGHT;
                let (_, dts) = position(e, tx);
                tx -= dts;
                let (pos, dts) = position(e, tx);
                let theta = OMEGA_E * pr / CLIGHT;
                let s = [theta.cos() * pos[0] + theta.sin() * pos[1], -theta.sin() * pos[0] + theta.cos() * pos[1], pos[2]];
                let los = [s[0] - truth[0], s[1] - truth[1], s[2] - truth[2]];
                let range = norm(los);
                let sin_el = (los[0] * up[0] + los[1] * up[1] + los[2] * up[2]) / range;
                if sin_el < MIN_ELEVATION_DEG.to_radians().sin() {
                    pr = f64::NAN;
                    break;
                }
                pr = range + clock_m - CLIGHT * dts + ZENITH_TROPO_M / sin_el;
            }
            if pr.is_finite() {
                obs.push(Observation { prn, pseudorange: pr });
            }
        }
        assert!(obs.len() >= 5, "nur {} Satelliten sichtbar", obs.len());
        let start = [truth[0] + 3000.0, truth[1] - 2000.0, truth[2] + 1000.0];
        let s = solve(&obs, tow, &eph, start).unwrap();
        let d = enu(truth, s.ecef);
        assert!(norm(d) < 0.01, "Abweichung {d:?}");
        assert!((s.clock_m - clock_m).abs() < 0.01);
    }
}
