//! Zweck:    Messmenü: Punktaufnahme, Streudiagramm, Ergebnis, Punktliste.
//! Hardware: simpleRTK2B (u-blox ZED-F9P)
//! Autor:    Johann Gerner
//!
//! Übernommen aus gnss-vermessung 0.4.0 (src/bin/gnss-gui/view_measure.rs), eigenständig gepflegt in Feldbuch-NT.

use crate::gnss::ansicht::app::Action;
use crate::gnss::ansicht::widgets::*;
use eframe::egui::{self, Align2, FontId, RichText, Sense, Stroke, Ui, vec2};
use crate::gnss::kern::export::{self, Stored};
use crate::gnss::kern::measure::Measurement;
use crate::gnss::kern::model::State;
use crate::gnss::kern::settings::{HeightMode, Settings};
use std::path::Path;

/// Ergebnis, das auf Speichern oder Verwerfen wartet.
pub struct Pending {
    pub point: export::Point,
    pub measurement: Measurement,
    pub sd: (f64, f64, f64),
    /// vorhandener Punkt gleicher Nummer
    pub existing: Option<Stored>,
}

pub struct MeasureView {
    pub nr: String,
    pub code: String,
    pub remark: String,
    pub pending: Option<Pending>,
    pub points: Vec<Stored>,
    pub points_error: Option<String>,
    pub filter: String,
}

impl Default for MeasureView {
    fn default() -> Self {
        MeasureView {
            nr: "1".into(),
            code: String::new(),
            remark: String::new(),
            pending: None,
            points: Vec::new(),
            points_error: None,
            filter: String::new(),
        }
    }
}

/// Abweichungen der Epochen vom Mittel in Millimetern.
fn scatter(ui: &mut Ui, m: &Measurement) {
    let size = 260.0;
    let (resp, painter) = ui.allocate_painter(vec2(size, size), Sense::hover());
    let c = resp.rect.center();
    let grid = ui.visuals().weak_text_color();
    let n = m.accepted.len().max(1) as f64;
    let me = m
        .accepted
        .iter()
        .map(|a| a.0.east - m.accepted[0].0.east)
        .sum::<f64>()
        / n;
    let mn = m
        .accepted
        .iter()
        .map(|a| a.0.north - m.accepted[0].0.north)
        .sum::<f64>()
        / n;
    let d: Vec<(f64, f64)> = m
        .accepted
        .iter()
        .map(|a| {
            (
                (a.0.east - m.accepted[0].0.east - me) * 1e3,
                (a.0.north - m.accepted[0].0.north - mn) * 1e3,
            )
        })
        .collect();
    let max = d.iter().map(|p| p.0.hypot(p.1)).fold(0.0, f64::max);
    let range = [5.0, 10.0, 20.0, 50.0, 100.0, 500.0]
        .into_iter()
        .find(|r| *r >= max)
        .unwrap_or(max.max(1.0));
    let r = size / 2.0 - 8.0;
    for f in [0.5, 1.0] {
        painter.circle_stroke(c, r * f, Stroke::new(0.5, grid));
    }
    painter.line_segment([c - vec2(r, 0.0), c + vec2(r, 0.0)], Stroke::new(0.5, grid));
    painter.line_segment([c - vec2(0.0, r), c + vec2(0.0, r)], Stroke::new(0.5, grid));
    painter.text(
        resp.rect.left_top(),
        Align2::LEFT_TOP,
        format!("Außenring {range:.0} mm, N oben"),
        FontId::proportional(11.0),
        grid,
    );
    let scale = f64::from(r) / range;
    for p in d {
        painter.circle_filled(
            c + vec2((p.0 * scale) as f32, (-p.1 * scale) as f32),
            3.0,
            BLUE,
        );
    }
}

/// Höhen der Epochen relativ zur ersten.
fn height_series(ui: &mut Ui, m: &Measurement) {
    let Some(first) = m.accepted.first() else {
        return;
    };
    let min = m
        .accepted
        .iter()
        .map(|a| a.0.h - first.0.h)
        .fold(0.0, f64::min);
    let data = m
        .accepted
        .iter()
        .map(|a| Some((a.0.h - first.0.h - min) * 1e3))
        .collect();
    plot(
        ui,
        "Höhe über dem tiefsten Wert [mm]",
        &[("Δh", PURPLE, data)],
        80.0,
    );
}

