// Zweck:    Verwaltung der Punktlisten: Feldbuchpunkte, Importpunkte (KOR/CSV/JSON) und Neupunkte
// Hardware: keine
// Autor:    Johann Gerner

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::daten::json_speicher;
use crate::daten::modelle::{FeldbuchDaten, FeldbuchPunkt, ImportPunkt, ImportPunkteDaten, NeupunktErgebnis, NeupunktRohdaten, StationierungsErgebnis, heute_text};
use crate::daten::rechenparameter::Rechenparameter;
use crate::grundlagen::datei_speicher;
use crate::grundlagen::format::parse_zahl;
use crate::rechnen::neupunkt;

/// Kennzeichnung für im Polarmodul berechnete Punkte.
pub const QUELLE_POLAR: &str = "Polar";
/// Toleranz der Duplikaterkennung bei Koordinaten [m].
const DUPLIKAT_TOLERANZ_M: f64 = 0.001;
/// Mindestanzahl Spalten einer KOR-Zeile (Nr, R, H).
const MIN_SPALTEN_KOR: usize = 3;
/// Kommentarzeichen in KOR- und CSV-Dateien.
const KOMMENTARZEICHEN: [char; 3] = ['%', '#', ';'];

/// Feldbuchpunkte.json: berechnete Standpunkte und Neupunkte.
#[derive(Default)]
pub struct FeldbuchpunkteManager {
    pfad: PathBuf,
    punkte: Vec<FeldbuchPunkt>,
}

impl FeldbuchpunkteManager {
    /// Lädt die Datei (falls vorhanden).
    pub fn initialisiere(pfad: &Path) -> Self {
        let mut m = Self { pfad: pfad.to_path_buf(), punkte: Vec::new() };
        if pfad.exists() {
            m.punkte = json_speicher::lade_oder_melde::<FeldbuchDaten>(pfad).punkte;
        } else {
            datei_speicher::laden_erfolgreich(pfad);
        }
        m
    }

    pub fn punkte(&self) -> &[FeldbuchPunkt] {
        &self.punkte
    }

    /// Fügt einen Punkt hinzu oder ersetzt den Punkt mit gleicher Nummer und gleichem Typ.
    pub fn add_or_update(&mut self, punkt: FeldbuchPunkt) -> io::Result<()> {
        datei_speicher::pruefe_schreibzugriff(&self.pfad)?;
        let vorher = self.punkte.clone();
        match self.punkte.iter().position(|p| p.punkt_nr == punkt.punkt_nr && p.typ == punkt.typ) {
            Some(i) => self.punkte[i] = punkt,
            None => self.punkte.push(punkt),
        }
        self.speichern().inspect_err(|_| self.punkte = vorher)
    }

    /// Entfernt Punkte einer Nummer (optional nur eines Typs).
    pub fn remove(&mut self, punkt_nr: &str, typ: &str) -> io::Result<()> {
        self.remove_where(|p| p.punkt_nr == punkt_nr && (typ.is_empty() || p.typ == typ)).map(|_| ())
    }

    /// Entfernt alle Punkte, die das Prädikat erfüllen; liefert die Anzahl.
    pub fn remove_where(&mut self, bedingung: impl Fn(&FeldbuchPunkt) -> bool) -> io::Result<usize> {
        datei_speicher::pruefe_schreibzugriff(&self.pfad)?;
        let vorher = self.punkte.clone();
        self.punkte.retain(|p| !bedingung(p));
        let entfernt = vorher.len() - self.punkte.len();
        if entfernt > 0 {
            self.speichern().inspect_err(|_| self.punkte = vorher)?;
        }
        Ok(entfernt)
    }

    pub fn speichern(&self) -> io::Result<()> {
        json_speicher::speichere(&self.pfad, &FeldbuchDaten { punkte: self.punkte.clone() }, "FeldbuchpunkteManager.speichern")
    }
}

/// ImportPunkte.json: importierte Punkte mit Duplikaterkennung.
#[derive(Default)]
pub struct ImportPunkteManager {
    pfad: PathBuf,
    punkte: Vec<ImportPunkt>,
}

