// Zweck:    Leica GeoCOM (RS-232): Parser der %R1P-Antworten, Befehlsstrings und Rückgabecodes
//           Anfrage: %R1Q,{RPC},{TrId}:{Parameter}   Antwort: %R1P,0,{TrId}:{RC},{Daten}
// Hardware: Leica TPS1200, TS06, TS11, Trimble S3 (GeoCOM)
// Autor:    Johann Gerner

use std::f64::consts::PI;

use super::messung::{MessungsEdmModus, MessungsTyp, TachymeterMessung};

pub const RPC_COM_NULL_PROC: i32 = 0;
pub const RPC_COM_GET_SW_BAUD: i32 = 1000;
pub const RPC_COM_SET_SW_BAUD: i32 = 1001;
pub const RPC_EDM_LASERPOINTER: i32 = 1004;
pub const RPC_BMM_BEEP_NORMAL: i32 = 11003;
pub const RPC_BMM_BEEP_ALARM: i32 = 11004;
pub const RPC_TMC_DO_MEASURE: i32 = 2008;
pub const RPC_TMC_SET_EDM_MODE: i32 = 2020;
pub const RPC_TMC_GET_EDM_MODE: i32 = 2021;
pub const RPC_TMC_GET_ANGLE1: i32 = 2003;
pub const RPC_TMC_SET_ATM_CORR: i32 = 2028;
pub const RPC_TMC_GET_ATM_CORR: i32 = 2029;
pub const RPC_BAP_MEAS_DIST: i32 = 17017;
pub const RPC_BAP_SET_TARGET_TYPE: i32 = 17021;
pub const RPC_BAP_GET_TARGET_TYPE: i32 = 17022;
pub const RPC_TMC_GET_PRISM_CORR: i32 = 2023;
pub const RPC_TMC_SET_PRISM_CORR: i32 = 2024;
pub const RPC_TMC_GET_ANGLE: i32 = 2107;
pub const RPC_TMC_GET_SIMPLE_MEA: i32 = 2108;
pub const RPC_TMC_GET_FULL_MEAS: i32 = 2167;
/// Relative Bewegung (addiert Hz/V zum aktuellen Winkel).
pub const RPC_MOT_MOVE_ANGLE: i32 = 6007;
/// Absolute Positionierung (PosTol/AngTol > 0 erforderlich).
pub const RPC_AUT_MAKE_POSITIONING: i32 = 9027;

/// Informative Rückgabecodes: Befehl wurde ausgeführt, das Gerät meldet Zusatzinfo.
const INFORMATIVE_RCS: [i32; 7] = [1283, 1284, 1285, 1288, 3079, 3080, 3081];
/// Ist der Returncode eine Zusatzinformation (Befehl wurde ausgeführt) und kein Fehler?
pub fn ist_informativ(rc: i32) -> bool {
    INFORMATIVE_RCS.contains(&rc)
}

/// Faktor rad → gon.
const RAD2GON: f64 = 200.0 / PI;
/// Faktor Genauigkeit rad → cc.
const RAD2CC: f64 = 200.0 / PI * 10_000.0;
/// Faktor gon → rad.
const GON2RAD: f64 = PI / 200.0;
/// Millimeter pro Meter.
const MM_PRO_M: f64 = 1000.0;
/// Koeffizienten der PPM-Formel (Barrel & Sears, Leica-Variante).
const PPM_BASIS: f64 = 281.8;
const PPM_FAKTOR: f64 = 79.661;
const KELVIN: f64 = 273.15;

// ── Befehle (PC → Gerät) ──────────────────────────────────────────────────────

/// Messung auslösen (TMC_DoMeasure).
pub const BEFEHL_MESS_TRIGGER: &str = "%R1Q,2008,0:1,1";
/// Messergebnis abrufen (TMC_GetSimpleMea).
pub const BEFEHL_MESS_ERGEBNIS: &str = "%R1Q,2108,0:5000,1";
/// Winkelabfrage (TMC_GetAngle).
pub const BEFEHL_WINKEL: &str = "%R1Q,2107,0:0";
/// Winkel inkl. Kompensator (TMC_GetAngle1).
pub const BEFEHL_LIBELLE: &str = "%R1Q,2003,0:1";

