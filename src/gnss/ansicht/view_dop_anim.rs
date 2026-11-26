//! Zweck:    Animierte Darstellung des GDOP: Zeigerinstrument und drehende Satellitengeometrie.
//! Hardware: simpleRTK2B (u-blox ZED-F9P)
//! Autor:    Johann Gerner
//!
//! Übernommen aus gnss-vermessung 0.4.0 (src/bin/gnss-gui/view_dop_anim.rs), eigenständig gepflegt in Feldbuch-NT.

use crate::gnss::ansicht::widgets::*;
use eframe::egui::{self, Align2, Color32, FontId, Pos2, Sense, Shape, Stroke, Ui, vec2};
use crate::gnss::kern::dop::{self, Direction};
use crate::gnss::kern::model::State;
use crate::gnss::kern::settings::Settings;
use std::f32::consts::{PI, TAU};
use std::time::Instant;

/// Endwert der Skala; größere Werte stehen am Anschlag.
const SCALE_MAX: f32 = 8.0;
/// Zonen der Skala: obere Grenze und Farbe
const ZONES: [(f32, Color32); 4] = [
    (2.0, GOOD),
    (4.0, Color32::from_rgb(130, 180, 50)),
    (6.0, WARN),
    (SCALE_MAX, BAD),
];
/// Dauer der Zeigerbewegung zum neuen Wert [s]
const NEEDLE_TIME: f32 = 0.7;
const ARC_STEPS: usize = 60;
/// Blickneigung auf die Himmelskuppel
const TILT_DEG: f32 = 22.0;
/// Höhe der Kuppel im Verhältnis zu ihrer Breite
const DOME_ASPECT: f32 = 0.68;
/// Umlaufzeit eines Lichtpunkts vom Satelliten zum Empfänger [s]
const PULSE_TIME: f64 = 1.6;
const DRAG_RAD_PER_PX: f32 = 0.01;
/// freier Rand über und unter der Kuppel für Beschriftungen
const DOME_MARGIN: f32 = 16.0;
/// Breite der Spalte mit den Pyramidenknöpfen
const BUTTON_COLUMN: f32 = 190.0;
const MAX_COLOR: Color32 = Color32::from_rgb(60, 190, 80);
const CURRENT_COLOR: Color32 = Color32::from_rgb(220, 40, 40);
/// Deckkraft der Pyramidenflächen (0 = unsichtbar, 255 = deckend)
const WALL_ALPHA: u8 = 55;
const GROUND_COLOR: Color32 = Color32::from_rgb(70, 125, 70);
const GROUND_ALPHA: u8 = 90;
const DASH: f32 = 7.0;
const GAP: f32 = 4.0;

/// Beste Vierergruppe der verwendeten Satelliten; neu nur bei neuen Satellitendaten.
struct Current {
    key: (Option<Instant>, u64),
    best: Option<Best>,
}

struct Best {
    dirs: [Direction; 4],
    names: [String; 4],
}

pub struct DopAnim {
    angle: f32,
    spinning: bool,
    /// Drehgeschwindigkeit [°/s]
    speed: f32,
    computed: bool,
    show_max: bool,
    show_current: bool,
    current: Option<Current>,
    /// Elevation der maximalen Pyramide [°]
    ring_elev: f64,
}

impl Default for DopAnim {
    fn default() -> Self {
        DopAnim {
            angle: 0.4,
            spinning: true,
            speed: 12.0,
            computed: false,
            show_max: false,
            show_current: false,
            current: None,
            ring_elev: dop::best_ring_elevation_deg(),
        }
    }
}

fn zone_color(v: f32) -> Color32 {
    ZONES.iter().find(|z| v < z.0).map_or(BAD, |z| z.1)
}

/// Punkt auf dem Halbkreis: links Wert 0, rechts Skalenende.
fn on_arc(center: Pos2, radius: f32, value: f32) -> Pos2 {
    let a = PI * (1.0 - (value / SCALE_MAX).clamp(0.0, 1.0));
    center + vec2(a.cos(), -a.sin()) * radius
}

