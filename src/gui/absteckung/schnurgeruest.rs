// Zweck:    Formular Schnurgerüst: Gebäudepolygon, Schnurgerüst im Abstand, Abstandsprüfung gegen eine Gebäudeseite
// Hardware: Tachymeter über gui::geraet (optional, Messung ist auch von Hand möglich)
// Autor:    Johann Gerner

use eframe::egui::{self, Color32, RichText, Vec2};

use super::gemeinsam::{MAX_ZEILEN, PLAN_HOEHE, PlanDaten, ProtokollZiel, hintergrundpunkte, lade_anschlusspunkte, zeichne_lageplan, standpunkt_fuer_absteckung, zeige_standpunkt};
use crate::daten::projekt::Projekt;
use crate::grundlagen::format::{f, parse_zahl};
use crate::gui::geraet::Geraet;
use crate::gui::tabelle::Raster;
use crate::protokolle::absteckung::{AbsteckProtokoll, schnurgeruest};
use crate::protokolle::signiert;
use crate::rechnen::absteckung::{
    AbsteckPunkt, StandpunktInfo, abstand_aussen, berechne_schnurgeruest, berechne_versatzlinie,
};
use crate::tachymeter::messablauf::MessErgebnis;

/// Spaltenbreiten: PunktNr, R, H.
const BREITEN: [f32; 3] = [80.0, 110.0, 110.0];
/// Breite der Zeilenköpfe.
const ZEILENKOPF_BREITE: f32 = 30.0;
/// Höhe des Tabellenbereichs.
const TABELLEN_HOEHE: f32 = 300.0;
/// Abstände unter dieser Grenze zeichnen kein Schnurgerüst [m].
const MIN_ABSTAND: f64 = 1e-6;
const GON_ZU_RAD: f64 = std::f64::consts::PI / 200.0;

#[derive(Clone, Default)]
struct Zeile {
    nr: String,
    r: String,
    h: String,
}

/// Ein Punkt des Polygons.
#[derive(Clone, Debug, PartialEq)]
struct Ecke {
    nr: Option<String>,
    r: f64,
    h: f64,
}

/// Bestätigte Messung gegen eine Gebäudeseite.
#[derive(Clone, Debug)]
struct Messung {
    abstand_ist: f64,
    abstand_soll: f64,
}

/// Formular des Schnurgerüsts.
pub struct SchnurgeruestAnsicht {
    station: Option<StandpunktInfo>,
    /// Standpunkt aus dem Rückwärtsschnitt statt aus der aktiven Aufstellung.
    rueckwaerts: bool,
    zeilen: Vec<Zeile>,
    polygon: Vec<Ecke>,
    abstand: String,
    fixiert: bool,
    achse: Option<usize>,
    gemessen: Option<(f64, f64)>,
    messungen: Vec<Messung>,
    hz_eingabe: String,
    s_eingabe: String,
    zielhoehe: String,
    wartet: bool,
    /// Bekannte Projektpunkte, im Plan anklickbar.
    hintergrund: Vec<(String, f64, f64)>,
    meldung: String,
    info: String,
    ziel: ProtokollZiel,
}

impl Drop for SchnurgeruestAnsicht {
    fn drop(&mut self) {
        let p = self.protokoll_bauen();
        self.ziel.schreibe_beim_verlassen(p, "Schnurgeruest.protokoll");
    }
}

/// Umlaufsinn: bei Gegenuhrzeigersinn wird die Reihenfolge umgekehrt, sodass das Polygon im Uhrzeigersinn läuft.
fn im_uhrzeigersinn(mut polygon: Vec<Ecke>) -> Vec<Ecke> {
    let n = polygon.len();
    let flaeche2: f64 = (0..n)
        .map(|i| {
            let (a, b) = (&polygon[i], &polygon[(i + 1) % n]);
            a.r * b.h - b.r * a.h
        })
        .sum();
    if flaeche2 > 0.0 {
        polygon.reverse();
    }
    polygon
}

