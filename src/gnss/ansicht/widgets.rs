//! Zweck:    Gemeinsame Anzeigeelemente: Farben, Zahlenfelder, Verlaufsdiagramm.
//! Hardware: keine
//! Autor:    Johann Gerner
//!
//! Übernommen aus gnss-vermessung 0.4.0 (src/bin/gnss-gui/widgets.rs), eigenständig gepflegt in Feldbuch-NT.

use eframe::egui::{
    self, Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, vec2,
};
use crate::gnss::kern::config::HISTORY_CAPACITY;
use crate::gnss::kern::model::Fix;

pub const GOOD: Color32 = Color32::from_rgb(30, 150, 60);
pub const WARN: Color32 = Color32::from_rgb(210, 140, 0);
pub const BAD: Color32 = Color32::from_rgb(200, 45, 45);
pub const STALE: Color32 = Color32::from_rgb(120, 120, 120);
pub const BLUE: Color32 = Color32::from_rgb(50, 110, 200);
pub const PURPLE: Color32 = Color32::from_rgb(140, 80, 180);

pub fn fix_color(f: Fix) -> Color32 {
    match f {
        Fix::None => BAD,
        Fix::Auto2d | Fix::Auto3d => Color32::from_rgb(225, 105, 20),
        Fix::Dgnss => Color32::from_rgb(190, 160, 0),
        Fix::Float => Color32::from_rgb(100, 165, 60),
        Fix::Fixed => GOOD,
    }
}

/// Kennbuchstabe, Name und Farbe eines GNSS (u-blox gnssId).
pub fn system(gnss: u8) -> (char, &'static str, Color32) {
    match gnss {
        0 => ('G', "GPS", Color32::from_rgb(40, 120, 220)),
        1 => ('S', "SBAS", Color32::from_rgb(130, 130, 130)),
        2 => ('E', "Galileo", Color32::from_rgb(40, 160, 90)),
        3 => ('C', "BeiDou", Color32::from_rgb(210, 80, 50)),
        5 => ('J', "QZSS", Color32::from_rgb(160, 90, 180)),
        6 => ('R', "GLONASS", Color32::from_rgb(215, 150, 20)),
        7 => ('I', "NavIC", Color32::from_rgb(90, 90, 160)),
        _ => ('?', "unbekannt", STALE),
    }
}

pub fn sat_name(gnss: u8, sv: u8) -> String {
    format!("{}{sv:02}", system(gnss).0)
}

/// Zahl mit fester Stellenzahl oder Strich.
pub fn opt(v: Option<f64>, decimals: usize) -> String {
    v.map_or_else(|| "–".to_string(), |v| format!("{v:.decimals$}"))
}

/// Zahlenfeld, das auch ein Dezimalkomma annimmt.
pub fn number<N: egui::emath::Numeric>(
    ui: &mut Ui,
    v: &mut N,
    range: std::ops::RangeInclusive<N>,
    speed: f64,
) -> egui::Response {
    ui.add(
        egui::DragValue::new(v)
            .range(range)
            .speed(speed)
            .custom_parser(|s| s.trim().replace(',', ".").parse().ok()),
    )
}

/// Zweispaltige Wertetabelle.
pub fn table(ui: &mut Ui, id: &str, rows: &[(&str, String)]) {
    egui::Grid::new(id)
        .num_columns(2)
        .striped(true)
        .spacing([16.0, 3.0])
        .show(ui, |ui| {
            for (k, v) in rows {
                ui.label(*k);
                ui.monospace(v);
                ui.end_row();
            }
        });
}

/// Verlauf mehrerer Reihen; der neueste Wert steht rechts.
pub fn plot(ui: &mut Ui, title: &str, series: &[(&str, Color32, Vec<Option<f64>>)], height: f32) {
    ui.horizontal_wrapped(|ui| {
        ui.strong(title);
        for (name, color, data) in series {
            let last = data.last().copied().flatten();
            ui.colored_label(*color, format!("■ {name} {}", opt(last, 3)));
        }
    });
    let (resp, painter) = ui.allocate_painter(vec2(ui.available_width(), height), Sense::hover());
    let r = resp.rect;
    let grid = ui.visuals().widgets.noninteractive.bg_stroke.color;
    let text = ui.visuals().weak_text_color();
    painter.rect_stroke(r, 2.0, Stroke::new(1.0, grid), StrokeKind::Inside);
    let max = series
        .iter()
        .flat_map(|s| s.2.iter().flatten())
        .copied()
        .filter(|v| v.is_finite())
        .fold(0.0_f64, f64::max);
    if max <= 0.0 {
        painter.text(
            r.center(),
            Align2::CENTER_CENTER,
            "keine Daten",
            FontId::proportional(12.0),
            text,
        );
        return;
    }
    let top = max * 1.1;
    for i in 1..4 {
        let y = r.bottom() - r.height() * i as f32 / 4.0;
        painter.hline(r.x_range(), y, Stroke::new(0.5, grid));
        let label = format!("{:.3}", top * f64::from(i) / 4.0);
        painter.text(
            Pos2::new(r.left() + 3.0, y),
            Align2::LEFT_BOTTOM,
            label,
            FontId::proportional(10.0),
            text,
        );
    }
    let step = r.width() / (HISTORY_CAPACITY - 1) as f32;
    for (_, color, data) in series {
        let start = r.right() - step * data.len().saturating_sub(1) as f32;
        let mut line = Vec::new();
        for (i, v) in data.iter().enumerate() {
            match v.filter(|v| v.is_finite()) {
                Some(v) => line.push(Pos2::new(
                    start + step * i as f32,
                    r.bottom() - (v / top) as f32 * r.height(),
                )),
                None if line.len() > 1 => {
                    painter.line(std::mem::take(&mut line), Stroke::new(1.5, *color));
                }
                None => line.clear(),
            }
        }
        if line.len() > 1 {
            painter.line(line, Stroke::new(1.5, *color));
        }
    }
}

/// Farbband des Fix-Status über der Zeit.
pub fn fix_band(ui: &mut Ui, data: &[Fix]) {
    let (resp, painter) = ui.allocate_painter(vec2(ui.available_width(), 18.0), Sense::hover());
    let r = resp.rect;
    let step = r.width() / HISTORY_CAPACITY as f32;
    let start = r.right() - step * data.len() as f32;
    for (i, f) in data.iter().enumerate() {
        let x = start + step * i as f32;
        painter.rect_filled(
            Rect::from_min_max(Pos2::new(x, r.top()), Pos2::new(x + step + 0.5, r.bottom())),
            0.0,
            fix_color(*f),
        );
    }
    if let Some(p) = resp.hover_pos() {
        let i = ((p.x - start) / step) as isize;
        if let Some(f) = usize::try_from(i).ok().and_then(|i| data.get(i)) {
            resp.on_hover_text_at_pointer(f.label());
        }
    }
}
