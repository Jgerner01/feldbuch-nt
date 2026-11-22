// Zweck:    Formular Profilabsteckung: Stationen entlang einer Achse mit Plan- und Geländehöhen, Auftrag/Aushub, Böschung
// Hardware: keine
// Autor:    Johann Gerner

use eframe::egui::{self, Color32, RichText, Vec2};

use super::gemeinsam::{PLAN_HOEHE, PlanDaten, ProtokollZiel, lese_zahl, zeichne_lageplan, standpunkt_fuer_absteckung, zeige_standpunkt};
use crate::daten::projekt::Projekt;
use crate::grundlagen::format::{f, parse_zahl};
use crate::gui::tabelle::Raster;
use crate::protokolle::absteckung::{AbsteckProtokoll, ProfilEingabe, profilabsteckung};
use crate::protokolle::signiert;
use crate::rechnen::absteckung::{
    AbsteckPunkt, ProfilAbsteckPunkt, StandpunktInfo, aktualisiere_profilpunkt, berechne_profilpunkte,
};

/// Spaltenbreiten: Station, R, H, H_Plan, H_Gelände, ΔH, Böschung, Hz, s.
const BREITEN: [f32; 9] = [70.0, 90.0, 90.0, 90.0, 90.0, 80.0, 80.0, 90.0, 70.0];
/// Breite der Zeilenköpfe.
const ZEILENKOPF_BREITE: f32 = 30.0;
/// Höhe des Tabellenbereichs.
const TABELLEN_HOEHE: f32 = 340.0;
/// Textfarbe von ΔH bei Auftrag und bei Aushub.
const FARBE_AUFTRAG: Color32 = Color32::from_rgb(20, 90, 20);
const FARBE_AUSHUB: Color32 = Color32::from_rgb(150, 30, 20);
/// Standard von Planumhalbbreite [m] und Böschungsneigung (1:n).
const HALBBREITE_STANDARD: f64 = 3.0;
const BOESCHUNG_STANDARD: f64 = 1.5;

/// Eingabetexte einer Profilzeile (Planhöhe und Geländehöhe sind editierbar).
#[derive(Clone, Default)]
struct Hoehen {
    plan: String,
    gelaende: String,
}

/// Formular der Profilabsteckung.
pub struct ProfilAnsicht {
    station: Option<StandpunktInfo>,
    /// Standpunkt aus dem Rückwärtsschnitt statt aus der aktiven Aufstellung.
    rueckwaerts: bool,
    r_a: String,
    h_a: String,
    r_e: String,
    h_e: String,
    intervall: String,
    halbbreite: String,
    boeschung: String,
    profile: Vec<ProfilAbsteckPunkt>,
    hoehen: Vec<Hoehen>,
    /// Werte, mit denen zuletzt berechnet wurde.
    achse: Option<((f64, f64), (f64, f64))>,
    planum_halbbreite: f64,
    boeschung_neigung: f64,
    meldung: String,
    ziel: ProtokollZiel,
}

impl Drop for ProfilAnsicht {
    fn drop(&mut self) {
        let p = self.protokoll_bauen();
        self.ziel.schreibe_beim_verlassen(p, "Profilabsteckung.protokoll");
    }
}

impl ProfilAnsicht {
    pub fn neu(projekt: &Projekt) -> Self {
        Self {
            station: standpunkt_fuer_absteckung(projekt, false),
            rueckwaerts: false,
            r_a: String::new(),
            h_a: String::new(),
            r_e: String::new(),
            h_e: String::new(),
            intervall: "25.0".to_string(),
            halbbreite: "3.000".to_string(),
            boeschung: "1.5".to_string(),
            profile: Vec::new(),
            hoehen: Vec::new(),
            achse: None,
            planum_halbbreite: HALBBREITE_STANDARD,
            boeschung_neigung: BOESCHUNG_STANDARD,
            meldung: String::new(),
            ziel: ProtokollZiel::neu(projekt),
        }
    }

    fn protokoll_bauen(&self) -> Option<AbsteckProtokoll> {
        (!self.profile.is_empty()).then(|| {
            let e = ProfilEingabe {
                r_a: &self.r_a,
                h_a: &self.h_a,
                r_e: &self.r_e,
                h_e: &self.h_e,
                intervall: &self.intervall,
                halbbreite: &self.halbbreite,
                boeschung: &self.boeschung,
            };
            profilabsteckung(self.station.as_ref(), &self.profile, &e, &self.ziel.bearbeiter, ProtokollZiel::jetzt())
        })
    }

    /// Erzeugt die Profilpunkte; vorhandene Planhöhen bleiben erhalten.
    fn generiere(&mut self, projekt: &Projekt) {
        self.station = standpunkt_fuer_absteckung(projekt, self.rueckwaerts);
        self.ziel.aktualisiere(projekt);
        match self.rechne() {
            Ok(()) => self.meldung.clear(),
            Err(text) => self.meldung = text,
        }
    }

