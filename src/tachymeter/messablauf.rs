// Zweck:    Messablauf einer Punktmessung wie im DXF-Viewer der Originalversion:
//           GeoCOM:     DoMeasure → GetSimpleMea → Kompensator → Atmosphäre → EDM-Modus → Prismenkonstante
//           GSI Online: GET/M (Hz, V, Schrägstrecke in einem Schritt)
//           Die Zusatzdaten (Kompensator, Atmosphäre, EDM, Prisma) sind freiwillig: Fehlen sie, wird mit den
//           Messwerten weitergearbeitet. Pflichtschritte warten höchstens 10 s, Zusatzschritte 3 s.
// Hardware: Leica TS06 / TPS300 / TPS1200
// Autor:    Johann Gerner

use std::time::{Duration, Instant};

use super::befehle::Protokoll;
use super::geocom;
use super::gsi;
use super::messung::{MessungsTyp, TachymeterMessung};
use super::testmessung::SCHRITT_TIMEOUT;

/// Wartezeit auf die Antwort eines freiwilligen Zusatzschrittes.
pub const ZUSATZ_TIMEOUT: Duration = Duration::from_secs(3);

/// Befehle der Zusatzschritte.
const BEFEHL_KOMPENSATOR: &str = "%R1Q,2003,0:1";
const BEFEHL_ATM: &str = "%R1Q,2029,0:";
const BEFEHL_EDM: &str = "%R1Q,2021,0:";
const BEFEHL_PRISMA: &str = "%R1Q,2023,0:";

/// Ergebnis des Messablaufs.
#[derive(Clone, Debug)]
pub enum MessErgebnis {
    /// Vollständige Messung (Hz, V, Schrägstrecke).
    Messung(Box<TachymeterMessung>),
    /// Fehlermeldung für die Statuszeile.
    Fehler(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Schritt {
    Messen,
    Ergebnis,
    Kompensator,
    Atmosphaere,
    EdmModus,
    Prisma,
    GsiMessen,
    Fertig,
}

impl Schritt {
    fn rpc(self) -> i32 {
        match self {
            Self::Messen => geocom::RPC_TMC_DO_MEASURE,
            Self::Ergebnis => geocom::RPC_TMC_GET_SIMPLE_MEA,
            Self::Kompensator => geocom::RPC_TMC_GET_ANGLE1,
            Self::Atmosphaere => geocom::RPC_TMC_GET_ATM_CORR,
            Self::EdmModus => geocom::RPC_TMC_GET_EDM_MODE,
            Self::Prisma => geocom::RPC_TMC_GET_PRISM_CORR,
            Self::GsiMessen | Self::Fertig => 0,
        }
    }

    /// Freiwillige Zusatzschritte.
    fn ist_zusatz(self) -> bool {
        matches!(self, Self::Kompensator | Self::Atmosphaere | Self::EdmModus | Self::Prisma)
    }

    fn timeout(self) -> Duration {
        if self.ist_zusatz() { ZUSATZ_TIMEOUT } else { SCHRITT_TIMEOUT }
    }

    fn text(self) -> &'static str {
        match self {
            Self::Messen | Self::GsiMessen => "Messung läuft …",
            Self::Ergebnis => "Messwerte abholen …",
            Self::Kompensator => "Messung OK – Kompensator …",
            Self::Atmosphaere => "Kompensator OK – Atmosph. Korrektur …",
            Self::EdmModus => "AtmKorr OK – EDM-Modus …",
            Self::Prisma => "EDM-Modus OK – Prismenkonstante …",
            Self::Fertig => "Fertig",
        }
    }
}

/// Zustand einer laufenden Messung.
pub struct Messablauf {
    protokoll: Protokoll,
    schritt: Schritt,
    schritt_start: Instant,
    offen: Option<(String, i32)>,
    sammlung: Option<TachymeterMessung>,
    zielhoehe: f64,
    ergebnis: Option<MessErgebnis>,
}

impl Messablauf {
    /// Startet eine Messung; `zielhoehe` [m] wird in die Messung eingetragen.
    pub fn neu(protokoll: Protokoll, zielhoehe: f64, jetzt: Instant) -> Self {
        let (schritt, offen, ergebnis) = match protokoll {
            Protokoll::GeoCom => {
                (Schritt::Messen, Some((geocom::BEFEHL_MESS_TRIGGER.to_string(), geocom::RPC_TMC_DO_MEASURE)), None)
            }
            Protokoll::GsiOnline => (Schritt::GsiMessen, Some((gsi::BEFEHL_MESS_TRIGGER.to_string(), 0)), None),
            _ => (
                Schritt::Fertig,
                None,
                Some(MessErgebnis::Fehler("Für dieses Gerät ist keine Messung über die Schnittstelle möglich.".to_string())),
            ),
        };
        Self { protokoll, schritt, schritt_start: jetzt, offen, sammlung: None, zielhoehe, ergebnis }
    }