fn gauge(ui: &mut Ui, value: Option<f64>, other: Option<f64>, stale: bool, width: f32) {
    let (resp, painter) = ui.allocate_painter(vec2(width, width * 0.68), Sense::hover());
    let center = Pos2::new(resp.rect.center().x, resp.rect.top() + width * 0.48);
    let radius = width * 0.40;
    let text = ui.visuals().text_color();
    let weak = ui.visuals().weak_text_color();

    let mut from = 0.0;
    for (to, color) in ZONES {
        let points = (0..=ARC_STEPS / 4)
            .map(|i| {
                on_arc(
                    center,
                    radius,
                    from + (to - from) * i as f32 / (ARC_STEPS / 4) as f32,
                )
            })
            .collect();
        painter.add(Shape::line(
            points,
            Stroke::new(
                width * 0.055,
                color.gamma_multiply(if stale { 0.25 } else { 0.85 }),
            ),
        ));
        from = to;
    }
    for tick in 0..=SCALE_MAX as u8 {
        let t = f32::from(tick);
        painter.line_segment(
            [
                on_arc(center, radius * 0.88, t),
                on_arc(center, radius * 0.92, t),
            ],
            Stroke::new(1.5, weak),
        );
        painter.text(
            on_arc(center, radius * 1.17, t),
            Align2::CENTER_CENTER,
            tick.to_string(),
            FontId::proportional(13.0),
            weak,
        );
    }

    // Zeiger gleitet zum neuen Wert
    let target = value.map_or(0.0, |v| v as f32).min(SCALE_MAX);
    let shown =
        ui.ctx()
            .animate_value_with_time(egui::Id::unique("gdop_needle"), target, NEEDLE_TIME);
    if let Some(v) = other {
        let v = v as f32;
        painter.circle_stroke(
            on_arc(center, radius, v),
            width * 0.022,
            Stroke::new(2.0, text),
        );
    }
    if let Some(v) = value {
        let color = if stale { STALE } else { zone_color(shown) };
        let tip = on_arc(center, radius * 0.86, shown);
        let side = (tip - center).normalized().rot90() * width * 0.018;
        painter.add(Shape::convex_polygon(
            vec![center + side, tip, center - side],
            color,
            Stroke::NONE,
        ));
        painter.circle_filled(center, width * 0.03, color);
        let number = if v > f64::from(SCALE_MAX) {
            format!("{v:.1}")
        } else {
            format!("{v:.2}")
        };
        painter.text(
            center + vec2(0.0, width * 0.05),
            Align2::CENTER_TOP,
            number,
            FontId::monospace(width * 0.11),
            color,
        );
        let rating = if stale { "veraltet" } else { dop::rating(v) };
        painter.text(
            Pos2::new(center.x, resp.rect.bottom()),
            Align2::CENTER_BOTTOM,
            rating,
            FontId::proportional(16.0),
            color,
        );
    } else {
        painter.text(
            center + vec2(0.0, width * 0.07),
            Align2::CENTER_TOP,
            "nicht berechenbar",
            FontId::proportional(16.0),
            weak,
        );
    }
    painter.text(
        resp.rect.left_top(),
        Align2::LEFT_TOP,
        "GDOP",
        FontId::proportional(20.0),
        text,
    );
}

/// Projektion eines Punkts der Einheitskugel (Ost, Nord, Hoch): Bildschirmversatz und Tiefe.
fn project(e: f32, n: f32, u: f32, angle: f32) -> (egui::Vec2, f32) {
    let (s, c) = angle.sin_cos();
    let (ts, tc) = TILT_DEG.to_radians().sin_cos();
    let right = e * c - n * s;
    let away = e * s + n * c;
    (vec2(right, -(u * tc + away * ts)), away * tc - u * ts)
}

fn direction(azim_deg: f64, elev_deg: f64) -> (f32, f32, f32) {
    let (az, el) = (
        (azim_deg as f32).to_radians(),
        (elev_deg as f32).to_radians(),
    );
    (el.cos() * az.sin(), el.cos() * az.cos(), el.sin())
}

