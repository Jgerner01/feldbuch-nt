// Zweck:    Entities der DXF-Ansicht (Zeichnungsobjekte und Feldbuch-Overlays) mit Zeichnen, Picking und Snap,
//           Symbole und Farben wie in der Originalversion (zoom-invariante Symbole in Bildschirmpixeln)
// Hardware: keine
// Autor:    Johann Gerner

use std::f64::consts::PI;

use eframe::egui::{Align2, Color32, FontId, Painter, Pos2, Shape, Stroke};

use crate::daten::modelle::{FeldbuchPunkt, NeupunktErgebnis, PunktResiduum};
use crate::grundlagen::format::de;

/// Segmentwinkel bei der Näherung von Bögen [Grad].
const BOGEN_SCHRITT_GRAD: f64 = 3.0;
/// Mindestanzahl Segmente eines Bogens.
const BOGEN_MIN_SEGMENTE: usize = 8;
/// Punkte der Fehlerellipse.
const ELLIPSEN_PUNKTE: usize = 36;
/// Strichlänge gepunkteter Kreise [px].
const PUNKT_STRICH_PX: f32 = 1.5;
/// Lückenlänge gepunkteter Kreise [px].
const PUNKT_LUECKE_PX: f32 = 2.5;
/// Höhenfarbe (dunkelbraun).
const FARBE_HOEHE: Color32 = Color32::from_rgb(120, 60, 0);
/// Standpunktfarbe (rot).
const FARBE_STANDPUNKT: Color32 = Color32::from_rgb(200, 30, 30);
/// Neupunktfarbe (grün).
const FARBE_NEUPUNKT: Color32 = Color32::from_rgb(20, 150, 40);
/// Beschriftungsfarbe der Importpunkte (grün).
const FARBE_BESCHRIFTUNG: Color32 = Color32::from_rgb(0, 160, 60);
/// Symbolfarbe der DXF-Punktmarker (orange).
const FARBE_MARKER: Color32 = Color32::from_rgb(200, 140, 30);
/// Farbe der gemessenen Neupunkte (blau).
const FARBE_GEMESSEN: Color32 = Color32::from_rgb(20, 80, 200);
/// Beschriftungsfarbe der gemessenen Neupunkte (dunkelblau).
const FARBE_GEMESSEN_TEXT: Color32 = Color32::from_rgb(10, 60, 160);

/// Ansichtstransformation: Welt (R/H, y nach oben) ↔ Bildschirm.
#[derive(Clone, Copy, Debug)]
pub struct Ansicht {
    pub mitte: [f64; 2],
    pub scale: f64,
    pub ursprung: Pos2,
}

impl Ansicht {
    pub fn zu_bildschirm(&self, x: f64, y: f64) -> Pos2 {
        Pos2::new(
            self.ursprung.x + ((x - self.mitte[0]) * self.scale) as f32,
            self.ursprung.y - ((y - self.mitte[1]) * self.scale) as f32,
        )
    }

    pub fn zu_welt(&self, p: Pos2) -> (f64, f64) {
        (
            self.mitte[0] + f64::from(p.x - self.ursprung.x) / self.scale,
            self.mitte[1] - f64::from(p.y - self.ursprung.y) / self.scale,
        )
    }
}

/// Umschließendes Rechteck.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rahmen {
    pub min_x: f64,
    pub min_y: f64,
    pub max_x: f64,
    pub max_y: f64,
}

impl Rahmen {
    pub fn neu(min_x: f64, min_y: f64, max_x: f64, max_y: f64) -> Self {
        Self { min_x, min_y, max_x, max_y }
    }
    pub fn breite(&self) -> f64 {
        self.max_x - self.min_x
    }
    pub fn hoehe(&self) -> f64 {
        self.max_y - self.min_y
    }
    pub fn mitte_x(&self) -> f64 {
        (self.min_x + self.max_x) / 2.0
    }
    pub fn mitte_y(&self) -> f64 {
        (self.min_y + self.max_y) / 2.0
    }
    pub fn erweitert(&self, o: &Rahmen) -> Rahmen {
        Rahmen::neu(self.min_x.min(o.min_x), self.min_y.min(o.min_y), self.max_x.max(o.max_x), self.max_y.max(o.max_y))
    }
}

/// Katasterpunkt-Typ nach Layername.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KatasterPunktTyp {
    GrenzpunktGenau,
    GrenzpunktSonstiger,
    Katasterfestpunkt,
    Sonstig,
}

