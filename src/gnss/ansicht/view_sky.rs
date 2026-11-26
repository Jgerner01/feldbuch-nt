//! Zweck:    Satellitenhimmel, mögliche und empfangene Satelliten, Signalstärken und DOP.
//! Hardware: simpleRTK2B (u-blox ZED-F9P)
//! Autor:    Johann Gerner
//!
//! Übernommen aus gnss-vermessung 0.4.0 (src/bin/gnss-gui/view_sky.rs), eigenständig gepflegt in Feldbuch-NT.

use crate::gnss::ansicht::view_dop_anim::{self, DopAnim};
use crate::gnss::ansicht::widgets::*;
use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, Ui, vec2};
use crate::gnss::kern::dop;
use crate::gnss::kern::model::State;
use crate::gnss::kern::settings::Settings;
use crate::gnss::kern::ubxmsg::{Dop, Sat};
use std::collections::{BTreeMap, BTreeSet};

/// Signalstärken-Richtwerte [dBHz]
const CNO_WEAK: u8 = 30;
const CNO_GOOD: u8 = 40;
const CNO_MAX: f32 = 55.0;
const HOVER_RADIUS: f32 = 12.0;

#[derive(Default, PartialEq, Clone, Copy)]
pub enum SatSort {
    #[default]
    Name,
    Elevation,
    Signal,
}

#[derive(Default)]
pub struct SkyView {
    pub trails: bool,
    pub sort: SatSort,
}

/// Anzahl möglicher, empfangener, verwendeter Satelliten und mittleres C/N0 je System.
#[derive(Default)]
struct Count {
    possible: BTreeSet<u8>,
    tracked: usize,
    used: usize,
    cno_sum: u32,
}

fn counts(st: &State, mask: f64) -> BTreeMap<u8, Count> {
    let mut m: BTreeMap<u8, Count> = BTreeMap::new();
    for s in &st.sats {
        let c = m.entry(s.gnss).or_default();
        if s.elev.is_some_and(|e| e >= mask) {
            c.possible.insert(s.sv);
        }
        if s.cno > 0 {
            c.tracked += 1;
            c.cno_sum += u32::from(s.cno);
        }
        c.used += usize::from(s.used);
    }
    // sichtbare Satelliten aus der Bahndatenbank, deren Richtung NAV-SAT nicht nennt
    for o in st.orbits.iter().filter(|o| o.visibility >= 2) {
        let known = st
            .sats
            .iter()
            .any(|s| s.gnss == o.gnss && s.sv == o.sv && s.elev.is_some());
        if !known {
            m.entry(o.gnss).or_default().possible.insert(o.sv);
        }
    }
    m
}

fn describe(s: &Sat) -> String {
    let source = [
        "keine",
        "Ephemeride",
        "Almanach",
        "AssistNow offline",
        "AssistNow autonom",
        "sonstige",
        "sonstige",
        "sonstige",
    ];
    format!(
        "{} ({})\nAzimut {:.0}°, Elevation {}°\nC/N0 {} dBHz\n{}\nBahndaten: {}\nGesundheit: {}",
        sat_name(s.gnss, s.sv),
        system(s.gnss).1,
        s.azim,
        opt(s.elev, 0),
        s.cno,
        if s.used && s.rtcm_used {
            "verwendet, mit RTCM-Korrektur"
        } else if s.used {
            "verwendet"
        } else if s.cno > 0 {
            "empfangen"
        } else {
            "kein Signal"
        },
        source[usize::from(s.orbit_source.min(7))],
        ["unbekannt", "gesund", "krank", "unbekannt"][usize::from(s.health.min(3))],
    )
}

