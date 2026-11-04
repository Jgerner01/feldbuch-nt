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

const SPALTENZEILE_ALT: &str = "PunktNr; HZ_gon; V_gon; Strecke_m; Zielhoehe_m; Code; Zeitstempel";
const SPALTENZEILE: &str =
    "PunktNr; HZ_gon; V_gon; Strecke_m; Zielhoehe_m; Code; Zeitstempel; ExzLaengs_m; ExzQuer_m; StandpunktNr; Typ";
/// Zusatzwerte für Datenzeilen alter Dateien.
const ZUSATZWERTE_ALTE_ZEILE: &str = "; 0.000; 0.000; ; ";
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
pub fn schreibe_tagesprotokoll(
    e: &Einstellungen,
    messmethode: &str,
    standpunkt_nr: &str,
    instr_hoehe: f64,
    punkt_nr: &str,
    code: &str,
    m: &TachymeterMessung,
) -> io::Result<()> {
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
    anhaengen(&pfad, &felder.join("; "))
}

/// Pfad der Messdatei des Projekts.
pub fn messdatei_pfad(e: &Einstellungen) -> PathBuf {
    e.get_pfad(&format!("{}-dat.csv", e.projekt_name))
}

/// Schreibt eine Neupunktmessung in die Messdatei.
pub fn schreibe_neupunktmessung(e: &Einstellungen, r: &NeupunktRohdaten, standpunkt_nr: &str, instr_hoehe: f64) {
    stelle_messdatei_sicher(e, standpunkt_nr, instr_hoehe);
    let zeile = [
        r.punkt_nr.clone(),
        f(r.hz_gon, 4),
        f(r.v_gon, 4),
        f(r.strecke_m, 3),
        f(r.zielhoehe_m, 3),
        r.code.clone(),
        zeit_text(r.zeitstempel, "%Y-%m-%d %H:%M"),
        f(r.exz_laengs_m, 3),
        f(r.exz_quer_m, 3),
        standpunkt_nr.to_string(),
        TYP_NEUPUNKT.to_string(),
    ]
    .join("; ");
    hänge_messdatei_an(e, &zeile);
}

/// Schreibt eine Anschlusspunktmessung (Stationierung) in die Messdatei.
pub fn schreibe_stationierungsmessung(
    e: &Einstellungen,
    p: &StationierungsPunkt,
    standpunkt_nr: &str,
    instr_hoehe: f64,
    zeit: NaiveDateTime,
) {
    stelle_messdatei_sicher(e, standpunkt_nr, instr_hoehe);
    let zeile = [
        p.punkt_nr.clone(),
        f(p.hz, 4),
        f(p.v, 4),
        f(p.strecke, 3),
        f(p.zielhoehe, 3),
        String::new(),
        zeit_text(zeit, "%Y-%m-%d %H:%M"),
        f(0.0, 3),
        f(0.0, 3),
        standpunkt_nr.to_string(),
        TYP_ANSCHLUSSPUNKT.to_string(),
    ]
    .join("; ");
    hänge_messdatei_an(e, &zeile);
}

/// Schreibt die Kommentarzeile nach einer Neuberechnung der Stationierung.
pub fn schreibe_standpunktinfo(
    e: &Einstellungen,
    standpunkt_nr: &str,
    instr_hoehe: f64,
    erg: &StationierungsErgebnis,
    sigma_faktor: i32,
    sigma_wert_mm: f64,
) {
    stelle_messdatei_sicher(e, standpunkt_nr, instr_hoehe);
    let mut zeile = format!(
        "# STATIONIERUNG: SP={standpunkt_nr}  IH={} m  R={}  H={}  Hoehe={}  Ori={} gon  s0={} mm  n={}",
        f(instr_hoehe, 3),
        f(erg.r, 3),
        f(erg.h, 3),
        f(erg.hoehe, 3),
        f(erg.orientierung_gon, 4),
        f(erg.s0_mm, 2),
        erg.redundanz
    );
    if sigma_faktor > 1 {
        zeile.push_str(&format!("  {sigma_faktor}s0={} mm", f(sigma_wert_mm, 2)));
    }
    hänge_messdatei_an(e, &zeile);
    protokoll::log("STANDPKT", zeile.trim_start_matches(['#', ' ']));
}