/// Laserpointer ein/aus.
pub fn laser_befehl(an: bool) -> String {
    format!("%R1Q,{RPC_EDM_LASERPOINTER},0:{}", i32::from(an))
}

/// Zieltyp und EDM-Programm setzen.
pub fn edm_modus_befehle(ziel_typ: i32, edm_prog: i32) -> [String; 2] {
    [
        format!("%R1Q,{RPC_BAP_SET_TARGET_TYPE},0:{ziel_typ}"),
        format!("%R1Q,{RPC_TMC_SET_EDM_MODE},0:{edm_prog}"),
    ]
}

/// Anfrage mit Transaktionsnummer.
pub fn anfrage(rpc: i32, tr_id: i32, parameter: &str) -> String {
    format!("%R1Q,{rpc},{tr_id}:{parameter}")
}

/// Extrahiert den RPC-Code aus einer Anfrage; 0, wenn keiner erkannt wurde.
pub fn rpc_aus_anfrage(anfrage: &str) -> i32 {
    anfrage
        .strip_prefix("%R1Q,")
        .and_then(|rest| rest.split(',').next())
        .and_then(|t| t.trim().parse().ok())
        .unwrap_or(0)
}

/// Zerlegte Antwort: Transaktionsnummer, Returncode, Daten.
#[derive(Clone, Debug, PartialEq)]
pub struct Antwort {
    pub tr_id: i32,
    pub rc: i32,
    pub daten: Vec<String>,
}

/// Zerlegt `%R1P,{x},{TrId}:{RC}[,{daten}]`.
pub fn zerlege_antwort(zeile: &str) -> Option<Antwort> {
    let rest = zeile.trim().strip_prefix("%R1P,")?;
    let (kopf, nutz) = rest.split_once(':')?;
    let tr_id = kopf.split(',').nth(1).and_then(|t| t.trim().parse().ok())?;
    let mut teile = nutz.split(',');
    let rc = teile.next()?.trim().parse().ok()?;
    Some(Antwort { tr_id, rc, daten: teile.map(|t| t.trim().to_string()).collect() })
}

/// Parst eine Antwortzeile; `rpc_kontext` ist der RPC der letzten Anfrage (0 = unbekannt).
pub fn parse_antwort(zeile: &str, rpc_kontext: i32) -> Option<TachymeterMessung> {
    let zeile = zeile.trim();
    if zeile.is_empty() || zeile.starts_with("%R1Q") || !zeile.starts_with("%R1P,") {
        return None;
    }
    let (_, nutz) = zeile.split_once(':')?;
    let werte: Vec<&str> = nutz.trim().split(',').collect();
    let rc: i32 = werte.first()?.trim().parse().ok()?;
    let daten: Vec<&str> = werte[1..].to_vec();

    let mut m = TachymeterMessung::neu("GeoCOM", zeile);
    m.return_code = Some(rc);
    if rc != 0 {
        if INFORMATIVE_RCS.contains(&rc) {
            m.bemerkung = format!("[Hinweis GRC={rc}: {}]", return_code_text(rc));
        } else {
            m.typ = MessungsTyp::Fehler;
            m.bemerkung = format!("GRC={rc}  {}", return_code_text(rc));
            return Some(m);
        }
    }

    Some(match rpc_kontext {
        RPC_TMC_GET_ANGLE1 => parse_winkel_voll(m, &daten),
        RPC_TMC_GET_ATM_CORR => parse_atm_korrektur(m, &daten),
        RPC_TMC_GET_ANGLE => parse_winkel(m, &daten),
        RPC_TMC_GET_SIMPLE_MEA => parse_vollmessung(m, &daten),
        RPC_TMC_GET_FULL_MEAS => parse_vollmessung_erweitert(m, &daten),
        RPC_TMC_GET_EDM_MODE => parse_edm_modus(m, &daten),
        RPC_TMC_GET_PRISM_CORR => parse_prismenkorrektur(m, &daten),
        RPC_BAP_GET_TARGET_TYPE => parse_zieltyp(m, &daten),
        RPC_COM_GET_SW_BAUD => parse_baudrate(m, &daten),
        // Kontext 0 (= RPC_COM_NULL_PROC) bedeutet "kein Kontext": Typ aus der Datenanzahl schließen.
        // In der C#-Version war dieser Zweig dadurch unerreichbar (jede Antwort galt als Ping).
        0 => inferiere_typ(m, &daten),
        RPC_BAP_MEAS_DIST | RPC_TMC_DO_MEASURE | RPC_TMC_SET_EDM_MODE | RPC_TMC_SET_ATM_CORR
        | RPC_TMC_SET_PRISM_CORR | RPC_BAP_SET_TARGET_TYPE | RPC_COM_SET_SW_BAUD | RPC_EDM_LASERPOINTER
        | RPC_BMM_BEEP_NORMAL | RPC_BMM_BEEP_ALARM | RPC_MOT_MOVE_ANGLE | RPC_AUT_MAKE_POSITIONING => {
            parse_status(m, &daten, rpc_kontext)
        }
        _ => inferiere_typ(m, &daten),
    })
}

