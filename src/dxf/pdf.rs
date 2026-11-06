// Maßstäbliche A4-Szene aus denselben Zeichenroutinen wie der DXF-Viewer.
// Ausgabe als PDF-Pfade und eingebettete TrueType-Schriften, ohne Rasterbilder.
use super::entity::{Ansicht, Art, DxfEntity};
use crate::daten::rechenparameter::PdfParameter;
use eframe::egui::{self, Color32, Pos2, Rect, Shape, Stroke, Vec2};
use egui::emath::TSTransform;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

pub const PX_MM: f32 = 96.0 / 25.4;
pub const FUSS_MM: f32 = 15.0;

#[derive(Clone, Debug)]
pub struct Blatt {
    pub seite: Rect,
    pub karte: Rect,
    pub parameter: PdfParameter,
}
impl Blatt {
    pub fn neu(p: &PdfParameter) -> Self {
        let p = p.normalisiert();
        let (w, h) = if p.querformat {
            (297.0, 210.0)
        } else {
            (210.0, 297.0)
        };
        let seite = Rect::from_min_size(Pos2::ZERO, Vec2::new(w, h) * PX_MM);
        let karte = Rect::from_min_max(
            Pos2::new(20.0, 10.0) * PX_MM,
            Pos2::new(w - 10.0, h - p.rand_mm as f32 - FUSS_MM) * PX_MM,
        );
        Self {
            seite,
            karte,
            parameter: p,
        }
    }
    pub fn welt_groesse(&self) -> Vec2 {
        self.karte.size() / PX_MM * self.parameter.massstab as f32 / 1000.0
    }
    pub fn ansicht(&self, mitte: [f64; 2]) -> Ansicht {
        Ansicht {
            mitte,
            scale: f64::from(PX_MM) * 1000.0 / self.parameter.massstab,
            ursprung: self.karte.center(),
        }
    }
    pub fn balken(&self) -> (f64, f32) {
        let ziel = 60.0 * self.parameter.massstab / 1000.0;
        let potenz = 10.0_f64.powf(ziel.log10().floor());
        let meter = [1.0, 2.0, 5.0, 10.0]
            .into_iter()
            .map(|n| n * potenz)
            .min_by(|a, b| (a - ziel).abs().total_cmp(&(b - ziel).abs()))
            .unwrap();
        (
            meter,
            (meter * 1000.0 / self.parameter.massstab) as f32 * PX_MM,
        )
    }
}

pub struct DruckForm {
    pub shape: Shape,
    pub transform: TSTransform,
    pub clip: Rect,
    /// Papierdekoration besitzt feste Strichbreiten, unabhängig von Zeichnungslinien.
    pub karteninhalt: bool,
    /// Exakte typographische Schriftgröße für Papierbeschriftungen.
    pub schrift_pt: Option<f32>,
}
pub struct Szene {
    pub blatt: Blatt,
    pub formen: Vec<DruckForm>,
}

/// Papiergröße der Punktzeichen; echte Geometrie und Ellipsen bleiben maßstäblich.
fn symbol_spanne(e: &DxfEntity) -> Option<f32> {
    use super::entity::KatasterPunktTyp;
    Some(match &e.art {
        Art::Point { .. } => 8.0,
        Art::Insert { .. } => match e.punkt_typ() {
            KatasterPunktTyp::GrenzpunktGenau => 8.0,
            KatasterPunktTyp::GrenzpunktSonstiger => 7.0,
            KatasterPunktTyp::Katasterfestpunkt => 12.0,
            KatasterPunktTyp::Sonstig => 4.0,
        },
        Art::Standpunkt(_) => 13.0,
        Art::Neupunkt(_) => 5.0,
        Art::Import { .. } => 6.0,
        Art::Gemessen(_) => 8.0,
        Art::Residual { .. } => 10.0,
        Art::Marker { .. } => 5.0,
        Art::LokalerStandpunkt { .. } => 11.4,
        _ => return None,
    })
}

/// Vor dem Schriftlayout aussortieren; geschnittene Linien und große Kreise bleiben erhalten.
fn im_ausschnitt(e: &DxfEntity, a: &Ansicht, karte: Rect, faktor: f32) -> bool {
    let rect = if let Art::Text { x, y, hoehe, text } = &e.art {
        let px = ((*hoehe * a.scale) as f32).max(6.0);
        let zeilen = text.lines().count().max(1) as f32;
        Rect::from_min_size(
            a.zu_bildschirm(*x, *y),
            Vec2::new(
                px * text.chars().count().max(1) as f32 * 2.0,
                px * zeilen * 2.0,
            ),
        )
    } else {
        let b = e.bounds();
        let rect = Rect::from_two_pos(
            a.zu_bildschirm(b.min_x, b.min_y),
            a.zu_bildschirm(b.max_x, b.max_y),
        );
        rect.expand(if symbol_spanne(e).is_some() {
            2048.0 * faktor
        } else {
            2.0
        })
    };
    rect.intersects(karte)
}

