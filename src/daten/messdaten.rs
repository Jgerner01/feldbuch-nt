// Zweck:    Messdaten festhalten wie die Originalversion:
//           • Tagesprotokoll Data-protocol-{Datum}.csv (lückenlos, nur anhängen, jede Messung sofort)
//           • Messdatei {Projekt}-dat.csv mit METADATA-Kopf (Anschluss- und Neupunktmessungen, Standpunktinfo)
// Hardware: Tachymeter (Leica GeoCOM / GSI Online)
// Autor:    Johann Gerner

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use chrono::{Local, NaiveDateTime};

use crate::daten::einstellungen::Einstellungen;
use crate::daten::modelle::{NeupunktRohdaten, StationierungsErgebnis, StationierungsPunkt};
use crate::grundlagen::datei_speicher;
use crate::grundlagen::format::f;
use crate::grundlagen::{fehler_log, protokoll};
use crate::tachymeter::messung::{MessungsEdmModus, TachymeterMessung};
use crate::tachymeter::modell::TachymeterModell;

/// Kompensator-Messbereich TPS1200: ±10' ≈ ±185 mgon.
const KOMP_MAX_MGON: f64 = 200.0;
/// mgon pro Radiant.
const MGON_PRO_RAD: f64 = 200_000.0 / std::f64::consts::PI;

/// Kennzeichen „Anschlusspunkt" in der Spalte Typ.
pub const TYP_ANSCHLUSSPUNKT: &str = "AP";
/// Kennzeichen „Neupunkt" in der Spalte Typ.
pub const TYP_NEUPUNKT: &str = "NP";

const SPALTENZEILE_PROTOKOLL: &str = "Timestamp; StationNr; InstrHeight_m; PointNr; Code; Method; \
EDM_Mode; PrismConst_mm; Atm_PPM; Pressure_mbar; Temp_C; \
CrossInclination_mgon; LengthInclination_mgon; Compensator_OK; \
Hz_gon; V_gon; SlopeDist_m; TargetHeight_m; Source; \
GNSS_Latitude; GNSS_Longitude; GNSS_Height_m; \
GNSS_HorizAccuracy_m; GNSS_VertAccuracy_m; \
GNSS_FixType; GNSS_Satellites; GNSS_HDOP; GNSS_PDOP";

#[cfg(test)]
const SPALTENZEILE_ALT: &str = "PunktNr; HZ_gon; V_gon; Strecke_m; Zielhoehe_m; Code; Zeitstempel";
/// Anzahl der Spalten des Dateiformats (Platzhalter leerer GNSS-Spalten im Tagesprotokoll).
const GNSS_SPALTEN: usize = 9;

fn zeit_text(z: NaiveDateTime, format: &str) -> String {
    z.format(format).to_string()
}

fn nullbar(wert: Option<f64>, stellen: usize) -> String {
    wert.map_or_else(String::new, |v| f(v, stellen))
}

fn edm_text(m: MessungsEdmModus) -> &'static str {
    match m {
        MessungsEdmModus::Prisma => "Prisma",
        MessungsEdmModus::Folie => "Folie",
        MessungsEdmModus::Reflektorlos => "Reflektorlos",
        MessungsEdmModus::Unbekannt => "",
    }
}