/// Kleines Satellitensymbol: Körper mit zwei Solarflächen quer zur Blickrichtung auf den Empfänger.
fn satellite_icon(
    painter: &egui::Painter,
    at: Pos2,
    towards: egui::Vec2,
    scale: f32,
    color: Color32,
) {
    let down = if towards.length() > 1.0 {
        towards.normalized()
    } else {
        vec2(0.0, 1.0)
    };
    let side = down.rot90();
    for direction in [-1.0, 1.0] {
        let middle = at + side * (direction * 8.5 * scale);
        let (w, h) = (side * (4.5 * scale), down * (3.0 * scale));
        painter.add(Shape::convex_polygon(
            vec![
                middle - w - h,
                middle + w - h,
                middle + w + h,
                middle - w + h,
            ],
            color.gamma_multiply(0.75),
            Stroke::new(0.8, color),
        ));
    }
    painter.line_segment(
        [at - side * (5.0 * scale), at + side * (5.0 * scale)],
        Stroke::new(1.0, color),
    );
    painter.circle_filled(at, 3.6 * scale, color);
    // Antenne in Richtung Empfänger
    painter.line_segment([at, at + down * (7.0 * scale)], Stroke::new(1.2, color));
}

fn dome(ui: &mut Ui, st: &State, view: &mut DopAnim, mask: f64, width: f32) {
    let height = width * DOME_ASPECT;
    let (resp, painter) = ui.allocate_painter(vec2(width, height), Sense::drag());
    if resp.dragged() {
        view.angle += resp.drag_delta().x * DRAG_RAD_PER_PX;
    } else if view.spinning {
        view.angle += view.speed.to_radians() * ui.input(|i| i.stable_dt);
    }
    view.angle = view.angle.rem_euclid(TAU);
    // Platzbedarf: nach oben ein Radius, nach unten der geneigte Horizont, dazu Beschriftung
    let below = 1.1 * TILT_DEG.to_radians().sin();
    let radius = ((height - 2.0 * DOME_MARGIN) / (1.0 + below)).min(width / 2.2 - 8.0);
    let center = Pos2::new(resp.rect.center().x, resp.rect.top() + DOME_MARGIN + radius);
    let weak = ui.visuals().weak_text_color();
    let at = |d: (f32, f32, f32)| center + project(d.0, d.1, d.2, view.angle).0 * radius;

    // Bodenscheibe in der Horizontebene
    let ground = (0..ARC_STEPS)
        .map(|i| at(direction(i as f64 * 360.0 / ARC_STEPS as f64, 0.0)))
        .collect();
    let fill = Color32::from_rgba_unmultiplied(
        GROUND_COLOR.r(),
        GROUND_COLOR.g(),
        GROUND_COLOR.b(),
        GROUND_ALPHA,
    );
    painter.add(Shape::convex_polygon(ground, fill, Stroke::NONE));

    // Horizont, Höhenkreise und Meridiane
    for elev in [0.0, 30.0, 60.0] {
        let points = (0..=ARC_STEPS)
            .map(|i| at(direction(i as f64 * 360.0 / ARC_STEPS as f64, elev)))
            .collect();
        painter.add(Shape::line(
            points,
            Stroke::new(if elev == 0.0 { 1.5 } else { 0.5 }, weak),
        ));
    }
    for azim in [0.0, 90.0, 180.0, 270.0] {
        let points = (0..=ARC_STEPS / 4)
            .map(|i| at(direction(azim, i as f64 * 90.0 / (ARC_STEPS / 4) as f64)))
            .collect();
        painter.add(Shape::line(points, Stroke::new(0.5, weak)));
    }
    for (azim, name) in [(0.0, "N"), (90.0, "O"), (180.0, "S"), (270.0, "W")] {
        let d = direction(azim, 0.0);
        painter.text(
            at((d.0 * 1.1, d.1 * 1.1, 0.0)),
            Align2::CENTER_CENTER,
            name,
            FontId::proportional(13.0),
            ui.visuals().text_color(),
        );
    }

    // Pyramiden: Spitze beim Empfänger, vier Satelliten als Grundfläche (nach Azimut geordnet)
    let maximum = view.show_max.then(|| (dop::ring_pyramid(view.ring_elev.max(mask)), MAX_COLOR));
    let current = view
        .current
        .as_ref()
        .and_then(|c| c.best.as_ref())
        .filter(|_| view.show_current)
        .map(|b| (b.dirs, CURRENT_COLOR));
    let mut faces = Vec::new();
    let mut rims = Vec::new();
    for (four, color) in maximum.into_iter().chain(current) {
        let corners: Vec<(Pos2, f32)> = dop::by_azimuth(&four)
            .iter()
            .map(|d| {
                let (e, n, u) = direction(d.azim_deg, d.elev_deg);
                let (offset, depth) = project(e, n, u, view.angle);
                (center + offset * radius, depth)
            })
            .collect();
        let fill = Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), WALL_ALPHA);
        for i in 0..4 {
            let (a, b) = (corners[i], corners[(i + 1) % 4]);
            faces.push(((a.1 + b.1) / 3.0, [center, a.0, b.0], fill));
        }
        for (a, b, d) in [(0, 1, 2), (0, 2, 3)] {
            let depth = (corners[a].1 + corners[b].1 + corners[d].1) / 3.0;
            faces.push((depth, [corners[a].0, corners[b].0, corners[d].0], fill));
        }
        rims.push((corners.into_iter().map(|p| p.0).collect::<Vec<_>>(), color));
    }
    faces.sort_by(|a, b| b.0.total_cmp(&a.0));
    for (_, points, fill) in faces {
        painter.add(Shape::convex_polygon(points.to_vec(), fill, Stroke::NONE));
    }
    // Kanten zum Empfänger durchgezogen, Rand der Grundfläche gestrichelt
    for (mut rim, color) in rims {
        for &corner in &rim {
            painter.line_segment([center, corner], Stroke::new(2.0, color));
        }
        if let Some(&first) = rim.first() {
            rim.push(first);
            painter.extend(Shape::dashed_line(&rim, Stroke::new(2.0, color), DASH, GAP));
        }
    }

    // verwendete Satelliten, hintere zuerst
    let mut sats: Vec<_> = st
        .sats
        .iter()
        .filter(|s| s.used)
        .filter_map(|s| {
            let elev = s.elev.filter(|e| *e >= mask)?;
            let d = direction(s.azim, elev);
            Some((project(d.0, d.1, d.2, view.angle), s.gnss, s.sv))
        })
        .collect();
    sats.sort_by(|a, b| b.0.1.total_cmp(&a.0.1));
    let time = ui.input(|i| i.time);
    for (i, ((offset, depth), gnss, sv)) in sats.iter().enumerate() {
        // weiter entfernte Satelliten blasser und kleiner
        let near = ((1.0 - depth) / 2.0).clamp(0.0, 1.0);
        let color = system(*gnss).2.gamma_multiply(0.45 + 0.55 * near);
        let p = center + *offset * radius;
        painter.line_segment([center, p], Stroke::new(1.0, color.gamma_multiply(0.5)));
        let phase = ((time / PULSE_TIME + i as f64 * 0.137).fract()) as f32;
        painter.circle_filled(p + (center - p) * phase, 2.5, color);
        satellite_icon(&painter, p, center - p, 0.55 + 0.4 * near, color);
        painter.text(
            p + vec2(0.0, -11.0),
            Align2::CENTER_BOTTOM,
            sat_name(*gnss, *sv),
            FontId::proportional(10.0),
            color,
        );
    }
    painter.circle_filled(center, 6.0, ui.visuals().text_color());
    if sats.is_empty() {
        painter.text(
            resp.rect.center_top(),
            Align2::CENTER_TOP,
            "keine verwendeten Satelliten",
            FontId::proportional(13.0),
            weak,
        );
    }
    if view.spinning {
        ui.ctx().request_repaint();
    }
}

