// Zweck:    Bewegungsrichtung der GNSS-Antenne aus der Spur (für die Ausrichtung der Absteckgrafik):
//           Richtung über die letzten Meter, Glättung, Hysterese und Einfrieren bei Stillstand oder nahe am Ziel
// Hardware: keine (Positionen vom GNSS-Empfänger u-blox ZED-F9P)
// Autor:    Johann Gerner

use std::collections::VecDeque;

/// Neue Spurpunkte erst ab diesem Abstand zum letzten [m] (Rauschen im Stand).
const MIN_SCHRITT_M: f64 = 0.05;
/// Höchstzahl der Spurpunkte.
const MAX_PUNKTE: usize = 400;
/// Richtungsänderungen unter dieser Schwelle werden ignoriert [rad] (≈ 10 gon).
const HYSTERESE_RAD: f64 = 0.157;
/// Darunter gilt die Antenne als stehend [m/s].
const MIN_GESCHWINDIGKEIT: f64 = 0.1;
/// Zeitfenster der Geschwindigkeit [s].
const GESCHWINDIGKEIT_FENSTER_S: f64 = 2.0;

/// Ein Spurpunkt (Plankoordinaten R, H und Zeit in Sekunden).
#[derive(Clone, Copy, Debug)]
struct Punkt {
    r: f64,
    h: f64,
    t: f64,
}

/// Bewegungsrichtung aus der Spur.
#[derive(Clone, Debug, Default)]
pub struct Bewegung {
    spur: VecDeque<Punkt>,
    /// Richtungswinkel ab Nord (Hochwert), im Uhrzeigersinn [rad]
    richtung: Option<f64>,
    eingefroren: bool,
}

impl Bewegung {
    /// Nimmt eine Position auf. `min_m`: Mindestbewegung für eine Richtung, `nahe`: nahe am Ziel (Richtung festhalten).
    pub fn neue_position(&mut self, r: f64, h: f64, t: f64, min_m: f64, nahe: bool) {
        let weit_genug = self.spur.back().is_none_or(|p| (p.r - r).hypot(p.h - h) >= MIN_SCHRITT_M);
        if weit_genug {
            self.spur.push_back(Punkt { r, h, t });
            while self.spur.len() > MAX_PUNKTE {
                self.spur.pop_front();
            }
        }
        let steht = self.geschwindigkeit(t) < MIN_GESCHWINDIGKEIT;
        self.eingefroren = nahe || steht;
        if self.eingefroren {
            return;
        }
        if let Some(neu) = self.richtung_ueber(r, h, min_m) {
            let wechsel = self.richtung.is_none_or(|alt| winkel_differenz(alt, neu).abs() > HYSTERESE_RAD);
            if wechsel {
                self.richtung = Some(neu);
            }
        }
    }

    /// Richtung ab Nord [rad], falls schon bestimmt.
    pub fn richtung(&self) -> Option<f64> {
        self.richtung
    }

    /// Die Richtung wird gerade festgehalten (Stillstand oder nahe am Ziel).
    pub fn eingefroren(&self) -> bool {
        self.eingefroren
    }

    /// Vergisst Spur und Richtung (z. B. bei einem neuen Ziel).
    pub fn zuruecksetzen(&mut self) {
        *self = Self::default();
    }

    /// Geschwindigkeit über das Zeitfenster [m/s].
    fn geschwindigkeit(&self, jetzt: f64) -> f64 {
        let Some(letzter) = self.spur.back() else {
            return 0.0;
        };
        let Some(erster) = self.spur.iter().find(|p| jetzt - p.t <= GESCHWINDIGKEIT_FENSTER_S) else {
            return 0.0;
        };
        let dt = (jetzt - erster.t).max(GESCHWINDIGKEIT_FENSTER_S / 2.0);
        (letzter.r - erster.r).hypot(letzter.h - erster.h) / dt
    }

