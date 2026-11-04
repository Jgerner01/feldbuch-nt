// Zweck:    Rechenkern der Absteckung: polare Absteckdaten, Achspunkte, Raster, Schnurgerüst, Profilpunkte
// Hardware: keine
// Autor:    Johann Gerner

use crate::daten::projektdaten::ProjektdatenManager;
use crate::grundlagen::format::{f, parse_zahl};

/// Grad-Umrechnung: Gon → Bogenmaß.
const GON_ZU_RAD: f64 = std::f64::consts::PI / 200.0;
/// Bogenmaß → Gon.
const RAD_ZU_GON: f64 = 200.0 / std::f64::consts::PI;
/// Vollkreis [gon].
const VOLLKREIS: f64 = 400.0;
/// Kürzeste zulässige Achslänge [m].
const MIN_ACHSLAENGE: f64 = 1e-6;
/// Kleinstes zulässiges Stationsintervall [m].
const MIN_INTERVALL: f64 = 0.01;
/// Toleranz am Achsende [m].
const ACHS_TOLERANZ: f64 = 1e-6;
/// Kleinster Kosinus des halben Eckwinkels der Gehrung (entspricht 80°).
const MIN_COS_GEHRUNG: f64 = 0.17;
/// Status eines noch nicht abgesteckten Punkts.
pub const STATUS_OFFEN: &str = "offen";
/// Status eines abgesteckten Punkts.
pub const STATUS_ABGESTECKT: &str = "abgesteckt";

/// Standpunkt der Absteckung.
#[derive(Clone, Debug, PartialEq)]
pub struct StandpunktInfo {
    pub punkt_nr: String,
    pub r: f64,
    pub h: f64,
    pub orientierung_gon: f64,
}

/// Absteckpunkt mit Sollwerten.
#[derive(Clone, Debug, PartialEq)]
pub struct AbsteckPunkt {
    pub punkt_nr: String,
    /// Station, Rasterbezeichnung usw.
    pub label: String,
    pub r_soll: f64,
    pub h_soll: f64,
    pub hz_soll_gon: f64,
    pub s_soll_m: f64,
    /// Stationsmaß entlang der Achse.
    pub abszisse_m: Option<f64>,
    /// Ordinate (Abstand zur Achse, links positiv).
    pub ordinate_m: Option<f64>,
    pub status: String,
}

impl AbsteckPunkt {
    pub fn neu(punkt_nr: &str, r_soll: f64, h_soll: f64) -> Self {
        Self {
            punkt_nr: punkt_nr.to_string(),
            label: String::new(),
            r_soll,
            h_soll,
            hz_soll_gon: 0.0,
            s_soll_m: 0.0,
            abszisse_m: None,
            ordinate_m: None,
            status: STATUS_OFFEN.to_string(),
        }
    }

    pub fn ist_abgesteckt(&self) -> bool {
        self.status == STATUS_ABGESTECKT
    }
}

/// Profilpunkt der Profilabsteckung.
#[derive(Clone, Debug, PartialEq)]
pub struct ProfilAbsteckPunkt {
    pub punkt_nr: String,
    pub station_m: f64,
    pub r: f64,
    pub h: f64,
    pub h_plan: f64,
    pub h_gelaende: Option<f64>,
    /// Plan − Gelände (+ = Auftrag, − = Aushub).
    pub delta_h_m: Option<f64>,
    pub boesch_links_m: Option<f64>,
    pub boesch_rechts_m: Option<f64>,
    pub hz_soll_gon: f64,
    pub s_soll_m: f64,
}

/// Normiert einen Winkel auf 0 … 400 gon.
pub fn norm400(gon: f64) -> f64 {
    ((gon % VOLLKREIS) + VOLLKREIS) % VOLLKREIS
}

/// Lädt den Standpunkt aus den Projektdaten: zuerst Rückwärtsschnitt, dann Freie Stationierung.
pub fn lade_standpunkt(projektdaten: &ProjektdatenManager) -> Option<StandpunktInfo> {
    for kategorie in ["Rückwärtschnitt", "Freie Stationierung"] {
        let zahl = |parameter: &str| projektdaten.get_value(kategorie, parameter).and_then(parse_zahl);
        if let (Some(r), Some(h), Some(z)) = (zahl("R [m]"), zahl("H [m]"), zahl("Orientierung [gon]")) {
            let punkt_nr = projektdaten.get_value(kategorie, "Standpunkt").unwrap_or("?").to_string();
            return Some(StandpunktInfo { punkt_nr, r, h, orientierung_gon: z });
        }
    }
    None
}

