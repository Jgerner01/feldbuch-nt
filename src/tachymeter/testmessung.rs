// Zweck:    Testmessung zum Prüfen der Verbindungs- und Protokolleinstellungen. Ablauf je Protokoll:
//           GeoCOM:     Winkel lesen (2107) → Messung auslösen (2008) → Ergebnis holen (2108)
//           GSI Online: Messung mit Hz, V und Schrägstrecke (GET/M)
//           Jeder Schritt wartet höchstens 10 Sekunden auf die Antwort.
// Hardware: Leica TS06 / TPS300 / TPS1200
// Autor:    Johann Gerner

use std::time::{Duration, Instant};

use super::befehle::{Befehlsgeber, Protokoll};
use super::geocom;
use super::gsi;
use super::messung::{MessungsTyp, TachymeterMessung};
use crate::grundlagen::format::de;

/// Längste Wartezeit auf die Antwort eines Schrittes.
pub const SCHRITT_TIMEOUT: Duration = Duration::from_secs(10);

/// Ergebnis der Testmessung.
#[derive(Clone, Debug, PartialEq)]
pub struct TestErgebnis {
    pub ok: bool,
    pub text: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Schritt {
    Winkel,
    Messen,
    Ergebnis,
    GsiMessen,
    Fertig,
}

/// Zustand einer laufenden Testmessung.
pub struct Testmessung {
    protokoll: Protokoll,
    schritt: Schritt,
    schritt_start: Instant,
    /// Noch nicht gesendeter Befehl samt zugehörigem RPC.
    offen: Option<(String, i32)>,
    hz: Option<f64>,
    v: Option<f64>,
    ergebnis: Option<TestErgebnis>,
}

impl Testmessung {
    /// Startet die Testmessung für das Protokoll.
    pub fn neu(protokoll: Protokoll, jetzt: Instant) -> Self {
        let bg = Befehlsgeber::fuer(protokoll);
        let (schritt, offen, ergebnis) = match protokoll {
            Protokoll::GeoCom => (Schritt::Winkel, bg.winkel_befehl().map(|b| (b, geocom::RPC_TMC_GET_ANGLE)), None),
            Protokoll::GsiOnline => (Schritt::GsiMessen, bg.mess_trigger_befehl().map(|b| (b, 0)), None),
            _ => (
                Schritt::Fertig,
                None,
                Some(TestErgebnis {
                    ok: false,
                    text: "Für dieses Gerät gibt es keine Testmessung (nur Leica GeoCOM und GSI Online).".to_string(),
                }),
            ),
        };
        Self { protokoll, schritt, schritt_start: jetzt, offen, hz: None, v: None, ergebnis }
    }

    /// Liefert den nächsten zu sendenden Befehl (einmalig) samt RPC-Kontext für den Parser.
    pub fn naechster_befehl(&mut self) -> Option<(String, i32)> {
        self.offen.take()
    }

    /// Das Ergebnis, sobald die Testmessung beendet ist.
    pub fn ergebnis(&self) -> Option<&TestErgebnis> {
        self.ergebnis.as_ref()
    }

    /// Beschreibung des laufenden Schrittes für die Anzeige.
    pub fn schritt_text(&self) -> String {
        match self.schritt {
            Schritt::Winkel => "Schritt 1/3: Winkel lesen …".to_string(),
            Schritt::Messen => "Schritt 2/3: Messung auslösen …".to_string(),
            Schritt::Ergebnis => "Schritt 3/3: Messergebnis abholen …".to_string(),
            Schritt::GsiMessen => "Messung läuft (GSI Online) …".to_string(),
            Schritt::Fertig => "Fertig".to_string(),
        }
    }

    fn fertig(&mut self, ok: bool, text: String) {
        self.schritt = Schritt::Fertig;
        self.ergebnis = Some(TestErgebnis { ok, text });
    }

    fn weiter(&mut self, schritt: Schritt, befehl: Option<(String, i32)>, jetzt: Instant) {
        self.schritt = schritt;
        self.schritt_start = jetzt;
        self.offen = befehl;
    }

    /// Prüft das Zeitlimit des laufenden Schrittes.
    pub fn pruefe_zeit(&mut self, jetzt: Instant) {
        if self.schritt == Schritt::Fertig || jetzt.saturating_duration_since(self.schritt_start) < SCHRITT_TIMEOUT {
            return;
        }
        let sprache = match self.protokoll {
            Protokoll::GsiOnline => "GSI Online",
            _ => "GeoCOM",
        };
        self.fertig(
            false,
            format!(
                "Keine Antwort vom Gerät innerhalb von {} s ({}). Bitte prüfen: Gerät eingeschaltet und Bluetooth verbunden, \
                 richtiger COM-Port, Baudrate/Zeichenformat und die Sprache am Gerät ({sprache}).",
                SCHRITT_TIMEOUT.as_secs(),
                self.schritt_text().trim_end_matches(" …")
            ),
        );
    }