/// Geometrie bzw. Inhalt einer Entity.
#[derive(Clone, Debug)]
pub enum Art {
    Line { x1: f64, y1: f64, x2: f64, y2: f64 },
    Circle { cx: f64, cy: f64, radius: f64 },
    Arc { cx: f64, cy: f64, radius: f64, start: f64, ende: f64 },
    LwPolyline { punkte: Vec<(f64, f64)>, geschlossen: bool },
    Point { x: f64, y: f64 },
    Text { x: f64, y: f64, hoehe: f64, text: String },
    Insert { block: String, x: f64, y: f64 },
    /// Feldbuchpunkt vom Typ Standpunkt.
    Standpunkt(FeldbuchPunkt),
    /// Feldbuchpunkt vom Typ Neupunkt.
    Neupunkt(FeldbuchPunkt),
    /// Importierter Punkt.
    Import { nr: String, wx: f64, wy: f64, hoehe: f64 },
    /// Gemessener Neupunkt.
    Gemessen(NeupunktErgebnis),
    /// Residual-Marker eines Anschlusspunkts.
    Residual { res: PunktResiduum, wx: f64, wy: f64 },
    /// Skalierbare Fehlerellipse.
    Ellipse { nr: String, r: f64, h: f64, v_strecke_mm: f64, v_quer_mm: f64, phi: f64, skalierung: f64 },
    /// Beschriftung eines DXF-Punktes mit Punktnummer.
    Marker { nr: String, wx: f64, wy: f64 },
    /// Standpunkt im lokalen System (Ursprung).
    LokalerStandpunkt { name: String },
}

/// Eine Entity der Ansicht.
#[derive(Clone, Debug)]
pub struct DxfEntity {
    pub layer: String,
    pub farbe: Option<Color32>,
    pub art: Art,
}

fn dist(ax: f64, ay: f64, bx: f64, by: f64) -> f64 {
    (ax - bx).hypot(ay - by)
}

/// Abstand und nächster Punkt eines Punktes zu einer Strecke.
fn strecke_naechster(wx: f64, wy: f64, a: (f64, f64), b: (f64, f64)) -> (f64, (f64, f64)) {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len2 = dx * dx + dy * dy;
    let t = if len2 < 1e-20 { 0.0 } else { (((wx - a.0) * dx + (wy - a.1) * dy) / len2).clamp(0.0, 1.0) };
    let p = (a.0 + t * dx, a.1 + t * dy);
    (dist(wx, wy, p.0, p.1), p)
}

impl DxfEntity {
    /// Neue Overlay-Entity mit Layername.
    pub fn overlay(layer: &str, art: Art) -> Self {
        Self { layer: layer.to_string(), farbe: None, art }
    }

    /// Katasterpunkt-Typ (nur für INSERT sinnvoll).
    pub fn punkt_typ(&self) -> KatasterPunktTyp {
        match self.layer.to_lowercase().as_str() {
            "grenzpunkt_genau" => KatasterPunktTyp::GrenzpunktGenau,
            "grenzpunkt_sonstiger" => KatasterPunktTyp::GrenzpunktSonstiger,
            "katasterfestpunkt" => KatasterPunktTyp::Katasterfestpunkt,
            _ => KatasterPunktTyp::Sonstig,
        }
    }

    pub fn ist_insert(&self) -> bool {
        matches!(self.art, Art::Insert { .. })
    }

    /// Mittelpunkt bzw. Bezugspunkt für Overlays mit einem Punkt.
    pub(crate) fn bezugspunkt(&self) -> Option<(f64, f64)> {
        match &self.art {
            Art::Standpunkt(p) | Art::Neupunkt(p) => Some((p.r, p.h)),
            Art::Import { wx, wy, .. } | Art::Residual { wx, wy, .. } | Art::Marker { wx, wy, .. } => Some((*wx, *wy)),
            Art::Gemessen(e) => Some((e.r, e.h)),
            Art::Ellipse { r, h, .. } => Some((*r, *h)),
            Art::LokalerStandpunkt { .. } => Some((0.0, 0.0)),
            _ => None,
        }
    }

