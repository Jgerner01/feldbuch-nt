// Zweck:    Formular Rasterabsteckung: Raster aus Ursprung, Richtung und Abständen mit polaren Absteckdaten
// Hardware: keine
// Autor:    Johann Gerner

use eframe::egui::{self, Color32, RichText, Vec2};

use super::gemeinsam::{PLAN_HOEHE, PlanDaten, ProtokollZiel, lese_zahl, zeichne_lageplan, standpunkt_fuer_absteckung, zeige_standpunkt};
use crate::daten::projekt::Projekt;
use crate::grundlagen::format::f;
use crate::gui::tabelle::Raster;
use crate::protokolle::absteckung::{AbsteckProtokoll, RasterEingabe, rasterabsteckung};
use crate::rechnen::absteckung::{AbsteckPunkt, StandpunktInfo, berechne_raster};

/// Spaltenbreiten: Punkt, R, H, Hz, s, Status.
const BREITEN: [f32; 6] = [70.0, 110.0, 110.0, 90.0, 80.0, 90.0];
/// Breite der Zeilenköpfe.
const ZEILENKOPF_BREITE: f32 = 30.0;
/// Höhe des Tabellenbereichs.
const TABELLEN_HOEHE: f32 = 340.0;
/// Höchstzahl der Rasterzeilen (Buchstaben A–Z).
const MAX_RASTERZEILEN: usize = 26;

/// Formular der Rasterabsteckung.
pub struct RasterAnsicht {
    station: Option<StandpunktInfo>,
    /// Standpunkt aus dem Rückwärtsschnitt statt aus der aktiven Aufstellung.
    rueckwaerts: bool,
    r0: String,
    h0: String,
    richtung: String,
    d_s: String,
    d_q: String,
    zeilen: String,
    spalten: String,
    punkte: Vec<AbsteckPunkt>,
    meldung: String,
    ziel: ProtokollZiel,
}

impl Drop for RasterAnsicht {
    fn drop(&mut self) {
        let p = self.protokoll_bauen();
        self.ziel.schreibe_beim_verlassen(p, "Rasterabsteckung.protokoll");
    }
}

impl RasterAnsicht {
    pub fn neu(projekt: &Projekt) -> Self {
        Self {
            station: standpunkt_fuer_absteckung(projekt, false),
            rueckwaerts: false,
            r0: String::new(),
            h0: String::new(),
            richtung: "0.0000".to_string(),
            d_s: "5.000".to_string(),
            d_q: "5.000".to_string(),
            zeilen: "4".to_string(),
            spalten: "4".to_string(),
            punkte: Vec::new(),
            meldung: String::new(),
            ziel: ProtokollZiel::neu(projekt),
        }
    }

    fn protokoll_bauen(&self) -> Option<AbsteckProtokoll> {
        (!self.punkte.is_empty()).then(|| {
            let e = RasterEingabe {
                r0: &self.r0,
                h0: &self.h0,
                richtung: &self.richtung,
                d_s: &self.d_s,
                d_q: &self.d_q,
                zeilen: &self.zeilen,
                spalten: &self.spalten,
            };
            rasterabsteckung(self.station.as_ref(), &self.punkte, &e, &self.ziel.bearbeiter, ProtokollZiel::jetzt())
        })
    }

    /// Prüft die Eingaben und berechnet das Raster.
    fn berechne(&mut self, projekt: &Projekt) {
        self.station = standpunkt_fuer_absteckung(projekt, self.rueckwaerts);
        self.ziel.aktualisiere(projekt);
        match self.rechne() {
            Ok(punkte) => {
                self.punkte = punkte;
                self.meldung.clear();
            }
            Err(text) => self.meldung = text,
        }
    }

    fn rechne(&self) -> Result<Vec<AbsteckPunkt>, String> {
        let eingabe = || -> Result<_, String> {
            Ok((
                lese_zahl(&self.r0, "R0")?,
                lese_zahl(&self.h0, "H0")?,
                lese_zahl(&self.richtung, "Richtung")?,
                lese_zahl(&self.d_s, "ΔS")?,
                lese_zahl(&self.d_q, "ΔQ")?,
            ))
        };
        let ungueltig = || "Alle Rasterparameter korrekt eingeben.".to_string();
        let (r0, h0, richtung, d_s, d_q) = eingabe().map_err(|_| ungueltig())?;
        let zeilen: usize = self.zeilen.trim().parse().map_err(|_| ungueltig())?;
        let spalten: usize = self.spalten.trim().parse().map_err(|_| ungueltig())?;
        if zeilen < 1 || spalten < 1 || d_s <= 0.0 || d_q <= 0.0 {
            return Err(ungueltig());
        }
        if zeilen > MAX_RASTERZEILEN {
            return Err("Maximal 26 Zeilen (A–Z).".to_string());
        }
        Ok(berechne_raster(r0, h0, richtung, d_s, d_q, zeilen, spalten, self.station.as_ref()))
    }

