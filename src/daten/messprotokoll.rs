// Zweck:    Messprotokoll {Projekt}-Messprotokoll.csv (siehe Protokoll.md): eine Zeile je Ereignis, feste Spalten,
//           laufende Nummer, nur Anhängen. Messungen, Korrekturen, Nummernänderungen, Löschungen, Standpunkte,
//           Instrumentenhöhe, EDM/Prisma und Stationierungsergebnisse. Ersetzt das frühere Data-protocol-*.csv.
//           Dazu der einmalige Import von Altprojekten (Quelle „Import“).
// Hardware: Tachymeter (Rohantworten in Spalte „Roh“)
// Autor:    Johann Gerner

use std::collections::{HashMap, HashSet};
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use chrono::{Local, NaiveDateTime};

use crate::daten::csv_basis;
use crate::daten::einstellungen::Einstellungen;
use crate::daten::projekt::StationsdatenDatei;
use crate::grundlagen::fehler_log;
use crate::grundlagen::format::{f, parse_zahl};
use crate::tachymeter::messung::{MessungsEdmModus, TachymeterMessung};

/// Formatversion des Protokolls.
const FORMATVERSION: &str = "1";
/// Trennzeichen.
const TRENNER: char = ';';
/// Kompensator-Messbereich TPS1200: ±10' ≈ ±185 mgon.
const KOMP_MAX_MGON: f64 = 200.0;
/// mgon pro Radiant.
const MGON_PRO_RAD: f64 = 200_000.0 / std::f64::consts::PI;
/// Zeitformat der Einträge (lokale Zeit).
const ZEIT_FORMAT: &str = "%Y-%m-%dT%H:%M:%S";
/// Trenner mehrerer Rohantworten in einer Zelle.
pub const ROH_TRENNER: &str = " | ";
/// Anzahl GNSS-Spalten (Breite, Länge, ellipsoidische Höhe, Genauigkeit Lage/Höhe, Fix, Satelliten, HDOP, PDOP).
const GNSS_SPALTEN: usize = 9;

/// Spalten des Messprotokolls.
pub const SPALTEN: [&str; 43] = [
    "LfdNr",
    "Zeit",
    "Ereignis",
    "AufstellungId",
    "StandpunktNr",
    "IH_m",
    "PunktNr",
    "Typ",
    "Code",
    "Hz_gon",
    "V_gon",
    "S_m",
    "ZH_m",
    "ExzL_m",
    "ExzQ_m",
    "R",
    "H",
    "Hoehe",
    "EDM",
    "Prismentyp",
    "PrismKonst_mm",
    "ppm",
    "Druck_mbar",
    "Temp_C",
    "NeigL_mgon",
    "NeigQ_mgon",
    "Kompensator_OK",
    "GNSS_Breite",
    "GNSS_Laenge",
    "GNSS_Hoehe_m",
    "GNSS_GenLage_m",
    "GNSS_GenHoehe_m",
    "GNSS_Fix",
    "GNSS_Sat",
    "GNSS_HDOP",
    "GNSS_PDOP",
    "Quelle",
    "Bezug",
    "Feld",
    "Alt",
    "Neu",
    "Text",
    "Roh",
];

/// Quellen eines Eintrags.
pub const QUELLE_GERAET: &str = "Geraet";
pub const QUELLE_HAND: &str = "Hand";
pub const QUELLE_GETRENNT: &str = "Getrennt";
pub const QUELLE_IMPORT: &str = "Import";
pub const QUELLE_PROGRAMM: &str = "Programm";

/// Art eines Protokolleintrags.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ereignis {
    Projekt,
    Geraet,
    Edm,
    Prisma,
    StpNeu,
    StpFort,
    StpUmbenannt,
    Ih,
    Messung,
    Korrektur,
    Nummer,
    Ersetzt,
    Geloescht,
    Exz,
    Stationierung,
    Verworfen,
    Bemerkung,
    /// Zeile aus einem alten Data-protocol-*.csv (nur Nachweis, wird beim Ableiten nicht verwendet).
    Altmessung,
    /// GNSS-Empfänger verbunden/getrennt, SAPOS, Konfiguration
    GnssVerbindung,
    /// GNSS-Einpassung übernommen, gewechselt oder gelöscht
    Einpassung,
}

impl Ereignis {
    pub const ALLE: [Ereignis; 20] = [
        Self::Projekt,
        Self::Geraet,
        Self::Edm,
        Self::Prisma,
        Self::StpNeu,
        Self::StpFort,
        Self::StpUmbenannt,
        Self::Ih,
        Self::Messung,
        Self::Korrektur,
        Self::Nummer,
        Self::Ersetzt,
        Self::Geloescht,
        Self::Exz,
        Self::Stationierung,
        Self::Verworfen,
        Self::Bemerkung,
        Self::Altmessung,
        Self::GnssVerbindung,
        Self::Einpassung,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Projekt => "PROJEKT",
            Self::Geraet => "GERAET",
            Self::Edm => "EDM",
            Self::Prisma => "PRISMA",
            Self::StpNeu => "STP_NEU",
            Self::StpFort => "STP_FORT",
            Self::StpUmbenannt => "STP_UMBENANNT",
            Self::Ih => "IH",
            Self::Messung => "MESSUNG",
            Self::Korrektur => "KORREKTUR",
            Self::Nummer => "NUMMER",
            Self::Ersetzt => "ERSETZT",
            Self::Geloescht => "GELOESCHT",
            Self::Exz => "EXZ",
            Self::Stationierung => "STATIONIERUNG",
            Self::Verworfen => "VERWORFEN",
            Self::Bemerkung => "BEMERKUNG",
            Self::Altmessung => "ALTMESSUNG",
            Self::GnssVerbindung => "GNSS_VERBINDUNG",
            Self::Einpassung => "EINPASSUNG",
        }
    }

    pub fn aus_name(name: &str) -> Option<Self> {
        Self::ALLE.into_iter().find(|e| e.name().eq_ignore_ascii_case(name.trim()))
    }
}

