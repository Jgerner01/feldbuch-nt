// Zweck:    Freie Stationierung mit Handeingabe der Anschlusspunkte: Berechnung, Residuen, Übernahme
// Hardware: keine
// Autor:    Johann Gerner

use eframe::egui;

use super::tabelle::{Raster, Zellen};
use crate::daten::modelle::{StationierungsErgebnis, StationierungsPunkt, jetzt};
use crate::daten::projekt::Projekt;
use crate::grundlagen::format::{de, parse_zahl};
use crate::grundlagen::protokoll;
use crate::protokolle::{schreibe_fuer_projekt, stationierung};
use crate::rechnen::freie_stationierung::{self, Optionen};
use crate::rechnen::neupunkt::pruefe_station;

/// Breiten der Tabellenspalten (Nr, R, H, Höhe, Hz, V, Strecke, Zielhöhe, 3 Haken, Aktionen).
const SPALTEN_BREITEN: [f32; 12] = [90.0, 110.0, 110.0, 80.0, 80.0, 70.0, 90.0, 90.0, 38.0, 38.0, 38.0, 64.0];
/// Breite der Zeilenköpfe (wie RowHeadersWidth im DataGridView).
const ZEILENKOPF_BREITE: f32 = 32.0;
/// Breiten der Residuentabelle.
const RESIDUEN_BREITEN: [f32; 6] = [90.0, 120.0, 110.0, 110.0, 120.0, 110.0];

/// Eingabezeile eines Anschlusspunkts.
#[derive(Clone)]
struct Zeile {
    nr: String,
    r: String,
    h: String,
    hoehe: String,
    hz: String,
    v: String,
    strecke: String,
    zielhoehe: String,
    akt_hz: bool,
    akt_str: bool,
    akt_hoe: bool,
}

impl Default for Zeile {
    fn default() -> Self {
        Self {
            nr: String::new(),
            r: String::new(),
            h: String::new(),
            hoehe: String::new(),
            hz: String::new(),
            v: "100".to_string(),
            strecke: String::new(),
            zielhoehe: "0".to_string(),
            akt_hz: true,
            akt_str: true,
            akt_hoe: true,
        }
    }
}

/// Maske der Freien Stationierung.
pub struct FreieStationierungAnsicht {
    standpunkt: String,
    instr_hoehe: String,
    zeilen: Vec<Zeile>,
    ergebnis: Option<(StationierungsErgebnis, Vec<StationierungsPunkt>)>,
    meldung: String,
    fehler: bool,
}

impl Default for FreieStationierungAnsicht {
    fn default() -> Self {
        Self {
            standpunkt: String::new(),
            instr_hoehe: "0".to_string(),
            zeilen: vec![Zeile::default(), Zeile::default()],
            ergebnis: None,
            meldung: String::new(),
            fehler: false,
        }
    }
}

impl FreieStationierungAnsicht {
    pub fn zeichne(&mut self, ui: &mut egui::Ui, projekt: &mut Projekt) {
        ui.heading("Freie Stationierung");
        egui::Grid::new("fs_kopf").num_columns(4).spacing([12.0, 6.0]).show(ui, |ui| {
            ui.label("Standpunkt-Nr:");
            ui.add(egui::TextEdit::singleline(&mut self.standpunkt).desired_width(120.0));
            ui.label("Instrumentenhöhe [m]:");
            ui.add(egui::TextEdit::singleline(&mut self.instr_hoehe).desired_width(80.0));
            ui.end_row();
        });
        ui.add_space(6.0);

        let mut loeschen = None;
        let mut suchen = None;
        let raster = Raster::neu(&SPALTEN_BREITEN).mit_zeilenkopf(ZEILENKOPF_BREITE);
        egui::ScrollArea::horizontal().show(ui, |ui| {
            ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
            raster.kopf(ui, &["Punkt-Nr", "R [m]", "H [m]", "Höhe [m]", "Hz [gon]", "V [gon]", "Strecke [m]", "Zielhöhe [m]", "Hz", "S", "Hö", ""]);
            for (i, z) in self.zeilen.iter_mut().enumerate() {
                raster.zeile(ui, i, &(i + 1).to_string(), |zellen| {
                    zellen.eingabe(&mut z.nr);
                    zellen.eingabe(&mut z.r);
                    zellen.eingabe(&mut z.h);
                    zellen.eingabe(&mut z.hoehe);
                    zellen.eingabe(&mut z.hz);
                    zellen.eingabe(&mut z.v);
                    zellen.eingabe(&mut z.strecke);
                    zellen.eingabe(&mut z.zielhoehe);
                    zellen.haken(&mut z.akt_hz);
                    zellen.haken(&mut z.akt_str);
                    zellen.haken(&mut z.akt_hoe);
                    zellen.zelle(|ui| {
                        if ui.small_button("🔍").on_hover_text("Koordinaten zur Nummer suchen").clicked() {
                            suchen = Some(i);
                        }
                        if ui.small_button("✕").on_hover_text("Zeile entfernen").clicked() {
                            loeschen = Some(i);
                        }
                    });
                });
            }
        });
        ui.add_space(6.0);
        if let Some(i) = suchen {
            self.suche_koordinaten(i, projekt);
        }
        if let Some(i) = loeschen {
            if self.zeilen.len() > 1 {
                self.zeilen.remove(i);
            }
        }

        ui.horizontal(|ui| {
            if ui.button("Zeile hinzufügen").clicked() {
                self.zeilen.push(Zeile::default());
            }
            if ui.button("Berechnen").clicked() {
                self.berechne(projekt);
            }
            let kann_uebernehmen = self.ergebnis.is_some() && !self.standpunkt.trim().is_empty();
            if ui.add_enabled(kann_uebernehmen, egui::Button::new("Übernehmen")).clicked() {
                self.uebernehme(projekt);
            }
            if ui.add_enabled(self.ergebnis.is_some(), egui::Button::new("Protokoll")).clicked() {
                self.schreibe_protokoll(projekt);
            }
        });

        if !self.meldung.is_empty() {
            let farbe = if self.fehler { egui::Color32::RED } else { ui.visuals().text_color() };
            ui.colored_label(farbe, &self.meldung);
        }
        if let Some((ergebnis, _)) = &self.ergebnis {
            ui.separator();
            zeige_ergebnis(ui, ergebnis, projekt);
        }
    }

