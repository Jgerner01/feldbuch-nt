// Zweck:    Konvertierung: editierbare Tabelle mit den Feldbuchpunkten, Import aus GSI, DXF, KOR und DAT mit Duplikaterkennung
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
/// Mindesthöhe des Tabellenbereichs; sonst füllt die Tabelle die verfügbare Fensterhöhe.
const TABELLE_MIN_HOEHE: f32 = 200.0;
/// Platz unter der Tabelle für die Statuszeile.
const STATUS_HOEHE: f32 = 28.0;
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
    /// Projektverzeichnis, zu dem die Tabelle gehört (die Tabelle bleibt beim Verlassen der Seite erhalten).
    projekt_kennung: String,
    /// Welche Zeilen exportiert werden.
    umfang: ExportUmfang,
    /// Stand der Zeilen; ändert sich beim Laden, Importieren, Hinzufügen und Entfernen von Zeilen.
    version: u64,
    /// Auftrag an den Auswahl-Viewer, den das Hauptfenster abholt.
    auswahl_anfrage: Option<konvertierung::AuswahlAnfrage>,
    /// Der Auswahl-Viewer ist geöffnet.
    auswahl_laeuft: bool,
    /// Markieren durch Ziehen über die Tabelle (Zeilen, die in diesem Zug schon umgeschaltet wurden).
    ziehen: Option<ZiehMarkierung>,
}

/// Zustand beim Markieren durch Ziehen.
#[derive(Debug, Default)]
struct ZiehMarkierung {
    letzte: Option<usize>,
    umgeschaltet: std::collections::HashSet<usize>,
}

/// Umfang der Exporte.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ExportUmfang {
    /// Alle Zeilen der Tabelle.
    #[default]
    Alle,
    /// Die markierten Zeilen (in der Tabelle oder über den DXF-Viewer markiert).
    Tabellenauswahl,
}