/// Werte einer GNSS-Messung für das Messprotokoll.
#[derive(Clone, Debug, Default)]
pub struct GnssWerte {
    pub breite: f64,
    pub laenge: f64,
    /// ellipsoidische Höhe des Bodenpunkts
    pub h_ell: f64,
    /// Streuung der Mittelung (Lage, Höhe) [m]
    pub gen_lage_m: Option<f64>,
    pub gen_hoehe_m: Option<f64>,
    pub fix: String,
    pub satelliten: Option<u8>,
    pub pdop: Option<f64>,
    /// Stabhöhe [m]
    pub stab_m: f64,
    /// weitere Angaben (Schlüssel, Wert) für die Spalte „Roh“
    pub zusatz: Vec<(String, String)>,
}

/// Eine Zeile des Messprotokolls.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Eintrag {
    pub lfd_nr: u64,
    pub zeit: String,
    pub ereignis: String,
    pub aufstellung_id: String,
    pub standpunkt_nr: String,
    pub ih: Option<f64>,
    pub punkt_nr: String,
    pub typ: String,
    pub code: String,
    pub hz: Option<f64>,
    pub v: Option<f64>,
    pub s: Option<f64>,
    pub zh: Option<f64>,
    pub exz_l: Option<f64>,
    pub exz_q: Option<f64>,
    pub r: Option<f64>,
    pub h: Option<f64>,
    pub hoehe: Option<f64>,
    pub edm: String,
    pub prismentyp: String,
    pub prism_konst: Option<f64>,
    pub ppm: Option<f64>,
    pub druck: Option<f64>,
    pub temp: Option<f64>,
    pub neig_l: Option<f64>,
    pub neig_q: Option<f64>,
    pub kompensator: String,
    pub gnss: [String; GNSS_SPALTEN],
    pub quelle: String,
    pub bezug: Option<u64>,
    pub feld: String,
    pub alt: String,
    pub neu: String,
    pub text: String,
    pub roh: String,
}

fn zahl(wert: Option<f64>, stellen: usize) -> String {
    wert.filter(|w| w.is_finite()).map_or_else(String::new, |w| f(w, stellen))
}

fn endlich(wert: f64) -> Option<f64> {
    wert.is_finite().then_some(wert)
}

/// Text des EDM-Modus einer Messung.
pub fn edm_text(m: MessungsEdmModus) -> &'static str {
    match m {
        MessungsEdmModus::Prisma => "Prisma",
        MessungsEdmModus::Folie => "Folie",
        MessungsEdmModus::Reflektorlos => "Reflektorlos",
        MessungsEdmModus::Unbekannt => "",
    }
}

fn kompensator_ok(kreuz: Option<f64>, laengs: Option<f64>) -> String {
    match (kreuz, laengs) {
        (Some(k), Some(l)) => {
            let ok = (k * MGON_PRO_RAD).abs() <= KOMP_MAX_MGON && (l * MGON_PRO_RAD).abs() <= KOMP_MAX_MGON;
            if ok { "Yes" } else { "No" }.to_string()
        }
        _ => String::new(),
    }
}

/// Zeitpunkt als Text des Protokolls.
pub fn zeit_text(z: NaiveDateTime) -> String {
    z.format(ZEIT_FORMAT).to_string()
}

/// Sortierschlüssel aus verschiedenen Zeitformaten (ISO mit/ohne Offset, mit Leerzeichen statt „T“).
pub fn zeit_schluessel(text: &str) -> Option<NaiveDateTime> {
    let t = text.trim().get(..19)?.replacen(' ', "T", 1);
    NaiveDateTime::parse_from_str(&t, ZEIT_FORMAT).ok()
}

impl Eintrag {
    pub fn neu(ereignis: Ereignis) -> Self {
        Self { ereignis: ereignis.name().to_string(), ..Self::default() }
    }

    pub fn art(&self) -> Option<Ereignis> {
        Ereignis::aus_name(&self.ereignis)
    }

    /// Aufstellung (Id, Nummer, Instrumentenhöhe).
    pub fn mit_aufstellung(mut self, s: &StationsdatenDatei) -> Self {
        self.aufstellung_id.clone_from(&s.aufstellung_id);
        self.standpunkt_nr.clone_from(&s.standpunkt_nr);
        self.ih = Some(s.instrumenten_hoehe);
        self
    }

    /// Messwerte, Gerätezustand und Rohantwort einer Tachymetermessung (Quelle Gerät).
    pub fn mit_messung(mut self, m: &TachymeterMessung) -> Self {
        self.zeit = zeit_text(m.zeitstempel);
        self.hz = m.hz_gon;
        self.v = m.v_gon;
        self.s = m.schraegstrecke_m;
        self.zh = m.zielhoehe_m;
        self.edm = edm_text(m.edm_modus).to_string();
        self.prism_konst = m.prismenkonstante_mm;
        self.ppm = m.atm_ppm;
        self.druck = m.atm_druck_mbar;
        self.temp = m.atm_temp_trock_c;
        self.neig_l = m.laengs_neigung_rad.map(|r| r * MGON_PRO_RAD);
        self.neig_q = m.kreuz_neigung_rad.map(|r| r * MGON_PRO_RAD);
        self.kompensator = kompensator_ok(m.kreuz_neigung_rad, m.laengs_neigung_rad);
        self.quelle = QUELLE_GERAET.to_string();
        self.roh = m.rohdaten.clone();
        if !m.quelle.is_empty() {
            self.text = format!("Format {}", m.quelle);
        }
        self
    }