fn skyplot(ui: &mut Ui, st: &State, view: &SkyView, mask: f64, size: f32) {
    let (resp, painter) = ui.allocate_painter(vec2(size, size), Sense::hover());
    let center = resp.rect.center();
    let radius = size / 2.0 - 18.0;
    let pos = |azim: f64, elev: f64| -> Pos2 {
        let r = radius * (90.0 - elev.clamp(0.0, 90.0)) as f32 / 90.0;
        let a = (azim as f32).to_radians();
        center + vec2(r * a.sin(), -r * a.cos())
    };
    let grid = ui.visuals().weak_text_color();
    let text = ui.visuals().text_color();
    // Bereich unter der Elevationsmaske tönen
    let inner = radius * (90.0 - mask as f32) / 90.0;
    painter.circle_stroke(
        center,
        (radius + inner) / 2.0,
        Stroke::new(
            radius - inner,
            Color32::from_rgba_unmultiplied(128, 128, 128, 35),
        ),
    );
    for elev in [0.0_f32, 30.0, 60.0] {
        painter.circle_stroke(
            center,
            radius * (90.0 - elev) / 90.0,
            Stroke::new(1.0, grid),
        );
        painter.text(
            center + vec2(3.0, -radius * (90.0 - elev) / 90.0),
            Align2::LEFT_TOP,
            format!("{elev:.0}°"),
            FontId::proportional(10.0),
            grid,
        );
    }
    for (azim, name) in [(0.0, "N"), (90.0, "O"), (180.0, "S"), (270.0, "W")] {
        painter.line_segment([center, pos(azim, 0.0)], Stroke::new(0.5, grid));
        let a = f32::to_radians(azim as f32);
        painter.text(
            center + vec2(a.sin(), -a.cos()) * (radius + 10.0),
            Align2::CENTER_CENTER,
            name,
            FontId::proportional(14.0),
            text,
        );
    }
    if view.trails {
        for ((gnss, _), trail) in &st.trails {
            let color = system(*gnss).2.gamma_multiply(0.4);
            for p in trail {
                painter.circle_filled(pos(p.0, p.1), 1.5, color);
            }
        }
    }
    let mut hovered: Option<(f32, &Sat)> = None;
    for s in &st.sats {
        let Some(elev) = s.elev.filter(|e| *e >= 0.0) else {
            continue;
        };
        let p = pos(s.azim, elev);
        let color = if elev < mask { STALE } else { system(s.gnss).2 };
        // gefüllt = verwendet, halb = empfangen, Ring = kein Signal
        if s.used {
            painter.circle_filled(p, 8.0, color);
        } else {
            painter.circle_stroke(p, 7.0, Stroke::new(2.0, color));
            if s.cno > 0 {
                painter.circle_filled(p, 3.5, color);
            }
        }
        painter.text(
            p + vec2(0.0, -9.0),
            Align2::CENTER_BOTTOM,
            sat_name(s.gnss, s.sv),
            FontId::proportional(10.0),
            text,
        );
        if let Some(m) = resp.hover_pos() {
            let d = m.distance(p);
            if d < HOVER_RADIUS && hovered.is_none_or(|h| d < h.0) {
                hovered = Some((d, s));
            }
        }
    }
    if let Some((_, s)) = hovered {
        resp.on_hover_text_at_pointer(describe(s));
    }
}

fn signal_bars(ui: &mut Ui, st: &State, height: f32) {
    ui.strong("Signalstärke C/N0 [dBHz]  (gefüllt = verwendet, Umriss = nur empfangen)");
    // je Signal ein Balken; ohne NAV-SIG je Satellit
    let mut bars: Vec<(u8, u8, u8, u8, bool)> = st
        .signals
        .iter()
        .filter(|s| s.cno > 0)
        .map(|s| (s.gnss, s.sv, s.sig, s.cno, s.used))
        .collect();
    if bars.is_empty() {
        bars = st
            .sats
            .iter()
            .filter(|s| s.cno > 0)
            .map(|s| (s.gnss, s.sv, 0, s.cno, s.used))
            .collect();
    }
    bars.sort_unstable();
    egui::ScrollArea::horizontal()
        .id_salt("bars")
        .show(ui, |ui| {
            let width = 13.0;
            let total = (bars.len() as f32 * width + 30.0).max(ui.available_width());
            let (resp, painter) = ui.allocate_painter(vec2(total, height), Sense::hover());
            let r = resp.rect;
            let base = r.bottom() - 30.0;
            let scale = (base - r.top()) / CNO_MAX;
            let grid = ui.visuals().weak_text_color();
            for level in [CNO_WEAK, CNO_GOOD] {
                let y = base - f32::from(level) * scale;
                painter.hline(r.x_range(), y, Stroke::new(0.5, grid));
                painter.text(
                    Pos2::new(r.left(), y),
                    Align2::LEFT_BOTTOM,
                    level.to_string(),
                    FontId::proportional(10.0),
                    grid,
                );
            }
            let mut previous = None;
            let mut hover = None;
            for (i, &(gnss, sv, sig, cno, used)) in bars.iter().enumerate() {
                let x = r.left() + 22.0 + i as f32 * width;
                let color = system(gnss).2;
                let bar = Rect::from_min_max(
                    Pos2::new(x, base - f32::from(cno).min(CNO_MAX) * scale),
                    Pos2::new(x + width - 3.0, base),
                );
                if used {
                    painter.rect_filled(bar, 1.0, color);
                } else {
                    painter.rect_stroke(
                        bar,
                        1.0,
                        Stroke::new(1.0, color),
                        egui::StrokeKind::Inside,
                    );
                }
                if previous != Some((gnss, sv)) {
                    let label = sat_name(gnss, sv);
                    // Kennung senkrecht in zwei Zeilen
                    painter.text(
                        Pos2::new(x, base + 2.0),
                        Align2::LEFT_TOP,
                        &label[..1],
                        FontId::proportional(10.0),
                        color,
                    );
                    painter.text(
                        Pos2::new(x - 1.0, base + 14.0),
                        Align2::LEFT_TOP,
                        &label[1..],
                        FontId::proportional(9.0),
                        color,
                    );
                    previous = Some((gnss, sv));
                }
                if resp
                    .hover_pos()
                    .is_some_and(|p| p.x >= x && p.x < x + width)
                {
                    hover = Some(format!(
                        "{} Signal {sig}: {cno} dBHz{}",
                        sat_name(gnss, sv),
                        if used { ", verwendet" } else { "" }
                    ));
                }
            }
            if let Some(h) = hover {
                resp.on_hover_text_at_pointer(h);
            }
        });
}