    pub fn zeichne(&mut self, ui: &mut egui::Ui, projekt: &Projekt) {
        ui.heading("Rasterabsteckung");
        if zeige_standpunkt(ui, projekt, self.station.as_ref(), &mut self.rueckwaerts) {
            self.station = standpunkt_fuer_absteckung(projekt, self.rueckwaerts);
        }
        ui.add_space(4.0);
        egui::Grid::new("raster_eingabe").num_columns(6).spacing([8.0, 4.0]).show(ui, |ui| {
            for (name, text) in [("R0 [m]:", &mut self.r0), ("H0 [m]:", &mut self.h0), ("Richtung [gon]:", &mut self.richtung)] {
                ui.label(name);
                ui.add(egui::TextEdit::singleline(text).desired_width(90.0));
            }
            ui.end_row();
            for (name, text) in [("ΔS [m]:", &mut self.d_s), ("ΔQ [m]:", &mut self.d_q)] {
                ui.label(name);
                ui.add(egui::TextEdit::singleline(text).desired_width(90.0));
            }
            ui.end_row();
            for (name, text) in [("Zeilen:", &mut self.zeilen), ("Spalten:", &mut self.spalten)] {
                ui.label(name);
                ui.add(egui::TextEdit::singleline(text).desired_width(90.0));
            }
            ui.end_row();
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
        ui.columns(2, |spalten| {
            let raster = Raster::neu(&BREITEN).mit_zeilenkopf(ZEILENKOPF_BREITE);
            egui::ScrollArea::both().id_salt("raster_punkte").max_height(TABELLEN_HOEHE).show(&mut spalten[0], |ui| {
                raster.kopf(ui, &["Punkt", "R [m]", "H [m]", "Hz [gon]", "s [m]", "Status"]);
                for (i, p) in self.punkte.iter().enumerate() {
                    raster.zeile(ui, i, &(i + 1).to_string(), |z| {
                        z.text(&p.punkt_nr);
                        z.zahl(&f(p.r_soll, 3));
                        z.zahl(&f(p.h_soll, 3));
                        z.zahl(&if self.station.is_some() { f(p.hz_soll_gon, 4) } else { "-".to_string() });
                        z.zahl(&if self.station.is_some() { f(p.s_soll_m, 3) } else { "-".to_string() });
                        z.text(&p.status);
                    });
                }
            });
            let plan = PlanDaten { station: self.station.as_ref(), punkte: &self.punkte, ..Default::default() };
            let breite = spalten[1].available_width();
            zeichne_lageplan(&mut spalten[1], &plan, Vec2::new(breite, PLAN_HOEHE));
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ansicht() -> RasterAnsicht {
        RasterAnsicht {
            station: None,
            rueckwaerts: false,
            r0: "100".into(),
            h0: "200,5".into(),
            richtung: "0".into(),
            d_s: "5".into(),
            d_q: "2.5".into(),
            zeilen: "3".into(),
            spalten: " 2 ".into(),
            punkte: Vec::new(),
            meldung: String::new(),
            ziel: ProtokollZiel::aus_werten(std::path::PathBuf::new(), false, "Tester"),
        }
    }

    #[test]
    fn rechnet_raster_aus_den_eingaben() {
        let p = ansicht().rechne().unwrap();
        assert_eq!(p.len(), 6);
        assert_eq!(p[0].punkt_nr, "A1");
        assert_eq!((p[0].r_soll, p[0].h_soll), (100.0, 200.5));
        assert_eq!(p[5].punkt_nr, "C2");
        assert!((p[5].h_soll - 210.5).abs() < 1e-9 && (p[5].r_soll - 102.5).abs() < 1e-9);
    }

    #[test]
    fn prueft_eingaben_wie_das_original() {
        let fehler = "Alle Rasterparameter korrekt eingeben.";
        for aendern in [
            (|a: &mut RasterAnsicht| a.r0 = String::new()) as fn(&mut RasterAnsicht),
            |a| a.d_s = "0".into(),
            |a| a.d_q = "-1".into(),
            |a| a.zeilen = "0".into(),
            |a| a.spalten = "x".into(),
            |a| a.zeilen = "2.5".into(),
        ] {
            let mut a = ansicht();
            aendern(&mut a);
            assert_eq!(a.rechne().unwrap_err(), fehler);
        }
        let mut a = ansicht();
        a.zeilen = "27".into();
        assert_eq!(a.rechne().unwrap_err(), "Maximal 26 Zeilen (A–Z).");
        a.zeilen = "26".into();
        assert!(a.rechne().is_ok());
    }

    #[test]
    fn protokoll_nur_mit_punkten() {
        let mut a = ansicht();
        assert!(a.protokoll_bauen().is_none());
        a.punkte = a.rechne().unwrap();
        assert!(a.protokoll_bauen().is_some());
    }
}