    /// Prismentyp aus den Einstellungen (und Konstante, wenn die Messung keine liefert).
    pub fn mit_prisma(mut self, e: &Einstellungen) -> Self {
        self.prismentyp.clone_from(&e.prisma_name);
        if self.prism_konst.is_none() {
            self.prism_konst = Some(e.prisma_konstante_mm);
        }
        if self.edm.is_empty() {
            self.edm.clone_from(&e.mess_modus_name);
        }
        self
    }

    pub fn punkt(mut self, nr: &str, typ: &str, code: &str) -> Self {
        self.punkt_nr = nr.to_string();
        self.typ = typ.to_string();
        self.code = code.to_string();
        self
    }

    pub fn werte(mut self, hz: f64, v: f64, s: f64, zh: f64) -> Self {
        (self.hz, self.v, self.s, self.zh) = (endlich(hz), endlich(v), endlich(s), endlich(zh));
        self
    }

    pub fn exzentrizitaet(mut self, laengs: f64, quer: f64) -> Self {
        (self.exz_l, self.exz_q) = (endlich(laengs), endlich(quer));
        self
    }

    /// GNSS-Messwerte: belegt die GNSS-Spalten, weitere Angaben (UTM, Höhenbezug, Epochen, Mountpoint …) stehen in „Roh“.
    pub fn mit_gnss(mut self, g: &GnssWerte) -> Self {
        let zahl_text = |w: Option<f64>, s: usize| zahl(w, s);
        self.gnss = [
            f(g.breite, 9),
            f(g.laenge, 9),
            f(g.h_ell, 4),
            zahl_text(g.gen_lage_m, 4),
            zahl_text(g.gen_hoehe_m, 4),
            g.fix.clone(),
            g.satelliten.map_or_else(String::new, |n| n.to_string()),
            String::new(),
            zahl_text(g.pdop, 2),
        ];
        self.zh = endlich(g.stab_m);
        let zusatz = g.zusatz.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join(ROH_TRENNER);
        self.roh = if self.roh.is_empty() { zusatz } else { format!("{zusatz}{ROH_TRENNER}{}", self.roh) };
        self.quelle = QUELLE_GERAET.to_string();
        self
    }

    pub fn koordinaten(mut self, r: f64, h: f64, hoehe: f64) -> Self {
        (self.r, self.h, self.hoehe) = (endlich(r), endlich(h), endlich(hoehe));
        self
    }

    pub fn quelle(mut self, quelle: &str) -> Self {
        self.quelle = quelle.to_string();
        self
    }

    pub fn bezug(mut self, lfd_nr: Option<u64>) -> Self {
        self.bezug = lfd_nr;
        self
    }

    pub fn aenderung(mut self, feld: &str, alt: &str, neu: &str) -> Self {
        self.feld = feld.to_string();
        self.alt = alt.to_string();
        self.neu = neu.to_string();
        self
    }

    pub fn text(mut self, text: impl Into<String>) -> Self {
        let t = text.into();
        self.text = if self.text.is_empty() { t } else { format!("{t}; {}", self.text) };
        self
    }

    fn felder(&self) -> Vec<String> {
        let mut v = vec![
            self.lfd_nr.to_string(),
            self.zeit.clone(),
            self.ereignis.clone(),
            self.aufstellung_id.clone(),
            self.standpunkt_nr.clone(),
            zahl(self.ih, 3),
            self.punkt_nr.clone(),
            self.typ.clone(),
            self.code.clone(),
            zahl(self.hz, 5),
            zahl(self.v, 5),
            zahl(self.s, 4),
            zahl(self.zh, 3),
            zahl(self.exz_l, 3),
            zahl(self.exz_q, 3),
            zahl(self.r, 4),
            zahl(self.h, 4),
            zahl(self.hoehe, 4),
            self.edm.clone(),
            self.prismentyp.clone(),
            zahl(self.prism_konst, 1),
            zahl(self.ppm, 2),
            zahl(self.druck, 1),
            zahl(self.temp, 1),
            zahl(self.neig_l, 1),
            zahl(self.neig_q, 1),
            self.kompensator.clone(),
        ];
        v.extend(self.gnss.iter().cloned());
        v.extend([
            self.quelle.clone(),
            self.bezug.map_or_else(String::new, |b| b.to_string()),
            self.feld.clone(),
            self.alt.clone(),
            self.neu.clone(),
            einzeilig(&self.text),
            einzeilig(&self.roh),
        ]);
        v
    }

    fn aus_felder(index: &HashMap<String, usize>, werte: &[String]) -> Self {
        let t = |name: &str| index.get(name).and_then(|&i| werte.get(i)).map_or("", |s| s.as_str()).to_string();
        let z = |name: &str| parse_zahl(&t(name));
        let mut gnss: [String; GNSS_SPALTEN] = Default::default();
        for (i, g) in gnss.iter_mut().enumerate() {
            *g = t(SPALTEN[27 + i]);
        }
        Self {
            lfd_nr: t("LfdNr").parse().unwrap_or(0),
            zeit: t("Zeit"),
            ereignis: t("Ereignis"),
            aufstellung_id: t("AufstellungId"),
            standpunkt_nr: t("StandpunktNr"),
            ih: z("IH_m"),
            punkt_nr: t("PunktNr"),
            typ: t("Typ"),
            code: t("Code"),
            hz: z("Hz_gon"),
            v: z("V_gon"),
            s: z("S_m"),
            zh: z("ZH_m"),
            exz_l: z("ExzL_m"),
            exz_q: z("ExzQ_m"),
            r: z("R"),
            h: z("H"),
            hoehe: z("Hoehe"),
            edm: t("EDM"),
            prismentyp: t("Prismentyp"),
            prism_konst: z("PrismKonst_mm"),
            ppm: z("ppm"),
            druck: z("Druck_mbar"),
            temp: z("Temp_C"),
            neig_l: z("NeigL_mgon"),
            neig_q: z("NeigQ_mgon"),
            kompensator: t("Kompensator_OK"),
            gnss,
            quelle: t("Quelle"),
            bezug: t("Bezug").parse().ok(),
            feld: t("Feld"),
            alt: t("Alt"),
            neu: t("Neu"),
            text: t("Text"),
            roh: t("Roh"),
        }
    }
}