impl ImportPunkteManager {
    pub fn initialisiere(pfad: &Path) -> Self {
        let mut m = Self { pfad: pfad.to_path_buf(), punkte: Vec::new() };
        if pfad.exists() {
            m.punkte = json_speicher::lade_oder_melde::<ImportPunkteDaten>(pfad).punkte;
        } else {
            datei_speicher::laden_erfolgreich(pfad);
        }
        m
    }

    pub fn punkte(&self) -> &[ImportPunkt] {
        &self.punkte
    }

    /// Fügt Punkte hinzu und überspringt Duplikate; liefert (hinzugefügt, doppelt).
    pub fn add_range(&mut self, neu: Vec<ImportPunkt>) -> io::Result<(usize, usize)> {
        datei_speicher::pruefe_schreibzugriff(&self.pfad)?;
        let vorher = self.punkte.clone();
        let (mut hinzu, mut doppelt) = (0, 0);
        for p in neu {
            if let Some(v) = self.punkte.iter().find(|v| v.punkt_nr.eq_ignore_ascii_case(&p.punkt_nr)) {
                if (v.r - p.r).abs() >= DUPLIKAT_TOLERANZ_M
                    || (v.h - p.h).abs() >= DUPLIKAT_TOLERANZ_M
                    || v.hoehe_vorhanden.unwrap_or(true) != p.hoehe_vorhanden.unwrap_or(true)
                    || (v.hoehe - p.hoehe).abs() >= DUPLIKAT_TOLERANZ_M
                    || v.code != p.code
                {
                    let nr = p.punkt_nr.clone();
                    self.punkte = vorher;
                    return Err(crate::daten::csv_basis::fehler(format!("Koordinatenkonflikt für Punkt {nr}. Bestehender Punkt wurde beibehalten.")));
                }
                doppelt += 1;
            } else {
                self.punkte.push(p);
                hinzu += 1;
            }
        }
        if hinzu > 0 {
            self.speichern().inspect_err(|_| self.punkte = vorher)?;
        }
        Ok((hinzu, doppelt))
    }

    pub fn clear(&mut self) -> io::Result<()> {
        self.remove_where(|_| true).map(|_| ())
    }

    pub fn remove_where(&mut self, bedingung: impl Fn(&ImportPunkt) -> bool) -> io::Result<usize> {
        datei_speicher::pruefe_schreibzugriff(&self.pfad)?;
        let vorher = self.punkte.clone();
        self.punkte.retain(|p| !bedingung(p));
        let entfernt = vorher.len() - self.punkte.len();
        if entfernt > 0 {
            self.speichern().inspect_err(|_| self.punkte = vorher)?;
        }
        Ok(entfernt)
    }

    pub fn speichern(&self) -> io::Result<()> {
        json_speicher::speichere(&self.pfad, &ImportPunkteDaten { punkte: self.punkte.clone() }, "ImportPunkteManager.speichern")
    }
}

fn ist_kommentar_oder_leer(zeile: &str) -> bool {
    zeile.is_empty() || zeile.starts_with(KOMMENTARZEICHEN)
}

