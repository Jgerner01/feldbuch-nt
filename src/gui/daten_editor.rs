// Zweck:    Datenmanager: zwei nebeneinanderliegende Editoren für Koordinaten- (*-kor.csv) und Messdatendateien
//           (*-dat.csv) mit Metadaten, editierbarer Tabelle, Spaltenverwaltung, Import/Export KOR/DAT
//           sowie Setzen/Addieren eines Werts für eine ganze Spalte
// Hardware: keine
// Autor:    Johann Gerner

use std::path::PathBuf;

use eframe::egui::{self, Color32, RichText};

use super::tabelle::Raster;
use crate::daten::csv_datei::{CsvDatei, TYP_KOORDINATEN, TYP_MESSDATEN, TYP_STANDPUNKT};
use crate::daten::projekt::Projekt;
use crate::grundlagen::format::{f, parse_zahl};
use crate::grundlagen::{fehler_log, pfade};

/// Breite der ersten Spalte und der weiteren Spalten der Datentabelle.
const BREITE_ERSTE: f32 = 80.0;
const BREITE_SPALTE: f32 = 96.0;
/// Breite von Auswahlspalte und Zeilenkopf.
const BREITE_AUSWAHL: f32 = 30.0;
const ZEILENKOPF_BREITE: f32 = 30.0;
/// Nachkommastellen beim Addieren.
const ADDIEREN_STELLEN: usize = 3;
/// Höhe des Tabellenbereichs.
const TABELLE_HOEHE: f32 = 360.0;

/// Art der Spaltenänderung.
#[derive(Clone, Copy, PartialEq, Eq)]
enum WertArt {
    Setzen,
    Addieren,
}

/// Offener Dialog eines Panels.
enum Dialog {
    Spalten(String),
    Wert { art: WertArt, text: String },
}

/// Ein Editor für eine Datendatei.
#[derive(Default)]
struct DatenPanel {
    datei: Option<CsvDatei>,
    geaendert: bool,
    projekt: String,
    sensor: String,
    bearbeiter: String,
    datum: String,
    messdaten: bool,
    standpunkt: bool,
    aufstellung_id: String,
    stp_nr: String,
    stp_hoehe: String,
    stp_code: String,
    spalten: Vec<String>,
    zeilen: Vec<Vec<String>>,
    markiert: Vec<bool>,
    wahl_spalte: usize,
    dialog: Option<Dialog>,
    meldung: String,
}

fn startordner(projekt: &Projekt) -> PathBuf {
    if projekt.einstellungen.ist_geladen() { projekt.einstellungen.projekt_verzeichnis.clone() } else { pfade::basis() }
}

impl DatenPanel {
    /// Übernimmt die Datei in die Oberfläche.
    fn lade_in_ui(&mut self, datei: CsvDatei) {
        self.projekt.clone_from(&datei.projekt);
        self.sensor.clone_from(&datei.sensor);
        self.bearbeiter.clone_from(&datei.bearbeiter);
        self.datum.clone_from(&datei.datum);
        self.messdaten = datei.datentyp == TYP_MESSDATEN;
        self.standpunkt = datei.datentyp == TYP_STANDPUNKT;
        self.aufstellung_id = datei.meta("AufstellungId").unwrap_or("").into();
        self.stp_nr.clone_from(&datei.standpunkt_nr);
        self.stp_hoehe.clone_from(&datei.instr_hoehe);
        self.stp_code.clone_from(&datei.standpunkt_code);
        self.spalten.clone_from(&datei.spalten);
        self.zeilen = datei.zeilen.clone();
        for z in &mut self.zeilen {
            z.resize(self.spalten.len(), String::new());
        }
        self.markiert = vec![false; self.zeilen.len()];
        self.wahl_spalte = 0;
        self.datei = Some(datei);
        self.geaendert = false;
    }