/// Richtung und Strecke vom Standpunkt zu einem Sollpunkt: (Hz [gon], s [m]).
pub fn polar_zu(st: &StandpunktInfo, r: f64, h: f64) -> (f64, f64) {
    let (dr, dh) = (r - st.r, h - st.h);
    let t_gon = dr.atan2(dh) * RAD_ZU_GON;
    (norm400(t_gon - st.orientierung_gon), dr.hypot(dh))
}

/// Polare Absteckung: Hz und Strecke vom Standpunkt zum Sollpunkt.
pub fn berechne_polare_absteckung(st: &StandpunktInfo, p: &mut AbsteckPunkt) {
    let (hz, s) = polar_zu(st, p.r_soll, p.h_soll);
    p.hz_soll_gon = hz;
    p.s_soll_m = s;
}

/// Orthogonalmaß: (Abszisse, Ordinate) des Punkts P bezogen auf die Achse A→E.
pub fn berechne_orthogonal(a: (f64, f64), e: (f64, f64), p: (f64, f64)) -> (f64, f64) {
    let (dr, dh) = (e.0 - a.0, e.1 - a.1);
    let l = dr.hypot(dh);
    if l < 1e-9 {
        return (0.0, 0.0);
    }
    let (ux, uy) = (dr / l, dh / l);
    let abszisse = (p.0 - a.0) * ux + (p.1 - a.1) * uy;
    let ordinate = -(p.0 - a.0) * uy + (p.1 - a.1) * ux;
    (abszisse, ordinate)
}

/// Abweichung Ist → Soll in Längs- und Querrichtung zur Sollrichtung [mm].
pub fn berechne_abweichung(soll: (f64, f64), ist: (f64, f64), t_soll_rad: f64) -> (f64, f64) {
    let (dr, dh) = (ist.0 - soll.0, ist.1 - soll.1);
    let ds = dr * t_soll_rad.sin() + dh * t_soll_rad.cos();
    let dq = -dr * t_soll_rad.cos() + dh * t_soll_rad.sin();
    (ds * 1000.0, dq * 1000.0)
}

/// Einweisung: Differenz aus Soll und Ist, (ΔHz [gon] im Bereich −200 … +200, Δs [mm]).
/// ΔHz > 0: das Instrument ist nach rechts zu drehen; Δs > 0: der Reflektor ist vorwärts (vom Instrument weg) zu bewegen.
pub fn berechne_einweisung(hz_soll: f64, s_soll: f64, hz_ist: f64, s_ist: f64) -> (f64, f64) {
    let mut dhz = norm400(hz_soll - hz_ist);
    if dhz > VOLLKREIS / 2.0 {
        dhz -= VOLLKREIS;
    }
    (dhz, (s_soll - s_ist) * 1000.0)
}

/// Station als Text („0+012.5"): mindestens drei Vorkommastellen, eine Nachkommastelle.
fn station_text(s: f64) -> String {
    format!("0+{:0>5}", f(s, 1))
}

/// Versatz mit Vorzeichen und zwei Nachkommastellen („+1.50", „-0.50").
fn versatz_text(offset: f64) -> String {
    if offset < 0.0 { f(offset, 2) } else { format!("+{}", f(offset, 2)) }
}

/// Achspunkte entlang der Achse A→E im Abstand `intervall`, je Versatz eine Reihe (links positiv).
pub fn berechne_achspunkte(
    a: (f64, f64),
    e: (f64, f64),
    intervall: f64,
    offsets: &[f64],
    st: Option<&StandpunktInfo>,
) -> Vec<AbsteckPunkt> {
    let mut ergebnis = Vec::new();
    let (dr, dh) = (e.0 - a.0, e.1 - a.1);
    let l = dr.hypot(dh);
    if l < MIN_ACHSLAENGE || intervall < MIN_INTERVALL {
        return ergebnis;
    }
    let (ux, uy) = (dr / l, dh / l);
    let (nx, ny) = (-uy, ux);
    let standard = [0.0];
    let versaetze: &[f64] = if offsets.is_empty() { &standard } else { offsets };
    let mut index = 0;
    for &offset in versaetze {
        let mut station = 0.0;
        while station <= l + ACHS_TOLERANZ {
            let s = station.min(l);
            index += 1;
            let mut label = station_text(s);
            if offset != 0.0 {
                label.push(' ');
                label.push_str(&versatz_text(offset));
                label.push('m');
            }
            let mut p = AbsteckPunkt::neu(&format!("A{index:03}"), a.0 + s * ux + offset * nx, a.1 + s * uy + offset * ny);
            p.label = label;
            p.abszisse_m = Some(s);
            p.ordinate_m = Some(offset);
            if let Some(st) = st {
                berechne_polare_absteckung(st, &mut p);
            }
            ergebnis.push(p);
            if s >= l {
                break;
            }
            station += intervall;
        }
    }
    ergebnis
}

