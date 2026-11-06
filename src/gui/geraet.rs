// Zweck:    Gemeinsamer Gerätezustand der Oberfläche: Verbindung (mit Zeitlimit im Hintergrund), Modell, Sprache
//           (GeoCOM/GSI Online), Parser, Messungscache, Empfangsprotokoll und Testmessung
// Hardware: Leica TS06 / TPS300 / TPS1200, Topcon, GNSS über serielle Schnittstelle (Bluetooth-SPP)
// Autor:    Johann Gerner

use std::collections::VecDeque;
use std::sync::mpsc::Receiver;
use std::time::Instant;

use crate::daten::einstellungen::Einstellungen;
use crate::grundlagen::{kommunikationslog, protokoll};
use crate::tachymeter::anfahren::{AnfahrErgebnis, Anfahren};
use crate::tachymeter::befehle::{Befehlsgeber, DatenParser, Protokoll};
use crate::tachymeter::geocom;
use crate::tachymeter::gsi;
use crate::tachymeter::hilfen::{LiveWinkelSperre, MessungsCache, signal_erfolg, signal_fehler};
use crate::tachymeter::konfiguration::MessModus;
use crate::tachymeter::messablauf::{MessErgebnis, Messablauf};
use crate::tachymeter::modell::{Sprache, TachymeterModell};
use crate::tachymeter::schalter::Schalter;
use crate::tachymeter::testmessung::{TestErgebnis, Testmessung};

/// Was ein laufender Schaltvorgang bei Erfolg ändert.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SchalterZiel {
    Reflektorlos(bool),
    Laser(bool),
}
use crate::tachymeter::verbindung::{Ereignis, SeriellEinstellung, Verbindung, hex, sichtbar};

/// Höchstzahl der Zeilen im Empfangsprotokoll.
const MAX_PROTOKOLL_ZEILEN: usize = 500;
/// Abstand der Hz-Abfragen für die Zielrichtung.
const LIVE_INTERVALL: std::time::Duration = std::time::Duration::from_millis(500);
/// Nach dieser Zeit gilt eine unbeantwortete Hz-Abfrage als verloren.
const LIVE_ANTWORT_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(crate::tachymeter::hilfen::WINKEL_ANTWORT_TIMEOUT_MS);

/// Abstand der Neigungsabfragen für die Libelle.
const NEIGUNG_INTERVALL: std::time::Duration = std::time::Duration::from_millis(400);
/// Nach dieser Zeit ohne neue Neigung gilt die Anzeige als veraltet.
const NEIGUNG_VERALTET: std::time::Duration = std::time::Duration::from_millis(3000);

/// Zustand des Messgeräts.
pub struct Geraet {
    pub verbindung: Verbindung,
    pub modell: TachymeterModell,
    pub sprache: Sprache,
    pub einstellung: SeriellEinstellung,
    pub parser: DatenParser,
    pub cache: MessungsCache,
    /// Empfangsprotokoll (neueste Zeile zuletzt).
    pub protokoll: VecDeque<String>,
    /// Gesendete und empfangene Bytes zusätzlich als Hex anzeigen.
    pub hex_anzeige: bool,
    /// Fehler eines Verbindungsversuchs; wird von der Oberfläche abgeholt und angezeigt.
    pub verbinde_fehler: Option<String>,
    /// Laufende Testmessung.
    pub test: Option<Testmessung>,
    /// Ergebnis der letzten Testmessung.
    pub test_ergebnis: Option<TestErgebnis>,
    /// Ziel ist reflektorlos (sonst Reflektor/Prisma).
    pub reflektorlos: bool,
    /// Laserpointer ist (laut letztem Schaltvorgang) an.
    pub laser_an: bool,
    /// Laufender Schaltvorgang (Reflektor/Reflektorlos oder Laserpointer).
    pub schalter: Option<Schalter>,
    /// Ergebnis des letzten Schaltvorgangs.
    pub schalter_ergebnis: Option<TestErgebnis>,
    schalter_ziel: Option<SchalterZiel>,
    /// Laufende Punktmessung.
    pub messung: Option<Messablauf>,
    /// Abgeschlossene Messung, bis der Viewer sie abholt.
    messung_ergebnis: Option<MessErgebnis>,
    /// Messung soll beginnen, sobald eine offene Hz-Abfrage beantwortet ist (Zielhöhe).
    messung_ausstehend: Option<f64>,
    /// Der Viewer möchte die Zielrichtung (laufende Hz-Abfrage) sehen.
    pub live_hz_aktiv: bool,
    /// Zuletzt gemessene Horizontalrichtung [gon].
    pub live_hz: Option<f64>,
    /// Zuletzt gemessener Vertikalwinkel [gon] (nur wenn `live_v_aktiv`, bei GeoCOM immer).
    pub live_v: Option<f64>,
    /// Zusätzlich zu Hz auch V abfragen (Messdaten-Dialog).
    pub live_v_aktiv: bool,
    live_wartet: bool,
    live_gesendet: Instant,
    live_naechste: Instant,
    /// Die Libelle ist sichtbar und möchte die Kompensatorneigung sehen.
    pub live_neigung_aktiv: bool,
    /// Zuletzt gelesene Neigung (Quer, Längs) [rad] und Zeitpunkt; `None` = Gerät liefert keine Neigung.
    neigung: Option<((f64, f64), Instant)>,
    neigung_wartet: bool,
    neigung_gesendet: Instant,
    neigung_naechste: Instant,
    /// Laufendes motorisches Anfahren.
    pub anfahren: Option<Anfahren>,
    /// Ergebnis des letzten Anfahrens, bis die Oberfläche es abholt.
    anfahren_ergebnis: Option<AnfahrErgebnis>,
    /// Zuletzt aus einer Antwortzeile gelesene Messung (für das Testfenster).
    pub letzte_messung: Option<crate::tachymeter::messung::TachymeterMessung>,
    /// Zähler, der bei jeder neu gelesenen Messung steigt.
    pub messung_nr: u64,
    empfaenger: Receiver<Ereignis>,
}