    /// Schreibt die Oberfläche in das Modell.
    fn lese_aus_ui(&mut self) {
        let Some(d) = self.datei.as_mut() else {
            return;
        };
        d.projekt = self.projekt.trim().to_string();
        d.sensor = self.sensor.trim().to_string();
        d.bearbeiter = self.bearbeiter.trim().to_string();
        d.datum = self.datum.trim().to_string();
        let vorher_typ = d.datentyp.clone();
        let vorher_sp = d.standpunkt_nr.clone();
        let vorher_id = d.meta("AufstellungId").unwrap_or("").to_string();
        d.datentyp = if self.standpunkt {
            TYP_STANDPUNKT
        } else if self.messdaten {
            TYP_MESSDATEN
        } else {
            TYP_KOORDINATEN
        }
        .to_string();
        d.set_meta("AufstellungId", &self.aufstellung_id);
        let vorher_ih = d.instr_hoehe.clone();
        d.standpunkt_nr = self.stp_nr.trim().to_string();
        d.instr_hoehe = self.stp_hoehe.trim().to_string();
        d.standpunkt_code = self.stp_code.trim().to_string();
        d.spalten.clone_from(&self.spalten);
        let vorher = d.zeilen.clone();
        d.zeilen = self.zeilen.iter().cloned().filter(|z| !z.iter().all(String::is_empty)).collect();
        if d.zeilen != vorher || d.instr_hoehe != vorher_ih || d.standpunkt_nr != vorher_sp || d.datentyp != vorher_typ || self.aufstellung_id != vorher_id {
            d.stationierung.clear();
        }
    }

    fn hinweis(&mut self, text: impl Into<String>) {
        self.meldung = text.into();
    }

    fn oeffnen(&mut self, projekt: &Projekt) {
        let Some(pfad) = rfd::FileDialog::new()
            .set_title("CSV-Datendatei öffnen")
            .add_filter("CSV-Datendateien", &["csv"])
            .add_filter("Alle Dateien", &["*"])
            .set_directory(startordner(projekt))
            .pick_file()
        else {
            return;
        };
        match CsvDatei::laden(&pfad) {
            Ok(d) => {
                self.lade_in_ui(d);
                self.meldung.clear();
            }
            Err(fehler) => self.hinweis(format!("Fehler beim Laden:\n{fehler}")),
        }
    }

    fn neue_datei(&mut self, projekt: &Projekt, datentyp: &str) {
        let vorschlag = match datentyp {
            TYP_STANDPUNKT => "standpunkt-stp.csv",
            TYP_MESSDATEN => "messdaten-dat.csv",
            _ => "koordinaten-kor.csv",
        };
        let Some(pfad) = rfd::FileDialog::new()
            .set_title(format!("Neue {datentyp}-Datei anlegen"))
            .add_filter("CSV-Datei", &["csv"])
            .set_directory(startordner(projekt))
            .set_file_name(vorschlag)
            .save_file()
        else {
            return;
        };
        let mut d = CsvDatei::neu(datentyp);
        d.pfad = CsvDatei::normalisiere_pfad(&pfad, datentyp);
        if projekt.einstellungen.ist_geladen() {
            d.projekt.clone_from(&projekt.einstellungen.projekt_name);
        }
        self.lade_in_ui(d);
        self.geaendert = true;
        self.meldung.clear();
    }

    /// Speichert die Datei; ohne Pfad wird (wenn `dialog`) nach einem Ziel gefragt.
    fn speichern(&mut self, dialog: bool) {
        if self.datei.is_none() {
            return;
        }
        self.lese_aus_ui();
        let Some(d) = self.datei.as_mut() else {
            return;
        };
        if !d.pfad.as_os_str().is_empty() {
            let ziel = CsvDatei::normalisiere_pfad(&d.pfad, &d.datentyp);
            if ziel != d.pfad && ziel.exists() {
                self.meldung = format!("Das normalisierte Ziel {} existiert bereits. Bitte einen anderen Dateinamen verwenden.", ziel.display());
                return;
            }
            d.pfad = ziel;
        }
        if d.pfad.as_os_str().is_empty() {
            if !dialog {
                return;
            }
            let Some(pfad) = rfd::FileDialog::new().set_title("Datei speichern").add_filter("CSV-Datei", &["csv"]).save_file() else {
                return;
            };
            d.pfad = CsvDatei::normalisiere_pfad(&pfad, &d.datentyp);
        }
        match d.speichern() {
            Ok(()) => {
                self.geaendert = false;
                self.meldung = format!("Gespeichert: {}", d.pfad.display());
            }
            Err(fehler) => {
                fehler_log::log("Datenmanager.speichern", &fehler.to_string());
                self.meldung = format!("Fehler beim Speichern:\n{fehler}");
            }
        }
    }