    /// Liefert den nächsten zu sendenden Befehl (einmalig) samt RPC-Kontext.
    pub fn naechster_befehl(&mut self) -> Option<(String, i32)> {
        self.offen.take()
    }

    pub fn ergebnis(&self) -> Option<&MessErgebnis> {
        self.ergebnis.as_ref()
    }

    /// Text für die Statuszeile.
    pub fn status_text(&self) -> &'static str {
        self.schritt.text()
    }

    fn fehler(&mut self, text: String) {
        self.schritt = Schritt::Fertig;
        self.ergebnis = Some(MessErgebnis::Fehler(text));
    }

    fn beende(&mut self, mut m: TachymeterMessung) {
        m.zielhoehe_m = Some(self.zielhoehe);
        self.schritt = Schritt::Fertig;
        self.ergebnis = Some(MessErgebnis::Messung(Box::new(m)));
    }

    fn weiter(&mut self, schritt: Schritt, befehl: &str, jetzt: Instant) {
        self.schritt = schritt;
        self.schritt_start = jetzt;
        self.offen = Some((befehl.to_string(), schritt.rpc()));
    }

    /// Nach einem Zusatzschritt zum nächsten oder zum Ende.
    fn naechster_zusatz(&mut self, jetzt: Instant) {
        match self.schritt {
            Schritt::Kompensator => self.weiter(Schritt::Atmosphaere, BEFEHL_ATM, jetzt),
            Schritt::Atmosphaere => self.weiter(Schritt::EdmModus, BEFEHL_EDM, jetzt),
            Schritt::EdmModus => self.weiter(Schritt::Prisma, BEFEHL_PRISMA, jetzt),
            _ => {
                if let Some(m) = self.sammlung.take() {
                    self.beende(m);
                }
            }
        }
    }

    /// Prüft die Zeitlimits: Pflichtschritte brechen ab, Zusatzschritte werden übersprungen.
    pub fn pruefe_zeit(&mut self, jetzt: Instant) {
        if self.schritt == Schritt::Fertig || jetzt.saturating_duration_since(self.schritt_start) < self.schritt.timeout() {
            return;
        }
        if self.schritt.ist_zusatz() {
            self.naechster_zusatz(jetzt);
        } else {
            self.fehler(format!(
                "Zeitüberschreitung: keine Antwort vom Tachymeter (nach {} s).",
                SCHRITT_TIMEOUT.as_secs()
            ));
        }
    }

    /// Verarbeitet eine empfangene Zeile.
    pub fn zeile(&mut self, zeile: &str, jetzt: Instant) {
        match self.protokoll {
            Protokoll::GsiOnline => self.zeile_gsi(zeile),
            Protokoll::GeoCom => self.zeile_geocom(zeile, jetzt),
            _ => {}
        }
    }

    fn zeile_gsi(&mut self, zeile: &str) {
        if self.schritt != Schritt::GsiMessen || !gsi::kann_verarbeiten_online(zeile) {
            return;
        }
        let Some(m) = gsi::parse_online_zeile(zeile) else {
            return;
        };
        match m.typ {
            MessungsTyp::Status => {}
            MessungsTyp::Fehler => self.fehler(format!("Messfehler: {}", m.bemerkung)),
            _ if !m.ist_vollmessung() => self.fehler("Unvollständige Messung (Hz/V/D fehlen).".to_string()),
            _ => self.beende(m),
        }
    }