fn dateiname(pfad: &Path) -> String {
    pfad.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

/// Liest eine KOR-Datei (PunktNr R H [Höhe], durch Leerzeichen getrennt).
/// Existiert eine Begleit-CSV `{name}-kor.csv` mit mindestens ebenso vielen Punkten, wird diese bevorzugt.
pub fn lese_kor(pfad: &Path) -> io::Result<Vec<ImportPunkt>> {
    let roh = lese_kor_roh(pfad)?;
    let begleit = pfad.with_file_name(format!("{}-kor.csv", pfad.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()));
    if begleit.exists() {
        if let Ok(csv) = lese_csv(&begleit) {
            return Ok(if csv.len() >= roh.len() { csv } else { roh });
        }
    }
    Ok(roh)
}

pub fn lese_kor_roh(pfad: &Path) -> io::Result<Vec<ImportPunkt>> {
    let text = fs::read_to_string(pfad)?;
    let quelle = dateiname(pfad);
    let datum = heute_text();
    let mut ergebnis = Vec::new();
    for roh in text.lines() {
        let zeile = roh.trim();
        if ist_kommentar_oder_leer(zeile) {
            continue;
        }
        let teile: Vec<&str> = zeile.split_whitespace().collect();
        if teile.len() < MIN_SPALTEN_KOR {
            continue;
        }
        let (Some(r), Some(h)) = (parse_zahl(teile[1]), parse_zahl(teile[2])) else {
            continue;
        };
        let hoehe = teile.get(3).and_then(|t| parse_zahl(t)).unwrap_or(0.0);
        ergebnis.push(ImportPunkt {
            punkt_nr: teile[0].to_string(),
            r,
            h,
            hoehe,
            quelle: quelle.clone(),
            datum: datum.clone(),
            code: teile.get(4).unwrap_or(&"").to_string(),
            hoehe_vorhanden: Some(teile.get(3).and_then(|t| parse_zahl(t)).is_some()),
        });
    }
    Ok(ergebnis)
}

/// Koordinatenimport mit gemeinsamer Syntax und namentlicher Spaltenzuordnung.
pub fn lese_csv(pfad: &Path) -> io::Result<Vec<ImportPunkt>> {
    let punkte = crate::daten::kor_csv::lese_punkte(pfad)?;
    Ok(punkte
        .into_iter()
        .map(|p| ImportPunkt {
            punkt_nr: p.punkt_nr,
            r: p.r,
            h: p.h,
            hoehe: p.hoehe.unwrap_or(0.0),
            hoehe_vorhanden: Some(p.hoehe.is_some()),
            code: p.code,
            quelle: dateiname(pfad),
            datum: heute_text(),
        })
        .collect())
}

/// Liest eine JSON-Datei im Import- oder Feldbuchformat.
pub fn lese_json(pfad: &Path) -> io::Result<Vec<ImportPunkt>> {
    let quelle = dateiname(pfad);
    if let Ok(daten) = json_speicher::lade::<ImportPunkteDaten>(pfad) {
        if !daten.punkte.is_empty() {
            return Ok(daten
                .punkte
                .into_iter()
                .map(|mut p| {
                    if p.quelle.is_empty() {
                        p.quelle = quelle.clone();
                    }
                    p
                })
                .collect());
        }
    }
    if let Ok(daten) = json_speicher::lade::<FeldbuchDaten>(pfad) {
        return Ok(daten
            .punkte
            .into_iter()
            .map(|p| ImportPunkt { punkt_nr: p.punkt_nr, r: p.r, h: p.h, hoehe: p.hoehe, quelle: quelle.clone(), datum: p.datum, code: String::new(), hoehe_vorhanden: None })
            .collect());
    }
    Ok(Vec::new())
}

/// Dateiinhalt der Neupunktdatei.
#[derive(serde::Serialize, serde::Deserialize, Clone, Default)]
pub struct NeupunkteDatei {
    #[serde(rename = "Rohdaten")]
    pub rohdaten: Vec<NeupunktRohdaten>,
    #[serde(rename = "Koordinaten")]
    pub koordinaten: Vec<NeupunktErgebnis>,
}

/// {Projekt}-Neupunkte.json: Rohmessungen und berechnete Koordinaten.
#[derive(Default)]
pub struct NeupunkteManager {
    pfad: PathBuf,
    daten: NeupunkteDatei,
}

impl NeupunkteManager {
    pub fn initialisiere(pfad: &Path) -> Self {
        let mut m = Self { pfad: pfad.to_path_buf(), daten: NeupunkteDatei::default() };
        if pfad.exists() {
            m.daten = json_speicher::lade_oder_melde(pfad);
        } else {
            datei_speicher::laden_erfolgreich(pfad);
        }
        m
    }

    pub fn rohdaten(&self) -> &[NeupunktRohdaten] {
        &self.daten.rohdaten
    }

    pub fn koordinaten(&self) -> &[NeupunktErgebnis] {
        &self.daten.koordinaten
    }

    /// Fügt einen Neupunkt hinzu oder ersetzt den mit gleicher Nummer und gleichem Standpunkt.
    pub fn hinzufuegen_oder_ersetzen(&mut self, roh: NeupunktRohdaten, ergebnis: NeupunktErgebnis) -> io::Result<()> {
        datei_speicher::pruefe_schreibzugriff(&self.pfad)?;
        let vorher = self.daten.clone();
        match self.daten.rohdaten.iter().position(|r| r.punkt_nr == roh.punkt_nr && r.standpunkt_nr == roh.standpunkt_nr && r.aufstellung_id == roh.aufstellung_id) {
            Some(i) => self.daten.rohdaten[i] = roh,
            None => self.daten.rohdaten.push(roh),
        }
        match self
            .daten
            .koordinaten
            .iter()
            .position(|e| e.punkt_nr == ergebnis.punkt_nr && e.standpunkt_nr == ergebnis.standpunkt_nr && e.aufstellung_id == ergebnis.aufstellung_id)
        {
            Some(i) => self.daten.koordinaten[i] = ergebnis,
            None => self.daten.koordinaten.push(ergebnis),
        }
        self.speichern().inspect_err(|_| self.daten = vorher)
    }

    /// (Anzahl Neupunkte, davon Polarpunkte) eines Standpunkts.
    pub fn zaehle(&self, standpunkt_nr: &str) -> (usize, usize) {
        let alle = self.daten.rohdaten.iter().filter(|r| r.standpunkt_nr == standpunkt_nr).count();
        let polar = self.daten.koordinaten.iter().filter(|e| e.standpunkt_nr == standpunkt_nr && e.quelle == QUELLE_POLAR).count();
        (alle, polar)
    }

    pub fn finde_koordinate(&self, punkt_nr: &str, standpunkt_nr: &str) -> Option<&NeupunktErgebnis> {
        self.daten.koordinaten.iter().find(|e| e.punkt_nr == punkt_nr && e.standpunkt_nr == standpunkt_nr)
    }

    pub fn finde_koordinate_aufstellung(&self, punkt_nr: &str, standpunkt_nr: &str, aufstellung_id: &str) -> Option<&NeupunktErgebnis> {
        self.daten.koordinaten.iter().find(|e| e.punkt_nr == punkt_nr && e.standpunkt_nr == standpunkt_nr && e.aufstellung_id == aufstellung_id)
    }

    /// Berechnet alle Neupunkte eines Standpunkts (auch die im Polarmodul berechneten) mit der neuen
    /// Stationierung neu und speichert die Datei.
    /// Liefert (Anzahl neu berechneter Punkte, davon Polarpunkte).
    pub fn berechne_neu(
        &mut self,
        station: &StationierungsErgebnis,
        standpunkt_nr: &str,
        params: &Rechenparameter,
        instr_hoehe: f64,
        aufstellung_id: &str,
    ) -> io::Result<(usize, usize)> {
        datei_speicher::pruefe_schreibzugriff(&self.pfad)?;
        neupunkt::pruefe_station(station).map_err(io::Error::other)?;
        let vorher = self.daten.clone();
        let polar_nr: std::collections::HashSet<String> =
            self.daten.koordinaten.iter().filter(|e| e.standpunkt_nr == standpunkt_nr && e.quelle == QUELLE_POLAR).map(|e| e.punkt_nr.clone()).collect();
        let mut neue = neupunkt::berechne_alle(
            self.daten.rohdaten.iter().filter(|r| r.aufstellung_id.is_empty() || r.aufstellung_id == aufstellung_id),
            station,
            standpunkt_nr,
            params,
            instr_hoehe,
        );
        // Polarpunkte bleiben als solche gekennzeichnet
        for e in neue.iter_mut().filter(|e| polar_nr.contains(&e.punkt_nr)) {
            e.quelle = QUELLE_POLAR.to_string();
        }
        let neu_nr: std::collections::HashSet<&str> = neue.iter().map(|e| e.punkt_nr.as_str()).collect();
        // Alte Koordinaten des Standpunkts entfallen; nicht berechenbare Polarpunkte behalten ihren letzten Wert
        self.daten.koordinaten.retain(|e| {
            !(e.standpunkt_nr == standpunkt_nr
                && (e.aufstellung_id.is_empty() || e.aufstellung_id == aufstellung_id)
                && (e.quelle != QUELLE_POLAR || neu_nr.contains(e.punkt_nr.as_str())))
        });
        let anzahl = (neue.len(), neue.iter().filter(|e| e.quelle == QUELLE_POLAR).count());
        self.daten.koordinaten.extend(neue);
        self.speichern().inspect_err(|_| self.daten = vorher)?;
        Ok(anzahl)
    }

    /// Entfernt Rohdaten (und Koordinaten) aller Punkte, die das Prädikat erfüllen.
    pub fn remove_where(&mut self, bedingung: impl Fn(&NeupunktRohdaten) -> bool) -> io::Result<usize> {
        datei_speicher::pruefe_schreibzugriff(&self.pfad)?;
        let vorher = self.daten.clone();
        let schluessel: std::collections::HashSet<(String, String, String)> =
            self.daten.rohdaten.iter().filter(|r| bedingung(r)).map(|r| (r.punkt_nr.clone(), r.standpunkt_nr.clone(), r.aufstellung_id.clone())).collect();
        let vor = self.daten.rohdaten.len();
        self.daten.rohdaten.retain(|r| !schluessel.contains(&(r.punkt_nr.clone(), r.standpunkt_nr.clone(), r.aufstellung_id.clone())));
        self.daten.koordinaten.retain(|e| !schluessel.contains(&(e.punkt_nr.clone(), e.standpunkt_nr.clone(), e.aufstellung_id.clone())));
        let entfernt = vor - self.daten.rohdaten.len();
        if entfernt > 0 {
            self.speichern().inspect_err(|_| self.daten = vorher)?;
        }
        Ok(entfernt)
    }

    pub fn speichern(&self) -> io::Result<()> {
        json_speicher::speichere(&self.pfad, &self.daten, "NeupunkteManager.speichern")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daten::modelle::jetzt;

    fn test_ordner(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("fb-pl-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn feldbuchpunkte_roundtrip_und_ersetzen() {
        let dir = test_ordner("fbp");
        let pfad = dir.join("Feldbuchpunkte.json");
        let mut m = FeldbuchpunkteManager::initialisiere(&pfad);
        let p = FeldbuchPunkt { punkt_nr: "S1".into(), typ: "Standpunkt".into(), r: 1.0, ..Default::default() };
        m.add_or_update(p.clone()).unwrap();
        m.add_or_update(FeldbuchPunkt { r: 2.0, ..p }).unwrap();
        let neu = FeldbuchpunkteManager::initialisiere(&pfad);
        assert_eq!(neu.punkte().len(), 1);
        assert_eq!(neu.punkte()[0].r, 2.0);
        let json = fs::read_to_string(&pfad).unwrap();
        assert!(json.contains("\"PunktNr\": \"S1\"") && json.contains("\"IstBerechnung3D\""));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn csharp_json_wird_gelesen() {
        let dir = test_ordner("cs");
        let pfad = dir.join("Feldbuchpunkte.json");
        fs::write(
            &pfad,
            r#"{"Punkte":[{"PunktNr":"7","Typ":"Neupunkt","R":3500000.5,"H":5400000.25,"Hoehe":410.1,
            "Orientierung_gon":0,"IstBerechnung3D":true,"Datum":"2026-01-02","Quelle":"Lokal"}]}"#,
        )
        .unwrap();
        let m = FeldbuchpunkteManager::initialisiere(&pfad);
        assert_eq!(m.punkte()[0].punkt_nr, "7");
        assert_eq!(m.punkte()[0].r, 3500000.5);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn defekte_datei_sperrt_schreiben() {
        let dir = test_ordner("defekt");
        let pfad = dir.join("Feldbuchpunkte.json");
        fs::write(&pfad, "{ kaputt").unwrap();
        let mut m = FeldbuchpunkteManager::initialisiere(&pfad);
        assert!(m.punkte().is_empty());
        assert!(m.add_or_update(FeldbuchPunkt::default()).is_err());
        assert_eq!(fs::read_to_string(&pfad).unwrap(), "{ kaputt");
        datei_speicher::laden_erfolgreich(&pfad);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn import_erkennt_duplikate() {
        let dir = test_ordner("imp");
        let mut m = ImportPunkteManager::initialisiere(&dir.join("ImportPunkte.json"));
        let a = ImportPunkt { punkt_nr: "1".into(), r: 10.0, h: 20.0, ..Default::default() };
        let gleiche_nr = ImportPunkt { punkt_nr: "1".into(), r: 99.0, h: 99.0, ..Default::default() };
        let gleiche_lage = ImportPunkt { punkt_nr: "2".into(), r: 10.0005, h: 20.0, ..Default::default() };
        let neu = ImportPunkt { punkt_nr: "3".into(), r: 11.0, h: 20.0, ..Default::default() };
        assert_eq!(m.add_range(vec![a.clone()]).unwrap(), (1, 0));
        assert!(m.add_range(vec![neu.clone(), gleiche_nr]).is_err());
        assert_eq!(m.punkte().len(), 1, "Konflikt muss gesamten Import zurückrollen");
        assert_eq!(m.add_range(vec![a, gleiche_lage, neu]).unwrap(), (2, 1));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn csv_und_kor_werden_gelesen() {
        let dir = test_ordner("lesen");
        let csv = dir.join("p.csv");
        fs::write(&csv, "Projekt: Test\nPunktNr;R;H;Hoehe\n1;100,5;200,25;300\n2;101;201\n").unwrap();
        let p = lese_csv(&csv).unwrap();
        assert_eq!(p.len(), 2);
        assert_eq!(p[0].r, 100.5);
        assert_eq!(p[0].hoehe, 300.0);
        assert_eq!(p[1].hoehe, 0.0);

        let kor = dir.join("p.kor");
        fs::write(&kor, "% Kommentar\n10 1.5 2.5 3.5\nkurz 1\n11 4 5\n").unwrap();
        let k = lese_kor(&kor).unwrap();
        assert_eq!(k.len(), 2);
        assert_eq!(k[0].hoehe, 3.5);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn neupunkte_polarpunkte_werden_mit_neu_berechnet() {
        let dir = test_ordner("np");
        let mut m = NeupunkteManager::initialisiere(&dir.join("P-Neupunkte.json"));
        let roh = NeupunktRohdaten { punkt_nr: "N1".into(), hz_gon: 0.0, v_gon: 100.0, strecke_m: 10.0, standpunkt_nr: "S1".into(), zeitstempel: jetzt(), ..Default::default() };
        let polar = NeupunktErgebnis { punkt_nr: "N1".into(), standpunkt_nr: "S1".into(), quelle: QUELLE_POLAR.into(), r: 123.0, zeitstempel: jetzt(), ..Default::default() };
        m.hinzufuegen_oder_ersetzen(roh, polar).unwrap();
        let station = StationierungsErgebnis { konvergiert: true, massstab: 1.0, ..Default::default() };
        let anzahl = m.berechne_neu(&station, "S1", &Rechenparameter::default(), 0.0, "").unwrap();
        assert_eq!(anzahl, (1, 1));
        assert_eq!(m.koordinaten().len(), 1);
        // Neu aus den Rohdaten berechnet, aber weiterhin als Polarpunkt gekennzeichnet
        assert_ne!(m.koordinaten()[0].r, 123.0);
        assert_eq!(m.koordinaten()[0].quelle, QUELLE_POLAR);

        let neu = NeupunkteManager::initialisiere(&dir.join("P-Neupunkte.json"));
        assert_eq!(neu.rohdaten().len(), 1);
        let _ = fs::remove_dir_all(dir);
    }
}
