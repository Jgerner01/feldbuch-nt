//! Zweck:    Lageplan mit Hintergrundkarte, gemessenen Punkten, Weg und aktueller Position.
//! Hardware: simpleRTK2B (u-blox ZED-F9P)
//! Autor:    Johann Gerner
//!
//! Übernommen aus gnss-vermessung 0.4.0 (src/bin/gnss-gui/view_map.rs), eigenständig gepflegt in Feldbuch-NT.

use crate::gnss::ansicht::map_stations;
use crate::gnss::ansicht::tiles::{LAYERS, TileId, Tiles};
use crate::gnss::ansicht::widgets::*;
use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, Ui, vec2};
use crate::gnss::kern::config::UTM_ZONE;
use crate::gnss::kern::export::Stored;
use crate::gnss::kern::geo::{Geographic, Utm, geo_to_utm, utm_to_geo};
use crate::gnss::kern::model::State;
use crate::gnss::kern::settings::Settings;
use std::f64::consts::PI;

const TILE_PX: f64 = 256.0;
const MIN_ZOOM: f64 = 6.0;
const MAX_ZOOM: f64 = 22.0;
/// Scrollweg für eine Zoomstufe
const SCROLL_PER_ZOOM: f64 = 200.0;
const EARTH_CIRCUMFERENCE: f64 = 40_075_016.686;
/// Startansicht ohne Position: Mitte Bayerns
const DEFAULT_CENTER: (f64, f64) = (48.95, 11.4);

pub struct MapView {
    center: (f64, f64),
    pub follow: bool,
    pub track: bool,
    pub labels: bool,
    /// virtuelle und physische SAPOS-Station zeigen
    pub stations: bool,
    centered_once: bool,
    pub tiles: Tiles,
}

impl Default for MapView {
    fn default() -> Self {
        MapView {
            center: DEFAULT_CENTER,
            follow: true,
            track: true,
            labels: true,
            stations: true,
            centered_once: false,
            tiles: Tiles::default(),
        }
    }
}

/// Web-Mercator, normiert auf 0..1.
fn project(lat: f64, lon: f64) -> (f64, f64) {
    let y = (1.0 - lat.to_radians().tan().asinh() / PI) / 2.0;
    ((lon + 180.0) / 360.0, y)
}

fn unproject(x: f64, y: f64) -> (f64, f64) {
    let lat = (PI * (1.0 - 2.0 * y)).sinh().atan().to_degrees();
    (lat, x * 360.0 - 180.0)
}

/// Gespeicherten UTM32-Punkt in geographische Koordinaten; lokale Koordinaten ergeben `None`.
fn stored_to_geo(p: &Stored) -> Option<(f64, f64)> {
    let prefix = f64::from(UTM_ZONE) * 1e6;
    let east = if p.x >= prefix { p.x - prefix } else { p.x };
    if !(100_000.0..900_000.0).contains(&east) || !(4_000_000.0..7_000_000.0).contains(&p.y) {
        return None;
    }
    let g = utm_to_geo(
        &Utm {
            zone: UTM_ZONE,
            east,
            north: p.y,
            k: 1.0,
            gamma: 0.0,
        },
        0.0,
    );
    Some((g.lat, g.lon))
}

/// Mitte und Zoomstufe, bei der alle Mercator-Punkte in `pixels` Bildpunkte passen.
fn fit(points: &[(f64, f64)], pixels: f64) -> Option<((f64, f64), f64)> {
    let first = points.first()?;
    let (mut lo, mut hi) = (*first, *first);
    for p in points {
        lo = (lo.0.min(p.0), lo.1.min(p.1));
        hi = (hi.0.max(p.0), hi.1.max(p.1));
    }
    let span = (hi.0 - lo.0).max(hi.1 - lo.1).max(1e-7);
    let zoom = (pixels / (span * TILE_PX)).log2().min(20.0);
    Some((unproject((lo.0 + hi.0) / 2.0, (lo.1 + hi.1) / 2.0), zoom))
}