    pub fn bounds(&self) -> Rahmen {
        match &self.art {
            Art::Line { x1, y1, x2, y2 } => Rahmen::neu(x1.min(*x2), y1.min(*y2), x1.max(*x2), y1.max(*y2)),
            Art::Circle { cx, cy, radius } => Rahmen::neu(cx - radius, cy - radius, cx + radius, cy + radius),
            Art::Arc { cx, cy, radius, start, ende } => {
                let mut pts: Vec<(f64, f64)> = Vec::new();
                let mut sweep = (ende - start + 360.0) % 360.0;
                if sweep < 0.001 {
                    sweep = 360.0;
                }
                let mut grad = 0.0;
                while grad < sweep {
                    let rad = (start + grad) * PI / 180.0;
                    pts.push((cx + radius * rad.cos(), cy + radius * rad.sin()));
                    grad += sweep.min(90.0);
                }
                let er = ende * PI / 180.0;
                pts.push((cx + radius * er.cos(), cy + radius * er.sin()));
                Rahmen::neu(
                    pts.iter().map(|p| p.0).fold(f64::MAX, f64::min),
                    pts.iter().map(|p| p.1).fold(f64::MAX, f64::min),
                    pts.iter().map(|p| p.0).fold(f64::MIN, f64::max),
                    pts.iter().map(|p| p.1).fold(f64::MIN, f64::max),
                )
            }
            Art::LwPolyline { punkte, .. } => {
                if punkte.is_empty() {
                    return Rahmen::neu(0.0, 0.0, 0.0, 0.0);
                }
                Rahmen::neu(
                    punkte.iter().map(|p| p.0).fold(f64::MAX, f64::min),
                    punkte.iter().map(|p| p.1).fold(f64::MAX, f64::min),
                    punkte.iter().map(|p| p.0).fold(f64::MIN, f64::max),
                    punkte.iter().map(|p| p.1).fold(f64::MIN, f64::max),
                )
            }
            Art::Point { x, y } | Art::Insert { x, y, .. } => Rahmen::neu(*x, *y, *x, *y),
            Art::Text { x, y, hoehe, .. } => Rahmen::neu(*x, *y, *x, y + hoehe),
            Art::Ellipse { r, h, v_strecke_mm, v_quer_mm, skalierung, .. } => {
                let d = v_strecke_mm.abs().max(v_quer_mm.abs()) * 0.001 * skalierung + 0.1;
                Rahmen::neu(r - d, h - d, r + d, h + d)
            }
            Art::LokalerStandpunkt { .. } => Rahmen::neu(-1.0, -1.0, 1.0, 1.0),
            Art::Marker { wx, wy, .. } => Rahmen::neu(wx - 0.5, wy - 0.5, wx + 0.5, wy + 0.5),
            _ => {
                let (x, y) = self.bezugspunkt().unwrap_or((0.0, 0.0));
                Rahmen::neu(x - 1.0, y - 1.0, x + 1.0, y + 1.0)
            }
        }
    }

    pub fn distance_to(&self, wx: f64, wy: f64) -> f64 {
        match &self.art {
            Art::Line { x1, y1, x2, y2 } => strecke_naechster(wx, wy, (*x1, *y1), (*x2, *y2)).0,
            Art::Circle { cx, cy, radius } | Art::Arc { cx, cy, radius, .. } => (dist(wx, wy, *cx, *cy) - radius).abs(),
            Art::LwPolyline { punkte, geschlossen } => {
                let n = punkte.len();
                let segmente = (n + usize::from(*geschlossen)).saturating_sub(1);
                (0..segmente)
                    .map(|i| strecke_naechster(wx, wy, punkte[i], punkte[(i + 1) % n]).0)
                    .fold(f64::MAX, f64::min)
            }
            Art::Point { x, y } | Art::Insert { x, y, .. } | Art::Text { x, y, .. } => dist(wx, wy, *x, *y),
            Art::Ellipse { .. } => f64::MAX,
            _ => self.bezugspunkt().map_or(f64::MAX, |(x, y)| dist(wx, wy, x, y)),
        }
    }

    pub fn nearest_point(&self, wx: f64, wy: f64) -> (f64, f64) {
        match &self.art {
            Art::Line { x1, y1, x2, y2 } => strecke_naechster(wx, wy, (*x1, *y1), (*x2, *y2)).1,
            Art::Circle { cx, cy, radius } => {
                let (dx, dy) = (wx - cx, wy - cy);
                let d = dx.hypot(dy);
                if d < 1e-10 { (cx + radius, *cy) } else { (cx + dx / d * radius, cy + dy / d * radius) }
            }
            Art::Arc { cx, cy, radius, .. } => {
                let mut ang = (wy - cy).atan2(wx - cx).to_degrees();
                if ang < 0.0 {
                    ang += 360.0;
                }
                let rad = ang.to_radians();
                (cx + radius * rad.cos(), cy + radius * rad.sin())
            }
            Art::LwPolyline { punkte, .. } => punkte
                .iter()
                .copied()
                .min_by(|a, b| dist(a.0, a.1, wx, wy).total_cmp(&dist(b.0, b.1, wx, wy)))
                .unwrap_or((0.0, 0.0)),
            Art::Point { x, y } | Art::Insert { x, y, .. } | Art::Text { x, y, .. } => (*x, *y),
            _ => self.bezugspunkt().unwrap_or((0.0, 0.0)),
        }
    }