impl SchnurgeruestAnsicht {
    pub fn neu(projekt: &Projekt) -> Self {
        Self {
            station: standpunkt_fuer_absteckung(projekt, false),
            rueckwaerts: false,
            zeilen: vec![Zeile::default(); MAX_ZEILEN],
            polygon: Vec::new(),
            abstand: "0".to_string(),
            fixiert: false,
            achse: None,
            gemessen: None,
            messungen: Vec::new(),
            hz_eingabe: String::new(),
            s_eingabe: String::new(),
            zielhoehe: "0.000".to_string(),
            wartet: false,
            hintergrund: hintergrundpunkte(projekt),
            meldung: String::new(),
            info: String::new(),
            ziel: ProtokollZiel::neu(projekt),
        }
    }

    fn abstand_wert(&self) -> f64 {
        parse_zahl(&self.abstand).unwrap_or(0.0)
    }

    fn punkte(&self) -> Vec<(f64, f64)> {
        self.polygon.iter().map(|e| (e.r, e.h)).collect()
    }

    /// Liest die Tabelle in das Polygon.
    fn lese_zeilen(&mut self) {
        self.polygon = self
            .zeilen
            .iter()
            .filter_map(|z| {
                let (r, h) = (parse_zahl(&z.r)?, parse_zahl(&z.h)?);
                let nr = z.nr.trim();
                Some(Ecke { nr: (!nr.is_empty()).then(|| nr.to_string()), r, h })
            })
            .collect();
        self.ziel.geschrieben = false;
    }

    fn fuelle_zeilen(&mut self) {
        self.zeilen = vec![Zeile::default(); MAX_ZEILEN];
        for (z, e) in self.zeilen.iter_mut().zip(&self.polygon) {
            *z = Zeile { nr: e.nr.clone().unwrap_or_default(), r: f(e.r, 3), h: f(e.h, 3) };
        }
    }

    /// Berechnete Schnurgerüstpunkte zur Anzeige (bei offener Strecke die Parallele).
    fn schnurgeruest_punkte(&self) -> Vec<AbsteckPunkt> {
        let abstand = self.abstand_wert();
        if abstand.abs() <= MIN_ABSTAND {
            return Vec::new();
        }
        match self.polygon.len() {
            n if n >= 3 => berechne_schnurgeruest(&self.punkte(), abstand, self.station.as_ref()),
            2 => berechne_versatzlinie((self.polygon[0].r, self.polygon[0].h), (self.polygon[1].r, self.polygon[1].h), abstand)
                .map(|l| {
                    vec![AbsteckPunkt::neu("SG01", l[0].0, l[0].1), AbsteckPunkt::neu("SG02", l[1].0, l[1].1)]
                })
                .unwrap_or_default(),
            _ => Vec::new(),
        }
    }

    fn protokoll_bauen(&self) -> Option<AbsteckProtokoll> {
        (self.polygon.len() >= 3).then(|| {
            let poly = self.punkte();
            let sg = berechne_schnurgeruest(&poly, self.abstand_wert(), self.station.as_ref());
            schnurgeruest(self.station.as_ref(), &poly, &sg, &self.abstand, &self.ziel.bearbeiter, ProtokollZiel::jetzt())
        })
    }

    /// Fügt einen angeklickten Projektpunkt als Polygonpunkt in die erste freie Zeile ein (nur vor dem Fixieren).
    fn fange(&mut self, index: usize) {
        if self.fixiert {
            return;
        }
        let Some((nr, r, h)) = self.hintergrund.get(index).cloned() else {
            return;
        };
        let Some(zeile) = self.zeilen.iter_mut().find(|z| z.r.trim().is_empty() && z.h.trim().is_empty()) else {
            self.meldung = "Keine freie Zeile mehr.".to_string();
            return;
        };
        *zeile = Zeile { nr, r: f(r, 3), h: f(h, 3) };
        self.meldung.clear();
        self.lese_zeilen();
    }

