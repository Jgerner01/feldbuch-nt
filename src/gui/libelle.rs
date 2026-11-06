// Zweck:    Animierte Dosenlibelle nach der Vorlage "Libelle mit drei Fußschrauben": Blase folgt der Kompensator-
//           neigung, Pfeile zeigen die Drehrichtung der Fußschrauben, grüne Haken zeigen "in Ordnung".
//           Liefert das Gerät keine Neigung, wird die Libelle ausgegraut.
// Hardware: Leica-Tachymeter mit Zweiachs-Kompensator (GeoCOM TMC_GetAngle1)
// Autor:    Johann Gerner

use std::f32::consts::{PI, TAU};

use eframe::egui::{self, Color32, Pos2, Sense, Shape, Stroke, Vec2};

/// Gesamtgröße der Zeichenfläche [px].
const FLAECHE: Vec2 = Vec2::new(200.0, 178.0);
/// Lage des Libellenmittelpunkts in der Zeichenfläche [px von oben].
const MITTE_Y: f32 = 102.0;
/// Radius des schwarzen Rings [px].
const RING_R: f32 = 66.0;
/// Radius der grünen Glasfläche [px].
const SCHEIBE_R: f32 = 58.0;
/// Radius der Libellenblase [px].
const BLASE_R: f32 = 11.0;
/// Neigung, bei der die Blase den Rand der Glasfläche erreicht [mgon].
const VOLLAUSSCHLAG_MGON: f32 = 60.0;
/// Neigung, bis zu der die Libelle als eingespielt gilt [mgon].
const TOLERANZ_MGON: f32 = 8.0;
/// Vergrößerung des gezeichneten Zielkreises gegenüber der rechnerischen Toleranz.
const ZIELKREIS_FAKTOR: f32 = 1.3;
/// Fußschraube gilt als in Ordnung, wenn ihre nötige Höhenänderung unter diesem Anteil der Toleranz bleibt.
const SCHRAUBE_TOLERANZ_ANTEIL: f32 = 0.5;
/// Vorzeichen der Querneigung (Blase nach rechts bei positiver Querneigung).
const VORZEICHEN_QUER: f32 = -1.0;
/// Vorzeichen der Längsneigung (Blase nach oben bei positiver Längsneigung, Fernrohr zeigt nach oben).
const VORZEICHEN_LAENGS: f32 = -1.0;
/// Mgon pro Radiant.
const MGON_PRO_RAD: f32 = 200_000.0 / PI;
/// Geschwindigkeit der Blasenbewegung (Glättung, 1/s).
const BLASE_GESCHWINDIGKEIT: f32 = 9.0;
/// Radius des Fußschrauben-Knopfs [px] und Zahnlänge.
const SCHRAUBE_R: f32 = 15.0;
const SCHRAUBE_ZAHN: f32 = 5.0;
const SCHRAUBE_ZAEHNE: usize = 14;
/// Abstand der Schraubenmitte vom Libellenmittelpunkt [px].
const SCHRAUBE_ABSTAND: f32 = RING_R + 2.0;
/// Radius und Öffnungswinkel des Drehpfeils um die Schraube [px, rad].
const PFEIL_R: f32 = 24.0;
const PFEIL_WINKEL: f32 = 1.9;
/// Winkel der Fußschrauben (Grad, 0 = rechts, mathematisch gegen den Uhrzeigersinn): oben, links unten, rechts unten.
const SCHRAUBEN_GRAD: [f32; 3] = [90.0, 210.0, 330.0];
/// Farbe der Drehpfeile und ihrer Kontur.
const PFEIL_FARBE: Color32 = Color32::from_rgb(240, 130, 20);
const PFEIL_KONTUR: Color32 = Color32::WHITE;
/// Farbe der Haken.
const HAKEN_FARBE: Color32 = Color32::from_rgb(40, 180, 70);
/// Anzahl der Stufen für die Farbverläufe.
const VERLAUF_STUFEN: usize = 22;

/// Zustand der Animation.
#[derive(Default)]
pub struct Libelle {
    /// Angezeigte Blasenlage in mgon (Quer rechts, Längs oben), weich nachgeführt.
    blase: Vec2,
}

fn grau(c: Color32) -> Color32 {
    let l = (0.3 * f32::from(c.r()) + 0.59 * f32::from(c.g()) + 0.11 * f32::from(c.b())) as u8;
    let l = 150 + (l / 3);
    Color32::from_gray(l.min(215))
}

