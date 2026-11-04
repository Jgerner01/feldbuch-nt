// Zweck:    Motorisches Anfahren einer Richtung (GeoCOM): Ist-Winkel lesen → relative Bewegung (6007) → Ist-Winkel prüfen
//           Lesen höchstens 10 s, die Bewegung höchstens 30 s (das Gerät blockiert währenddessen).
// Hardware: Leica TS06 / TPS1200 (motorisiert)
// Autor:    Johann Gerner

use std::f64::consts::PI;
use std::time::{Duration, Instant};

use super::befehle::Protokoll;
use super::geocom;
use super::messung::MessungsTyp;
use crate::grundlagen::format::{f, plus_minus};

/// Wartezeit beim Lesen des Winkels.
pub const TIMEOUT_LESEN: Duration = Duration::from_secs(10);
/// Wartezeit auf das Ende der Bewegung.
pub const TIMEOUT_BEWEGEN: Duration = Duration::from_secs(30);
const GON_ZU_RAD: f64 = PI / 200.0;
const HALBKREIS: f64 = 200.0;
const VOLLKREIS: f64 = 400.0;

/// Ergebnis des Anfahrens.
#[derive(Clone, Debug, PartialEq)]
pub struct AnfahrErgebnis {
    pub ok: bool,
    pub text: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Schritt {
    AktPos,
    Bewegen,
    Winkel,
    Fertig,
}

/// Kürzester Drehweg in gon (−200 … +200).
pub fn kuerzester_weg(delta: f64) -> f64 {
    let mut d = delta;
    while d > HALBKREIS {
        d -= VOLLKREIS;
    }
    while d < -HALBKREIS {
        d += VOLLKREIS;
    }
    d
}

/// Befehl für die relative Bewegung um (ΔHz, ΔV) in gon.
pub fn bewegungsbefehl(d_hz_gon: f64, d_v_gon: f64) -> String {
    format!("%R1Q,{},0:{:.10},{:.10},0,0,0", geocom::RPC_MOT_MOVE_ANGLE, d_hz_gon * GON_ZU_RAD, d_v_gon * GON_ZU_RAD)
}

/// Zustand eines laufenden Anfahrvorgangs.
pub struct Anfahren {
    ziel_hz: f64,
    ziel_v: f64,
    schritt: Schritt,
    schritt_start: Instant,
    offen: Option<(String, i32)>,
    status: String,
    ergebnis: Option<AnfahrErgebnis>,
}

impl Anfahren {
    /// Startet das Anfahren der Richtung (Hz, V) in gon; nur mit GeoCOM möglich.
    pub fn neu(protokoll: Protokoll, ziel_hz: f64, ziel_v: f64, jetzt: Instant) -> Self {
        let mut a = Self {
            ziel_hz,
            ziel_v,
            schritt: Schritt::AktPos,
            schritt_start: jetzt,
            offen: Some((geocom::BEFEHL_WINKEL.to_string(), geocom::RPC_TMC_GET_ANGLE)),
            status: "Lese Ist-Position …".to_string(),
            ergebnis: None,
        };
        if protokoll != Protokoll::GeoCom {
            a.offen = None;
            a.fertig(false, "Anfahren ist nur mit einem Leica-Gerät im GeoCOM-Protokoll möglich.".to_string());
        }
        a
    }

    pub fn naechster_befehl(&mut self) -> Option<(String, i32)> {
        self.offen.take()
    }

    pub fn ergebnis(&self) -> Option<&AnfahrErgebnis> {
        self.ergebnis.as_ref()
    }

    /// Text zum laufenden Schritt.
    pub fn status(&self) -> &str {
        &self.status
    }

    fn fertig(&mut self, ok: bool, text: String) {
        self.schritt = Schritt::Fertig;
        self.status = text.clone();
        self.ergebnis = Some(AnfahrErgebnis { ok, text });
    }

    fn weiter(&mut self, schritt: Schritt, befehl: String, rpc: i32, status: String, jetzt: Instant) {
        self.schritt = schritt;
        self.schritt_start = jetzt;
        self.offen = Some((befehl, rpc));
        self.status = status;
    }

    /// Prüft das Zeitlimit des laufenden Schrittes.
    pub fn pruefe_zeit(&mut self, jetzt: Instant) {
        let limit = match self.schritt {
            Schritt::Fertig => return,
            Schritt::Bewegen => TIMEOUT_BEWEGEN,
            _ => TIMEOUT_LESEN,
        };
        if jetzt.saturating_duration_since(self.schritt_start) >= limit {
            self.fertig(false, format!("Timeout ({} s) – keine Antwort vom Gerät.", limit.as_secs()));
        }
    }