fn zahl(d: &[&str], i: usize) -> Option<f64> {
    d.get(i).and_then(|t| t.trim().parse().ok())
}

fn rad_nach_gon(rad: f64) -> f64 {
    rad * RAD2GON
}

fn runde4(x: f64) -> f64 {
    (x * 1e4).round() / 1e4
}

fn parse_atm_korrektur(mut m: TachymeterMessung, d: &[&str]) -> TachymeterMessung {
    m.typ = MessungsTyp::AtmKorrektur;
    m.atm_lambda_m = zahl(d, 0);
    m.atm_druck_mbar = zahl(d, 1);
    m.atm_temp_trock_c = zahl(d, 2);
    m.atm_temp_feucht_c = zahl(d, 3);
    if let (Some(p), Some(t)) = (m.atm_druck_mbar, m.atm_temp_trock_c) {
        let ppm = PPM_BASIS - PPM_FAKTOR * p / (KELVIN + t);
        m.atm_ppm = Some((ppm * 100.0).round() / 100.0);
    }
    m.bemerkung = match (m.atm_ppm, m.atm_druck_mbar, m.atm_temp_trock_c) {
        (Some(ppm), Some(p), Some(t)) => format!("PPM={ppm:+.2}  P={p:.1} mbar  T={t:.1}°C"),
        _ => "AtmCorr gelesen (unvollständige Daten)".to_string(),
    };
    m
}

/// TMC_GetAngle1: Hz, V, Genauigkeit, Zeit, Quer-, Längsneigung, …
fn parse_winkel_voll(mut m: TachymeterMessung, d: &[&str]) -> TachymeterMessung {
    m.typ = MessungsTyp::Winkel;
    m.hz_gon = zahl(d, 0).map(rad_nach_gon);
    m.v_gon = zahl(d, 1).map(rad_nach_gon);
    m.winkel_genauigkeit_cc = zahl(d, 2).map(|a| a * RAD2CC);
    m.kreuz_neigung_rad = zahl(d, 4);
    m.laengs_neigung_rad = zahl(d, 5);
    m
}

/// TMC_GetAngle: Hz, V [, Genauigkeit].
fn parse_winkel(mut m: TachymeterMessung, d: &[&str]) -> TachymeterMessung {
    m.typ = MessungsTyp::Winkel;
    m.hz_gon = zahl(d, 0).map(rad_nach_gon);
    m.v_gon = zahl(d, 1).map(rad_nach_gon);
    m.winkel_genauigkeit_cc = zahl(d, 2).map(|a| a * RAD2CC);
    m
}

