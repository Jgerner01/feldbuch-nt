//! Zweck:    Fix-Status und Genauigkeit (Seitenleiste, Statusreiter, große Außendienstansicht).
//! Hardware: simpleRTK2B (u-blox ZED-F9P)
//! Autor:    Johann Gerner
//!
//! Übernommen aus gnss-vermessung 0.4.0 (src/bin/gnss-gui/view_status.rs), eigenständig gepflegt in Feldbuch-NT.

use crate::gnss::ansicht::widgets::*;
use eframe::egui::{self, Color32, Pos2, RichText, Sense, Shape, Stroke, Ui, vec2};
use crate::gnss::kern::config::UTM_ZONE;
use crate::gnss::kern::geo::{Geographic, geo_to_utm};
use crate::gnss::kern::height::HeightModel;
use crate::gnss::kern::model::{Solution, State};

const ELLIPSE_POINTS: usize = 48;

fn fix_label(ui: &mut Ui, st: &State, size: f32) {
    let (text, color) = match &st.solution {
        Some(s) if !st.is_stale() => (s.fix.label(), fix_color(s.fix)),
        Some(_) => ("VERALTET", STALE),
        None => ("KEINE DATEN", STALE),
    };
    egui::Frame::new()
        .fill(color)
        .corner_radius(6.0)
        .inner_margin(8.0)
        .show(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.label(
                    RichText::new(text)
                        .size(size)
                        .strong()
                        .color(Color32::WHITE),
                )
            });
        });
}

/// UTM-Koordinaten und Höhe H (falls Höhenbezug vorhanden) einer Lösung.
fn position(s: &Solution, hm: &HeightModel) -> (f64, f64, Option<f64>) {
    let u = geo_to_utm(
        &Geographic {
            lat: s.lat,
            lon: s.lon,
            h: 0.0,
        },
        UTM_ZONE,
    );
    (
        u.east_prefixed(),
        u.north,
        hm.zeta(s.lat, s.lon).map(|z| s.h - z),
    )
}

/// Standardellipse mit Maßstab.
fn ellipse(ui: &mut Ui, s: &Solution) {
    let Some((a, b, azimuth)) = s.ellipse() else {
        ui.weak("keine Kovarianz");
        return;
    };
    let size = ui.available_width().min(170.0);
    let (resp, painter) = ui.allocate_painter(vec2(size, size), Sense::hover());
    let c = resp.rect.center();
    let radius = size * 0.45;
    // Maßstab: nächste runde Zahl oberhalb der großen Halbachse
    let range = [0.005, 0.01, 0.02, 0.05, 0.1, 0.2, 0.5, 1.0, 5.0, 20.0]
        .into_iter()
        .find(|r| *r >= a)
        .unwrap_or(a);
    let grid = ui.visuals().weak_text_color();
    painter.circle_stroke(c, radius, Stroke::new(0.5, grid));
    painter.line_segment(
        [c - vec2(radius, 0.0), c + vec2(radius, 0.0)],
        Stroke::new(0.5, grid),
    );
    painter.line_segment(
        [c - vec2(0.0, radius), c + vec2(0.0, radius)],
        Stroke::new(0.5, grid),
    );
    let (sin, cos) = azimuth.to_radians().sin_cos();
    let scale = f64::from(radius) / range;
    let points: Vec<Pos2> = (0..ELLIPSE_POINTS)
        .map(|i| {
            let t = i as f64 / ELLIPSE_POINTS as f64 * std::f64::consts::TAU;
            let (u, v) = (a * t.cos(), b * t.sin());
            // u entlang der großen Achse (Azimut ab Nord), v quer dazu
            let east = u * sin + v * cos;
            let north = u * cos - v * sin;
            c + vec2((east * scale) as f32, (-north * scale) as f32)
        })
        .collect();
    painter.add(Shape::closed_line(points, Stroke::new(2.0, BLUE)));
    ui.small(format!(
        "Ring {:.0} mm · a {:.1} mm · b {:.1} mm · {azimuth:.0}°",
        range * 1e3,
        a * 1e3,
        b * 1e3
    ));
}

/// Schmale, immer sichtbare Seitenleiste.
pub fn side(ui: &mut Ui, st: &State, hm: &HeightModel) {
    fix_label(ui, st, 30.0);
    ui.add_space(6.0);
    let Some(s) = &st.solution else {
        ui.weak("Noch keine Position. Verbinden (F5) oder Wiedergabe starten.");
        return;
    };
    let (e, n, h) = position(s, hm);
    let mut rows = vec![
        ("σ 2D [m]", opt(s.sigma_2d(), 3)),
        ("σ Höhe [m]", opt(s.sigma_h(), 3)),
        ("Korrekturalter [s]", opt(s.age, 1)),
        ("Satelliten", s.num_sv.to_string()),
        ("PDOP", format!("{:.2}", s.pdop)),
        ("GDOP", opt(st.receiver_dop.map(|d| d.g), 2)),
        ("Ost", format!("{e:.3}")),
        ("Nord", format!("{n:.3}")),
        ("h ell.", format!("{:.3}", s.h)),
    ];
    if let Some(h) = h {
        rows.push(("H", format!("{h:.3}")));
    }
    table(ui, "side", &rows);
    ui.add_space(6.0);
    ellipse(ui, s);
}

