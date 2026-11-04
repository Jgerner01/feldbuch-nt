// Zweck:    Formular Achsabsteckung: Achse aus Anfangs- und Endpunkt, Stationen mit Versätzen, Lageplan und Einweiser
// Hardware: keine
// Autor:    Johann Gerner

use eframe::egui::{self, Color32, RichText, Vec2};

use super::gemeinsam::{
    Einweiser, PLAN_HOEHE, PlanDaten, ProtokollZiel, hintergrundpunkte, lade_anschlusspunkte, lese_zahl, zeichne_lageplan, zeige_standpunkt,
    zeilenfarbe,
};
use crate::daten::projekt::Projekt;
use crate::grundlagen::format::{f, parse_zahl};
use crate::gui::tabelle::Raster;
use crate::protokolle::absteckung::{AbsteckProtokoll, AchsEingabe, achsabsteckung};
use crate::rechnen::absteckung::{AbsteckPunkt, STATUS_ABGESTECKT, StandpunktInfo, berechne_achspunkte, lade_standpunkt};

/// Spaltenbreiten der Achstabelle: Punktnummer, R, H.
const BREITEN_ACHSE: [f32; 3] = [90.0, 110.0, 110.0];
/// Breite des Zeilenkopfs der Achstabelle.
const KOPF_ACHSE: f32 = 70.0;
/// Spaltenbreiten der Punkttabelle: Nr, Station, R, H, Abszisse, Ordinate, Hz, s, Status, Ziel, Entfernen.
const BREITEN_PUNKTE: [f32; 11] = [55.0, 140.0, 90.0, 90.0, 70.0, 60.0, 80.0, 70.0, 80.0, 30.0, 26.0];
/// Breite der Zeilenköpfe der Punkttabelle.
const KOPF_PUNKTE: f32 = 30.0;
/// Höhe des Punktebereichs.
const TABELLEN_HOEHE: f32 = 300.0;
/// Standard des Stationsintervalls und der Versätze.
const INTERVALL_STANDARD: &str = "10.0";
const OFFSETS_STANDARD: &str = "0";

#[derive(Clone, Default)]
struct Eckpunkt {
    nr: String,
    r: String,
    h: String,
}

/// Formular der Achsabsteckung.
pub struct AchseAnsicht {
    station: Option<StandpunktInfo>,
    achse_punkte: [Eckpunkt; 2],
    intervall: String,
    offsets: String,
    punkte: Vec<AbsteckPunkt>,
    aktuell: Option<usize>,
    anschluss: Vec<(String, String, String)>,
    /// Bekannte Projektpunkte, im Plan anklickbar.
    hintergrund: Vec<(String, f64, f64)>,
    einweiser: Einweiser,
    meldung: String,
    ziel: ProtokollZiel,
}

impl Drop for AchseAnsicht {
    fn drop(&mut self) {
        let p = self.protokoll_bauen();
        self.ziel.schreibe_beim_verlassen(p, "Achsabsteckung.protokoll");
    }
}

/// Versätze aus einem Text mit Komma, Semikolon oder Leerzeichen als Trenner; Ungültiges wird übersprungen.
fn lese_versaetze(text: &str) -> Vec<f64> {
    text.split([',', ';', ' ']).filter(|t| !t.is_empty()).filter_map(parse_zahl).collect()
}

impl AchseAnsicht {
    pub fn neu(projekt: &Projekt) -> Self {
        Self {
            station: lade_standpunkt(&projekt.projektdaten),
            achse_punkte: Default::default(),
            intervall: INTERVALL_STANDARD.to_string(),
            offsets: OFFSETS_STANDARD.to_string(),
            punkte: Vec::new(),
            aktuell: None,
            anschluss: Vec::new(),
            hintergrund: hintergrundpunkte(projekt),
            einweiser: Einweiser::default(),
            meldung: String::new(),
            ziel: ProtokollZiel::neu(projekt),
        }
    }

    /// Anfangs- und Endpunkt der Achse, wenn beide vollständig sind.
    fn achse(&self) -> Option<((f64, f64), (f64, f64))> {
        let a = &self.achse_punkte;
        Some(((parse_zahl(&a[0].r)?, parse_zahl(&a[0].h)?), (parse_zahl(&a[1].r)?, parse_zahl(&a[1].h)?)))
    }

