// Zweck:    Konvertierung: editierbare Tabelle mit den Feldbuchpunkten, Import aus GSI und DXF mit Duplikaterkennung
//           und automatischer Nummernvergabe, Export als CSV, KOR und DAT
// Hardware: keine
// Autor:    Johann Gerner

use std::path::Path;

use eframe::egui::{self, RichText};

use super::tabelle::Raster;
use crate::daten::konvertierung::{self, KonvertierungPunkt};
use crate::daten::projekt::Projekt;
use crate::grundlagen::datei_speicher;
use crate::grundlagen::format::{f, parse_zahl};

/// Spaltenbreiten: Auswahl, Nr, Typ, R, H, Höhe, Hz, V, Strecke, Zielhöhe, Code, Bemerkung.
const BREITEN: [f32; 12] = [30.0, 70.0, 90.0, 100.0, 100.0, 80.0, 80.0, 80.0, 80.0, 70.0, 80.0, 260.0];
/// Spaltenüberschriften (ohne Auswahlspalte).
const KOPF: [&str; 11] = ["PunktNr", "Typ", "R [m]", "H [m]", "Höhe [m]", "HZ [gon]", "V [gon]", "Strecke [m]", "Zielh. [m]", "Code", "Bemerkung"];
/// Breite der Zeilenköpfe.
const ZEILENKOPF_BREITE: f32 = 30.0;
/// Höhe des Tabellenbereichs.
const TABELLE_HOEHE: f32 = 420.0;
const SPALTE_NR: usize = 0;
const SPALTE_TYP: usize = 1;
const SPALTE_R: usize = 2;
const SPALTE_H: usize = 3;
const SPALTE_HOEHE: usize = 4;
const SPALTE_HZ: usize = 5;
const SPALTE_V: usize = 6;
const SPALTE_STRECKE: usize = 7;
const SPALTE_ZIELH: usize = 8;
const SPALTE_CODE: usize = 9;
const SPALTE_BEM: usize = 10;
/// Anzahl der Tabellenspalten.
const SPALTEN: usize = 11;

/// Formular der Konvertierung.
#[derive(Default)]
pub struct KonvertierungAnsicht {
    zeilen: Vec<[String; SPALTEN]>,
    markiert: Vec<bool>,
    import_datei: String,
    format: String,
    status: String,
}

fn zahl(text: &str) -> f64 {
    parse_zahl(text).unwrap_or(0.0)
}

/// Zahl als Text, 0 als leere Zelle.
fn zelle(wert: f64, stellen: usize) -> String {
    if wert != 0.0 { f(wert, stellen) } else { String::new() }
}

fn projektname(projekt: &Projekt) -> String {
    if projekt.einstellungen.ist_geladen() { projekt.einstellungen.projekt_name.clone() } else { "–".to_string() }
}

fn datum() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

impl KonvertierungAnsicht {
    pub fn neu(projekt: &Projekt) -> Self {
        let mut a = Self::default();
        a.lade_punkte(projekt);
        a
    }

    fn lade_punkte(&mut self, projekt: &Projekt) {
        self.zeilen = projekt
            .feldbuchpunkte
            .punkte()
            .iter()
            .map(|p| {
                let mut z: [String; SPALTEN] = Default::default();
                z[SPALTE_NR] = p.punkt_nr.clone();
                z[SPALTE_TYP] = p.typ.clone();
                z[SPALTE_R] = f(p.r, 3);
                z[SPALTE_H] = f(p.h, 3);
                z[SPALTE_HOEHE] = f(p.hoehe, 3);
                z[SPALTE_BEM] = format!("Quelle={}  Datum={}", p.quelle, p.datum);
                z
            })
            .collect();
        self.markiert = vec![false; self.zeilen.len()];
        self.status = format!("{} Punkte geladen.", self.zeilen.len());
    }