    fn importieren(&mut self, projekt: &Projekt) {
        let kor = !self.messdaten;
        let (titel, filter, endung) = if kor { ("KOR-Datei importieren", "KOR-Dateien", "kor") } else { ("DAT-Datei importieren", "DAT-Dateien", "dat") };
        let Some(pfad) = rfd::FileDialog::new().set_title(titel).add_filter(filter, &[endung]).add_filter("Alle Dateien", &["*"]).set_directory(startordner(projekt)).pick_file()
        else {
            return;
        };
        let ergebnis = if kor { CsvDatei::importiere_kor(&pfad) } else { CsvDatei::importiere_dat(&pfad) };
        match ergebnis {
            Ok(d) => {
                self.lade_in_ui(d);
                self.geaendert = true;
                self.meldung.clear();
            }
            Err(fehler) => self.hinweis(format!("Fehler beim Import:\n{fehler}")),
        }
    }

    fn exportieren(&mut self, projekt: &Projekt) {
        if self.datei.is_none() {
            self.hinweis("Keine Datei geöffnet.");
            return;
        }
        self.lese_aus_ui();
        let Some(d) = &self.datei else {
            return;
        };
        if d.datentyp == TYP_STANDPUNKT {
            self.hinweis("STP-Dateien über Speichern im vollständigen CSV-Format sichern.");
            return;
        }
        let kor = !d.ist_messdaten();
        let mut basis = d.pfad.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let klein = basis.to_lowercase();
        if klein.ends_with("-kor") || klein.ends_with("-dat") {
            basis.truncate(basis.len() - 4);
        }
        let (titel, filter, endung) = if kor { ("Als KOR exportieren", "KOR-Dateien", "kor") } else { ("Als DAT exportieren", "DAT-Dateien", "dat") };
        let Some(pfad) = rfd::FileDialog::new()
            .set_title(titel)
            .add_filter(filter, &[endung])
            .add_filter("Alle Dateien", &["*"])
            .set_directory(startordner(projekt))
            .set_file_name(format!("{basis}.{endung}"))
            .save_file()
        else {
            return;
        };
        let ergebnis = if kor { d.exportiere_kor(&pfad) } else { d.exportiere_dat(&pfad) };
        match ergebnis {
            Ok(()) => self.meldung = format!("Exportiert: {}", pfad.display()),
            Err(fehler) => self.meldung = format!("Fehler beim Export:\n{fehler}"),
        }
    }

    fn zeile_hinzufuegen(&mut self) {
        self.zeilen.push(vec![String::new(); self.spalten.len()]);
        self.markiert.push(false);
        self.geaendert = true;
    }

    fn markierte_entfernen(&mut self) {
        let mut i = 0;
        let vorher = self.zeilen.len();
        self.zeilen.retain(|_| {
            let behalten = !self.markiert.get(i).copied().unwrap_or(false);
            i += 1;
            behalten
        });
        self.markiert = vec![false; self.zeilen.len()];
        if self.zeilen.len() != vorher {
            self.geaendert = true;
        }
    }

    fn setze_spalten(&mut self, neu: Vec<String>) {
        for z in &mut self.zeilen {
            z.resize(neu.len(), String::new());
        }
        self.spalten = neu;
        self.wahl_spalte = self.wahl_spalte.min(self.spalten.len().saturating_sub(1));
        self.geaendert = true;
    }

    fn wende_wert_an(&mut self, art: WertArt, eingabe: &str) -> Result<(), String> {
        let spalte = self.wahl_spalte;
        let wert = eingabe.trim();
        if wert.is_empty() || spalte >= self.spalten.len() {
            return Ok(());
        }
        match art {
            WertArt::Setzen => {
                for z in &mut self.zeilen {
                    z[spalte] = wert.to_string();
                }
            }
            WertArt::Addieren => {
                let summand = parse_zahl(wert).ok_or_else(|| format!("'{wert}' ist keine gültige Zahl."))?;
                for z in &mut self.zeilen {
                    let neu = parse_zahl(&z[spalte]).unwrap_or(0.0) + summand;
                    z[spalte] = f(neu, ADDIEREN_STELLEN);
                }
            }
        }
        self.geaendert = true;
        Ok(())
    }