/// Schnurgerüst: Polygon um `abstand` nach außen versetzen (Gehrung, höchstens 80°).
pub fn berechne_schnurgeruest(polygon: &[(f64, f64)], abstand: f64, st: Option<&StandpunktInfo>) -> Vec<AbsteckPunkt> {
    let n = polygon.len();
    if n < 3 {
        return Vec::new();
    }
    // Umlaufsinn bestimmen
    let flaeche2: f64 = (0..n)
        .map(|i| {
            let (r1, h1) = polygon[i];
            let (r2, h2) = polygon[(i + 1) % n];
            (r2 - r1) * (h2 + h1)
        })
        .sum();
    let vorzeichen = if flaeche2 > 0.0 { -1.0 } else { 1.0 };
    let normalen: Vec<(f64, f64)> = (0..n)
        .map(|i| {
            let (r1, h1) = polygon[i];
            let (r2, h2) = polygon[(i + 1) % n];
            let (ex, ey) = (r2 - r1, h2 - h1);
            let l = ex.hypot(ey);
            if l < 1e-9 { (0.0, 0.0) } else { (vorzeichen * ey / l, vorzeichen * (-ex) / l) }
        })
        .collect();
    (0..n)
        .map(|i| {
            let (nx1, ny1) = normalen[(i + n - 1) % n];
            let (nx2, ny2) = normalen[i];
            let (mut bx, mut by) = (nx1 + nx2, ny1 + ny2);
            let mut b_laenge = bx.hypot(by);
            if b_laenge < 1e-9 {
                (bx, by, b_laenge) = (nx2, ny2, 1.0);
            }
            bx /= b_laenge;
            by /= b_laenge;
            let cos_halb = nx1 * bx + ny1 * by;
            let gehrung = abstand / cos_halb.abs().max(MIN_COS_GEHRUNG);
            let mut p = AbsteckPunkt::neu(&format!("SG{:02}", i + 1), polygon[i].0 + gehrung * bx, polygon[i].1 + gehrung * by);
            p.label = format!("Ecke {}", i + 1);
            if let Some(st) = st {
                berechne_polare_absteckung(st, &mut p);
            }
            p
        })
        .collect()
}

/// Parallele zu einer offenen Strecke A→E im Abstand `abstand` (positiv nach rechts von A→E gesehen).
pub fn berechne_versatzlinie(a: (f64, f64), e: (f64, f64), abstand: f64) -> Option<[(f64, f64); 2]> {
    let (dr, dh) = (e.0 - a.0, e.1 - a.1);
    let l = dr.hypot(dh);
    if l < 1e-9 {
        return None;
    }
    let (nr, nh) = (dh / l, -dr / l);
    Some([(a.0 + nr * abstand, a.1 + nh * abstand), (e.0 + nr * abstand, e.1 + nh * abstand)])
}

/// Senkrechter Abstand eines Punkts zur Geraden A→B; positiv auf der Außenseite eines im Uhrzeigersinn umlaufenen Polygons
/// (das ist die linke Seite von A→B). Das Original hatte das Vorzeichen entgegen seinem Kommentar vertauscht.
pub fn abstand_aussen(p: (f64, f64), a: (f64, f64), b: (f64, f64)) -> f64 {
    let (dr, dh) = (b.0 - a.0, b.1 - a.1);
    let l = dr.hypot(dh);
    if l < 1e-10 {
        return 0.0;
    }
    (dr * (p.1 - a.1) - dh * (p.0 - a.0)) / l
}

