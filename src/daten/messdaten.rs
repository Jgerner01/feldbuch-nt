// Zweck:    Messdaten festhalten wie die Originalversion:
//           (das lückenlose Messprotokoll schreibt daten::messprotokoll; es ersetzt das frühere Data-protocol-*.csv)
//           • Messdatei {Projekt}-dat.csv mit METADATA-Kopf (Anschluss- und Neupunktmessungen, Standpunktinfo)
// Hardware: Tachymeter (Leica GeoCOM / GSI Online)
// Autor:    Johann Gerner

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use chrono::{Local, NaiveDateTime};

use crate::daten::einstellungen::Einstellungen;
use crate::daten::modelle::{NeupunktRohdaten, StationierungsErgebnis, StationierungsPunkt};
use crate::grundlagen::format::f;
use crate::daten::messprotokoll::{self, Eintrag, Ereignis};
use crate::grundlagen::{fehler_log, protokoll};
use crate::tachymeter::modell::TachymeterModell;

/// Kennzeichen „Anschlusspunkt" in der Spalte Typ.
pub const TYP_ANSCHLUSSPUNKT: &str = "AP";
/// Kennzeichen „Neupunkt" in der Spalte Typ.
pub const TYP_NEUPUNKT: &str = "NP";
/// Kennzeichen „Kontrollmessung" (Orientierungskontrolle) in der Spalte Typ.
pub const TYP_KONTROLLPUNKT: &str = "KP";
/// Kennzeichen „Absteckmessung" (Punktabsteckung im DXF-Viewer) in der Spalte Typ.
pub const TYP_ABSTECKUNG: &str = "AB";

#[cfg(test)]
const SPALTENZEILE_ALT: &str = "PunktNr; HZ_gon; V_gon; Strecke_m; Zielhoehe_m; Code; Zeitstempel";

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

pub(crate) fn messzeit(zeit: NaiveDateTime) -> String {
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

#[cfg(test)]
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

/// Text der Kommentarzeile bei einem Standpunktwechsel.
pub fn standpunktwechsel_text(anlass: &str, s: &crate::daten::projekt::StationsdatenDatei) -> String {
    let mut text = format!("# {anlass}: SP={}  IH={} m  Id={}", s.standpunkt_nr, f(s.instrumenten_hoehe, 3), s.aufstellung_id);
    if !s.code.is_empty() {
        text.push_str(&format!("  Code={}", s.code));
    }
    if let Some(b) = &s.bekannt {
        text.push_str(&format!("  bekannter Punkt {} (R={} H={})", b.punkt_nr, f(b.r, 3), f(b.h, 3)));
    }
    text
}

/// Vermerkt einen Standpunktwechsel (STP_NEU oder STP_FORT) im Messprotokoll, als Kommentarzeile in der Messdatei
/// und im Textprotokoll. Fehler werden nur gemeldet (der Wechsel selbst ist bereits gespeichert).
pub fn schreibe_standpunktwechsel(e: &Einstellungen, s: &crate::daten::projekt::StationsdatenDatei, ereignis: Ereignis) {
    let anlass = if ereignis == Ereignis::StpNeu { "NEUER STANDPUNKT" } else { "STANDPUNKT FORTGESETZT" };
    let text = standpunktwechsel_text(anlass, s);
    let mut eintrag = Eintrag::neu(ereignis).mit_aufstellung(s).quelle(messprotokoll::QUELLE_HAND);
    eintrag.code.clone_from(&s.code);
    if let Some(b) = &s.bekannt {
        eintrag = eintrag.text(format!("bekannter Punkt {}", b.punkt_nr)).koordinaten(b.r, b.h, b.hoehe);
    }
    if let Some(g) = &s.geraet {
        eintrag.edm.clone_from(&g.edm_modus);
        eintrag.prismentyp.clone_from(&g.prisma);
        eintrag.prism_konst = Some(g.prismenkonstante_mm);
        eintrag.ppm = g.atm_ppm;
        eintrag.neig_l = g.laengsneigung_mgon;
        eintrag.neig_q = g.querneigung_mgon;
        eintrag = eintrag.text(format!("Geraet {}{}", g.modell, if g.verbunden { "" } else { " (nicht verbunden)" }));
    }
    messprotokoll::protokolliere(e, eintrag);
    let ergebnis = lade_messdatei(e).and_then(|mut d| {
        d.kommentare.push((d.zeilen.len(), text.clone()));
        d.speichern()
    });
    if let Err(fehler) = ergebnis {
        fehler_log::log("Messdaten.schreibe_standpunktwechsel", &fehler.to_string());
    }
    protokoll::log("STANDPKT", text.trim_start_matches("# "));
    if let Some(g) = &s.geraet {
        protokoll::log("STANDPKT", &format!("Gerätezustand: {}", g.text()));
    }
}

/// Führt eine korrigierte Instrumentenhöhe in der Messdatei nach: alle Zeilen der Aufstellung erhalten die neue
/// Höhe, eine Kommentarzeile vermerkt die Korrektur (die ursprünglichen Werte stehen im Messprotokoll).
/// Liefert die Zahl der geänderten Zeilen; ohne Messdatei geschieht nichts.
pub fn setze_instrumentenhoehe_der_aufstellung(e: &Einstellungen, aufstellung_id: &str, standpunkt_nr: &str, alt: f64, neu: f64) -> io::Result<usize> {
    let pfad = messdatei_pfad(e);
    if aufstellung_id.is_empty() || !pfad.exists() {
        return Ok(0);
    }
    let mut d = lade_messdatei(e)?;
    let mut geaendert = 0;
    for i in 0..d.zeilen.len() {
        if d.zelle(&d.zeilen[i], "AufstellungId") == aufstellung_id && d.zelle(&d.zeilen[i], "InstrumentenHoehe_m") != f(neu, 3) {
            d.set_zelle(i, "InstrumentenHoehe_m", f(neu, 3));
            geaendert += 1;
        }
    }
    let alle_dieser_aufstellung = !d.zeilen.is_empty() && d.zeilen.iter().all(|z| d.zelle(z, "AufstellungId") == aufstellung_id);
    if alle_dieser_aufstellung {
        d.instr_hoehe = f(neu, 3);
    }
    d.kommentare.push((d.zeilen.len(), format!("# IH KORRIGIERT: SP={standpunkt_nr}  IH={} -> {} m  Id={aufstellung_id}  ({geaendert} Zeile(n))", f(alt, 3), f(neu, 3))));
    d.speichern()?;
    Ok(geaendert)
}

/// Schreibt eine Kontrollmessung (Orientierungskontrolle) als Zeile vom Typ `KP` in die Messdatei.
pub fn schreibe_kontrollmessung(e: &Einstellungen, p: &StationierungsPunkt, standpunkt_nr: &str, instr_hoehe: f64, aufstellung_id: &str, zeit: NaiveDateTime) -> io::Result<()> {
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
            TYP_KONTROLLPUNKT.into(),
        ],
        standpunkt_nr,
        instr_hoehe,
        aufstellung_id,
    );
    ergebnis.inspect_err(|e| fehler_log::log("Messdaten.schreibe_kontrollmessung", &e.to_string()))
}