/// Stellt sicher, dass die Messdatei mit korrektem Kopf existiert; alte Formate werden aufgerüstet oder gesichert.
fn stelle_messdatei_sicher(e: &Einstellungen, standpunkt_nr: &str, instr_hoehe: f64) {
    let pfad = messdatei_pfad(e);
    if pfad.exists() {
        let erste = fs::read_to_string(&pfad).ok().and_then(|t| t.lines().next().map(|z| z.trim().to_string()));
        match erste {
            Some(z) if z == "# METADATA" => {
                migriere_spalten(&pfad);
                return;
            }
            None => return,
            Some(_) => {
                // Veraltetes Format: Sicherungskopie anlegen
                let stamm = pfad.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
                let backup = pfad.with_file_name(format!("{stamm}_backup_{}.csv", Local::now().format("%Y%m%d-%H%M%S")));
                if fs::rename(&pfad, &backup).is_err() {
                    return;
                }
                fehler_log::log("Messdaten.stelle_messdatei_sicher", &format!("Veraltetes Format gesichert nach: {}", backup.display()));
            }
        }
    }
    let sensor = if e.tachymeter_modell == TachymeterModell::Manuell { "" } else { e.tachymeter_modell.name() };
    let kopf = format!(
        "# METADATA\r\nProjekt: {}\r\nSensor: {sensor}\r\nBearbeiter: \r\nDatum: {}\r\nStandpunktNr: {standpunkt_nr}\r\n\
         InstrumentenHoehe: {}\r\nStandpunktCode: \r\n---\r\n# DATENTYP\r\nMessdaten\r\n# DATEN\r\n{SPALTENZEILE}\r\n",
        e.projekt_name,
        Local::now().format("%Y-%m-%d"),
        f(instr_hoehe, 3)
    );
    if let Err(fehler) = datei_speicher::schreibe_text(&pfad, &kopf) {
        fehler_log::log("Messdaten.stelle_messdatei_sicher", &fehler.to_string());
    }
}

/// Rüstet eine Datei mit alter 7-Spalten-Zeile auf das 11-Spalten-Format auf (mit Sicherungskopie).
pub fn migriere_spalten(pfad: &Path) -> bool {
    let Ok(text) = fs::read_to_string(pfad) else {
        return false;
    };
    let mut zeilen: Vec<String> = text.lines().map(str::to_string).collect();
    let Some(idx) = zeilen.iter().position(|z| z.trim() == SPALTENZEILE_ALT) else {
        return false;
    };
    let stamm = pfad.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let backup = pfad.with_file_name(format!("{stamm}_backup_{}.csv", Local::now().format("%Y%m%d-%H%M%S")));
    if fs::copy(pfad, backup).is_err() {
        return false;
    }
    zeilen[idx] = SPALTENZEILE.to_string();
    for z in zeilen.iter_mut().skip(idx + 1) {
        if z.trim().is_empty() || z.trim_start().starts_with('#') {
            continue;
        }
        *z = format!("{}{ZUSATZWERTE_ALTE_ZEILE}", z.trim_end());
    }
    datei_speicher::schreibe_zeilen(pfad, zeilen).is_ok()
}

/// Anzahl der Spalten der Messdatei.
const SPALTENANZAHL: usize = 11;
const SPALTE_EXZ_LAENGS: usize = 7;
const SPALTE_EXZ_QUER: usize = 8;

/// Schreibt geänderte Exzentrizitäten (Zeilenindex, Längs [m], Quer [m]) zurück in die Messdatei.
pub fn schreibe_exzentrizitaeten(pfad: &Path, aenderungen: &[(usize, f64, f64)]) -> io::Result<()> {
    migriere_spalten(pfad);
    let text = fs::read_to_string(pfad)?;
    let mut zeilen: Vec<String> = text.lines().map(str::to_string).collect();
    for (index, laengs, quer) in aenderungen {
        let Some(zeile) = zeilen.get_mut(*index) else {
            continue;
        };
        let mut teile: Vec<String> = zeile.split(';').map(|t| t.trim().to_string()).collect();
        while teile.len() < SPALTENANZAHL {
            teile.push(String::new());
        }
        teile[SPALTE_EXZ_LAENGS] = f(*laengs, 3);
        teile[SPALTE_EXZ_QUER] = f(*quer, 3);
        *zeile = teile.join("; ");
    }
    datei_speicher::schreibe_zeilen(pfad, zeilen)
}