/// Zeilenumbrüche durch den Rohtrenner ersetzen (eine physische Zeile je Eintrag).
fn einzeilig(text: &str) -> String {
    text.trim().replace("\r\n", "\n").split('\n').map(str::trim).filter(|s| !s.is_empty()).collect::<Vec<_>>().join(ROH_TRENNER)
}

// ── Schreiben ────────────────────────────────────────────────────────────────

/// Pfad des Messprotokolls eines Projekts.
pub fn pfad(e: &Einstellungen) -> PathBuf {
    e.get_pfad(&format!("{}-Messprotokoll.csv", e.projekt_name))
}

struct Zustand {
    /// Letzte LfdNr je Datei.
    lfd: HashMap<PathBuf, u64>,
    /// Ziel für Einträge ohne Projektzugriff (Gerät, Datenmanager): (Pfad, Projektname).
    aktiv: Option<(PathBuf, String)>,
    /// Schreibfehler zur Anzeige in der Oberfläche.
    fehler: Vec<String>,
}

fn zustand() -> &'static Mutex<Zustand> {
    static Z: OnceLock<Mutex<Zustand>> = OnceLock::new();
    Z.get_or_init(|| Mutex::new(Zustand { lfd: HashMap::new(), aktiv: None, fehler: Vec::new() }))
}

/// Setzt das Projekt für `schreibe_aktiv` (beim Laden eines Projekts).
pub fn konfiguriere(e: &Einstellungen) {
    if let Ok(mut z) = zustand().lock() {
        z.aktiv = e.ist_geladen().then(|| (pfad(e), e.projekt_name.clone()));
    }
}

/// Holt aufgelaufene Schreibfehler für die Anzeige ab.
pub fn hole_fehler() -> Vec<String> {
    zustand().lock().map(|mut z| std::mem::take(&mut z.fehler)).unwrap_or_default()
}

fn kopf(projekt: &str) -> String {
    format!(
        "# Messprotokoll: {projekt}\r\n# Formatversion: {FORMATVERSION}\r\n\
         # Nur anhaengen, nie ueberschreiben. Korrekturen, Nummernaenderungen und Loeschungen verweisen ueber Bezug auf LfdNr.\r\n{}\r\n",
        csv_basis::zeile(SPALTEN, TRENNER)
    )
}

/// Hängt einen Eintrag an; vergibt die laufende Nummer (und die Zeit, wenn leer). Liefert die LfdNr.
pub fn schreibe_in(pfad: &Path, projekt: &str, mut eintrag: Eintrag) -> io::Result<u64> {
    let mut z = zustand().lock().map_err(|_| csv_basis::fehler("Messprotokoll gesperrt."))?;
    let vorhanden = pfad.exists();
    let letzte = match (vorhanden, z.lfd.get(pfad)) {
        (false, _) => 0,
        (true, Some(n)) => *n,
        (true, None) => lese(pfad)?.iter().map(|e| e.lfd_nr).max().unwrap_or(0),
    };
    if let Some(ordner) = pfad.parent() {
        fs::create_dir_all(ordner)?;
    }
    let mut datei = OpenOptions::new().create(true).append(true).open(pfad)?;
    if !vorhanden {
        datei.write_all(kopf(projekt).as_bytes())?;
    }
    eintrag.lfd_nr = letzte + 1;
    if eintrag.zeit.is_empty() {
        eintrag.zeit = zeit_text(Local::now().naive_local());
    }
    datei.write_all(format!("{}\r\n", csv_basis::zeile(eintrag.felder(), TRENNER)).as_bytes())?;
    z.lfd.insert(pfad.to_path_buf(), eintrag.lfd_nr);
    Ok(eintrag.lfd_nr)
}

/// Schreibt einen Eintrag ins Messprotokoll des Projekts. Ohne geladenes Projekt geschieht nichts.
pub fn schreibe(e: &Einstellungen, eintrag: Eintrag) -> io::Result<u64> {
    if !e.ist_geladen() {
        return Ok(0);
    }
    schreibe_in(&pfad(e), &e.projekt_name, eintrag)
}

fn melde(fehler: &io::Error) {
    let text = format!("Messprotokoll konnte nicht geschrieben werden: {fehler}");
    fehler_log::log("Messprotokoll", &text);
    if let Ok(mut z) = zustand().lock() {
        if z.fehler.last() != Some(&text) {
            z.fehler.push(text);
        }
    }
}

/// Wie `schreibe`, Fehler werden protokolliert und in der Oberfläche gemeldet (die Messung wird nicht blockiert).
pub fn protokolliere(e: &Einstellungen, eintrag: Eintrag) -> Option<u64> {
    schreibe(e, eintrag).inspect_err(melde).ok().filter(|n| *n > 0)
}

/// Schreibt ins Messprotokoll des zuletzt geladenen Projekts (für Stellen ohne Projektzugriff).
pub fn protokolliere_aktiv(eintrag: Eintrag) -> Option<u64> {
    let ziel = zustand().lock().ok()?.aktiv.clone()?;
    schreibe_in(&ziel.0, &ziel.1, eintrag).inspect_err(melde).ok()
}