    fn lese_zeilen(&self) -> Vec<KonvertierungPunkt> {
        self.zeilen
            .iter()
            .map(|z| KonvertierungPunkt {
                punkt_nr: z[SPALTE_NR].trim().to_string(),
                typ: z[SPALTE_TYP].trim().to_string(),
                r: zahl(&z[SPALTE_R]),
                h: zahl(&z[SPALTE_H]),
                hoehe: zahl(&z[SPALTE_HOEHE]),
                hz: zahl(&z[SPALTE_HZ]),
                v: zahl(&z[SPALTE_V]),
                strecke: zahl(&z[SPALTE_STRECKE]),
                zielhoehe: zahl(&z[SPALTE_ZIELH]),
                punktcode: z[SPALTE_CODE].trim().to_string(),
                bemerkung: z[SPALTE_BEM].trim().to_string(),
            })
            .collect()
    }

    fn zeile_aus(p: &KonvertierungPunkt) -> [String; SPALTEN] {
        let mut z: [String; SPALTEN] = Default::default();
        z[SPALTE_NR] = p.punkt_nr.clone();
        z[SPALTE_TYP] = p.typ.clone();
        z[SPALTE_R] = zelle(p.r, 3);
        z[SPALTE_H] = zelle(p.h, 3);
        z[SPALTE_HOEHE] = zelle(p.hoehe, 3);
        z[SPALTE_HZ] = zelle(p.hz, 4);
        z[SPALTE_V] = zelle(p.v, 4);
        z[SPALTE_STRECKE] = zelle(p.strecke, 3);
        z[SPALTE_ZIELH] = zelle(p.zielhoehe, 3);
        z[SPALTE_CODE] = p.punktcode.clone();
        z[SPALTE_BEM] = p.bemerkung.clone();
        z
    }

    fn importieren(&mut self) {
        let Some(pfad) = rfd::FileDialog::new()
            .set_title("Datei zum Importieren wählen")
            .add_filter("Alle unterstützten Formate", &["gsi", "dxf"])
            .add_filter("GSI-Datei (Leica)", &["gsi"])
            .add_filter("DXF-Datei", &["dxf"])
            .add_filter("Alle Dateien", &["*"])
            .pick_file()
        else {
            return;
        };
        self.import_datei = pfad.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let endung = pfad.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
        self.format = match endung.as_str() {
            "gsi" => "Format: GSI (Leica)",
            "dxf" => "Format: DXF",
            _ => "Format: unbekannt",
        }
        .to_string();
        self.lese_import(&pfad, &endung);
    }

    fn lese_import(&mut self, pfad: &Path, endung: &str) {
        let punkte = match endung {
            "gsi" => konvertierung::gsi_lese(pfad).map_err(|e| e.to_string()),
            "dxf" => konvertierung::dxf_import(pfad),
            _ => Err(format!("Format '.{endung}' wird nicht unterstützt.")),
        };
        let punkte = match punkte {
            Ok(p) => p,
            Err(fehler) => {
                self.status = format!("Fehler beim Einlesen: {fehler}");
                return;
            }
        };
        let mut tabelle = self.lese_zeilen();
        let vorher = tabelle.len();
        let (hinzugefuegt, doppelt) = konvertierung::importiere_in(&mut tabelle, punkte);
        for p in &tabelle[vorher..] {
            self.zeilen.push(Self::zeile_aus(p));
            self.markiert.push(false);
        }
        let hinweis = if doppelt > 0 { format!(", {doppelt} Duplikat(e) übersprungen.") } else { ".".to_string() };
        self.status = format!("Import: {hinzugefuegt} Punkte hinzugefügt{hinweis}");
    }

    fn markierte_entfernen(&mut self) {
        let mut i = 0;
        self.zeilen.retain(|_| {
            let behalten = !self.markiert.get(i).copied().unwrap_or(false);
            i += 1;
            behalten
        });
        self.markiert = vec![false; self.zeilen.len()];
    }

    /// Fragt einen Zielpfad ab und schreibt den Text.
    fn exportiere(&mut self, titel: &str, filter: (&str, &str), name: &str, text: &str, kurz: &str) {
        let Some(pfad) = rfd::FileDialog::new().set_title(titel).add_filter(filter.0, &[filter.1]).add_filter("Alle Dateien", &["*"]).set_file_name(name).save_file()
        else {
            return;
        };
        match datei_speicher::schreibe_text(&pfad, text) {
            Ok(()) => self.status = format!("{kurz} gespeichert: {}", pfad.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()),
            Err(fehler) => self.status = format!("Fehler beim Speichern: {fehler}"),
        }
    }

