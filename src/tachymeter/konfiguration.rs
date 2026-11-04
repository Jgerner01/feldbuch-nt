// Zweck:    Gemeinsame Messkonfiguration (Prisma/Reflektorlos, EDM-Programm, Prismenkonstante) und Abgleich
//           mit dem Gerät (nur Leica GeoCOM). Jede Anfrage trägt eine Transaktionsnummer 1..7; verspätete
//           Antworten werden verworfen. Meldet das Gerät keine Nummer zurück, wird auf Nummer 0 umgestellt.
// Hardware: Leica-Tachymeter mit GeoCOM
// Autor:    Johann Gerner

use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use super::geocom::{self, Antwort};
use super::befehle::Protokoll;
use super::messung::TachymeterMessung;
use super::verbindung::{Ereignis, Verbindung};
use crate::daten::einstellungen::Einstellungen;

/// Zieltyp Reflektor (IR).
const BAP_REFL_USE: i32 = 0;
/// Zieltyp Reflektorlos (RL).
const BAP_REFL_LESS: i32 = 1;
/// EDM-Programm IR schnell.
const EDM_SINGLE_PRISM: i32 = 3;
/// EDM-Programm RL Standard.
const EDM_SINGLE_RL_K: i32 = 5;
/// EDM-Programm RL Mittelwert.
const EDM_SINGLE_RL_L: i32 = 11;
/// Größte Transaktionsnummer (laut GeoCOM-Handbuch 1..7).
const TRID_MAX: i32 = 7;
/// Wartezeit auf eine Antwort.
const ANTWORT_TIMEOUT_MS: u64 = 4000;
/// Toleranz beim Vergleich der Prismenkonstanten [mm].
const PK_TOLERANZ_MM: f64 = 0.6;
/// Millimeter pro Meter.
const MM_PRO_M: f64 = 1000.0;

/// Messmodus der Distanzmessung.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MessModus {
    Prisma,
    ReflektorlosKurz,
    ReflektorlosLang,
}

impl MessModus {
    pub fn name(self) -> &'static str {
        match self {
            Self::Prisma => "Prisma",
            Self::ReflektorlosKurz => "ReflektorlosKurz",
            Self::ReflektorlosLang => "ReflektorlosLang",
        }
    }

    pub fn aus_name(name: &str) -> Option<Self> {
        [Self::Prisma, Self::ReflektorlosKurz, Self::ReflektorlosLang]
            .into_iter()
            .find(|m| m.name().eq_ignore_ascii_case(name))
    }

    /// GeoCOM-EDM-Programm des Modus.
    pub fn edm_programm(self) -> i32 {
        match self {
            Self::ReflektorlosKurz => EDM_SINGLE_RL_K,
            Self::ReflektorlosLang => EDM_SINGLE_RL_L,
            Self::Prisma => EDM_SINGLE_PRISM,
        }
    }

    /// Nächster Modus im Wechsel Prisma → RL kurz (→ RL lang) → Prisma.
    pub fn weiterschalten(self, drei_stufig: bool) -> Self {
        match self {
            Self::Prisma => Self::ReflektorlosKurz,
            Self::ReflektorlosKurz if drei_stufig => Self::ReflektorlosLang,
            _ => Self::Prisma,
        }
    }
}

/// Ergebnis eines Abgleichs.
#[derive(Clone, Debug, PartialEq)]
pub struct AbgleichErgebnis {
    pub ok: bool,
    pub text: String,
}

impl AbgleichErgebnis {
    fn neu(ok: bool, text: impl Into<String>) -> Self {
        Self { ok, text: text.into() }
    }
}

/// Konfiguration aus den Einstellungen.
#[derive(Clone, Debug, PartialEq)]
pub struct Konfiguration {
    pub modus: MessModus,
    pub prisma_name: String,
    pub prismenkonstante_mm: f64,
}

impl Konfiguration {
    pub fn aus_einstellungen(e: &Einstellungen) -> Self {
        Self {
            modus: MessModus::aus_name(&e.mess_modus_name).unwrap_or(MessModus::Prisma),
            prisma_name: e.prisma_name.clone(),
            prismenkonstante_mm: e.prisma_konstante_mm,
        }
    }