    /// Snap: Mittelpunkt (Kreis) oder nächster Endpunkt bzw. Eckpunkt.
    pub fn snap_point(&self, wx: f64, wy: f64) -> (f64, f64) {
        match &self.art {
            Art::Line { x1, y1, x2, y2 } => {
                if dist(wx, wy, *x1, *y1) <= dist(wx, wy, *x2, *y2) { (*x1, *y1) } else { (*x2, *y2) }
            }
            Art::Circle { cx, cy, .. } => (*cx, *cy),
            Art::Arc { cx, cy, radius, start, ende } => {
                let (sr, er) = (start.to_radians(), ende.to_radians());
                let s = (cx + radius * sr.cos(), cy + radius * sr.sin());
                let e = (cx + radius * er.cos(), cy + radius * er.sin());
                if dist(wx, wy, s.0, s.1) <= dist(wx, wy, e.0, e.1) { s } else { e }
            }
            Art::LwPolyline { .. } => self.nearest_point(wx, wy),
            _ => self.nearest_point(wx, wy),
        }
    }

    /// Infotext für die Statusleiste.
    pub fn info(&self) -> String {
        match &self.art {
            Art::Line { x1, y1, x2, y2 } => format!(
                "LINE  von ({} / {})  nach ({} / {})",
                de(*x1, 3), de(*y1, 3), de(*x2, 3), de(*y2, 3)
            ),
            Art::Circle { cx, cy, radius } => format!("CIRCLE  Mitte ({} / {})  R = {}", de(*cx, 3), de(*cy, 3), de(*radius, 3)),
            Art::Arc { cx, cy, radius, start, ende } => format!(
                "ARC  Mitte ({} / {})  R={}  {}°–{}°",
                de(*cx, 3), de(*cy, 3), de(*radius, 3), de(*start, 1), de(*ende, 1)
            ),
            Art::LwPolyline { punkte, geschlossen } => {
                format!("POLYLINE  {} Punkte{}", punkte.len(), if *geschlossen { " (geschlossen)" } else { "" })
            }
            Art::Point { x, y } => format!("POINT  ({} / {})", de(*x, 3), de(*y, 3)),
            Art::Text { x, y, text, .. } => format!("TEXT  \"{text}\"  ({} / {})", de(*x, 3), de(*y, 3)),
            Art::Insert { block, x, y } => format!("{}  ({} / {})  [{block}]", self.layer, de(*x, 3), de(*y, 3)),
            Art::Standpunkt(p) | Art::Neupunkt(p) => format!(
                "{}  {}  R={}  H={}{}",
                p.typ, p.punkt_nr, de(p.r, 3), de(p.h, 3),
                if p.ist_berechnung_3d { format!("  Höhe={}", de(p.hoehe, 3)) } else { String::new() }
            ),
            Art::Import { nr, wx, wy, hoehe } => {
                format!("Import  {nr}  R={}  H={}  Höhe={}", de(*wx, 3), de(*wy, 3), de(*hoehe, 3))
            }
            Art::Gemessen(e) => {
                let mut s = format!("Neupunkt  {}  R={}  H={}", e.punkt_nr, de(e.r, 3), de(e.h, 3));
                if e.ist_3d {
                    s.push_str(&format!("  Höhe={}", de(e.hoehe, 3)));
                }
                if !e.code.is_empty() {
                    s.push_str(&format!("  Code={}", e.code));
                }
                s.push_str(&format!("  SP={}", e.standpunkt_nr));
                s
            }
            Art::Residual { res, .. } => format!(
                "Anschlusspunkt {}  vQuer={} mm  vStr={} mm  D={} m",
                res.punkt_nr, vorzeichen(res.v_quer_mm), vorzeichen(res.v_strecke_mm), de(res.strecke_h, 1)
            ),
            Art::Ellipse { nr, v_strecke_mm, v_quer_mm, .. } => {
                format!("Fehlerellipse {nr}  vL={} mm  vQ={} mm", vorzeichen(*v_strecke_mm), vorzeichen(*v_quer_mm))
            }
            Art::Marker { nr, wx, wy } => format!("DXF-Punkt {nr}  R={}  H={}", de(*wx, 3), de(*wy, 3)),
            Art::LokalerStandpunkt { name } => {
                format!("Standpunkt  {name}  R=0,000  H=0,000  Höhe=0,000  (lokales System)")
            }
        }
    }