fn projekt_kennung(projekt: &Projekt) -> String {
    projekt.einstellungen.projekt_verzeichnis.display().to_string()
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

impl KonvertierungAnsicht {
    pub fn neu(projekt: &Projekt) -> Self {
        let mut a = Self { projekt_kennung: projekt_kennung(projekt), ..Self::default() };
        a.lade_punkte(projekt);
        a
    }

    /// Gehört die (gemerkte) Tabelle zum aktuellen Projekt?
    pub fn gehoert_zu(&self, projekt: &Projekt) -> bool {
        self.projekt_kennung == projekt_kennung(projekt)
    }

    /// Die Zeilen haben sich geändert (eine offene Viewer-Auswahl passt dann nicht mehr).
    fn zeilen_geaendert(&mut self) {
        self.version += 1;
    }

    /// Bereitet den Auftrag an den DXF-Viewer vor (wird vom Hauptfenster mit `nimm_auswahl_anfrage` abgeholt).
    fn starte_dxf_auswahl(&mut self) {
        let anfrage = konvertierung::auswahl_anfrage(&self.lese_zeilen(), &self.markiert, self.version);
        if anfrage.kandidaten.is_empty() {
            self.status = "Keine Zeile mit Lage (Koordinaten oder Richtung mit Strecke) – im DXF-Viewer gibt es nichts zu wählen.".into();
            return;
        }
        self.status = format!("DXF-Viewer zur Auswahl geöffnet: {} wählbare Punkte.", anfrage.kandidaten.len());
        self.auswahl_anfrage = Some(anfrage);
        self.auswahl_laeuft = true;
    }

    /// Auftrag an den DXF-Viewer abholen (einmalig).
    pub fn nimm_auswahl_anfrage(&mut self) -> Option<konvertierung::AuswahlAnfrage> {
        self.auswahl_anfrage.take()
    }

    /// Übernimmt das Ergebnis des DXF-Viewers in die Markierungen der Tabelle.
    pub fn uebernimm_dxf_auswahl(&mut self, ergebnis: konvertierung::AuswahlErgebnis) {
        self.auswahl_laeuft = false;
        if !ergebnis.uebernehmen {
            self.status = "Auswahl im DXF-Viewer verworfen – Markierungen unverändert.".into();
            return;
        }
        if ergebnis.tabellen_version != self.version {
            self.status = "Die Tabelle wurde während der Auswahl geändert – Auswahl verworfen. Bitte erneut auswählen.".into();
            return;
        }
        for (zeile, gewaehlt) in &ergebnis.zeilen {
            if let Some(m) = self.markiert.get_mut(*zeile) {
                *m = *gewaehlt;
            }
        }
        self.umfang = ExportUmfang::Tabellenauswahl;
        let n = self.markiert.iter().filter(|m| **m).count();
        self.status = format!("Auswahl aus dem DXF-Viewer übernommen: {n} Zeile(n) markiert, Export auf „Markierte Zeilen“ gestellt.");
    }

    /// Zeilen für den Export nach dem gewählten Umfang; Fehlertext, wenn die Auswahl leer ist.
    fn export_indizes(&self) -> Result<Vec<usize>, String> {
        let indizes: Vec<usize> = match self.umfang {
            ExportUmfang::Alle => (0..self.zeilen.len()).collect(),
            ExportUmfang::Tabellenauswahl => (0..self.zeilen.len()).filter(|&i| self.markiert.get(i).copied().unwrap_or(false)).collect(),
        };
        if indizes.is_empty() {
            return Err(match self.umfang {
                ExportUmfang::Alle => "Die Tabelle ist leer.".into(),
                ExportUmfang::Tabellenauswahl => "Keine Zeile markiert – bitte Zeilen in der Tabelle oder im DXF-Viewer markieren.".into(),
            });
        }
        Ok(indizes)
    }

    fn export_punkte(&self) -> Result<Vec<KonvertierungPunkt>, String> {
        let alle = self.lese_zeilen();
        Ok(self.export_indizes()?.into_iter().map(|i| alle[i].clone()).collect())
    }

    /// Ergänzt den Statustext eines gespeicherten KOR-/DAT-Exports um Anzahl und übersprungene Zeilen.
    fn melde_textexport(&mut self, export: &konvertierung::TextExport) {
        self.status.push_str(&format!(" – {} Punkte", export.geschrieben));
        if export.uebersprungen > 0 {
            self.status.push_str(&format!(", {} Zeile(n) ohne Punktnummer oder Koordinaten übersprungen", export.uebersprungen));
        }
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
        self.zeilen_geaendert();
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
                bemerkung: z[SPALTE_BEM].clone(),
                vorhanden: [SPALTE_R, SPALTE_H, SPALTE_HOEHE, SPALTE_HZ, SPALTE_V, SPALTE_STRECKE, SPALTE_ZIELH].map(|i| !z[i].trim().is_empty()),
            })
            .collect()
    }

    fn zeile_aus(p: &KonvertierungPunkt) -> [String; SPALTEN] {
        let mut z: [String; SPALTEN] = Default::default();
        z[SPALTE_NR] = p.punkt_nr.clone();
        z[SPALTE_TYP] = p.typ.clone();
        z[SPALTE_R] = if p.vorhanden[0] { f(p.r, 4) } else { String::new() };
        z[SPALTE_H] = if p.vorhanden[1] { f(p.h, 4) } else { String::new() };
        z[SPALTE_HOEHE] = if p.vorhanden[2] { f(p.hoehe, 4) } else { String::new() };
        z[SPALTE_HZ] = if p.vorhanden[3] { f(p.hz, 4) } else { String::new() };
        z[SPALTE_V] = if p.vorhanden[4] { f(p.v, 4) } else { String::new() };
        z[SPALTE_STRECKE] = if p.vorhanden[5] { f(p.strecke, 3) } else { String::new() };
        z[SPALTE_ZIELH] = if p.vorhanden[6] { f(p.zielhoehe, 3) } else { String::new() };
        z[SPALTE_CODE] = p.punktcode.clone();
        z[SPALTE_BEM] = p.bemerkung.clone();
        z
    }

    fn importieren(&mut self) {
        let Some(pfad) = rfd::FileDialog::new()
            .set_title("Datei zum Importieren wählen")
            .add_filter("Alle unterstützten Formate", &["gsi", "dxf", "kor", "dat"])
            .add_filter("GSI-Datei (Leica)", &["gsi"])
            .add_filter("DXF-Datei", &["dxf"])
            .add_filter("KOR-Koordinatendatei", &["kor"])
            .add_filter("DAT-Datei (Koordinaten oder Messdaten)", &["dat"])
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
            "kor" => "Format: KOR (Koordinaten)",
            "dat" => match std::fs::read(&pfad) {
                Ok(b) if konvertierung::ist_koordinaten_dat(&String::from_utf8_lossy(&b)) => "Format: DAT (Koordinaten)",
                _ => "Format: DAT (Messdaten)",
            },
            _ => "Format: unbekannt",
        }
        .to_string();
        self.lese_import(&pfad, &endung);
    }

    fn lese_import(&mut self, pfad: &Path, endung: &str) {
        let punkte = match endung {
            "gsi" => konvertierung::gsi_lese(pfad).map_err(|e| e.to_string()),
            "dxf" => konvertierung::dxf_import(pfad),
            "kor" => konvertierung::kor_lese(pfad).map_err(|e| e.to_string()),
            "dat" => konvertierung::dat_lese(pfad).map_err(|e| e.to_string()),
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
        if hinzugefuegt > 0 {
            self.zeilen_geaendert();
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
        self.zeilen_geaendert();
    }

    /// Fragt einen Zielpfad ab und schreibt den Text; liefert `true`, wenn gespeichert wurde.
    fn exportiere(&mut self, titel: &str, filter: (&str, &str), name: &str, text: &str, kurz: &str) -> bool {
        let Some(pfad) = rfd::FileDialog::new().set_title(titel).add_filter(filter.0, &[filter.1]).add_filter("Alle Dateien", &["*"]).set_file_name(name).save_file() else {
            return false;
        };
        match datei_speicher::schreibe_text(&pfad, text) {
            Ok(()) => {
                self.status = format!("{kurz} gespeichert: {}", pfad.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default());
                true
            }
            Err(fehler) => {
                self.status = format!("Fehler beim Speichern: {fehler}");
                false
            }
        }
    }

    /// KOR-Export der gewählten Punkte (nur Nr, R, H, Höhe, Code; leerer Code → 005).
    fn exportiere_kor(&mut self, punkte: &[KonvertierungPunkt]) {
        let export = konvertierung::kor_text(punkte);
        if export.geschrieben == 0 {
            self.status = "KOR: keine Zeile mit Punktnummer und Koordinaten (R, H) in der Auswahl.".into();
        } else if self.exportiere("KOR-Datei speichern", ("Koordinatendatei", "kor"), "Punkte.kor", &export.text, "KOR") {
            self.melde_textexport(&export);
        }
    }

    /// DAT-Export der gewählten Punkte (Standpunktzeile, dann wie KOR).
    fn exportiere_dat(&mut self, punkte: &[KonvertierungPunkt]) {
        let (export, mit_standpunkt) = konvertierung::dat_text(punkte);
        if export.geschrieben == 0 {
            self.status = "DAT: keine Zeile mit Punktnummer und Koordinaten (R, H) in der Auswahl.".into();
        } else if self.exportiere("DAT-Datei speichern", ("Felddatendatei", "dat"), "Punkte.dat", &export.text, "DAT") {
            self.melde_textexport(&export);
            if !mit_standpunkt {
                self.status.push_str(&format!(" [Standardstandpunkt STP1 / 0.000 / {}, da kein Standpunkt gewählt war]", konvertierung::STANDARD_CODE));
            }
        }
    }

    fn exportiere_csv(&mut self, typ: &str, projekt: &Projekt) {
        let indizes = match self.export_indizes() {
            Ok(i) => i,
            Err(e) => {
                self.status = e;
                return;
            }
        };
        // Ungültige Eingaben dürfen beim Export nicht als 0 erscheinen.
        for n in indizes {
            let z = &self.zeilen[n];
            for i in SPALTE_R..=SPALTE_ZIELH {
                if !z[i].trim().is_empty() && parse_zahl(&z[i]).is_none() {
                    self.status = format!("Zeile {}, Spalte {}: ungültiger Zahlenwert.", n + 1, KOPF[i]);
                    return;
                }
            }
        }
        let punkte = match self.export_punkte() {
            Ok(p) => p,
            Err(e) => {
                self.status = e;
                return;
            }
        };
        let mut d = match konvertierung::csv_datei(&punkte, typ, &projektname(projekt)) {
            Ok(d) => d,
            Err(e) => {
                self.status = e.to_string();
                return;
            }
        };
        if typ != crate::daten::csv_datei::TYP_KOORDINATEN && d.zeilen.iter().any(|z| d.zelle(z, "Typ").is_empty()) {
            self.status = "Für Messdaten bitte in der Spalte Typ jede Beobachtung ausdrücklich als AP oder NP kennzeichnen.".into();
            return;
        }
        let name = crate::daten::csv_datei::CsvDatei::normalisiere_pfad(Path::new("Punkte.csv"), typ);
        let Some(pfad) = rfd::FileDialog::new().set_title("CSV speichern").add_filter("CSV", &["csv"]).set_file_name(name.to_string_lossy()).save_file() else {
            return;
        };
        d.pfad = crate::daten::csv_datei::CsvDatei::normalisiere_pfad(&pfad, typ);
        self.status = match d.speichern() {
            Ok(()) => format!("Gespeichert: {}", d.pfad.display()),
            Err(e) => e.to_string(),
        };
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
                self.zeilen_geaendert();
            }
            if ui.button("− Markierte Zeilen").clicked() {
                self.markierte_entfernen();
            }
            ui.separator();
            ui.label(RichText::new("Markieren:").strong());
            if ui.button("Alle").clicked() {
                self.markiert.iter_mut().for_each(|m| *m = true);
            }
            if ui.button("Keine").clicked() {
                self.markiert.iter_mut().for_each(|m| *m = false);
            }
            let viewer_knopf = ui
                .add_enabled(!self.auswahl_laeuft, egui::Button::new("Im DXF-Viewer auswählen …"))
                .on_hover_text("Punkte per Klick, Rechteck oder Polygon in der Grafik wählen; Messdaten ohne Koordinaten erscheinen hilfsweise mit Standpunkt 0/0 und t = 0")
                .on_disabled_hover_text("Der DXF-Viewer zur Auswahl ist bereits geöffnet.");
            if viewer_knopf.clicked() {
                self.starte_dxf_auswahl();
            }
            ui.label(RichText::new("Tipp: mit gedrückter Maustaste über ✓-Spalte oder Zeilennummern fahren schaltet die Markierung um").weak().size(11.0));
        });
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("Export:").strong());
            let anzahl_markiert = self.markiert.iter().filter(|m| **m).count();
            ui.radio_value(&mut self.umfang, ExportUmfang::Alle, format!("Alle Zeilen ({})", self.zeilen.len()));
            ui.radio_value(&mut self.umfang, ExportUmfang::Tabellenauswahl, format!("Markierte Zeilen ({anzahl_markiert})"));
            ui.separator();
            for (label, typ) in [
                ("KOR-CSV …", crate::daten::csv_datei::TYP_KOORDINATEN),
                ("DAT-CSV …", crate::daten::csv_datei::TYP_MESSDATEN),
                ("STP-CSV …", crate::daten::csv_datei::TYP_STANDPUNKT),
            ] {
                if ui.button(label).clicked() {
                    self.exportiere_csv(typ, projekt);
                }
            }
            if ui.button("Export KOR …").clicked() {
                match self.export_punkte() {
                    Err(e) => self.status = e,
                    Ok(punkte) => self.exportiere_kor(&punkte),
                }
            }
            if ui.button("Export DAT …").clicked() {
                match self.export_punkte() {
                    Err(e) => self.status = e,
                    Ok(punkte) => self.exportiere_dat(&punkte),
                }
            }
        });
        ui.add_space(4.0);

        let raster = Raster::neu(&BREITEN).mit_zeilenkopf(ZEILENKOPF_BREITE);
        let mut kopf: Vec<&str> = vec!["✓"];
        kopf.extend(KOPF);
        let tabelle_hoehe = (ui.available_height() - STATUS_HOEHE).max(TABELLE_MIN_HOEHE);
        // Kein Scrollen durch Ziehen (auch nicht am Touchscreen): Ziehen über die Tabelle schaltet Markierungen um
        let scrollen = egui::containers::scroll_area::ScrollSource { drag: egui::containers::scroll_area::DragScroll::Never, ..Default::default() };
        egui::ScrollArea::both().id_salt("konv_tabelle").max_height(tabelle_hoehe).scroll_source(scrollen).show(ui, |ui| {
            raster.kopf(ui, &kopf);
            let mut zeilen_rects = Vec::with_capacity(self.zeilen.len());
            for (i, zeile) in self.zeilen.iter_mut().enumerate() {
                zeilen_rects.push(egui::Rect::from_min_size(ui.cursor().min, egui::vec2(MARKIER_ZONE_BREITE, raster.zeilen_hoehe())));
                let markiert = &mut self.markiert[i];
                raster.zeile(ui, i, &(i + 1).to_string(), |z| {
                    z.haken(markiert);
                    for zellwert in zeile.iter_mut() {
                        z.eingabe(zellwert);
                    }
                });
            }
            let sicht = ui.clip_rect();
            self.verarbeite_ziehen(ui, &zeilen_rects, sicht);
        });
        ui.add_space(4.0);
        ui.label(&self.status);
    }

    /// Markieren durch Ziehen: Beginnt der Zug in der ✓-Spalte oder bei den Zeilennummern, schaltet jede danach
    /// überfahrene Zeile genau einmal um (unmarkiert → markiert und umgekehrt).
    fn verarbeite_ziehen(&mut self, ui: &egui::Ui, zonen: &[egui::Rect], sicht: egui::Rect) {
        let (gedrueckt, ziehend, ursprung, zeiger) =
            ui.input(|i| (i.pointer.primary_down(), i.pointer.is_decidedly_dragging(), i.pointer.press_origin(), i.pointer.interact_pos()));
        if !gedrueckt {
            self.ziehen = None;
            return;
        }
        if self.ziehen.is_none() {
            let Some(o) = ursprung.filter(|o| ziehend && sicht.contains(*o)) else {
                return;
            };
            let Some(start) = zonen.iter().position(|r| r.contains(o)) else {
                return;
            };
            let mut z = ZiehMarkierung::default();
            ziehe_markierung(&mut self.markiert, &mut z, start);
            self.ziehen = Some(z);
        }
        // Zeile unter dem Zeiger: nur die Höhe zählt, die Spalte ist beliebig
        let Some(p) = zeiger.filter(|p| sicht.y_range().contains(p.y)) else {
            return;
        };
        if let (Some(zeile), Some(z)) = (zonen.iter().position(|r| p.y >= r.top() && p.y < r.bottom()), self.ziehen.as_mut()) {
            ziehe_markierung(&mut self.markiert, z, zeile);
        }
    }
}