/// Rasterpunkte ab (R0, H0) in Richtung `richtung_gon`: Zeilen A, B, … in Hauptrichtung, Spalten nach rechts.
pub fn berechne_raster(
    r0: f64,
    h0: f64,
    richtung_gon: f64,
    d_s: f64,
    d_q: f64,
    zeilen: usize,
    spalten: usize,
    st: Option<&StandpunktInfo>,
) -> Vec<AbsteckPunkt> {
    let phi = richtung_gon * GON_ZU_RAD;
    let (ux, uy) = (phi.sin(), phi.cos());
    let (vx, vy) = (phi.cos(), -phi.sin());
    let mut ergebnis = Vec::with_capacity(zeilen * spalten);
    for zeile in 0..zeilen {
        for spalte in 0..spalten {
            let buchstabe = char::from(b'A' + u8::try_from(zeile).unwrap_or(25).min(25));
            let name = format!("{buchstabe}{}", spalte + 1);
            let (z, s) = (zeile as f64, spalte as f64);
            let mut p = AbsteckPunkt::neu(&name, r0 + z * d_s * ux + s * d_q * vx, h0 + z * d_s * uy + s * d_q * vy);
            p.label = name;
            if let Some(st) = st {
                berechne_polare_absteckung(st, &mut p);
            }
            ergebnis.push(p);
        }
    }
    ergebnis
}

/// Rundet wie .NET `Math.Round` (zur nächsten geraden Zahl bei exakter Mitte).
fn runde_gerade(x: f64) -> f64 {
    let r = x.round();
    if (x - x.trunc()).abs() == 0.5 && r % 2.0 != 0.0 { r - x.signum() } else { r }
}

/// Profilpunkte entlang der Achse A→E mit den Plan-Höhen je Station.
pub fn berechne_profilpunkte(
    a: (f64, f64),
    e: (f64, f64),
    intervall: f64,
    h_plan: &[f64],
    st: Option<&StandpunktInfo>,
) -> Vec<ProfilAbsteckPunkt> {
    let (dr, dh) = (e.0 - a.0, e.1 - a.1);
    let l = dr.hypot(dh);
    if l < MIN_ACHSLAENGE || intervall <= 0.0 || !intervall.is_finite() {
        return Vec::new();
    }
    let (ux, uy) = (dr / l, dh / l);
    let stationen = runde_gerade(l / intervall) as usize + 1;
    let mut ergebnis = Vec::new();
    for i in 0..stationen {
        let station = (i as f64 * intervall).min(l);
        let (r, h) = (a.0 + station * ux, a.1 + station * uy);
        let (hz, s) = st.map_or((0.0, 0.0), |st| polar_zu(st, r, h));
        ergebnis.push(ProfilAbsteckPunkt {
            punkt_nr: format!("P{i:03}"),
            station_m: station,
            r,
            h,
            h_plan: h_plan.get(i).copied().unwrap_or(0.0),
            h_gelaende: None,
            delta_h_m: None,
            boesch_links_m: None,
            boesch_rechts_m: None,
            hz_soll_gon: hz,
            s_soll_m: s,
        });
        if station >= l {
            break;
        }
    }
    ergebnis
}

/// Trägt die Geländehöhe eines Profilpunkts ein und berechnet Auftrag/Aushub und Böschungsfuß.
pub fn aktualisiere_profilpunkt(p: &mut ProfilAbsteckPunkt, h_gelaende: f64, planum_halbbreite: f64, boeschung_neigung: f64) {
    let delta = p.h_plan - h_gelaende;
    let abstand = planum_halbbreite + delta.abs() * boeschung_neigung;
    p.h_gelaende = Some(h_gelaende);
    p.delta_h_m = Some(delta);
    p.boesch_links_m = Some(abstand);
    p.boesch_rechts_m = Some(abstand);
}

/// Randanteil der Lageplan-Abbildung (je Seite, bezogen auf die Spannweite).
const PLAN_RAND: f64 = 0.12;

/// Grundweite der Rasterlinien im Lageplan [m].
const PLAN_RASTER_WEITE: f64 = 10.0;
/// Höchstzahl der Rasterlinien je Richtung.
const PLAN_MAX_RASTERLINIEN: f64 = 60.0;

