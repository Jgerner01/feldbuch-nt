// Zweck:    Fenster "Messgerät – Kommunikation": Gerätetyp, COM-Port, Kommunikationsparameter, Verbinden;
//           zusätzlich Empfangsprotokoll und Testbefehl zum Prüfen der Verbindung
// Hardware: Tachymeter und GNSS über serielle Schnittstelle (RS-232, Bluetooth-SPP)
// Autor:    Johann Gerner

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use eframe::egui::{self, Color32, RichText, Vec2};

use super::geraet::Geraet;
use crate::daten::projekt::Projekt;
use crate::grundlagen::kommunikationslog;
use crate::tachymeter::modell::{ALLE_MODELLE, Sprache, TachymeterModell};
use crate::tachymeter::verbindung::{AnalyseTreffer, Zeilenende, analysiere, verfuegbare_ports};

/// Anteil der linken Spalte an der Fensterbreite (Original: 572 von 1200 px).
const SPALTE_LINKS: f32 = 0.47;
/// Anteil der rechten Spalte (Original: 588 von 1200 px).
const SPALTE_RECHTS: f32 = 0.47;
const BAUDRATEN: [u32; 8] = [1200, 2400, 4800, 9600, 19_200, 38_400, 57_600, 115_200];
const DATENBITS: [u8; 2] = [7, 8];
const PARITAETEN: [&str; 3] = ["None", "Odd", "Even"];
const STOPPBITS: [&str; 3] = ["1", "1.5", "2"];
/// Farbe der Statusanzeige bei bestehender Verbindung.
const FARBE_VERBUNDEN: Color32 = Color32::from_rgb(50, 205, 50);
/// Farbe der Statusanzeige ohne Verbindung.
const FARBE_GETRENNT: Color32 = Color32::from_rgb(200, 50, 50);
/// Hinweis zu 1,5 Stoppbits.
const HINWEIS_STOPPBITS: &str = "Hinweis: 1,5 Stoppbits werden von der seriellen Bibliothek nicht unterstützt (es wird 2 verwendet).";

/// Zustand einer laufenden oder beendeten Analyse (wird vom Arbeits-Thread befüllt).
#[derive(Default)]
struct AnalyseZustand {
    laeuft: bool,
    status: String,
    treffer: Vec<AnalyseTreffer>,
}

/// Maske der Kommunikation.
#[derive(Default)]
pub struct KommunikationAnsicht {
    ports: Vec<String>,
    geladen: bool,
    befehl: String,
    meldung: String,
    fehler: bool,
    analyse: Arc<Mutex<AnalyseZustand>>,
    analyse_abbruch: Arc<AtomicBool>,
    war_verbindet: bool,
}