fn mische(a: Color32, b: Color32, t: f32) -> Color32 {
    let m = |x: u8, y: u8| (f32::from(x) + (f32::from(y) - f32::from(x)) * t).round() as u8;
    Color32::from_rgb(m(a.r(), b.r()), m(a.g(), b.g()), m(a.b(), b.b()))
}

impl Libelle {
    /// Zeichnet die Libelle. `neigung` = (Quer, Längs) in rad; `None` = keine Kompensatordaten (ausgegraut).
    pub fn zeichne(&mut self, ui: &mut egui::Ui, neigung: Option<(f64, f64)>) {
        let (antwort, painter) = ui.allocate_painter(FLAECHE, Sense::hover());
        let c = Pos2::new(antwort.rect.center().x, antwort.rect.top() + MITTE_Y);
        let aktiv = neigung.is_some();
        let farbe = |f: Color32| if aktiv { f } else { grau(f) };

        // Ziel der Blase [mgon]; ohne Daten in der Mitte
        let ziel = match neigung {
            Some((q, l)) => Vec2::new(q as f32 * MGON_PRO_RAD * VORZEICHEN_QUER, l as f32 * MGON_PRO_RAD * VORZEICHEN_LAENGS),
            None => Vec2::ZERO,
        };
        let dt = ui.input(|i| i.stable_dt).min(0.1);
        let faktor = 1.0 - (-BLASE_GESCHWINDIGKEIT * dt).exp();
        self.blase += (ziel - self.blase) * faktor;
        if (ziel - self.blase).length() > 0.05 {
            ui.ctx().request_repaint();
        }
        let eingespielt = aktiv && self.blase.length() <= TOLERANZ_MGON;

        // Fußschrauben (unter dem Ring)
        let richtungen: Vec<Vec2> = SCHRAUBEN_GRAD
            .iter()
            .map(|g| {
                let w = g.to_radians();
                Vec2::new(w.cos(), -w.sin())
            })
            .collect();
        for r in &richtungen {
            let m = c + *r * SCHRAUBE_ABSTAND;
            painter.circle_filled(m, SCHRAUBE_R, farbe(Color32::from_gray(70)));
            for k in 0..SCHRAUBE_ZAEHNE {
                let w = TAU * k as f32 / SCHRAUBE_ZAEHNE as f32;
                let d = Vec2::new(w.cos(), w.sin());
                painter.line_segment(
                    [m + d * (SCHRAUBE_R - 1.0), m + d * (SCHRAUBE_R + SCHRAUBE_ZAHN)],
                    Stroke::new(4.0, farbe(Color32::from_gray(85))),
                );
            }
            painter.circle_stroke(m, SCHRAUBE_R, Stroke::new(1.5, farbe(Color32::from_gray(35))));
        }

        // Ring und Glasfläche mit Farbverlauf
        painter.circle_filled(c, RING_R, farbe(Color32::BLACK));
        painter.circle_stroke(c, RING_R - 1.0, Stroke::new(1.5, farbe(Color32::from_gray(90))));
        let innen = farbe(Color32::from_rgb(165, 220, 80));
        let aussen = farbe(Color32::from_rgb(55, 145, 5));
        for k in (0..VERLAUF_STUFEN).rev() {
            let t = (k + 1) as f32 / VERLAUF_STUFEN as f32;
            let versatz = Vec2::new(-0.22, -0.22) * SCHEIBE_R * (1.0 - t);
            painter.circle_filled(c + versatz, SCHEIBE_R * t, mische(innen, aussen, t.powf(1.6)));
        }

        // Fadenkreuz und Zielkreis
        let strich = Stroke::new(2.0, farbe(Color32::BLACK));
        painter.line_segment([c - Vec2::new(SCHEIBE_R, 0.0), c + Vec2::new(SCHEIBE_R, 0.0)], strich);
        painter.line_segment([c - Vec2::new(0.0, SCHEIBE_R), c + Vec2::new(0.0, SCHEIBE_R)], strich);
        let max_ausschlag = SCHEIBE_R - BLASE_R;
        let ziel_r = (TOLERANZ_MGON / VOLLAUSSCHLAG_MGON * max_ausschlag + BLASE_R) * ZIELKREIS_FAKTOR;
        painter.circle_stroke(c, ziel_r + 2.5, Stroke::new(1.5, farbe(Color32::WHITE)));
        painter.circle_stroke(c, ziel_r, strich);

        // Blase: Auslenkung auf die Glasfläche begrenzen
        let mut versatz = Vec2::new(self.blase.x, -self.blase.y) / VOLLAUSSCHLAG_MGON * max_ausschlag;
        if versatz.length() > max_ausschlag {
            versatz = versatz.normalized() * max_ausschlag;
        }
        let b = c + versatz;
        painter.circle_filled(b, BLASE_R + 1.5, farbe(Color32::from_gray(60)));
        for k in (0..VERLAUF_STUFEN).rev() {
            let t = (k + 1) as f32 / VERLAUF_STUFEN as f32;
            let m = b + Vec2::new(-0.3, -0.3) * BLASE_R * (1.0 - t);
            painter.circle_filled(m, BLASE_R * t, mische(farbe(Color32::from_gray(245)), farbe(Color32::from_gray(120)), t.powf(1.8)));
        }
        painter.line_segment([b - Vec2::new(BLASE_R, 0.0), b + Vec2::new(BLASE_R, 0.0)], Stroke::new(1.0, farbe(Color32::BLACK)));
        painter.line_segment([b - Vec2::new(0.0, BLASE_R), b + Vec2::new(0.0, BLASE_R)], Stroke::new(1.0, farbe(Color32::BLACK)));

        if !aktiv {
            painter.text(
                Pos2::new(antwort.rect.center().x, antwort.rect.bottom() - 2.0),
                egui::Align2::CENTER_BOTTOM,
                "keine Kompensatordaten",
                egui::FontId::proportional(12.0),
                Color32::from_gray(110),
            );
            return;
        }

        // Nötige Höhenänderung je Schraube: Blase zur Mitte bringen heißt, die Gegenseite anzuheben.
        // Die Blase wandert zur höheren Seite. Rechtsdrehung (von oben) hebt die Schraube (Linker-Daumen-Regel).
        let blase_math = Vec2::new(self.blase.x, self.blase.y);
        for r in &richtungen {
            let m = c + *r * SCHRAUBE_ABSTAND;
            let r_math = Vec2::new(r.x, -r.y);
            let dh = -(blase_math.x * r_math.x + blase_math.y * r_math.y);
            if eingespielt || dh.abs() < TOLERANZ_MGON * SCHRAUBE_TOLERANZ_ANTEIL {
                zeichne_haken(&painter, m + *r * (SCHRAUBE_R + 16.0));
            } else {
                zeichne_pfeil(&painter, m, *r, dh > 0.0, (dh.abs() / VOLLAUSSCHLAG_MGON).min(1.0));
            }
        }
    }
}

