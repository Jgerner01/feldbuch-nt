// Zweck:    Formular Punktabsteckung: Sollpunkte eingeben oder laden, polare Absteckdaten, Lageplan und Einweiser
// Hardware: keine
// Autor:    Johann Gerner

use eframe::egui::{self, Color32, RichText, Vec2};

use super::gemeinsam::{
    Einweiser, MAX_ZEILEN, PLAN_HOEHE, PlanDaten, ProtokollZiel, hintergrundpunkte, lade_anschlusspunkte, zeichne_lageplan, zeige_standpunkt,
    zeilenfarbe,
};
use crate::daten::projekt::Projekt;
use crate::grundlagen::format::{f, parse_zahl};
use crate::protokolle::absteckung::{AbsteckProtokoll, punktabsteckung};
use crate::gui::tabelle::Raster;
use crate::rechnen::absteckung::{
    AbsteckPunkt, STATUS_ABGESTECKT, StandpunktInfo, berechne_polare_absteckung, lade_standpunkt,
};

/// Spaltenbreiten: Nr, R, H, Hz, s, Status, Ziel, Löschen.
const BREITEN: [f32; 8] = [80.0, 100.0, 100.0, 90.0, 80.0, 90.0, 30.0, 26.0];
/// Breite der Zeilenköpfe.
const ZEILENKOPF_BREITE: f32 = 30.0;
/// Höhe des Tabellenbereichs.
const TABELLEN_HOEHE: f32 = 360.0;

#[derive(Clone, Default)]
struct Zeile {
    nr: String,
    r: String,
    h: String,
}

/// Formular der Punktabsteckung.
pub struct PunktAnsicht {
    station: Option<StandpunktInfo>,
    zeilen: Vec<Zeile>,
    punkte: Vec<AbsteckPunkt>,
    /// Zu jeder Tabellenzeile der Index des berechneten Punkts.
    zeile_zu_punkt: Vec<Option<usize>>,
    aktuell: Option<usize>,
    /// Bekannte Projektpunkte, im Plan anklickbar.
    hintergrund: Vec<(String, f64, f64)>,
    einweiser: Einweiser,
    meldung: String,
    ziel: ProtokollZiel,
}

impl Drop for PunktAnsicht {
    fn drop(&mut self) {
        let p = self.protokoll_bauen();
        self.ziel.schreibe_beim_verlassen(p, "Punktabsteckung.protokoll");
    }
}

impl PunktAnsicht {
    pub fn neu(projekt: &Projekt) -> Self {
        Self {
            station: lade_standpunkt(&projekt.projektdaten),
            zeilen: vec![Zeile::default(); MAX_ZEILEN],
            punkte: Vec::new(),
            zeile_zu_punkt: vec![None; MAX_ZEILEN],
            aktuell: None,
            hintergrund: hintergrundpunkte(projekt),
            einweiser: Einweiser::default(),
            meldung: String::new(),
            ziel: ProtokollZiel::neu(projekt),
        }
    }

    /// Protokoll des aktuellen Stands (nur mit mindestens einem Punkt).
    fn protokoll_bauen(&self) -> Option<AbsteckProtokoll> {
        (!self.punkte.is_empty()).then(|| punktabsteckung(self.station.as_ref(), &self.punkte, &self.ziel.bearbeiter, ProtokollZiel::jetzt()))
    }

    /// Wählt einen Punkt als Absteckziel.
    fn waehle(&mut self, index: usize) {
        let Some(p) = self.punkte.get(index) else {
            return;
        };
        self.aktuell = Some(index);
        let soll = self.station.as_ref().map(|_| (p.hz_soll_gon, p.s_soll_m));
        self.einweiser.setze_ziel(Some(&p.punkt_nr), soll);
    }

    /// Berechnet alle Absteckdaten neu; bereits abgesteckte Punkte behalten ihren Status.
    fn berechne_alle(&mut self) {
        let alt = std::mem::take(&mut self.punkte);
        let alte_nr = self.aktuell.and_then(|i| alt.get(i)).map(|p| p.punkt_nr.clone());
        self.zeile_zu_punkt = vec![None; self.zeilen.len()];
        for (i, zeile) in self.zeilen.iter().enumerate() {
            if zeile.nr.trim().is_empty() {
                continue;
            }
            let (Some(r), Some(h)) = (parse_zahl(&zeile.r), parse_zahl(&zeile.h)) else {
                continue;
            };
            let mut p = AbsteckPunkt::neu(zeile.nr.trim(), r, h);
            if let Some(st) = &self.station {
                berechne_polare_absteckung(st, &mut p);
            }
            if let Some(vorher) = alt.iter().find(|a| a.punkt_nr == p.punkt_nr && a.r_soll == p.r_soll && a.h_soll == p.h_soll) {
                p.status = vorher.status.clone();
            }
            self.zeile_zu_punkt[i] = Some(self.punkte.len());
            self.punkte.push(p);
        }
        self.ziel.geschrieben = false;
        let behalten = alte_nr.and_then(|nr| self.punkte.iter().position(|p| p.punkt_nr == nr));
        match (behalten, self.punkte.is_empty()) {
            (Some(i), _) => self.waehle(i),
            (None, false) => self.waehle(0),
            (None, true) => {
                self.aktuell = None;
                self.einweiser.zuruecksetzen();
            }
        }
    }