    fn fixiere_oder_loese(&mut self) {
        self.lese_zeilen();
        if self.fixiert {
            self.fixiert = false;
            self.achse = None;
            self.gemessen = None;
            return;
        }
        if self.polygon.len() < 2 {
            self.meldung = "Mindestens 2 Polygon-Punkte erforderlich.".to_string();
            return;
        }
        self.meldung.clear();
        if self.polygon.len() >= 3 {
            self.polygon = im_uhrzeigersinn(std::mem::take(&mut self.polygon));
        }
        self.fixiert = true;
        self.fuelle_zeilen();
    }

    /// Kante, gegen die gemessen wird: (Anfang, Ende).
    fn achs_kante(&self) -> Option<((f64, f64), (f64, f64))> {
        let i = self.achse?;
        let n = self.polygon.len();
        (n >= 2 && i < n).then(|| {
            let (a, b) = (&self.polygon[i], &self.polygon[(i + 1) % n]);
            ((a.r, a.h), (b.r, b.h))
        })
    }

    fn achs_text(&self) -> String {
        let (Some(i), true) = (self.achse, self.fixiert) else {
            return "Achse: – (fixieren, dann Gebäudeseite anklicken)".to_string();
        };
        let Some((a, b)) = self.achs_kante() else {
            return "Achse: – (fixieren, dann Gebäudeseite anklicken)".to_string();
        };
        let n = self.polygon.len();
        let name = |e: &Ecke| e.nr.clone().unwrap_or_else(|| format!("({}/{})", f(e.r, 2), f(e.h, 2)));
        let (e1, e2) = (name(&self.polygon[i]), name(&self.polygon[(i + 1) % n]));
        let ist = match self.gemessen {
            Some(m) => {
                let d = abstand_aussen(m, a, b);
                let abw = d - self.abstand_wert();
                format!(
                    "Ist: {} m ({})   Soll: {} m   Abw.: {} m",
                    f(d, 3),
                    if d >= 0.0 { "außen" } else { "innen" },
                    f(self.abstand_wert(), 3),
                    signiert(abw, 3)
                )
            }
            None => "–".to_string(),
        };
        format!("Achse: {e1} → {e2}    |    {ist}")
    }

    /// Gemessenen Punkt aus Hz und Horizontalstrecke bezogen auf den Standpunkt.
    fn setze_messung(&mut self, hz_gon: f64, strecke_m: f64) {
        let Some(st) = &self.station else {
            self.meldung = "Kein Standpunkt geladen.".to_string();
            return;
        };
        let richtung = (st.orientierung_gon + hz_gon) * GON_ZU_RAD;
        self.gemessen = Some((st.r + strecke_m * richtung.sin(), st.h + strecke_m * richtung.cos()));
        self.meldung.clear();
    }

    fn bestaetige_messung(&mut self) {
        let Some(m) = self.gemessen else {
            self.meldung = "Kein gemessener Punkt vorhanden.".to_string();
            return;
        };
        let Some((a, b)) = self.achs_kante().filter(|_| self.achse.is_some()) else {
            self.meldung = "Bitte zuerst eine Gebäudeseite als Achse auswählen.".to_string();
            return;
        };
        let perp = abstand_aussen(m, a, b);
        self.messungen.push(Messung { abstand_ist: perp, abstand_soll: self.abstand_wert() });
        self.meldung.clear();
        self.info = format!(
            "Messung #{} gespeichert.\nAbstand zur Gebäudeseite: {} m  (Soll: {} m)",
            self.messungen.len(),
            f(perp, 3),
            f(self.abstand_wert(), 3)
        );
    }