    fn rechne(&mut self) -> Result<(), String> {
        let ungueltig = || "Achse A/E, Intervall, Planumhalbbreite und Böschungsneigung eingeben.".to_string();
        let a = (lese_zahl(&self.r_a, "R_A").map_err(|_| ungueltig())?, lese_zahl(&self.h_a, "H_A").map_err(|_| ungueltig())?);
        let e = (lese_zahl(&self.r_e, "R_E").map_err(|_| ungueltig())?, lese_zahl(&self.h_e, "H_E").map_err(|_| ungueltig())?);
        let intervall = lese_zahl(&self.intervall, "Intervall").map_err(|_| ungueltig())?;
        if intervall < 0.01 {
            return Err(ungueltig());
        }
        let halbbreite = lese_zahl(&self.halbbreite, "Planumhalbbreite").map_err(|_| ungueltig())?;
        let neigung = lese_zahl(&self.boeschung, "Böschungsneigung").map_err(|_| ungueltig())?;
        // Planhöhen der vorhandenen Zeilen übernehmen (nur gültige, in Reihenfolge)
        let plan: Vec<f64> = self.hoehen.iter().filter_map(|h| parse_zahl(&h.plan)).collect();
        let gelaende: Vec<Option<f64>> = self.hoehen.iter().map(|h| parse_zahl(&h.gelaende)).collect();
        self.planum_halbbreite = halbbreite;
        self.boeschung_neigung = neigung;
        self.achse = Some((a, e));
        self.profile = berechne_profilpunkte(a, e, intervall, &plan, self.station.as_ref());
        self.hoehen = self
            .profile
            .iter()
            .enumerate()
            .map(|(i, p)| Hoehen {
                plan: if i < plan.len() { f(p.h_plan, 3) } else { "0.000".to_string() },
                gelaende: gelaende.get(i).copied().flatten().map_or_else(String::new, |h| f(h, 3)),
            })
            .collect();
        for i in 0..self.profile.len() {
            self.uebernimm_hoehen(i);
        }
        self.ziel.geschrieben = false;
        Ok(())
    }

    /// Überträgt Plan- und Geländehöhe einer Zeile in den Profilpunkt und berechnet ΔH und Böschung neu.
    fn uebernimm_hoehen(&mut self, i: usize) {
        let (Some(p), Some(h)) = (self.profile.get_mut(i), self.hoehen.get(i)) else {
            return;
        };
        if let Some(plan) = parse_zahl(&h.plan) {
            p.h_plan = plan;
        }
        match parse_zahl(&h.gelaende) {
            Some(gel) => aktualisiere_profilpunkt(p, gel, self.planum_halbbreite, self.boeschung_neigung),
            None => {
                p.h_gelaende = None;
                p.delta_h_m = None;
                p.boesch_links_m = None;
                p.boesch_rechts_m = None;
            }
        }
        self.ziel.geschrieben = false;
    }