    fn lade_punkte(&mut self, projekt: &Projekt) {
        match lade_anschlusspunkte(projekt) {
            Ok(liste) => {
                self.zeilen = vec![Zeile::default(); MAX_ZEILEN];
                for (zeile, (nr, r, h)) in self.zeilen.iter_mut().zip(liste) {
                    *zeile = Zeile { nr, r, h };
                }
                self.meldung.clear();
                self.berechne_alle();
            }
            Err(fehler) => self.meldung = fehler,
        }
    }

    /// Ein Punkt wurde als abgesteckt markiert; weiter zum nächsten offenen.
    fn markiere(&mut self, punkt_nr: &str) {
        let Some(index) = self.punkte.iter().position(|p| p.punkt_nr == punkt_nr) else {
            return;
        };
        self.punkte[index].status = STATUS_ABGESTECKT.to_string();
        self.ziel.geschrieben = false;
        let n = self.punkte.len();
        let naechster = (1..=n).map(|d| (index + d) % n).find(|i| !self.punkte[*i].ist_abgesteckt());
        match naechster {
            Some(i) => self.waehle(i),
            None => self.waehle(index),
        }
    }

    /// Übernimmt einen angeklickten Projektpunkt in die erste freie Zeile.
    fn fange(&mut self, index: usize) {
        let Some((nr, r, h)) = self.hintergrund.get(index).cloned() else {
            return;
        };
        let Some(zeile) = self.zeilen.iter_mut().find(|z| z.nr.trim().is_empty() && z.r.trim().is_empty() && z.h.trim().is_empty()) else {
            self.meldung = "Keine freie Zeile mehr.".to_string();
            return;
        };
        *zeile = Zeile { nr, r: f(r, 3), h: f(h, 3) };
        self.meldung.clear();
        self.berechne_alle();
    }

    fn loesche_zeile(&mut self, index: usize) {
        self.zeilen.remove(index);
        self.zeilen.push(Zeile::default());
        self.aktuell = None;
        self.berechne_alle();
    }

    pub fn zeichne(&mut self, ui: &mut egui::Ui, projekt: &Projekt) {
        ui.heading("Punktabsteckung");
        zeige_standpunkt(ui, self.station.as_ref());
        ui.add_space(4.0);
        ui.columns(2, |spalten| {
            self.zeichne_tabelle(&mut spalten[0], projekt);
            self.zeichne_karte(&mut spalten[1]);
        });
    }