/// TMC_GetSimpleMea: Hz, V, Schrägstrecke.
fn parse_vollmessung(mut m: TachymeterMessung, d: &[&str]) -> TachymeterMessung {
    m.typ = MessungsTyp::Vollmessung;
    m.hz_gon = zahl(d, 0).map(rad_nach_gon);
    m.v_gon = zahl(d, 1).map(rad_nach_gon);
    m.schraegstrecke_m = zahl(d, 2);
    berechne_abgeleitete_masse(&mut m);
    m
}

/// TMC_GetFullMeas: Hz, V, AngAccH, AngAccV, Schrägstrecke, …
fn parse_vollmessung_erweitert(mut m: TachymeterMessung, d: &[&str]) -> TachymeterMessung {
    m.typ = MessungsTyp::Vollmessung;
    m.hz_gon = zahl(d, 0).map(rad_nach_gon);
    m.v_gon = zahl(d, 1).map(rad_nach_gon);
    m.schraegstrecke_m = zahl(d, 4);
    m.winkel_genauigkeit_cc = zahl(d, 2).map(|a| a * RAD2CC);
    berechne_abgeleitete_masse(&mut m);
    m
}

fn parse_edm_modus(mut m: TachymeterMessung, d: &[&str]) -> TachymeterMessung {
    m.typ = MessungsTyp::EdmModusInfo;
    if let Some(modus) = d.first().and_then(|t| t.trim().parse::<i32>().ok()) {
        m.edm_modus_roh = Some(modus);
        m.edm_modus = edm_modus_aus_code(modus);
        m.bemerkung = format!("EDM-Modus {modus}: {}", edm_modus_text(modus));
    }
    m
}

/// Prismenkonstante: Gerät liefert Meter (z. B. -0,034 m), intern mm.
fn parse_prismenkorrektur(mut m: TachymeterMessung, d: &[&str]) -> TachymeterMessung {
    m.typ = MessungsTyp::Status;
    if let Some(pk) = zahl(d, 0) {
        let mm = pk * MM_PRO_M;
        m.prismenkonstante_mm = Some(mm);
        m.bemerkung = format!("Prismenkonstante: {mm:+.1} mm");
    }
    m
}

fn parse_zieltyp(mut m: TachymeterMessung, d: &[&str]) -> TachymeterMessung {
    m.typ = MessungsTyp::EdmModusInfo;
    if let Some(typ) = d.first().and_then(|t| t.trim().parse::<i32>().ok()) {
        m.edm_modus_roh = Some(typ);
        m.edm_modus = if typ == 0 { MessungsEdmModus::Prisma } else { MessungsEdmModus::Reflektorlos };
        m.bemerkung = if typ == 0 {
            "Zieltyp: Reflektor / Prisma (IR-EDM, BAP_REFL_USE)".to_string()
        } else {
            "Zieltyp: Reflektorlos (RL-EDM, BAP_REFL_LESS)".to_string()
        };
    }
    m
}

fn parse_baudrate(mut m: TachymeterMessung, d: &[&str]) -> TachymeterMessung {
    m.typ = MessungsTyp::Status;
    if let Some(code) = d.first().and_then(|t| t.trim().parse::<i32>().ok()) {
        m.bemerkung = format!("Baudrate-Code {code} ({} Baud)", baudrate_aus_code(code));
    }
    m
}