    /// Schreibt die Konfiguration in die Einstellungen (ohne zu speichern).
    pub fn in_einstellungen(&self, e: &mut Einstellungen) {
        e.mess_modus_name = self.modus.name().to_string();
        e.prisma_name = self.prisma_name.clone();
        e.prisma_konstante_mm = self.prismenkonstante_mm;
    }

    pub fn ist_reflektorlos(&self) -> bool {
        self.modus != MessModus::Prisma
    }

    /// Kurzbeschreibung für Tooltips und Meldungen.
    pub fn beschreibung(&self) -> String {
        match self.modus {
            MessModus::Prisma => format!("Prisma ({}, {} mm)", self.prisma_name, vorzeichen_mm(self.prismenkonstante_mm)),
            MessModus::ReflektorlosKurz => "Reflektorlos kurz".to_string(),
            MessModus::ReflektorlosLang => "Reflektorlos lang".to_string(),
        }
    }

    /// Vergleicht die vom Gerät gemeldeten Werte mit der Konfiguration; liefert eine Warnung bei Abweichung.
    pub fn pruefe_messung(&self, protokoll: Protokoll, messung: &TachymeterMessung) -> Option<String> {
        if !protokoll.ist_geocom() {
            return None;
        }
        let mut probleme = Vec::new();
        if let Some(roh) = messung.edm_modus_roh {
            if roh != self.modus.edm_programm() {
                probleme.push(format!("EDM-Modus am Gerät {roh}, eingestellt {}", self.modus.edm_programm()));
            }
        }
        if self.modus == MessModus::Prisma {
            if let Some(pk) = messung.prismenkonstante_mm {
                if (pk - self.prismenkonstante_mm).abs() > PK_TOLERANZ_MM {
                    probleme.push(format!(
                        "Prismenkonstante am Gerät {} mm, eingestellt {} mm",
                        vorzeichen_mm(pk),
                        vorzeichen_mm(self.prismenkonstante_mm)
                    ));
                }
            }
        }
        (!probleme.is_empty()).then(|| format!("Gerät weicht von der Einstellung ab: {}", probleme.join("; ")))
    }
}

/// Wert mit Vorzeichen und einer Nachkommastelle; 0 ohne Vorzeichen.
fn vorzeichen_mm(wert: f64) -> String {
    if wert == 0.0 { "0.0".to_string() } else { format!("{wert:+.1}") }
}

/// Zustand des Abgleichs mit dem Gerät.
pub struct GeraetAbgleich {
    tr_id_echo: bool,
    tr_id_zaehler: i32,
    abgeglichen_fuer: Option<(Konfiguration, u32)>,
}

impl Default for GeraetAbgleich {
    fn default() -> Self {
        Self { tr_id_echo: true, tr_id_zaehler: 0, abgeglichen_fuer: None }
    }
}

impl GeraetAbgleich {
    /// Nach einer neuen Verbindung wieder mit Nummern arbeiten.
    pub fn zuruecksetzen_nach_verbinden(&mut self) {
        self.tr_id_echo = true;
    }

    /// Verwirft den Abgleichsstatus (z. B. nach einer Abweichung).
    pub fn verwerfen(&mut self) {
        self.abgeglichen_fuer = None;
    }

    /// Wurde das Gerät mit dieser Konfiguration auf dieser Verbindung abgeglichen?
    pub fn ist_abgeglichen(&self, konfig: &Konfiguration, verbindung: &Verbindung) -> bool {
        self.abgeglichen_fuer.as_ref().is_some_and(|(k, nr)| k == konfig && *nr == verbindung.verbindungs_nr())
    }

    fn naechste_tr_id(&mut self) -> i32 {
        let id = self.tr_id_zaehler % TRID_MAX + 1;
        self.tr_id_zaehler += 1;
        id
    }

