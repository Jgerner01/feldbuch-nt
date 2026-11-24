// Zweck:    Maske „Extraktion aus Messdaten“: liest das Messprotokoll des Projekts, listet die darin enthaltenen
//           Aufstellungen (Standpunkte) zur Auswahl und erzeugt daraus stp.csv (je Aufstellung), dat.csv und kor.csv im
//           Unterordner „Extraktion aus Messdaten“ des Projekts (siehe daten::messdaten_extraktion)
// Hardware: keine
// Autor:    Johann Gerner

use std::collections::HashSet;
use std::path::PathBuf;

use chrono::Local;
use eframe::egui::{self, Color32, RichText};

use crate::daten::messdaten_extraktion::{self as extraktion, Aufstellung, Kopf};
use crate::daten::messprotokoll::{self, Eintrag};
use crate::daten::projekt::Projekt;
use crate::protokolle::{self, ausgabeverzeichnis};

/// Format der Zeitmarke in den Dateinamen.
const ZEITMARKE: &str = "%Y%m%d-%H%M%S";

/// Zustand der Maske.
#[derive(Default)]
pub struct ExtraktionAnsicht {
    geladen: bool,
    /// Gewähltes Messprotokoll (leer: das des Projekts).
    eigener_pfad: Option<PathBuf>,
    pfad: PathBuf,
    eintraege: Vec<Eintrag>,
    /// Aufstellungen mit Auswahlhaken.
    liste: Vec<(Aufstellung, bool)>,
    meldung: String,
    fehler: bool,
    dateien: Vec<PathBuf>,
    hinweise: Vec<String>,
    ordner: Option<PathBuf>,
}

impl ExtraktionAnsicht {
    /// Liest das Messprotokoll neu ein; bisherige Auswahl und Ergebnis werden verworfen.
    fn laden(&mut self, projekt: &Projekt) {
        self.geladen = true;
        self.liste.clear();
        self.eintraege.clear();
        self.dateien.clear();
        self.hinweise.clear();
        self.ordner = None;
        self.pfad = self.eigener_pfad.clone().unwrap_or_else(|| messprotokoll::pfad(&projekt.einstellungen));
        if !self.pfad.exists() {
            self.melde(format!("Kein Messprotokoll gefunden: {}", self.pfad.display()), true);
            return;
        }
        match messprotokoll::lese(&self.pfad) {
            Ok(e) => {
                self.liste = extraktion::aufstellungen(&e).into_iter().map(|a| (a, true)).collect();
                self.eintraege = e;
                if self.liste.is_empty() {
                    self.melde("Das Messprotokoll enthält keine Aufstellungen oder Messungen.".to_string(), true);
                } else {
                    self.melde(format!("{} Zeilen, {} Aufstellung(en) gelesen.", self.eintraege.len(), self.liste.len()), false);
                }
            }
            Err(fehler) => self.melde(format!("Messprotokoll konnte nicht gelesen werden: {fehler}"), true),
        }
    }

    fn melde(&mut self, text: String, fehler: bool) {
        self.meldung = text;
        self.fehler = fehler;
    }

    fn extrahieren(&mut self, projekt: &Projekt) {
        let auswahl: HashSet<String> = self.liste.iter().filter(|(_, an)| *an).map(|(a, _)| a.id.clone()).collect();
        let kopf = Kopf {
            projekt: projekt.einstellungen.projekt_name.clone(),
            sensor: projekt.einstellungen.tachymeter_modell.name().to_string(),
            bearbeiter: projekt.projektdaten.bearbeiter.clone(),
        };
        let ex = extraktion::extrahiere(&self.eintraege, Some(&auswahl), &kopf, &projekt.rechenparameter.params);
        let ordner = extraktion::zielordner(&ausgabeverzeichnis(projekt));
        match extraktion::schreibe(&ex, &ordner, &kopf, &Local::now().format(ZEITMARKE).to_string()) {
            Ok(d) if d.is_empty() => {
                self.melde("Aus den gewählten Aufstellungen ließ sich nichts extrahieren.".to_string(), true);
                self.dateien.clear();
            }
            Ok(d) => {
                self.melde(format!("{} Datei(en) in „{}“ geschrieben.", d.len(), extraktion::ORDNER), false);
                self.dateien = d;
                self.ordner = Some(ordner);
            }
            Err(fehler) => {
                self.melde(format!("Fehler beim Schreiben: {fehler}"), true);
                self.dateien.clear();
            }
        }
        self.hinweise = ex.hinweise;
        crate::grundlagen::protokoll::log("EXTRAKT", &format!("{} Datei(en) aus {} Aufstellung(en) extrahiert", self.dateien.len(), auswahl.len()));
    }