impl Geraet {
    /// Übernimmt Modell, Sprache und Schnittstelle aus den Einstellungen (ohne zu verbinden).
    pub fn aus_einstellungen(e: &Einstellungen) -> Self {
        let verbindung = Verbindung::default();
        let empfaenger = verbindung.abonnieren();
        let sprache = Sprache::aus_name(&e.tachymeter_sprache).unwrap_or_else(|| e.tachymeter_modell.standard_sprache());
        let mut g = Self {
            verbindung,
            modell: e.tachymeter_modell,
            sprache,
            einstellung: SeriellEinstellung::aus_einstellungen(e),
            parser: DatenParser::fuer(Protokoll::Keines),
            cache: MessungsCache::default(),
            protokoll: VecDeque::new(),
            hex_anzeige: false,
            verbinde_fehler: None,
            test: None,
            test_ergebnis: None,
            reflektorlos: e.mess_modus_name != MessModus::Prisma.name(),
            laser_an: false,
            schalter: None,
            schalter_ergebnis: None,
            schalter_ziel: None,
            messung: None,
            messung_ergebnis: None,
            messung_ausstehend: None,
            live_hz_aktiv: false,
            live_hz: None,
            live_v: None,
            live_v_aktiv: false,
            live_wartet: false,
            live_gesendet: Instant::now(),
            live_naechste: Instant::now(),
            live_neigung_aktiv: false,
            neigung: None,
            neigung_wartet: false,
            neigung_gesendet: Instant::now(),
            neigung_naechste: Instant::now(),
            anfahren: None,
            anfahren_ergebnis: None,
            letzte_messung: None,
            messung_nr: 0,
            empfaenger,
        };
        g.parser = DatenParser::fuer(g.protokoll());
        g.logge(&format!(
            "=== Programmstart: Modell {}, Sprache {}, {} ===",
            g.modell.name(),
            g.sprache.name(),
            g.einstellung.kurz()
        ));
        g
    }

    /// Schreibt eine Zeile nur in die Protokolldatei.
    fn logge(&self, zeile: &str) {
        kommunikationslog::schreibe(zeile);
    }

    /// Schaltet zwischen Reflektor und Reflektorlos um (Prisma ↔ Reflektorlos kurz).
    pub fn starte_ziel_umschaltung(&mut self) -> Result<(), String> {
        let modus = if self.reflektorlos { MessModus::Prisma } else { MessModus::ReflektorlosKurz };
        self.starte_modus_umschaltung(modus)
    }