fn parse_status(mut m: TachymeterMessung, d: &[&str], rpc: i32) -> TachymeterMessung {
    m.typ = MessungsTyp::Status;
    m.bemerkung = match rpc {
        RPC_COM_NULL_PROC => "Ping OK",
        RPC_BAP_MEAS_DIST => "Messung gestartet (BAP_MeasDist)",
        RPC_TMC_DO_MEASURE => "Messung gestartet",
        RPC_TMC_SET_EDM_MODE => "EDM-Modus gesetzt",
        RPC_TMC_SET_ATM_CORR => "Atmosphärische Korrektur (PPM) gesetzt",
        RPC_BAP_SET_TARGET_TYPE => "Zieltyp (IR/RL) gesetzt",
        RPC_COM_SET_SW_BAUD => "Baudrate geändert",
        // Die Antwort enthält keinen Zustand, nur die Bestätigung des Befehls
        RPC_EDM_LASERPOINTER => "Laserpointer-Befehl bestätigt",
        RPC_BMM_BEEP_NORMAL => "Signalton (einfach) ausgegeben",
        RPC_BMM_BEEP_ALARM => "Signalton (Alarm, dreifach) ausgegeben",
        _ => "OK",
    }
    .to_string();
    m
}

/// Ohne Kontext: Typ aus der Anzahl der Datenwerte schließen.
fn inferiere_typ(mut m: TachymeterMessung, d: &[&str]) -> TachymeterMessung {
    match d.len() {
        0 => {
            m.typ = MessungsTyp::Status;
            m.bemerkung = "OK".to_string();
            m
        }
        1 => {
            m.typ = MessungsTyp::Status;
            if let Ok(v) = d[0].trim().parse::<i32>() {
                m.bemerkung = format!("Wert={v}");
            }
            m
        }
        2 => parse_winkel(m, d),
        _ => parse_vollmessung(m, d),
    }
}

fn berechne_abgeleitete_masse(m: &mut TachymeterMessung) {
    if let (Some(v), Some(s)) = (m.v_gon, m.schraegstrecke_m) {
        let v_rad = v * GON2RAD;
        m.horizontalstrecke_m = Some(runde4(s * v_rad.sin()));
        m.hoehenunterschied_m = Some(runde4(s * v_rad.cos()));
    }
}

fn edm_modus_aus_code(code: i32) -> MessungsEdmModus {
    match code {
        1 => MessungsEdmModus::Folie,
        2 | 5 | 8 | 10 | 11 => MessungsEdmModus::Reflektorlos,
        3 | 4 | 7 => MessungsEdmModus::Prisma,
        _ => MessungsEdmModus::Unbekannt,
    }
}

fn edm_modus_text(code: i32) -> String {
    match code {
        0 => "Nicht aktiv".to_string(),
        1 => "EDM_SINGLE_TAPE – Folie/Kleintarget".to_string(),
        2 => "EDM_SINGLE_LRRL – RL Langdistanz (alt)".to_string(),
        3 => "EDM_SINGLE_PRISM – Prisma Einzelmessung".to_string(),
        4 => "EDM_CONT_STANDARD – Prisma Dauermessung".to_string(),
        5 => "EDM_SINGLE_SRANGE – RL Kurzstrecke Einzelmessung".to_string(),
        6 => "EDM_CONT_DYNAMIC – Prisma Dauermessung dynamisch".to_string(),
        7 => "EDM_SINGLE_TRACKING – Prisma Tracking".to_string(),
        8 => "EDM_CONT_REFLESS – RL Dauermessung".to_string(),
        9 => "EDM_SINGLE_FAST – Schnellmessung".to_string(),
        10 => "EDM_SINGLE_LRRL – RL Langdistanz".to_string(),
        11 => "EDM_AVERAGE_SR – RL Mittelwert Langstrecke".to_string(),
        12 => "EDM_SCAN_REF".to_string(),
        _ => format!("Unbekannt ({code})"),
    }
}

fn baudrate_aus_code(code: i32) -> i32 {
    match code {
        0 => 1200,
        1 => 2400,
        2 => 4800,
        3 => 9600,
        4 => 19_200,
        5 => 38_400,
        6 => 57_600,
        7 => 115_200,
        _ => 0,
    }
}