    /// Schreibt das RTF-Protokoll der letzten Berechnung.
    fn schreibe_protokoll(&mut self, projekt: &Projekt) {
        let Some((erg, punkte)) = &self.ergebnis else {
            return;
        };
        let (hz, st, hoe) = self
            .lese_eingaben()
            .map(|(_, hz, st, hoe)| (hz, st, hoe))
            .unwrap_or_else(|_| (vec![true; punkte.len()], vec![true; punkte.len()], vec![true; punkte.len()]));
        let ih = parse_zahl(&self.instr_hoehe).unwrap_or(0.0);
        let p = stationierung::protokoll(
            erg,
            punkte,
            (&hz, &st, &hoe),
            self.standpunkt.trim(),
            ih,
            &projekt.rechenparameter.params,
            &projekt.projektdaten.bearbeiter,
            jetzt(),
        );
        match schreibe_fuer_projekt(&p, projekt) {
            Ok(pfad) => {
                self.meldung = format!("Protokoll gespeichert:\n{}", pfad.display());
                self.fehler = false;
            }
            Err(text) => {
                self.meldung = text;
                self.fehler = true;
            }
        }
    }

    fn suche_koordinaten(&mut self, index: usize, projekt: &Projekt) {
        let nr = self.zeilen[index].nr.clone();
        match projekt.finde_anschlusspunkt(&nr) {
            Some((r, h, hoehe)) => {
                let z = &mut self.zeilen[index];
                z.r = format!("{r:.3}");
                z.h = format!("{h:.3}");
                z.hoehe = format!("{hoehe:.3}");
                self.meldung.clear();
            }
            None => {
                self.meldung = format!("Punkt „{nr}“ wurde nicht gefunden.");
                self.fehler = true;
            }
        }
    }

    /// Liest die Eingaben; `Err` mit Zeilennummer bei ungültigen Zahlen.
    fn lese_eingaben(&self) -> Result<(Vec<StationierungsPunkt>, Vec<bool>, Vec<bool>, Vec<bool>), String> {
        let (mut punkte, mut hz, mut st, mut hoe) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
        for (i, z) in self.zeilen.iter().enumerate() {
            if z.nr.trim().is_empty() && z.r.trim().is_empty() && z.hz.trim().is_empty() {
                continue;
            }
            let zahl = |text: &str, name: &str| {
                parse_zahl(text).ok_or_else(|| format!("Zeile {}: ungültiger Wert für {name}.", i + 1))
            };
            punkte.push(StationierungsPunkt {
                punkt_nr: z.nr.trim().to_string(),
                r: zahl(&z.r, "R")?,
                h: zahl(&z.h, "H")?,
                hoehe: parse_zahl(&z.hoehe).unwrap_or(0.0),
                hz: zahl(&z.hz, "Hz")?,
                v: zahl(&z.v, "V")?,
                strecke: zahl(&z.strecke, "Strecke")?,
                zielhoehe: parse_zahl(&z.zielhoehe).unwrap_or(0.0),
            });
            hz.push(z.akt_hz);
            st.push(z.akt_str);
            hoe.push(z.akt_hoe);
        }
        Ok((punkte, hz, st, hoe))
    }