    /// Verarbeitet das Ergebnis einer Tachymetermessung.
    fn verarbeite_ergebnis(&mut self, ergebnis: MessErgebnis) {
        self.wartet = false;
        match ergebnis {
            MessErgebnis::Messung(m) => match (m.hz_gon, m.horizontalstrecke_m) {
                (Some(hz), Some(s)) => {
                    self.hz_eingabe = f(hz, 4);
                    self.s_eingabe = f(s, 3);
                    self.setze_messung(hz, s);
                    self.info = "Messung übernommen.".to_string();
                }
                _ => self.meldung = "Die Messung enthält keine Richtung oder Strecke.".to_string(),
            },
            MessErgebnis::Fehler(text) => self.meldung = format!("Messfehler: {text}"),
        }
    }

    pub fn zeichne(&mut self, ui: &mut egui::Ui, projekt: &Projekt, geraet: &mut Geraet) {
        if self.wartet {
            if let Some(ergebnis) = geraet.nimm_messergebnis() {
                self.verarbeite_ergebnis(ergebnis);
            } else if let Some(text) = geraet.messung_status() {
                self.info = text.to_string();
            }
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
        }
        ui.heading("Schnurgerüst");
        if zeige_standpunkt(ui, projekt, self.station.as_ref(), &mut self.rueckwaerts) {
            self.station = standpunkt_fuer_absteckung(projekt, self.rueckwaerts);
        }
        ui.add_space(4.0);
        ui.columns(2, |spalten| {
            self.zeichne_eingabe(&mut spalten[0], projekt, geraet);
            self.zeichne_karte(&mut spalten[1]);
        });
    }

