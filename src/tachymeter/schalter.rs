// Zweck:    Schaltbefehle am Gerät (Reflektor/Reflektorlos, Laserpointer): sendet die Befehle nacheinander,
//           wartet je Befehl auf die Quittung des Geräts (GeoCOM: Returncode, GSI Online: "?") und meldet
//           Erfolg oder die Fehlermeldung des Geräts. Jeder Schritt wartet höchstens 10 Sekunden.
// Hardware: Leica TS06 / TPS300 / TPS1200
// Autor:    Johann Gerner

use std::collections::VecDeque;
use std::time::Instant;

use super::befehle::Protokoll;
use super::geocom;
use super::gsi;
use super::messung::MessungsTyp;
use super::testmessung::{SCHRITT_TIMEOUT, TestErgebnis};

/// Ablauf eines Schaltvorgangs.
pub struct Schalter {
    beschreibung: String,
    protokoll: Protokoll,
    /// Noch zu sendende Befehle samt RPC-Kontext.
    befehle: VecDeque<(String, i32)>,
    /// Ein Befehl wurde gesendet und wird noch quittiert.
    wartet_auf_quittung: bool,
    schritt_start: Instant,
    erfolg_text: String,
    ergebnis: Option<TestErgebnis>,
}

/// Auswertung einer Zeile als Quittung: `Some(Ok)` bestätigt, `Some(Err)` Fehler des Geräts, `None` irrelevant.
fn quittung(protokoll: Protokoll, zeile: &str) -> Option<Result<(), String>> {
    match protokoll {
        Protokoll::GeoCom => {
            let a = geocom::zerlege_antwort(zeile)?;
            if a.rc == 0 || geocom::ist_informativ(a.rc) {
                Some(Ok(()))
            } else {
                let text = geocom::return_code_text(a.rc);
                Some(Err(if text.is_empty() { format!("GRC={}", a.rc) } else { format!("GRC={}  {text}", a.rc) }))
            }
        }
        Protokoll::GsiOnline => {
            let m = gsi::parse_online_zeile(zeile)?;
            match m.typ {
                MessungsTyp::Fehler => Some(Err(m.bemerkung)),
                MessungsTyp::Status => Some(Ok(())),
                _ => None,
            }
        }
        _ => None,
    }
}

impl Schalter {
    /// Startet einen Schaltvorgang mit den gegebenen Befehlen.
    pub fn neu(
        beschreibung: &str,
        protokoll: Protokoll,
        befehle: Vec<(String, i32)>,
        erfolg_text: &str,
        jetzt: Instant,
    ) -> Self {
        let leer = befehle.is_empty();
        Self {
            beschreibung: beschreibung.to_string(),
            protokoll,
            befehle: befehle.into(),
            wartet_auf_quittung: false,
            schritt_start: jetzt,
            erfolg_text: erfolg_text.to_string(),
            ergebnis: leer.then(|| TestErgebnis {
                ok: false,
                text: format!("{}: Dieses Gerät unterstützt den Befehl nicht.", beschreibung),
            }),
        }
    }

    /// Liefert den nächsten Befehl, sobald der vorige quittiert ist.
    pub fn naechster_befehl(&mut self, jetzt: Instant) -> Option<(String, i32)> {
        if self.ergebnis.is_some() || self.wartet_auf_quittung {
            return None;
        }
        let b = self.befehle.pop_front()?;
        self.wartet_auf_quittung = true;
        self.schritt_start = jetzt;
        Some(b)
    }

    pub fn ergebnis(&self) -> Option<&TestErgebnis> {
        self.ergebnis.as_ref()
    }

    pub fn beschreibung(&self) -> &str {
        &self.beschreibung
    }

    /// Verarbeitet eine empfangene Zeile.
    pub fn zeile(&mut self, zeile: &str, jetzt: Instant) {
        if self.ergebnis.is_some() || !self.wartet_auf_quittung {
            return;
        }
        match quittung(self.protokoll, zeile) {
            Some(Ok(())) => {
                self.wartet_auf_quittung = false;
                self.schritt_start = jetzt;
                if self.befehle.is_empty() {
                    self.ergebnis = Some(TestErgebnis { ok: true, text: self.erfolg_text.clone() });
                }
            }
            Some(Err(fehler)) => {
                self.ergebnis = Some(TestErgebnis {
                    ok: false,
                    text: format!("{} fehlgeschlagen: Gerät meldet {fehler}", self.beschreibung),
                });
            }
            None => {}
        }
    }