    fn protokoll_bauen(&self) -> Option<AbsteckProtokoll> {
        if self.punkte.is_empty() {
            return None;
        }
        let a = &self.achse_punkte;
        let eingabe = AchsEingabe { r_a: &a[0].r, h_a: &a[0].h, r_e: &a[1].r, h_e: &a[1].h, intervall: &self.intervall, offsets: &self.offsets };
        Some(achsabsteckung(self.station.as_ref(), &self.punkte, self.achse(), &eingabe, &self.ziel.bearbeiter, ProtokollZiel::jetzt()))
    }

    /// Übernimmt einen angeklickten Projektpunkt als Anfang oder Ende der Achse (erste leere Zeile, sonst Anfang).
    fn fange(&mut self, index: usize) {
        let Some((nr, r, h)) = self.hintergrund.get(index).cloned() else {
            return;
        };
        let zeile = self.achse_punkte.iter().position(|e| e.r.trim().is_empty() || e.h.trim().is_empty()).unwrap_or(0);
        self.achse_punkte[zeile] = Eckpunkt { nr, r: f(r, 3), h: f(h, 3) };
    }

    fn waehle(&mut self, index: usize) {
        let Some(p) = self.punkte.get(index) else {
            return;
        };
        self.aktuell = Some(index);
        let soll = self.station.as_ref().map(|_| (p.hz_soll_gon, p.s_soll_m));
        self.einweiser.setze_ziel(Some(&p.punkt_nr), soll);
    }

    fn berechne(&mut self, projekt: &Projekt) {
        self.station = lade_standpunkt(&projekt.projektdaten);
        self.ziel.aktualisiere(projekt);
        let Some((a, e)) = self.achse() else {
            self.meldung = "Anfangs- und Endpunkt der Achse eingeben.".to_string();
            return;
        };
        let intervall = match lese_zahl(&self.intervall, "Intervall") {
            Ok(v) if v >= 0.01 => v,
            _ => {
                self.meldung = "Gültiges Intervall eingeben.".to_string();
                return;
            }
        };
        self.meldung.clear();
        self.punkte = berechne_achspunkte(a, e, intervall, &lese_versaetze(&self.offsets), self.station.as_ref());
        self.aktuell = None;
        if self.punkte.is_empty() {
            self.einweiser.zuruecksetzen();
            self.meldung = "Die Achse ist zu kurz.".to_string();
        } else {
            self.waehle(0);
        }
    }

    fn markiere(&mut self, punkt_nr: &str) {
        let Some(index) = self.punkte.iter().position(|p| p.punkt_nr == punkt_nr) else {
            return;
        };
        self.punkte[index].status = STATUS_ABGESTECKT.to_string();
        self.ziel.geschrieben = false;
        let n = self.punkte.len();
        match (1..=n).map(|d| (index + d) % n).find(|i| !self.punkte[*i].ist_abgesteckt()) {
            Some(i) => self.waehle(i),
            None => self.waehle(index),
        }
    }

    fn entferne(&mut self, index: usize) {
        if index < self.punkte.len() {
            self.punkte.remove(index);
            self.aktuell = None;
            self.einweiser.zuruecksetzen();
        }
    }

    pub fn zeichne(&mut self, ui: &mut egui::Ui, projekt: &Projekt) {
        ui.heading("Achsabsteckung");
        zeige_standpunkt(ui, self.station.as_ref());
        ui.add_space(4.0);
        ui.columns(2, |spalten| {
            self.zeichne_eingabe(&mut spalten[0], projekt);
            self.zeichne_karte(&mut spalten[1]);
        });
    }