    /// Verarbeitet eine empfangene Zeile.
    pub fn zeile(&mut self, zeile: &str, jetzt: Instant) {
        match self.schritt {
            Schritt::Winkel | Schritt::Messen | Schritt::Ergebnis => self.zeile_geocom(zeile, jetzt),
            Schritt::GsiMessen => self.zeile_gsi(zeile),
            Schritt::Fertig => {}
        }
    }

    fn zeile_geocom(&mut self, zeile: &str, jetzt: Instant) {
        if !zeile.trim_start().starts_with("%R1P") {
            return;
        }
        let rpc = match self.schritt {
            Schritt::Winkel => geocom::RPC_TMC_GET_ANGLE,
            Schritt::Messen => geocom::RPC_TMC_DO_MEASURE,
            _ => geocom::RPC_TMC_GET_SIMPLE_MEA,
        };
        let Some(m) = geocom::parse_antwort(zeile, rpc) else {
            return;
        };
        if m.typ == MessungsTyp::Fehler {
            let text = format!("{} fehlgeschlagen: {}", self.schritt_text().trim_end_matches(" …"), m.bemerkung);
            self.fertig(false, text);
            return;
        }
        match self.schritt {
            Schritt::Winkel => {
                self.hz = m.hz_gon;
                self.v = m.v_gon;
                self.weiter(Schritt::Messen, Some((geocom::BEFEHL_MESS_TRIGGER.to_string(), geocom::RPC_TMC_DO_MEASURE)), jetzt);
            }
            Schritt::Messen => {
                self.weiter(Schritt::Ergebnis, Some((geocom::BEFEHL_MESS_ERGEBNIS.to_string(), geocom::RPC_TMC_GET_SIMPLE_MEA)), jetzt);
            }
            _ => self.auswerten(&m),
        }
    }

    fn zeile_gsi(&mut self, zeile: &str) {
        let Some(m) = gsi::parse_online_zeile(zeile) else {
            return;
        };
        match m.typ {
            // "?" = Bestätigung ohne Messwert: weiter warten
            MessungsTyp::Status => {}
            MessungsTyp::Fehler => self.fertig(false, format!("Messung fehlgeschlagen: {}", m.bemerkung)),
            _ => self.auswerten(&m),
        }
    }