pub fn erzeuge(
    ctx: &egui::Context,
    p: &PdfParameter,
    mitte: [f64; 2],
    entities: &[(&DxfEntity, Color32)],
    titel: &str,
    ellipsen: Option<f64>,
) -> Szene {
    let blatt = Blatt::neu(p);
    let a = blatt.ansicht(mitte);
    let mut formen = Vec::new();
    let mut ausgabe = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(blatt.seite),
            ..Default::default()
        },
        |ui| {
            let layer = egui::LayerId::new(egui::Order::Middle, egui::Id::unique("pdf_geometrie"));
            let maler = egui::Painter::new(ui.ctx().clone(), layer, Rect::EVERYTHING);
            for (e, farbe) in entities {
                let mut faktor = symbol_spanne(e).map_or(1.0, |s| {
                    (blatt.parameter.symbol_min_mm as f32 * PX_MM / s).max(1.0)
                });
                if !im_ausschnitt(e, &a, blatt.karte, faktor) {
                    continue;
                }
                let (x, y) = match &e.art {
                    Art::Point { x, y } | Art::Insert { x, y, .. } | Art::Text { x, y, .. } => {
                        (*x, *y)
                    }
                    _ => e.bezugspunkt().unwrap_or((mitte[0], mitte[1])),
                };
                let anker = a.zu_bildschirm(x, y);
                let mut zeichnung = a;
                if let Art::Text { hoehe, .. } = &e.art {
                    // Große Papiertexte klein layouten und erst danach vektoriell skalieren.
                    // Verhindert riesige Font-Atlas-Allokationen bei z.B. 1:1.
                    faktor = ((*hoehe * a.scale) as f32 / 128.0).max(1.0);
                    zeichnung = Ansicht {
                        mitte: [x, y],
                        ursprung: anker,
                        scale: a.scale / f64::from(faktor),
                    };
                }
                let start = maler.add(Shape::Noop).0;
                e.zeichne(&maler, &zeichnung, *farbe, 1.0);
                let ende = maler.add(Shape::Noop).0;
                let transform = TSTransform {
                    scaling: faktor,
                    translation: anker.to_vec2() * (1.0 - faktor),
                };
                ui.ctx().graphics(|g| {
                    if let Some(list) = g.get(layer) {
                        for form in list.all_entries().skip(start + 1).take(ende - start - 1) {
                            formen.push(DruckForm {
                                shape: form.shape.clone(),
                                transform,
                                clip: blatt.karte,
                                karteninhalt: true,
                                schrift_pt: None,
                            });
                        }
                    }
                });
            }
            let start = maler.add(Shape::Noop).0;
            maler.rect_stroke(
                blatt.karte,
                0.0,
                Stroke::new(0.5 * PX_MM, Color32::BLACK),
                egui::StrokeKind::Inside,
            );
            let y = blatt.karte.bottom() + 3.0 * PX_MM;
            let x = blatt.karte.left();
            maler.text(
                Pos2::new(x, y),
                egui::Align2::LEFT_TOP,
                format!("1 : {:.0}", blatt.parameter.massstab),
                // 96 logische Einheiten/Zoll, 72 typographische Punkte/Zoll.
                egui::FontId::proportional(12.0 * 96.0 / 72.0),
                Color32::BLACK,
            );
            let (meter, breite) = blatt.balken();
            let bx = x + 28.0 * PX_MM;
            for i in 0..4 {
                let r = Rect::from_min_size(
                    Pos2::new(bx + i as f32 * breite / 4.0, y + 2.0),
                    Vec2::new(breite / 4.0, 2.0 * PX_MM),
                );
                maler.rect_filled(
                    r,
                    0.0,
                    if i % 2 == 0 {
                        Color32::BLACK
                    } else {
                        Color32::WHITE
                    },
                );
                maler.rect_stroke(
                    r,
                    0.0,
                    Stroke::new(0.25 * PX_MM, Color32::BLACK),
                    egui::StrokeKind::Inside,
                );
            }
            maler.text(
                Pos2::new(bx, y + 3.5 * PX_MM),
                egui::Align2::LEFT_TOP,
                "0",
                egui::FontId::proportional(9.0),
                Color32::BLACK,
            );
            maler.text(
                Pos2::new(bx + breite, y + 3.5 * PX_MM),
                egui::Align2::RIGHT_TOP,
                format!(
                    "{} m",
                    crate::grundlagen::format::de(meter, if meter < 1.0 { 3 } else { 0 })
                ),
                egui::FontId::proportional(9.0),
                Color32::BLACK,
            );
            let text = if let Some(f) = ellipsen {
                format!(
                    "{}  |  Residuen-/Konfidenzellipsen × {}",
                    titel,
                    crate::grundlagen::format::de(f, 1)
                )
            } else {
                titel.to_owned()
            };
            // Eigene Zeile: auch lange Projektnamen können die Maßstabsleiste nicht übermalen.
            maler.text(
                Pos2::new(x, blatt.karte.bottom() + 11.0 * PX_MM),
                egui::Align2::LEFT_TOP,
                text,
                egui::FontId::proportional(9.0),
                Color32::BLACK,
            );
            let ende = maler.add(Shape::Noop).0;
            ui.ctx().graphics(|g| {
                if let Some(list) = g.get(layer) {
                    for form in list.all_entries().skip(start + 1).take(ende - start - 1) {
                        formen.push(DruckForm {
                            shape: form.shape.clone(),
                            transform: TSTransform::IDENTITY,
                            clip: blatt.seite,
                            karteninhalt: false,
                            schrift_pt: if matches!(&form.shape, Shape::Text(t) if t.galley.job.text == format!("1 : {:.0}",blatt.parameter.massstab)) {Some(12.0)} else {None},
                        });
                    }
                }
            });
        },
    );
    ausgabe.textures_delta.clear();
    Szene { blatt, formen }
}

