//! Zweck:    Darstellung der SAPOS-Referenzstationen (virtuell und physisch) in der Karte.
//! Hardware: simpleRTK2B (u-blox ZED-F9P), SAPOS HEPS
//! Autor:    Johann Gerner
//!
//! Übernommen aus gnss-vermessung 0.4.0 (src/bin/gnss-gui/map_stations.rs), eigenständig gepflegt in Feldbuch-NT.

use crate::gnss::ansicht::widgets::*;
use eframe::egui::{Align2, Color32, FontId, Painter, Pos2, Rect, Shape, Stroke, vec2};
use crate::gnss::kern::geo::{Geographic, ecef_to_geo, geo_to_ecef};
use crate::gnss::kern::model::{Solution, State};

const MARKER: f32 = 13.0;
const HOVER_RADIUS: f32 = 14.0;
const DASH: f32 = 9.0;
const GAP: f32 = 6.0;

pub struct MapStation {
    pub label: String,
    pub lat: f64,
    pub lon: f64,
    ecef: [f64; 3],
    color: Color32,
    virtual_station: bool,
}

/// Stationen aus dem Korrekturdatenstrom.
pub fn collect(st: &State) -> Vec<MapStation> {
    let make = |label: String, ecef: [f64; 3], color, virtual_station| {
        let g = ecef_to_geo(ecef);
        MapStation {
            label,
            lat: g.lat,
            lon: g.lon,
            ecef,
            color,
            virtual_station,
        }
    };
    let content = &st.ntrip.content;
    let mut out = Vec::new();
    if let Some(p) = &content.physical {
        out.push(make(format!("Station {}", p.id), p.ecef, PURPLE, false));
    }
    if let Some(s) = &content.station {
        let (kind, color) = if s.virtual_station {
            ("VRS", BLUE)
        } else {
            ("Referenz", PURPLE)
        };
        out.push(make(
            format!("{kind} {}", s.id),
            s.ecef,
            color,
            s.virtual_station,
        ));
    }
    out
}

fn distance_text(metres: f64) -> String {
    if metres < 1000.0 {
        format!("{metres:.1} m")
    } else {
        format!("{:.2} km", metres / 1000.0)
    }
}

/// Räumlicher Abstand zwischen Station und Empfänger [m].
fn distance(station: &MapStation, rover: &Solution) -> f64 {
    let r = geo_to_ecef(&Geographic {
        lat: rover.lat,
        lon: rover.lon,
        h: rover.h,
    });
    (0..3)
        .map(|i| (station.ecef[i] - r[i]).powi(2))
        .sum::<f64>()
        .sqrt()
}

/// Zeichnet Stationen mit Verbindungslinie zum Empfänger; liefert den Hinweistext unter der Maus.
pub fn draw(
    painter: &Painter,
    st: &State,
    rect: Rect,
    screen: &dyn Fn(f64, f64) -> Pos2,
    rover: Option<(Pos2, &Solution)>,
    mouse: Option<Pos2>,
) -> Option<String> {
    let mut hover = None;
    let mut legend = Vec::new();
    for s in collect(st) {
        let q = screen(s.lat, s.lon);
        let range = rover.map(|r| distance(&s, r.1));
        if let Some((r, _)) = rover {
            painter.extend(Shape::dashed_line(
                &[r, q],
                Stroke::new(1.5, s.color),
                DASH,
                GAP,
            ));
        }
        // virtuelle Station als Raute, physische als Dreieck; hohl, damit die Position sichtbar bleibt
        let corners = if s.virtual_station {
            vec![
                q + vec2(0.0, -MARKER),
                q + vec2(MARKER, 0.0),
                q + vec2(0.0, MARKER),
                q + vec2(-MARKER, 0.0),
            ]
        } else {
            vec![
                q + vec2(0.0, -MARKER),
                q + vec2(MARKER, MARKER * 0.7),
                q + vec2(-MARKER, MARKER * 0.7),
            ]
        };
        painter.add(Shape::closed_line(
            corners.clone(),
            Stroke::new(5.0, Color32::WHITE),
        ));
        painter.add(Shape::closed_line(corners, Stroke::new(2.5, s.color)));
        let galley = painter.layout_no_wrap(s.label.clone(), FontId::proportional(13.0), s.color);
        let back = Align2::CENTER_TOP.anchor_size(q + vec2(0.0, MARKER + 4.0), galley.size());
        painter.rect_filled(
            back.expand(2.0),
            2.0,
            Color32::from_rgba_unmultiplied(255, 255, 255, 220),
        );
        painter.galley(back.min, galley, s.color);
        let away = range.map(distance_text).unwrap_or_else(|| "–".into());
        if mouse.is_some_and(|m| m.distance(q) < HOVER_RADIUS) {
            hover = Some(format!(
                "{}\nBreite {:.8}°\nLänge {:.8}°\nh {:.3} m\nAbstand zum Empfänger {away}",
                s.label,
                s.lat,
                s.lon,
                ecef_to_geo(s.ecef).h
            ));
        }
        let visible = if rect.contains(q) {
            ""
        } else {
            " (außerhalb des Ausschnitts)"
        };
        legend.push((
            format!("{}: {away} vom Empfänger{visible}", s.label),
            s.color,
        ));
    }
    for (i, (text, color)) in legend.iter().enumerate() {
        let at = rect.right_top() + vec2(-8.0, 8.0 + 22.0 * i as f32);
        let galley = painter.layout_no_wrap(text.clone(), FontId::proportional(13.0), *color);
        let back = Align2::RIGHT_TOP.anchor_size(at, galley.size());
        painter.rect_filled(
            back.expand(3.0),
            3.0,
            Color32::from_rgba_unmultiplied(255, 255, 255, 225),
        );
        painter.galley(back.min, galley, *color);
    }
    hover
}