    fn auswerten(&mut self, m: &TachymeterMessung) {
        match (m.hz_gon, m.v_gon, m.schraegstrecke_m) {
            (Some(hz), Some(v), Some(s)) => self.fertig(
                true,
                format!(
                    "Testmessung erfolgreich ({}): Hz = {} gon, V = {} gon, Schrägstrecke = {} m",
                    if self.protokoll == Protokoll::GsiOnline { "GSI Online" } else { "GeoCOM" },
                    de(hz, 4),
                    de(v, 4),
                    de(s, 3)
                ),
            ),
            (Some(hz), Some(v), None) => self.fertig(
                false,
                format!(
                    "Verbindung funktioniert (Hz = {} gon, V = {} gon), aber ohne Strecke. Prisma/Ziel anzielen und Messmodus prüfen.",
                    de(hz, 4),
                    de(v, 4)
                ),
            ),
            _ => self.fertig(false, format!("Antwort ohne Messwerte: {}", m.text())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 100 gon in Bogenmaß.
    const HUNDERT_GON_RAD: f64 = std::f64::consts::PI / 2.0;

    #[test]
    fn geocom_ablauf_bis_zum_ergebnis() {
        let t0 = Instant::now();
        let mut t = Testmessung::neu(Protokoll::GeoCom, t0);
        let (b, rpc) = t.naechster_befehl().unwrap();
        assert_eq!((b.as_str(), rpc), ("%R1Q,2107,0:0", 2107));
        assert!(t.naechster_befehl().is_none());
        // Fremde Zeilen werden ignoriert
        t.zeile(" ", t0);
        t.zeile("Müll", t0);
        assert!(t.ergebnis().is_none());

        t.zeile(&format!("%R1P,0,0:0,{HUNDERT_GON_RAD},{HUNDERT_GON_RAD}"), t0);
        let (b, rpc) = t.naechster_befehl().unwrap();
        assert_eq!((b.as_str(), rpc), ("%R1Q,2008,0:1,1", 2008));
        t.zeile("%R1P,0,0:0", t0);
        let (b, rpc) = t.naechster_befehl().unwrap();
        assert_eq!((b.as_str(), rpc), ("%R1Q,2108,0:5000,1", 2108));
        t.zeile(&format!("%R1P,0,0:0,{HUNDERT_GON_RAD},{HUNDERT_GON_RAD},12.345"), t0);
        let e = t.ergebnis().unwrap();
        assert!(e.ok, "{}", e.text);
        assert!(e.text.contains("12,345 m"));
        assert!(e.text.contains("100,0000 gon"));
    }

    #[test]
    fn geocom_fehler_beendet_mit_klartext() {
        let t0 = Instant::now();
        let mut t = Testmessung::neu(Protokoll::GeoCom, t0);
        t.naechster_befehl();
        t.zeile("%R1P,0,0:0,1.0,1.5", t0);
        t.naechster_befehl();
        t.zeile("%R1P,0,0:0", t0);
        t.naechster_befehl();
        t.zeile("%R1P,0,0:1292", t0);
        let e = t.ergebnis().unwrap();
        assert!(!e.ok);
        assert!(e.text.contains("GRC_TMC_DIST_ERROR"), "{}", e.text);
    }

    #[test]
    fn fehler_im_ersten_schritt_beendet_sofort() {
        let t0 = Instant::now();
        let mut t = Testmessung::neu(Protokoll::GeoCom, t0);
        t.naechster_befehl();
        t.zeile("%R1P,0,0:1,0,0", t0);
        let e = t.ergebnis().unwrap();
        assert!(!e.ok);
        assert!(e.text.contains("Winkel lesen fehlgeschlagen"), "{}", e.text);
        assert!(t.naechster_befehl().is_none());
    }

    #[test]
    fn geocom_ohne_strecke_wird_erklaert() {
        let t0 = Instant::now();
        let mut t = Testmessung::neu(Protokoll::GeoCom, t0);
        t.naechster_befehl();
        t.zeile("%R1P,0,0:0,1.0,1.5", t0);
        t.naechster_befehl();
        t.zeile("%R1P,0,0:0", t0);
        t.naechster_befehl();
        t.zeile("%R1P,0,0:0,1.0,1.5", t0);
        assert!(t.ergebnis().unwrap().text.contains("ohne Strecke"));
    }

    #[test]
    fn gsi_online_messung_und_fehler() {
        let t0 = Instant::now();
        let mut t = Testmessung::neu(Protokoll::GsiOnline, t0);
        let (b, _) = t.naechster_befehl().unwrap();
        assert_eq!(b, "GET/M/WI21/WI22/WI31");
        t.zeile("?", t0);
        t.zeile(" ", t0);
        assert!(t.ergebnis().is_none());
        t.zeile("21.022+17920860 22.022+09843660 31..00+00152340", t0);
        assert!(t.ergebnis().unwrap().ok);

        let mut f = Testmessung::neu(Protokoll::GsiOnline, t0);
        f.naechster_befehl();
        f.zeile("@E139", t0);
        let e = f.ergebnis().unwrap();
        assert!(!e.ok && e.text.contains("EDM-Fehler"));
    }

    #[test]
    fn nach_zehn_sekunden_ist_schluss() {
        let t0 = Instant::now();
        let mut t = Testmessung::neu(Protokoll::GeoCom, t0);
        t.naechster_befehl();
        t.pruefe_zeit(t0 + Duration::from_secs(9));
        assert!(t.ergebnis().is_none());
        t.pruefe_zeit(t0 + SCHRITT_TIMEOUT);
        let e = t.ergebnis().unwrap();
        assert!(!e.ok);
        assert!(e.text.contains("10 s") && e.text.contains("GeoCOM"), "{}", e.text);
        // Danach ändert sich nichts mehr
        t.zeile("%R1P,0,0:0,1.0,1.0", t0);
        assert!(!t.ergebnis().unwrap().ok);
    }

    #[test]
    fn jeder_schritt_hat_sein_eigenes_zeitlimit() {
        let t0 = Instant::now();
        let mut t = Testmessung::neu(Protokoll::GeoCom, t0);
        t.naechster_befehl();
        let spaeter = t0 + Duration::from_secs(8);
        t.zeile("%R1P,0,0:0,1.0,1.0", spaeter);
        t.pruefe_zeit(spaeter + Duration::from_secs(9));
        assert!(t.ergebnis().is_none(), "Schritt 2 hat noch Zeit");
    }

    #[test]
    fn andere_geraete_haben_keine_testmessung() {
        let t = Testmessung::neu(Protokoll::Keines, Instant::now());
        assert!(!t.ergebnis().unwrap().ok);
    }
}
