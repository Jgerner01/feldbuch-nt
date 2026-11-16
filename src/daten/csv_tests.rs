//! Modulübergreifende Prüfungen des Austauschformats.
use super::{csv_datei::*, kor_csv, messdaten, polar_messdaten, punktlisten, stationierung_csv};

fn ordner(name: &str) -> std::path::PathBuf {
    let p = std::env::temp_dir().join(format!("feldbuch-csv-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&p).unwrap();
    p
}

#[test]
fn koordinaten_mit_bom_aliasen_text_und_fehlender_hoehe() {
    let d = CsvDatei::parse("\u{feff}# METADATA\nProjekt: Test; mit Semikolon\nFremdschluessel: erhalten\n# DATEN\nCode;H;PunktNr;Hoehe;R;Zusatz\n\"Baum; \"\"A\"\"\";200,25;001;;100,5;\"Zeile 1\nZeile 2\"\n").unwrap();
    let text = d.text().unwrap();
    let neu = CsvDatei::parse(&text).unwrap();
    assert_eq!(neu.meta("Fremdschluessel"), Some("erhalten"));
    let p = kor_csv::punkte_aus_datei(&neu).unwrap();
    assert_eq!(p[0].punkt_nr, "001");
    assert_eq!((p[0].r, p[0].h, p[0].hoehe), (100.5, 200.25, None));
    assert!(p[0].code.contains("Baum;"));
    assert_eq!(neu.zelle(&neu.zeilen[0], "Zusatz"), "Zeile 1\nZeile 2");
    assert!(text.contains("PunktNr;X;Y;Z;Code;Zusatz"));
}

#[test]
fn kommentar_darf_nachfolgende_messungen_nicht_verschlucken() {
    let text = "# METADATA\nStandpunktNr: SP1\nInstrumentenHoehe: 1.5\n# DATENTYP\nMessdaten\n# DATEN\nPunktNr;HZ;V;Strecke;Zielhoehe\n1;0;100;20;1.5\n# STATIONIERUNG: SP=SP1 IH=1.5\n2;100;100;25;1.5\n";
    let d = CsvDatei::parse(text).unwrap();
    assert_eq!(d.zeilen.len(), 2);
    let neu = CsvDatei::parse(&d.text().unwrap()).unwrap();
    assert_eq!(neu.zeilen.len(), 2);
    assert!(neu.kommentare.iter().any(|(i, z)| *i == 1 && z.starts_with("# STATIONIERUNG:")));
}

#[test]
fn gleiche_standpunktnummer_mit_verschiedenen_aufstellungen() {
    let mut d = CsvDatei::neu(TYP_MESSDATEN);
    for (id, ih) in [("SP1-a", "1.500"), ("SP1-b", "1.700")] {
        d.zeilen.push(["001", "0", "100", "10", "1.5", "", "2026-11-16T10:00:00+01:00", "0", "0", "SP1", "NP", id, ih].map(str::to_string).to_vec());
    }
    let text = d.text().unwrap();
    let p = polar_messdaten::parse(&text.lines().map(str::to_string).collect::<Vec<_>>(), |_| false).unwrap();
    assert_eq!(p.standpunkte().len(), 2);
    assert_eq!(p.messungen[0].instrumenten_hoehe, Some(1.5));
    assert_eq!(p.messungen[1].instrumenten_hoehe, Some(1.7));
    assert_eq!(p.messungen.len(), 2, "gleiche Punktnummer bleibt als Mehrfachmessung erhalten");
}

#[test]
fn stp_erhaelt_np_zusatzfelder_und_ergebnis() {
    let text = "# METADATA\nStandpunktNr: SP1\nInstrumentenHoehe: 1.5\n# DATENTYP\nStandpunkt\n# DATEN\nPunktNr;X;Y;Z;HZ;V;Strecke;Zielhoehe;Typ;Extra\nA;10;20;0;0;100;25;1.5;AP;eins\nA;10;20;0;200;300;25;1.5;AP;zwei\nN;;;;100;100;30;1.5;NP;drei\n# STATIONIERUNG\nX;Y;Z;Orientierung_gon;Massstab;s0_mm;Redundanz\n1;2;3;4;1;0;2\n";
    let i = stationierung_csv::parse(text).unwrap();
    assert_eq!(i.punkte.len(), 2);
    assert_eq!(i.punkte[0].hoehe, 0.0, "bekannte Nullhöhe darf nicht verschwinden");
    let neu = stationierung_csv::parse(&i.dokument.text().unwrap()).unwrap();
    assert_eq!(neu.dokument.zeilen.len(), 3);
    assert_eq!(neu.dokument.stationierung.len(), 2);
    assert_eq!(neu.dokument.zelle(&neu.dokument.zeilen[2], "Extra"), "drei");
}

#[test]
fn fehlerhafte_spalten_und_zahlen_werden_gemeldet() {
    assert!(CsvDatei::parse("PunktNr;R;X;Y\n1;1;1;2").is_err());
    let d = CsvDatei::parse("PunktNr;X;Y\n1;falsch;2").unwrap();
    let e = kor_csv::punkte_aus_datei(&d).unwrap_err().to_string();
    assert!(e.contains("Zeile 2") && e.contains("X"));
    assert!(d.text().is_err());
    assert!(CsvDatei::parse("# METADATA\nFormatversion: 99\n# DATEN\nPunktNr;X;Y\n1;1;2").is_err());
}

#[test]
fn metadaten_und_kommentare_sind_keine_csv_felder() {
    let d = CsvDatei::parse("# METADATA\nNotiz: Firma; \"Name\";\n# DATEN\nPunktNr;X;Y\n1;1;2\n# Kommentar; \"offen\n2;3;4\n").unwrap();
    assert_eq!(d.meta("Notiz"), Some("Firma; \"Name\";"));
    let neu = CsvDatei::parse(&d.text().unwrap()).unwrap();
    assert_eq!(neu.meta("Notiz"), d.meta("Notiz"));
    assert_eq!(neu.zeilen.len(), 2);
}

#[test]
fn exzentrizitaeten_nach_spaltennamen_und_mehrzeiligem_code() {
    let dir = ordner("exz");
    let p = dir.join("Test-dat.csv");
    let text =
        "# DATENTYP\nMessdaten\n# DATEN\nCode;PunktNr;V;HZ;Zielhoehe;Strecke;Extra\n\"Baum\nalt\";001;100;0;1.5;20;behalten\n# STATIONIERUNG: SP=A\nBaum;002;100;10;1.5;25;auch\n";
    std::fs::write(&p, text).unwrap();
    let original = CsvDatei::laden(&p).unwrap();
    messdaten::schreibe_exzentrizitaeten(&p, &[(original.zeilennummern[1] - 1, 0.5, -0.1)]).unwrap();
    let neu = CsvDatei::laden(&p).unwrap();
    assert_eq!(neu.zeilen.len(), 2);
    assert_eq!(neu.zelle(&neu.zeilen[0], "Code"), "Baum\nalt");
    assert_eq!(neu.zelle(&neu.zeilen[1], "ExzLaengs_m"), "0.500");
    assert_eq!(neu.zelle(&neu.zeilen[1], "Extra"), "auch");
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn importpunkte_erhalten_code_und_unbekannte_hoehe_im_json() {
    let dir = ordner("import");
    let p = dir.join("Test-kor.csv");
    std::fs::write(&p, "PunktNr;X;Y;Z;Code\n001;1;2;;\"Baum; Bestand\"\n").unwrap();
    let punkte = punktlisten::lese_csv(&p).unwrap();
    assert_eq!(punkte[0].hoehe_vorhanden, Some(false));
    assert_eq!(punkte[0].code, "Baum; Bestand");
    let json = dir.join("ImportPunkte.json");
    punktlisten::ImportPunkteManager::initialisiere(&json).add_range(punkte).unwrap();
    let m = punktlisten::ImportPunkteManager::initialisiere(&json);
    assert_eq!(m.punkte()[0].hoehe_vorhanden, Some(false));
    assert_eq!(m.punkte()[0].code, "Baum; Bestand");
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn zusammenfuehren_erhaelt_fremdspalten_und_metadaten() {
    let dir = ordner("merge");
    let p = dir.join("Test-kor.csv");
    std::fs::write(&p, "# METADATA\nEigentuemer: Test\n# DATEN\nPunktNr;X;Y;Z;Code;Zusatz\n1;1;2;;alt;behalten\n2;3;4;0;FP;auch\n").unwrap();
    kor_csv::schreibe(&p, "Demo", "", &[kor_csv::KorPunkt { punkt_nr: "1".into(), r: 5.0, h: 6.0, hoehe: None, code: "neu;Text".into() }], true).unwrap();
    let d = CsvDatei::laden(&p).unwrap();
    assert_eq!(d.zeilen.len(), 2);
    assert_eq!(d.meta("Eigentuemer"), Some("Test"));
    assert_eq!(d.zelle(&d.zeilen[0], "Zusatz"), "behalten");
    assert_eq!(d.zelle(&d.zeilen[0], "Code"), "neu;Text");
    assert_eq!(d.zelle(&d.zeilen[0], "Z"), "");
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn aufstellungen_bleiben_auch_im_neupunktspeicher_getrennt() {
    let dir = ordner("aufstellungen");
    let datei = dir.join("P-Neupunkte.json");
    let mut m = punktlisten::NeupunkteManager::initialisiere(&datei);
    for id in ["SP1-a", "SP1-b"] {
        let roh = super::modelle::NeupunktRohdaten { punkt_nr: "1".into(), standpunkt_nr: "SP1".into(), aufstellung_id: id.into(), ..Default::default() };
        let erg = super::modelle::NeupunktErgebnis { punkt_nr: "1".into(), standpunkt_nr: "SP1".into(), aufstellung_id: id.into(), ..Default::default() };
        m.hinzufuegen_oder_ersetzen(roh, erg).unwrap();
    }
    assert_eq!(m.koordinaten().len(), 2);
    assert!(m.finde_koordinate_aufstellung("1", "SP1", "SP1-b").is_some());
    m.remove_where(|r| r.aufstellung_id == "SP1-a").unwrap();
    assert_eq!(m.koordinaten().len(), 1);
    assert_eq!(m.koordinaten()[0].aufstellung_id, "SP1-b");
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn beispiele_sind_lesbar() {
    let kor = CsvDatei::parse(include_str!("../../beispiele/csv/Demo-kor.csv")).unwrap();
    assert_eq!(kor_csv::punkte_aus_datei(&kor).unwrap().len(), 2);
    let stp = stationierung_csv::parse(include_str!("../../beispiele/csv/Demo-SP01-stp.csv")).unwrap();
    assert_eq!(stp.punkte.len(), 3);
    let dat = include_str!("../../beispiele/csv/Demo-dat.csv");
    let d = polar_messdaten::parse(&dat.lines().map(str::to_string).collect::<Vec<_>>(), |_| false).unwrap();
    assert_eq!(d.standpunkte().len(), 2);
}

#[test]
fn eine_aufstellung_darf_keine_widerspruechlichen_instrumentenhoehen_haben() {
    let mut d = CsvDatei::neu(TYP_MESSDATEN);
    for ih in ["1.500", "1.700"] {
        d.zeilen.push(["001", "0", "100", "10", "1.5", "", "", "0", "0", "SP1", "NP", "gleiche-ID", ih].map(str::to_string).to_vec());
    }
    assert!(polar_messdaten::parse(&d.text().unwrap().lines().map(str::to_string).collect::<Vec<_>>(), |_| false).is_err());
}
