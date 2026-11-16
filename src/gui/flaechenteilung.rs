// Zweck:    Formular Flächenteilung (Abspaltung): Eckpunkteingabe, sechs Verfahren, Ergebnis mit neuen
//           Grenzpunkten und Vorschau mit Auswahl von Grundkante, Eckpunkt und festem Punkt per Klick
// Hardware: keine
// Autor:    Johann Gerner

use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, RichText, Sense, Stroke, Vec2};

use super::tabelle::Raster;
use crate::daten::modelle::jetzt;
use crate::daten::projekt::Projekt;
use crate::grundlagen::format::{f, parse_zahl};
use crate::protokolle::{ausgabeverzeichnis, fl_teilung as ft_protokoll};
use crate::rechnen::flaechenteilung::{self as ft, Basis, Ergebnis, Punkt};

/// Verfahrensnamen in der Reihenfolge der Auswahl.
const VERFAHREN: [&str; 6] =
    ["1  Parallele zur Grundseite", "2  Durch festen Punkt", "3  Gegebene Richtung", "4  Dreiecksabtrennung", "5  Verhältnisteilung", "6  Streifenabtrennung"];
/// Basisverfahren der Verhältnisteilung.
const BASIS: [&str; 3] = ["Parallele zur Grundseite", "Durch festen Punkt", "Gegebene Richtung"];
/// Spaltenbreiten der Eckpunkttabelle: R, H, Löschen.
const BREITEN_ECKEN: [f32; 3] = [110.0, 110.0, 26.0];
/// Breite der Zeilenköpfe.
const ZEILENKOPF_BREITE: f32 = 30.0;
/// Spaltenbreiten der Grenzpunkttabelle: Nr, R, H.
const BREITEN_GRENZPUNKTE: [f32; 3] = [60.0, 110.0, 110.0];
/// Höhe der Vorschau.
const VORSCHAU_HOEHE: f32 = 380.0;
/// Rand der Vorschau [px].
const VORSCHAU_RAND: f32 = 30.0;
/// Toleranz, unter der die Flächendifferenz als grün gilt [m²].
const DIFFERENZ_GRUEN: f64 = 0.01;
/// Farben der Vorschau.
const FARBE_HINTERGRUND: Color32 = Color32::from_rgb(250, 250, 250);
const FARBE_TEIL1: Color32 = Color32::from_rgba_premultiplied(40, 110, 40, 60);
const FARBE_TEIL2: Color32 = Color32::from_rgba_premultiplied(30, 60, 120, 50);
const FARBE_AUSWAHL: Color32 = Color32::from_rgb(220, 30, 30);
const FARBE_LINIE: Color32 = Color32::from_rgb(30, 30, 30);
const FARBE_GRENZPUNKT: Color32 = Color32::from_rgb(200, 0, 120);
/// Radius der Punktmarken [px].
const PUNKT_RADIUS: f32 = 4.0;

/// Eingabezeile der Eckpunkte.
#[derive(Clone, Default)]
struct Ecke {
    r: String,
    h: String,
}

/// Formular der Flächenteilung.
pub struct FlaechenteilungAnsicht {
    verfahren: usize,
    ecken: Vec<Ecke>,
    polygon: Vec<Punkt>,
    kante: usize,
    eckpunkt: usize,
    a_soll: String,
    fest_r: String,
    fest_h: String,
    richtung: String,
    breite: String,
    verhaeltnis_a: String,
    verhaeltnis_b: String,
    basis: usize,
    ergebnis: Option<Ergebnis>,
    meldung: String,
}

fn abstand_punkt_strecke(p: Punkt, a: Punkt, b: Punkt) -> f64 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let laenge2 = dx * dx + dy * dy;
    if laenge2 < 1e-12 {
        return (p.0 - a.0).hypot(p.1 - a.1);
    }
    let t = (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / laenge2).clamp(0.0, 1.0);
    (p.0 - a.0 - t * dx).hypot(p.1 - a.1 - t * dy)
}

impl Default for FlaechenteilungAnsicht {
    fn default() -> Self {
        Self {
            verfahren: 0,
            ecken: vec![Ecke::default(); 4],
            polygon: Vec::new(),
            kante: 0,
            eckpunkt: 0,
            a_soll: String::new(),
            fest_r: String::new(),
            fest_h: String::new(),
            richtung: "0.0000".to_string(),
            breite: String::new(),
            verhaeltnis_a: "1".to_string(),
            verhaeltnis_b: "1".to_string(),
            basis: 0,
            ergebnis: None,
            meldung: String::new(),
        }
    }
}

