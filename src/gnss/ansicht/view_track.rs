//! Zweck:    Spur der 2D-Position des Empfängers mit automatischem Zoom.
//!           Die Weitergabe der SAPOS-Korrekturen lässt sich abschalten, um die Spur
//!           mit und ohne Korrektur zu zeigen.
//! Hardware: simpleRTK2B (u-blox ZED-F9P), SAPOS HEPS
//! Autor:    Johann Gerner
//!
//! Übernommen aus gnss-vermessung 0.4.0 (src/bin/gnss-gui/view_track.rs), eigenständig gepflegt in Feldbuch-NT.

use crate::gnss::ansicht::app::Action;
use crate::gnss::ansicht::widgets::*;
use eframe::egui::{self, Align2, Color32, FontId, Pos2, RichText, Sense, Shape, Stroke, Ui, vec2};
use crate::gnss::kern::config::UTM_ZONE;
use crate::gnss::kern::geo::{Geographic, geo_to_utm};
use crate::gnss::kern::model::{Fix, State};
use std::collections::VecDeque;

const MS_PER_WEEK: u32 = 604_800_000;
/// Zoomstufen: Radius des Außenrings [m]
const RANGES: [f64; 18] = [
    0.002, 0.005, 0.01, 0.02, 0.05, 0.1, 0.2, 0.5, 1.0, 2.0, 5.0, 10.0, 20.0, 50.0, 100.0, 200.0, 500.0, 1000.0,
];
/// Dauer des Zoomübergangs [s]
const ZOOM_TIME: f32 = 0.6;
/// Rand, damit die äußersten Punkte nicht auf dem Ring liegen
const ZOOM_MARGIN: f64 = 1.1;
const DEFAULT_LENGTH_S: f64 = 300.0;
const PANEL_MIN: f32 = 300.0;
const STATS_HEIGHT: f32 = 140.0;

#[derive(Debug, Clone, Copy)]
struct Sample {
    east: f64,
    north: f64,
    fix: Fix,
    itow: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Centre {
    Mean,
    Latest,
}

pub struct Track {
    samples: VecDeque<Sample>,
    last_itow: Option<u32>,
    length_s: f64,
    centre: Centre,
    paused: bool,
}

impl Default for Track {
    fn default() -> Self {
        Track { samples: VecDeque::new(), last_itow: None, length_s: DEFAULT_LENGTH_S, centre: Centre::Mean, paused: false }
    }
}

fn age_s(newest: u32, older: u32) -> f64 {
    f64::from((newest + MS_PER_WEEK - older) % MS_PER_WEEK) / 1000.0
}

/// Länge mit passender Einheit.
fn length(m: f64) -> String {
    match m.abs() {
        v if v >= 1.0 => format!("{m:.2} m"),
        v if v >= 0.01 => format!("{:.1} cm", m * 100.0),
        _ => format!("{:.1} mm", m * 1000.0),
    }
}

/// Kleinste Zoomstufe, in die alle Punkte passen.
fn auto_range(max: f64) -> f64 {
    RANGES.into_iter().find(|r| *r >= max * ZOOM_MARGIN).unwrap_or(RANGES[RANGES.len() - 1])
}

impl Track {
    pub fn clear(&mut self) {
        self.samples.clear();
        self.last_itow = None;
    }