    pub fn zeichne(&mut self, ui: &mut egui::Ui, projekt: &Projekt) {
        ui.heading("Profilabsteckung");
        if zeige_standpunkt(ui, projekt, self.station.as_ref(), &mut self.rueckwaerts) {
            self.station = standpunkt_fuer_absteckung(projekt, self.rueckwaerts);
        }
        ui.add_space(4.0);
        egui::Grid::new("profil_eingabe").num_columns(8).spacing([8.0, 4.0]).show(ui, |ui| {
            for (name, text) in [("R_A [m]:", &mut self.r_a), ("H_A [m]:", &mut self.h_a), ("R_E [m]:", &mut self.r_e), ("H_E [m]:", &mut self.h_e)] {
                ui.label(name);
                ui.add(egui::TextEdit::singleline(text).desired_width(90.0));
            }
            ui.end_row();
            for (name, text) in
                [("Intervall [m]:", &mut self.intervall), ("Planum-Halbbreite [m]:", &mut self.halbbreite), ("Böschung 1:", &mut self.boeschung)]
            {
                ui.label(name);
                ui.add(egui::TextEdit::singleline(text).desired_width(90.0));
            }
            ui.end_row();
        });
        ui.horizontal(|ui| {
            if ui.button(RichText::new("Generieren").strong()).clicked() {
                self.generiere(projekt);
            }
            if ui.add_enabled(!self.profile.is_empty(), egui::Button::new("Protokoll")).clicked() {
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
            self.zeichne_tabelle(&mut spalten[0]);
            let punkte: Vec<AbsteckPunkt> = self
                .profile
                .iter()
                .map(|p| {
                    let mut q = AbsteckPunkt::neu(&f(p.station_m, 2), p.r, p.h);
                    q.hz_soll_gon = p.hz_soll_gon;
                    q.s_soll_m = p.s_soll_m;
                    q
                })
                .collect();
            let plan = PlanDaten { station: self.station.as_ref(), punkte: &punkte, achse: self.achse, ..Default::default() };
            let breite = spalten[1].available_width();
            zeichne_lageplan(&mut spalten[1], &plan, Vec2::new(breite, PLAN_HOEHE));
        });
    }

    fn zeichne_tabelle(&mut self, ui: &mut egui::Ui) {
        let raster = Raster::neu(&BREITEN).mit_zeilenkopf(ZEILENKOPF_BREITE);
        let mut geaendert = Vec::new();
        egui::ScrollArea::both().id_salt("profil_tabelle").max_height(TABELLEN_HOEHE).show(ui, |ui| {
            raster.kopf(ui, &["Station", "R [m]", "H [m]", "H_Plan [m]", "H_Gel. [m]", "ΔH [m]", "Bösch. [m]", "Hz [gon]", "s [m]"]);
            for (i, p) in self.profile.iter().enumerate() {
                let h = &mut self.hoehen[i];
                let mit_station = self.station.is_some();
                raster.zeile(ui, i, &(i + 1).to_string(), |z| {
                    z.zahl(&f(p.station_m, 2));
                    z.zahl(&f(p.r, 3));
                    z.zahl(&f(p.h, 3));
                    if z.eingabe(&mut h.plan) | z.eingabe(&mut h.gelaende) {
                        geaendert.push(i);
                    }
                    match p.delta_h_m {
                        Some(d) => z.zahl_farbig(&signiert(d, 3), if d > 0.0 { FARBE_AUFTRAG } else { FARBE_AUSHUB }),
                        None => z.zahl(""),
                    }
                    z.zahl(&p.boesch_links_m.map_or_else(String::new, |b| f(b, 2)));
                    z.zahl(&if mit_station { f(p.hz_soll_gon, 4) } else { "-".to_string() });
                    z.zahl(&if mit_station { f(p.s_soll_m, 3) } else { "-".to_string() });
                });
            }
        });
        for i in geaendert {
            self.uebernimm_hoehen(i);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ansicht() -> ProfilAnsicht {
        ProfilAnsicht {
            station: None,
            rueckwaerts: false,
            r_a: "0".into(),
            h_a: "0".into(),
            r_e: "0".into(),
            h_e: "50".into(),
            intervall: "25".into(),
            halbbreite: "3".into(),
            boeschung: "1,5".into(),
            profile: Vec::new(),
            hoehen: Vec::new(),
            achse: None,
            planum_halbbreite: 3.0,
            boeschung_neigung: 1.5,
            meldung: String::new(),
            ziel: ProtokollZiel::aus_werten(std::path::PathBuf::new(), false, "Tester"),
        }
    }

    #[test]
    fn generiert_stationen_mit_standardhoehen() {
        let mut a = ansicht();
        a.rechne().unwrap();
        assert_eq!(a.profile.len(), 3);
        assert_eq!(a.hoehen[1].plan, "0.000");
        assert_eq!(a.profile[2].station_m, 50.0);
        assert_eq!(a.boeschung_neigung, 1.5);
    }

    #[test]
    fn gelaendehoehe_berechnet_auftrag_und_boeschung() {
        let mut a = ansicht();
        a.rechne().unwrap();
        a.hoehen[0].plan = "100".into();
        a.hoehen[0].gelaende = "99,5".into();
        a.uebernimm_hoehen(0);
        assert_eq!(a.profile[0].delta_h_m, Some(0.5));
        assert_eq!(a.profile[0].boesch_links_m, Some(3.75));
        // Planhöhe geändert → ΔH folgt (im Original blieb sie veraltet)
        a.hoehen[0].plan = "98".into();
        a.uebernimm_hoehen(0);
        assert_eq!(a.profile[0].delta_h_m, Some(-1.5));
        // Gelände gelöscht → Werte zurückgesetzt
        a.hoehen[0].gelaende.clear();
        a.uebernimm_hoehen(0);
        assert!(a.profile[0].delta_h_m.is_none() && a.profile[0].boesch_links_m.is_none());
    }

    #[test]
    fn neu_generieren_behaelt_hoehen() {
        let mut a = ansicht();
        a.rechne().unwrap();
        a.hoehen[0].plan = "100".into();
        a.hoehen[1].plan = "101".into();
        a.hoehen[1].gelaende = "100.4".into();
        a.rechne().unwrap();
        assert_eq!(a.profile[1].h_plan, 101.0);
        assert!((a.profile[1].delta_h_m.unwrap() - 0.6).abs() < 1e-9);
        assert_eq!(a.hoehen[1].gelaende, "100.400");
        // Intervall halbiert → mehr Stationen, bisherige Werte rücken nach
        a.intervall = "10".into();
        a.rechne().unwrap();
        assert_eq!(a.profile.len(), 6);
    }

    #[test]
    fn prueft_eingaben() {
        let fehler = "Achse A/E, Intervall, Planumhalbbreite und Böschungsneigung eingeben.";
        for aendern in [
            (|a: &mut ProfilAnsicht| a.r_a.clear()) as fn(&mut ProfilAnsicht),
            |a| a.intervall = "0.001".into(),
            |a| a.halbbreite = "x".into(),
            |a| a.boeschung.clear(),
        ] {
            let mut a = ansicht();
            aendern(&mut a);
            assert_eq!(a.rechne().unwrap_err(), fehler);
        }
    }

    #[test]
    fn protokoll_nur_mit_profilen() {
        let mut a = ansicht();
        assert!(a.protokoll_bauen().is_none());
        a.rechne().unwrap();
        assert!(a.protokoll_bauen().is_some());
    }
}