// ── Lesen ────────────────────────────────────────────────────────────────────

/// Liest alle Einträge eines Messprotokolls.
pub fn lese(pfad: &Path) -> io::Result<Vec<Eintrag>> {
    let text = fs::read_to_string(pfad)?;
    let mut index: Option<HashMap<String, usize>> = None;
    let mut aus = Vec::new();
    for (nr, satz) in csv_basis::saetze(&text)? {
        let s = satz.trim();
        if s.is_empty() || s.starts_with('#') {
            continue;
        }
        let werte = csv_basis::felder(s, TRENNER).map_err(|e| csv_basis::fehler(format!("Messprotokoll Zeile {nr}: {e}")))?;
        match &index {
            None => index = Some(werte.iter().enumerate().map(|(i, n)| (n.clone(), i)).collect()),
            Some(idx) => aus.push(Eintrag::aus_felder(idx, &werte)),
        }
    }
    Ok(aus)
}

/// Letzte noch gültige Messung (nicht ersetzt/gelöscht) einer Aufstellung mit Nummer und Typ.
pub fn letzte_messung(eintraege: &[Eintrag], aufstellung_id: &str, punkt_nr: &str, typ: &str) -> Option<u64> {
    let erledigt: HashSet<u64> =
        eintraege.iter().filter(|e| matches!(e.art(), Some(Ereignis::Ersetzt | Ereignis::Geloescht))).filter_map(|e| e.bezug).collect();
    // Nummernänderungen: aktuelle Nummer je Messung
    let mut nummer: HashMap<u64, String> = HashMap::new();
    for e in eintraege.iter().filter(|e| e.art() == Some(Ereignis::Nummer)) {
        if let Some(b) = e.bezug {
            nummer.insert(b, e.neu.clone());
        }
    }
    eintraege
        .iter()
        .rev()
        .filter(|e| e.art() == Some(Ereignis::Messung) && !erledigt.contains(&e.lfd_nr))
        .find(|e| e.aufstellung_id == aufstellung_id && e.typ.eq_ignore_ascii_case(typ) && nummer.get(&e.lfd_nr).unwrap_or(&e.punkt_nr) == punkt_nr)
        .map(|e| e.lfd_nr)
}

/// Sucht im Protokoll des Projekts die letzte gültige Messung (siehe `letzte_messung`).
pub fn finde_messung(e: &Einstellungen, aufstellung_id: &str, punkt_nr: &str, typ: &str) -> Option<u64> {
    let p = pfad(e);
    if !p.exists() {
        return None;
    }
    letzte_messung(&lese(&p).ok()?, aufstellung_id, punkt_nr, typ)
}

// ── Beobachter für Gerät und Prisma ──────────────────────────────────────────

/// Erkennt Änderungen von Verbindung, EDM-Modus und Prisma, gleich von welcher Stelle sie ausgelöst wurden.
#[derive(Default)]
pub struct Beobachter {
    projekt: String,
    verbunden: Option<bool>,
    reflektorlos: Option<bool>,
    prisma: Option<(String, String)>,
}

impl Beobachter {
    /// Einmal je Bild aufrufen.
    pub fn pruefe(&mut self, e: &Einstellungen, verbunden: bool, geraet_reflektorlos: bool, modell: &str) {
        if !e.ist_geladen() {
            return;
        }
        if self.projekt != e.projekt_name {
            *self = Self { projekt: e.projekt_name.clone(), ..Self::default() };
        }
        let rl = if verbunden { geraet_reflektorlos } else { e.mess_modus_name != "Prisma" };
        let prisma = (e.prisma_name.clone(), f(e.prisma_konstante_mm, 1));
        if self.verbunden.is_some_and(|v| v != verbunden) {
            let text = if verbunden { format!("Verbunden: {modell}") } else { format!("Getrennt: {modell}") };
            protokolliere(e, Eintrag::neu(Ereignis::Geraet).aenderung("Verbindung", if verbunden { "nein" } else { "ja" }, if verbunden { "ja" } else { "nein" }).text(text).quelle(QUELLE_PROGRAMM));
        }
        let name = |rl: bool| if rl { "Reflektorlos" } else { "Prisma" };
        if let Some(alt) = self.reflektorlos.filter(|alt| *alt != rl) {
            protokolliere(e, Eintrag::neu(Ereignis::Edm).aenderung("EDM", name(alt), name(rl)).quelle(QUELLE_PROGRAMM));
        }
        if let Some(alt) = self.prisma.as_ref().filter(|alt| **alt != prisma) {
            protokolliere(
                e,
                Eintrag::neu(Ereignis::Prisma)
                    .aenderung("Prisma", &format!("{} ({} mm)", alt.0, alt.1), &format!("{} ({} mm)", prisma.0, prisma.1))
                    .quelle(QUELLE_PROGRAMM)
                    .mit_prisma(e),
            );
        }
        self.verbunden = Some(verbunden);
        self.reflektorlos = Some(rl);
        self.prisma = Some(prisma);
    }
}

// ── Import von Altprojekten (P7) ─────────────────────────────────────────────