    // ── Darstellung ───────────────────────────────────────────────────────────

    fn zeichne(&mut self, ui: &mut egui::Ui, projekt: &Projekt, id: &str) {
        ui.push_id(id, |ui| {
            ui.horizontal_wrapped(|ui| {
                if ui.button("Öffnen").clicked() {
                    self.oeffnen(projekt);
                }
                if ui.button("Neu KOR").clicked() {
                    self.neue_datei(projekt, TYP_KOORDINATEN);
                }
                if ui.button("Neu DAT").clicked() {
                    self.neue_datei(projekt, TYP_MESSDATEN);
                }
                if ui.button("Neu STP").clicked() {
                    self.neue_datei(projekt, TYP_STANDPUNKT);
                }
                if ui.add_enabled(self.datei.is_some(), egui::Button::new("Speichern")).clicked() {
                    self.speichern(true);
                }
                let name = self.datei.as_ref().and_then(|d| d.pfad.file_name()).map_or_else(|| "(keine Datei geöffnet)".to_string(), |n| n.to_string_lossy().into_owned());
                let titel = if self.geaendert { format!("{name} *") } else { name };
                ui.add(egui::Label::new(RichText::new(titel).monospace()).wrap());
            });
            ui.separator();
            if self.datei.is_none() {
                if !self.meldung.is_empty() {
                    ui.label(&self.meldung);
                }
                return;
            }
            self.zeichne_metadaten(ui);
            ui.add_space(4.0);
            self.zeichne_tabelle(ui, projekt);
            if !self.meldung.is_empty() {
                ui.add_space(4.0);
                ui.label(&self.meldung);
            }
            self.zeichne_dialog(ui.ctx());
        });
    }

    fn zeichne_metadaten(&mut self, ui: &mut egui::Ui) {
        let mut geaendert = false;
        let eingabe_breite = (ui.available_width() - 100.0).clamp(60.0, 220.0);
        egui::Grid::new("metadaten").num_columns(2).show(ui, |ui| {
            let mut zeile = |ui: &mut egui::Ui, titel: &str, text: &mut String| {
                ui.label(titel);
                geaendert |= ui.add(egui::TextEdit::singleline(text).desired_width(eingabe_breite)).changed();
                ui.end_row();
            };
            zeile(ui, "Projekt:", &mut self.projekt);
            zeile(ui, "Sensor:", &mut self.sensor);
            zeile(ui, "Bearbeiter:", &mut self.bearbeiter);
            zeile(ui, "Datum:", &mut self.datum);
        });
        ui.horizontal(|ui| {
            ui.label("Datentyp:");
            let mut typ = if self.standpunkt {
                2
            } else if self.messdaten {
                1
            } else {
                0
            };
            geaendert |= ui.radio_value(&mut typ, 0, "Koordinaten").changed();
            geaendert |= ui.radio_value(&mut typ, 1, "Messdaten").changed();
            geaendert |= ui.radio_value(&mut typ, 2, "Standpunkt").changed();
            self.standpunkt = typ == 2;
            self.messdaten = typ == 1;
        });
        if self.messdaten || self.standpunkt {
            ui.horizontal(|ui| {
                ui.label("Aufstellung:");
                geaendert |= ui.text_edit_singleline(&mut self.aufstellung_id).changed();
            });
            ui.horizontal_wrapped(|ui| {
                ui.label("Standpunkt-Nr:");
                geaendert |= ui.add(egui::TextEdit::singleline(&mut self.stp_nr).desired_width(90.0)).changed();
                ui.label("Instr.-Höhe:");
                geaendert |= ui.add(egui::TextEdit::singleline(&mut self.stp_hoehe).desired_width(60.0)).changed();
                ui.label("Code:");
                geaendert |= ui.add(egui::TextEdit::singleline(&mut self.stp_code).desired_width(60.0)).changed();
            });
        }
        self.geaendert |= geaendert;
    }