    /// Stellt den EDM-Modus (Prisma, Reflektorlos kurz oder lang) am Gerät ein.
    pub fn starte_modus_umschaltung(&mut self, modus: MessModus) -> Result<(), String> {
        if !self.ist_verbunden() {
            return Err("Nicht verbunden – bitte zuerst verbinden.".to_string());
        }
        let neu_reflektorlos = modus != MessModus::Prisma;
        let ziel_typ = i32::from(neu_reflektorlos);
        let bg = Befehlsgeber::fuer(self.protokoll());
        let befehle: Vec<(String, i32)> = bg
            .edm_modus_befehle(ziel_typ, modus.edm_programm())
            .unwrap_or_default()
            .into_iter()
            .map(|b| {
                let rpc = geocom::rpc_aus_anfrage(&b);
                (b, rpc)
            })
            .collect();
        let name = match modus {
            MessModus::Prisma => "Reflektor",
            MessModus::ReflektorlosKurz => "Reflektorlos kurz",
            MessModus::ReflektorlosLang => "Reflektorlos lang",
        };
        self.starte_schalter(
            &format!("Umschalten auf {name}"),
            befehle,
            &format!("Ziel {name} eingestellt."),
            SchalterZiel::Reflektorlos(neu_reflektorlos),
        )
    }

    /// Schaltet den Laserpointer ein oder aus.
    pub fn starte_laser_umschaltung(&mut self) -> Result<(), String> {
        if !self.ist_verbunden() {
            return Err("Nicht verbunden – bitte zuerst verbinden.".to_string());
        }
        let an = !self.laser_an;
        let befehle: Vec<(String, i32)> = Befehlsgeber::fuer(self.protokoll())
            .laser_befehl(an)
            .map(|b| {
                let rpc = geocom::rpc_aus_anfrage(&b);
                (b, rpc)
            })
            .into_iter()
            .collect();
        self.starte_schalter(
            if an { "Laserpointer einschalten" } else { "Laserpointer ausschalten" },
            befehle,
            if an { "Laserpointer eingeschaltet." } else { "Laserpointer ausgeschaltet." },
            SchalterZiel::Laser(an),
        )
    }

    /// Läuft gerade eine Testmessung, ein Schaltvorgang oder eine Punktmessung?
    pub fn beschaeftigt(&self) -> bool {
        self.test.is_some()
            || self.schalter.is_some()
            || self.messung.is_some()
            || self.messung_ausstehend.is_some()
            || self.anfahren.is_some()
    }

    /// Startet eine Punktmessung (im Viewer). Beginnt nach einer offenen Hz-Abfrage, damit keine Antworten vermischt werden.
    pub fn starte_messung(&mut self, zielhoehe: f64) -> Result<(), String> {
        if !self.ist_verbunden() {
            return Err("Kein Tachymeter verbunden.".to_string());
        }
        if self.beschaeftigt() {
            return Err("Es läuft noch ein anderer Vorgang.".to_string());
        }
        self.messung_ergebnis = None;
        self.messung_ausstehend = Some(zielhoehe);
        Ok(())
    }

    /// Startet das motorische Anfahren der Richtung (Hz, V) in gon.
    pub fn starte_anfahren(&mut self, hz_gon: f64, v_gon: f64) -> Result<(), String> {
        if !self.ist_verbunden() {
            return Err("Kein Tachymeter verbunden.".to_string());
        }
        if self.beschaeftigt() {
            return Err("Es läuft noch ein anderer Vorgang.".to_string());
        }
        self.anfahren_ergebnis = None;
        self.live_wartet = false;
        LiveWinkelSperre::anfrage_beendet();
        self.schreibe(format!("— Anfahren: Hz={hz_gon:.4} gon  V={v_gon:.4} gon —"));
        self.anfahren = Some(Anfahren::neu(self.protokoll(), hz_gon, v_gon, Instant::now()));
        Ok(())
    }

    /// Bricht das Anfahren in der Oberfläche ab (das Gerät fährt ggf. noch weiter).
    pub fn stoppe_anfahren(&mut self) {
        if self.anfahren.take().is_some() {
            self.schreibe("— Anfahren abgebrochen (Gerät fährt ggf. noch) —".to_string());
        }
    }

    /// Statustext des laufenden Anfahrens.
    pub fn anfahren_status(&self) -> Option<&str> {
        self.anfahren.as_ref().map(Anfahren::status)
    }

    /// Holt das Ergebnis des Anfahrens ab.
    pub fn nimm_anfahr_ergebnis(&mut self) -> Option<AnfahrErgebnis> {
        self.anfahren_ergebnis.take()
    }