/// Reiter „Status“ mit allen Angaben und Verläufen.
pub fn status(ui: &mut Ui, st: &State, hm: &HeightModel) {
    egui::ScrollArea::vertical()
        .auto_shrink(false)
        .show(ui, |ui| {
            if let Some(s) = &st.solution {
                let (e, n, h) = position(s, hm);
                ui.columns(3, |c| {
                    c[0].strong("Genauigkeit (Schätzung des Empfängers)");
                    table(
                        &mut c[0],
                        "acc",
                        &[
                            ("σ Ost [m]", opt(s.sigma_e(), 4)),
                            ("σ Nord [m]", opt(s.sigma_n(), 4)),
                            ("σ 2D [m]", opt(s.sigma_2d(), 4)),
                            ("σ Höhe [m]", opt(s.sigma_h(), 4)),
                            ("hAcc [m]", format!("{:.3}", s.h_acc)),
                            ("vAcc [m]", format!("{:.3}", s.v_acc)),
                        ],
                    );
                    c[1].strong("Position ETRS89");
                    table(
                        &mut c[1],
                        "pos",
                        &[
                            ("UTM32 Ost", format!("{e:.3}")),
                            ("UTM32 Nord", format!("{n:.3}")),
                            ("Breite", format!("{:.9}°", s.lat)),
                            ("Länge", format!("{:.9}°", s.lon)),
                            ("h ellipsoidisch", format!("{:.3}", s.h)),
                            ("H (Höhenbezug)", opt(h, 3)),
                            ("Geschwindigkeit [m/s]", format!("{:.2}", s.speed)),
                        ],
                    );
                    c[2].strong("Lösung");
                    table(
                        &mut c[2],
                        "sol",
                        &[
                            (
                                "Referenzstation",
                                s.ref_station.clone().unwrap_or_else(|| "–".into()),
                            ),
                            (
                                "RTK Fix seit",
                                st.fix_since.map_or("–".into(), |t| {
                                    format!("{} s", t.elapsed().as_secs())
                                }),
                            ),
                            ("Fix-Verluste", st.fix_losses.to_string()),
                            ("Erster Fix nach [s]", opt(st.ttff_s, 1)),
                            ("Empfänger läuft [s]", opt(st.uptime_s, 0)),
                            (
                                "Epoche",
                                if s.complete {
                                    "vollständig".into()
                                } else {
                                    "unvollständig".into()
                                },
                            ),
                        ],
                    );
                });
            } else {
                ui.label("Noch keine Positionsdaten.");
            }
            ui.separator();
            ui.strong("Fix-Status (letzte 600 Epochen)");
            fix_band(ui, &st.history.iter().map(|h| h.fix).collect::<Vec<_>>());
            let col = |f: fn(&crate::gnss::kern::model::HistorySample) -> Option<f64>| {
                st.history.iter().map(f).collect::<Vec<_>>()
            };
            plot(
                ui,
                "Genauigkeit [m]",
                &[
                    ("σ 2D", BLUE, col(|h| h.sigma_2d)),
                    ("σ Höhe", PURPLE, col(|h| h.sigma_h)),
                ],
                110.0,
            );
            plot(
                ui,
                "Satelliten",
                &[("verwendet", GOOD, col(|h| Some(f64::from(h.used))))],
                80.0,
            );
            plot(
                ui,
                "Korrekturalter [s]",
                &[("Alter", WARN, col(|h| h.age))],
                80.0,
            );
            if !st.rf.is_empty() {
                ui.separator();
                ui.strong("Antenne und Störungen");
                for b in &st.rf {
                    let jam = ["unbekannt", "unauffällig", "Warnung", "kritisch"]
                        [usize::from(b.jamming.min(3))];
                    let ant = ["Start", "unbekannt", "in Ordnung", "Kurzschluss", "offen"]
                        [usize::from(b.antenna.min(4))];
                    let color = match b.jamming {
                        2 => WARN,
                        3 => BAD,
                        _ => ui.visuals().text_color(),
                    };
                    ui.colored_label(
                        color,
                        format!(
                            "HF-Block {}: Störung {jam}, Antenne {ant}, Dauerträger {}/255",
                            b.block, b.cw_indicator
                        ),
                    );
                }
            }
            ui.separator();
            ui.strong("Ereignisse");
            for e in st.events.iter().take(30) {
                ui.monospace(e);
            }
        });
}

/// Wenige, sehr große Werte für den Außendienst.
pub fn field(ui: &mut Ui, st: &State, hm: &HeightModel) {
    fix_label(ui, st, 72.0);
    let Some(s) = &st.solution else { return };
    let (e, n, h) = position(s, hm);
    ui.add_space(20.0);
    egui::Grid::new("field")
        .num_columns(2)
        .spacing([40.0, 14.0])
        .show(ui, |ui| {
            let mut row = |k: &str, v: String| {
                ui.label(RichText::new(k).size(30.0));
                ui.label(RichText::new(v).size(42.0).monospace().strong());
                ui.end_row();
            };
            row("Ost", format!("{e:.3}"));
            row("Nord", format!("{n:.3}"));
            row(
                if h.is_some() { "H" } else { "h ell." },
                format!("{:.3}", h.unwrap_or(s.h)),
            );
            row("σ 2D", format!("{} m", opt(s.sigma_2d(), 3)));
            row("σ Höhe", format!("{} m", opt(s.sigma_h(), 3)));
            row("Alter", format!("{} s", opt(s.age, 1)));
            row("Satelliten", s.num_sv.to_string());
        });
}