impl FlaechenteilungAnsicht {
    /// Liest die Eckpunkttabelle neu ein und verwirft das Ergebnis.
    fn aktualisiere_polygon(&mut self) {
        self.polygon = self.ecken.iter().filter_map(|e| Some((parse_zahl(&e.r)?, parse_zahl(&e.h)?))).collect();
        self.kante = self.kante.min(self.polygon.len().saturating_sub(1));
        self.eckpunkt = self.eckpunkt.min(self.polygon.len().saturating_sub(1));
        self.ergebnis = None;
    }

    fn lade_csv(&mut self) {
        let Some(pfad) = rfd::FileDialog::new().set_title("Punkte laden (CSV)").add_filter("CSV-Dateien", &["csv"]).add_filter("Alle Dateien", &["*"]).pick_file() else {
            return;
        };
        match crate::daten::kor_csv::lese_punkte(&pfad) {
            Ok(punkte) => {
                self.ecken = punkte.iter().map(|p| Ecke { r: f(p.r, 4), h: f(p.h, 4) }).collect();
                self.aktualisiere_polygon();
                self.meldung = format!("{} Eckpunkte geladen.", self.polygon.len());
            }
            Err(fehler) => self.meldung = format!("Fehler beim Laden:\n{fehler}"),
        }
    }

    fn lese(text: &str, name: &str) -> Result<f64, String> {
        parse_zahl(text).ok_or_else(|| format!("Ungültiger Wert für {name}: '{text}'"))
    }

    fn berechne(&mut self) {
        self.meldung.clear();
        if self.polygon.len() < 3 {
            self.meldung = "Mindestens 3 Eckpunkte eingeben.".to_string();
            return;
        }
        match self.rechne() {
            Ok(e) => self.ergebnis = Some(e),
            Err(fehler) => {
                self.ergebnis = None;
                self.meldung = format!("Berechnungsfehler:\n{fehler}");
            }
        }
    }

    fn rechne(&self) -> Result<Ergebnis, String> {
        let poly = &self.polygon;
        match self.verfahren {
            0 => ft::parallele(poly, self.kante, Self::lese(&self.a_soll, "Soll-Fläche")?),
            1 => {
                let fest = (Self::lese(&self.fest_r, "R (fester Punkt)")?, Self::lese(&self.fest_h, "H (fester Punkt)")?);
                ft::fester_punkt(poly, fest, Self::lese(&self.a_soll, "Soll-Fläche")?)
            }
            2 => ft::gegebene_richtung(poly, Self::lese(&self.richtung, "Richtung [gon]")?, Self::lese(&self.a_soll, "Soll-Fläche")?),
            3 => ft::dreieck(poly, self.eckpunkt, Self::lese(&self.a_soll, "Soll-Fläche")?),
            4 => {
                let (a, b) = (Self::lese(&self.verhaeltnis_a, "Verhältnis a")?, Self::lese(&self.verhaeltnis_b, "Verhältnis b")?);
                let basis = match self.basis {
                    1 => match (parse_zahl(&self.fest_r), parse_zahl(&self.fest_h)) {
                        (Some(r), Some(h)) => Basis::FesterPunkt { punkt: (r, h) },
                        _ => Basis::Parallele { grundseite: self.kante },
                    },
                    2 => Basis::Richtung { gon: Self::lese(&self.richtung, "Richtung [gon]")? },
                    _ => Basis::Parallele { grundseite: self.kante },
                };
                ft::verhaeltnis(poly, a, b, basis)
            }
            _ => ft::streifen(poly, self.kante, Self::lese(&self.breite, "Streifenbreite [m]")?),
        }
    }