    fn zeile_geocom(&mut self, zeile: &str, jetzt: Instant) {
        if self.schritt == Schritt::Fertig || !zeile.trim_start().starts_with("%R1P") {
            return;
        }
        let Some(m) = geocom::parse_antwort(zeile, self.schritt.rpc()) else {
            return;
        };
        let fehler = m.typ == MessungsTyp::Fehler;
        match self.schritt {
            Schritt::Messen => {
                if fehler {
                    self.fehler(format!("Messfehler: {}", m.bemerkung));
                } else {
                    self.weiter(Schritt::Ergebnis, geocom::BEFEHL_MESS_ERGEBNIS, jetzt);
                }
            }
            Schritt::Ergebnis => {
                if fehler {
                    self.fehler(format!("Messung fehlgeschlagen: {}", m.bemerkung));
                } else if !m.ist_vollmessung() {
                    self.fehler("Unvollständige Messung (Hz/V/D fehlen).".to_string());
                } else {
                    self.sammlung = Some(m);
                    self.weiter(Schritt::Kompensator, BEFEHL_KOMPENSATOR, jetzt);
                }
            }
            Schritt::Kompensator => {
                if let (false, Some(s)) = (fehler, self.sammlung.as_mut()) {
                    s.kreuz_neigung_rad = m.kreuz_neigung_rad;
                    s.laengs_neigung_rad = m.laengs_neigung_rad;
                }
                self.naechster_zusatz(jetzt);
            }
            Schritt::Atmosphaere => {
                if let (false, Some(s)) = (fehler, self.sammlung.as_mut()) {
                    s.atm_lambda_m = m.atm_lambda_m;
                    s.atm_druck_mbar = m.atm_druck_mbar;
                    s.atm_temp_trock_c = m.atm_temp_trock_c;
                    s.atm_temp_feucht_c = m.atm_temp_feucht_c;
                    s.atm_ppm = m.atm_ppm;
                }
                self.naechster_zusatz(jetzt);
            }
            Schritt::EdmModus => {
                if let (false, Some(s)) = (fehler, self.sammlung.as_mut()) {
                    s.edm_modus = m.edm_modus;
                    s.edm_modus_roh = m.edm_modus_roh;
                }
                self.naechster_zusatz(jetzt);
            }
            Schritt::Prisma => {
                if let (false, Some(s)) = (fehler, self.sammlung.as_mut()) {
                    s.prismenkonstante_mm = m.prismenkonstante_mm;
                }
                self.naechster_zusatz(jetzt);
            }
            Schritt::GsiMessen | Schritt::Fertig => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tachymeter::messung::MessungsEdmModus;

    // Echte Antwortzeilen der Leica TS06 aus dem Kommunikationsprotokoll vom 4.11.2026
    const A_DO_MEASURE: &str = "%R1P,0,0:0";
    const A_SIMPLE_MEA: &str = "%R1P,0,0:0,4.160426404530059,1.524550413535651,1.442399999999949";
    const A_DIST_FEHLER: &str = "%R1P,0,0:1292,4.160426383286048,1.524550998866913,0";
    const A_ANGLE1: &str = "%R1P,0,0:0,4.16,1.52,0.0001,12345,0.0004,-0.0003,0.0001,12346,0";
    const A_ATM: &str = "%R1P,0,0:0,6.58e-7,1013.25,15.0,10.0";
    const A_EDM: &str = "%R1P,0,0:0,5";
    const A_PRISMA: &str = "%R1P,0,0:0,-0.0175";

    fn durchlauf(m: &mut Messablauf, t0: Instant, antworten: &[&str]) {
        for a in antworten {
            assert!(m.naechster_befehl().is_some(), "Befehl fehlt vor Antwort {a}");
            m.zeile(a, t0);
        }
    }

    #[test]
    fn vollstaendiger_geocom_ablauf_sammelt_alle_zusatzdaten() {
        let t0 = Instant::now();
        let mut m = Messablauf::neu(Protokoll::GeoCom, 1.5, t0);
        let (b, rpc) = m.naechster_befehl().unwrap();
        assert_eq!((b.as_str(), rpc), ("%R1Q,2008,0:1,1", 2008));
        m.zeile(A_DO_MEASURE, t0);
        let (b, rpc) = m.naechster_befehl().unwrap();
        assert_eq!((b.as_str(), rpc), ("%R1Q,2108,0:5000,1", 2108));
        m.zeile(A_SIMPLE_MEA, t0);
        let (b, rpc) = m.naechster_befehl().unwrap();
        assert_eq!((b.as_str(), rpc), ("%R1Q,2003,0:1", 2003));
        m.zeile(A_ANGLE1, t0);
        assert_eq!(m.naechster_befehl().unwrap().0, "%R1Q,2029,0:");
        m.zeile(A_ATM, t0);
        assert_eq!(m.naechster_befehl().unwrap().0, "%R1Q,2021,0:");
        m.zeile(A_EDM, t0);
        assert_eq!(m.naechster_befehl().unwrap().0, "%R1Q,2023,0:");
        m.zeile(A_PRISMA, t0);

        let Some(MessErgebnis::Messung(g)) = m.ergebnis().cloned() else {
            panic!("Messung erwartet");
        };
        assert!(g.ist_vollmessung());
        assert!((g.schraegstrecke_m.unwrap() - 1.4424).abs() < 1e-9);
        assert_eq!(g.zielhoehe_m, Some(1.5));
        assert_eq!(g.edm_modus, MessungsEdmModus::Reflektorlos);
        assert!((g.prismenkonstante_mm.unwrap() + 17.5).abs() < 1e-9);
        assert!(g.atm_ppm.is_some());
        assert!(g.kreuz_neigung_rad.is_some() && g.laengs_neigung_rad.is_some());
        assert!(m.naechster_befehl().is_none());
    }

    #[test]
    fn distanzfehler_aus_dem_protokoll_beendet_mit_klartext() {
        let t0 = Instant::now();
        let mut m = Messablauf::neu(Protokoll::GeoCom, 0.0, t0);
        durchlauf(&mut m, t0, &[A_DO_MEASURE, A_DIST_FEHLER]);
        let Some(MessErgebnis::Fehler(t)) = m.ergebnis() else {
            panic!("Fehler erwartet");
        };
        assert!(t.contains("GRC_TMC_DIST_ERROR"), "{t}");
        assert!(m.naechster_befehl().is_none());
    }

    #[test]
    fn fehlende_zusatzdaten_verhindern_die_messung_nicht() {
        let t0 = Instant::now();
        let mut m = Messablauf::neu(Protokoll::GeoCom, 0.0, t0);
        durchlauf(&mut m, t0, &[A_DO_MEASURE, A_SIMPLE_MEA]);
        // Kompensator liefert einen Fehlercode: wird übersprungen
        m.naechster_befehl();
        m.zeile("%R1P,0,0:1", t0);
        // Atmosphäre antwortet nie: nach 3 s weiter
        m.naechster_befehl();
        m.pruefe_zeit(t0 + Duration::from_secs(2));
        assert!(m.ergebnis().is_none());
        m.pruefe_zeit(t0 + ZUSATZ_TIMEOUT);
        assert_eq!(m.naechster_befehl().unwrap().0, "%R1Q,2021,0:");
        m.zeile(A_EDM, t0);
        m.naechster_befehl();
        m.pruefe_zeit(t0 + ZUSATZ_TIMEOUT * 2);
        let Some(MessErgebnis::Messung(g)) = m.ergebnis() else {
            panic!("Messung erwartet");
        };
        assert!(g.kreuz_neigung_rad.is_none());
        assert!(g.edm_modus_roh == Some(5));
        assert!(g.prismenkonstante_mm.is_none());
    }

    #[test]
    fn pflichtschritt_bricht_nach_zehn_sekunden_ab() {
        let t0 = Instant::now();
        let mut m = Messablauf::neu(Protokoll::GeoCom, 0.0, t0);
        m.naechster_befehl();
        m.pruefe_zeit(t0 + Duration::from_secs(9));
        assert!(m.ergebnis().is_none());
        m.pruefe_zeit(t0 + SCHRITT_TIMEOUT);
        assert!(matches!(m.ergebnis(), Some(MessErgebnis::Fehler(t)) if t.contains("Zeitüberschreitung")));
    }

    #[test]
    fn unvollstaendige_messung_wird_abgelehnt() {
        let t0 = Instant::now();
        let mut m = Messablauf::neu(Protokoll::GeoCom, 0.0, t0);
        durchlauf(&mut m, t0, &[A_DO_MEASURE, "%R1P,0,0:0,1.0,1.5"]);
        assert!(matches!(m.ergebnis(), Some(MessErgebnis::Fehler(t)) if t.contains("Unvollständige")));
    }

    #[test]
    fn gsi_online_einstufig() {
        let t0 = Instant::now();
        let mut m = Messablauf::neu(Protokoll::GsiOnline, 1.3, t0);
        assert_eq!(m.naechster_befehl().unwrap().0, "GET/M/WI21/WI22/WI31");
        m.zeile("?", t0);
        m.zeile(" ", t0);
        assert!(m.ergebnis().is_none());
        m.zeile("21.022+17920860 22.022+09843660 31..00+00152340", t0);
        let Some(MessErgebnis::Messung(g)) = m.ergebnis() else {
            panic!("Messung erwartet");
        };
        assert_eq!(g.zielhoehe_m, Some(1.3));

        let mut f = Messablauf::neu(Protokoll::GsiOnline, 0.0, t0);
        f.naechster_befehl();
        f.zeile("@E139", t0);
        assert!(matches!(f.ergebnis(), Some(MessErgebnis::Fehler(t)) if t.contains("EDM-Fehler")));

        let mut u = Messablauf::neu(Protokoll::GsiOnline, 0.0, t0);
        u.naechster_befehl();
        u.zeile("21.022+17920860 22.022+09843660", t0);
        assert!(matches!(u.ergebnis(), Some(MessErgebnis::Fehler(t)) if t.contains("Unvollständige")));
    }

    #[test]
    fn andere_geraete_koennen_nicht_messen() {
        let m = Messablauf::neu(Protokoll::Keines, 0.0, Instant::now());
        assert!(matches!(m.ergebnis(), Some(MessErgebnis::Fehler(_))));
    }
}