/// Abbildung von Koordinaten (R nach rechts, H nach oben) auf eine Zeichenfläche (x nach rechts, y nach unten).
#[derive(Clone, Copy, Debug)]
pub struct Abbildung {
    min_r: f64,
    max_r: f64,
    min_h: f64,
    max_h: f64,
    massstab: f64,
    off_x: f64,
    off_y: f64,
}

impl Abbildung {
    /// Passt alle Punkte mit 12 % Rand in eine Fläche `breite` × `hoehe` ein (kleinste Spannweite 1 m).
    pub fn neu(punkte: &[(f64, f64)], breite: f64, hoehe: f64) -> Option<Self> {
        if punkte.is_empty() || breite <= 0.0 || hoehe <= 0.0 {
            return None;
        }
        let (mut min_r, mut max_r) = punkte.iter().fold((f64::MAX, f64::MIN), |(lo, hi), p| (lo.min(p.0), hi.max(p.0)));
        let (mut min_h, mut max_h) = punkte.iter().fold((f64::MAX, f64::MIN), |(lo, hi), p| (lo.min(p.1), hi.max(p.1)));
        let span_r = (max_r - min_r).max(1.0);
        let span_h = (max_h - min_h).max(1.0);
        min_r -= span_r * PLAN_RAND;
        max_r += span_r * PLAN_RAND;
        min_h -= span_h * PLAN_RAND;
        max_h += span_h * PLAN_RAND;
        let (span_r, span_h) = (max_r - min_r, max_h - min_h);
        let massstab = (breite / span_r).min(hoehe / span_h);
        Some(Self {
            min_r,
            max_r,
            min_h,
            max_h,
            massstab,
            off_x: (breite - span_r * massstab) / 2.0,
            off_y: (hoehe - span_h * massstab) / 2.0,
        })
    }

    /// Zeichenfläche (x, y) zu Koordinaten (R, H).
    pub fn bild(&self, r: f64, h: f64) -> (f64, f64) {
        (self.off_x + (r - self.min_r) * self.massstab, self.off_y + (self.max_h - h) * self.massstab)
    }

    /// Koordinaten (R, H) zu Zeichenfläche (x, y).
    pub fn welt(&self, x: f64, y: f64) -> (f64, f64) {
        (self.min_r + (x - self.off_x) / self.massstab, self.max_h - (y - self.off_y) / self.massstab)
    }

    /// Rasterweite des Lageplans: 10 m, bei großer Ausdehnung 100 m, 1000 m usw. (höchstens 60 Linien je Richtung).
    pub fn raster_weite(&self) -> f64 {
        let mut weite = PLAN_RASTER_WEITE;
        while (self.max_r - self.min_r) / weite > PLAN_MAX_RASTERLINIEN || (self.max_h - self.min_h) / weite > PLAN_MAX_RASTERLINIEN {
            weite *= 10.0;
        }
        weite
    }