    /// Richtung über die letzten `min_m` Meter: Schwerpunkt der älteren zum Schwerpunkt der neueren Hälfte.
    fn richtung_ueber(&self, r: f64, h: f64, min_m: f64) -> Option<f64> {
        let start = self.spur.iter().rposition(|p| (p.r - r).hypot(p.h - h) >= min_m)?;
        let abschnitt: Vec<&Punkt> = self.spur.iter().skip(start).collect();
        if abschnitt.len() < 2 {
            return None;
        }
        let mitte = abschnitt.len() / 2;
        let schwerpunkt = |teil: &[&Punkt]| {
            let n = teil.len() as f64;
            (teil.iter().map(|p| p.r).sum::<f64>() / n, teil.iter().map(|p| p.h).sum::<f64>() / n)
        };
        let (r0, h0) = schwerpunkt(&abschnitt[..mitte.max(1)]);
        let (r1, h1) = schwerpunkt(&abschnitt[mitte..]);
        let (dr, dh) = (r1 - r0, h1 - h0);
        (dr.hypot(dh) > f64::EPSILON).then(|| dr.atan2(dh))
    }
}

/// Kleinste Differenz zweier Richtungen [rad] im Bereich −π … π.
pub fn winkel_differenz(a: f64, b: f64) -> f64 {
    use std::f64::consts::{PI, TAU};
    (b - a + PI).rem_euclid(TAU) - PI
}

/// Zerlegt den Vektor Antenne → Ziel in „vor“ und „rechts“ bezogen auf die Richtung (ab Nord) [m].
pub fn vor_rechts(richtung: f64, d_r: f64, d_h: f64) -> (f64, f64) {
    let (s, c) = richtung.sin_cos();
    (d_r * s + d_h * c, d_r * c - d_h * s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::FRAC_PI_2;

    #[test]
    fn richtung_nach_genug_weg() {
        let mut b = Bewegung::default();
        // nach Osten mit 1 m/s
        for i in 0..=10 {
            b.neue_position(f64::from(i) * 0.5, 0.0, f64::from(i) * 0.5, 2.0, false);
        }
        let r = b.richtung().expect("Richtung");
        assert!((r - FRAC_PI_2).abs() < 1e-6, "{r}");
    }

    #[test]
    fn keine_richtung_unter_mindestweg() {
        let mut b = Bewegung::default();
        for i in 0..=3 {
            b.neue_position(f64::from(i) * 0.5, 0.0, f64::from(i) * 0.5, 2.0, false);
        }
        assert!(b.richtung().is_none());
    }

    #[test]
    fn stillstand_und_naehe_frieren_ein() {
        let mut b = Bewegung::default();
        for i in 0..=10 {
            b.neue_position(0.0, f64::from(i) * 0.5, f64::from(i) * 0.5, 2.0, false);
        }
        let vorher = b.richtung().unwrap();
        // Rauschen im Stand dreht nicht
        for i in 0..10 {
            b.neue_position(0.02 * f64::from(i % 2), 5.0, 5.5 + f64::from(i), 2.0, false);
        }
        assert!(b.eingefroren());
        assert_eq!(b.richtung(), Some(vorher));
        // nahe am Ziel: auch bei Bewegung festhalten
        for i in 0..10 {
            b.neue_position(f64::from(i), 5.0, 20.0 + f64::from(i), 2.0, true);
        }
        assert_eq!(b.richtung(), Some(vorher));
    }

    #[test]
    fn zerlegung_vor_rechts() {
        // Blick nach Osten, Ziel 3 m östlich und 1 m südlich: 3 m vor, 1 m rechts
        let (vor, rechts) = vor_rechts(FRAC_PI_2, 3.0, -1.0);
        assert!((vor - 3.0).abs() < 1e-12 && (rechts - 1.0).abs() < 1e-12);
        assert!((winkel_differenz(0.1, 6.2) + 0.1 + (std::f64::consts::TAU - 6.2)).abs() < 1e-12);
    }
}