/// Erzeugt einmalig ein Start-Protokoll aus vorhandenen Stationsdateien, Messdatei und Data-protocol-*.csv,
/// wenn das Projekt noch kein Messprotokoll hat. Die Quelldateien bleiben unverändert. Liefert die Zahl der Einträge.
pub fn erzeuge_startprotokoll(e: &Einstellungen) -> io::Result<usize> {
    let ziel = pfad(e);
    if !e.ist_geladen() || ziel.exists() {
        return Ok(0);
    }
    let mut liste: Vec<(Option<NaiveDateTime>, Eintrag)> = Vec::new();
    let (stationen, ap) = importiere_stationen(e, &mut liste);
    let dat = importiere_messdatei(e, &mut liste);
    let alt = importiere_data_protocol(e, &mut liste);
    if liste.is_empty() {
        return Ok(0);
    }
    liste.sort_by_key(|(z, _)| *z);
    let n = liste.len();
    for (_, eintrag) in liste {
        schreibe_in(&ziel, &e.projekt_name, eintrag)?;
    }
    schreibe_in(
        &ziel,
        &e.projekt_name,
        Eintrag::neu(Ereignis::Bemerkung)
            .quelle(QUELLE_IMPORT)
            .text(format!("Start-Protokoll aus Altdaten erzeugt: {stationen} Aufstellung(en), {ap} Anschlussmessung(en), {dat} Messung(en) aus der Messdatei, {alt} Zeile(n) aus Data-protocol")),
    )?;
    Ok(n + 1)
}

fn importiere_stationen(e: &Einstellungen, liste: &mut Vec<(Option<NaiveDateTime>, Eintrag)>) -> (usize, usize) {
    let Ok(eintraege) = fs::read_dir(&e.projekt_verzeichnis) else {
        return (0, 0);
    };
    let mut dateien: Vec<(String, PathBuf)> = eintraege
        .filter_map(Result::ok)
        .filter_map(|d| {
            let name = d.file_name().to_string_lossy().into_owned();
            (name.starts_with("Station-") && name.ends_with(".json")).then(|| (name, d.path()))
        })
        .collect();
    dateien.sort();
    // Je AufstellungId die Datei mit den meisten Informationen (Ergebnis, Messungen)
    let mut beste: Vec<(String, StationsdatenDatei)> = Vec::new();
    for (name, pfad) in dateien {
        let Ok(d) = crate::daten::json_speicher::lade::<StationsdatenDatei>(&pfad) else {
            continue;
        };
        let gewicht = |s: &StationsdatenDatei| (s.ergebnis.is_some(), s.messungen.len());
        match beste.iter_mut().find(|(_, b)| !d.aufstellung_id.is_empty() && b.aufstellung_id == d.aufstellung_id) {
            Some(b) if gewicht(&d) > gewicht(&b.1) => *b = (name, d),
            Some(_) => {}
            None => beste.push((name, d)),
        }
    }
    let mut ap = 0;
    for (name, d) in &beste {
        let zeit = crate::daten::projekt::beginn_der_station(d);
        let zeit_text_ = zeit.map(zeit_text).unwrap_or_default();
        let mut stp = Eintrag::neu(Ereignis::StpNeu).mit_aufstellung(d).quelle(QUELLE_IMPORT).text(format!("aus {name}"));
        stp.zeit.clone_from(&zeit_text_);
        stp.code.clone_from(&d.code);
        if let Some(b) = &d.bekannt {
            stp = stp.text(format!("bekannter Punkt {}", b.punkt_nr)).koordinaten(b.r, b.h, b.hoehe);
        }
        if let Some(g) = &d.geraet {
            stp.edm.clone_from(&g.edm_modus);
            stp.prismentyp.clone_from(&g.prisma);
            stp.prism_konst = Some(g.prismenkonstante_mm);
            stp.ppm = g.atm_ppm;
            stp.neig_l = g.laengsneigung_mgon;
            stp.neig_q = g.querneigung_mgon;
        }
        liste.push((zeit, stp));
        for p in &d.messungen {
            let mut m = Eintrag::neu(Ereignis::Messung)
                .mit_aufstellung(d)
                .punkt(&p.punkt_nr, crate::daten::messdaten::TYP_ANSCHLUSSPUNKT, "")
                .werte(p.hz, p.v, p.strecke, p.zielhoehe)
                .koordinaten(p.r, p.h, p.hoehe)
                .quelle(QUELLE_IMPORT)
                .text(format!("aus {name}"));
            m.zeit.clone_from(&zeit_text_);
            liste.push((zeit, m));
            ap += 1;
        }
        if let Some(erg) = &d.ergebnis {
            let mut s = stationierung_eintrag(d, erg).quelle(QUELLE_IMPORT);
            s.zeit.clone_from(&zeit_text_);
            liste.push((zeit, s));
        }
    }
    (beste.len(), ap)
}

/// Eintrag STATIONIERUNG zu einer Aufstellung und ihrem Ergebnis (inaktive Beobachtungen im Text).
pub fn stationierung_eintrag(s: &StationsdatenDatei, erg: &crate::daten::modelle::StationierungsErgebnis) -> Eintrag {
    let inaktiv = |aktiv: &[bool]| -> String {
        s.messungen.iter().zip(aktiv).filter(|(_, a)| !**a).map(|(p, _)| p.punkt_nr.as_str()).collect::<Vec<_>>().join(", ")
    };
    let mut text = format!(
        "{} Ori={} gon Massstab={} s0={} mm n={} 3D={}",
        if s.bekannt.is_some() { "bekannter Standpunkt" } else { "freie Stationierung" },
        f(erg.orientierung_gon, 5),
        f(erg.massstab, 6),
        f(erg.s0_mm, 2),
        erg.redundanz,
        if erg.berechnung_3d { "ja" } else { "nein" }
    );
    for (titel, liste) in [("Richtung", &s.aktiv_richtung), ("Strecke", &s.aktiv_strecke), ("Hoehe", &s.aktiv_hoehe)] {
        let t = inaktiv(liste);
        if !t.is_empty() {
            text.push_str(&format!(" inaktiv {titel}: {t}"));
        }
    }
    let punkte: Vec<&str> = s.messungen.iter().map(|p| p.punkt_nr.as_str()).collect();
    text.push_str(&format!(" Anschluss: {}", punkte.join(", ")));
    Eintrag::neu(Ereignis::Stationierung).mit_aufstellung(s).koordinaten(erg.r, erg.h, erg.hoehe).text(text)
}