    pub fn zeichne(&mut self, ui: &mut egui::Ui, projekt: &Projekt) {
        ui.heading("Konvertierung");
        ui.horizontal_wrapped(|ui| {
            if ui.button("Feldbuchpunkte neu laden").clicked() {
                self.lade_punkte(projekt);
            }
            if ui.button("Datei importieren …").clicked() {
                self.importieren();
            }
            if !self.import_datei.is_empty() {
                ui.label(RichText::new(&self.import_datei).monospace());
                ui.label(&self.format);
            }
        });
        ui.horizontal_wrapped(|ui| {
            if ui.button("+ Zeile").clicked() {
                self.zeilen.push(Default::default());
                self.markiert.push(false);
            }
            if ui.button("− Markierte Zeilen").clicked() {
                self.markierte_entfernen();
            }
            ui.separator();
            if ui.button("Export CSV …").clicked() {
                let text = konvertierung::csv_text(&self.lese_zeilen());
                self.exportiere("CSV speichern", ("CSV-Datei", "csv"), "Punkte.csv", &text, "CSV");
            }
            if ui.button("Export KOR …").clicked() {
                let text = konvertierung::kor_text(&self.lese_zeilen(), &datum(), &projektname(projekt));
                self.exportiere("KOR-Datei speichern", ("Koordinatendatei", "kor"), "Punkte.kor", &text, "KOR");
            }
            if ui.button("Export DAT …").clicked() {
                let (text, mit_standpunkt) = konvertierung::dat_text(&self.lese_zeilen(), &datum(), &projektname(projekt));
                self.exportiere("DAT-Datei speichern", ("Felddatendatei", "dat"), "Punkte.dat", &text, "DAT");
                if !mit_standpunkt && self.status.starts_with("DAT gespeichert") {
                    self.status.push_str(" [Standardstandpunkt STP1 / 0.000 / 005, da kein Standpunkt gefunden wurde]");
                }
            }
        });
        ui.add_space(4.0);

        let raster = Raster::neu(&BREITEN).mit_zeilenkopf(ZEILENKOPF_BREITE);
        let mut kopf: Vec<&str> = vec!["✓"];
        kopf.extend(KOPF);
        egui::ScrollArea::both().id_salt("konv_tabelle").max_height(TABELLE_HOEHE).show(ui, |ui| {
            raster.kopf(ui, &kopf);
            for (i, zeile) in self.zeilen.iter_mut().enumerate() {
                let markiert = &mut self.markiert[i];
                raster.zeile(ui, i, &(i + 1).to_string(), |z| {
                    z.haken(markiert);
                    for zellwert in zeile.iter_mut() {
                        z.eingabe(zellwert);
                    }
                });
            }
        });
        ui.add_space(4.0);
        ui.label(&self.status);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zeilen_werden_gelesen_und_formatiert() {
        let mut a = KonvertierungAnsicht::default();
        let p = KonvertierungPunkt { punkt_nr: "5".into(), typ: "Neupunkt".into(), r: 10.5, h: 20.0, hz: 1.25, ..KonvertierungPunkt::default() };
        a.zeilen.push(KonvertierungAnsicht::zeile_aus(&p));
        a.markiert.push(false);
        assert_eq!(a.zeilen[0][SPALTE_R], "10.500");
        assert_eq!(a.zeilen[0][SPALTE_HOEHE], "", "0 wird als leere Zelle gezeigt");
        let gelesen = a.lese_zeilen();
        assert_eq!(gelesen[0], p);
    }

    #[test]
    fn markierte_zeilen_werden_entfernt() {
        let mut a = KonvertierungAnsicht::default();
        for nr in ["1", "2", "3"] {
            let mut z: [String; SPALTEN] = Default::default();
            z[SPALTE_NR] = nr.to_string();
            a.zeilen.push(z);
        }
        a.markiert = vec![false, true, false];
        a.markierte_entfernen();
        assert_eq!(a.zeilen.iter().map(|z| z[SPALTE_NR].as_str()).collect::<Vec<_>>(), ["1", "3"]);
    }
}