fn neigung_mgon(rad: Option<f64>) -> String {
    rad.map_or_else(String::new, |r| f(r * MGON_PRO_RAD, 1))
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

/// Pfad des Tagesprotokolls für einen Zeitpunkt.
pub fn tagesprotokoll_pfad(e: &Einstellungen, zeit: NaiveDateTime) -> PathBuf {
    e.get_pfad(&format!("Data-protocol-{}.csv", zeit_text(zeit, "%Y-%m-%d")))
}

/// Schreibt einen Messeintrag ins Tagesprotokoll (GNSS-Spalten bleiben leer).
pub fn schreibe_tagesprotokoll(e: &Einstellungen, messmethode: &str, standpunkt_nr: &str, instr_hoehe: f64, punkt_nr: &str, code: &str, m: &TachymeterMessung) -> io::Result<()> {
    let pfad = tagesprotokoll_pfad(e, m.zeitstempel);
    if !pfad.exists() {
        let kopf = format!(
            "# Data Protocol: {}\r\n# Date: {}\r\n# This file is created daily and never overwritten.\r\n\
             # All measurements are recorded without gaps.\r\n{SPALTENZEILE_PROTOKOLL}\r\n",
            e.projekt_name,
            zeit_text(m.zeitstempel, "%Y-%m-%d")
        );
        datei_speicher::schreibe_text(&pfad, &kopf)?;
    }
    let mut felder: Vec<String> = vec![
        zeit_text(m.zeitstempel, "%Y-%m-%d %H:%M:%S"),
        standpunkt_nr.to_string(),
        f(instr_hoehe, 3),
        punkt_nr.to_string(),
        code.to_string(),
        messmethode.to_string(),
        edm_text(m.edm_modus).to_string(),
        nullbar(m.prismenkonstante_mm, 1),
        nullbar(m.atm_ppm, 2),
        nullbar(m.atm_druck_mbar, 1),
        nullbar(m.atm_temp_trock_c, 1),
        neigung_mgon(m.kreuz_neigung_rad),
        neigung_mgon(m.laengs_neigung_rad),
        kompensator_ok(m.kreuz_neigung_rad, m.laengs_neigung_rad),
        nullbar(m.hz_gon, 4),
        nullbar(m.v_gon, 4),
        nullbar(m.schraegstrecke_m, 3),
        nullbar(m.zielhoehe_m, 3),
        m.quelle.clone(),
    ];
    felder.extend(std::iter::repeat_n(String::new(), GNSS_SPALTEN));
    anhaengen(&pfad, &crate::daten::csv_basis::zeile(&felder, ';'))
}

/// Pfad der Messdatei des Projekts.
pub fn messdatei_pfad(e: &Einstellungen) -> PathBuf {
    e.get_pfad(&format!("{}-dat.csv", e.projekt_name))
}

/// Stabile Kennung der aktiven Aufstellung (auch über Programmneustarts).
pub fn aufstellung_id(e: &Einstellungen, nr: &str, ih: f64) -> String {
    let station = e.get_pfad(&format!("Station-{nr}.json"));
    if let Ok(d) = crate::daten::json_speicher::lade::<crate::daten::projekt::StationsdatenDatei>(&station) {
        if !d.aufstellung_id.is_empty() && (d.instrumenten_hoehe - ih).abs() < 0.0005 {
            return d.aufstellung_id;
        }
    }
    if let Ok(d) = crate::daten::csv_datei::CsvDatei::laden(&messdatei_pfad(e)) {
        if let Some(z) = d
            .zeilen
            .last()
            .filter(|z| d.zelle(z, "StandpunktNr") == nr && crate::grundlagen::format::parse_zahl(d.zelle(z, "InstrumentenHoehe_m")).is_some_and(|h| (h - ih).abs() < 0.0005))
        {
            let id = d.zelle(z, "AufstellungId");
            if !id.is_empty() {
                return id.into();
            }
        }
    }
    neue_aufstellung_id(nr)
}

pub fn neue_aufstellung_id(nr: &str) -> String {
    let sicher: String = nr.chars().map(|c| if r#"\/:*?"<>|"#.contains(c) { '_' } else { c }).collect();
    format!("{sicher}-{}", Local::now().format("%Y%m%dT%H%M%S%f"))
}

fn messzeit(zeit: NaiveDateTime) -> String {
    use chrono::TimeZone;
    // Nur eindeutig zuordenbare neue lokale Zeiten mit Offset ausgeben.
    Local.from_local_datetime(&zeit).single().map(|z| z.to_rfc3339_opts(chrono::SecondsFormat::Secs, false)).unwrap_or_else(|| zeit.format("%Y-%m-%d %H:%M:%S").to_string())
}

fn lade_messdatei(e: &Einstellungen) -> io::Result<crate::daten::csv_datei::CsvDatei> {
    use crate::daten::csv_datei::{CsvDatei, TYP_MESSDATEN};
    let pfad = messdatei_pfad(e);
    let mut d = if pfad.exists() { CsvDatei::laden(&pfad)? } else { CsvDatei::neu(TYP_MESSDATEN) };
    if d.datentyp != TYP_MESSDATEN {
        return Err(crate::daten::csv_basis::fehler("Die Zieldatei enthält keine Messdaten."));
    }
    d.pfad = pfad;
    d.projekt = e.projekt_name.clone();
    if d.sensor.is_empty() && e.tachymeter_modell != TachymeterModell::Manuell {
        d.sensor = e.tachymeter_modell.name().into();
    }
    Ok(d)
}

fn schreibe_messung(e: &Einstellungen, felder: Vec<String>, nr: &str, ih: f64, id: &str) -> io::Result<()> {
    use crate::daten::csv_datei::SPALTEN_DAT;
    let mut d = lade_messdatei(e)?;
    // Vor einer Schemaumstellung unveränderte Altdatei sichern.
    if d.pfad.exists() && d.meta("Formatversion").is_none() {
        sichere(&d.pfad)?;
    }
    // Vor dem Leeren eines globalen Kopfes dessen nachweisbare Altzuordnung
    // in die Zeilen übernehmen. Keine Aufstellungs-ID für Altmessungen erfinden.
    let alter_sp = d.standpunkt_nr.clone();
    let alte_ih = d.instr_hoehe.clone();
    let alt = if d.pfad.exists() { crate::daten::polar_messdaten::lese(&d.pfad, |_| false).ok() } else { None };
    for i in 0..d.zeilen.len() {
        if d.zelle(&d.zeilen[i], "StandpunktNr").is_empty() {
            let sp = alt.as_ref().and_then(|a| a.messungen.get(i)).map(|m| m.standpunkt_nr.clone()).unwrap_or_else(|| alter_sp.clone());
            d.set_zelle(i, "StandpunktNr", sp);
        }
        if !alter_sp.is_empty() && d.zelle(&d.zeilen[i], "StandpunktNr") == alter_sp && d.zelle(&d.zeilen[i], "InstrumentenHoehe_m").is_empty() {
            d.set_zelle(i, "InstrumentenHoehe_m", alte_ih.clone());
        }
    }
    let mut werte = felder;
    werte.push(if id.is_empty() { aufstellung_id(e, nr, ih) } else { id.into() });
    werte.push(f(ih, 3));
    let i = d.zeilen.len();
    d.zeilen.push(vec![String::new(); d.spalten.len()]);
    for (spalte, wert) in SPALTEN_DAT.iter().zip(werte) {
        d.set_zelle(i, spalte, wert);
    }
    // Kopfwerte sind bei mehreren Aufstellungen keine gültigen Vorgaben.
    let mut ids = std::collections::HashSet::new();
    for z in &d.zeilen {
        ids.insert(d.zelle(z, "AufstellungId").to_string());
    }
    if ids.len() == 1 && !ids.contains("") {
        d.standpunkt_nr = nr.into();
        d.instr_hoehe = f(ih, 3);
    } else {
        d.standpunkt_nr.clear();
        d.instr_hoehe.clear();
    }
    d.speichern()
}

pub fn schreibe_neupunktmessung(e: &Einstellungen, r: &NeupunktRohdaten, standpunkt_nr: &str, instr_hoehe: f64) -> io::Result<()> {
    let ergebnis = schreibe_messung(
        e,
        vec![
            r.punkt_nr.clone(),
            f(r.hz_gon, 4),
            f(r.v_gon, 4),
            f(r.strecke_m, 3),
            f(r.zielhoehe_m, 3),
            r.code.clone(),
            messzeit(r.zeitstempel),
            f(r.exz_laengs_m, 3),
            f(r.exz_quer_m, 3),
            standpunkt_nr.into(),
            TYP_NEUPUNKT.into(),
        ],
        standpunkt_nr,
        instr_hoehe,
        &r.aufstellung_id,
    );
    ergebnis.inspect_err(|e| fehler_log::log("Messdaten.schreibe_neupunktmessung", &e.to_string()))
}

pub fn schreibe_stationierungsmessung(e: &Einstellungen, p: &StationierungsPunkt, standpunkt_nr: &str, instr_hoehe: f64, zeit: NaiveDateTime) -> io::Result<()> {
    let ergebnis = schreibe_messung(
        e,
        vec![
            p.punkt_nr.clone(),
            f(p.hz, 4),
            f(p.v, 4),
            f(p.strecke, 3),
            f(p.zielhoehe, 3),
            String::new(),
            messzeit(zeit),
            "0.000".into(),
            "0.000".into(),
            standpunkt_nr.into(),
            TYP_ANSCHLUSSPUNKT.into(),
        ],
        standpunkt_nr,
        instr_hoehe,
        "",
    );
    ergebnis.inspect_err(|e| fehler_log::log("Messdaten.schreibe_stationierungsmessung", &e.to_string()))
}

pub fn schreibe_standpunktinfo(e: &Einstellungen, standpunkt_nr: &str, instr_hoehe: f64, erg: &StationierungsErgebnis, sigma_faktor: i32, sigma_wert_mm: f64) {
    let mut kommentar = format!(
        "# STATIONIERUNG: SP={standpunkt_nr}  IH={} m  R={}  H={}  Hoehe={}  Ori={} gon  s0={} mm  n={}",
        f(instr_hoehe, 3),
        f(erg.r, 4),
        f(erg.h, 4),
        f(erg.hoehe, 4),
        f(erg.orientierung_gon, 4),
        f(erg.s0_mm, 2),
        erg.redundanz
    );
    if sigma_faktor > 1 {
        kommentar.push_str(&format!("  {sigma_faktor}s0={} mm", f(sigma_wert_mm, 2)));
    }
    let ergebnis = lade_messdatei(e).and_then(|mut d| {
        d.kommentare.push((d.zeilen.len(), kommentar.clone()));
        d.speichern()
    });
    if let Err(e) = ergebnis {
        fehler_log::log("Messdaten.schreibe_standpunktinfo", &e.to_string());
    }
    protokoll::log("STANDPKT", &kommentar);
}

fn sichere(pfad: &Path) -> io::Result<()> {
    let stamm = pfad.file_stem().unwrap_or_default().to_string_lossy();
    fs::copy(pfad, pfad.with_file_name(format!("{stamm}_backup_{}.csv", Local::now().format("%Y%m%d-%H%M%S%f"))))?;
    Ok(())
}

/// Kompatible Aufrüstung, ohne unbekannte Spalten/Kommentare zu verwerfen.
pub fn migriere_spalten(pfad: &Path) -> bool {
    let Ok(mut d) = crate::daten::csv_datei::CsvDatei::laden(pfad) else {
        return false;
    };
    if d.spalten_index("ExzLaengs_m").is_some() {
        return false;
    }
    if sichere(pfad).is_err() {
        return false;
    }
    for i in 0..d.zeilen.len() {
        for (k, v) in [("ExzLaengs_m", "0.000"), ("ExzQuer_m", "0.000"), ("StandpunktNr", ""), ("Typ", ""), ("AufstellungId", ""), ("InstrumentenHoehe_m", "")] {
            d.set_zelle(i, k, v.into());
        }
    }
    d.speichern().is_ok()
}

/// Indizes beziehen sich auf die physischen Startzeilen der geladenen Datensätze.
pub fn schreibe_exzentrizitaeten(pfad: &Path, aenderungen: &[(usize, f64, f64)]) -> io::Result<()> {
    let mut d = crate::daten::csv_datei::CsvDatei::laden(pfad)?;
    for (index, laengs, quer) in aenderungen {
        if !laengs.is_finite() || !quer.is_finite() {
            return Err(crate::daten::csv_basis::fehler("Exzentrizität ist keine endliche Zahl."));
        }
        let i = d
            .zeilennummern
            .iter()
            .position(|nr| *nr == index + 1)
            .ok_or_else(|| crate::daten::csv_basis::fehler(format!("Messzeile {} wurde verändert. Datei neu laden.", index + 1)))?;
        d.set_zelle(i, "ExzLaengs_m", f(*laengs, 3));
        d.set_zelle(i, "ExzQuer_m", f(*quer, 3));
    }
    d.speichern()
}

fn anhaengen(pfad: &Path, zeile: &str) -> io::Result<()> {
    let mut datei = OpenOptions::new().create(true).append(true).open(pfad)?;
    datei.write_all(format!("{zeile}\r\n").as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daten::modelle::jetzt;

    fn einst(name: &str) -> (Einstellungen, PathBuf) {
        let dir = std::env::temp_dir().join(format!("fb-md-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let mut e = Einstellungen::mit_pfad(dir.join("Einstellungen.xml"));
        e.set_projekt("Demo", &dir).unwrap();
        e.tachymeter_modell = TachymeterModell::LeicaTs06;
        (e, dir)
    }

    fn messung() -> TachymeterMessung {
        let mut m = TachymeterMessung::neu("GeoCOM", "");
        m.hz_gon = Some(264.861);
        m.v_gon = Some(97.0559);
        m.schraegstrecke_m = Some(1.4424);
        m.zielhoehe_m = Some(1.5);
        m.edm_modus = MessungsEdmModus::Reflektorlos;
        m.prismenkonstante_mm = Some(-17.5);
        m.kreuz_neigung_rad = Some(0.00005);
        m.laengs_neigung_rad = Some(-0.00003);
        m
    }

    #[test]
    fn tagesprotokoll_hat_kopf_und_vollstaendige_zeile() {
        let (e, dir) = einst("tag");
        let m = messung();
        schreibe_tagesprotokoll(&e, "Neupunkt", "STP1", 1.556, "7", "Baum", &m).unwrap();
        schreibe_tagesprotokoll(&e, "Neupunkt", "STP1", 1.556, "8", "", &m).unwrap();
        let text = fs::read_to_string(tagesprotokoll_pfad(&e, m.zeitstempel)).unwrap();
        let zeilen: Vec<&str> = text.lines().collect();
        assert_eq!(zeilen[0], "# Data Protocol: Demo");
        assert_eq!(zeilen[4], SPALTENZEILE_PROTOKOLL);
        assert_eq!(zeilen.len(), 7, "Kopf (5) + 2 Messungen");
        let felder = crate::daten::csv_basis::felder(zeilen[5], ';').unwrap();
        assert_eq!(felder.len(), SPALTENZEILE_PROTOKOLL.split("; ").count());
        assert_eq!(&felder[1..7], &["STP1", "1.556", "7", "Baum", "Neupunkt", "Reflektorlos"]);
        assert_eq!(felder[7], "-17.5");
        assert_eq!(felder[13], "Yes");
        assert_eq!(&felder[14..19], &["264.8610", "97.0559", "1.442", "1.500", "GeoCOM"]);
        assert!(felder[19..].iter().all(|x| x.is_empty()), "GNSS-Spalten leer");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn kompensator_ausserhalb_des_bereichs() {
        assert_eq!(kompensator_ok(Some(0.01), Some(0.0)), "No");
        assert_eq!(kompensator_ok(Some(0.0001), Some(0.0)), "Yes");
        assert_eq!(kompensator_ok(None, Some(0.0)), "");
        assert_eq!(neigung_mgon(Some(std::f64::consts::PI / 200_000.0)), "1.0");
    }

    #[test]
    fn messdatei_hat_metadata_kopf_und_zeilen() {
        let (e, dir) = einst("dat");
        let roh = NeupunktRohdaten {
            aufstellung_id: String::new(),
            punkt_nr: "7".into(),
            code: "Baum".into(),
            hz_gon: 1.5,
            v_gon: 100.0,
            strecke_m: 12.3456,
            zielhoehe_m: 1.5,
            zeitstempel: jetzt(),
            standpunkt_nr: "STP1".into(),
            exz_laengs_m: 0.25,
            exz_quer_m: -0.1,
        };
        schreibe_neupunktmessung(&e, &roh, "STP1", 1.556).unwrap();
        let ap = StationierungsPunkt { punkt_nr: "A".into(), hz: 10.0, v: 99.0, strecke: 50.0, ..Default::default() };
        schreibe_stationierungsmessung(&e, &ap, "STP1", 1.556, jetzt()).unwrap();
        let erg = StationierungsErgebnis { r: 1.0, h: 2.0, hoehe: 3.0, orientierung_gon: 4.0, s0_mm: 5.0, redundanz: 6, ..Default::default() };
        schreibe_standpunktinfo(&e, "STP1", 1.556, &erg, 2, 10.0);

        let text = fs::read_to_string(messdatei_pfad(&e)).unwrap();
        let zeilen: Vec<&str> = text.lines().collect();
        assert_eq!(zeilen[0], "# METADATA");
        assert!(text.contains("Projekt: Demo"));
        assert!(text.contains("Sensor: LeicaTS06"));
        let d = crate::daten::csv_datei::CsvDatei::laden(&messdatei_pfad(&e)).unwrap();
        assert_eq!(d.zeilen.len(), 2);
        assert_eq!(d.spalten.len(), 13);
        assert_eq!(d.zelle(&d.zeilen[0], "Typ"), "NP");
        assert_eq!(d.zelle(&d.zeilen[1], "Typ"), "AP");
        assert_eq!(d.zelle(&d.zeilen[0], "InstrumentenHoehe_m"), "1.556");
        assert!(!d.zelle(&d.zeilen[0], "AufstellungId").is_empty());
        assert!(text.contains("# STATIONIERUNG: SP=STP1"));

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn alte_siebenspaltige_datei_wird_aufgeruestet() {
        let (e, dir) = einst("mig");
        let alt = format!("# METADATA\r\nProjekt: Demo\r\n---\r\n# DATEN\r\n{SPALTENZEILE_ALT}\r\n1; 2; 3; 4; 5; c; 2026-01-01 10:00\r\n");
        fs::write(messdatei_pfad(&e), alt).unwrap();
        let roh = NeupunktRohdaten { punkt_nr: "9".into(), zeitstempel: jetzt(), ..Default::default() };
        schreibe_neupunktmessung(&e, &roh, "S", 0.0).unwrap();
        let text = fs::read_to_string(messdatei_pfad(&e)).unwrap();
        let d = crate::daten::csv_datei::CsvDatei::parse(&text).unwrap();
        assert_eq!(d.zeilen.len(), 2);
        assert_eq!(d.zelle(&d.zeilen[0], "PunktNr"), "1");
        assert_eq!(d.zelle(&d.zeilen[0], "Zeitstempel"), "2026-01-01 10:00");
        assert!(d.spalten_index("AufstellungId").is_some());
        let sicherungen = fs::read_dir(&dir).unwrap().filter_map(Result::ok).filter(|d| d.file_name().to_string_lossy().contains("_backup_")).count();
        assert_eq!(sicherungen, 1);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn unbekanntes_format_wird_nicht_ueberschrieben() {
        let (e, dir) = einst("alt");
        fs::write(messdatei_pfad(&e), "irgendein altes Format\r\n").unwrap();
        let roh = NeupunktRohdaten { punkt_nr: "1".into(), zeitstempel: jetzt(), ..Default::default() };
        assert!(schreibe_neupunktmessung(&e, &roh, "S", 0.0).is_err());
        let text = fs::read_to_string(messdatei_pfad(&e)).unwrap();
        assert_eq!(text, "irgendein altes Format\r\n");
        let _ = fs::remove_dir_all(dir);
    }
}