/// Grüner Haken mit weißer Kontur.
fn zeichne_haken(painter: &egui::Painter, m: Pos2) {
    let punkte = [m + Vec2::new(-11.0, 1.0), m + Vec2::new(-4.0, 9.0), m + Vec2::new(11.0, -10.0)];
    painter.add(Shape::line(punkte.to_vec(), Stroke::new(9.0, Color32::WHITE)));
    painter.add(Shape::line(punkte.to_vec(), Stroke::new(5.5, HAKEN_FARBE)));
}

/// Drehpfeil als Kreisbogen um die Schraube, außen vom Ring weg; `rechts` = im Uhrzeigersinn.
fn zeichne_pfeil(painter: &egui::Painter, m: Pos2, nach_aussen: Vec2, rechts: bool, staerke: f32) {
    let mitte_winkel = nach_aussen.y.atan2(nach_aussen.x);
    let breite = 3.0 + 2.5 * staerke;
    let schritte = 18;
    let start = mitte_winkel - PFEIL_WINKEL / 2.0;
    let mut punkte: Vec<Pos2> = (0..=schritte)
        .map(|k| {
            let w = start + PFEIL_WINKEL * k as f32 / schritte as f32;
            m + Vec2::new(w.cos(), w.sin()) * PFEIL_R
        })
        .collect();
    if !rechts {
        punkte.reverse();
    }
    let ende = punkte[punkte.len() - 1];
    let davor = punkte[punkte.len() - 3];
    let richtung = (ende - davor).normalized();
    let quer = Vec2::new(-richtung.y, richtung.x);
    let spitze = ende + richtung * 9.0;
    let kopf = [spitze, ende + quer * 7.5, ende - quer * 7.5];
    painter.add(Shape::line(punkte.clone(), Stroke::new(breite + 3.0, PFEIL_KONTUR)));
    painter.add(Shape::convex_polygon(kopf.to_vec(), PFEIL_KONTUR, Stroke::new(3.0, PFEIL_KONTUR)));
    painter.add(Shape::line(punkte, Stroke::new(breite, PFEIL_FARBE)));
    painter.add(Shape::convex_polygon(kopf.to_vec(), PFEIL_FARBE, Stroke::NONE));
}