    fn hinweis(&self) -> &'static str {
        match self.verfahren {
            0 => "Grundkante wählen, dann Soll-Fläche eingeben.",
            1 => "Festen Punkt eingeben oder in der Vorschau klicken.",
            2 => "Trennrichtung [gon] und Soll-Fläche eingeben.",
            3 => "Eckpunkt wählen, dann Soll-Fläche eingeben.",
            4 => "Verhältnis a:b, Basisverfahren und Grundkante wählen.",
            _ => "Grundkante und Streifenbreite [m] eingeben.",
        }
    }

    fn braucht_kante(&self) -> bool {
        matches!(self.verfahren, 0 | 4 | 5)
    }

    // ── Darstellung ───────────────────────────────────────────────────────────

    pub fn zeichne(&mut self, ui: &mut egui::Ui, projekt: &Projekt) {
        ui.heading("Flächenteilung");
        ui.columns(2, |spalten| {
            self.zeichne_eingabe(&mut spalten[0], projekt);
            self.zeichne_vorschau(&mut spalten[1]);
        });
    }

    fn zeichne_eingabe(&mut self, ui: &mut egui::Ui, projekt: &Projekt) {
        ui.label(RichText::new("Eckpunkte").strong());
        let mut geaendert = false;
        let raster = Raster::neu(&BREITEN_ECKEN).mit_zeilenkopf(ZEILENKOPF_BREITE);
        raster.kopf(ui, &["R [m]", "H [m]", ""]);
        let mut loeschen = None;
        egui::ScrollArea::vertical().id_salt("ecken").max_height(180.0).show(ui, |ui| {
            for (i, ecke) in self.ecken.iter_mut().enumerate() {
                raster.zeile(ui, i, &(i + 1).to_string(), |z| {
                    geaendert |= z.eingabe(&mut ecke.r);
                    geaendert |= z.eingabe(&mut ecke.h);
                    if z.knopf("✕") {
                        loeschen = Some(i);
                    }
                });
            }
        });
        if let Some(i) = loeschen {
            self.ecken.remove(i);
            geaendert = true;
        }
        ui.horizontal(|ui| {
            if ui.small_button("+ Zeile").clicked() {
                self.ecken.push(Ecke::default());
            }
            if ui.small_button("CSV laden …").clicked() {
                self.lade_csv();
            }
        });
        if geaendert {
            self.aktualisiere_polygon();
        } else if self.polygon.is_empty() {
            self.aktualisiere_polygon();
        }
        let a_ges = if self.polygon.len() >= 3 { format!("{} m²", f(ft::flaeche(&self.polygon), 2)) } else { "–".to_string() };
        ui.label(RichText::new(format!("A_ges = {a_ges}")).monospace());

        ui.add_space(6.0);
        egui::ComboBox::from_label("Verfahren").selected_text(VERFAHREN[self.verfahren]).show_ui(ui, |ui| {
            for (i, name) in VERFAHREN.iter().enumerate() {
                ui.selectable_value(&mut self.verfahren, i, *name);
            }
        });
        ui.label(self.hinweis());

        if self.braucht_kante() && !self.polygon.is_empty() {
            let n = self.polygon.len();
            egui::ComboBox::from_label("Grundkante").selected_text(format!("Kante {}–{}", self.kante + 1, (self.kante + 1) % n + 1)).show_ui(ui, |ui| {
                for i in 0..n {
                    ui.selectable_value(&mut self.kante, i, format!("Kante {}–{}", i + 1, (i + 1) % n + 1));
                }
            });
        }
        if self.verfahren == 3 && !self.polygon.is_empty() {
            let p = self.polygon[self.eckpunkt];
            egui::ComboBox::from_label("Eckpunkt").selected_text(format!("Eckpunkt {}  ({} / {})", self.eckpunkt + 1, f(p.0, 3), f(p.1, 3))).show_ui(ui, |ui| {
                for (i, p) in self.polygon.iter().enumerate() {
                    ui.selectable_value(&mut self.eckpunkt, i, format!("Eckpunkt {}  ({} / {})", i + 1, f(p.0, 3), f(p.1, 3)));
                }
            });
        }
        let mit_fest = self.verfahren == 1 || (self.verfahren == 4 && self.basis == 1);
        if mit_fest {
            ui.horizontal(|ui| {
                ui.label("R:");
                ui.add(egui::TextEdit::singleline(&mut self.fest_r).desired_width(90.0));
                ui.label("H:");
                ui.add(egui::TextEdit::singleline(&mut self.fest_h).desired_width(90.0));
            });
        }
        if self.verfahren == 2 || (self.verfahren == 4 && self.basis == 2) {
            ui.horizontal(|ui| {
                ui.label("Richtung [gon]:");
                ui.add(egui::TextEdit::singleline(&mut self.richtung).desired_width(90.0));
            });
        }
        if self.verfahren == 4 {
            ui.horizontal(|ui| {
                ui.label("Verhältnis a : b =");
                ui.add(egui::TextEdit::singleline(&mut self.verhaeltnis_a).desired_width(50.0));
                ui.label(":");
                ui.add(egui::TextEdit::singleline(&mut self.verhaeltnis_b).desired_width(50.0));
            });
            egui::ComboBox::from_label("Basisverfahren").selected_text(BASIS[self.basis]).show_ui(ui, |ui| {
                for (i, name) in BASIS.iter().enumerate() {
                    ui.selectable_value(&mut self.basis, i, *name);
                }
            });
        }
        if self.verfahren == 5 {
            ui.horizontal(|ui| {
                ui.label("Streifenbreite [m]:");
                ui.add(egui::TextEdit::singleline(&mut self.breite).desired_width(90.0));
            });
        } else if self.verfahren != 4 {
            ui.horizontal(|ui| {
                ui.label("Soll-Fläche A1 [m²]:");
                ui.add(egui::TextEdit::singleline(&mut self.a_soll).desired_width(90.0));
            });
        }

        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if ui.button(RichText::new("Berechnen").strong()).clicked() {
                self.berechne();
            }
            if ui.add_enabled(self.ergebnis.is_some(), egui::Button::new("Protokoll")).clicked() {
                self.schreibe_protokoll(projekt);
            }
        });
        if !self.meldung.is_empty() {
            ui.colored_label(Color32::from_rgb(180, 40, 40), &self.meldung);
        }
        self.zeichne_ergebnis(ui);
    }

    /// Schreibt Protokoll und Lageplanskizze der letzten Berechnung.
    fn schreibe_protokoll(&mut self, projekt: &Projekt) {
        let Some(e) = &self.ergebnis else {
            return;
        };
        if !projekt.einstellungen.protokoll_aktiv {
            self.meldung = "Die Protokollierung ist ausgeschaltet (Hauptmenü: „Protokollieren“).".to_string();
            return;
        }
        match ft_protokoll::schreibe(&self.polygon, e, &projekt.projektdaten.bearbeiter, jetzt(), &ausgabeverzeichnis(projekt)) {
            Ok((rtf, png)) => self.meldung = format!("Protokoll gespeichert:\n{}\n\nLageplan:\n{}", rtf.display(), png.display()),
            Err(fehler) => self.meldung = fehler,
        }
    }

    fn zeichne_ergebnis(&self, ui: &mut egui::Ui) {
        let Some(e) = &self.ergebnis else {
            return;
        };
        ui.add_space(8.0);
        ui.separator();
        ui.label(RichText::new(&e.verfahren).strong());
        ui.label(RichText::new(format!("A1 (Soll): {} m²", f(e.a1_soll, 2))).monospace());
        ui.label(RichText::new(format!("A1 (Ist):  {} m²", f(e.a1_ist, 2))).monospace());
        ui.label(RichText::new(format!("A2:        {} m²", f(e.a2, 2))).monospace());
        let d = e.differenz();
        let farbe = if d.abs() < DIFFERENZ_GRUEN { Color32::DARK_GREEN } else { Color32::DARK_RED };
        let vorzeichen = if d < 0.0 { "" } else { "+" };
        ui.label(RichText::new(format!("Δ:         {vorzeichen}{} m²", f(d, 4))).monospace().color(farbe));
        ui.add_space(4.0);
        let raster = Raster::neu(&BREITEN_GRENZPUNKTE);
        raster.kopf(ui, &["Nr", "R [m]", "H [m]"]);
        for (i, (nr, r, h)) in e.neue_grenzpunkte.iter().enumerate() {
            raster.zeile(ui, i, "", |z| {
                z.text(nr);
                z.zahl(&f(*r, 3));
                z.zahl(&f(*h, 3));
            });
        }
        ui.label("Die Übergabe an die Absteckung folgt mit dem Absteckungsmodul.");
    }

    fn zeichne_vorschau(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new("Vorschau").strong());
        let (antwort, maler) = ui.allocate_painter(Vec2::new(ui.available_width(), VORSCHAU_HOEHE), Sense::click());
        let rect = antwort.rect;
        maler.rect_filled(rect, 0.0, FARBE_HINTERGRUND);
        if self.polygon.is_empty() {
            return;
        }
        let Some(abb) = Abbildung::neu(rect, &self.polygon, self.ergebnis.as_ref()) else {
            return;
        };

        if let Some(e) = &self.ergebnis {
            fuelle(&maler, &abb, &e.polygon1, FARBE_TEIL1);
            fuelle(&maler, &abb, &e.polygon2, FARBE_TEIL2);
        }
        let n = self.polygon.len();
        for i in 0..n {
            let (a, b) = (abb.punkt(self.polygon[i]), abb.punkt(self.polygon[(i + 1) % n]));
            let gewaehlt = self.braucht_kante() && i == self.kante;
            maler.line_segment([a, b], Stroke::new(if gewaehlt { 3.0 } else { 1.5 }, if gewaehlt { FARBE_AUSWAHL } else { FARBE_LINIE }));
        }
        for (i, p) in self.polygon.iter().enumerate() {
            let pos = abb.punkt(*p);
            let gewaehlt = self.verfahren == 3 && i == self.eckpunkt;
            maler.circle_filled(pos, PUNKT_RADIUS, if gewaehlt { FARBE_AUSWAHL } else { FARBE_LINIE });
            maler.text(pos + Vec2::new(6.0, -6.0), Align2::LEFT_BOTTOM, (i + 1).to_string(), FontId::proportional(11.0), FARBE_LINIE);
        }
        if let (Some(r), Some(h)) = (parse_zahl(&self.fest_r), parse_zahl(&self.fest_h)) {
            if self.verfahren == 1 || (self.verfahren == 4 && self.basis == 1) {
                let pos = abb.punkt((r, h));
                let stift = Stroke::new(2.0, FARBE_AUSWAHL);
                maler.line_segment([pos - Vec2::new(7.0, 0.0), pos + Vec2::new(7.0, 0.0)], stift);
                maler.line_segment([pos - Vec2::new(0.0, 7.0), pos + Vec2::new(0.0, 7.0)], stift);
            }
        }
        if let Some(e) = &self.ergebnis {
            if e.neue_grenzpunkte.len() == 2 {
                let (a, b) = (abb.punkt((e.neue_grenzpunkte[0].1, e.neue_grenzpunkte[0].2)), abb.punkt((e.neue_grenzpunkte[1].1, e.neue_grenzpunkte[1].2)));
                maler.line_segment([a, b], Stroke::new(2.0, FARBE_GRENZPUNKT));
            }
            for (nr, r, h) in &e.neue_grenzpunkte {
                let pos = abb.punkt((*r, *h));
                maler.circle_filled(pos, PUNKT_RADIUS + 1.0, FARBE_GRENZPUNKT);
                maler.text(pos + Vec2::new(7.0, 7.0), Align2::LEFT_TOP, nr, FontId::proportional(12.0), FARBE_GRENZPUNKT);
            }
        }

        if antwort.clicked() {
            if let Some(klick) = antwort.interact_pointer_pos() {
                self.klick_in_vorschau(abb.welt(klick));
            }
        }
    }

    /// Klick in die Vorschau: festen Punkt setzen oder nächste Kante bzw. nächsten Eckpunkt wählen.
    fn klick_in_vorschau(&mut self, welt: Punkt) {
        if self.polygon.is_empty() {
            return;
        }
        if self.verfahren == 1 {
            self.fest_r = f(welt.0, 3);
            self.fest_h = f(welt.1, 3);
            return;
        }
        let n = self.polygon.len();
        if self.verfahren == 3 {
            self.eckpunkt = (0..n)
                .min_by(|a, b| {
                    let da = (self.polygon[*a].0 - welt.0).hypot(self.polygon[*a].1 - welt.1);
                    let db = (self.polygon[*b].0 - welt.0).hypot(self.polygon[*b].1 - welt.1);
                    da.total_cmp(&db)
                })
                .unwrap_or(0);
        } else if self.braucht_kante() {
            self.kante = (0..n)
                .min_by(|a, b| {
                    let da = abstand_punkt_strecke(welt, self.polygon[*a], self.polygon[(*a + 1) % n]);
                    let db = abstand_punkt_strecke(welt, self.polygon[*b], self.polygon[(*b + 1) % n]);
                    da.total_cmp(&db)
                })
                .unwrap_or(0);
        }
    }
}