/// Klartext zu einem GeoCOM-Returncode (leer, wenn unbekannt).
pub fn return_code_text(rc: i32) -> &'static str {
    match rc {
        2 => "GRC_UNDEFINED",
        3 => "GRC_IVPARAM – Ungültiger Parameter",
        5 => "GRC_NOTOK – Gerät nicht bereit",
        13 => "GRC_IVRESULT – Ungültiges Ergebnis",
        25 => "GRC_ABORT – Messung abgebrochen",
        27 => "GRC_IVRESULT – Ungültige Parameter (z. B. PosTol=0 nicht erlaubt; explizite Toleranzwerte senden)",
        30 => "GRC_NOMEAS – Keine Messung vorhanden",
        35 => "GRC_NOTACCEPTED – Befehl abgelehnt (Funktion nicht verfügbar?)",
        1280 => "GRC_TMC_NO_MEASUREMENT – Keine Messung gestartet",
        1281 => "GRC_TMC_NOT_STABLE – Instrument nicht stabil",
        1282 => "GRC_TMC_DIST_WAIT – Messung läuft noch, Ergebnis noch nicht bereit",
        1283 => "GRC_TMC_NO_FULL_CORRECTION – Ergebnis nicht vollständig korrigiert (Kompensator/PPM prüfen!)",
        1284 => "GRC_TMC_ACCURACY_GUARANTEE – Genauigkeit nicht garantiert, Daten vorhanden",
        1285 => "GRC_TMC_ANGLE_OK – Nur Winkel gültig, keine Distanz",
        1286 => "GRC_TMC_SIGNAL_ERROR – Kein Reflexionssignal empfangen",
        1288 => "GRC_TMC_ANGLE_NO_FULL_CORRECTION – Winkel vorhanden, nicht vollständig korrigiert",
        1291 => "GRC_TMC_DIST_PPM – Falsche EDM/PPM-Einstellungen, keine Distanz",
        1292 => "GRC_TMC_DIST_ERROR – Distanzfehler (kein Reflex / falscher Modus?)",
        1293 => "GRC_TMC_BUSY – Messmodul beschäftigt (TMC_DoMeasure läuft noch)",
        508 => "GRC_TMC_NO_FULL_CORRECTION – Keine vollständige Korrektion",
        1792 => "GRC_EDM_BUSYMODE – EDM in parallelem Messmodus",
        1793 => "GRC_EDM_NO_VALID_MEAS – Keine gültige Messung (Reflektormodus/Ziel prüfen)",
        1800 => "GRC_EDM_DEV_NOT_INSTALLED – Reflektorlos-Option nicht installiert",
        3079 => "GRC_BAP_CHANGE_ALL_TO_DIST – Alle EDM-Parameter auf Reflektor/IR umgestellt",
        3080 => "GRC_BAP_CHANGE_ALL_TO_EGL – EDM-Parameter automatisch angepasst",
        3081 => "GRC_BAP_CHANGE_ALL_TO_RL – Alle EDM-Parameter auf Reflektorlos/RL umgestellt",
        _ => "",
    }
}

/// Kann diese Zeile vom GeoCOM-Parser verarbeitet werden?
pub fn kann_verarbeiten(zeile: &str) -> bool {
    zeile.trim_start().starts_with("%R1")
}

#[cfg(test)]
mod tests {
    use super::*;

    const HALBER_KREIS_RAD: f64 = PI;

    #[test]
    fn vollmessung_wird_in_gon_umgerechnet() {
        let zeile = format!("%R1P,0,0:0,{},{},25.1234", HALBER_KREIS_RAD / 2.0, HALBER_KREIS_RAD / 2.0);
        let m = parse_antwort(&zeile, RPC_TMC_GET_SIMPLE_MEA).unwrap();
        assert_eq!(m.typ, MessungsTyp::Vollmessung);
        assert!((m.hz_gon.unwrap() - 100.0).abs() < 1e-9);
        assert!((m.v_gon.unwrap() - 100.0).abs() < 1e-9);
        assert_eq!(m.schraegstrecke_m, Some(25.1234));
        // V = 100 gon → horizontal
        assert!((m.horizontalstrecke_m.unwrap() - 25.1234).abs() < 1e-9);
        assert!(m.hoehenunterschied_m.unwrap().abs() < 1e-4);
        assert!(m.ist_vollmessung());
    }