    /// Zeichnet die Entity. `farbe` und `breite` gelten nur für reine Geometrie.
    pub fn zeichne(&self, maler: &Painter, a: &Ansicht, farbe: Color32, breite: f32) {
        let stift = Stroke::new(breite, farbe);
        let ts = |x: f64, y: f64| a.zu_bildschirm(x, y);
        match &self.art {
            Art::Line { x1, y1, x2, y2 } => {
                maler.line_segment([ts(*x1, *y1), ts(*x2, *y2)], stift);
            }
            Art::Circle { cx, cy, radius } => {
                maler.circle_stroke(ts(*cx, *cy), (*radius * a.scale) as f32, stift);
            }
            Art::Arc { cx, cy, radius, start, ende } => {
                let mut sweep = (ende - start + 360.0) % 360.0;
                if sweep < 0.001 {
                    sweep = 360.0;
                }
                let n = ((sweep / BOGEN_SCHRITT_GRAD).ceil() as usize).max(BOGEN_MIN_SEGMENTE);
                let punkte: Vec<Pos2> = (0..=n)
                    .map(|i| {
                        let w = (start + sweep * i as f64 / n as f64).to_radians();
                        ts(cx + radius * w.cos(), cy + radius * w.sin())
                    })
                    .collect();
                maler.add(Shape::line(punkte, stift));
            }
            Art::LwPolyline { punkte, geschlossen } => {
                if punkte.len() < 2 {
                    return;
                }
                let mut pts: Vec<Pos2> = punkte.iter().map(|p| ts(p.0, p.1)).collect();
                if *geschlossen {
                    pts.push(pts[0]);
                }
                maler.add(Shape::line(pts, stift));
            }
            Art::Point { x, y } => {
                let sz = 4.0_f32.max((2.0 * a.scale / 100.0) as f32);
                let c = ts(*x, *y);
                maler.line_segment([Pos2::new(c.x - sz, c.y), Pos2::new(c.x + sz, c.y)], stift);
                maler.line_segment([Pos2::new(c.x, c.y - sz), Pos2::new(c.x, c.y + sz)], stift);
            }
            Art::Text { x, y, hoehe, text } => {
                let px = 6.0_f32.max((hoehe * a.scale) as f32);
                let anker = ts(*x, *y);
                let layout_px = px.min(128.0);
                let galley = maler.layout_no_wrap(text.clone(), FontId::proportional(layout_px), farbe);
                let mut shape = Shape::Text(eframe::egui::epaint::TextShape::new(anker, galley, farbe));
                let faktor = px / layout_px;
                shape.transform(eframe::egui::emath::TSTransform {
                    scaling: faktor,
                    translation: anker.to_vec2() * (1.0 - faktor),
                });
                maler.add(shape);
            }
            Art::Insert { x, y, .. } => zeichne_insert(maler, ts(*x, *y), self.punkt_typ(), stift),
            Art::Standpunkt(p) => zeichne_standpunkt(maler, ts(p.r, p.h), p),
            Art::Neupunkt(p) => zeichne_neupunkt(maler, ts(p.r, p.h), p),
            Art::Import { nr, wx, wy, hoehe } => zeichne_import(maler, ts(*wx, *wy), nr, *hoehe),
            Art::Gemessen(e) => zeichne_gemessen(maler, ts(e.r, e.h), e),
            Art::Residual { res, wx, wy } => zeichne_residual(maler, ts(*wx, *wy), res),
            Art::Ellipse { r, h, v_strecke_mm, v_quer_mm, phi, skalierung, .. } => {
                zeichne_ellipse(maler, a, *r, *h, *v_strecke_mm, *v_quer_mm, *phi, *skalierung);
            }
            Art::Marker { nr, wx, wy } => zeichne_marker(maler, ts(*wx, *wy), nr),
            Art::LokalerStandpunkt { name } => zeichne_lokaler_standpunkt(maler, ts(0.0, 0.0), name),
        }
    }
}

/// Wert mit Vorzeichen und einer Nachkommastelle ("+1,2" / "-0,4").
fn vorzeichen(wert: f64) -> String {
    let text = de(wert, 1);
    if wert >= 0.0 { format!("+{text}") } else { text }
}

fn kreis_punkte(c: Pos2, r: f32, n: usize) -> Vec<Pos2> {
    (0..=n)
        .map(|i| {
            let w = 2.0 * std::f32::consts::PI * i as f32 / n as f32;
            Pos2::new(c.x + r * w.cos(), c.y + r * w.sin())
        })
        .collect()
}