/// Breite des Bereichs, in dem ein Markierzug beginnen kann: Zeilennummern und ✓-Spalte.
const MARKIER_ZONE_BREITE: f32 = ZEILENKOPF_BREITE + BREITEN[0];

/// Schaltet beim Ziehen alle Zeilen von der zuletzt überfahrenen bis `zeile` um, jede nur einmal pro Zug
/// (auch wenn der Zeiger zwischen zwei Bildern mehrere Zeilen überspringt).
fn ziehe_markierung(markiert: &mut [bool], zustand: &mut ZiehMarkierung, zeile: usize) {
    let von = zustand.letzte.unwrap_or(zeile);
    for i in von.min(zeile)..=von.max(zeile) {
        if zustand.umgeschaltet.insert(i) {
            if let Some(m) = markiert.get_mut(i) {
                *m = !*m;
            }
        }
    }
    zustand.letzte = Some(zeile);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zeilen_werden_gelesen_und_formatiert() {
        let mut a = KonvertierungAnsicht::default();
        let p = KonvertierungPunkt {
            punkt_nr: "5".into(),
            typ: "Neupunkt".into(),
            r: 10.5,
            h: 20.0,
            hz: 1.25,
            vorhanden: [true, true, false, true, false, false, false],
            ..KonvertierungPunkt::default()
        };
        a.zeilen.push(KonvertierungAnsicht::zeile_aus(&p));
        a.markiert.push(false);
        assert_eq!(a.zeilen[0][SPALTE_R], "10.5000");
        assert_eq!(a.zeilen[0][SPALTE_HOEHE], "", "0 wird als leere Zelle gezeigt");
        let gelesen = a.lese_zeilen();
        assert_eq!(gelesen[0], p);
    }

    fn ansicht_mit_punkten() -> KonvertierungAnsicht {
        let mut a = KonvertierungAnsicht::default();
        for (nr, r, h) in [("1", "100.0", "200.0"), ("2", "300.0", "400.0"), ("3", "500.0", "600.0")] {
            let mut z: [String; SPALTEN] = Default::default();
            z[SPALTE_NR] = nr.to_string();
            z[SPALTE_R] = r.to_string();
            z[SPALTE_H] = h.to_string();
            a.zeilen.push(z);
        }
        a.markiert = vec![false; 3];
        a
    }

    #[test]
    fn exportumfang_alle_und_tabellenauswahl() {
        let mut a = ansicht_mit_punkten();
        assert_eq!(a.export_indizes(), Ok(vec![0, 1, 2]), "Standard: alle Zeilen");
        a.umfang = ExportUmfang::Tabellenauswahl;
        assert!(a.export_indizes().is_err(), "nichts markiert");
        a.markiert[2] = true;
        a.markiert[0] = true;
        assert_eq!(a.export_punkte().unwrap().iter().map(|p| p.punkt_nr.as_str()).collect::<Vec<_>>(), ["1", "3"]);
        a.umfang = ExportUmfang::Alle;
        assert_eq!(a.export_indizes(), Ok(vec![0, 1, 2]), "Markierung wirkt nur bei Tabellenauswahl");
    }

    #[test]
    fn ziehen_schaltet_jede_zeile_einmal_um() {
        let mut markiert = vec![false, true, false, false, true];
        let mut z = ZiehMarkierung::default();
        ziehe_markierung(&mut markiert, &mut z, 0);
        ziehe_markierung(&mut markiert, &mut z, 0); // gleiche Zeile im nächsten Bild: kein zweites Umschalten
        ziehe_markierung(&mut markiert, &mut z, 3); // Zeilen 1–3 übersprungen: alle umschalten
        assert_eq!(markiert, [true, false, true, true, true]);
        ziehe_markierung(&mut markiert, &mut z, 1); // zurück: bereits umgeschaltete bleiben
        assert_eq!(markiert, [true, false, true, true, true]);
    }

    #[test]
    fn dxf_auswahl_wird_in_markierung_uebernommen() {
        let mut a = ansicht_mit_punkten();
        a.markiert = vec![true, false, false];
        a.starte_dxf_auswahl();
        let anfrage = a.nimm_auswahl_anfrage().expect("Auftrag an den Viewer");
        assert!(a.auswahl_laeuft && a.nimm_auswahl_anfrage().is_none());
        assert_eq!(anfrage.vorauswahl, [true, false, false]);
        let ergebnis = |uebernehmen, version| konvertierung::AuswahlErgebnis {
            tabellen_version: version,
            uebernehmen,
            zeilen: vec![(0, false), (1, true), (2, true)],
        };
        // Verworfen: Markierung bleibt
        a.uebernimm_dxf_auswahl(ergebnis(false, anfrage.tabellen_version));
        assert_eq!(a.markiert, [true, false, false]);
        assert!(!a.auswahl_laeuft);
        // Übernommen: Markierung ersetzt, Export auf markierte Zeilen
        a.uebernimm_dxf_auswahl(ergebnis(true, anfrage.tabellen_version));
        assert_eq!(a.markiert, [false, true, true]);
        assert_eq!(a.umfang, ExportUmfang::Tabellenauswahl);
        // Tabelle inzwischen geändert: Auswahl verworfen
        a.zeilen_geaendert();
        a.markiert = vec![false; 3];
        a.uebernimm_dxf_auswahl(ergebnis(true, anfrage.tabellen_version));
        assert_eq!(a.markiert, [false; 3]);
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