    /// Text zum laufenden Messvorgang für die Statuszeile.
    pub fn messung_status(&self) -> Option<&'static str> {
        if self.messung_ausstehend.is_some() {
            Some("Messung startet …")
        } else {
            self.messung.as_ref().map(Messablauf::status_text)
        }
    }

    /// Holt das Ergebnis einer abgeschlossenen Punktmessung ab.
    pub fn nimm_messergebnis(&mut self) -> Option<MessErgebnis> {
        self.messung_ergebnis.take()
    }

    fn starte_schalter(&mut self, beschreibung: &str, befehle: Vec<(String, i32)>, erfolg: &str, ziel: SchalterZiel) -> Result<(), String> {
        if self.beschaeftigt() {
            return Err("Es läuft noch ein anderer Vorgang.".to_string());
        }
        self.schalter_ergebnis = None;
        self.schreibe(format!("— {beschreibung} ({}) —", self.protokoll_name()));
        self.schalter = Some(Schalter::neu(beschreibung, self.protokoll(), befehle, erfolg, Instant::now()));
        self.schalter_ziel = Some(ziel);
        Ok(())
    }

    /// Das aktuell verwendete Protokoll.
    pub fn protokoll(&self) -> Protokoll {
        Protokoll::fuer(self.modell, self.sprache)
    }

    /// Schreibt Modell und Sprache in die Einstellungen (ohne zu speichern).
    pub fn in_einstellungen(&self, e: &mut Einstellungen) {
        self.einstellung.in_einstellungen(self.modell, e);
        e.tachymeter_sprache = if self.modell.sprache_waehlbar() { self.sprache.name().to_string() } else { String::new() };
    }

    /// Setzt das Modell (mit seiner Standardsprache) und passt den Parser an.
    pub fn setze_modell(&mut self, modell: TachymeterModell) {
        self.modell = modell;
        self.sprache = modell.standard_sprache();
        self.parser = DatenParser::fuer(self.protokoll());
    }

    /// Schaltet zwischen GeoCOM und GSI Online um.
    pub fn setze_sprache(&mut self, sprache: Sprache) {
        self.sprache = sprache;
        self.parser = DatenParser::fuer(self.protokoll());
    }

    /// Startet das Verbinden im Hintergrund (höchstens 10 s); das Ergebnis meldet `aktualisiere`.
    pub fn starte_verbinden(&mut self) -> Result<(), String> {
        let einstellung = self.einstellung.clone();
        self.verbindung.starte_verbinden(&einstellung)?;
        self.schreibe(format!(
            "Verbinde {} ({}  DTR={} RTS={}  Zeilenende={}  Modell={}  Sprache={}) …",
            einstellung.port,
            einstellung.kurz(),
            u8::from(einstellung.dtr),
            u8::from(einstellung.rts),
            einstellung.zeilenende.name(),
            self.modell.name(),
            self.sprache.name()
        ));
        Ok(())
    }

    /// Bricht einen laufenden Verbindungsversuch ab.
    pub fn abbrechen_verbinden(&mut self) {
        self.verbindung.abbrechen_verbinden();
        self.schreibe("Verbindungsversuch abgebrochen.".to_string());
    }

    /// Trennt die Verbindung (ein eingeschalteter Laserpointer wird vorher ausgeschaltet).
    pub fn trennen(&mut self) {
        if self.laser_an && self.ist_verbunden() {
            if let Some(b) = Befehlsgeber::fuer(self.protokoll()).laser_befehl(false) {
                let _ = self.verbindung.senden(&b);
                self.schreibe(format!(">> {b}   (Laserpointer vor dem Trennen aus)"));
                std::thread::sleep(std::time::Duration::from_millis(150));
            }
        }
        self.laser_an = false;
        self.test = None;
        self.schalter = None;
        self.schalter_ziel = None;
        self.verbindung.trennen();
    }

    /// Sendet einen Befehl und merkt sich den RPC als Kontext für die Antwort.
    pub fn sende(&mut self, befehl: &str) -> Result<(), String> {
        let rpc = geocom::rpc_aus_anfrage(befehl);
        if rpc != 0 {
            self.parser.setze_letzten_rpc(rpc);
        }
        self.verbindung.senden(befehl)?;
        self.schreibe(format!(">> {}", if befehl.is_empty() { "<Zeilenende>" } else { befehl }));
        Ok(())
    }

    /// Startet die Testmessung für das aktuelle Protokoll.
    pub fn starte_testmessung(&mut self) -> Result<(), String> {
        if !self.ist_verbunden() {
            return Err("Nicht verbunden – bitte zuerst verbinden.".to_string());
        }
        if self.beschaeftigt() {
            return Err("Es läuft noch ein anderer Vorgang.".to_string());
        }
        self.test_ergebnis = None;
        self.schreibe(format!("— Testmessung ({}) —", self.protokoll_name()));
        self.test = Some(Testmessung::neu(self.protokoll(), Instant::now()));
        Ok(())
    }

    /// Name des aktuellen Protokolls für Anzeigen.
    pub fn protokoll_name(&self) -> &'static str {
        match self.protokoll() {
            Protokoll::GeoCom => "GeoCOM",
            Protokoll::GsiOnline => "GSI Online",
            Protokoll::Topcon => "Topcon",
            Protokoll::Keines => "kein Protokoll",
        }
    }

    /// Verarbeitet Verbindungsversuch, eingegangene Ereignisse und die Testmessung (einmal je Bild aufrufen).
    pub fn aktualisiere(&mut self) {
        match self.verbindung.pruefe_verbinden() {
            Some(Ok(())) => {
                let e = self.einstellung.clone();
                self.schreibe(format!("Verbunden: {}  ({})", e.port, e.kurz()));
                protokoll::log("TACHY", &format!("Verbunden: {}  ({} Baud)", e.port, e.baud));
            }
            Some(Err(fehler)) => {
                self.schreibe(format!("Verbinden fehlgeschlagen: {fehler}"));
                self.verbinde_fehler = Some(format!(
                    "Tachymeter ({}) konnte nicht verbunden werden:\n{fehler}",
                    self.einstellung.port
                ));
            }
            None => {}
        }

        let jetzt = Instant::now();
        while let Ok(ereignis) = self.empfaenger.try_recv() {
            match ereignis {
                Ereignis::Zeile(zeile) => {
                    // Antwort auf die Hz-Abfrage der Zielrichtung: still verarbeiten (kein Protokoll-Rauschen)
                    if self.live_wartet && !self.beschaeftigt() {
                        if let Some((hz, v)) = self.live_winkel_aus(&zeile) {
                            self.live_hz = Some(hz);
                            self.live_v = v;
                            self.live_wartet = false;
                            LiveWinkelSperre::anfrage_beendet();
                            continue;
                        }
                    }
                    // Antwort auf die Neigungsabfrage der Libelle: still verarbeiten
                    if self.neigung_wartet && !self.beschaeftigt() {
                        if let Some(m) = geocom::parse_antwort(&zeile, geocom::RPC_TMC_GET_ANGLE1) {
                            self.neigung = match (m.kreuz_neigung_rad, m.laengs_neigung_rad) {
                                (Some(q), Some(l)) if m.typ != crate::tachymeter::messung::MessungsTyp::Fehler => Some(((q, l), jetzt)),
                                _ => None,
                            };
                            self.neigung_wartet = false;
                            LiveWinkelSperre::anfrage_beendet();
                            continue;
                        }
                    }
                    self.schreibe(format!("<< {zeile}"));
                    if let Some(m) = self.parser.parse_zeile(&zeile) {
                        self.schreibe(format!("   {}", m.text()));
                        self.letzte_messung = Some(m);
                        self.messung_nr += 1;
                    }
                    self.cache.verarbeite_zeile(&self.parser, &zeile);
                    if let Some(t) = self.test.as_mut() {
                        t.zeile(&zeile, jetzt);
                    }
                    if let Some(s) = self.schalter.as_mut() {
                        s.zeile(&zeile, jetzt);
                    }
                    if let Some(m) = self.messung.as_mut() {
                        m.zeile(&zeile, jetzt);
                    }
                    if let Some(a) = self.anfahren.as_mut() {
                        a.zeile(&zeile, jetzt);
                    }
                }
                Ereignis::Getrennt => {
                    self.schreibe("Verbindung getrennt.".to_string());
                    self.laser_an = false;
                    self.live_wartet = false;
                    self.live_hz = None;
                    self.live_v = None;
                    self.neigung = None;
                    self.neigung_wartet = false;
                    LiveWinkelSperre::anfrage_beendet();
                    if self.messung.take().is_some() || self.messung_ausstehend.take().is_some() {
                        self.messung_ergebnis = Some(MessErgebnis::Fehler("Verbindung während der Messung getrennt.".to_string()));
                    }
                    if self.anfahren.take().is_some() {
                        self.anfahren_ergebnis = Some(AnfahrErgebnis { ok: false, text: "Verbindung während des Anfahrens getrennt.".into() });
                    }
                    if self.test.take().is_some() {
                        self.test_ergebnis = Some(TestErgebnis { ok: false, text: "Verbindung während der Testmessung getrennt.".into() });
                    }
                    if self.schalter.take().is_some() {
                        self.schalter_ziel = None;
                        self.schalter_ergebnis = Some(TestErgebnis { ok: false, text: "Verbindung während des Schaltvorgangs getrennt.".into() });
                    }
                }
                Ereignis::Bytes { gesendet, daten } => {
                    // Die Protokolldatei enthält immer die Rohbytes, die Anzeige nur auf Wunsch
                    let richtung = if gesendet { "TX" } else { "RX" };
                    let zeile = format!("{richtung} {}   |{}|", hex(&daten), sichtbar(&daten));
                    if (self.live_wartet || self.neigung_wartet) && !self.beschaeftigt() {
                        // Hz- bzw. Neigungsabfrage: nicht protokollieren
                    } else if self.hex_anzeige {
                        self.schreibe(zeile);
                    } else {
                        self.logge(&zeile);
                    }
                }
                Ereignis::Daten(_) => {}
            }
        }
        self.treibe_testmessung(jetzt);
        self.treibe_schalter(jetzt);
        self.treibe_messung(jetzt);
        self.treibe_anfahren(jetzt);
        self.treibe_live(jetzt);
        self.treibe_neigung(jetzt);
    }

    /// Aktuelle Kompensatorneigung (Quer, Längs) [rad]; `None`, wenn das Gerät keine liefert oder die Werte veraltet sind.
    pub fn neigung(&self) -> Option<(f64, f64)> {
        self.neigung
            .filter(|(_, zeit)| zeit.elapsed() < NEIGUNG_VERALTET)
            .map(|(werte, _)| werte)
    }

    /// Fragt die Neigung zyklisch ab (nur GeoCOM, nur solange die Libelle sichtbar ist).
    fn treibe_neigung(&mut self, jetzt: Instant) {
        if self.neigung_wartet && jetzt.saturating_duration_since(self.neigung_gesendet) >= LIVE_ANTWORT_TIMEOUT {
            self.neigung_wartet = false;
            self.neigung = None;
            LiveWinkelSperre::anfrage_beendet();
        }
        if !self.live_neigung_aktiv || !self.ist_verbunden() || self.protokoll() != Protokoll::GeoCom {
            self.neigung = None;
            return;
        }
        if self.neigung_wartet
            || self.live_wartet
            || self.beschaeftigt()
            || LiveWinkelSperre::gesperrt()
            || jetzt < self.neigung_naechste
            || LiveWinkelSperre::antwort_offen()
        {
            return;
        }
        self.neigung_wartet = true;
        self.neigung_gesendet = jetzt;
        self.neigung_naechste = jetzt + NEIGUNG_INTERVALL;
        LiveWinkelSperre::anfrage_gesendet();
        if self.verbindung.senden(crate::tachymeter::messablauf::BEFEHL_KOMPENSATOR).is_err() {
            self.neigung_wartet = false;
            LiveWinkelSperre::anfrage_beendet();
        }
    }

    /// Sendet anstehende Befehle des Anfahrens und übernimmt das Ergebnis.
    fn treibe_anfahren(&mut self, jetzt: Instant) {
        let Some(mut a) = self.anfahren.take() else {
            return;
        };
        a.pruefe_zeit(jetzt);
        if let Some((befehl, rpc)) = a.naechster_befehl() {
            self.parser.setze_letzten_rpc(rpc);
            match self.verbindung.senden(&befehl) {
                Ok(()) => self.schreibe(format!(">> {befehl}")),
                Err(e) => {
                    self.anfahren_ergebnis = Some(AnfahrErgebnis { ok: false, text: format!("Senden fehlgeschlagen: {e}") });
                    return;
                }
            }
        }
        match a.ergebnis() {
            Some(e) => {
                self.schreibe(format!("— {} —", e.text));
                self.anfahren_ergebnis = Some(e.clone());
            }
            None => self.anfahren = Some(a),
        }
    }

    /// Horizontalrichtung aus einer Antwortzeile der Live-Abfrage.
    fn live_winkel_aus(&self, zeile: &str) -> Option<(f64, Option<f64>)> {
        let m = match self.protokoll() {
            Protokoll::GeoCom => geocom::parse_antwort(zeile, geocom::RPC_TMC_GET_ANGLE)?,
            Protokoll::GsiOnline => gsi::parse_online_zeile(zeile)?,
            _ => return None,
        };
        Some((m.hz_gon?, m.v_gon))
    }

    /// Startet eine ausstehende Messung, treibt die laufende voran und übergibt das Ergebnis.
    fn treibe_messung(&mut self, jetzt: Instant) {
        if let Some(zielhoehe) = self.messung_ausstehend {
            // Erst nach der Antwort auf eine offene Hz-Abfrage, damit keine Antworten vermischt werden
            if self.live_wartet && jetzt.saturating_duration_since(self.live_gesendet) < LIVE_ANTWORT_TIMEOUT {
                return;
            }
            self.live_wartet = false;
            LiveWinkelSperre::anfrage_beendet();
            self.messung_ausstehend = None;
            self.schreibe(format!("— Messung ({}) —", self.protokoll_name()));
            self.messung = Some(Messablauf::neu(self.protokoll(), zielhoehe, jetzt));
        }
        let Some(mut m) = self.messung.take() else {
            return;
        };
        m.pruefe_zeit(jetzt);
        if let Some((befehl, rpc)) = m.naechster_befehl() {
            if rpc != 0 {
                self.parser.setze_letzten_rpc(rpc);
            }
            if let Err(e) = self.verbindung.senden(&befehl) {
                self.messung_ergebnis = Some(MessErgebnis::Fehler(format!("Senden fehlgeschlagen: {e}")));
                return;
            }
            self.schreibe(format!(">> {befehl}"));
        }
        match m.ergebnis() {
            Some(e) => {
                let erfolg = matches!(e, MessErgebnis::Messung(_));
                match e {
                    MessErgebnis::Messung(g) => self.schreibe(format!("— Messung: {} —", g.text())),
                    MessErgebnis::Fehler(t) => self.schreibe(format!("— Messung fehlgeschlagen: {t} —")),
                }
                self.messung_ergebnis = Some(e.clone());
                let protokoll = self.protokoll();
                if erfolg {
                    signal_erfolg(&mut self.verbindung, protokoll);
                } else {
                    signal_fehler(&mut self.verbindung, protokoll);
                }
            }
            None => self.messung = Some(m),
        }
    }

    /// Fragt in festem Abstand die Horizontalrichtung ab, solange der Viewer die Zielrichtung zeigt.
    fn treibe_live(&mut self, jetzt: Instant) {
        if self.live_wartet && jetzt.saturating_duration_since(self.live_gesendet) >= LIVE_ANTWORT_TIMEOUT {
            self.live_wartet = false;
            LiveWinkelSperre::anfrage_beendet();
        }
        if !self.live_hz_aktiv || !self.ist_verbunden() {
            self.live_hz = None;
            self.live_v = None;
            return;
        }
        if self.live_wartet
            || self.beschaeftigt()
            || LiveWinkelSperre::gesperrt()
            || jetzt < self.live_naechste
            || LiveWinkelSperre::antwort_offen()
        {
            return;
        }
        let geber = Befehlsgeber::fuer(self.protokoll());
        let befehl = if self.live_v_aktiv { geber.winkel_befehl() } else { geber.hz_winkel_befehl() };
        let Some(befehl) = befehl else {
            return;
        };
        self.live_wartet = true;
        self.live_gesendet = jetzt;
        self.live_naechste = jetzt + LIVE_INTERVALL;
        LiveWinkelSperre::anfrage_gesendet();
        if self.verbindung.senden(&befehl).is_err() {
            self.live_wartet = false;
            LiveWinkelSperre::anfrage_beendet();
        }
    }

    /// Sendet anstehende Schaltbefehle und übernimmt das Ergebnis.
    fn treibe_schalter(&mut self, jetzt: Instant) {
        let Some(mut s) = self.schalter.take() else {
            return;
        };
        s.pruefe_zeit(jetzt);
        if let Some((befehl, rpc)) = s.naechster_befehl(jetzt) {
            if rpc != 0 {
                self.parser.setze_letzten_rpc(rpc);
            }
            match self.verbindung.senden(&befehl) {
                Ok(()) => self.schreibe(format!(">> {befehl}")),
                Err(e) => {
                    self.schalter_ergebnis = Some(TestErgebnis { ok: false, text: format!("Senden fehlgeschlagen: {e}") });
                    self.schalter_ziel = None;
                    return;
                }
            }
        }
        if let Some(e) = s.ergebnis() {
            self.schreibe(format!("— {} —", e.text));
            if e.ok {
                match self.schalter_ziel {
                    Some(SchalterZiel::Reflektorlos(rl)) => self.reflektorlos = rl,
                    Some(SchalterZiel::Laser(an)) => self.laser_an = an,
                    None => {}
                }
            }
            self.schalter_ergebnis = Some(e.clone());
            self.schalter_ziel = None;
        } else {
            self.schalter = Some(s);
        }
    }

    /// Sendet anstehende Befehle der Testmessung und beendet sie bei Ergebnis oder Zeitablauf.
    fn treibe_testmessung(&mut self, jetzt: Instant) {
        let Some(mut test) = self.test.take() else {
            return;
        };
        test.pruefe_zeit(jetzt);
        if let Some((befehl, rpc)) = test.naechster_befehl() {
            self.parser.setze_letzten_rpc(rpc);
            match self.verbindung.senden(&befehl) {
                Ok(()) => self.schreibe(format!(">> {}", if befehl.is_empty() { "<Zeilenende>" } else { &befehl })),
                Err(e) => {
                    self.test_ergebnis = Some(TestErgebnis { ok: false, text: format!("Senden fehlgeschlagen: {e}") });
                    return;
                }
            }
        }
        if let Some(e) = test.ergebnis() {
            self.schreibe(format!("— {} —", e.text));
            self.test_ergebnis = Some(e.clone());
        } else {
            self.test = Some(test);
        }
    }

    /// Schreibt eine Zeile ins Empfangsprotokoll der Oberfläche und in die Protokolldatei.
    fn schreibe(&mut self, zeile: String) {
        self.logge(&zeile);
        if self.protokoll.len() >= MAX_PROTOKOLL_ZEILEN {
            self.protokoll.pop_front();
        }
        self.protokoll.push_back(zeile);
    }

    /// Ist die Verbindung offen?
    pub fn ist_verbunden(&self) -> bool {
        self.verbindung.ist_verbunden()
    }

    /// Wird gerade verbunden?
    pub fn verbindet(&self) -> bool {
        self.verbindung.verbindet()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn neues_geraet() -> Geraet {
        let e = Einstellungen::mit_pfad(std::env::temp_dir().join("fb-geraet-test.xml"));
        Geraet::aus_einstellungen(&e)
    }

    #[test]
    fn protokoll_ist_begrenzt_und_sendefehler_wird_gemeldet() {
        let mut g = neues_geraet();
        for i in 0..(MAX_PROTOKOLL_ZEILEN + 20) {
            g.schreibe(format!("z{i}"));
        }
        assert_eq!(g.protokoll.len(), MAX_PROTOKOLL_ZEILEN);
        assert_eq!(g.protokoll.back().map(String::as_str), Some("z519"));
        assert!(g.sende("%R1Q,2108,0:5000,1").is_err());
        assert_eq!(g.parser.letzter_rpc, 2108);
        assert!(!g.ist_verbunden());
    }

    #[test]
    fn sprache_wechselt_protokoll_und_parser() {
        let mut g = neues_geraet();
        g.setze_modell(TachymeterModell::LeicaTps300);
        assert_eq!(g.protokoll(), Protokoll::GsiOnline);
        assert_eq!(g.parser.format_name(), "GSI-Online");
        g.setze_sprache(Sprache::GeoCom);
        assert_eq!(g.protokoll(), Protokoll::GeoCom);
        assert_eq!(g.parser.format_name(), "GeoCOM");
        g.setze_modell(TachymeterModell::LeicaTs06);
        assert_eq!(g.sprache, Sprache::GeoCom);
    }

    #[test]
    fn sprache_wird_gespeichert_und_gelesen() {
        let mut e = Einstellungen::mit_pfad(std::env::temp_dir().join("fb-geraet-test2.xml"));
        let mut g = neues_geraet();
        g.setze_modell(TachymeterModell::LeicaTs06);
        g.setze_sprache(Sprache::GsiOnline);
        g.in_einstellungen(&mut e);
        assert_eq!(e.tachymeter_sprache, "GSI Online");
        let g2 = Geraet::aus_einstellungen(&e);
        assert_eq!(g2.sprache, Sprache::GsiOnline);
        // Bei Geräten ohne Sprachwahl wird nichts gespeichert
        let mut g3 = neues_geraet();
        g3.setze_modell(TachymeterModell::TopconGts);
        g3.in_einstellungen(&mut e);
        assert!(e.tachymeter_sprache.is_empty());
    }

    #[test]
    fn testmessung_braucht_verbindung() {
        let mut g = neues_geraet();
        assert!(g.starte_testmessung().is_err());
        assert!(g.test.is_none());
    }
}