pub fn show(ui: &mut Ui, st: &State, points: &[Stored], v: &mut MapView, s: &mut Settings) {
    let current = st
        .solution
        .as_ref()
        .filter(|s| s.fix != crate::gnss::kern::model::Fix::None);
    ui.horizontal_wrapped(|ui| {
        s.map_layer = s.map_layer.min(LAYERS.len() - 1);
        egui::ComboBox::from_id_salt("layer")
            .selected_text(LAYERS[s.map_layer].name)
            .show_ui(ui, |ui| {
                for (i, l) in LAYERS.iter().enumerate() {
                    ui.selectable_value(&mut s.map_layer, i, l.name);
                }
            });
        if ui.button("−").clicked() {
            s.map_zoom -= 1.0;
        }
        if ui.button("+").clicked() {
            s.map_zoom += 1.0;
        }
        ui.checkbox(&mut v.follow, "Position folgen");
        ui.checkbox(&mut v.track, "Weg");
        ui.checkbox(&mut v.labels, "Punktnummern");
        ui.checkbox(&mut v.stations, "SAPOS-Station");
        let found = map_stations::collect(st);
        if ui
            .add_enabled(!found.is_empty(), egui::Button::new("Stationen anzeigen"))
            .clicked()
        {
            let mut geo: Vec<(f64, f64)> = found.iter().map(|s| project(s.lat, s.lon)).collect();
            geo.extend(current.map(|c| project(c.lat, c.lon)));
            if let Some((center, zoom)) =
                fit(&geo, f64::from(ui.ctx().content_rect().height()) * 0.6)
            {
                v.center = center;
                s.map_zoom = zoom;
                v.follow = false;
                v.stations = true;
            }
        }
        if ui.button("Alle Punkte").clicked() {
            let geo: Vec<(f64, f64)> = points
                .iter()
                .filter_map(stored_to_geo)
                .map(|g| project(g.0, g.1))
                .collect();
            if let Some((center, zoom)) =
                fit(&geo, f64::from(ui.ctx().content_rect().height()) * 0.6)
            {
                v.center = center;
                s.map_zoom = zoom;
                v.follow = false;
            }
        }
        let center = current.map_or(v.center, |c| (c.lat, c.lon));
        if ui
            .button("BayernAtlas")
            .on_hover_text("im Browser öffnen")
            .clicked()
        {
            let u = geo_to_utm(
                &Geographic {
                    lat: center.0,
                    lon: center.1,
                    h: 0.0,
                },
                UTM_ZONE,
            );
            ui.ctx().open_url(egui::OpenUrl::new_tab(format!(
                "https://atlas.bayern.de/?c={:.0},{:.0}&z=17",
                u.east, u.north
            )));
        }
        if ui
            .button("Google Maps")
            .on_hover_text("im Browser öffnen")
            .clicked()
        {
            let url = format!(
                "https://www.google.com/maps/search/?api=1&query={:.7},{:.7}",
                center.0, center.1
            );
            ui.ctx().open_url(egui::OpenUrl::new_tab(url));
        }
    });

    let (resp, painter) = ui.allocate_painter(ui.available_size(), Sense::click_and_drag());
    let rect = resp.rect;
    let painter = painter.with_clip_rect(rect);
    if let Some(c) = current
        && (v.follow || !v.centered_once)
    {
        v.center = (c.lat, c.lon);
        v.centered_once = true;
    }
    if resp.hovered() {
        let scroll = f64::from(ui.input(|i| i.smooth_scroll_delta().y));
        let pinch = f64::from(ui.input(|i| i.zoom_delta()));
        s.map_zoom += scroll / SCROLL_PER_ZOOM + pinch.log2();
    }
    s.map_zoom = s.map_zoom.clamp(MIN_ZOOM, MAX_ZOOM);
    let world = TILE_PX * 2f64.powf(s.map_zoom);
    let mut c = project(v.center.0, v.center.1);
    if resp.dragged() {
        let d = resp.drag_delta();
        c = (c.0 - f64::from(d.x) / world, c.1 - f64::from(d.y) / world);
        v.center = unproject(c.0, c.1);
        v.follow = false;
        v.centered_once = true;
    }
    let screen = |lat: f64, lon: f64| -> Pos2 {
        let p = project(lat, lon);
        rect.center() + vec2(((p.0 - c.0) * world) as f32, ((p.1 - c.1) * world) as f32)
    };

    painter.rect_filled(rect, 0.0, ui.visuals().extreme_bg_color);
    v.tiles.poll(ui.ctx());
    let layer = &LAYERS[s.map_layer];
    // über der höchsten Kachelstufe werden Kacheln vergrößert
    let z = (s.map_zoom.floor() as u8).min(layer.max_zoom);
    let count = 1u32 << z;
    let size = world / f64::from(count);
    let half = (
        f64::from(rect.width()) / 2.0 / world,
        f64::from(rect.height()) / 2.0 / world,
    );
    let index = |v: f64| {
        (v * f64::from(count))
            .floor()
            .clamp(0.0, f64::from(count - 1)) as u32
    };
    for x in index(c.0 - half.0)..=index(c.0 + half.0) {
        for y in index(c.1 - half.1)..=index(c.1 + half.1) {
            if let Some(texture) = v.tiles.get(
                ui.ctx(),
                TileId {
                    layer: s.map_layer,
                    z,
                    x,
                    y,
                },
            ) {
                let min = rect.center()
                    + vec2(
                        ((f64::from(x) / f64::from(count) - c.0) * world) as f32,
                        ((f64::from(y) / f64::from(count) - c.1) * world) as f32,
                    );
                let tile = Rect::from_min_size(min, vec2(size as f32, size as f32));
                painter.image(
                    texture.id(),
                    tile,
                    Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                    Color32::WHITE,
                );
            }
        }
    }

    if v.track && st.track.len() > 1 {
        painter.line(
            st.track.iter().map(|p| screen(p.0, p.1)).collect(),
            Stroke::new(2.0, PURPLE),
        );
    }
    let halo = Color32::from_rgba_unmultiplied(255, 255, 255, 210);
    let mut hover = None;
    for p in points {
        let Some(g) = stored_to_geo(p) else { continue };
        let q = screen(g.0, g.1);
        if !rect.contains(q) {
            continue;
        }
        painter.circle(q, 4.0, BAD, Stroke::new(1.5, Color32::WHITE));
        if v.labels {
            let at = q + vec2(7.0, -7.0);
            let galley =
                painter.layout_no_wrap(p.nr.clone(), FontId::proportional(13.0), Color32::BLACK);
            let back = Align2::LEFT_BOTTOM.anchor_size(at, galley.size());
            painter.rect_filled(back.expand(1.0), 2.0, halo);
            painter.galley(back.min, galley, Color32::BLACK);
        }
        if resp.hover_pos().is_some_and(|m| m.distance(q) < 9.0) {
            hover = Some(format!(
                "{}\n{:.3}\n{:.3}\nZ {}\n{}",
                p.nr,
                p.x,
                p.y,
                opt(p.z, 3),
                p.code
            ));
        }
    }
    let metres_per_px = |lat: f64| EARTH_CIRCUMFERENCE * lat.to_radians().cos() / world;
    if let Some(sol) = current {
        let q = screen(sol.lat, sol.lon);
        let accuracy = (sol.h_acc / metres_per_px(sol.lat)) as f32;
        painter.circle_stroke(q, accuracy.max(9.0), Stroke::new(1.5, fix_color(sol.fix)));
        painter.circle(q, 5.0, fix_color(sol.fix), Stroke::new(2.0, Color32::WHITE));
    }

    if v.stations {
        let rover = current.map(|c| (screen(c.lat, c.lon), c));
        if let Some(text) = map_stations::draw(&painter, st, rect, &screen, rover, resp.hover_pos())
        {
            hover = Some(text);
        }
    }

    // Maßstabsbalken mit runder Länge
    let mpp = metres_per_px(v.center.0);
    let target = mpp * 140.0;
    let power = 10f64.powf(target.log10().floor());
    let length = [5.0, 2.0, 1.0]
        .into_iter()
        .map(|f| f * power)
        .find(|l| *l <= target)
        .unwrap_or(power);
    let start = rect.left_bottom() + vec2(12.0, -26.0);
    let end = start + vec2((length / mpp) as f32, 0.0);
    painter.rect_filled(
        Rect::from_min_max(start + vec2(-6.0, -18.0), end + vec2(46.0, 8.0)),
        3.0,
        halo,
    );
    painter.line_segment([start, end], Stroke::new(3.0, Color32::BLACK));
    let label = if length >= 1000.0 {
        format!("{} km", length / 1000.0)
    } else {
        format!("{length} m")
    };
    painter.text(
        start + vec2(0.0, -3.0),
        Align2::LEFT_BOTTOM,
        label,
        FontId::proportional(12.0),
        Color32::BLACK,
    );

    if !layer.attribution.is_empty() {
        let galley = painter.layout_no_wrap(
            layer.attribution.to_string(),
            FontId::proportional(11.0),
            Color32::BLACK,
        );
        let back =
            Align2::RIGHT_BOTTOM.anchor_size(rect.right_bottom() - vec2(4.0, 3.0), galley.size());
        painter.rect_filled(back.expand(2.0), 2.0, halo);
        painter.galley(back.min, galley, Color32::BLACK);
    }
    if let Some(e) = &v.tiles.last_error {
        painter.text(
            rect.left_top() + vec2(8.0, 8.0),
            Align2::LEFT_TOP,
            format!("Karte: {e}"),
            FontId::proportional(11.0),
            BAD,
        );
    }
    if let Some(h) = hover {
        resp.on_hover_text_at_pointer(h);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mercator_roundtrip_and_tile() {
        let (x, y) = project(48.137, 11.575);
        let back = unproject(x, y);
        assert!((back.0 - 48.137).abs() < 1e-9 && (back.1 - 11.575).abs() < 1e-9);
        assert_eq!(((x * 32768.0) as u32, (y * 32768.0) as u32), (17437, 11371));
    }
}