    fn berechne(&mut self, projekt: &Projekt) {
        self.ergebnis = None;
        let ergebnis = self.lese_eingaben().and_then(|(punkte, hz, st, hoe)| {
            let par = &projekt.rechenparameter.params;
            let ih = parse_zahl(&self.instr_hoehe).unwrap_or(0.0);
            let optionen = Optionen {
                freier_massstab: par.freier_massstab,
                aktiv_richtung: Some(hz),
                aktiv_strecke: Some(st),
                aktiv_hoehe: Some(hoe),
                berechnung_3d: par.berechnung_3d,
                fehlergrenze_mm_hoehe: par.fehlergrenze_mm_hoehe,
                ..Optionen::default()
            };
            freie_stationierung::berechnen(&punkte, ih, &optionen).map(|e| (e, punkte))
        });
        match ergebnis {
            Ok((e, punkte)) => {
                self.meldung = if e.konvergiert {
                    String::new()
                } else {
                    "Die Ausgleichung ist nicht konvergiert.".to_string()
                };
                self.fehler = !e.konvergiert;
                protokoll::log("RECHNUNG", &format!("Freie Stationierung berechnet, s0 = {} mm", de(e.s0_mm, 2)));
                self.ergebnis = Some((e, punkte));
            }
            Err(fehler) => {
                self.meldung = fehler;
                self.fehler = true;
            }
        }
    }

    fn uebernehme(&mut self, projekt: &mut Projekt) {
        let Some((ergebnis, punkte)) = self.ergebnis.clone() else {
            return;
        };
        let ih = parse_zahl(&self.instr_hoehe).unwrap_or(0.0);
        let nr = self.standpunkt.trim().to_string();
        let ergebnis_pruefung = pruefe_station(&ergebnis)
            .and_then(|()| projekt.uebernehme_manuelle_stationierung(&nr, ih, punkte, ergebnis));
        match ergebnis_pruefung {
            Ok(()) => {
                self.meldung = format!("Standpunkt {nr} übernommen.");
                self.fehler = false;
            }
            Err(fehler) => {
                self.meldung = format!("Übernahme fehlgeschlagen: {fehler}");
                self.fehler = true;
            }
        }
    }
}

fn zeige_ergebnis(ui: &mut egui::Ui, e: &StationierungsErgebnis, projekt: &Projekt) {
    let p = &projekt.rechenparameter.params;
    egui::Grid::new("fs_ergebnis").num_columns(4).spacing([16.0, 4.0]).show(ui, |ui| {
        ui.label("R [m]");
        ui.monospace(de(e.r, 3));
        ui.label("H [m]");
        ui.monospace(de(e.h, 3));
        ui.end_row();
        ui.label("Höhe [m]");
        ui.monospace(if e.berechnung_3d { de(e.hoehe, 3) } else { "–".to_string() });
        ui.label("Orientierung [gon]");
        ui.monospace(de(e.orientierung_gon, 4));
        ui.end_row();
        ui.label("Maßstab");
        ui.monospace(de(e.massstab, 6));
        ui.label("Genauigkeit");
        ui.monospace(p.sigma_text(e.s0_mm, 2));
        ui.end_row();
        ui.label("Redundanz");
        ui.monospace(e.redundanz.to_string());
        ui.label("Iterationen");
        ui.monospace(e.iterationen.to_string());
        ui.end_row();
    });
    if !e.warnung_hoehe.is_empty() {
        ui.colored_label(egui::Color32::from_rgb(200, 120, 0), &e.warnung_hoehe);
    }
    ui.add_space(6.0);
    ui.strong("Residuen");
    let raster = Raster::neu(&RESIDUEN_BREITEN).mit_zeilenkopf(ZEILENKOPF_BREITE);
    ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
    raster.kopf(ui, &["Punkt", "Strecke H [m]", "vWinkel [cc]", "vQuer [mm]", "vStrecke [mm]", "vHöhe [mm]"]);
    for (i, r) in e.residuen.iter().enumerate() {
        raster.zeile(ui, i, &(i + 1).to_string(), |zellen| {
            zellen.text(&r.punkt_nr);
            zellen.zahl(&de(r.strecke_h, 3));
            wert_mit_grenze(zellen, r.v_winkel_cc, p.fehlergrenze_cc_winkel);
            wert_mit_grenze(zellen, r.v_quer_mm, p.fehlergrenze_mm_strecke);
            wert_mit_grenze(zellen, r.v_strecke_mm, p.fehlergrenze_mm_strecke);
            wert_mit_grenze(zellen, r.v_hoehe_mm, p.fehlergrenze_mm_hoehe);
        });
    }
}

/// Zeigt einen Residuenwert und färbt ihn rot, wenn die Fehlergrenze überschritten ist.
fn wert_mit_grenze(zellen: &mut Zellen<'_>, wert: f64, grenze: f64) {
    if wert.is_nan() {
        zellen.zahl("–");
    } else if wert.abs() > grenze {
        zellen.zahl_farbig(&de(wert, 1), egui::Color32::RED);
    } else {
        zellen.zahl(&de(wert, 1));
    }
}