    #[test]
    fn fehlercode_ergibt_fehlermessung() {
        let m = parse_antwort("%R1P,0,0:1293", RPC_TMC_GET_SIMPLE_MEA).unwrap();
        assert_eq!(m.typ, MessungsTyp::Fehler);
        assert!(m.ist_fehler());
        assert!(m.bemerkung.contains("GRC_TMC_BUSY"));
    }

    #[test]
    fn informativer_code_wird_weiter_geparst() {
        let m = parse_antwort("%R1P,0,0:1284,1.0,1.5,12.0", RPC_TMC_GET_SIMPLE_MEA).unwrap();
        assert_eq!(m.typ, MessungsTyp::Vollmessung);
        assert!(m.bemerkung.starts_with("[Hinweis GRC=1284"));
        assert!(m.schraegstrecke_m.is_some());
    }

    #[test]
    fn prismenkonstante_und_edm_modus() {
        let pk = parse_antwort("%R1P,0,0:0,-0.034", RPC_TMC_GET_PRISM_CORR).unwrap();
        assert!((pk.prismenkonstante_mm.unwrap() + 34.0).abs() < 1e-9);
        let edm = parse_antwort("%R1P,0,0:0,5", RPC_TMC_GET_EDM_MODE).unwrap();
        assert_eq!(edm.edm_modus, MessungsEdmModus::Reflektorlos);
        assert_eq!(edm.edm_modus_roh, Some(5));
        let zt = parse_antwort("%R1P,0,0:0,0", RPC_BAP_GET_TARGET_TYPE).unwrap();
        assert_eq!(zt.edm_modus, MessungsEdmModus::Prisma);
    }

    #[test]
    fn atm_korrektur_berechnet_ppm() {
        let m = parse_antwort("%R1P,0,0:0,6.58e-7,1013.25,15.0,10.0", RPC_TMC_GET_ATM_CORR).unwrap();
        let erwartet: f64 = 281.8 - 79.661 * 1013.25 / (273.15 + 15.0);
        assert!((m.atm_ppm.unwrap() - (erwartet * 100.0).round() / 100.0).abs() < 1e-9);
    }

    #[test]
    fn anfragen_und_zerlegung() {
        assert_eq!(rpc_aus_anfrage("%R1Q,2108,0:5000,1"), 2108);
        assert_eq!(rpc_aus_anfrage("GET/I/WI21"), 0);
        assert_eq!(anfrage(2107, 3, "0"), "%R1Q,2107,3:0");
        assert_eq!(laser_befehl(true), "%R1Q,1004,0:1");
        let a = zerlege_antwort("%R1P,0,3:0,1.5,2.5").unwrap();
        assert_eq!((a.tr_id, a.rc), (3, 0));
        assert_eq!(a.daten, vec!["1.5", "2.5"]);
        assert!(zerlege_antwort("%R1Q,1,1:0").is_none());
        assert!(parse_antwort("%R1Q,2108,0:1", 0).is_none());
        assert!(parse_antwort("irgendwas", 0).is_none());
    }

    #[test]
    fn ohne_kontext_wird_aus_der_anzahl_geschlossen() {
        assert_eq!(parse_antwort("%R1P,0,0:0", 0).unwrap().typ, MessungsTyp::Status);
        assert_eq!(parse_antwort("%R1P,0,0:0,1.0,2.0", 0).unwrap().typ, MessungsTyp::Winkel);
        assert_eq!(parse_antwort("%R1P,0,0:0,1.0,2.0,3.0", 0).unwrap().typ, MessungsTyp::Vollmessung);
    }
}