fn mit_alpha(f: Color32, alpha: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(f.r(), f.g(), f.b(), alpha)
}

fn zeichne_insert(maler: &Painter, c: Pos2, typ: KatasterPunktTyp, stift: Stroke) {
    match typ {
        KatasterPunktTyp::GrenzpunktGenau => {
            let farbe = Color32::from_rgb(170, 30, 30);
            maler.circle_stroke(c, 4.0, Stroke::new(1.5, farbe));
            maler.circle_filled(c, 1.25, farbe);
        }
        KatasterPunktTyp::GrenzpunktSonstiger => {
            let farbe = Color32::from_rgb(140, 90, 20);
            let punkte = kreis_punkte(c, 3.5, 24);
            maler.extend(Shape::dashed_line(&punkte, Stroke::new(1.5, farbe), PUNKT_STRICH_PX, PUNKT_LUECKE_PX));
            maler.circle_filled(c, 1.0, farbe);
        }
        KatasterPunktTyp::Katasterfestpunkt => {
            let s = Stroke::new(1.8, Color32::from_rgb(20, 60, 170));
            let r = 4.0;
            maler.circle_stroke(c, r, s);
            maler.line_segment([Pos2::new(c.x, c.y - r - 2.0), Pos2::new(c.x, c.y + r + 2.0)], s);
            maler.line_segment([Pos2::new(c.x - r - 2.0, c.y), Pos2::new(c.x + r + 2.0, c.y)], s);
        }
        KatasterPunktTyp::Sonstig => {
            let h = 2.0;
            let p = [
                Pos2::new(c.x, c.y - h),
                Pos2::new(c.x + h, c.y),
                Pos2::new(c.x, c.y + h),
                Pos2::new(c.x - h, c.y),
                Pos2::new(c.x, c.y - h),
            ];
            maler.add(Shape::line(p.to_vec(), stift));
        }
    }
}

/// Beschriftung: Nummer oben rechts, Höhe unten rechts (nur bei 3D).
fn beschriftung(maler: &Painter, c: Pos2, nr: &str, hoehe: Option<f64>, farbe: Color32, symbol_radius: f32) {
    let gap = symbol_radius + 3.0;
    maler.text(
        Pos2::new(c.x + gap * 0.4, c.y - gap - 9.0),
        Align2::LEFT_TOP,
        nr,
        FontId::proportional(9.0),
        farbe,
    );
    if let Some(h) = hoehe {
        maler.text(
            Pos2::new(c.x + gap * 0.4, c.y + gap),
            Align2::LEFT_TOP,
            de(h, 3),
            FontId::proportional(8.0),
            FARBE_HOEHE,
        );
    }
}

fn zeichne_standpunkt(maler: &Painter, c: Pos2, p: &FeldbuchPunkt) {
    let s = Stroke::new(1.8, FARBE_STANDPUNKT);
    let r = 4.0_f32;
    maler.circle_stroke(c, r, s);
    let arm = r + 2.5;
    let k = r * 0.6;
    maler.line_segment([Pos2::new(c.x - arm, c.y - arm), Pos2::new(c.x - k, c.y - k)], s);
    maler.line_segment([Pos2::new(c.x + k, c.y + k), Pos2::new(c.x + arm, c.y + arm)], s);
    maler.line_segment([Pos2::new(c.x + arm, c.y - arm), Pos2::new(c.x + k, c.y - k)], s);
    maler.line_segment([Pos2::new(c.x - k, c.y + k), Pos2::new(c.x - arm, c.y + arm)], s);
    maler.circle_filled(c, 1.25, FARBE_STANDPUNKT);
    beschriftung(maler, c, &p.punkt_nr, p.ist_berechnung_3d.then_some(p.hoehe), FARBE_STANDPUNKT, r);
}

fn zeichne_neupunkt(maler: &Painter, c: Pos2, p: &FeldbuchPunkt) {
    let h = 5.0_f32;
    let b = h * 1.155;
    let dreieck = vec![
        Pos2::new(c.x, c.y - h * 0.67),
        Pos2::new(c.x + b / 2.0, c.y + h * 0.33),
        Pos2::new(c.x - b / 2.0, c.y + h * 0.33),
    ];
    maler.add(Shape::convex_polygon(
        dreieck,
        Color32::from_rgba_unmultiplied(20, 150, 40, 180),
        Stroke::new(1.5, FARBE_NEUPUNKT),
    ));
    beschriftung(maler, c, &p.punkt_nr, p.ist_berechnung_3d.then_some(p.hoehe), FARBE_NEUPUNKT, h * 0.67);
}