fn progress(ui: &mut Ui, m: &Measurement, actions: &mut Vec<Action>) {
    let by_count = m.accepted.len() as f32 / m.criteria.epochs.max(1) as f32;
    let by_time = if m.criteria.min_seconds > 0.0 {
        (m.seconds() / m.criteria.min_seconds) as f32
    } else {
        1.0
    };
    ui.add(
        egui::ProgressBar::new(by_count.min(by_time).min(1.0)).text(format!(
            "{}/{} Epochen · {:.0} s · {} verworfen",
            m.accepted.len(),
            m.criteria.epochs,
            m.seconds(),
            m.rejected_total()
        )),
    );
    if let Some(r) = m.last_rejection {
        ui.colored_label(WARN, format!("letzte Epoche verworfen: {r}"));
    }
    if m.restarts > 0 {
        ui.colored_label(
            WARN,
            format!("{} Neubeginn(e) nach Fix-Verlust", m.restarts),
        );
    }
    if ui.button("Abbrechen (Esc)").clicked() {
        actions.push(Action::CancelMeasure);
    }
    ui.horizontal_top(|ui| {
        if !m.accepted.is_empty() {
            scatter(ui, m);
        }
        ui.vertical(|ui| {
            for (reason, n) in &m.rejected {
                ui.label(format!("{n} × {reason}"));
            }
        });
    });
    height_series(ui, m);
}

fn result(ui: &mut Ui, p: &Pending, actions: &mut Vec<Action>) {
    ui.heading(format!("Ergebnis Punkt {}", p.point.nr));
    ui.horizontal_top(|ui| {
        scatter(ui, &p.measurement);
        ui.vertical(|ui| {
            table(
                ui,
                "result",
                &[
                    ("Rechtswert X", format!("{:.3}", p.point.x)),
                    ("Hochwert Y", format!("{:.3}", p.point.y)),
                    ("Höhe Z", format!("{:.3}", p.point.z)),
                    ("s Ost [mm]", format!("{:.1}", p.sd.0 * 1e3)),
                    ("s Nord [mm]", format!("{:.1}", p.sd.1 * 1e3)),
                    ("s Höhe [mm]", format!("{:.1}", p.sd.2 * 1e3)),
                    (
                        "Epochen / verworfen",
                        format!(
                            "{} / {}",
                            p.measurement.accepted.len(),
                            p.measurement.rejected_total()
                        ),
                    ),
                    ("Dauer [s]", format!("{:.0}", p.measurement.seconds())),
                ],
            );
            ui.small("Streuung innerhalb dieser Belegung, keine äußere Genauigkeit.");
            if let Some(e) = &p.existing {
                let (dx, dy) = (p.point.x - e.x, p.point.y - e.y);
                ui.add_space(6.0);
                ui.colored_label(
                    WARN,
                    format!(
                        "Punkt {} ist bereits gespeichert. Vergleich Messung − Bestand:",
                        e.nr
                    ),
                );
                ui.monospace(format!(
                    "ΔX {:+.3}  ΔY {:+.3}  Lage {:.3} m",
                    dx,
                    dy,
                    dx.hypot(dy)
                ));
                if let Some(z) = e.z {
                    ui.monospace(format!("ΔZ {:+.3} m", p.point.z - z));
                }
            }
        });
    });
    height_series(ui, &p.measurement);
    ui.horizontal(|ui| {
        let label = if p.existing.is_some() {
            "Vorhandenen Punkt ersetzen"
        } else {
            "Speichern (Enter)"
        };
        if ui.button(RichText::new(label).strong()).clicked() {
            actions.push(Action::SavePoint {
                replace: p.existing.is_some(),
            });
        }
        if ui.button("Wiederholen").clicked() {
            actions.push(Action::RepeatPoint);
        }
        if ui.button("Verwerfen").clicked() {
            actions.push(Action::DiscardPoint);
        }
    });
}