    fn zeichne_eingabe(&mut self, ui: &mut egui::Ui, projekt: &Projekt, geraet: &mut Geraet) {
        let raster = Raster::neu(&BREITEN).mit_zeilenkopf(ZEILENKOPF_BREITE);
        let mut geaendert = false;
        raster.kopf(ui, &["PunktNr", "R [m]", "H [m]"]);
        ui.add_enabled_ui(!self.fixiert, |ui| {
            egui::ScrollArea::vertical().id_salt("sg_polygon").max_height(TABELLEN_HOEHE).show(ui, |ui| {
                for (i, z) in self.zeilen.iter_mut().enumerate() {
                    raster.zeile(ui, i, &(i + 1).to_string(), |zellen| {
                        geaendert |= zellen.eingabe(&mut z.nr);
                        geaendert |= zellen.eingabe(&mut z.r);
                        geaendert |= zellen.eingabe(&mut z.h);
                    });
                }
            });
        });
        if geaendert {
            self.lese_zeilen();
        }
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if ui.add_enabled(!self.fixiert, egui::Button::new("Anschlusspunkte laden")).clicked() {
                match lade_anschlusspunkte(projekt) {
                    Ok(liste) => {
                        self.zeilen = vec![Zeile::default(); MAX_ZEILEN];
                        for (z, (nr, r, h)) in self.zeilen.iter_mut().zip(liste) {
                            *z = Zeile { nr, r, h };
                        }
                        self.lese_zeilen();
                        self.meldung.clear();
                    }
                    Err(fehler) => self.meldung = fehler,
                }
            }
            if ui.add_enabled(!self.fixiert, egui::Button::new("Zeilen leeren")).clicked() {
                self.zeilen = vec![Zeile::default(); MAX_ZEILEN];
                self.lese_zeilen();
            }
            let (text, fuellung) = if self.fixiert { ("Lösen", Color32::from_rgb(140, 50, 50)) } else { ("Fixieren", Color32::from_rgb(60, 120, 60)) };
            if ui.add(egui::Button::new(RichText::new(text).strong().color(Color32::WHITE)).fill(fuellung)).clicked() {
                self.fixiere_oder_loese();
            }
        });
        ui.horizontal(|ui| {
            ui.label("Schnurgerüst-Abstand [m]:");
            ui.add(egui::TextEdit::singleline(&mut self.abstand).desired_width(70.0));
            if ui.add_enabled(self.polygon.len() >= 3, egui::Button::new("Protokoll")).clicked() {
                self.ziel.aktualisiere(projekt);
                self.meldung = match self.ziel.schreibe(self.protokoll_bauen()) {
                    Ok(text) | Err(text) => text,
                };
            }
        });
        ui.separator();
        ui.label(RichText::new(self.achs_text()).monospace());
        self.zeichne_messung(ui, geraet);
        if !self.info.is_empty() {
            ui.label(&self.info);
        }
        if !self.meldung.is_empty() {
            ui.colored_label(Color32::from_rgb(180, 40, 40), &self.meldung);
        }
        if !self.messungen.is_empty() {
            ui.add_space(4.0);
            ui.label(RichText::new(format!("Bestätigte Messungen: {}", self.messungen.len())).strong());
            for (i, m) in self.messungen.iter().enumerate() {
                ui.label(RichText::new(format!("#{}  Ist {} m   Soll {} m   Abw. {} m", i + 1, f(m.abstand_ist, 3), f(m.abstand_soll, 3), signiert(m.abstand_ist - m.abstand_soll, 3))).monospace());
            }
        }
        if !self.fixiert && !self.polygon.is_empty() {
            ui.label(format!("{} Polygonpunkte", self.polygon.len()));
        }
    }

    fn zeichne_messung(&mut self, ui: &mut egui::Ui, geraet: &mut Geraet) {
        egui::Grid::new("sg_messung").num_columns(4).spacing([8.0, 4.0]).show(ui, |ui| {
            ui.label("Hz [gon]:");
            ui.add(egui::TextEdit::singleline(&mut self.hz_eingabe).desired_width(90.0));
            ui.label("s [m]:");
            ui.add(egui::TextEdit::singleline(&mut self.s_eingabe).desired_width(90.0));
            ui.end_row();
            ui.label("Zielhöhe [m]:");
            ui.add(egui::TextEdit::singleline(&mut self.zielhoehe).desired_width(90.0));
            ui.end_row();
        });
        ui.horizontal(|ui| {
            let verbunden = geraet.ist_verbunden();
            let frei = verbunden && !geraet.beschaeftigt() && !self.wartet;
            if ui.add_enabled(frei, egui::Button::new(RichText::new("► Messen").strong().color(Color32::WHITE)).fill(Color32::from_rgb(160, 60, 20))).clicked() {
                match geraet.starte_messung(parse_zahl(&self.zielhoehe).unwrap_or(0.0)) {
                    Ok(()) => {
                        self.wartet = true;
                        self.meldung.clear();
                        self.info = "Messung startet …".to_string();
                    }
                    Err(fehler) => self.meldung = fehler,
                }
            }
            if ui.button("Messpunkt berechnen").clicked() {
                match (parse_zahl(&self.hz_eingabe), parse_zahl(&self.s_eingabe)) {
                    (Some(hz), Some(s)) => self.setze_messung(hz, s),
                    _ => self.meldung = "Hz und s als Zahlen eingeben.".to_string(),
                }
            }
            if ui.button("Messung bestätigen").clicked() {
                self.bestaetige_messung();
            }
        });
        if !geraet.ist_verbunden() {
            ui.label("Kein Tachymeter verbunden – Hz und s können von Hand eingegeben werden.");
        }
    }

    fn zeichne_karte(&mut self, ui: &mut egui::Ui) {
        let ecken: Vec<AbsteckPunkt> = self
            .polygon
            .iter()
            .enumerate()
            .map(|(i, e)| AbsteckPunkt::neu(&e.nr.clone().unwrap_or_else(|| (i + 1).to_string()), e.r, e.h))
            .collect();
        let sg = if self.fixiert { self.schnurgeruest_punkte() } else { Vec::new() };
        let poly = self.punkte();
        let plan = PlanDaten {
            station: self.station.as_ref(),
            punkte: &ecken,
            polygon: &poly,
            schnurgeruest: &sg,
            kante: self.achse.filter(|_| self.fixiert),
            gemessen: self.gemessen,
            hintergrund: &self.hintergrund,
            ..Default::default()
        };
        let klick = zeichne_lageplan(ui, &plan, Vec2::new(ui.available_width(), PLAN_HOEHE));
        if let Some(k) = klick {
            if self.fixiert {
                if let Some(kante) = k.kante {
                    self.achse = Some(kante);
                }
            } else if let Some(i) = k.hintergrund {
                self.fange(i);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ansicht() -> SchnurgeruestAnsicht {
        SchnurgeruestAnsicht {
            station: Some(StandpunktInfo { punkt_nr: "S".into(), r: 5.0, h: -20.0, orientierung_gon: 0.0 }),
            rueckwaerts: false,
            zeilen: vec![Zeile::default(); MAX_ZEILEN],
            polygon: Vec::new(),
            abstand: "2".into(),
            fixiert: false,
            achse: None,
            gemessen: None,
            messungen: Vec::new(),
            hz_eingabe: String::new(),
            s_eingabe: String::new(),
            zielhoehe: "0".into(),
            wartet: false,
            hintergrund: vec![("G1".into(), 0.0, 0.0), ("G2".into(), 10.0, 0.0)],
            meldung: String::new(),
            info: String::new(),
            ziel: ProtokollZiel::aus_werten(std::path::PathBuf::new(), false, "Tester"),
        }
    }

    fn trage_ein(a: &mut SchnurgeruestAnsicht, punkte: &[(&str, &str, &str)]) {
        for (z, (nr, r, h)) in a.zeilen.iter_mut().zip(punkte) {
            *z = Zeile { nr: (*nr).into(), r: (*r).into(), h: (*h).into() };
        }
        a.lese_zeilen();
    }

    #[test]
    fn fixieren_braucht_zwei_punkte_und_dreht_den_umlaufsinn() {
        let mut a = ansicht();
        trage_ein(&mut a, &[("", "0", "0")]);
        a.fixiere_oder_loese();
        assert!(!a.fixiert);
        assert_eq!(a.meldung, "Mindestens 2 Polygon-Punkte erforderlich.");
        // Gegen den Uhrzeigersinn eingegeben → wird umgedreht
        trage_ein(&mut a, &[("A", "0", "0"), ("B", "10", "0"), ("C", "10", "8"), ("D", "0", "8")]);
        a.fixiere_oder_loese();
        assert!(a.fixiert && a.meldung.is_empty());
        let nr: Vec<_> = a.polygon.iter().map(|e| e.nr.clone().unwrap()).collect();
        assert_eq!(nr, ["D", "C", "B", "A"]);
        assert_eq!(a.zeilen[0].nr, "D");
        assert_eq!(a.zeilen[0].r, "0.000");
        // Lösen setzt Achse und Messpunkt zurück
        a.achse = Some(1);
        a.gemessen = Some((1.0, 1.0));
        a.fixiere_oder_loese();
        assert!(!a.fixiert && a.achse.is_none() && a.gemessen.is_none());
    }

    #[test]
    fn im_uhrzeigersinn_laesst_richtige_reihenfolge() {
        let cw = vec![
            Ecke { nr: None, r: 0.0, h: 0.0 },
            Ecke { nr: None, r: 0.0, h: 8.0 },
            Ecke { nr: None, r: 10.0, h: 8.0 },
            Ecke { nr: None, r: 10.0, h: 0.0 },
        ];
        assert_eq!(im_uhrzeigersinn(cw.clone()), cw);
    }

    #[test]
    fn schnurgeruest_liegt_aussen_und_messung_zeigt_abstand() {
        let mut a = ansicht();
        trage_ein(&mut a, &[("A", "0", "0"), ("B", "0", "8"), ("C", "10", "8"), ("D", "10", "0")]);
        a.fixiere_oder_loese();
        assert_eq!(a.polygon[0].nr.as_deref(), Some("A"), "Uhrzeigersinn bleibt");
        let sg = a.schnurgeruest_punkte();
        assert_eq!(sg.len(), 4);
        assert!((sg[0].r_soll + 2.0).abs() < 1e-9 && (sg[0].h_soll + 2.0).abs() < 1e-9);
        // Achse = Südseite D→A (Index 3); ein Punkt 2 m südlich davon ist 2 m außen
        a.achse = Some(3);
        a.gemessen = Some((5.0, -2.0));
        assert!(a.achs_text().contains("Ist: 2.000 m (außen)"), "{}", a.achs_text());
        assert!(a.achs_text().contains("Abw.: 0.000 m"));
        assert!(a.achs_text().starts_with("Achse: D → A"));
        a.bestaetige_messung();
        assert_eq!(a.messungen.len(), 1);
        assert!(a.info.contains("Messung #1 gespeichert."));
    }

    #[test]
    fn fangen_fuegt_polygonpunkte_ein_aber_nicht_nach_dem_fixieren() {
        let mut a = ansicht();
        a.fange(0);
        a.fange(1);
        assert_eq!(a.polygon.len(), 2);
        assert_eq!(a.polygon[1], Ecke { nr: Some("G2".into()), r: 10.0, h: 0.0 });
        a.fixiere_oder_loese();
        assert!(a.fixiert);
        a.fange(0);
        assert_eq!(a.polygon.len(), 2, "fixiert: keine Änderung");
        a.fixiere_oder_loese();
        for z in &mut a.zeilen {
            *z = Zeile { nr: "N".into(), r: "1".into(), h: "1".into() };
        }
        a.fange(0);
        assert_eq!(a.meldung, "Keine freie Zeile mehr.");
    }

    #[test]
    fn messung_aus_hz_und_strecke() {
        let mut a = ansicht();
        // Standpunkt (5, -20), Orientierung 0: 100 gon = Osten, 10 m
        a.setze_messung(100.0, 10.0);
        let (r, h) = a.gemessen.unwrap();
        assert!((r - 15.0).abs() < 1e-9 && (h + 20.0).abs() < 1e-9);
        a.station = None;
        a.gemessen = None;
        a.setze_messung(0.0, 1.0);
        assert!(a.gemessen.is_none());
        assert_eq!(a.meldung, "Kein Standpunkt geladen.");
    }

    #[test]
    fn bestaetigen_braucht_messpunkt_und_achse() {
        let mut a = ansicht();
        a.bestaetige_messung();
        assert_eq!(a.meldung, "Kein gemessener Punkt vorhanden.");
        a.gemessen = Some((0.0, 0.0));
        a.bestaetige_messung();
        assert_eq!(a.meldung, "Bitte zuerst eine Gebäudeseite als Achse auswählen.");
        assert!(a.messungen.is_empty());
    }

    #[test]
    fn offene_strecke_zeigt_parallele_und_hat_kein_protokoll() {
        let mut a = ansicht();
        trage_ein(&mut a, &[("", "0", "0"), ("", "10", "0")]);
        a.fixiere_oder_loese();
        assert!(a.fixiert);
        let sg = a.schnurgeruest_punkte();
        assert_eq!(sg.len(), 2);
        assert!((sg[0].h_soll + 2.0).abs() < 1e-9);
        assert!(a.protokoll_bauen().is_none());
        a.abstand = "0".into();
        assert!(a.schnurgeruest_punkte().is_empty());
    }

    #[test]
    fn protokoll_ab_drei_punkten() {
        let mut a = ansicht();
        trage_ein(&mut a, &[("", "0", "0"), ("", "0", "8"), ("", "10", "8")]);
        assert!(a.protokoll_bauen().is_some());
    }
}