fn zeichne_import(maler: &Painter, c: Pos2, nr: &str, hoehe: f64) {
    let r = 3.0_f32;
    maler.circle_stroke(c, r, Stroke::new(1.5, FARBE_STANDPUNKT));
    maler.circle_filled(c, 1.0, FARBE_STANDPUNKT);
    let gap = r + 2.0;
    maler.text(
        Pos2::new(c.x + gap * 0.4, c.y - gap - 8.0),
        Align2::LEFT_TOP,
        nr,
        FontId::proportional(8.0),
        FARBE_BESCHRIFTUNG,
    );
    if hoehe != 0.0 {
        maler.text(
            Pos2::new(c.x + gap * 0.4, c.y + gap),
            Align2::LEFT_TOP,
            de(hoehe, 3),
            FontId::proportional(7.0),
            FARBE_HOEHE,
        );
    }
}

fn zeichne_gemessen(maler: &Painter, c: Pos2, e: &NeupunktErgebnis) {
    let s = 4.0_f32;
    let rect = eframe::egui::Rect::from_center_size(c, eframe::egui::Vec2::splat(s * 2.0));
    maler.rect_filled(rect, 0.0, mit_alpha(FARBE_GEMESSEN, 160));
    maler.rect_stroke(rect, 0.0, Stroke::new(1.5, FARBE_GEMESSEN), eframe::egui::StrokeKind::Middle);
    maler.circle_filled(c, 1.2, FARBE_GEMESSEN);

    let gap = s + 3.0;
    let mut y = c.y - gap - 9.0 - 1.0;
    maler.text(Pos2::new(c.x + gap * 0.4, y), Align2::LEFT_TOP, &e.punkt_nr, FontId::proportional(9.0), FARBE_GEMESSEN_TEXT);
    y += 9.0 + 1.0;
    if !e.code.is_empty() {
        maler.text(Pos2::new(c.x + gap * 0.4, y), Align2::LEFT_TOP, &e.code, FontId::proportional(8.0), FARBE_GEMESSEN_TEXT);
    }
    if e.ist_3d {
        maler.text(
            Pos2::new(c.x + gap * 0.4, c.y + gap),
            Align2::LEFT_TOP,
            de(e.hoehe, 3),
            FontId::proportional(7.5),
            FARBE_HOEHE,
        );
    }
}

/// Farbe und Radius des Residual-Markers nach Gesamtabweichung [mm].
fn residual_stufe(res: &PunktResiduum) -> (f64, Color32, f32) {
    let v_ges = res.v_quer_mm.hypot(res.v_strecke_mm);
    let (farbe, radius) = if v_ges > 20.0 {
        (Color32::from_rgb(220, 20, 20), 6.0)
    } else if v_ges > 8.0 {
        (Color32::from_rgb(220, 140, 0), 5.0)
    } else if v_ges > 3.0 {
        (Color32::from_rgb(180, 200, 0), 4.0)
    } else {
        (Color32::from_rgb(40, 180, 50), 3.0)
    };
    (v_ges, farbe, radius)
}

fn zeichne_residual(maler: &Painter, c: Pos2, res: &PunktResiduum) {
    let (v_ges, farbe, r) = residual_stufe(res);
    maler.circle_stroke(c, r + 2.0, Stroke::new(1.5, farbe));
    maler.circle_filled(c, r, mit_alpha(farbe, 120));
    maler.text(
        Pos2::new(c.x + r + 2.0, c.y),
        Align2::LEFT_CENTER,
        de(v_ges, 1),
        FontId::proportional(7.5),
        farbe,
    );
}

#[allow(clippy::too_many_arguments)]
fn zeichne_ellipse(maler: &Painter, a: &Ansicht, r: f64, h: f64, v_str: f64, v_quer: f64, phi: f64, skal: f64) {
    let min_r = 0.5 / a.scale;
    let ha = (v_str.abs() * 0.001 * skal).max(min_r);
    let hb = (v_quer.abs() * 0.001 * skal).max(min_r);
    let (sin_p, cos_p) = phi.sin_cos();
    let mut punkte: Vec<Pos2> = (0..ELLIPSEN_PUNKTE)
        .map(|i| {
            let t = 2.0 * PI * i as f64 / ELLIPSEN_PUNKTE as f64;
            let (lx, ly) = (ha * t.cos(), hb * t.sin());
            a.zu_bildschirm(r + lx * cos_p - ly * sin_p, h + lx * sin_p + ly * cos_p)
        })
        .collect();
    punkte.push(punkte[0]);
    let stift = Stroke::new(1.5, FARBE_GEMESSEN);
    maler.add(Shape::line(punkte, stift));
    let c = a.zu_bildschirm(r, h);
    let d = 3.5;
    maler.line_segment([Pos2::new(c.x - d, c.y), Pos2::new(c.x + d, c.y)], stift);
    maler.line_segment([Pos2::new(c.x, c.y - d), Pos2::new(c.x, c.y + d)], stift);
}