pub fn show(
    ui: &mut Ui,
    st: &State,
    v: &mut MeasureView,
    s: &mut Settings,
    connected: bool,
    actions: &mut Vec<Action>,
) {
    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        let files = export::files(Path::new(&s.output));
        ui.horizontal_wrapped(|ui| {
            ui.label("Punktdatei:");
            ui.monospace(files.csv.display().to_string());
        });
        ui.horizontal(|ui| {
            if ui.button("Vorhandene öffnen …").clicked() {
                actions.push(Action::ChooseOutput { new: false });
            }
            if ui.button("Neue Datei …").clicked() {
                actions.push(Action::ChooseOutput { new: true });
            }
            ui.weak(format!("parallel: {}", files.kor.file_name().unwrap_or_default().to_string_lossy()));
        });
        match s.height_mode {
            _ if s.sapos_transform => {
                match st.solution.as_ref().map(|sol| {
                    let geoid_sep = st.gga.as_ref().map(|g| g.0.geoid_sep);
                    st.ntrip.content.transform.height(sol.lat, sol.lon, geoid_sep)
                }) {
                    Some(Ok(c)) => ui.label(format!("Höhenbezug: DHHN2016 aus SAPOS-Transformation (ζ = {:.3} m)", c.zeta)),
                    Some(Err(why)) => ui.colored_label(BAD, format!("Höhenbezug SAPOS nicht verfügbar: {why}")),
                    None => ui.colored_label(WARN, "Höhenbezug SAPOS: noch keine Position"),
                };
            }
            HeightMode::Ellipsoidal => {
                ui.colored_label(WARN, "Höhenbezug: ellipsoidisch (GRS80), nicht DHHN2016. Einstellung unter Konfiguration → Antenne und Höhe.");
            }
            HeightMode::Grid => {
                ui.label("Höhenbezug: DHHN2016 über Quasigeoid-Gitter");
            }
            HeightMode::Constant => {
                ui.label(format!("Höhenbezug: DHHN2016 mit konstanter Höhenanomalie {:.3} m", s.zeta));
            }
        }
        ui.separator();
        egui::Grid::new("point").num_columns(2).show(ui, |ui| {
            ui.label("Punktnummer");
            ui.add(egui::TextEdit::singleline(&mut v.nr).desired_width(160.0));
            ui.end_row();
            ui.label("Code");
            ui.add(egui::TextEdit::singleline(&mut v.code).desired_width(160.0));
            ui.end_row();
            ui.label("Bemerkung");
            ui.add(egui::TextEdit::singleline(&mut v.remark).desired_width(320.0));
            ui.end_row();
            ui.label("Stabhöhe [m]");
            number(ui, &mut s.pole_height, 0.0..=10.0, 0.001);
            ui.end_row();
            ui.label("Antennenoffset ARP → Phasenzentrum [m]");
            number(ui, &mut s.antenna_offset, -1.0..=1.0, 0.001);
            ui.end_row();
            ui.label("Epochen / Mindestdauer [s]");
            ui.horizontal(|ui| {
                number(ui, &mut s.epochs, 2..=3600, 1.0);
                number(ui, &mut s.min_seconds, 0.0..=3600.0, 1.0);
            });
            ui.end_row();
        });
        ui.separator();
        if let Some(p) = &v.pending {
            result(ui, p, actions);
        } else if let Some(m) = &st.measurement {
            progress(ui, m, actions);
        } else {
            let known = v.points.iter().any(|p| p.nr.eq_ignore_ascii_case(&export::clean(&v.nr)));
            if known {
                ui.colored_label(WARN, "Diese Punktnummer ist vorhanden: Das Ergebnis wird als Kontrolle verglichen.");
            }
            // Startbedingungen anzeigen
            let blocker = match (&st.solution, connected) {
                (_, false) => Some("nicht verbunden"),
                (None, _) => Some("keine Positionsdaten"),
                _ if st.is_stale() => Some("Daten veraltet"),
                (Some(sol), _) => s.criteria().check(sol),
            };
            match blocker {
                Some(b) => ui.colored_label(WARN, format!("Bedingungen derzeit nicht erfüllt: {b}")),
                None => ui.colored_label(GOOD, "Bedingungen erfüllt"),
            };
            let enabled = connected && !v.nr.trim().is_empty();
            if ui.add_enabled(enabled, egui::Button::new(RichText::new("▶ Punkt messen (Leertaste)").size(22.0))).clicked() {
                actions.push(Action::StartMeasure);
            }
        }
        ui.separator();
        ui.horizontal(|ui| {
            ui.strong(format!("Punktliste ({})", v.points.len()));
            ui.add(egui::TextEdit::singleline(&mut v.filter).hint_text("Suchen").desired_width(140.0));
            if ui.button("Neu laden").clicked() {
                actions.push(Action::ReloadPoints);
            }
        });
        if let Some(e) = &v.points_error {
            ui.colored_label(BAD, e);
        }
        let needle = v.filter.to_lowercase();
        egui::Grid::new("points").num_columns(5).striped(true).spacing([20.0, 3.0]).show(ui, |ui| {
            for h in ["PunktNr", "X", "Y", "Z", "Code"] {
                ui.strong(h);
            }
            ui.end_row();
            // neueste zuerst
            for p in v.points.iter().rev().filter(|p| needle.is_empty() || p.nr.to_lowercase().contains(&needle) || p.code.to_lowercase().contains(&needle)) {
                ui.monospace(&p.nr);
                ui.monospace(format!("{:.3}", p.x));
                ui.monospace(format!("{:.3}", p.y));
                ui.monospace(opt(p.z, 3));
                ui.monospace(&p.code);
                ui.end_row();
            }
        });
    });
}