fn importiere_messdatei(e: &Einstellungen, liste: &mut Vec<(Option<NaiveDateTime>, Eintrag)>) -> usize {
    let pfad = crate::daten::messdaten::messdatei_pfad(e);
    let Ok(d) = crate::daten::csv_datei::CsvDatei::laden(&pfad) else {
        return 0;
    };
    let name = pfad.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let mut n = 0;
    for z in &d.zeilen {
        let t = |s: &str| d.zelle(z, s).to_string();
        let zahl = |s: &str| parse_zahl(d.zelle(z, s));
        let typ = if t("Typ").is_empty() { crate::daten::messdaten::TYP_NEUPUNKT.to_string() } else { t("Typ") };
        let mut m = Eintrag::neu(Ereignis::Messung).punkt(&t("PunktNr"), &typ, &t("Code")).quelle(QUELLE_IMPORT).text(format!("aus {name}"));
        m.aufstellung_id = t("AufstellungId");
        m.standpunkt_nr = t("StandpunktNr");
        m.ih = zahl("InstrumentenHoehe_m");
        (m.hz, m.v, m.s, m.zh) = (zahl("HZ_gon"), zahl("V_gon"), zahl("Strecke_m"), zahl("Zielhoehe_m"));
        (m.exz_l, m.exz_q) = (zahl("ExzLaengs_m"), zahl("ExzQuer_m"));
        let zeit = zeit_schluessel(&t("Zeitstempel"));
        m.zeit = zeit.map(zeit_text).unwrap_or_else(|| t("Zeitstempel"));
        liste.push((zeit, m));
        n += 1;
    }
    n
}