impl KommunikationAnsicht {
    /// Zeichnet die Maske; liefert `true`, wenn sie geschlossen werden soll.
    pub fn zeichne(&mut self, ui: &mut egui::Ui, projekt: &mut Projekt, geraet: &mut Geraet) -> bool {
        if !self.geladen {
            self.geladen = true;
            self.ports = verfuegbare_ports();
            if geraet.einstellung.port.is_empty() {
                geraet.einstellung.port = self.ports.first().cloned().unwrap_or_default();
            }
        }
        geraet.aktualisiere();
        // Nach erfolgreichem Verbinden die Einstellungen speichern
        if self.war_verbindet && !geraet.verbindet() && geraet.ist_verbunden() {
            self.uebernehmen(projekt, geraet);
        }
        self.war_verbindet = geraet.verbindet();
        // Eingestelltes Ziel (Reflektor/Reflektorlos) in den Einstellungen festhalten
        let rl_gespeichert = projekt.einstellungen.mess_modus_name != "Prisma";
        if rl_gespeichert != geraet.reflektorlos {
            projekt.einstellungen.mess_modus_name =
                if geraet.reflektorlos { "ReflektorlosKurz" } else { "Prisma" }.to_string();
            let _ = projekt.einstellungen.speichern();
        }
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));

        let mut schliessen = false;
        ui.heading("Messgerät – Kommunikation");
        ui.add_space(6.0);

        // Zwei Spalten, relativ zur Fensterbreite
        let gesamt = ui.available_width();
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width(gesamt * SPALTE_LINKS);
                self.gruppe_geraetetyp(ui, geraet);
                self.gruppe_schnittstelle(ui, geraet);
                self.gruppe_koppeln(ui);
            });
            ui.vertical(|ui| {
                ui.set_width(gesamt * SPALTE_RECHTS);
                self.gruppe_parameter(ui, geraet);
                self.gruppe_protokoll(ui, geraet);
                self.gruppe_analyse(ui, geraet);
            });
        });

        ui.add_space(8.0);
        if !self.meldung.is_empty() {
            let farbe = if self.fehler { Color32::RED } else { ui.visuals().text_color() };
            ui.colored_label(farbe, &self.meldung);
        }
        ui.horizontal(|ui| {
            if ui.add(egui::Button::new(RichText::new("OK").strong()).min_size(Vec2::new(130.0, 36.0))).clicked() {
                self.uebernehmen(projekt, geraet);
                schliessen = true;
            }
            if ui.add(egui::Button::new("Abbrechen").min_size(Vec2::new(130.0, 36.0))).clicked() {
                schliessen = true;
            }
        });
        schliessen
    }

    fn gruppe_geraetetyp(&mut self, ui: &mut egui::Ui, geraet: &mut Geraet) {
        ui.group(|ui| {
            ui.set_width(ui.available_width());
            ui.strong("Gerätetyp");
            ui.horizontal(|ui| {
                ui.label("Typ:");
                let mut neu = geraet.modell;
                egui::ComboBox::from_id_salt("geraetetyp")
                    .width(470.0)
                    .selected_text(geraet.modell.anzeige())
                    .show_ui(ui, |ui| {
                        for m in ALLE_MODELLE {
                            ui.selectable_value(&mut neu, m, m.anzeige());
                        }
                    });
                if neu != geraet.modell {
                    geraet.setze_modell(neu);
                    // Voreinstellungen je Modell (außer Manuell)
                    if neu != TachymeterModell::Manuell {
                        let p = neu.preset();
                        geraet.einstellung.baud = p.baud;
                        geraet.einstellung.datenbits = p.datenbits;
                        geraet.einstellung.paritaet = "None".to_string();
                        geraet.einstellung.stopbits = "1".to_string();
                    }
                }
            });
            if geraet.modell == TachymeterModell::GnssNmea {
                ui.label("Gerät / Protokoll: nicht relevant für GNSS (typisch: 4800 Baud, 8N1 für NMEA)");
            }
            if geraet.modell.sprache_waehlbar() {
                ui.horizontal(|ui| {
                    ui.label("Sprache am Gerät:");
                    let mut wahl = geraet.sprache;
                    ui.radio_value(&mut wahl, Sprache::GeoCom, "GeoCOM");
                    ui.radio_value(&mut wahl, Sprache::GsiOnline, "GSI Online");
                    if wahl != geraet.sprache {
                        geraet.setze_sprache(wahl);
                    }
                });
                ui.label(
                    RichText::new("Muss mit der Einstellung am Instrument übereinstimmen (Schnittstelle/Kommunikation: GeoCOM bzw. GSI Online).")
                        .small(),
                );
            }
        });
    }

    fn gruppe_schnittstelle(&mut self, ui: &mut egui::Ui, geraet: &mut Geraet) {
        ui.group(|ui| {
            ui.set_width(ui.available_width());
            ui.strong("Bluetooth / Schnittstelle");
            let verbunden = geraet.ist_verbunden();
            ui.horizontal(|ui| {
                ui.label("Schnittstelle:");
                ui.add_enabled_ui(!verbunden, |ui| {
                    egui::ComboBox::from_id_salt("comport")
                        .width(270.0)
                        .selected_text(if geraet.einstellung.port.is_empty() { "– Kein COM-Port gefunden –" } else { geraet.einstellung.port.as_str() })
                        .show_ui(ui, |ui| {
                            for p in &self.ports {
                                ui.selectable_value(&mut geraet.einstellung.port, p.clone(), p);
                            }
                        });
                });
                if ui.button("↻ Aktualisieren").clicked() {
                    self.ports = verfuegbare_ports();
                    self.meldung = "Liste aktualisiert.".to_string();
                    self.fehler = false;
                }
            });
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(Vec2::splat(18.0), egui::Sense::hover());
                let farbe = if verbunden { FARBE_VERBUNDEN } else { FARBE_GETRENNT };
                ui.painter().rect_filled(rect, 0.0, farbe);
                let text = if verbunden {
                    format!("Verbunden  ({})", geraet.einstellung.port)
                } else {
                    "Getrennt".to_string()
                };
                ui.label(RichText::new(text).strong().color(if verbunden { Color32::from_rgb(0, 100, 0) } else { Color32::from_rgb(160, 40, 40) }));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let beschriftung = if geraet.verbindet() {
                        "Abbrechen"
                    } else if verbunden {
                        "Trennen"
                    } else {
                        "Verbinden"
                    };
                    let knopf = egui::Button::new(RichText::new(beschriftung).strong().color(Color32::WHITE))
                        .fill(Color32::from_rgb(60, 100, 160))
                        .min_size(Vec2::new(164.0, 40.0));
                    if ui.add(knopf).clicked() {
                        self.verbinden_umschalten(geraet);
                    }
                });
            });
            if geraet.verbindet() {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("Verbinde … (höchstens 10 s)");
                });
            }
            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                let laeuft = geraet.test.is_some();
                let knopf = egui::Button::new(RichText::new("Testmessung").strong()).min_size(Vec2::new(150.0, 32.0));
                if ui
                    .add_enabled(verbunden && !laeuft, knopf)
                    .on_hover_text("Prüft die Einstellungen: Winkel lesen, messen und das Ergebnis anzeigen (GeoCOM bzw. GSI Online)")
                    .clicked()
                {
                    if let Err(e) = geraet.starte_testmessung() {
                        self.meldung = e;
                        self.fehler = true;
                    } else {
                        self.meldung.clear();
                    }
                }
                let schaltet = geraet.schalter.is_some();
                let frei = verbunden && !laeuft && !schaltet;

                // Reflektor / Reflektorlos (aktueller Zustand: Farbe)
                let (fill, rand) = if geraet.reflektorlos {
                    (Color32::from_rgb(150, 70, 20), Color32::from_rgb(200, 100, 40))
                } else {
                    (Color32::from_rgb(60, 95, 160), Color32::from_rgb(80, 115, 185))
                };
                let ziel_knopf = egui::Button::new(RichText::new("Reflektor/Reflektorlos").strong().color(Color32::WHITE))
                    .fill(fill)
                    .stroke(egui::Stroke::new(1.0, rand))
                    .min_size(Vec2::new(190.0, 32.0));
                if ui
                    .add_enabled(frei, ziel_knopf)
                    .on_hover_text("Schaltet das Ziel zwischen Reflektor (Prisma) und Reflektorlos um und prüft die Quittung des Geräts")
                    .clicked()
                {
                    if let Err(e) = geraet.starte_ziel_umschaltung() {
                        self.meldung = e;
                        self.fehler = true;
                    } else {
                        self.meldung.clear();
                    }
                }

                // Laserpointer (an = rot)
                let (fill, rand) = if geraet.laser_an {
                    (Color32::from_rgb(180, 55, 15), Color32::from_rgb(240, 90, 30))
                } else {
                    (Color32::from_rgb(60, 95, 160), Color32::from_rgb(80, 115, 185))
                };
                let laser_knopf = egui::Button::new(RichText::new("Laserpointer").strong().color(Color32::WHITE))
                    .fill(fill)
                    .stroke(egui::Stroke::new(1.0, rand))
                    .min_size(Vec2::new(130.0, 32.0));
                if ui
                    .add_enabled(frei, laser_knopf)
                    .on_hover_text(if geraet.laser_an { "Laserpointer ausschalten" } else { "Laserpointer einschalten" })
                    .clicked()
                {
                    if let Err(e) = geraet.starte_laser_umschaltung() {
                        self.meldung = e;
                        self.fehler = true;
                    } else {
                        self.meldung.clear();
                    }
                }
                if laeuft {
                    ui.spinner();
                    let text = geraet.test.as_ref().map(|t| t.schritt_text()).unwrap_or_default();
                    ui.label(text);
                }
                if schaltet {
                    ui.spinner();
                    ui.label(geraet.schalter.as_ref().map(|s| format!("{} …", s.beschreibung())).unwrap_or_default());
                }
            });
            ui.label(
                RichText::new(format!(
                    "Ziel: {}   |   Laserpointer: {}",
                    if geraet.reflektorlos { "Reflektorlos" } else { "Reflektor (Prisma)" },
                    if geraet.laser_an { "an" } else { "aus" }
                ))
                .small(),
            );
            for e in [&geraet.test_ergebnis, &geraet.schalter_ergebnis].into_iter().flatten() {
                let farbe = if e.ok { Color32::from_rgb(0, 120, 0) } else { Color32::from_rgb(190, 40, 40) };
                ui.add(egui::Label::new(RichText::new(&e.text).color(farbe).strong()).wrap());
            }
            ui.label(RichText::new(format!("Protokolldatei: {}", kommunikationslog::pfad().display())).small());
        });
    }

    fn gruppe_koppeln(&mut self, ui: &mut egui::Ui) {
        ui.group(|ui| {
            ui.set_width(ui.available_width());
            ui.strong("Bluetooth-Geräte (gekoppelt)");
            ui.label("Die Liste der gekoppelten Geräte steht in den Windows-Bluetooth-Einstellungen; vorhandene COM-Ports siehe oben.");
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                let knopf = egui::Button::new(RichText::new("+ Neues Gerät koppeln").strong().color(Color32::WHITE))
                    .fill(Color32::from_rgb(50, 120, 70))
                    .min_size(Vec2::new(220.0, 40.0));
                if ui.add(knopf).clicked() {
                    let ergebnis = std::process::Command::new("cmd").args(["/C", "start", "", "ms-settings:bluetooth"]).spawn();
                    match ergebnis {
                        Ok(_) => {
                            self.meldung = "Füge ein Gerät in den Windows-Einstellungen hinzu. Klicke dann auf 'Aktualisieren'.".to_string();
                            self.fehler = false;
                        }
                        Err(_) => {
                            self.meldung = "Bluetooth-Einstellungen konnten nicht geöffnet werden (Windows: Einstellungen → Bluetooth & Geräte).".to_string();
                            self.fehler = true;
                        }
                    }
                }
                ui.label(RichText::new("Öffnet Windows Bluetooth-Einstellungen.").italics());
            });
        });
    }

    fn gruppe_parameter(&mut self, ui: &mut egui::Ui, geraet: &mut Geraet) {
        ui.group(|ui| {
            ui.set_width(ui.available_width());
            ui.strong("Kommunikationsparameter");
            ui.label(RichText::new("Voreinstellungen werden automatisch je nach Geräteauswahl gesetzt.").italics());
            let manuell = geraet.modell == TachymeterModell::Manuell;
            let verbunden = geraet.ist_verbunden();
            ui.add_enabled_ui(manuell && !verbunden, |ui| {
                egui::Grid::new("parameter").num_columns(4).spacing([14.0, 10.0]).show(ui, |ui| {
                    let e = &mut geraet.einstellung;
                    ui.label("Baudrate:");
                    egui::ComboBox::from_id_salt("baud").selected_text(e.baud.to_string()).show_ui(ui, |ui| {
                        for b in BAUDRATEN {
                            ui.selectable_value(&mut e.baud, b, b.to_string());
                        }
                    });
                    ui.label("Datenbits:");
                    egui::ComboBox::from_id_salt("bits").selected_text(e.datenbits.to_string()).show_ui(ui, |ui| {
                        for b in DATENBITS {
                            ui.selectable_value(&mut e.datenbits, b, b.to_string());
                        }
                    });
                    ui.end_row();
                    ui.label("Parität:");
                    egui::ComboBox::from_id_salt("par").selected_text(e.paritaet.clone()).show_ui(ui, |ui| {
                        for p in PARITAETEN {
                            ui.selectable_value(&mut e.paritaet, p.to_string(), p);
                        }
                    });
                    ui.label("Stoppbits:");
                    egui::ComboBox::from_id_salt("stop").selected_text(e.stopbits.clone()).show_ui(ui, |ui| {
                        for s in STOPPBITS {
                            ui.selectable_value(&mut e.stopbits, s.to_string(), s);
                        }
                    });
                    ui.end_row();
                });
            });
            if geraet.einstellung.stopbits == "1.5" {
                ui.label(RichText::new(HINWEIS_STOPPBITS).small());
            }
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.add_enabled_ui(!verbunden, |ui| {
                    ui.checkbox(&mut geraet.einstellung.dtr, "DTR")
                        .on_hover_text("Steuerleitung DTR beim Öffnen setzen (Original: aus). Manche Kabel/Adapter brauchen DTR/RTS.");
                    ui.checkbox(&mut geraet.einstellung.rts, "RTS")
                        .on_hover_text("Steuerleitung RTS beim Öffnen setzen (Original: aus).");
                });
                ui.label("Zeilenende:");
                let vorher = geraet.einstellung.zeilenende;
                egui::ComboBox::from_id_salt("zeilenende")
                    .selected_text(vorher.name())
                    .show_ui(ui, |ui| {
                        for z in [Zeilenende::CrLf, Zeilenende::Cr, Zeilenende::Lf] {
                            ui.selectable_value(&mut geraet.einstellung.zeilenende, z, z.name());
                        }
                    });
                if geraet.einstellung.zeilenende != vorher {
                    geraet.verbindung.setze_zeilenende(geraet.einstellung.zeilenende);
                }
            });
        });
    }

    /// Startet die Analyse im Hintergrund.
    fn starte_analyse(&mut self, port: String) {
        self.analyse_abbruch.store(false, Ordering::SeqCst);
        if let Ok(mut z) = self.analyse.lock() {
            *z = AnalyseZustand { laeuft: true, status: "Analyse gestartet …".to_string(), treffer: Vec::new() };
        }
        let (zustand, abbruch) = (Arc::clone(&self.analyse), Arc::clone(&self.analyse_abbruch));
        std::thread::spawn(move || {
            let z1 = Arc::clone(&zustand);
            let z2 = Arc::clone(&zustand);
            analysiere(
                &port,
                &abbruch,
                move |text| {
                    if let Ok(mut z) = z1.lock() {
                        z.status = text;
                    }
                },
                move |t| {
                    if let Ok(mut z) = z2.lock() {
                        z.treffer.push(t);
                    }
                },
            );
            if let Ok(mut z) = zustand.lock() {
                z.laeuft = false;
            }
        });
    }

    fn gruppe_analyse(&mut self, ui: &mut egui::Ui, geraet: &mut Geraet) {
        ui.group(|ui| {
            ui.set_width(ui.available_width());
            ui.strong("Verbindungsanalyse");
            ui.label("Probiert Baudraten, Zeichenformate und DTR/RTS durch und zeigt, womit das Gerät antwortet (Port muss getrennt sein).");
            let (laeuft, status, treffer) = match self.analyse.lock() {
                Ok(z) => (z.laeuft, z.status.clone(), z.treffer.clone()),
                Err(_) => (false, String::new(), Vec::new()),
            };
            ui.horizontal(|ui| {
                let kann = !laeuft && !geraet.ist_verbunden() && !geraet.einstellung.port.is_empty();
                if ui.add_enabled(kann, egui::Button::new("Analyse starten")).clicked() {
                    self.starte_analyse(geraet.einstellung.port.clone());
                }
                if ui.add_enabled(laeuft, egui::Button::new("Abbrechen")).clicked() {
                    self.analyse_abbruch.store(true, Ordering::SeqCst);
                }
                if laeuft {
                    ui.spinner();
                }
            });
            if !status.is_empty() {
                ui.label(RichText::new(status).small());
            }
            egui::ScrollArea::vertical().id_salt("analyse_liste").max_height(180.0).show(ui, |ui| {
                for t in &treffer {
                    let farbe = if t.brauchbar { Color32::from_rgb(0, 110, 0) } else { Color32::GRAY };
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new(format!("{} | {}", t.einstellung, t.befehl)).color(farbe).strong());
                        if t.brauchbar && ui.small_button("Übernehmen").clicked() {
                            let z = geraet.einstellung.zeilenende;
                            geraet.einstellung = t.werte.clone();
                            geraet.einstellung.zeilenende = z;
                            self.meldung = format!("Einstellung übernommen: {}", t.werte.kurz());
                            self.fehler = false;
                        }
                    });
                    ui.label(RichText::new(format!("{}   |{}|", t.antwort_hex, t.antwort_text)).monospace().size(11.0));
                }
            });
        });
    }

    fn gruppe_protokoll(&mut self, ui: &mut egui::Ui, geraet: &mut Geraet) {
        ui.group(|ui| {
            ui.set_width(ui.available_width());
            ui.strong("Empfangsprotokoll und Testbefehl");
            egui::ScrollArea::vertical().max_height(230.0).stick_to_bottom(true).show(ui, |ui| {
                ui.set_width(ui.available_width());
                for zeile in &geraet.protokoll {
                    ui.label(RichText::new(zeile).monospace().size(12.0));
                }
            });
            ui.horizontal(|ui| {
                let antwort = ui.add(egui::TextEdit::singleline(&mut self.befehl).desired_width(300.0).hint_text("Befehl, z. B. %R1Q,0,0:"));
                let senden = ui.button("Senden").clicked() || (antwort.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
                if senden {
                    match geraet.sende(self.befehl.trim()) {
                        Ok(()) => {
                            self.meldung.clear();
                        }
                        Err(e) => {
                            self.meldung = e;
                            self.fehler = true;
                        }
                    }
                }
                if ui.button("Protokoll leeren").clicked() {
                    geraet.protokoll.clear();
                }
            });
            ui.checkbox(&mut geraet.hex_anzeige, "Rohbytes als Hex anzeigen (TX = gesendet, RX = empfangen)");
        });
    }

    fn verbinden_umschalten(&mut self, geraet: &mut Geraet) {
        if geraet.verbindet() {
            geraet.abbrechen_verbinden();
            return;
        }
        if geraet.ist_verbunden() {
            geraet.trennen();
            self.meldung.clear();
            return;
        }
        if !geraet.einstellung.port.to_uppercase().starts_with("COM") || geraet.einstellung.port.len() <= 3 {
            self.meldung = "Bitte einen gültigen COM-Port auswählen.".to_string();
            self.fehler = true;
            return;
        }
        geraet.test_ergebnis = None;
        match geraet.starte_verbinden() {
            Ok(()) => self.meldung.clear(),
            Err(e) => {
                self.meldung = format!("Verbindung fehlgeschlagen:\n{e}");
                self.fehler = true;
            }
        }
    }

    /// Speichert Modell, Sprache und Schnittstelle in den Einstellungen.
    fn uebernehmen(&mut self, projekt: &mut Projekt, geraet: &Geraet) {
        geraet.in_einstellungen(&mut projekt.einstellungen);
        if let Err(e) = projekt.einstellungen.speichern() {
            self.meldung = format!("Einstellungen konnten nicht gespeichert werden: {e}");
            self.fehler = true;
        }
    }
}
