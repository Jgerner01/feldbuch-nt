// Zweck:    Gemeinsamer Dialog „Neuer Stp" für DXF-Viewer und Freie Stationierung: neuen Standpunkt bzw. neue
//           Aufstellung anlegen, vorhandene Aufstellung fortsetzen, Standpunkt umbenennen oder zusammenführen.
//           Einziger Weg, eine Aufstellung zu beginnen (siehe Standpunkt.md).
// Hardware: liest den Gerätezustand (Modell, EDM, Prisma, letzte Messung) für das Stationsprotokoll
// Autor:    Johann Gerner

use eframe::egui::{self, Align2, Color32, RichText, Vec2};

use super::bildschirmtastatur;
use super::geraet::Geraet;
use crate::daten::projekt::{BekannterStandpunkt, Geraetezustand, NeuerStandpunkt, Projekt, StandpunktZeile};
use crate::grundlagen::format::{f, parse_zahl};

/// Breite des Dialogs.
const BREITE: f32 = 640.0;
/// Höhe der Standpunktliste.
const LISTE_HOEHE: f32 = 170.0;
/// Breite der Eingabefelder.
const FELD_BREITE: f32 = 150.0;
/// Farben.
const FARBE_WARNUNG: Color32 = Color32::from_rgb(200, 120, 20);
const FARBE_FEHLER: Color32 = Color32::from_rgb(190, 40, 30);
const FARBE_OK: Color32 = Color32::from_rgb(40, 140, 60);
const FARBE_KNOPF: Color32 = Color32::from_rgb(220, 120, 20);
/// mgon je Radiant.
const MGON_PRO_RAD: f64 = 200_000.0 / std::f64::consts::PI;

/// Gerätezustand für eine neue Aufstellung (V7): Einstellungen und, falls vorhanden, letzte Messung des Geräts.
pub fn geraetezustand(projekt: &Projekt, geraet: Option<&Geraet>) -> Geraetezustand {
    let e = &projekt.einstellungen;
    let letzte = geraet.and_then(|g| g.letzte_messung.as_ref());
    Geraetezustand {
        modell: e.tachymeter_modell.name().to_string(),
        verbunden: geraet.is_some_and(Geraet::ist_verbunden),
        edm_modus: match geraet {
            Some(g) if g.reflektorlos => "Reflektorlos".to_string(),
            Some(_) => "Prisma".to_string(),
            None => e.mess_modus_name.clone(),
        },
        prisma: e.prisma_name.clone(),
        prismenkonstante_mm: e.prisma_konstante_mm,
        atm_ppm: letzte.and_then(|m| m.atm_ppm),
        laengsneigung_mgon: letzte.and_then(|m| m.laengs_neigung_rad).map(|r| r * MGON_PRO_RAD),
        querneigung_mgon: letzte.and_then(|m| m.kreuz_neigung_rad).map(|r| r * MGON_PRO_RAD),
    }
}

/// Reiter des Dialogs.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Modus {
    Neu,
    Fortsetzen,
    Umbenennen,
}

/// Ergebnis eines Bildaufbaus.
pub enum Ergebnis {
    /// Dialog bleibt offen.
    Offen,
    /// Ohne Änderung geschlossen.
    Abgebrochen,
    /// Aufstellung angelegt oder fortgesetzt bzw. umbenannt; Text für die Statuszeile.
    Fertig { meldung: String, fortgesetzt: bool },
}

/// Dialog „Neuer Stp".
pub struct StandpunktDialog {
    modus: Modus,
    nr: String,
    ih: String,
    code: String,
    bekannt: bool,
    bekannt_nr: String,
    /// Bestätigung „neue Aufstellung unter gleicher Nummer".
    gleiche_nr_ok: bool,
    /// Bestätigung „offene Neupunkte der bisherigen Aufstellung" (V3).
    offene_ok: bool,
    uebersicht: Vec<StandpunktZeile>,
    auswahl: Option<usize>,
    umbenennen_neu: String,
    fehler: String,
    bericht: String,
    geraet: Geraetezustand,
}

impl StandpunktDialog {
    /// Dialog für einen neuen Standpunkt mit Nummernvorschlag (STP1 bzw. höchste Nummer + 1).
    pub fn neu(projekt: &Projekt, geraet: Geraetezustand) -> Self {
        let ih = if projekt.hat_aktive_aufstellung() { projekt.station.instrumenten_hoehe } else { projekt.einstellungen.instrumenten_hoehe };
        Self::mit_nummer(projekt, &projekt.naechste_standpunkt_nr(), ih, geraet)
    }