/// Sucht die vier verwendeten Satelliten mit den besten Schnittwinkeln, wenn sich Daten oder Maske ändern.
fn update_current(st: &State, view: &mut DopAnim, mask: f64) {
    let key = (st.sats_at, mask.to_bits());
    if view.current.as_ref().is_some_and(|c| c.key == key) {
        return;
    }
    let used: Vec<(Direction, String)> = st
        .sats
        .iter()
        .filter(|s| s.used)
        .filter_map(|s| {
            let elev = s.elev.filter(|e| *e >= mask)?;
            Some((Direction { azim_deg: s.azim, elev_deg: elev, system: dop::clock_group(s.gnss) }, sat_name(s.gnss, s.sv)))
        })
        .collect();
    let dirs: Vec<Direction> = used.iter().map(|u| u.0).collect();
    let best = dop::best_intersection(&dirs).map(|idx| Best { dirs: idx.map(|i| dirs[i]), names: idx.map(|i| used[i].1.clone()) });
    view.current = Some(Current { key, best });
}

fn pyramid_buttons(ui: &mut Ui, view: &mut DopAnim, mask: f64) {
    ui.vertical(|ui| {
        ui.set_width(BUTTON_COLUMN);
        ui.add_space(20.0);
        let button = |ui: &mut Ui, on: &mut bool, label: &str, color: Color32| {
            let mark = if *on { "■" } else { "□" };
            let text = egui::RichText::new(format!("{mark} {label}")).color(color).strong().size(15.0);
            ui.add_sized([BUTTON_COLUMN, 34.0], |ui: &mut Ui| ui.toggle_value(on, text));
        };
        let elev = view.ring_elev.max(mask);
        let ideal = dop::ring_pyramid(elev);
        let max_volume = dop::volume(&ideal);
        button(ui, &mut view.show_max, "Maximale Pyramide", MAX_COLOR);
        ui.small(format!(
            "theoretisch: 4 Sat. im Abstand 90°\nSchnittwinkel 90° · Volumen {max_volume:.3}\nLage-Kennzahl {:.2}",
            dop::horizontal_figure(&ideal).unwrap_or(f64::NAN)
        ));
        ui.horizontal(|ui| {
            ui.small("Elevation");
            ui.add(egui::Slider::new(&mut view.ring_elev, 0.0..=80.0).suffix("°").integer());
        });
        if ui.small_button(format!("größtes Volumen ({:.1}°)", dop::best_ring_elevation_deg())).clicked() {
            view.ring_elev = dop::best_ring_elevation_deg();
        }
        ui.add_space(10.0);
        button(ui, &mut view.show_current, "Aktuelle Pyramide", CURRENT_COLOR);
        match view.current.as_ref().and_then(|c| c.best.as_ref()) {
            Some(b) => {
                let volume = dop::volume(&b.dirs);
                ui.small(format!(
                    "beste Schnittwinkel von 4 verwendeten:\n{}\nkleinster Winkel {:.0}° · Volumen {volume:.3}\nLage-Kennzahl {:.2}",
                    b.names.join(" "),
                    dop::smallest_gap_deg(&b.dirs),
                    dop::horizontal_figure(&b.dirs).unwrap_or(f64::NAN)
                ));
                ui.label(egui::RichText::new(format!("{:.0} % des maximalen Volumens", 100.0 * volume / max_volume)).strong());
            }
            None => {
                ui.small("weniger als 4 Satelliten");
            }
        }
        ui.small("Lage-Kennzahl: DOP nur für Ost/Nord aus den Schnittwinkeln, nicht der GDOP.");
    });
}