/// Mindeststrichbreite gleichermaßen für Vorschau und Datei.
pub fn mindestbreite(shape: &mut Shape, min: f32) {
    let breite = |w: &mut f32| {
        if *w > 0.0 {
            *w = w.max(min)
        }
    };
    match shape {
        Shape::Vec(v) => {
            for s in v {
                mindestbreite(s, min)
            }
        }
        Shape::Circle(s) => breite(&mut s.stroke.width),
        Shape::Ellipse(s) => breite(&mut s.stroke.width),
        Shape::Rect(s) => breite(&mut s.stroke.width),
        Shape::LineSegment { stroke, .. } => breite(&mut stroke.width),
        Shape::Path(s) => breite(&mut s.stroke.width),
        _ => {}
    }
}

// Kleiner PDF-1.4-Schreiber. Objekte und Streams sind längengenau, XRef-Offsets bytebasiert.
struct Pdf {
    obj: Vec<Vec<u8>>,
}
impl Pdf {
    fn add(&mut self, b: impl Into<Vec<u8>>) -> usize {
        self.obj.push(b.into());
        self.obj.len()
    }
    fn stream(&mut self, dict: &str, data: &[u8]) -> usize {
        let mut b = format!("<< /Length {} {} >>\nstream\n", data.len(), dict).into_bytes();
        b.extend_from_slice(data);
        b.extend_from_slice(b"\nendstream");
        self.add(b)
    }
    fn finish(self) -> Vec<u8> {
        let mut out = b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n".to_vec();
        let mut offsets = vec![0];
        for (i, b) in self.obj.iter().enumerate() {
            offsets.push(out.len());
            out.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
            out.extend_from_slice(b);
            out.extend_from_slice(b"\nendobj\n");
        }
        let xref = out.len();
        out.extend_from_slice(
            format!("xref\n0 {}\n0000000000 65535 f \n", offsets.len()).as_bytes(),
        );
        for n in offsets.iter().skip(1) {
            out.extend_from_slice(format!("{n:010} 00000 n \n").as_bytes());
        }
        out.extend_from_slice(
            format!(
                "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
                offsets.len()
            )
            .as_bytes(),
        );
        out
    }
}