/// Schreibt eine Absteckmessung als Zeile vom Typ `AB` in die Messdatei (die Polarpunktberechnung überspringt sie).
pub fn schreibe_absteckmessung(e: &Einstellungen, p: &StationierungsPunkt, code: &str, standpunkt_nr: &str, instr_hoehe: f64, aufstellung_id: &str, zeit: NaiveDateTime) -> io::Result<()> {
    let ergebnis = schreibe_messung(
        e,
        vec![
            p.punkt_nr.clone(),
            f(p.hz, 4),
            f(p.v, 4),
            f(p.strecke, 3),
            f(p.zielhoehe, 3),
            code.into(),
            messzeit(zeit),
            "0.000".into(),
            "0.000".into(),
            standpunkt_nr.into(),
            TYP_ABSTECKUNG.into(),
        ],
        standpunkt_nr,
        instr_hoehe,
        aufstellung_id,
    );
    ergebnis.inspect_err(|e| fehler_log::log("Messdaten.schreibe_absteckmessung", &e.to_string()))
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

/// Eine zurückzuschreibende Exzentrizität. `datensatz` ist der Index unter allen Datenzeilen der Messdatei
/// (einschließlich AP/KP); Punktnummer und Zeitstempel prüfen, dass die Zeile noch dieselbe Messung ist.
#[derive(Clone, Debug, PartialEq)]
pub struct ExzAenderung {
    pub datensatz: usize,
    pub punkt_nr: String,
    pub zeitstempel: String,
    pub laengs: f64,
    pub quer: f64,
}

/// Schreibt Exzentrizitäten zurück (audit.md P3): erst alle Ziele prüfen, dann einmal speichern. Passt eine Zeile
/// nicht mehr zur geladenen Messung (Datei extern geändert), wird nichts geschrieben.
pub fn schreibe_exzentrizitaeten(pfad: &Path, aenderungen: &[ExzAenderung]) -> io::Result<()> {
    let mut d = crate::daten::csv_datei::CsvDatei::laden(pfad)?;
    for a in aenderungen {
        if !a.laengs.is_finite() || !a.quer.is_finite() {
            return Err(crate::daten::csv_basis::fehler("Exzentrizität ist keine endliche Zahl."));
        }
        let passt = d.zeilen.get(a.datensatz).is_some_and(|z| d.zelle(z, "PunktNr") == a.punkt_nr && d.zelle(z, "Zeitstempel") == a.zeitstempel);
        if !passt {
            return Err(crate::daten::csv_basis::fehler(format!("Die Messung {} wurde in der Datei verändert oder verschoben. Bitte die Datei neu laden.", a.punkt_nr)));
        }
    }
    for a in aenderungen {
        d.set_zelle(a.datensatz, "ExzLaengs_m", f(a.laengs, 3));
        d.set_zelle(a.datensatz, "ExzQuer_m", f(a.quer, 3));
    }
    d.speichern()
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
    fn standpunktwechsel_steht_in_messdatei_und_messprotokoll() {
        let (e, dir) = einst("wechsel");
        let s = crate::daten::projekt::StationsdatenDatei { standpunkt_nr: "STP3".into(), aufstellung_id: "STP3-20261122T101500".into(), instrumenten_hoehe: 1.6, ..Default::default() };
        schreibe_standpunktwechsel(&e, &s, Ereignis::StpNeu);
        let dat = fs::read_to_string(messdatei_pfad(&e)).unwrap();
        assert!(dat.contains("# NEUER STANDPUNKT: SP=STP3  IH=1.600 m  Id=STP3-20261122T101500"));
        let mp = messprotokoll::lese(&messprotokoll::pfad(&e)).unwrap();
        assert_eq!(mp[0].art(), Some(Ereignis::StpNeu));
        assert_eq!((mp[0].standpunkt_nr.as_str(), mp[0].ih), ("STP3", Some(1.6)));
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
    fn exzentrizitaet_trifft_nur_den_gewaehlten_neupunkt() {
        // Audit P3: NP1, KP, NP2 – zweimal die Exzentrizität von NP2 ändern; KP und NP1 bleiben unverändert
        let (e, dir) = einst("p3");
        let roh = |nr: &str, s: i64| NeupunktRohdaten { punkt_nr: nr.into(), strecke_m: 10.0, v_gon: 100.0, aufstellung_id: "A".into(), zeitstempel: jetzt() + chrono::Duration::seconds(s), ..Default::default() };
        schreibe_neupunktmessung(&e, &roh("NP1", 0), "S1", 1.5).unwrap();
        let kp = StationierungsPunkt { punkt_nr: "K".into(), hz: 1.0, v: 100.0, strecke: 20.0, ..Default::default() };
        schreibe_kontrollmessung(&e, &kp, "S1", 1.5, "A", jetzt() + chrono::Duration::seconds(1)).unwrap();
        schreibe_neupunktmessung(&e, &roh("NP2", 2), "S1", 1.5).unwrap();
        let pfad = messdatei_pfad(&e);
        let polar = crate::daten::polar_messdaten::lese(&pfad, |_| false).unwrap();
        let np2 = polar.messungen.iter().find(|m| m.punkt_nr == "NP2").unwrap().clone();
        for wert in [1.0, 2.0] {
            let a = ExzAenderung { datensatz: np2.datensatz, punkt_nr: np2.punkt_nr.clone(), zeitstempel: np2.zeitstempel.clone(), laengs: wert, quer: 0.0 };
            schreibe_exzentrizitaeten(&pfad, &[a]).unwrap();
        }
        let d = crate::daten::csv_datei::CsvDatei::laden(&pfad).unwrap();
        let exz = |nr: &str| d.zeilen.iter().find(|z| d.zelle(z, "PunktNr") == nr).map(|z| d.zelle(z, "ExzLaengs_m").to_string()).unwrap();
        assert_eq!(exz("NP2"), "2.000");
        assert_eq!(exz("K"), "0.000");
        assert_eq!(exz("NP1"), "0.000");
        // Extern verschobene Zeilen: Konflikt statt falscher Zeile, nichts wird geschrieben
        let falsch = ExzAenderung { datensatz: 0, punkt_nr: "NP2".into(), zeitstempel: np2.zeitstempel.clone(), laengs: 9.0, quer: 0.0 };
        assert!(schreibe_exzentrizitaeten(&pfad, &[falsch]).is_err());
        let d2 = crate::daten::csv_datei::CsvDatei::laden(&pfad).unwrap();
        assert_eq!(d2.zelle(&d2.zeilen[0], "ExzLaengs_m"), "0.000");
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