    fn zeichne_tabelle(&mut self, ui: &mut egui::Ui, projekt: &Projekt) {
        let raster = Raster::neu(&BREITEN).mit_zeilenkopf(ZEILENKOPF_BREITE);
        raster.kopf(ui, &["PunktNr", "R_soll [m]", "H_soll [m]", "Hz [gon]", "s [m]", "Status", "", ""]);
        let mut geaendert = false;
        let mut ziel_zeile = None;
        let mut loeschen = None;
        egui::ScrollArea::vertical().id_salt("punkt_tabelle").max_height(TABELLEN_HOEHE).show(ui, |ui| {
            for (i, zeile) in self.zeilen.iter_mut().enumerate() {
                let punkt = self.zeile_zu_punkt[i].and_then(|p| self.punkte.get(p));
                let erledigt = punkt.is_some_and(AbsteckPunkt::ist_abgesteckt);
                let aktuell = punkt.is_some() && self.zeile_zu_punkt[i] == self.aktuell;
                raster.zeile_farbig(ui, i, &(i + 1).to_string(), zeilenfarbe(erledigt, aktuell), |z| {
                    geaendert |= z.eingabe(&mut zeile.nr);
                    geaendert |= z.eingabe(&mut zeile.r);
                    geaendert |= z.eingabe(&mut zeile.h);
                    match punkt {
                        Some(p) if self.station.is_some() => {
                            z.zahl(&f(p.hz_soll_gon, 4));
                            z.zahl(&f(p.s_soll_m, 3));
                        }
                        Some(_) => {
                            z.zahl("-");
                            z.zahl("-");
                        }
                        None => {
                            z.zahl("");
                            z.zahl("");
                        }
                    }
                    z.text(punkt.map_or("", |p| p.status.as_str()));
                    if z.knopf("▶") && punkt.is_some() {
                        ziel_zeile = self.zeile_zu_punkt[i];
                    }
                    if z.knopf("✕") {
                        loeschen = Some(i);
                    }
                });
            }
        });
        if let Some(i) = loeschen {
            self.loesche_zeile(i);
        } else if geaendert {
            self.berechne_alle();
        }
        if let Some(p) = ziel_zeile {
            self.waehle(p);
        }
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if ui.button("Anschlusspunkte laden").clicked() {
                self.lade_punkte(projekt);
            }
            if ui.button(RichText::new("Absteckdaten").strong()).clicked() {
                self.station = lade_standpunkt(&projekt.projektdaten);
                self.hintergrund = hintergrundpunkte(projekt);
                self.ziel.aktualisiere(projekt);
                self.berechne_alle();
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
    }

    fn zeichne_karte(&mut self, ui: &mut egui::Ui) {
        let plan = PlanDaten {
            station: self.station.as_ref(),
            punkte: &self.punkte,
            aktuell: self.aktuell,
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
    use crate::rechnen::absteckung::STATUS_OFFEN;

    fn ansicht() -> PunktAnsicht {
        let mut a = PunktAnsicht {
            station: Some(StandpunktInfo { punkt_nr: "S".into(), r: 0.0, h: 0.0, orientierung_gon: 0.0 }),
            zeilen: vec![Zeile::default(); MAX_ZEILEN],
            punkte: Vec::new(),
            zeile_zu_punkt: vec![None; MAX_ZEILEN],
            aktuell: None,
            hintergrund: vec![("X1".into(), 1.0, 2.0), ("X2".into(), 3.5, 4.25)],
            einweiser: Einweiser::default(),
            meldung: String::new(),
            ziel: ProtokollZiel::aus_werten(std::path::PathBuf::new(), false, "Tester"),
        };
        for (i, (nr, r, h)) in [("A", "0", "10"), ("B", "10", "0"), ("C", "x", "5"), ("", "1", "1"), ("D", "5,5", "5")].iter().enumerate() {
            a.zeilen[i] = Zeile { nr: (*nr).into(), r: (*r).into(), h: (*h).into() };
        }
        a
    }

    #[test]
    fn berechnet_nur_gueltige_zeilen() {
        let mut a = ansicht();
        a.berechne_alle();
        let nummern: Vec<&str> = a.punkte.iter().map(|p| p.punkt_nr.as_str()).collect();
        assert_eq!(nummern, ["A", "B", "D"]);
        assert_eq!(a.zeile_zu_punkt[..5], [Some(0), Some(1), None, None, Some(2)]);
        assert_eq!(a.aktuell, Some(0));
        assert!((a.punkte[1].hz_soll_gon - 100.0).abs() < 1e-9);
        assert_eq!(a.punkte[2].r_soll, 5.5);
    }

    #[test]
    fn markieren_springt_zum_naechsten_offenen_und_status_bleibt() {
        let mut a = ansicht();
        a.berechne_alle();
        a.markiere("A");
        assert!(a.punkte[0].ist_abgesteckt());
        assert_eq!(a.aktuell, Some(1));
        a.markiere("D");
        assert_eq!(a.aktuell, Some(1), "von D weiter zum ersten offenen (B)");
        a.markiere("B");
        // alles erledigt: bleibt beim zuletzt markierten
        assert_eq!(a.aktuell, Some(1));
        assert!(a.punkte.iter().all(AbsteckPunkt::ist_abgesteckt));
        // Neuberechnung ohne Änderung behält den Status, geänderte Koordinaten setzen ihn zurück
        a.zeilen[0].h = "11".into();
        a.berechne_alle();
        assert_eq!(a.punkte[0].status, STATUS_OFFEN);
        assert!(a.punkte[1].ist_abgesteckt());
    }

    #[test]
    fn fangen_fuellt_die_erste_freie_zeile() {
        let mut a = ansicht();
        a.zeilen = vec![Zeile::default(); MAX_ZEILEN];
        a.fange(0);
        a.fange(1);
        assert_eq!((a.zeilen[0].nr.as_str(), a.zeilen[0].r.as_str(), a.zeilen[0].h.as_str()), ("X1", "1.000", "2.000"));
        assert_eq!(a.zeilen[1].r, "3.500");
        assert_eq!(a.punkte.len(), 2);
        assert_eq!(a.punkte[1].h_soll, 4.25);
        a.fange(9);
        assert_eq!(a.punkte.len(), 2, "unbekannter Index tut nichts");
        for z in &mut a.zeilen {
            *z = Zeile { nr: "N".into(), r: "1".into(), h: "1".into() };
        }
        a.fange(0);
        assert_eq!(a.meldung, "Keine freie Zeile mehr.");
    }

    #[test]
    fn loeschen_haelt_zeilenzahl() {
        let mut a = ansicht();
        a.berechne_alle();
        a.loesche_zeile(0);
        assert_eq!(a.zeilen.len(), MAX_ZEILEN);
        assert_eq!(a.punkte.len(), 2);
        assert_eq!(a.punkte[0].punkt_nr, "B");
    }

    #[test]
    fn protokoll_nur_mit_punkten() {
        let mut a = ansicht();
        assert!(a.protokoll_bauen().is_none());
        a.berechne_alle();
        assert!(a.protokoll_bauen().is_some());
    }
}