fn hänge_messdatei_an(e: &Einstellungen, zeile: &str) {
    if let Err(fehler) = anhaengen(&messdatei_pfad(e), zeile) {
        fehler_log::log("Messdaten.anhaengen", &fehler.to_string());
    }
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
        let felder: Vec<&str> = zeilen[5].split("; ").collect();
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
        schreibe_neupunktmessung(&e, &roh, "STP1", 1.556);
        let ap = StationierungsPunkt { punkt_nr: "A".into(), hz: 10.0, v: 99.0, strecke: 50.0, ..Default::default() };
        schreibe_stationierungsmessung(&e, &ap, "STP1", 1.556, jetzt());
        let erg = StationierungsErgebnis { r: 1.0, h: 2.0, hoehe: 3.0, orientierung_gon: 4.0, s0_mm: 5.0, redundanz: 6, ..Default::default() };
        schreibe_standpunktinfo(&e, "STP1", 1.556, &erg, 2, 10.0);

        let text = fs::read_to_string(messdatei_pfad(&e)).unwrap();
        let zeilen: Vec<&str> = text.lines().collect();
        assert_eq!(zeilen[0], "# METADATA");
        assert_eq!(zeilen[1], "Projekt: Demo");
        assert_eq!(zeilen[2], "Sensor: LeicaTS06");
        assert_eq!(zeilen[12], SPALTENZEILE);
        assert!(zeilen[13].starts_with("7; 1.5000; 100.0000; 12.346; 1.500; Baum; "));
        assert!(zeilen[13].ends_with("; 0.250; -0.100; STP1; NP"));
        assert!(zeilen[14].ends_with("; 0.000; 0.000; STP1; AP"));
        assert!(zeilen[15].starts_with("# STATIONIERUNG: SP=STP1  IH=1.556 m  R=1.000"));
        assert!(zeilen[15].ends_with("n=6  2s0=10.00 mm"));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn alte_siebenspaltige_datei_wird_aufgeruestet() {
        let (e, dir) = einst("mig");
        let alt = format!("# METADATA\r\nProjekt: Demo\r\n---\r\n# DATEN\r\n{SPALTENZEILE_ALT}\r\n1; 2; 3; 4; 5; c; 2026-01-01 10:00\r\n");
        fs::write(messdatei_pfad(&e), alt).unwrap();
        let roh = NeupunktRohdaten { punkt_nr: "9".into(), zeitstempel: jetzt(), ..Default::default() };
        schreibe_neupunktmessung(&e, &roh, "S", 0.0);
        let text = fs::read_to_string(messdatei_pfad(&e)).unwrap();
        assert!(text.contains(SPALTENZEILE));
        assert!(text.contains("1; 2; 3; 4; 5; c; 2026-01-01 10:00; 0.000; 0.000; ; "));
        let sicherungen = fs::read_dir(&dir).unwrap().filter_map(Result::ok).filter(|d| d.file_name().to_string_lossy().contains("_backup_")).count();
        assert_eq!(sicherungen, 1);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn veraltetes_format_wird_gesichert_und_neu_angelegt() {
        let (e, dir) = einst("alt");
        fs::write(messdatei_pfad(&e), "irgendein altes Format\r\n").unwrap();
        let roh = NeupunktRohdaten { punkt_nr: "1".into(), zeitstempel: jetzt(), ..Default::default() };
        schreibe_neupunktmessung(&e, &roh, "S", 0.0);
        let text = fs::read_to_string(messdatei_pfad(&e)).unwrap();
        assert!(text.starts_with("# METADATA"));
        assert!(fs::read_dir(&dir).unwrap().filter_map(Result::ok).any(|d| d.file_name().to_string_lossy().contains("_backup_")));
        let _ = fs::remove_dir_all(dir);
    }
}