pub fn show(ui: &mut Ui, st: &State, view: &mut DopAnim, s: &Settings) {
    ui.horizontal(|ui| {
        ui.label("Zeiger zeigt");
        ui.selectable_value(&mut view.computed, false, "Empfänger");
        ui.selectable_value(&mut view.computed, true, "berechnet");
        ui.separator();
        ui.toggle_value(&mut view.spinning, "Animation");
        ui.add_enabled(
            view.spinning,
            egui::Slider::new(&mut view.speed, 2.0..=60.0)
                .suffix(" °/s")
                .integer(),
        );
    });
    let computed = dop::compute(
        &dop::directions(&st.sats, s.display_mask, true),
        s.clock_per_system,
    )
    .map(|d| d.g);
    let receiver = st.receiver_dop.map(|d| d.g);
    let (value, other) = if view.computed {
        (computed, receiver)
    } else {
        (receiver, computed)
    };
    update_current(st, view, s.display_mask);
    let width = ((ui.available_width() - BUTTON_COLUMN - 30.0) / 2.0).clamp(260.0, 480.0);
    ui.horizontal_wrapped(|ui| {
        gauge(ui, value, other, st.is_stale(), width);
        dome(ui, st, view, s.display_mask, width);
        pyramid_buttons(ui, view, s.display_mask);
    });
    ui.small("Ring auf der Skala: der jeweils andere Wert. Geometrie mit der Maus drehbar; je weiter die Satelliten verteilt sind, desto kleiner der GDOP.");
}