fn importiere_data_protocol(e: &Einstellungen, liste: &mut Vec<(Option<NaiveDateTime>, Eintrag)>) -> usize {
    let Ok(eintraege) = fs::read_dir(&e.projekt_verzeichnis) else {
        return 0;
    };
    let mut dateien: Vec<PathBuf> = eintraege
        .filter_map(Result::ok)
        .map(|d| d.path())
        .filter(|p| p.file_name().is_some_and(|n| n.to_string_lossy().starts_with("Data-protocol-") && n.to_string_lossy().ends_with(".csv")))
        .collect();
    dateien.sort();
    let mut n = 0;
    for pfad in dateien {
        let Ok(text) = fs::read_to_string(&pfad) else {
            continue;
        };
        let name = pfad.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let mut index: Option<HashMap<String, usize>> = None;
        for zeile in text.lines() {
            let s = zeile.trim();
            if s.is_empty() || s.starts_with('#') {
                continue;
            }
            let Ok(werte) = csv_basis::felder(s, ';') else {
                continue;
            };
            let Some(idx) = &index else {
                index = Some(werte.iter().enumerate().map(|(i, n)| (n.trim().to_string(), i)).collect());
                continue;
            };
            let t = |k: &str| idx.get(k).and_then(|&i| werte.get(i)).map_or("", |s| s.as_str()).to_string();
            let z = |k: &str| parse_zahl(&t(k));
            let mut m = Eintrag::neu(Ereignis::Altmessung).punkt(&t("PointNr"), "", &t("Code")).quelle(QUELLE_IMPORT);
            m.standpunkt_nr = t("StationNr");
            m.ih = z("InstrHeight_m");
            (m.hz, m.v, m.s, m.zh) = (z("Hz_gon"), z("V_gon"), z("SlopeDist_m"), z("TargetHeight_m"));
            m.edm = t("EDM_Mode");
            m.prism_konst = z("PrismConst_mm");
            m.ppm = z("Atm_PPM");
            m.druck = z("Pressure_mbar");
            m.temp = z("Temp_C");
            m.neig_q = z("CrossInclination_mgon");
            m.neig_l = z("LengthInclination_mgon");
            m.kompensator = t("Compensator_OK");
            m.text = format!("aus {name}: {} ({})", t("Method"), t("Source"));
            let zeit = zeit_schluessel(&t("Timestamp"));
            m.zeit = zeit.map(zeit_text).unwrap_or_else(|| t("Timestamp"));
            liste.push((zeit, m));
            n += 1;
        }
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    fn einstellungen(name: &str) -> (Einstellungen, PathBuf) {
        let dir = std::env::temp_dir().join(format!("feldbuch_messprotokoll_{name}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let mut e = Einstellungen::mit_pfad(dir.join("Einstellungen.xml"));
        e.set_projekt("Test", &dir).unwrap();
        (e, dir)
    }

    #[test]
    fn schreibt_und_liest_mit_laufender_nummer() {
        let (e, dir) = einstellungen("lesen");
        let mut m = TachymeterMessung::neu("GeoCOM", "%R1P,0,0:0,1.2,1.5,10.0\n%R1P,0,0:0,0.0001,0.0002");
        (m.hz_gon, m.v_gon, m.schraegstrecke_m, m.zielhoehe_m) = (Some(12.34567), Some(99.5), Some(10.1234), Some(1.7));
        m.kreuz_neigung_rad = Some(0.0001);
        m.laengs_neigung_rad = Some(0.0002);
        let s = StationsdatenDatei { aufstellung_id: "STP1-x".into(), standpunkt_nr: "STP1".into(), instrumenten_hoehe: 1.55, ..Default::default() };
        let n1 = schreibe(&e, Eintrag::neu(Ereignis::Messung).mit_messung(&m).mit_aufstellung(&s).punkt("7", "NP", "Baum; Eiche")).unwrap();
        let n2 = schreibe(&e, Eintrag::neu(Ereignis::Korrektur).bezug(Some(n1)).aenderung("ZH_m", "1.700", "1.800").quelle(QUELLE_HAND)).unwrap();
        assert_eq!((n1, n2), (1, 2));
        let text = fs::read_to_string(pfad(&e)).unwrap();
        assert!(text.starts_with("# Messprotokoll: Test"));
        assert_eq!(text.lines().filter(|l| !l.starts_with('#')).count(), 3);
        let alle = lese(&pfad(&e)).unwrap();
        assert_eq!(alle.len(), 2);
        let a = &alle[0];
        assert_eq!(a.art(), Some(Ereignis::Messung));
        assert_eq!(a.code, "Baum; Eiche");
        assert_eq!(a.hz, Some(12.34567));
        assert_eq!(a.ih, Some(1.55));
        assert_eq!(a.kompensator, "Yes");
        assert!(a.roh.contains(ROH_TRENNER) && a.roh.starts_with("%R1P"));
        assert_eq!(alle[1].bezug, Some(1));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn letzte_gueltige_messung_beachtet_ersetzen_und_nummern() {
        let m = |lfd: u64, nr: &str| Eintrag { lfd_nr: lfd, aufstellung_id: "A".into(), ..Eintrag::neu(Ereignis::Messung).punkt(nr, "NP", "") };
        let mut liste = vec![m(1, "5"), m(2, "6")];
        assert_eq!(letzte_messung(&liste, "A", "5", "NP"), Some(1));
        liste.push(Eintrag { lfd_nr: 3, ..Eintrag::neu(Ereignis::Ersetzt).bezug(Some(1)) });
        assert_eq!(letzte_messung(&liste, "A", "5", "NP"), None);
        liste.push(Eintrag { lfd_nr: 4, ..Eintrag::neu(Ereignis::Nummer).bezug(Some(2)).aenderung("PunktNr", "6", "60") });
        assert_eq!(letzte_messung(&liste, "A", "60", "NP"), Some(2));
        assert_eq!(letzte_messung(&liste, "A", "6", "NP"), None);
        assert_eq!(letzte_messung(&liste, "B", "60", "NP"), None);
    }

    #[test]
    fn startprotokoll_aus_altdaten() {
        let (e, dir) = einstellungen("import");
        let station = StationsdatenDatei {
            aufstellung_id: "STP1-20261101T080000000".into(),
            standpunkt_nr: "STP1".into(),
            instrumenten_hoehe: 1.5,
            beginn: "2026-11-01T08:00:00".into(),
            messungen: vec![crate::daten::modelle::StationierungsPunkt { punkt_nr: "A1".into(), r: 100.0, h: 200.0, hz: 1.0, v: 100.0, strecke: 10.0, zielhoehe: 1.7, ..Default::default() }],
            ..Default::default()
        };
        crate::daten::json_speicher::speichere(&dir.join("Station-STP1.json"), &station, "test").unwrap();
        fs::write(
            dir.join("Data-protocol-2026-11-01.csv"),
            "# Data Protocol: Test\r\nTimestamp; StationNr; InstrHeight_m; PointNr; Code; Method; Hz_gon; V_gon; SlopeDist_m; TargetHeight_m; Source\r\n\
             2026-11-01 09:00:00;STP1;1.500;7;;Neupunkt;10.0000;100.0000;5.000;1.700;GeoCOM\r\n# NEUER STANDPUNKT: SP=STP1\r\n",
        )
        .unwrap();
        let n = erzeuge_startprotokoll(&e).unwrap();
        assert_eq!(n, 4);
        let alle = lese(&pfad(&e)).unwrap();
        let arten: Vec<_> = alle.iter().map(|a| a.art().unwrap()).collect();
        assert_eq!(arten, vec![Ereignis::StpNeu, Ereignis::Messung, Ereignis::Altmessung, Ereignis::Bemerkung]);
        assert_eq!(alle[1].r, Some(100.0));
        assert_eq!(alle[2].punkt_nr, "7");
        assert!(alle.iter().all(|a| a.quelle == QUELLE_IMPORT));
        // Ein zweiter Aufruf ändert nichts
        assert_eq!(erzeuge_startprotokoll(&e).unwrap(), 0);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn kompensator_ausserhalb_des_bereichs() {
        assert_eq!(kompensator_ok(Some(0.01), Some(0.0)), "No");
        assert_eq!(kompensator_ok(Some(0.0001), Some(0.0)), "Yes");
        assert_eq!(kompensator_ok(None, Some(0.0)), "");
    }

    #[test]
    fn beobachter_meldet_edm_prisma_und_verbindung() {
        let (mut e, dir) = einstellungen("beobachter");
        let mut b = Beobachter::default();
        b.pruefe(&e, false, false, "TS06");
        assert!(!pfad(&e).exists(), "der erste Zustand wird nur gemerkt");
        b.pruefe(&e, true, true, "TS06");
        e.prisma_name = "GRZ4 360°".into();
        e.prisma_konstante_mm = 23.1;
        b.pruefe(&e, true, true, "TS06");
        let arten: Vec<_> = lese(&pfad(&e)).unwrap().iter().map(|a| a.art().unwrap()).collect();
        assert_eq!(arten, vec![Ereignis::Geraet, Ereignis::Edm, Ereignis::Prisma]);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn zeitschluessel_verschiedener_formate() {
        let a = zeit_schluessel("2026-11-01T09:00:00+01:00").unwrap();
        assert_eq!(zeit_schluessel("2026-11-01 09:00:00"), Some(a));
        assert!(zeit_schluessel("x").is_none());
    }
}