    /// Sendet eine Anfrage und wartet auf die passende Antwort; ohne Antwort einmal ohne Nummer wiederholen.
    fn anfrage(&mut self, v: &mut Verbindung, rx: &Receiver<Ereignis>, rpc: i32, parameter: &str) -> Result<Antwort, String> {
        let tr_id = if self.tr_id_echo { self.naechste_tr_id() } else { 0 };
        match sende_und_warte(v, rx, rpc, parameter, tr_id) {
            Err(e) if e == TIMEOUT_TEXT && self.tr_id_echo => {
                self.tr_id_echo = false;
                sende_und_warte(v, rx, rpc, parameter, 0)
            }
            anderes => anderes,
        }
    }

    /// Überträgt die Konfiguration ans Gerät und liest sie zur Kontrolle zurück (blockierend, in einem Arbeits-Thread aufrufen).
    pub fn abgleichen(
        &mut self,
        v: &mut Verbindung,
        protokoll: Protokoll,
        konfig: &Konfiguration,
        erzwingen: bool,
    ) -> AbgleichErgebnis {
        if !v.ist_verbunden() {
            return AbgleichErgebnis::neu(false, "Kein Tachymeter verbunden.");
        }
        if !protokoll.ist_geocom() {
            return AbgleichErgebnis::neu(true, "Prisma / Reflektorlos bitte am Gerät einstellen (kein GeoCOM).");
        }
        if !erzwingen && self.ist_abgeglichen(konfig, v) {
            return AbgleichErgebnis::neu(true, format!("Gerät abgeglichen: {}", konfig.beschreibung()));
        }
        let rx = v.abonnieren();
        let ergebnis = match self.durchfuehren(v, &rx, konfig) {
            Ok(e) => e,
            Err(e) if e == TIMEOUT_TEXT => AbgleichErgebnis::neu(false, "Abgleich: keine Antwort vom Tachymeter."),
            Err(e) => AbgleichErgebnis::neu(false, format!("Abgleich fehlgeschlagen: {e}")),
        };
        if ergebnis.ok {
            self.abgeglichen_fuer = Some((konfig.clone(), v.verbindungs_nr()));
        }
        ergebnis
    }

    fn durchfuehren(&mut self, v: &mut Verbindung, rx: &Receiver<Ereignis>, konfig: &Konfiguration) -> Result<AbgleichErgebnis, String> {
        let modus = konfig.modus;
        let zieltyp = if modus == MessModus::Prisma { BAP_REFL_USE } else { BAP_REFL_LESS };
        let edm = modus.edm_programm();
        let mut probleme: Vec<String> = Vec::new();

        let a = self.anfrage(v, rx, geocom::RPC_BAP_SET_TARGET_TYPE, &zieltyp.to_string())?;
        if a.rc != 0 {
            probleme.push(format!("Zieltyp setzen (Fehler {})", a.rc));
        }
        let a = self.anfrage(v, rx, geocom::RPC_TMC_SET_EDM_MODE, &edm.to_string())?;
        if a.rc != 0 {
            probleme.push(format!("EDM-Modus setzen (Fehler {})", a.rc));
        }
        if modus == MessModus::Prisma {
            let pk_m = konfig.prismenkonstante_mm / MM_PRO_M;
            let a = self.anfrage(v, rx, geocom::RPC_TMC_SET_PRISM_CORR, &format!("{pk_m:.4}"))?;
            if a.rc != 0 {
                probleme.push(format!("Prismenkonstante setzen (Fehler {})", a.rc));
            }
        }

        // Zur Kontrolle zurücklesen
        let a = self.anfrage(v, rx, geocom::RPC_BAP_GET_TARGET_TYPE, "")?;
        match (a.rc, a.daten.first().and_then(|t| t.parse::<i32>().ok())) {
            (0, Some(gelesen)) if gelesen != zieltyp => probleme.push(format!("Zieltyp am Gerät {gelesen}, eingestellt {zieltyp}")),
            (0, Some(_)) => {}
            _ => probleme.push("Zieltyp nicht lesbar".to_string()),
        }
        let a = self.anfrage(v, rx, geocom::RPC_TMC_GET_EDM_MODE, "")?;
        match (a.rc, a.daten.first().and_then(|t| t.parse::<i32>().ok())) {
            (0, Some(gelesen)) if gelesen != edm => probleme.push(format!("EDM-Modus am Gerät {gelesen}, eingestellt {edm}")),
            (0, Some(_)) => {}
            _ => probleme.push("EDM-Modus nicht lesbar".to_string()),
        }
        if modus == MessModus::Prisma {
            let a = self.anfrage(v, rx, geocom::RPC_TMC_GET_PRISM_CORR, "")?;
            match (a.rc, a.daten.first().and_then(|t| t.parse::<f64>().ok())) {
                (0, Some(m)) if (m * MM_PRO_M - konfig.prismenkonstante_mm).abs() > PK_TOLERANZ_MM => probleme.push(format!(
                    "Prismenkonstante am Gerät {} mm, eingestellt {} mm",
                    vorzeichen_mm(m * MM_PRO_M),
                    vorzeichen_mm(konfig.prismenkonstante_mm)
                )),
                (0, Some(_)) => {}
                _ => probleme.push("Prismenkonstante nicht lesbar".to_string()),
            }
        }

        Ok(if probleme.is_empty() {
            AbgleichErgebnis::neu(true, format!("Gerät abgeglichen: {}", konfig.beschreibung()))
        } else {
            AbgleichErgebnis::neu(false, format!("Abgleich mit Abweichung: {}", probleme.join("; ")))
        })
    }
}