    fn zeichne_tabelle(&mut self, ui: &mut egui::Ui, projekt: &Projekt) {
        ui.horizontal_wrapped(|ui| {
            if ui.button("Spalten …").clicked() {
                self.dialog = Some(Dialog::Spalten(self.spalten.join("; ")));
            }
            if ui.button("+ Zeile").clicked() {
                self.zeile_hinzufuegen();
            }
            if ui.button("− Zeile").clicked() {
                self.markierte_entfernen();
            }
            if ui.button("↓ Import").clicked() {
                self.importieren(projekt);
            }
            if ui.button("↑ Export").clicked() {
                self.exportieren(projekt);
            }
        });
        if !self.spalten.is_empty() {
            ui.horizontal_wrapped(|ui| {
                ui.label("Spalte:");
                let aktuell = self.spalten[self.wahl_spalte.min(self.spalten.len() - 1)].clone();
                egui::ComboBox::from_id_salt("wahl_spalte").selected_text(aktuell).show_ui(ui, |ui| {
                    for (i, s) in self.spalten.iter().enumerate() {
                        ui.selectable_value(&mut self.wahl_spalte, i, s);
                    }
                });
                if ui.button("= Alle auf Wert setzen …").clicked() {
                    self.dialog = Some(Dialog::Wert { art: WertArt::Setzen, text: String::new() });
                }
                if ui.button("+ Zu allen addieren …").clicked() {
                    self.dialog = Some(Dialog::Wert { art: WertArt::Addieren, text: String::new() });
                }
            });
        }
        let mut breiten = vec![BREITE_AUSWAHL];
        breiten.extend((0..self.spalten.len()).map(|i| if i == 0 { BREITE_ERSTE } else { BREITE_SPALTE }));
        let raster = Raster::neu(&breiten).mit_zeilenkopf(ZEILENKOPF_BREITE);
        let mut kopf: Vec<&str> = vec!["✓"];
        kopf.extend(self.spalten.iter().map(String::as_str));
        let mut geaendert = false;
        egui::ScrollArea::horizontal().id_salt("datentabelle_horizontal").auto_shrink([false, true]).show(ui, |ui| {
            raster.kopf(ui, &kopf);
            egui::ScrollArea::vertical().id_salt("datentabelle_zeilen").max_height(TABELLE_HOEHE).show(ui, |ui| {
                for (i, zeile) in self.zeilen.iter_mut().enumerate() {
                    let markiert = &mut self.markiert[i];
                    raster.zeile(ui, i, &(i + 1).to_string(), |z| {
                        z.haken(markiert);
                        for zelle in zeile.iter_mut() {
                            geaendert |= z.eingabe(zelle);
                        }
                    });
                }
            });
        });
        self.geaendert |= geaendert;
    }

    fn zeichne_dialog(&mut self, ctx: &egui::Context) {
        let Some(dialog) = self.dialog.take() else {
            return;
        };
        let mut offen = true;
        let mut bestaetigt = false;
        let (titel, mut text, art) = match dialog {
            Dialog::Spalten(t) => ("Spalten bearbeiten".to_string(), t, None),
            Dialog::Wert { art, text } => {
                let name = self.spalten.get(self.wahl_spalte).cloned().unwrap_or_default();
                let aktion = if art == WertArt::Setzen { "Alle Werte setzen" } else { "Wert addieren" };
                (format!("Spalte '{name}' – {aktion}"), text, Some(art))
            }
        };
        egui::Window::new(titel).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO).show(ctx, |ui| {
            ui.label(match art {
                None => "Spaltenbezeichnungen (Semikolon-getrennt):".to_string(),
                Some(WertArt::Setzen) => format!("Wert für alle {} Zeilen:", self.zeilen.len()),
                Some(WertArt::Addieren) => "Wert zum Addieren (+) für alle Zeilen:".to_string(),
            });
            ui.add(egui::TextEdit::singleline(&mut text).desired_width(420.0).font(egui::TextStyle::Monospace));
            ui.horizontal(|ui| {
                if ui.button(RichText::new("OK").strong()).clicked() {
                    bestaetigt = true;
                }
                if ui.button("Abbrechen").clicked() {
                    offen = false;
                }
            });
        });
        if bestaetigt {
            match art {
                None => {
                    let neu: Vec<String> = text.split(';').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
                    if !neu.is_empty() {
                        self.setze_spalten(neu);
                    }
                }
                Some(a) => {
                    if let Err(fehler) = self.wende_wert_an(a, &text) {
                        self.meldung = fehler;
                    }
                }
            }
        } else if offen {
            self.dialog = Some(match art {
                None => Dialog::Spalten(text),
                Some(a) => Dialog::Wert { art: a, text },
            });
        }
    }
}