struct Schrift {
    data: std::sync::Arc<egui::FontData>,
    glyphs: BTreeMap<u16, char>,
}
struct Ausgabe {
    s: String,
    fonts: Vec<Schrift>,
    namen: Vec<String>,
    defs: egui::FontDefinitions,
    alpha: BTreeSet<(bool, u8)>,
}
impl Ausgabe {
    fn farbe(&mut self, c: Color32, stroke: bool) {
        let [r, g, b, a] = c.to_srgba_unmultiplied();
        self.alpha.insert((stroke, a));
        let _ = writeln!(self.s, "/A{}{a} gs", if stroke { "S" } else { "F" });
        let f = |v: u8| f32::from(v) / 255.0;
        let _ = writeln!(
            self.s,
            "{:.5} {:.5} {:.5} {}",
            f(r),
            f(g),
            f(b),
            if stroke { "RG" } else { "rg" }
        );
    }
    fn ende(&mut self, fill: Color32, stroke: Stroke) {
        let fu = fill.a() > 0;
        let st = stroke.width > 0.0 && stroke.color.a() > 0;
        if fu {
            self.farbe(fill, false)
        }
        if st {
            self.farbe(stroke.color, true);
            let _ = writeln!(self.s, "{:.5} w", stroke.width);
        }
        self.s.push_str(match (fu, st) {
            (true, true) => "B\n",
            (true, false) => "f\n",
            (false, true) => "S\n",
            _ => "n\n",
        });
    }
    fn path(&mut self, pts: &[Pos2], closed: bool, fill: Color32, stroke: Stroke) {
        if pts.is_empty() || pts.iter().any(|p| !p.x.is_finite() || !p.y.is_finite()) {
            return;
        }
        let _ = writeln!(self.s, "{:.5} {:.5} m", pts[0].x, pts[0].y);
        for p in &pts[1..] {
            let _ = writeln!(self.s, "{:.5} {:.5} l", p.x, p.y);
        }
        if closed {
            self.s.push_str("h\n")
        }
        self.ende(fill, stroke);
    }
    fn ellipse(&mut self, c: Pos2, r: Vec2, fill: Color32, stroke: Stroke) {
        if !c.x.is_finite()
            || !c.y.is_finite()
            || !r.x.is_finite()
            || !r.y.is_finite()
            || r.x <= 0.0
            || r.y <= 0.0
        {
            return;
        }
        let k = 0.55228475;
        let _ = writeln!(self.s, "{} {} m", c.x + r.x, c.y);
        for (p1, p2, p3) in [
            (
                Vec2::new(r.x, k * r.y),
                Vec2::new(k * r.x, r.y),
                Vec2::new(0.0, r.y),
            ),
            (
                Vec2::new(-k * r.x, r.y),
                Vec2::new(-r.x, k * r.y),
                Vec2::new(-r.x, 0.0),
            ),
            (
                Vec2::new(-r.x, -k * r.y),
                Vec2::new(-k * r.x, -r.y),
                Vec2::new(0.0, -r.y),
            ),
            (
                Vec2::new(k * r.x, -r.y),
                Vec2::new(r.x, -k * r.y),
                Vec2::new(r.x, 0.0),
            ),
        ] {
            let _ = writeln!(
                self.s,
                "{} {} {} {} {} {} c",
                c.x + p1.x,
                c.y + p1.y,
                c.x + p2.x,
                c.y + p2.y,
                c.x + p3.x,
                c.y + p3.y
            );
        }
        self.s.push_str("h\n");
        self.ende(fill, stroke);
    }
    fn shape(&mut self, shape: &Shape, schrift_pt: Option<f32>) -> Result<(), String> {
        match shape {
            Shape::Noop => {}
            Shape::Vec(v) => {
                for s in v {
                    self.shape(s, schrift_pt)?
                }
            }
            Shape::LineSegment { points, stroke } => {
                self.path(points, false, Color32::TRANSPARENT, *stroke)
            }
            Shape::Path(p) => {
                let color = match p.stroke.color {
                    egui::epaint::ColorMode::Solid(c) => c,
                    _ => return Err("Nicht unterstützte Linienfarbe".into()),
                };
                self.path(
                    &p.points,
                    p.closed,
                    p.fill,
                    Stroke::new(p.stroke.width, color),
                );
            }
            Shape::Circle(c) => self.ellipse(c.center, Vec2::splat(c.radius), c.fill, c.stroke),
            Shape::Ellipse(e) => self.ellipse(e.center, e.radius, e.fill, e.stroke),
            Shape::Rect(r) => self.path(
                &[
                    r.rect.left_top(),
                    r.rect.right_top(),
                    r.rect.right_bottom(),
                    r.rect.left_bottom(),
                ],
                true,
                r.fill,
                r.stroke,
            ),
            Shape::Text(t) => {
                self.farbe(t.override_text_color.unwrap_or(t.fallback_color), false);
                for row in &t.galley.rows {
                    for glyph in &row.glyphs {
                        if glyph.chr.is_control() {
                            continue;
                        }
                        let family = &t.galley.job.sections[0].format.font_id.family;
                        let mut found = None;
                        for name in self.defs.families.get(family).into_iter().flatten() {
                            let data = &self.defs.font_data[name];
                            if let Ok(face) = ttf_parser::Face::parse(data.bytes(), data.index) {
                                if let Some(gid) = face.glyph_index(glyph.chr) {
                                    found = Some((name.clone(), data.clone(), gid.0));
                                    break;
                                }
                            }
                        }
                        let Some((name, data, gid)) = found else {
                            let hinweis = if glyph.chr == '\u{fffd}' {
                                "\n\nDies ist ein Ersatzzeichen für nicht korrekt gelesenen Text. Bitte die DXF-Datei neu laden und die Dateikodierung prüfen."
                            } else {
                                ""
                            };
                            return Err(format!(
                                "Die PDF-Schriften enthalten das Zeichen {:?} (U+{:04X}) nicht.\n\nBetroffene Beschriftung:\n{}{}",
                                glyph.chr,
                                glyph.chr as u32,
                                t.galley.job.text.chars().take(240).collect::<String>(),
                                hinweis
                            ));
                        };
                        let id = if let Some(i) = self.namen.iter().position(|n| n == &name) {
                            i
                        } else {
                            self.namen.push(name);
                            self.fonts.push(Schrift {
                                data: data.clone(),
                                glyphs: BTreeMap::new(),
                            });
                            self.fonts.len() - 1
                        };
                        self.fonts[id].glyphs.insert(gid, glyph.chr);
                        let face = ttf_parser::Face::parse(data.bytes(), data.index)
                            .map_err(|e| format!("Schrift: {e:?}"))?;
                        let em = schrift_pt.map_or_else(
                            || {
                                glyph.font_face_height * f32::from(face.units_per_em())
                                    / f32::from(face.ascender() - face.descender())
                            },
                            |pt| pt / 0.75,
                        );
                        let x = t.pos.x + row.pos.x + glyph.pos.x;
                        let y = t.pos.y + row.pos.y + glyph.pos.y;
                        let _ = writeln!(
                            self.s,
                            "BT /F{} {:.5} Tf 1 0 0 -1 {:.5} {:.5} Tm <{:04X}> Tj ET",
                            id, em, x, y, gid
                        );
                    }
                }
            }
            _ => {
                return Err(
                    "Die Zeichnung enthält eine nicht unterstützte PDF-Zeichenform.".into(),
                );
            }
        }
        Ok(())
    }
}