pub fn show(ui: &mut Ui, st: &State, view: &mut SkyView, s: &mut Settings) {
    ui.horizontal(|ui| {
        ui.label("Elevationsmaske der Anzeige");
        ui.add(
            egui::Slider::new(&mut s.display_mask, 0.0..=45.0)
                .suffix("°")
                .integer(),
        );
        ui.checkbox(&mut view.trails, "Bahnspuren");
    });
    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        let size = (ui.available_width() * 0.55).clamp(300.0, 480.0);
        ui.horizontal_top(|ui| {
            skyplot(ui, st, view, s.display_mask, size);
            ui.vertical(|ui| {
                let c = counts(st, s.display_mask);
                egui::Grid::new("counts").num_columns(5).striped(true).show(ui, |ui| {
                    for h in ["System", "möglich", "empfangen", "verwendet", "Ø C/N0"] {
                        ui.strong(h);
                    }
                    ui.end_row();
                    let mut sum = [0usize; 3];
                    for (gnss, n) in &c {
                        let (_, name, color) = system(*gnss);
                        ui.colored_label(color, name);
                        for (i, v) in [n.possible.len(), n.tracked, n.used].into_iter().enumerate() {
                            sum[i] += v;
                            ui.monospace(v.to_string());
                        }
                        let mean = (n.tracked > 0).then(|| f64::from(n.cno_sum) / n.tracked as f64);
                        ui.monospace(opt(mean, 1));
                        ui.end_row();
                    }
                    ui.strong("Summe");
                    for v in sum {
                        ui.strong(v.to_string());
                    }
                    ui.end_row();
                });
                if st.orbits.is_empty() {
                    ui.colored_label(WARN, "Bahndatenbank (NAV-ORB) noch nicht empfangen:\n„möglich“ ist unvollständig.");
                }
                ui.add_space(8.0);
                ui.label("● verwendet   ◉ empfangen   ○ kein Signal   grau: unter der Maske");
                let series = [("verwendet", GOOD, st.history.iter().map(|h| Some(f64::from(h.used))).collect())];
                plot(ui, "Satellitenzahl", &series, 90.0);
            });
        });
        ui.separator();
        signal_bars(ui, st, 170.0);
        ui.separator();
        ui.horizontal(|ui| {
            ui.strong("Satellitentabelle, sortiert nach");
            ui.selectable_value(&mut view.sort, SatSort::Name, "Kennung");
            ui.selectable_value(&mut view.sort, SatSort::Elevation, "Elevation");
            ui.selectable_value(&mut view.sort, SatSort::Signal, "Signal");
        });
        let mut sats: Vec<&Sat> = st.sats.iter().collect();
        match view.sort {
            SatSort::Name => sats.sort_by_key(|s| (s.gnss, s.sv)),
            SatSort::Elevation => sats.sort_by(|a, b| b.elev.unwrap_or(-99.0).total_cmp(&a.elev.unwrap_or(-99.0))),
            SatSort::Signal => sats.sort_by_key(|s| std::cmp::Reverse(s.cno)),
        }
        egui::Grid::new("sats").num_columns(7).striped(true).show(ui, |ui| {
            for h in ["Sat", "Azimut", "Elevation", "C/N0", "verwendet", "RTCM", "Gesundheit"] {
                ui.strong(h);
            }
            ui.end_row();
            for s in sats {
                ui.colored_label(system(s.gnss).2, sat_name(s.gnss, s.sv));
                ui.monospace(format!("{:.0}°", s.azim));
                ui.monospace(format!("{}°", opt(s.elev, 0)));
                ui.monospace(s.cno.to_string());
                ui.label(if s.used { "ja" } else { "–" });
                ui.label(if s.rtcm_used { "ja" } else { "–" });
                ui.label(["?", "gesund", "krank", "?"][usize::from(s.health.min(3))]);
                ui.end_row();
            }
        });
    });
}

