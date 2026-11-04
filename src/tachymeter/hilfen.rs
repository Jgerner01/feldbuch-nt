// Zweck:    Hilfsbausteine der Tachymeter-Anbindung: Quittungstöne, Sperre der Live-Winkelabfrage,
//           Messungscache (letzte Voll- und Winkelmessung)
// Hardware: Leica GeoCOM (Quittungstöne), alle Geräte (Cache)
// Autor:    Johann Gerner

use std::sync::atomic::{AtomicI32, AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use super::befehle::{DatenParser, Protokoll};
use super::geocom;
use super::messung::TachymeterMessung;
use super::verbindung::Verbindung;
use crate::grundlagen::fehler_log;

// ── Quittungstöne ─────────────────────────────────────────────────────────────

/// Ein Piepton: Messung erfolgreich (nur GeoCOM, sonst ohne Wirkung).
pub fn signal_erfolg(v: &mut Verbindung, protokoll: Protokoll) {
    sende_signal(v, protokoll, geocom::RPC_BMM_BEEP_NORMAL);
}

/// Dreifacher Piepton (Alarm): Messung fehlgeschlagen (nur GeoCOM).
pub fn signal_fehler(v: &mut Verbindung, protokoll: Protokoll) {
    sende_signal(v, protokoll, geocom::RPC_BMM_BEEP_ALARM);
}

fn sende_signal(v: &mut Verbindung, protokoll: Protokoll, rpc: i32) {
    if !v.ist_verbunden() || !protokoll.ist_geocom() {
        return;
    }
    // Ein fehlender Quittungston darf den Messablauf nicht stören
    if let Err(e) = v.senden(&format!("%R1Q,{rpc},0:")) {
        fehler_log::log("TachymeterSignal.senden", &e);
    }
}

// ── Sperre der Live-Winkelabfrage ─────────────────────────────────────────────

/// Nach dieser Zeit gilt eine unbeantwortete Winkelabfrage als verloren [ms].
pub const WINKEL_ANTWORT_TIMEOUT_MS: u64 = 1500;

static SPERREN: AtomicI32 = AtomicI32::new(0);
/// Zeitpunkt der letzten Winkelabfrage in ms seit Unix-Epoche; 0 = keine offen.
static LETZTE_ANFRAGE_MS: AtomicU64 = AtomicU64::new(0);

fn jetzt_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

/// Koordiniert die Dauerabfrage des Horizontalwinkels mit Fenstern, die selbst am Tachymeter messen.
pub struct LiveWinkelSperre;

impl LiveWinkelSperre {
    /// Sperrt die Winkelabfrage (ein messendes Fenster ist geöffnet).
    pub fn sperren() {
        SPERREN.fetch_add(1, Ordering::SeqCst);
    }

    /// Hebt eine Sperre auf; der Zähler fällt nie unter 0.
    pub fn freigeben() {
        if SPERREN.fetch_sub(1, Ordering::SeqCst) <= 0 {
            SPERREN.store(0, Ordering::SeqCst);
        }
    }

    /// Ist die Winkelabfrage gesperrt?
    pub fn gesperrt() -> bool {
        SPERREN.load(Ordering::SeqCst) > 0
    }

    /// Merkt eine soeben gesendete Winkelabfrage.
    pub fn anfrage_gesendet() {
        LETZTE_ANFRAGE_MS.store(jetzt_ms(), Ordering::SeqCst);
    }

    /// Die Antwort ist eingetroffen oder die Abfrage wurde verworfen.
    pub fn anfrage_beendet() {
        LETZTE_ANFRAGE_MS.store(0, Ordering::SeqCst);
    }

    /// Steht die Antwort auf eine gesendete Winkelabfrage noch aus?
    pub fn antwort_offen() -> bool {
        let t = LETZTE_ANFRAGE_MS.load(Ordering::SeqCst);
        t != 0 && jetzt_ms().saturating_sub(t) < WINKEL_ANTWORT_TIMEOUT_MS
    }
}

// ── Messungscache ─────────────────────────────────────────────────────────────

/// Hält die letzte Voll- und Winkelmessung vor.
#[derive(Default)]
pub struct MessungsCache {
    pub letzte_vollmessung: Option<TachymeterMessung>,
    pub letzte_winkelmessung: Option<TachymeterMessung>,
}

/// Art einer neu eingetroffenen Messung.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Neu {
    Vollmessung,
    Winkelmessung,
}

impl MessungsCache {
    /// Verarbeitet eine eingegangene Zeile und meldet, ob eine neue Voll- oder Winkelmessung entstand.
    pub fn verarbeite_zeile(&mut self, parser: &DatenParser, zeile: &str) -> Option<Neu> {
        let m = parser.parse_zeile(zeile)?;
        if m.ist_vollmessung() {
            self.letzte_vollmessung = Some(m.clone());
            self.letzte_winkelmessung = Some(m);
            Some(Neu::Vollmessung)
        } else if m.hat_winkel() {
            self.letzte_winkelmessung = Some(m);
            Some(Neu::Winkelmessung)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_unterscheidet_voll_und_winkel() {
        let mut c = MessungsCache::default();
        let mut p = DatenParser::fuer(Protokoll::GeoCom);
        p.setze_letzten_rpc(geocom::RPC_TMC_GET_ANGLE);
        assert_eq!(c.verarbeite_zeile(&p, "%R1P,0,0:0,1.0,1.5"), Some(Neu::Winkelmessung));
        assert!(c.letzte_vollmessung.is_none());
        p.setze_letzten_rpc(geocom::RPC_TMC_GET_SIMPLE_MEA);
        assert_eq!(c.verarbeite_zeile(&p, "%R1P,0,0:0,1.0,1.5,20.0"), Some(Neu::Vollmessung));
        assert!(c.letzte_vollmessung.is_some());
        // Status ohne Winkel ändert nichts
        p.setze_letzten_rpc(geocom::RPC_COM_NULL_PROC);
        assert_eq!(c.verarbeite_zeile(&p, "%R1P,0,0:0"), None);
        assert_eq!(c.verarbeite_zeile(&p, "Müll"), None);
    }

    #[test]
    fn live_winkel_sperre_zaehlt_und_faellt_nicht_unter_null() {
        // Der Zustand ist global; der Test prüft nur relative Änderungen.
        LiveWinkelSperre::freigeben();
        LiveWinkelSperre::freigeben();
        assert!(!LiveWinkelSperre::gesperrt());
        LiveWinkelSperre::sperren();
        assert!(LiveWinkelSperre::gesperrt());
        LiveWinkelSperre::freigeben();
        assert!(!LiveWinkelSperre::gesperrt());
    }

    #[test]
    fn offene_winkelantwort_laeuft_ab() {
        LiveWinkelSperre::anfrage_beendet();
        assert!(!LiveWinkelSperre::antwort_offen());
        LiveWinkelSperre::anfrage_gesendet();
        assert!(LiveWinkelSperre::antwort_offen());
        LiveWinkelSperre::anfrage_beendet();
        assert!(!LiveWinkelSperre::antwort_offen());
    }

    #[test]
    fn signale_ohne_verbindung_sind_wirkungslos() {
        let mut v = Verbindung::default();
        signal_erfolg(&mut v, Protokoll::GeoCom);
        signal_fehler(&mut v, Protokoll::GsiOnline);
    }
}