fn zeichne_marker(maler: &Painter, c: Pos2, nr: &str) {
    maler.circle_stroke(c, 2.5, Stroke::new(1.0, FARBE_MARKER));
    maler.circle_filled(c, 1.0, FARBE_MARKER);
    let gap = 2.5 + 2.0;
    maler.text(
        Pos2::new(c.x + gap * 0.4, c.y - gap - 12.0),
        Align2::LEFT_TOP,
        nr,
        FontId::proportional(12.0),
        FARBE_BESCHRIFTUNG,
    );
}

fn zeichne_lokaler_standpunkt(maler: &Painter, c: Pos2, name: &str) {
    let farbe = Color32::from_rgb(200, 30, 30);
    let r = 6.0_f32;
    let dreieck = vec![
        Pos2::new(c.x, c.y - r * 1.2),
        Pos2::new(c.x - r * 1.05, c.y + r * 0.7),
        Pos2::new(c.x + r * 1.05, c.y + r * 0.7),
        Pos2::new(c.x, c.y - r * 1.2),
    ];
    maler.add(Shape::line(dreieck, Stroke::new(1.8, farbe)));
    maler.circle_filled(c, 1.5, farbe);
    maler.text(Pos2::new(c.x + r + 3.0, c.y + 2.0), Align2::LEFT_TOP, name, FontId::proportional(11.0), farbe);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn linie() -> DxfEntity {
        DxfEntity { layer: "0".into(), farbe: None, art: Art::Line { x1: 0.0, y1: 0.0, x2: 10.0, y2: 0.0 } }
    }

    #[test]
    fn linie_abstand_snap_und_nearest() {
        let l = linie();
        assert!((l.distance_to(5.0, 3.0) - 3.0).abs() < 1e-12);
        assert_eq!(l.snap_point(8.0, 2.0), (10.0, 0.0));
        assert_eq!(l.snap_point(1.0, 2.0), (0.0, 0.0));
        assert_eq!(l.nearest_point(5.0, 3.0), (5.0, 0.0));
        assert_eq!(l.nearest_point(-4.0, 1.0), (0.0, 0.0));
    }

    #[test]
    fn kreis_snap_ist_mittelpunkt() {
        let k = DxfEntity { layer: "0".into(), farbe: None, art: Art::Circle { cx: 5.0, cy: 5.0, radius: 2.0 } };
        assert_eq!(k.snap_point(0.0, 0.0), (5.0, 5.0));
        assert!((k.distance_to(5.0, 8.0) - 1.0).abs() < 1e-12);
        let b = k.bounds();
        assert_eq!((b.min_x, b.max_y), (3.0, 7.0));
    }

    #[test]
    fn bogen_bounds_enthalten_90_grad_punkte() {
        let a = DxfEntity { layer: "0".into(), farbe: None, art: Art::Arc { cx: 0.0, cy: 0.0, radius: 10.0, start: 0.0, ende: 180.0 } };
        let b = a.bounds();
        assert!((b.max_y - 10.0).abs() < 1e-9);
        assert!((b.min_x + 10.0).abs() < 1e-9);
    }

    #[test]
    fn ansicht_rundlauf_und_y_achse_nach_oben() {
        let a = Ansicht { mitte: [100.0, 200.0], scale: 2.0, ursprung: Pos2::new(400.0, 300.0) };
        let p = a.zu_bildschirm(110.0, 205.0);
        assert_eq!((p.x, p.y), (420.0, 290.0));
        let (x, y) = a.zu_welt(p);
        assert!((x - 110.0).abs() < 1e-4 && (y - 205.0).abs() < 1e-4);
    }

    #[test]
    fn katasterpunkt_typ_nach_layer() {
        let e = DxfEntity { layer: "Grenzpunkt_Genau".into(), farbe: None, art: Art::Insert { block: "b".into(), x: 0.0, y: 0.0 } };
        assert_eq!(e.punkt_typ(), KatasterPunktTyp::GrenzpunktGenau);
    }
}