fn dop_row(ui: &mut Ui, name: &str, n: Option<usize>, d: Option<Dop>) {
    ui.label(name);
    ui.monospace(n.map_or_else(String::new, |n| n.to_string()));
    for v in [
        d.map(|d| d.g),
        d.map(|d| d.p),
        d.map(|d| d.h),
        d.map(|d| d.v),
        d.map(|d| d.t),
    ] {
        ui.monospace(opt(v, 2));
    }
    match d {
        Some(d) => {
            let color = if d.g < 4.0 {
                GOOD
            } else if d.g < 6.0 {
                WARN
            } else {
                BAD
            };
            ui.colored_label(color, dop::rating(d.g));
        }
        None => {
            ui.weak("nicht berechenbar");
        }
    }
    ui.end_row();
}

pub fn show_dop(ui: &mut Ui, st: &State, anim: &mut DopAnim, s: &mut Settings) {
    ui.horizontal(|ui| {
        ui.label("Elevationsmaske");
        ui.add(
            egui::Slider::new(&mut s.display_mask, 0.0..=45.0)
                .suffix("°")
                .integer(),
        );
        ui.label("Uhrenmodell");
        ui.selectable_value(&mut s.clock_per_system, false, "gemeinsame Uhr");
        ui.selectable_value(&mut s.clock_per_system, true, "Uhr je System");
    });
    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        view_dop_anim::show(ui, st, anim, s);
        ui.separator();
        let calc = |sats: &[Sat], used| {
            let d = dop::directions(sats, s.display_mask, used);
            (d.len(), dop::compute(&d, s.clock_per_system))
        };
        egui::Grid::new("dop").num_columns(8).striped(true).spacing([18.0, 4.0]).show(ui, |ui| {
            for h in ["Auswahl", "Sat", "GDOP", "PDOP", "HDOP", "VDOP", "TDOP", "Bewertung"] {
                ui.strong(h);
            }
            ui.end_row();
            let (n, d) = calc(&st.sats, true);
            dop_row(ui, "berechnet: verwendete", Some(n), d);
            let (n, d) = calc(&st.sats, false);
            dop_row(ui, "berechnet: alle möglichen", Some(n), d);
            let systems: BTreeSet<u8> = st.sats.iter().map(|s| s.gnss).collect();
            for g in systems {
                let only: Vec<Sat> = st.sats.iter().filter(|s| s.gnss == g).cloned().collect();
                let (n, d) = calc(&only, false);
                dop_row(ui, &format!("nur {}", system(g).1), Some(n), d);
            }
            dop_row(ui, "Empfänger (NAV-DOP)", None, st.receiver_dop);
        });
        ui.small("Berechnung aus Azimut und Elevation (ganze Grad), ungewichtet. Der Empfänger kann ein anderes Uhrenmodell verwenden.");
        ui.separator();
        type Pick = fn(&crate::gnss::kern::model::HistorySample) -> Option<Dop>;
        for (title, pick) in [("Verlauf Empfänger", (|h| h.receiver_dop) as Pick), ("Verlauf berechnet (verwendete)", |h| h.computed_dop)] {
            let col = |f: fn(Dop) -> f64| st.history.iter().map(|h| pick(h).map(f)).collect::<Vec<_>>();
            plot(ui, title, &[
                ("GDOP", BAD, col(|d| d.g)),
                ("PDOP", BLUE, col(|d| d.p)),
                ("HDOP", GOOD, col(|d| d.h)),
                ("VDOP", PURPLE, col(|d| d.v)),
            ], 140.0);
        }
        ui.small("Richtwerte GDOP: unter 2 sehr gut, 2–4 gut, 4–6 ausreichend, über 6 ungünstig.");
    });
}