    /// Dialog mit vorgegebener Nummer und Instrumentenhöhe (z. B. aus einer geladenen stp.csv).
    pub fn mit_nummer(projekt: &Projekt, nr: &str, ih: f64, geraet: Geraetezustand) -> Self {
        let uebersicht = projekt.standpunkte_uebersicht();
        let auswahl = uebersicht.iter().position(|z| z.aktiv).or_else(|| uebersicht.len().checked_sub(1));
        Self {
            modus: Modus::Neu,
            nr: nr.to_string(),
            ih: f(ih, 3),
            code: String::new(),
            bekannt: false,
            bekannt_nr: String::new(),
            gleiche_nr_ok: false,
            offene_ok: false,
            uebersicht,
            auswahl,
            umbenennen_neu: String::new(),
            fehler: String::new(),
            bericht: String::new(),
            geraet,
        }
    }

    /// Zeichnet den Dialog.
    pub fn zeige(&mut self, ctx: &egui::Context, projekt: &mut Projekt) -> Ergebnis {
        let mut ergebnis = Ergebnis::Offen;
        let mut offen = true;
        egui::Window::new("Standpunkt").open(&mut offen).collapsible(false).resizable(false).anchor(Align2::CENTER_CENTER, Vec2::ZERO).show(ctx, |ui| {
            ui.set_width(BREITE);
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.modus, Modus::Neu, RichText::new("Neuer Standpunkt").strong());
                ui.selectable_value(&mut self.modus, Modus::Fortsetzen, RichText::new("Vorhandenen fortsetzen").strong());
                ui.selectable_value(&mut self.modus, Modus::Umbenennen, RichText::new("Umbenennen / Zusammenführen").strong());
            });
            ui.separator();
            ergebnis = match self.modus {
                Modus::Neu => self.zeichne_neu(ui, projekt),
                Modus::Fortsetzen => self.zeichne_fortsetzen(ui, projekt),
                Modus::Umbenennen => self.zeichne_umbenennen(ui, projekt),
            };
            if !self.fehler.is_empty() {
                ui.add_space(4.0);
                ui.colored_label(FARBE_FEHLER, &self.fehler);
            }
        });
        if !offen {
            return Ergebnis::Abgebrochen;
        }
        ergebnis
    }

    /// Reiter „Neuer Standpunkt".
    fn zeichne_neu(&mut self, ui: &mut egui::Ui, projekt: &mut Projekt) -> Ergebnis {
        egui::Grid::new("stp_neu").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
            ui.label("Standpunktnummer:");
            let antwort = ui.add_sized([FELD_BREITE, 24.0], egui::TextEdit::singleline(&mut self.nr).font(egui::TextStyle::Monospace));
            bildschirmtastatur::text_feld(&antwort);
            if antwort.changed() {
                self.gleiche_nr_ok = false;
            }
            ui.end_row();
            ui.label("Instrumentenhöhe [m]:");
            ui.add_sized([FELD_BREITE, 24.0], egui::TextEdit::singleline(&mut self.ih).font(egui::TextStyle::Monospace));
            ui.end_row();
            ui.label("Code (optional):");
            ui.add_sized([FELD_BREITE, 24.0], egui::TextEdit::singleline(&mut self.code).font(egui::TextStyle::Monospace));
            ui.end_row();
        });
        ui.add_space(4.0);
        ui.checkbox(&mut self.bekannt, "Standpunkt ist ein bekannter Punkt (ein Anschlussziel genügt)");
        let mut bekannt_punkt = None;
        if self.bekannt {
            ui.horizontal(|ui| {
                ui.label("   Punktnummer des bekannten Punkts:");
                let antwort = ui.add_sized([FELD_BREITE, 24.0], egui::TextEdit::singleline(&mut self.bekannt_nr).font(egui::TextStyle::Monospace));
                bildschirmtastatur::text_feld(&antwort);
            });
            match projekt.finde_punkt_mit_standpunkten(self.bekannt_nr.trim()) {
                Some((r, h, hoehe)) => {
                    let hoehe_text = if hoehe.is_finite() { format!("  Höhe {}", f(hoehe, 3)) } else { "  ohne Höhe".to_string() };
                    ui.colored_label(FARBE_OK, format!("   R {}   H {}{hoehe_text}", f(r, 3), f(h, 3)));
                    bekannt_punkt = Some(BekannterStandpunkt { punkt_nr: self.bekannt_nr.trim().to_string(), r, h, hoehe });
                }
                None if !self.bekannt_nr.trim().is_empty() => {
                    ui.colored_label(FARBE_FEHLER, "   Keine Koordinaten zu dieser Punktnummer gefunden.");
                }
                None => {}
            }
        }

        // Prüfungen und Hinweise
        let ih = parse_zahl(&self.ih).filter(|v| v.is_finite() && *v >= 0.0);
        let mut sperre: Option<String> = None;
        match projekt.pruefe_standpunkt_nr(&self.nr) {
            Err(e) => sperre = Some(e),
            Ok(hinweise) => {
                for h in hinweise {
                    ui.colored_label(FARBE_WARNUNG, format!("⚠ {h}"));
                }
            }
        }
        if ih.is_none() {
            sperre.get_or_insert_with(|| "Bitte eine gültige Instrumentenhöhe eingeben.".into());
        }
        if let Some(h) = ih.and_then(|v| projekt.ih_hinweis(v)) {
            ui.colored_label(FARBE_WARNUNG, format!("⚠ {h}"));
        }
        if self.bekannt && bekannt_punkt.is_none() {
            sperre.get_or_insert_with(|| "Bitte einen bekannten Punkt mit Koordinaten angeben.".into());
        }
        if projekt.standpunkt_existiert(&self.nr) {
            ui.colored_label(FARBE_WARNUNG, format!("⚠ {} existiert bereits in diesem Projekt.", self.nr.trim()));
            ui.checkbox(&mut self.gleiche_nr_ok, "Neue Aufstellung unter gleicher Nummer beginnen (bisherige Station wird archiviert)");
            if !self.gleiche_nr_ok {
                sperre.get_or_insert_with(|| "Bitte bestätigen oder eine andere Nummer wählen.".into());
            }
        }
        let offene = projekt.offene_neupunkte();
        if offene > 0 && !projekt.hat_gueltige_station() {
            ui.colored_label(
                FARBE_WARNUNG,
                format!("⚠ {} hat {offene} Neupunkt(e) ohne gültige Stationierung. Sie bleiben als Rohdaten erhalten.", projekt.station.standpunkt_nr),
            );
            ui.checkbox(&mut self.offene_ok, "Trotzdem neuen Standpunkt beginnen");
            if !self.offene_ok {
                sperre.get_or_insert_with(|| "Bitte die offenen Neupunkte bestätigen.".into());
            }
        }
        ui.add_space(4.0);
        ui.label(RichText::new(format!("Gerät: {}", self.geraet.text())).small().weak());
        ui.add_space(6.0);
        self.zeichne_liste(ui);
        ui.add_space(6.0);

        let mut ergebnis = Ergebnis::Offen;
        ui.horizontal(|ui| {
            let knopf = egui::Button::new(RichText::new("Standpunkt anlegen").strong().color(Color32::WHITE)).fill(FARBE_KNOPF).min_size(Vec2::new(180.0, 32.0));
            let antwort = ui.add_enabled(sperre.is_none(), knopf);
            let antwort = match &sperre {
                Some(grund) => antwort.on_disabled_hover_text(grund.as_str()),
                None => antwort,
            };
            if antwort.clicked() {
                let neu = NeuerStandpunkt {
                    nr: self.nr.trim().to_string(),
                    ih: ih.unwrap_or(0.0),
                    code: self.code.trim().to_string(),
                    bekannt: bekannt_punkt.clone(),
                    geraet: Some(self.geraet.clone()),
                };
                match projekt.standpunkt_beginnen(neu) {
                    Ok(()) => {
                        projekt.einstellungen.instrumenten_hoehe = ih.unwrap_or(0.0);
                        let _ = projekt.einstellungen.speichern();
                        ergebnis = Ergebnis::Fertig {
                            meldung: format!("Neuer Standpunkt {} angelegt (IH {} m).", projekt.station.standpunkt_nr, f(projekt.station.instrumenten_hoehe, 3)),
                            fortgesetzt: false,
                        };
                    }
                    Err(e) => self.fehler = e,
                }
            }
            if ui.button("Abbrechen").clicked() {
                ergebnis = Ergebnis::Abgebrochen;
            }
        });
        if let Some(grund) = &sperre {
            ui.label(RichText::new(grund).small().color(FARBE_WARNUNG));
        }
        ergebnis
    }

    /// Reiter „Vorhandenen fortsetzen".
    fn zeichne_fortsetzen(&mut self, ui: &mut egui::Ui, projekt: &mut Projekt) -> Ergebnis {
        ui.label("Das Instrument steht noch unverändert auf einer bereits angelegten Aufstellung:");
        ui.add_space(4.0);
        self.zeichne_liste(ui);
        ui.add_space(6.0);
        let mut ergebnis = Ergebnis::Offen;
        ui.horizontal(|ui| {
            let zeile = self.auswahl.and_then(|i| self.uebersicht.get(i)).cloned();
            let knopf = egui::Button::new(RichText::new("Fortsetzen").strong()).min_size(Vec2::new(140.0, 32.0));
            if ui.add_enabled(zeile.is_some(), knopf).clicked() {
                if let Some(z) = zeile {
                    match projekt.standpunkt_fortsetzen(&z.nr) {
                        Ok(()) => ergebnis = Ergebnis::Fertig { meldung: format!("Aufstellung {} fortgesetzt.", z.nr), fortgesetzt: true },
                        Err(e) => self.fehler = e,
                    }
                }
            }
            if ui.button("Abbrechen").clicked() {
                ergebnis = Ergebnis::Abgebrochen;
            }
        });
        ui.label(RichText::new("Nach dem Fortsetzen empfiehlt sich eine Orientierungskontrolle auf einen Anschlusspunkt.").small().weak());
        ergebnis
    }

    /// Reiter „Umbenennen / Zusammenführen" (V4).
    fn zeichne_umbenennen(&mut self, ui: &mut egui::Ui, projekt: &mut Projekt) -> Ergebnis {
        ui.label("Standpunkt wählen und neue Nummer eingeben. Gibt es die neue Nummer mit derselben Aufstellung schon, werden beide zusammengeführt.");
        ui.add_space(4.0);
        self.zeichne_liste(ui);
        ui.add_space(6.0);
        let alt = self.auswahl.and_then(|i| self.uebersicht.get(i)).map(|z| z.nr.clone());
        ui.horizontal(|ui| {
            ui.label(format!("{}  →  neue Nummer:", alt.as_deref().unwrap_or("–")));
            let antwort = ui.add_sized([FELD_BREITE, 24.0], egui::TextEdit::singleline(&mut self.umbenennen_neu).font(egui::TextStyle::Monospace));
            bildschirmtastatur::text_feld(&antwort);
        });
        ui.label(RichText::new("Vorher werden alle betroffenen Dateien in den Ordner „Sicherung“ des Projekts kopiert.").small().weak());
        let mut ergebnis = Ergebnis::Offen;
        ui.horizontal(|ui| {
            let moeglich = alt.is_some() && !self.umbenennen_neu.trim().is_empty();
            if ui.add_enabled(moeglich, egui::Button::new(RichText::new("Umbenennen").strong()).min_size(Vec2::new(140.0, 32.0))).clicked() {
                if let Some(alt) = &alt {
                    match projekt.standpunkt_umbenennen(alt, &self.umbenennen_neu) {
                        Ok(bericht) => {
                            self.bericht = bericht;
                            self.fehler.clear();
                            self.uebersicht = projekt.standpunkte_uebersicht();
                            self.auswahl = self.uebersicht.iter().position(|z| z.nr == self.umbenennen_neu.trim());
                            self.umbenennen_neu.clear();
                        }
                        Err(e) => self.fehler = e,
                    }
                }
            }
            if ui.button("Schließen").clicked() {
                ergebnis = if self.bericht.is_empty() {
                    Ergebnis::Abgebrochen
                } else {
                    Ergebnis::Fertig { meldung: "Standpunkt umbenannt.".into(), fortgesetzt: false }
                };
            }
        });
        if !self.bericht.is_empty() {
            ui.add_space(4.0);
            ui.colored_label(FARBE_OK, &self.bericht);
        }
        ergebnis
    }

    /// Liste der Standpunkte des Projekts mit Auswahl.
    fn zeichne_liste(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new("Standpunkte im Projekt:").strong());
        if self.uebersicht.is_empty() {
            ui.label(RichText::new("   noch keine").weak());
            return;
        }
        egui::ScrollArea::vertical().id_salt("stp_liste").max_height(LISTE_HOEHE).auto_shrink([false, true]).show(ui, |ui| {
            egui::Grid::new("stp_liste_grid").num_columns(6).striped(true).spacing([14.0, 4.0]).show(ui, |ui| {
                for kopf in ["Standpunkt", "Beginn", "IH [m]", "AP", "s₀ [mm]", "Status"] {
                    ui.label(RichText::new(kopf).strong());
                }
                ui.end_row();
                for (i, z) in self.uebersicht.iter().enumerate() {
                    let text = if z.aktiv { format!("▶ {}", z.nr) } else { z.nr.clone() };
                    if ui.selectable_label(self.auswahl == Some(i), RichText::new(text).monospace()).clicked() {
                        self.auswahl = Some(i);
                    }
                    ui.label(&z.beginn);
                    ui.label(RichText::new(f(z.instr_hoehe, 3)).monospace());
                    ui.label(z.anschluss.to_string());
                    ui.label(z.s0_mm.map_or_else(|| "–".to_string(), |s| f(s, 1)));
                    let (status, farbe) = if z.gueltig { ("gültig", FARBE_OK) } else { ("offen", FARBE_WARNUNG) };
                    ui.colored_label(farbe, if z.aktiv { format!("{status}, aktiv") } else { status.to_string() });
                    ui.end_row();
                }
            });
        });
    }
}