    /// Prüft das Zeitlimit des laufenden Schrittes.
    pub fn pruefe_zeit(&mut self, jetzt: Instant) {
        if self.ergebnis.is_none()
            && self.wartet_auf_quittung
            && jetzt.saturating_duration_since(self.schritt_start) >= SCHRITT_TIMEOUT
        {
            self.ergebnis = Some(TestErgebnis {
                ok: false,
                text: format!(
                    "{}: keine Antwort vom Gerät innerhalb von {} s. Sprache (GeoCOM/GSI Online) und Verbindung prüfen.",
                    self.beschreibung,
                    SCHRITT_TIMEOUT.as_secs()
                ),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn zwei_befehle() -> Vec<(String, i32)> {
        vec![("%R1Q,17021,0:1".to_string(), 17021), ("%R1Q,2020,0:5".to_string(), 2020)]
    }

    #[test]
    fn befehle_werden_nacheinander_nach_quittung_gesendet() {
        let t0 = Instant::now();
        let mut s = Schalter::neu("Reflektorlos", Protokoll::GeoCom, zwei_befehle(), "Reflektorlos eingestellt", t0);
        assert_eq!(s.naechster_befehl(t0).unwrap().1, 17021);
        // Solange nicht quittiert, kommt kein weiterer Befehl
        assert!(s.naechster_befehl(t0).is_none());
        s.zeile("irgendwas", t0);
        assert!(s.naechster_befehl(t0).is_none());
        s.zeile("%R1P,0,0:0", t0);
        assert!(s.ergebnis().is_none());
        assert_eq!(s.naechster_befehl(t0).unwrap().1, 2020);
        s.zeile("%R1P,0,0:0", t0);
        let e = s.ergebnis().unwrap();
        assert!(e.ok);
        assert_eq!(e.text, "Reflektorlos eingestellt");
        assert!(s.naechster_befehl(t0).is_none());
    }

    #[test]
    fn informative_codes_gelten_als_erfolg() {
        let t0 = Instant::now();
        let mut s = Schalter::neu("Reflektorlos", Protokoll::GeoCom, zwei_befehle(), "ok", t0);
        s.naechster_befehl(t0);
        s.zeile("%R1P,0,0:3081", t0);
        assert!(s.ergebnis().is_none());
        assert!(s.naechster_befehl(t0).is_some());
    }

    #[test]
    fn geraetefehler_bricht_ab() {
        let t0 = Instant::now();
        let mut s = Schalter::neu("Reflektorlos", Protokoll::GeoCom, zwei_befehle(), "ok", t0);
        s.naechster_befehl(t0);
        s.zeile("%R1P,0,0:1800", t0);
        let e = s.ergebnis().unwrap();
        assert!(!e.ok);
        assert!(e.text.contains("GRC_EDM_DEV_NOT_INSTALLED"), "{}", e.text);
        assert!(s.naechster_befehl(t0).is_none());
    }

    #[test]
    fn gsi_online_quittiert_mit_fragezeichen() {
        let t0 = Instant::now();
        let mut s = Schalter::neu("Laserpointer", Protokoll::GsiOnline, vec![("SET/36/1".into(), 0)], "Laser an", t0);
        s.naechster_befehl(t0);
        s.zeile(" ", t0);
        assert!(s.ergebnis().is_none());
        s.zeile("?", t0);
        assert!(s.ergebnis().unwrap().ok);

        let mut f = Schalter::neu("Laserpointer", Protokoll::GsiOnline, vec![("SET/36/1".into(), 0)], "Laser an", t0);
        f.naechster_befehl(t0);
        f.zeile("@W127", t0);
        assert!(f.ergebnis().unwrap().text.contains("Ungültiger Befehl"));
    }

    #[test]
    fn ohne_antwort_nach_zehn_sekunden_ende() {
        let t0 = Instant::now();
        let mut s = Schalter::neu("Laserpointer", Protokoll::GeoCom, vec![("x".into(), 1)], "ok", t0);
        s.pruefe_zeit(t0 + Duration::from_secs(20));
        assert!(s.ergebnis().is_none(), "ohne gesendeten Befehl läuft noch nichts");
        s.naechster_befehl(t0);
        s.pruefe_zeit(t0 + Duration::from_secs(9));
        assert!(s.ergebnis().is_none());
        s.pruefe_zeit(t0 + SCHRITT_TIMEOUT);
        let e = s.ergebnis().unwrap();
        assert!(!e.ok && e.text.contains("10 s"), "{}", e.text);
    }

    #[test]
    fn leere_befehlsliste_ist_nicht_unterstuetzt() {
        let s = Schalter::neu("Laserpointer", Protokoll::Topcon, vec![], "ok", Instant::now());
        assert!(!s.ergebnis().unwrap().ok);
    }
}