pub fn pdf(szene: &Szene) -> Result<Vec<u8>, String> {
    let mut a = Ausgabe {
        s: String::new(),
        fonts: Vec::new(),
        namen: Vec::new(),
        defs: egui::FontDefinitions::default(),
        alpha: BTreeSet::new(),
    };
    let h = szene.blatt.seite.height();
    let w = szene.blatt.seite.width();
    // Logische 96-dpi-Einheiten -> PDF-Punkte, Y nach unten.
    let _ = writeln!(a.s, "0.75 0 0 -0.75 0 {} cm\n1 J 1 j", h * 0.75);
    for form in &szene.formen {
        let t = form.transform;
        let r = form.clip;
        if !r.is_positive() {
            continue;
        }
        let _ = writeln!(
            a.s,
            "q\n{} {} {} {} re W n\n{} 0 0 {} {} {} cm",
            r.left(),
            r.top(),
            r.width(),
            r.height(),
            t.scaling,
            t.scaling,
            t.translation.x,
            t.translation.y
        );
        let mut shape = form.shape.clone();
        if form.karteninhalt {
            mindestbreite(
                &mut shape,
                szene.blatt.parameter.linie_min_mm as f32 * PX_MM / t.scaling,
            );
        }
        a.shape(&shape, form.schrift_pt.map(|pt| pt / t.scaling))?;
        a.s.push_str("Q\n");
    }
    let mut p = Pdf { obj: Vec::new() };
    p.add(b"<< /Type /Catalog /Pages 2 0 R >>".to_vec());
    p.add(b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec());
    p.add(Vec::new());
    let content = p.stream("", a.s.as_bytes());
    let mut resources = String::new();
    for (i, font) in a.fonts.iter().enumerate() {
        let face = ttf_parser::Face::parse(font.data.bytes(), font.data.index)
            .map_err(|e| format!("Schrift: {e:?}"))?;
        let file = p.stream(
            &format!("/Length1 {}", font.data.bytes().len()),
            font.data.bytes(),
        );
        let b = face.global_bounding_box();
        let unit = 1000.0 / f32::from(face.units_per_em());
        let descriptor=p.add(format!("<< /Type /FontDescriptor /FontName /Feldbuch{i} /Flags 32 /FontBBox [{} {} {} {}] /ItalicAngle 0 /Ascent {} /Descent {} /CapHeight {} /StemV 80 /FontFile2 {file} 0 R >>",b.x_min as f32*unit,b.y_min as f32*unit,b.x_max as f32*unit,b.y_max as f32*unit,face.ascender() as f32*unit,face.descender() as f32*unit,face.ascender() as f32*unit).into_bytes());
        let widths = font
            .glyphs
            .keys()
            .map(|g| {
                format!(
                    "{} [{}]",
                    g,
                    face.glyph_hor_advance(ttf_parser::GlyphId(*g)).unwrap_or(0) as f32 * unit
                )
            })
            .collect::<Vec<_>>()
            .join(" ");
        let cid=p.add(format!("<< /Type /Font /Subtype /CIDFontType2 /BaseFont /Feldbuch{i} /CIDSystemInfo << /Registry (Adobe) /Ordering (Identity) /Supplement 0 >> /FontDescriptor {descriptor} 0 R /CIDToGIDMap /Identity /W [{widths}] >>").into_bytes());
        let mut cmap = String::from(
            "/CIDInit /ProcSet findresource begin 12 dict begin begincmap /CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def /CMapName /Feldbuch def /CMapType 2 def 1 begincodespacerange <0000> <FFFF> endcodespacerange\n",
        );
        let entries: Vec<_> = font.glyphs.iter().collect();
        for chunk in entries.chunks(100) {
            let _ = writeln!(cmap, "{} beginbfchar", chunk.len());
            for (gid, c) in chunk {
                let utf = c
                    .encode_utf16(&mut [0; 2])
                    .iter()
                    .map(|n| format!("{n:04X}"))
                    .collect::<String>();
                let _ = writeln!(cmap, "<{gid:04X}> <{utf}>");
            }
            cmap.push_str("endbfchar\n");
        }
        cmap.push_str("endcmap CMapName currentdict /CMap defineresource pop end end");
        let unicode = p.stream("", cmap.as_bytes());
        let root=p.add(format!("<< /Type /Font /Subtype /Type0 /BaseFont /Feldbuch{i} /Encoding /Identity-H /DescendantFonts [{cid} 0 R] /ToUnicode {unicode} 0 R >>").into_bytes());
        let _ = write!(resources, "/F{i} {root} 0 R ");
    }
    let mut alpha = String::new();
    for (stroke, value) in a.alpha {
        let id = p.add(
            format!(
                "<< /Type /ExtGState /{} {:.6} >>",
                if stroke { "CA" } else { "ca" },
                f32::from(value) / 255.0
            )
            .into_bytes(),
        );
        let _ = write!(
            alpha,
            "/A{}{value} {id} 0 R ",
            if stroke { "S" } else { "F" }
        );
    }
    p.obj[2]=format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {} {}] /Resources << /Font << {} >> /ExtGState << {} >> >> /Contents {} 0 R >>",w*0.75,h*0.75,resources,alpha,content).into_bytes();
    Ok(p.finish())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn schriftfehler_nennt_zeichen_beschriftung_und_kodierungshinweis() {
        let e = DxfEntity {
            layer: "0".into(),
            farbe: None,
            art: Art::Text {
                x: 0.0,
                y: 0.0,
                hoehe: 2.0,
                text: "Stra\u{fffd}e".into(),
            },
        };
        let szene = erzeuge(
            &egui::Context::default(),
            &PdfParameter::default(),
            [0.0, 0.0],
            &[(&e, Color32::BLACK)],
            "",
            None,
        );
        let fehler = pdf(&szene).unwrap_err();
        assert!(fehler.contains("U+FFFD"));
        assert!(fehler.contains("Stra\u{fffd}e"));
        assert!(fehler.contains("Dateikodierung"));
    }
    #[test]
    fn massstab_eins_begrenzt_fontlayout_ohne_die_papiergroesse_zu_aendern() {
        let e = DxfEntity {
            layer: "0".into(),
            farbe: None,
            art: Art::Text {
                x: 0.0,
                y: 0.0,
                hoehe: 20.0,
                text: "Grosser Text".into(),
            },
        };
        let p = PdfParameter {
            massstab: 1.0,
            ..Default::default()
        };
        let szene = erzeuge(
            &egui::Context::default(),
            &p,
            [0.0, 0.0],
            &[(&e, Color32::BLACK)],
            "",
            None,
        );
        let f = szene
            .formen
            .iter()
            .find(|f| matches!(&f.shape,Shape::Text(t) if t.galley.job.text=="Grosser Text"))
            .unwrap();
        let Shape::Text(t) = &f.shape else {
            unreachable!()
        };
        let layout = t.galley.job.sections[0].format.font_id.size;
        assert!(layout <= 128.001);
        assert!(
            (layout * f.transform.scaling - 20.0 * szene.blatt.ansicht([0.0, 0.0]).scale as f32)
                .abs()
                < 0.1
        );
        let fern = DxfEntity {
            layer: "0".into(),
            farbe: None,
            art: Art::Text {
                x: 100000.0,
                y: 100000.0,
                hoehe: 20.0,
                text: "fern".into(),
            },
        };
        assert!(!im_ausschnitt(
            &fern,
            &szene.blatt.ansicht([0.0, 0.0]),
            szene.blatt.karte,
            1.0
        ));
        let linie = DxfEntity {
            layer: "0".into(),
            farbe: None,
            art: Art::Line {
                x1: -100.0,
                y1: 0.0,
                x2: 100.0,
                y2: 0.0,
            },
        };
        assert!(im_ausschnitt(
            &linie,
            &szene.blatt.ansicht([0.0, 0.0]),
            szene.blatt.karte,
            1.0
        ));
    }

    #[test]
    fn a4_massstab_und_leiste() {
        for querformat in [false, true] {
            let b = Blatt::neu(&PdfParameter {
                querformat,
                ..Default::default()
            });
            let g = b.welt_groesse();
            assert!((g.x - if querformat { 267.0 } else { 180.0 }).abs() < 0.001);
            assert!((b.karte.left() / PX_MM - 20.0).abs() < 0.001);
            assert!((b.karte.top() / PX_MM - 10.0).abs() < 0.001);
            assert!(((b.seite.right() - b.karte.right()) / PX_MM - 10.0).abs() < 0.001);
            assert!((g.y - if querformat { 175.0 } else { 262.0 }).abs() < 0.001);
            let (m, px) = b.balken();
            assert!((f64::from(px / PX_MM) - m).abs() < 0.001);
        }
    }
    #[test]
    fn vektor_pdf_mit_unicode_und_grosser_koordinate() {
        let ctx = egui::Context::default();
        let e = DxfEntity {
            layer: "0".into(),
            farbe: None,
            art: Art::Text {
                x: 4_500_000.0,
                y: 5_500_000.0,
                hoehe: 3.0,
                text: "Höhe ÄÖÜ ß".into(),
            },
        };
        let szene = erzeuge(
            &ctx,
            &PdfParameter::default(),
            [4_500_000.0, 5_500_000.0],
            &[(&e, Color32::BLACK)],
            "Prüfung",
            None,
        );
        let bytes = pdf(&szene).unwrap();
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.contains("/FontFile2"));
        assert!(text.contains("/ToUnicode"));
        assert!(!text.contains("/Subtype /Image"));
        assert!(text.contains("re W n"));
        if let Ok(pfad) = std::env::var("FELDBUCH_PDF_PRUEFDATEI") {
            std::fs::write(pfad, bytes).unwrap();
        }
    }

    #[test]
    fn papiermasse_und_alle_zeichenformen() {
        let base = [4_500_000.0, 5_500_000.0];
        let mut entities = vec![
            DxfEntity {
                layer: "0".into(),
                farbe: None,
                art: Art::Line {
                    x1: base[0] - 50.0,
                    y1: base[1],
                    x2: base[0] + 50.0,
                    y2: base[1],
                },
            },
            DxfEntity {
                layer: "0".into(),
                farbe: None,
                art: Art::Circle {
                    cx: base[0],
                    cy: base[1],
                    radius: 25.0,
                },
            },
            DxfEntity {
                layer: "0".into(),
                farbe: None,
                art: Art::Arc {
                    cx: base[0],
                    cy: base[1],
                    radius: 40.0,
                    start: 20.0,
                    ende: 160.0,
                },
            },
            DxfEntity {
                layer: "0".into(),
                farbe: None,
                art: Art::LwPolyline {
                    punkte: vec![
                        (base[0] - 60.0, base[1] - 30.0),
                        (base[0], base[1] - 50.0),
                        (base[0] + 60.0, base[1] - 30.0),
                    ],
                    geschlossen: true,
                },
            },
            DxfEntity {
                layer: "0".into(),
                farbe: None,
                art: Art::Text {
                    x: base[0] - 40.0,
                    y: base[1] + 65.0,
                    hoehe: 4.0,
                    text: "Vektorplan: Höhe ÄÖÜ ß · 100 m".into(),
                },
            },
            DxfEntity {
                layer: "0".into(),
                farbe: None,
                art: Art::Ellipse {
                    nr: "E1".into(),
                    r: base[0],
                    h: base[1],
                    v_strecke_mm: 20.0,
                    v_quer_mm: 10.0,
                    phi: 0.3,
                    skalierung: 1000.0,
                    konfidenz: false,
                },
            },
        ];
        for (i, layer) in [
            "Grenzpunkt_Genau",
            "Grenzpunkt_Sonstiger",
            "Katasterfestpunkt",
            "0",
        ]
        .iter()
        .enumerate()
        {
            entities.push(DxfEntity {
                layer: layer.to_string(),
                farbe: None,
                art: Art::Insert {
                    block: "P".into(),
                    x: base[0] - 45.0 + i as f64 * 30.0,
                    y: base[1] + 40.0,
                },
            });
        }
        entities.push(DxfEntity {
            layer: "Import".into(),
            farbe: None,
            art: Art::Import {
                nr: "Punkt 123".into(),
                wx: base[0] + 40.0,
                wy: base[1],
                hoehe: 123.456,
            },
        });
        for querformat in [false, true] {
            let p = PdfParameter {
                querformat,
                ..Default::default()
            };
            let ctx = egui::Context::default();
            let refs = entities
                .iter()
                .map(|e| (e, Color32::from_rgb(40, 40, 100)))
                .collect::<Vec<_>>();
            let scene = erzeuge(
                &ctx,
                &p,
                base,
                &refs,
                "Layoutprüfung · Feldbuch",
                Some(1000.0),
            );
            let first = scene
                .formen
                .iter()
                .find_map(|f| {
                    if let Shape::LineSegment { points, .. } = &f.shape {
                        Some(points)
                    } else {
                        None
                    }
                })
                .unwrap();
            assert!(
                ((first[1].x - first[0].x) / PX_MM - 100.0).abs() < 0.001,
                "100 m müssen bei 1:1000 exakt 100 mm sein"
            );
            let min = p.linie_min_mm as f32 * PX_MM;
            let mut line = Shape::line_segment(
                [Pos2::ZERO, Pos2::new(10.0, 0.0)],
                Stroke::new(0.1, Color32::BLACK),
            );
            mindestbreite(&mut line, min);
            if let Shape::LineSegment { stroke, .. } = line {
                assert!((stroke.width / PX_MM - 0.25).abs() < 0.0001);
            }
            let import = entities.last().unwrap();
            let span = symbol_spanne(import).unwrap();
            let factor = (p.symbol_min_mm as f32 * PX_MM / span).max(1.0);
            assert!(span * factor / PX_MM >= 2.0 - 0.0001);
            let bytes = pdf(&scene).unwrap();
            if let Ok(dir) = std::env::var("FELDBUCH_PDF_LAYOUT") {
                std::fs::create_dir_all(&dir).unwrap();
                std::fs::write(
                    std::path::Path::new(&dir).join(if querformat {
                        "layout-quer.pdf"
                    } else {
                        "layout-hoch.pdf"
                    }),
                    bytes,
                )
                .unwrap();
            }
        }
    }

    #[test]
    fn mindestsymbol_wird_um_seinen_eigenen_punkt_vergroessert() {
        let ctx = egui::Context::default();
        let e = DxfEntity {
            layer: "0".into(),
            farbe: None,
            art: Art::Insert {
                block: "P".into(),
                x: 45.0,
                y: 40.0,
            },
        };
        let p = PdfParameter::default();
        let scene = erzeuge(&ctx, &p, [0.0, 0.0], &[(&e, Color32::BLACK)], "", None);
        let f = scene
            .formen
            .iter()
            .find(|f| matches!(f.shape, Shape::Path(_)))
            .unwrap();
        let original = scene.blatt.ansicht([0.0, 0.0]).zu_bildschirm(45.0, 40.0);
        let transformed = f.transform * original;
        assert!(transformed.distance(original) < 0.001);
        let Shape::Path(path) = &f.shape else {
            unreachable!()
        };
        let min = path
            .points
            .iter()
            .map(|p| p.x)
            .fold(f32::INFINITY, f32::min);
        let max = path
            .points
            .iter()
            .map(|p| p.x)
            .fold(f32::NEG_INFINITY, f32::max);
        assert!(((max - min) * f.transform.scaling / PX_MM - 2.0).abs() < 0.001);
    }
}