/// Abbildung Weltkoordinaten (R, H) ↔ Bildschirm.
struct Abbildung {
    ursprung: Pos2,
    massstab: f64,
    r_min: f64,
    h_min: f64,
}

impl Abbildung {
    fn neu(rect: Rect, polygon: &[Punkt], ergebnis: Option<&Ergebnis>) -> Option<Self> {
        let mut alle: Vec<Punkt> = polygon.to_vec();
        if let Some(e) = ergebnis {
            alle.extend(e.neue_grenzpunkte.iter().map(|g| (g.1, g.2)));
        }
        let (r_min, r_max) = alle.iter().fold((f64::MAX, f64::MIN), |(lo, hi), p| (lo.min(p.0), hi.max(p.0)));
        let (h_min, h_max) = alle.iter().fold((f64::MAX, f64::MIN), |(lo, hi), p| (lo.min(p.1), hi.max(p.1)));
        let (breite, hoehe) = ((r_max - r_min).max(1e-6), (h_max - h_min).max(1e-6));
        let frei = rect.shrink(VORSCHAU_RAND);
        if frei.width() <= 1.0 || frei.height() <= 1.0 {
            return None;
        }
        let massstab = (f64::from(frei.width()) / breite).min(f64::from(frei.height()) / hoehe);
        // Zentriert im freien Bereich; Hochwert zeigt nach oben
        let ursprung = Pos2::new(frei.center().x - (breite * massstab / 2.0) as f32, frei.center().y + (hoehe * massstab / 2.0) as f32);
        Some(Self { ursprung, massstab, r_min, h_min })
    }