    pub fn zeichne(&mut self, ui: &mut egui::Ui, projekt: &Projekt) {
        if !self.geladen {
            self.laden(projekt);
        }
        ui.heading("Extraktion aus Messdaten");
        ui.label("Erzeugt aus dem Messprotokoll die Dateien stp.csv (je Standpunkt), dat.csv und kor.csv. Vorhandene Projektdateien werden nicht verändert.");
        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(format!("Messprotokoll: {}", self.pfad.display())).small());
        });
        ui.horizontal(|ui| {
            if ui.button("↻ Neu einlesen").clicked() {
                self.laden(projekt);
            }
            if ui.button("Anderes Messprotokoll wählen …").clicked() {
                let mut dialog = rfd::FileDialog::new().set_title("Messprotokoll wählen").add_filter("Messprotokoll (*.csv)", &["csv"]);
                if let Some(p) = self.pfad.parent().filter(|p| p.is_dir()) {
                    dialog = dialog.set_directory(p);
                }
                if let Some(p) = dialog.pick_file() {
                    self.eigener_pfad = Some(p);
                    self.laden(projekt);
                }
            }
            if self.eigener_pfad.is_some() && ui.button("Protokoll des Projekts").clicked() {
                self.eigener_pfad = None;
                self.laden(projekt);
            }
        });
        if !self.meldung.is_empty() {
            let farbe = if self.fehler { Color32::from_rgb(190, 40, 40) } else { ui.visuals().text_color() };
            ui.colored_label(farbe, &self.meldung);
        }
        ui.add_space(8.0);

        if !self.liste.is_empty() {
            ui.label(RichText::new("Aufstellungen (Standpunkte) im Protokoll").strong());
            ui.horizontal(|ui| {
                if ui.small_button("Alle").clicked() {
                    self.liste.iter_mut().for_each(|(_, an)| *an = true);
                }
                if ui.small_button("Keine").clicked() {
                    self.liste.iter_mut().for_each(|(_, an)| *an = false);
                }
            });
            egui::ScrollArea::vertical().id_salt("extraktion_liste").max_height((ui.available_height() - 230.0).max(120.0)).show(ui, |ui| {
                egui::Grid::new("extraktion_raster").num_columns(5).spacing([14.0, 4.0]).striped(true).show(ui, |ui| {
                    for kopf in ["", "Standpunkt", "Datum", "Messungen", "Stationierung"] {
                        ui.label(RichText::new(kopf).strong());
                    }
                    ui.end_row();
                    for (a, an) in &mut self.liste {
                        ui.checkbox(an, "");
                        ui.label(RichText::new(&a.standpunkt_nr).strong()).on_hover_text(&a.id);
                        ui.label(&a.datum);
                        ui.label(format!("AP {} · NP {} · KP {} · AB {}", a.anschlusspunkte, a.neupunkte, a.kontrollen, a.absteckungen));
                        ui.label(if a.hat_stationierung { "ja" } else { "nein" });
                        ui.end_row();
                    }
                });
            });
            ui.add_space(8.0);
            let gewaehlt = self.liste.iter().filter(|(_, an)| *an).count();
            ui.horizontal(|ui| {
                let knopf = egui::Button::new(RichText::new("Extrahieren").strong()).min_size(egui::Vec2::new(170.0, 36.0));
                if ui.add_enabled(gewaehlt > 0, knopf).clicked() {
                    self.extrahieren(projekt);
                }
                ui.label(format!("{gewaehlt} von {} Aufstellung(en) gewählt", self.liste.len()));
                if let Some(ordner) = self.ordner.clone() {
                    if ui.button("Ordner öffnen").clicked() {
                        if let Err(e) = protokolle::oeffne(&ordner) {
                            self.melde(e, true);
                        }
                    }
                }
            });
            ui.label(
                RichText::new(format!(
                    "Ablage: <Projektordner>\\{}\\  –  Dateinamen mit Zeitmarke, frühere Extraktionen bleiben erhalten. \
                     Die Koordinaten der Neupunkte werden mit den aktuellen Rechenparametern neu gerechnet.",
                    extraktion::ORDNER
                ))
                .small(),
            );
        }

        if !self.dateien.is_empty() {
            ui.add_space(6.0);
            ui.label(RichText::new("Erzeugte Dateien").strong());
            for d in &self.dateien {
                ui.label(RichText::new(d.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()).monospace());
            }
        }
        if !self.hinweise.is_empty() {
            ui.add_space(6.0);
            ui.label(RichText::new("Hinweise").strong().color(Color32::from_rgb(190, 110, 20)));
            for h in &self.hinweise {
                ui.add(egui::Label::new(RichText::new(format!("• {h}")).color(Color32::from_rgb(150, 80, 10))).wrap());
            }
        }
    }
}