/// Fehlertext bei ausbleibender Antwort.
const TIMEOUT_TEXT: &str = "Zeitüberschreitung";

/// Sendet eine Anfrage und wartet auf die Antwort mit passender Transaktionsnummer (0 = erste Antwort).
fn sende_und_warte(v: &mut Verbindung, rx: &Receiver<Ereignis>, rpc: i32, parameter: &str, tr_id: i32) -> Result<Antwort, String> {
    while rx.try_recv().is_ok() {}
    v.senden(&geocom::anfrage(rpc, tr_id, parameter))?;
    warte_auf_antwort(rx, tr_id, Duration::from_millis(ANTWORT_TIMEOUT_MS))
}

/// Wartet auf eine Antwortzeile; Antworten anderer Transaktionen werden übersprungen.
fn warte_auf_antwort(rx: &Receiver<Ereignis>, tr_id: i32, frist: Duration) -> Result<Antwort, String> {
    let ende = Instant::now() + frist;
    loop {
        let rest = ende.saturating_duration_since(Instant::now());
        match rx.recv_timeout(rest) {
            Ok(Ereignis::Zeile(zeile)) => {
                if let Some(a) = geocom::zerlege_antwort(&zeile) {
                    if tr_id == 0 || a.tr_id == tr_id {
                        return Ok(a);
                    }
                }
            }
            Ok(Ereignis::Getrennt) => return Err("Verbindung zum Tachymeter getrennt.".to_string()),
            Ok(Ereignis::Daten(_) | Ereignis::Bytes { .. }) => {}
            Err(RecvTimeoutError::Timeout) => return Err(TIMEOUT_TEXT.to_string()),
            Err(RecvTimeoutError::Disconnected) => return Err("Verbindung zum Tachymeter getrennt.".to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    fn konfig(modus: MessModus) -> Konfiguration {
        Konfiguration { modus, prisma_name: "GPR1 Standard".into(), prismenkonstante_mm: 0.0 }
    }

    #[test]
    fn modus_wechsel_und_programme() {
        assert_eq!(MessModus::Prisma.weiterschalten(false), MessModus::ReflektorlosKurz);
        assert_eq!(MessModus::ReflektorlosKurz.weiterschalten(false), MessModus::Prisma);
        assert_eq!(MessModus::ReflektorlosKurz.weiterschalten(true), MessModus::ReflektorlosLang);
        assert_eq!(MessModus::ReflektorlosLang.weiterschalten(true), MessModus::Prisma);
        assert_eq!(MessModus::Prisma.edm_programm(), 3);
        assert_eq!(MessModus::ReflektorlosKurz.edm_programm(), 5);
        assert_eq!(MessModus::ReflektorlosLang.edm_programm(), 11);
        assert_eq!(MessModus::aus_name("reflektorloslang"), Some(MessModus::ReflektorlosLang));
        assert_eq!(MessModus::aus_name("x"), None);
    }

    #[test]
    fn beschreibung_mit_vorzeichen() {
        let mut k = konfig(MessModus::Prisma);
        assert_eq!(k.beschreibung(), "Prisma (GPR1 Standard, 0.0 mm)");
        k.prismenkonstante_mm = -17.5;
        assert_eq!(k.beschreibung(), "Prisma (GPR1 Standard, -17.5 mm)");
        assert_eq!(konfig(MessModus::ReflektorlosKurz).beschreibung(), "Reflektorlos kurz");
    }

    #[test]
    fn abweichung_nach_messung_wird_gemeldet() {
        let k = konfig(MessModus::Prisma);
        let mut m = TachymeterMessung::default();
        m.edm_modus_roh = Some(5);
        m.prismenkonstante_mm = Some(-34.0);
        let w = k.pruefe_messung(Protokoll::GeoCom, &m).unwrap();
        assert!(w.contains("EDM-Modus am Gerät 5, eingestellt 3"));
        assert!(w.contains("Prismenkonstante am Gerät -34.0 mm"));
        m.edm_modus_roh = Some(3);
        m.prismenkonstante_mm = Some(0.3);
        assert!(k.pruefe_messung(Protokoll::GeoCom, &m).is_none());
        // Kein GeoCOM: keine Prüfung
        assert!(k.pruefe_messung(Protokoll::GsiOnline, &m).is_none());
    }

    #[test]
    fn transaktionsnummern_laufen_von_1_bis_7() {
        let mut a = GeraetAbgleich::default();
        let ids: Vec<i32> = (0..9).map(|_| a.naechste_tr_id()).collect();
        assert_eq!(ids, vec![1, 2, 3, 4, 5, 6, 7, 1, 2]);
    }

    #[test]
    fn antwort_mit_falscher_nummer_wird_uebersprungen() {
        let (tx, rx) = mpsc::channel();
        tx.send(Ereignis::Zeile("%R1P,0,2:0,alt".into())).unwrap();
        tx.send(Ereignis::Daten("roh".into())).unwrap();
        tx.send(Ereignis::Zeile("%R1P,0,3:0,richtig".into())).unwrap();
        let a = warte_auf_antwort(&rx, 3, Duration::from_millis(200)).unwrap();
        assert_eq!(a.daten, vec!["richtig"]);
    }

    #[test]
    fn nummer_null_nimmt_die_erste_antwort() {
        let (tx, rx) = mpsc::channel();
        tx.send(Ereignis::Zeile("%R1P,0,5:0,x".into())).unwrap();
        assert_eq!(warte_auf_antwort(&rx, 0, Duration::from_millis(200)).unwrap().tr_id, 5);
    }

    #[test]
    fn timeout_und_trennung() {
        let (tx, rx) = mpsc::channel::<Ereignis>();
        assert_eq!(warte_auf_antwort(&rx, 1, Duration::from_millis(50)).unwrap_err(), TIMEOUT_TEXT);
        tx.send(Ereignis::Getrennt).unwrap();
        assert!(warte_auf_antwort(&rx, 1, Duration::from_millis(50)).unwrap_err().contains("getrennt"));
    }

    #[test]
    fn abgleich_ohne_verbindung_und_ohne_geocom() {
        let mut v = Verbindung::default();
        let mut a = GeraetAbgleich::default();
        let k = konfig(MessModus::Prisma);
        assert!(!a.abgleichen(&mut v, Protokoll::GeoCom, &k, false).ok);
        assert_eq!(a.abgleichen(&mut v, Protokoll::GeoCom, &k, false).text, "Kein Tachymeter verbunden.");
    }
}