impl Drop for DatenPanel {
    /// Wie im Original: beim Schließen wird gespeichert (nur mit bekanntem Pfad, ohne Dialog).
    fn drop(&mut self) {
        self.speichern(false);
    }
}

/// Der Datenmanager mit zwei Editoren.
#[derive(Default)]
pub struct DatenEditorAnsicht {
    links: DatenPanel,
    rechts: DatenPanel,
}

impl DatenEditorAnsicht {
    pub fn zeichne(&mut self, ui: &mut egui::Ui, projekt: &Projekt) {
        ui.heading("Datenmanager");
        ui.colored_label(Color32::GRAY, "Beim Verlassen werden beide Dateien gespeichert.");
        ui.add_space(6.0);
        ui.columns(2, |spalten| {
            editor_bereich(&mut spalten[0], "links", "Datei links", Color32::from_rgb(232, 241, 252), Color32::from_rgb(65, 105, 155), &mut self.links, projekt);
            editor_bereich(&mut spalten[1], "rechts", "Datei rechts", Color32::from_rgb(232, 246, 237), Color32::from_rgb(55, 120, 80), &mut self.rechts, projekt);
        });
    }
}

/// Jeder Editor bleibt in seiner eigenen Fläche und scrollt unabhängig.
fn editor_bereich(ui: &mut egui::Ui, id: &str, titel: &str, hintergrund: Color32, akzent: Color32, panel: &mut DatenPanel, projekt: &Projekt) {
    egui::Frame::new().fill(hintergrund).stroke(egui::Stroke::new(1.0, akzent)).corner_radius(5.0).inner_margin(8.0).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.label(RichText::new(titel).strong().color(akzent));
        ui.add_space(4.0);
        egui::ScrollArea::vertical().id_salt(("dateneditor", id)).auto_shrink([false, false]).show(ui, |ui| panel.zeichne(ui, projekt, id));
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn panel() -> DatenPanel {
        let mut p = DatenPanel::default();
        let mut d = CsvDatei::neu(TYP_KOORDINATEN);
        d.zeilen = vec![vec!["1".into(), "10.5".into(), "20".into(), "".into(), "".into()], vec!["2".into(), "x".into(), "".into(), "".into(), "".into()]];
        p.lade_in_ui(d);
        p
    }

    #[test]
    fn setzen_und_addieren() {
        let mut p = panel();
        p.wahl_spalte = 1;
        p.wende_wert_an(WertArt::Addieren, "1,5").unwrap();
        assert_eq!(p.zeilen[0][1], "12.000");
        assert_eq!(p.zeilen[1][1], "1.500", "keine Zahl: nur der Summand");
        assert!(p.wende_wert_an(WertArt::Addieren, "abc").is_err());
        p.wahl_spalte = 4;
        p.wende_wert_an(WertArt::Setzen, "Baum").unwrap();
        assert!(p.zeilen.iter().all(|z| z[4] == "Baum"));
        assert!(p.geaendert);
    }

    #[test]
    fn spalten_und_zeilen_verwalten() {
        let mut p = panel();
        p.setze_spalten(vec!["A".into(), "B".into()]);
        assert!(p.zeilen.iter().all(|z| z.len() == 2));
        p.zeile_hinzufuegen();
        assert_eq!(p.zeilen.len(), 3);
        p.markiert[0] = true;
        p.markierte_entfernen();
        assert_eq!(p.zeilen.len(), 2);
        assert_eq!(p.zeilen[0][0], "2");
        p.datei = None;
    }

    #[test]
    fn leere_zeilen_werden_nicht_uebernommen() {
        let mut p = panel();
        p.zeile_hinzufuegen();
        p.lese_aus_ui();
        assert_eq!(p.datei.as_ref().unwrap().zeilen.len(), 2);
        p.datei = None;
    }
}