    /// Verarbeitet eine empfangene Zeile.
    pub fn zeile(&mut self, zeile: &str, jetzt: Instant) {
        if self.schritt == Schritt::Fertig || !zeile.trim_start().starts_with("%R1P") {
            return;
        }
        let rpc = if self.schritt == Schritt::Bewegen { geocom::RPC_MOT_MOVE_ANGLE } else { geocom::RPC_TMC_GET_ANGLE };
        let Some(m) = geocom::parse_antwort(zeile, rpc) else {
            return;
        };
        let fehler = m.typ == MessungsTyp::Fehler;
        match self.schritt {
            Schritt::AktPos => {
                let (true, Some(akt_hz)) = (!fehler, m.hz_gon) else {
                    self.fertig(false, format!("Ist-Position nicht lesbar: {}", m.bemerkung));
                    return;
                };
                let akt_v = m.v_gon.unwrap_or(self.ziel_v);
                let d_hz = kuerzester_weg(self.ziel_hz - akt_hz);
                let d_v = self.ziel_v - akt_v;
                let status = format!(
                    "Bewege … Ziel Hz={} gon  V={} gon  (ΔHz={} gon)",
                    f(self.ziel_hz, 4),
                    f(self.ziel_v, 4),
                    plus_minus(d_hz, 4)
                );
                self.weiter(Schritt::Bewegen, bewegungsbefehl(d_hz, d_v), geocom::RPC_MOT_MOVE_ANGLE, status, jetzt);
            }
            Schritt::Bewegen => {
                if fehler {
                    self.fertig(false, format!("Bewegung fehlgeschlagen: {}", m.bemerkung));
                    return;
                }
                self.weiter(
                    Schritt::Winkel,
                    geocom::BEFEHL_WINKEL.to_string(),
                    geocom::RPC_TMC_GET_ANGLE,
                    "Bewegung OK – messe Ist-Winkel …".to_string(),
                    jetzt,
                );
            }
            Schritt::Winkel => match (fehler, m.hz_gon) {
                (false, Some(hz)) => {
                    let v = m.v_gon.map_or_else(|| "NaN".to_string(), |v| f(v, 4));
                    self.fertig(true, format!("Ist-Winkel:  Hz={} gon  V={v} gon", f(hz, 4)));
                }
                _ => self.fertig(false, format!("Bewegung ausgeführt, Winkelmessung fehlgeschlagen: {}", m.bemerkung)),
            },
            Schritt::Fertig => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HUNDERT_GON_RAD: f64 = PI / 2.0;

    fn gestartet() -> (Anfahren, Instant) {
        let t0 = Instant::now();
        let mut a = Anfahren::neu(Protokoll::GeoCom, 100.0, 100.0, t0);
        assert_eq!(a.naechster_befehl(), Some(("%R1Q,2107,0:0".to_string(), 2107)));
        assert!(a.naechster_befehl().is_none());
        (a, t0)
    }

    #[test]
    fn kuerzester_weg_wickelt() {
        assert_eq!(kuerzester_weg(10.0), 10.0);
        assert_eq!(kuerzester_weg(390.0), -10.0);
        assert_eq!(kuerzester_weg(-390.0), 10.0);
        assert_eq!(kuerzester_weg(200.0), 200.0);
        assert_eq!(kuerzester_weg(-200.0), -200.0);
        assert_eq!(kuerzester_weg(600.0), 200.0);
    }

    #[test]
    fn bewegungsbefehl_hat_zehn_nachkommastellen() {
        assert_eq!(bewegungsbefehl(100.0, -50.0), format!("%R1Q,6007,0:{:.10},{:.10},0,0,0", PI / 2.0, -PI / 4.0));
        assert!(bewegungsbefehl(0.0, 0.0).ends_with("0:0.0000000000,0.0000000000,0,0,0"));
    }

    #[test]
    fn voller_ablauf() {
        let (mut a, t0) = gestartet();
        assert_eq!(a.status(), "Lese Ist-Position …");
        // Fremde Zeilen und Befehle werden ignoriert
        a.zeile("Müll", t0);
        a.zeile("%R1Q,2107,0:0", t0);
        assert!(a.ergebnis().is_none() && a.naechster_befehl().is_none());
        // Ist: Hz 90 gon, V 80 gon → Δ = +10 / +20 gon
        let hz = 90.0 * GON_ZU_RAD;
        let v = 80.0 * GON_ZU_RAD;
        a.zeile(&format!("%R1P,0,0:0,{hz},{v}"), t0);
        let (befehl, rpc) = a.naechster_befehl().unwrap();
        assert_eq!(rpc, 6007);
        assert_eq!(befehl, bewegungsbefehl(10.0, 20.0));
        assert!(a.status().contains("ΔHz=+10.0000 gon"), "{}", a.status());
        a.zeile("%R1P,0,0:0", t0);
        assert_eq!(a.naechster_befehl(), Some(("%R1Q,2107,0:0".to_string(), 2107)));
        assert!(a.status().starts_with("Bewegung OK"));
        a.zeile(&format!("%R1P,0,0:0,{HUNDERT_GON_RAD},{HUNDERT_GON_RAD}"), t0);
        let e = a.ergebnis().unwrap();
        assert!(e.ok, "{}", e.text);
        assert_eq!(e.text, "Ist-Winkel:  Hz=100.0000 gon  V=100.0000 gon");
    }

    #[test]
    fn nimmt_den_kuerzesten_weg_ueber_den_nullpunkt() {
        let (mut a, t0) = gestartet();
        let mut a2 = Anfahren::neu(Protokoll::GeoCom, 5.0, 100.0, t0);
        a2.naechster_befehl();
        a2.zeile(&format!("%R1P,0,0:0,{},{}", 395.0 * GON_ZU_RAD, HUNDERT_GON_RAD), t0);
        assert_eq!(a2.naechster_befehl().unwrap().0, bewegungsbefehl(10.0, 0.0));
        a.zeile("%R1P,0,0:0,1.0,1.0", t0);
        assert!(a.naechster_befehl().is_some());
    }

    #[test]
    fn fehler_in_jedem_schritt() {
        let (mut a, t0) = gestartet();
        a.zeile("%R1P,0,0:1,0,0", t0);
        assert!(a.ergebnis().unwrap().text.starts_with("Ist-Position nicht lesbar: GRC="), "{}", a.ergebnis().unwrap().text);
        assert!(!a.ergebnis().unwrap().ok);

        let (mut a, t0) = gestartet();
        a.zeile("%R1P,0,0:0,1.0,1.0", t0);
        a.naechster_befehl();
        a.zeile("%R1P,0,0:1285", t0); // informativ: kein Fehler
        assert!(a.ergebnis().is_none(), "informativer Code ist kein Fehler");
        let (mut a, t0) = gestartet();
        a.zeile("%R1P,0,0:0,1.0,1.0", t0);
        a.naechster_befehl();
        a.zeile("%R1P,0,0:1292", t0);
        assert!(a.ergebnis().unwrap().text.starts_with("Bewegung fehlgeschlagen:"));

        let (mut a, t0) = gestartet();
        a.zeile("%R1P,0,0:0,1.0,1.0", t0);
        a.naechster_befehl();
        a.zeile("%R1P,0,0:0", t0);
        a.naechster_befehl();
        a.zeile("%R1P,0,0:1,0,0", t0);
        let e = a.ergebnis().unwrap();
        assert!(!e.ok && e.text.starts_with("Bewegung ausgeführt, Winkelmessung fehlgeschlagen:"));
        // Danach ändert sich nichts mehr
        a.zeile("%R1P,0,0:0,1.0,1.0", t0);
        assert!(!a.ergebnis().unwrap().ok);
    }

    #[test]
    fn zeitlimits_je_schritt() {
        let (mut a, t0) = gestartet();
        a.pruefe_zeit(t0 + Duration::from_secs(9));
        assert!(a.ergebnis().is_none());
        a.pruefe_zeit(t0 + TIMEOUT_LESEN);
        assert_eq!(a.ergebnis().unwrap().text, "Timeout (10 s) – keine Antwort vom Gerät.");

        let (mut a, t0) = gestartet();
        let t1 = t0 + Duration::from_secs(5);
        a.zeile("%R1P,0,0:0,1.0,1.0", t1);
        a.naechster_befehl();
        a.pruefe_zeit(t1 + Duration::from_secs(29));
        assert!(a.ergebnis().is_none(), "Bewegung darf 30 s dauern");
        a.pruefe_zeit(t1 + TIMEOUT_BEWEGEN);
        assert_eq!(a.ergebnis().unwrap().text, "Timeout (30 s) – keine Antwort vom Gerät.");
    }

    #[test]
    fn nur_mit_geocom() {
        for p in [Protokoll::GsiOnline, Protokoll::Keines] {
            let mut a = Anfahren::neu(p, 0.0, 100.0, Instant::now());
            assert!(a.naechster_befehl().is_none());
            assert!(!a.ergebnis().unwrap().ok);
        }
    }
}