    /// Sichtbarer Bereich (min R, max R, min H, max H).
    pub fn grenzen(&self) -> (f64, f64, f64, f64) {
        (self.min_r, self.max_r, self.min_h, self.max_h)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn standpunkt() -> StandpunktInfo {
        StandpunktInfo { punkt_nr: "S1".into(), r: 100.0, h: 200.0, orientierung_gon: 0.0 }
    }

    #[test]
    fn norm400_wickelt_in_den_bereich() {
        assert_eq!(norm400(-100.0), 300.0);
        assert_eq!(norm400(450.0), 50.0);
        assert_eq!(norm400(0.0), 0.0);
    }

    #[test]
    fn polare_absteckung_nach_norden_und_osten() {
        let st = standpunkt();
        let mut nord = AbsteckPunkt::neu("N", 100.0, 210.0);
        berechne_polare_absteckung(&st, &mut nord);
        assert!(nord.hz_soll_gon.abs() < 1e-9);
        assert!((nord.s_soll_m - 10.0).abs() < 1e-9);
        let mut ost = AbsteckPunkt::neu("O", 105.0, 200.0);
        berechne_polare_absteckung(&st, &mut ost);
        assert!((ost.hz_soll_gon - 100.0).abs() < 1e-9);
        let mut west = AbsteckPunkt::neu("W", 95.0, 200.0);
        berechne_polare_absteckung(&st, &mut west);
        assert!((west.hz_soll_gon - 300.0).abs() < 1e-9);
    }

    #[test]
    fn orientierung_wird_abgezogen() {
        let st = StandpunktInfo { orientierung_gon: 50.0, ..standpunkt() };
        let mut p = AbsteckPunkt::neu("N", 100.0, 210.0);
        berechne_polare_absteckung(&st, &mut p);
        assert!((p.hz_soll_gon - 350.0).abs() < 1e-9);
    }

    #[test]
    fn orthogonalmass_und_abweichung() {
        // Achse nach Norden: Abszisse = Δh, Ordinate = Δr·(-0) … links (west) positiv
        let (ab, or) = berechne_orthogonal((0.0, 0.0), (0.0, 10.0), (-2.0, 4.0));
        assert!((ab - 4.0).abs() < 1e-9);
        assert!((or - 2.0).abs() < 1e-9);
        assert_eq!(berechne_orthogonal((1.0, 1.0), (1.0, 1.0), (5.0, 5.0)), (0.0, 0.0));
        // Sollrichtung Norden: Ist 2 cm nördlich, 1 cm östlich
        let (ds, dq) = berechne_abweichung((0.0, 0.0), (0.01, 0.02), 0.0);
        assert!((ds - 20.0).abs() < 1e-9);
        assert!((dq + 10.0).abs() < 1e-9);
    }

    #[test]
    fn achspunkte_mit_versatz_und_labels() {
        let st = standpunkt();
        let p = berechne_achspunkte((0.0, 0.0), (0.0, 25.0), 10.0, &[0.0, 1.5], Some(&st));
        // Stationen 0, 10, 20 je Versatz (wie im Original fehlt das Achsende, wenn es nicht auf dem Intervall liegt)
        assert_eq!(p.len(), 6);
        assert_eq!(p[0].punkt_nr, "A001");
        assert_eq!(p[0].label, "0+000.0");
        assert_eq!(p[1].label, "0+010.0");
        assert_eq!(p[3].label, "0+000.0 +1.50m");
        assert_eq!(p[5].punkt_nr, "A006");
        // linke Normale bei Achse nach Norden zeigt nach Westen
        assert!((p[3].r_soll + 1.5).abs() < 1e-9);
        assert_eq!(p[2].abszisse_m, Some(20.0));
        assert_eq!(p[3].ordinate_m, Some(1.5));
        assert!(p[0].s_soll_m > 0.0);
        // Liegt das Ende auf dem Intervall, ist es enthalten
        let q = berechne_achspunkte((0.0, 0.0), (0.0, 20.0), 10.0, &[], None);
        assert_eq!(q.last().unwrap().abszisse_m, Some(20.0));
    }

    #[test]
    fn achspunkte_negativer_versatz_und_standardversatz() {
        let p = berechne_achspunkte((0.0, 0.0), (10.0, 0.0), 5.0, &[-0.5], None);
        assert_eq!(p[0].label, "0+000.0 -0.50m");
        let p = berechne_achspunkte((0.0, 0.0), (10.0, 0.0), 5.0, &[], None);
        assert_eq!(p.len(), 3);
        assert_eq!(p[2].label, "0+010.0");
    }

    #[test]
    fn achspunkte_ungueltige_eingaben() {
        assert!(berechne_achspunkte((0.0, 0.0), (0.0, 0.0), 1.0, &[], None).is_empty());
        assert!(berechne_achspunkte((0.0, 0.0), (0.0, 10.0), 0.001, &[], None).is_empty());
    }

    #[test]
    fn schnurgeruest_quadrat_nach_aussen() {
        let quadrat = [(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)];
        let p = berechne_schnurgeruest(&quadrat, 2.0, None);
        assert_eq!(p.len(), 4);
        assert_eq!(p[0].punkt_nr, "SG01");
        assert_eq!(p[0].label, "Ecke 1");
        // Außenecken liegen auf der Diagonalen um 2·√2 versetzt, unabhängig vom Umlaufsinn
        let mitte = (5.0, 5.0);
        for (orig, neu) in quadrat.iter().zip(&p) {
            let vorher = (orig.0 - mitte.0).hypot(orig.1 - mitte.1);
            let nachher = (neu.r_soll - mitte.0).hypot(neu.h_soll - mitte.1);
            assert!((nachher - vorher - 2.0 * 2f64.sqrt()).abs() < 1e-9, "{nachher} {vorher}");
        }
        let umgekehrt: Vec<_> = quadrat.iter().rev().copied().collect();
        let q = berechne_schnurgeruest(&umgekehrt, 2.0, None);
        for (orig, neu) in umgekehrt.iter().zip(&q) {
            let vorher = (orig.0 - mitte.0).hypot(orig.1 - mitte.1);
            let nachher = (neu.r_soll - mitte.0).hypot(neu.h_soll - mitte.1);
            assert!(nachher > vorher);
        }
        assert!(berechne_schnurgeruest(&quadrat[..2], 1.0, None).is_empty());
    }

    #[test]
    fn raster_richtung_norden_und_osten() {
        let p = berechne_raster(0.0, 0.0, 0.0, 10.0, 5.0, 2, 3, None);
        assert_eq!(p.len(), 6);
        assert_eq!(p[0].punkt_nr, "A1");
        assert_eq!(p[2].punkt_nr, "A3");
        assert_eq!(p[3].punkt_nr, "B1");
        // Hauptrichtung nach Norden, Querrichtung nach Osten
        assert!((p[3].h_soll - 10.0).abs() < 1e-9 && p[3].r_soll.abs() < 1e-9);
        assert!((p[2].r_soll - 10.0).abs() < 1e-9 && p[2].h_soll.abs() < 1e-9);
        let q = berechne_raster(0.0, 0.0, 100.0, 10.0, 5.0, 2, 1, None);
        assert!((q[1].r_soll - 10.0).abs() < 1e-9 && q[1].h_soll.abs() < 1e-9);
    }

    #[test]
    fn profilpunkte_und_boeschung() {
        let st = standpunkt();
        let mut p = berechne_profilpunkte((0.0, 0.0), (0.0, 25.0), 10.0, &[100.0, 101.0], Some(&st));
        // round(2.5) = 2 (gerade) → 3 Stationen, die dritte liegt bei 20 m und das Ende (25 m) fehlt
        assert_eq!(p.len(), 3);
        assert_eq!(p[0].punkt_nr, "P000");
        assert_eq!(p[1].h_plan, 101.0);
        assert_eq!(p[2].h_plan, 0.0);
        let q = berechne_profilpunkte((0.0, 0.0), (0.0, 30.0), 10.0, &[], None);
        assert_eq!(q.len(), 4);
        assert_eq!(q[3].station_m, 30.0);
        aktualisiere_profilpunkt(&mut p[1], 100.5, 3.0, 1.5);
        assert_eq!(p[1].delta_h_m, Some(0.5));
        assert_eq!(p[1].boesch_links_m, Some(3.75));
        aktualisiere_profilpunkt(&mut p[1], 102.0, 3.0, 1.5);
        assert_eq!(p[1].delta_h_m, Some(-1.0));
        assert_eq!(p[1].boesch_rechts_m, Some(4.5));
        assert!(berechne_profilpunkte((0.0, 0.0), (0.0, 10.0), 0.0, &[], None).is_empty());
    }

    #[test]
    fn abbildung_passt_punkte_mit_rand_ein() {
        assert!(Abbildung::neu(&[], 100.0, 100.0).is_none());
        let a = Abbildung::neu(&[(0.0, 0.0), (10.0, 10.0)], 120.0, 120.0).unwrap();
        // 10 m + 2 × 12 % = 12,4 m auf 120 px
        let (x0, y0) = a.bild(0.0, 10.0);
        let (x1, y1) = a.bild(10.0, 0.0);
        assert!(x0 > 0.0 && y0 > 0.0 && x1 < 120.0 && y1 < 120.0);
        assert!((x1 - x0 - y1 + y0).abs() < 1e-9, "quadratisch");
        // H wächst nach oben
        assert!(a.bild(5.0, 9.0).1 < a.bild(5.0, 1.0).1);
        let (r, h) = a.welt(a.bild(3.0, 7.0).0, a.bild(3.0, 7.0).1);
        assert!((r - 3.0).abs() < 1e-9 && (h - 7.0).abs() < 1e-9);
        // Einzelpunkt: Spannweite 1 m
        assert!(Abbildung::neu(&[(5.0, 5.0)], 100.0, 100.0).is_some());
    }

    #[test]
    fn einweisung_richtung_und_strecke() {
        // Soll 100 gon, Ist 90 gon: 10 gon nach rechts; Soll 20 m, Ist 19,5 m: 500 mm vorwärts
        let (dhz, ds) = berechne_einweisung(100.0, 20.0, 90.0, 19.5);
        assert!((dhz - 10.0).abs() < 1e-9 && (ds - 500.0).abs() < 1e-9);
        // Über den Nullpunkt: Soll 5 gon, Ist 395 gon → +10 gon
        let (dhz, _) = berechne_einweisung(5.0, 1.0, 395.0, 1.0);
        assert!((dhz - 10.0).abs() < 1e-9);
        // Soll 395, Ist 5 → −10 gon (nach links)
        let (dhz, ds) = berechne_einweisung(395.0, 1.0, 5.0, 1.002);
        assert!((dhz + 10.0).abs() < 1e-9 && (ds + 2.0).abs() < 1e-9);
    }

    #[test]
    fn rasterweite_waechst_mit_der_ausdehnung() {
        let klein = Abbildung::neu(&[(0.0, 0.0), (100.0, 100.0)], 100.0, 100.0).unwrap();
        assert_eq!(klein.raster_weite(), 10.0);
        let mittel = Abbildung::neu(&[(0.0, 0.0), (3000.0, 100.0)], 100.0, 100.0).unwrap();
        assert_eq!(mittel.raster_weite(), 100.0);
        let gross = Abbildung::neu(&[(0.0, 0.0), (5000.0, 100.0)], 100.0, 100.0).unwrap();
        assert_eq!(gross.raster_weite(), 1000.0);
    }

    #[test]
    fn versatzlinie_und_aussenabstand() {
        // Strecke nach Osten: rechts davon (Süden) liegt bei positivem Abstand
        let l = berechne_versatzlinie((0.0, 0.0), (10.0, 0.0), 2.0).unwrap();
        assert!((l[0].0).abs() < 1e-9 && (l[0].1 + 2.0).abs() < 1e-9);
        assert!((l[1].0 - 10.0).abs() < 1e-9 && (l[1].1 + 2.0).abs() < 1e-9);
        assert!(berechne_versatzlinie((1.0, 1.0), (1.0, 1.0), 2.0).is_none());
        // Bei Umlauf im Uhrzeigersinn liegt die Außenseite links: ein Punkt 3 m nördlich einer Strecke nach Osten ist 3 m „außen"
        assert!((abstand_aussen((5.0, 3.0), (0.0, 0.0), (10.0, 0.0)) - 3.0).abs() < 1e-9);
        assert!((abstand_aussen((5.0, -3.0), (0.0, 0.0), (10.0, 0.0)) + 3.0).abs() < 1e-9);
        assert_eq!(abstand_aussen((1.0, 1.0), (0.0, 0.0), (0.0, 0.0)), 0.0);
    }

    #[test]
    fn rundung_wie_dotnet() {
        assert_eq!(runde_gerade(2.5), 2.0);
        assert_eq!(runde_gerade(3.5), 4.0);
        assert_eq!(runde_gerade(2.4), 2.0);
        assert_eq!(runde_gerade(2.6), 3.0);
    }

    #[test]
    fn standpunkt_aus_projektdaten() {
        let dir = std::env::temp_dir().join(format!("fb-abst-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut pd = ProjektdatenManager::initialisiere(&dir.join("Projektdaten.csv"));
        assert!(lade_standpunkt(&pd).is_none());
        pd.set_value("Freie Stationierung", "Standpunkt", "S7").unwrap();
        pd.set_value("Freie Stationierung", "R [m]", "100.5").unwrap();
        pd.set_value("Freie Stationierung", "H [m]", "200,25").unwrap();
        pd.set_value("Freie Stationierung", "Orientierung [gon]", "12.3456").unwrap();
        let st = lade_standpunkt(&pd).unwrap();
        assert_eq!(st, StandpunktInfo { punkt_nr: "S7".into(), r: 100.5, h: 200.25, orientierung_gon: 12.3456 });
        // Rückwärtsschnitt hat Vorrang
        pd.set_value("Rückwärtschnitt", "R [m]", "1").unwrap();
        pd.set_value("Rückwärtschnitt", "H [m]", "2").unwrap();
        pd.set_value("Rückwärtschnitt", "Orientierung [gon]", "3").unwrap();
        assert_eq!(lade_standpunkt(&pd).unwrap().punkt_nr, "?");
        let _ = std::fs::remove_dir_all(dir);
    }
}