    fn zeichne_eingabe(&mut self, ui: &mut egui::Ui, projekt: &Projekt) {
        let raster = Raster::neu(&BREITEN_ACHSE).mit_zeilenkopf(KOPF_ACHSE);
        raster.kopf(ui, &["PunktNr", "R [m]", "H [m]"]);
        for (i, name) in ["Anfang", "Ende"].iter().enumerate() {
            let p = &mut self.achse_punkte[i];
            raster.zeile(ui, i, name, |z| {
                z.eingabe(&mut p.nr);
                z.eingabe(&mut p.r);
                z.eingabe(&mut p.h);
            });
        }
        ui.horizontal(|ui| {
            if ui.small_button("Anschlusspunkte laden").clicked() {
                match lade_anschlusspunkte(projekt) {
                    Ok(liste) => {
                        self.anschluss = liste;
                        self.meldung.clear();
                    }
                    Err(fehler) => self.meldung = fehler,
                }
            }
            for (i, name) in ["Anfang", "Ende"].iter().enumerate() {
                if self.anschluss.is_empty() {
                    continue;
                }
                egui::ComboBox::from_id_salt(("achse_punkt", i)).selected_text(format!("{name} wählen")).show_ui(ui, |ui| {
                    for (nr, r, h) in &self.anschluss {
                        if ui.selectable_label(false, format!("{nr}  ({r} / {h})")).clicked() {
                            self.achse_punkte[i] = Eckpunkt { nr: nr.clone(), r: r.clone(), h: h.clone() };
                        }
                    }
                });
            }
            if ui.small_button("Achse leeren").clicked() {
                self.achse_punkte = Default::default();
                self.punkte.clear();
                self.aktuell = None;
                self.einweiser.zuruecksetzen();
            }
        });
        ui.horizontal(|ui| {
            ui.label("Intervall [m]:");
            ui.add(egui::TextEdit::singleline(&mut self.intervall).desired_width(70.0));
            ui.label("Versätze [m] (links +):");
            ui.add(egui::TextEdit::singleline(&mut self.offsets).desired_width(160.0));
        });
        ui.horizontal(|ui| {
            if ui.button(RichText::new("Berechnen").strong()).clicked() {
                self.berechne(projekt);
            }
            if ui.add_enabled(!self.punkte.is_empty(), egui::Button::new("Protokoll")).clicked() {
                self.meldung = match self.ziel.schreibe(self.protokoll_bauen()) {
                    Ok(text) | Err(text) => text,
                };
            }
        });
        if !self.meldung.is_empty() {
            ui.colored_label(Color32::from_rgb(180, 40, 40), &self.meldung);
        }
        ui.add_space(4.0);
        self.zeichne_punkte(ui);
    }

    fn zeichne_punkte(&mut self, ui: &mut egui::Ui) {
        let raster = Raster::neu(&BREITEN_PUNKTE).mit_zeilenkopf(KOPF_PUNKTE);
        let mut ziel_zeile = None;
        let mut entfernen = None;
        egui::ScrollArea::both().id_salt("achse_punkte").max_height(TABELLEN_HOEHE).show(ui, |ui| {
            raster.kopf(ui, &["Nr.", "Station", "R [m]", "H [m]", "Abs. [m]", "Ord. [m]", "Hz [gon]", "s [m]", "Status", "", ""]);
            for (i, p) in self.punkte.iter().enumerate() {
                let mit_station = self.station.is_some();
                raster.zeile_farbig(ui, i, &(i + 1).to_string(), zeilenfarbe(p.ist_abgesteckt(), self.aktuell == Some(i)), |z| {
                    z.text(&p.punkt_nr);
                    z.text(&p.label);
                    z.zahl(&f(p.r_soll, 3));
                    z.zahl(&f(p.h_soll, 3));
                    z.zahl(&p.abszisse_m.map_or_else(String::new, |v| f(v, 3)));
                    z.zahl(&p.ordinate_m.map_or_else(String::new, |v| f(v, 3)));
                    z.zahl(&if mit_station { f(p.hz_soll_gon, 4) } else { "-".to_string() });
                    z.zahl(&if mit_station { f(p.s_soll_m, 3) } else { "-".to_string() });
                    z.text(&p.status);
                    if z.knopf("▶") {
                        ziel_zeile = Some(i);
                    }
                    if z.knopf("✕") {
                        entfernen = Some(i);
                    }
                });
            }
        });
        if let Some(i) = entfernen {
            self.entferne(i);
        } else if let Some(i) = ziel_zeile {
            self.waehle(i);
        }
    }