    /// Übernimmt die neueste Epoche und entfernt Punkte, die älter als die Spurlänge sind.
    pub fn tick(&mut self, st: &State) {
        if self.paused {
            return;
        }
        let Some(s) = st.solution.as_ref().filter(|s| !st.is_stale() && s.fix != Fix::None) else { return };
        if self.last_itow == Some(s.itow) {
            return;
        }
        self.last_itow = Some(s.itow);
        let u = geo_to_utm(&Geographic { lat: s.lat, lon: s.lon, h: 0.0 }, UTM_ZONE);
        self.samples.push_back(Sample { east: u.east, north: u.north, fix: s.fix, itow: s.itow });
        while self.samples.front().is_some_and(|f| age_s(s.itow, f.itow) > self.length_s) {
            self.samples.pop_front();
        }
    }
}

fn draw(ui: &mut Ui, t: &Track, size: f32) {
    let (resp, painter) = ui.allocate_painter(vec2(size, size), Sense::hover());
    let rect = resp.rect;
    let painter = painter.with_clip_rect(rect);
    let c = rect.center();
    let weak = ui.visuals().weak_text_color();
    painter.rect_filled(rect, 4.0, ui.visuals().extreme_bg_color);
    painter.text(rect.right_top() + vec2(-14.0, 8.0), Align2::CENTER_TOP, "N ↑", FontId::proportional(15.0), ui.visuals().text_color());
    let n = t.samples.len() as f64;
    if t.samples.is_empty() {
        painter.text(c, Align2::CENTER_CENTER, "noch keine Positionen", FontId::proportional(16.0), weak);
        return;
    }
    let centre = match t.centre {
        Centre::Mean => (t.samples.iter().map(|s| s.east).sum::<f64>() / n, t.samples.iter().map(|s| s.north).sum::<f64>() / n),
        Centre::Latest => t.samples.back().map(|s| (s.east, s.north)).unwrap_or_default(),
    };
    let max = t.samples.iter().map(|s| (s.east - centre.0).hypot(s.north - centre.1)).fold(0.0, f64::max);
    // Zoom gleitet zur passenden Stufe (logarithmisch, damit große Sprünge ruhig wirken)
    let target = auto_range(max).ln() as f32;
    let range = f64::from(ui.ctx().animate_value_with_time(egui::Id::unique("track_zoom"), target, ZOOM_TIME)).exp();
    let radius = size / 2.0 - 16.0;
    let scale = f64::from(radius) / range;
    for f in [0.25, 0.5, 0.75, 1.0] {
        painter.circle_stroke(c, radius * f, Stroke::new(if f == 1.0 { 1.0 } else { 0.5 }, weak));
    }
    for f in [0.5, 1.0] {
        painter.text(c + vec2(3.0, -radius * f), Align2::LEFT_BOTTOM, length(range * f64::from(f)), FontId::proportional(12.0), weak);
    }
    painter.line_segment([c - vec2(radius, 0.0), c + vec2(radius, 0.0)], Stroke::new(0.5, weak));
    painter.line_segment([c - vec2(0.0, radius), c + vec2(0.0, radius)], Stroke::new(0.5, weak));
    let at = |s: &Sample| c + vec2(((s.east - centre.0) * scale) as f32, (-(s.north - centre.1) * scale) as f32);
    let points: Vec<Pos2> = t.samples.iter().map(at).collect();
    // Linie verblasst zu älteren Punkten hin
    let count = points.len().max(2) as f32;
    for (i, w) in points.windows(2).enumerate() {
        let alpha = 0.25 + 0.6 * i as f32 / count;
        painter.line_segment([w[0], w[1]], Stroke::new(1.2, weak.gamma_multiply(alpha)));
    }
    for (s, p) in t.samples.iter().zip(&points) {
        painter.circle_filled(*p, 2.8, fix_color(s.fix));
    }
    if let (Some(s), Some(p)) = (t.samples.back(), points.last()) {
        painter.circle(*p, 7.0, fix_color(s.fix), Stroke::new(2.0, Color32::WHITE));
    }
    painter.add(Shape::circle_stroke(c, 3.0, Stroke::new(1.0, weak)));
}

pub fn show(ui: &mut Ui, t: &mut Track, forwarding: Option<bool>, actions: &mut Vec<Action>) {
    ui.horizontal_wrapped(|ui| {
        ui.label("Spurlänge [s]");
        ui.add(egui::DragValue::new(&mut t.length_s).range(10.0..=3600.0).speed(10.0));
        ui.label("Mitte");
        ui.selectable_value(&mut t.centre, Centre::Mean, "Mittel");
        ui.selectable_value(&mut t.centre, Centre::Latest, "letzte Position");
        ui.toggle_value(&mut t.paused, "⏸ Anhalten");
        if ui.button("Spur löschen").clicked() {
            actions.push(Action::ClearTrack);
        }
        ui.separator();
        match forwarding {
            Some(true) => {
                if ui.button("Korrekturen anhalten").on_hover_text("SAPOS bleibt verbunden, die Daten gehen nur nicht an den Empfänger").clicked() {
                    actions.push(Action::SetForward(false));
                }
            }
            Some(false) => {
                if ui.button(RichText::new("Korrekturen wieder geben").strong()).clicked() {
                    actions.push(Action::SetForward(true));
                }
                ui.colored_label(WARN, "Empfänger erhält keine Korrekturen");
            }
            None => {
                ui.weak("SAPOS nicht verbunden");
            }
        }
    });
    ui.separator();
    let size = (ui.available_width() - 260.0).min(ui.available_height() - STATS_HEIGHT).max(PANEL_MIN);
    ui.horizontal_top(|ui| {
        draw(ui, t, size);
        ui.vertical(|ui| {
            let n = t.samples.len();
            let mut rows = vec![("Epochen", n.to_string())];
            if let (Some(a), Some(b)) = (t.samples.front(), t.samples.back()) {
                rows.push(("Dauer", format!("{:.0} s", age_s(b.itow, a.itow))));
                let m = (t.samples.iter().map(|s| s.east).sum::<f64>() / n as f64, t.samples.iter().map(|s| s.north).sum::<f64>() / n as f64);
                let k = (n.max(2) - 1) as f64;
                let se = (t.samples.iter().map(|s| (s.east - m.0).powi(2)).sum::<f64>() / k).sqrt();
                let sn = (t.samples.iter().map(|s| (s.north - m.1).powi(2)).sum::<f64>() / k).sqrt();
                let max = t.samples.iter().map(|s| (s.east - m.0).hypot(s.north - m.1)).fold(0.0, f64::max);
                rows.push(("s Ost", length(se)));
                rows.push(("s Nord", length(sn)));
                rows.push(("s 2D", length(se.hypot(sn))));
                rows.push(("größte Abweichung vom Mittel", length(max)));
                rows.push(("Status jetzt", b.fix.label().to_string()));
            }
            table(ui, "trackstats", &rows);
            ui.add_space(8.0);
            ui.strong("Farben");
            for f in [Fix::Fixed, Fix::Float, Fix::Dgnss, Fix::Auto3d] {
                ui.colored_label(fix_color(f), format!("● {}", f.label()));
            }
            ui.add_space(8.0);
            ui.small("Der Maßstab passt sich automatisch an alle Punkte der Spur an.");
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn helpers() {
        assert_eq!(age_s(500, MS_PER_WEEK - 500), 1.0);
        assert_eq!(auto_range(0.7), 1.0);
        assert_eq!(auto_range(0.95), 2.0);
        assert_eq!(auto_range(1e9), 1000.0);
        assert_eq!(length(0.0123), "1.2 cm");
    }
}