    fn punkt(&self, p: Punkt) -> Pos2 {
        Pos2::new(self.ursprung.x + ((p.0 - self.r_min) * self.massstab) as f32, self.ursprung.y - ((p.1 - self.h_min) * self.massstab) as f32)
    }

    fn welt(&self, pos: Pos2) -> Punkt {
        (self.r_min + f64::from(pos.x - self.ursprung.x) / self.massstab, self.h_min - f64::from(pos.y - self.ursprung.y) / self.massstab)
    }
}

fn fuelle(maler: &egui::Painter, abb: &Abbildung, poly: &[Punkt], farbe: Color32) {
    if poly.len() < 3 {
        return;
    }
    let punkte: Vec<Pos2> = poly.iter().map(|p| abb.punkt(*p)).collect();
    maler.add(egui::Shape::convex_polygon(punkte, farbe, Stroke::NONE));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ansicht_mit_quadrat() -> FlaechenteilungAnsicht {
        let mut a = FlaechenteilungAnsicht::default();
        a.ecken = [(0.0, 0.0), (100.0, 0.0), (100.0, 100.0), (0.0, 100.0)].iter().map(|(r, h)| Ecke { r: f(*r, 3), h: f(*h, 3) }).collect();
        a.aktualisiere_polygon();
        a
    }

    #[test]
    fn berechnet_parallele_aus_den_eingaben() {
        let mut a = ansicht_mit_quadrat();
        a.a_soll = "2500".to_string();
        a.berechne();
        let e = a.ergebnis.as_ref().expect("Ergebnis");
        assert!((e.a1_ist - 2500.0).abs() < 1e-3);
    }

    #[test]
    fn meldet_fehlende_eingaben() {
        let mut a = ansicht_mit_quadrat();
        a.berechne();
        assert!(a.meldung.contains("Soll-Fläche"));
        let mut leer = FlaechenteilungAnsicht::default();
        leer.berechne();
        assert!(leer.meldung.contains("3 Eckpunkte"));
    }

    #[test]
    fn klick_waehlt_naechste_kante_und_ecke() {
        let mut a = ansicht_mit_quadrat();
        a.klick_in_vorschau((50.0, 98.0));
        assert_eq!(a.kante, 2);
        a.verfahren = 3;
        a.klick_in_vorschau((99.0, 99.0));
        assert_eq!(a.eckpunkt, 2);
        a.verfahren = 1;
        a.klick_in_vorschau((12.5, 40.0));
        assert_eq!((a.fest_r.as_str(), a.fest_h.as_str()), ("12.500", "40.000"));
    }

    #[test]
    fn abbildung_ist_umkehrbar() {
        let rect = Rect::from_min_size(Pos2::new(10.0, 20.0), Vec2::new(400.0, 300.0));
        let poly = [(0.0, 0.0), (100.0, 0.0), (100.0, 50.0)];
        let abb = Abbildung::neu(rect, &poly, None).unwrap();
        let (r, h) = abb.welt(abb.punkt((37.0, 12.0)));
        assert!((r - 37.0).abs() < 1e-3 && (h - 12.0).abs() < 1e-3);
    }
}