    fn zeichne_karte(&mut self, ui: &mut egui::Ui) {
        let plan = PlanDaten {
            station: self.station.as_ref(),
            punkte: &self.punkte,
            aktuell: self.aktuell,
            achse: self.achse(),
            hintergrund: &self.hintergrund,
            ..Default::default()
        };
        let klick = zeichne_lageplan(ui, &plan, Vec2::new(ui.available_width(), PLAN_HOEHE));
        if let Some(k) = klick {
            if let Some(i) = k.punkt {
                self.waehle(i);
            } else if let Some(i) = k.hintergrund {
                self.fange(i);
            }
        }
        ui.add_space(6.0);
        if let Some(nr) = self.einweiser.zeichne(ui) {
            self.markiere(&nr);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ansicht() -> AchseAnsicht {
        AchseAnsicht {
            station: Some(StandpunktInfo { punkt_nr: "S".into(), r: 50.0, h: -10.0, orientierung_gon: 0.0 }),
            achse_punkte: [
                Eckpunkt { nr: "A".into(), r: "0".into(), h: "0".into() },
                Eckpunkt { nr: "E".into(), r: "0".into(), h: "30".into() },
            ],
            intervall: "10.0".into(),
            offsets: "0".into(),
            punkte: Vec::new(),
            aktuell: None,
            anschluss: Vec::new(),
            hintergrund: vec![("X1".into(), 1.0, 2.0), ("X2".into(), 3.5, 4.25), ("X3".into(), 9.0, 9.0)],
            einweiser: Einweiser::default(),
            meldung: String::new(),
            ziel: ProtokollZiel::aus_werten(std::path::PathBuf::new(), false, "Tester"),
        }
    }

    #[test]
    fn versaetze_werden_tolerant_gelesen() {
        assert_eq!(lese_versaetze("0, 1.5; -2 x 3.5"), vec![0.0, 1.5, -2.0, 3.5]);
        // Das Komma trennt (wie im Original), ein Dezimalkomma ist hier nicht möglich
        assert_eq!(lese_versaetze("3,5"), vec![3.0, 5.0]);
        assert!(lese_versaetze("").is_empty());
        assert!(lese_versaetze("abc").is_empty());
    }

    #[test]
    fn fangen_fuellt_anfang_dann_ende_dann_ersetzt_anfang() {
        let mut a = ansicht();
        a.achse_punkte = Default::default();
        a.fange(0);
        a.fange(1);
        assert_eq!((a.achse_punkte[0].nr.as_str(), a.achse_punkte[0].r.as_str()), ("X1", "1.000"));
        assert_eq!((a.achse_punkte[1].nr.as_str(), a.achse_punkte[1].h.as_str()), ("X2", "4.250"));
        assert_eq!(a.achse(), Some(((1.0, 2.0), (3.5, 4.25))));
        a.fange(2);
        assert_eq!(a.achse_punkte[0].nr, "X3");
        assert_eq!(a.achse_punkte[1].nr, "X2");
        a.fange(7);
        assert_eq!(a.achse_punkte[0].nr, "X3");
    }

    #[test]
    fn achse_braucht_beide_punkte() {
        let mut a = ansicht();
        assert_eq!(a.achse(), Some(((0.0, 0.0), (0.0, 30.0))));
        a.achse_punkte[1].h.clear();
        assert!(a.achse().is_none());
    }

    #[test]
    fn markieren_und_entfernen() {
        let mut a = ansicht();
        a.punkte = berechne_achspunkte((0.0, 0.0), (0.0, 30.0), 10.0, &[0.0], a.station.as_ref());
        assert_eq!(a.punkte.len(), 4);
        a.waehle(0);
        a.markiere("A001");
        assert_eq!(a.aktuell, Some(1));
        a.markiere("A004");
        assert_eq!(a.aktuell, Some(1), "springt zum ersten offenen");
        a.entferne(1);
        assert_eq!(a.punkte.len(), 3);
        assert_eq!(a.aktuell, None);
        assert!(a.protokoll_bauen().is_some());
        a.punkte.clear();
        assert!(a.protokoll_bauen().is_none());
    }
}
