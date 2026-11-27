//! Zweck:    Überwachung der Korrekturdaten (NTRIP/RTCM).
//! Hardware: simpleRTK2B (u-blox ZED-F9P), SAPOS HEPS
//! Autor:    Johann Gerner
//!
//! Übernommen aus gnss-vermessung 0.4.0 (src/bin/gnss-gui/view_ntrip.rs), eigenständig gepflegt in Feldbuch-NT.

use crate::gnss::ansicht::app::Action;
use crate::gnss::ansicht::widgets::*;
use eframe::egui::{self, Ui};
use crate::gnss::kern::config::UTM_ZONE;
use crate::gnss::kern::geo::{Geographic, ecef_to_geo, geo_to_ecef, geo_to_utm};
use crate::gnss::kern::model::{CommEntry, NtripPhase, State};
use crate::gnss::kern::nmea::{self, Sentence};
use crate::gnss::kern::ntrip::Direction;
use crate::gnss::kern::rtcm_decode::message_name;
use crate::gnss::kern::settings::Settings;
use crate::gnss::kern::transform;
use std::collections::BTreeSet;

#[derive(Default, PartialEq, Clone, Copy)]
enum NtripTab {
    #[default]
    Overview,
    Content,
    Communication,
}

pub struct NtripView {
    tab: NtripTab,
    /// auch einzelne Datenpakete im Protokoll zeigen
    data: bool,
    search: String,
    paused: Option<Vec<CommEntry>>,
}

impl Default for NtripView {
    fn default() -> Self {
        NtripView {
            tab: NtripTab::default(),
            data: true,
            search: String::new(),
            paused: None,
        }
    }
}

fn arrow(d: Direction) -> &'static str {
    match d {
        Direction::Sent => "→",
        Direction::Received => "←",
        Direction::Info => "·",
    }
}

/// Bedeutung einer gesendeten GGA-Zeile.
fn explain(e: &CommEntry) -> String {
    if e.direction == Direction::Sent {
        if let Ok(Sentence::Gga(g)) = nmea::parse(&e.text) {
            return format!(
                "{}\n{:17}Position an den Dienst: {:.7}° {:.7}°, {}, {} Satelliten",
                e.text,
                "",
                g.lat,
                g.lon,
                g.quality.label(),
                g.sats
            );
        }
    }
    e.text.clone()
}

fn line(e: &CommEntry) -> String {
    format!("{}  {} {}", e.time, arrow(e.direction), explain(e))
}

/// Vollständiges Protokoll mit Zusammenfassung zum Speichern.
pub fn log_text(st: &State) -> String {
    let mut out = vec![
        "SAPOS-Protokoll (Zugangsdaten werden nicht aufgezeichnet)".to_string(),
        String::new(),
    ];
    out.extend(st.ntrip.content.summary());
    out.push(String::new());
    out.extend(st.comm.iter().map(line));
    out.join("\r\n").replace("\n", "\r\n").replace("\r\r", "\r")
}

/// Transformationsnachrichten mit Schalter und Prüfergebnis.
fn transformation(ui: &mut Ui, st: &State, settings: &mut Settings) {
    let t = &st.ntrip.content.transform;
    ui.strong("Transformation (RTCM 1021 / 1023 / 1025)");
    let mut on = settings.sapos_transform;
    let label = egui::RichText::new(if on { "■ Höhe DHHN2016 aus SAPOS nutzen" } else { "□ Höhe DHHN2016 aus SAPOS nutzen" }).strong();
    if ui.toggle_value(&mut on, label).on_hover_text("Ein: H = h − ζ aus dem Gitter 1023; aus: Einstellung unter Konfiguration → Antenne und Höhe").changed() {
        settings.sapos_transform = on;
    }
    if t.helmert.is_none() && t.grid.is_none() && t.projection.is_none() {
        ui.weak("Noch keine Transformationsnachrichten empfangen.");
        return;
    }
    let age = |i: &std::time::Instant| format!("vor {} s", i.elapsed().as_secs());
    if let Some((h, at)) = &t.helmert {
        let rows = vec![
            ("Quelle → Ziel", format!("{} → {}", h.source.trim(), h.target.trim())),
            ("System / Platte", format!("{} / {}", h.system_id, h.plate)),
            ("Höhenindikator", format!("{} – {}", h.height_indicator, transform::height_indicator_text(h.height_indicator))),
            ("Rechenverfahren (DF150)", h.computation.to_string()),
            ("Datumsänderung", if h.is_identity() { "keine (alle Parameter null)".into() } else {
                format!("dX {:.3} dY {:.3} dZ {:.3} m, Drehungen {:?}, Maßstab {} (Rohwerte)", h.dx, h.dy, h.dz, h.rotation_raw, h.scale_raw)
            }),
            ("Gültigkeitsbereich", format!("{:.4}° {:.4}°, Ausdehnung {:.4}° × {:.4}°", h.validity[0], h.validity[1], h.validity[2], h.validity[3])),
            ("Ellipsoid Quelle / Ziel a", format!("{:.3} m / {:.3} m", h.axes[0], h.axes[2])),
            ("empfangen", age(at)),
        ];
        table(ui, "t1021", &rows);
    }
    if let Some((g, at)) = &t.grid {
        let (lo, hi) = g.residual_range();
        let rows = vec![
            ("Gitterursprung", format!("{:.6}° {:.6}° (System {})", g.lat0, g.lon0, g.system_id)),
            ("Abstand Δφ × Δλ", format!("{:.1}\" × {:.1}\"", g.dlat * 3600.0, g.dlon * 3600.0)),
            ("mittleres ΔH", format!("{:.3} m", g.mean_dh)),
            ("Höhenresiduen δh", format!("{:+.3} … {:+.3} m", lo, hi)),
            ("Lageresiduen", if g.has_horizontal_residuals() { "vorhanden (nicht angewendet)".into() } else { "keine".into() }),
            ("Interpolation Lage / Höhe", format!("{} / {}", g.interpolation[0], g.interpolation[1])),
            ("Güte Lage / Höhe", format!("{} / {}", g.quality[0], g.quality[1])),
            ("Stand (MJD)", g.mjd.to_string()),
            ("empfangen", age(at)),
        ];
        table(ui, "t1023", &rows);
    }
    if let Some((p, at)) = &t.projection {
        table(ui, "t1025", &[
            ("Abbildung", format!("Typ {}, Mittelmeridian {:.6}°, Breite {:.6}°", p.kind, p.lon_origin, p.lat_origin)),
            ("False Easting / Northing", format!("{:.3} / {:.3} m", p.false_easting, p.false_northing)),
            ("empfangen", age(at)),
        ]);
        if (p.lon_origin - 9.0).abs() > 1e-6 {
            ui.colored_label(WARN, "Abbildung ist nicht UTM-Zone 32 (Mittelmeridian 9°).");
        }
    }
    let geoid_sep = st.gga.as_ref().map(|g| g.0.geoid_sep);
    match &st.solution {
        Some(sol) => match t.height(sol.lat, sol.lon, geoid_sep) {
            Ok(c) => {
                ui.colored_label(GOOD, format!(
                    "Höhenanomalie hier ζ = {:.3} m → H = {:.3} m (h {:.3} m). Unsicherheit aus der Gitterauslegung {:.1} mm.",
                    c.zeta, sol.h - c.zeta, sol.h, c.spread * 1000.0
                ));
                if let Some(n) = geoid_sep {
                    ui.small(format!("Zum Vergleich Modell des Empfängers: {n:.3} m (Abweichung {:+.3} m)", c.zeta - n));
                }
            }
            Err(why) => {
                ui.colored_label(BAD, format!("Nicht verwendbar: {why}"));
            }
        },
        None => {
            ui.weak("Keine Position für die Auswertung.");
        }
    }
    ui.small("Vor der ersten Verwendung auf einem Höhenfestpunkt prüfen.");
}

fn content(ui: &mut Ui, st: &State, settings: &mut Settings) {
    let c = &st.ntrip.content;
    egui::ScrollArea::vertical()
        .auto_shrink(false)
        .show(ui, |ui| {
            ui.strong("Was überträgt der Dienst?");
            for l in c.summary() {
                ui.label(format!("• {l}"));
            }
            ui.separator();
            transformation(ui, st, settings);
            for (title, ecef, id) in [
                c.station.as_ref().map(|s| {
                    (
                        if s.virtual_station {
                            "Virtuelle Referenzstation"
                        } else {
                            "Referenzstation"
                        },
                        s.ecef,
                        s.id,
                    )
                }),
                c.physical
                    .as_ref()
                    .map(|p| ("Physische Referenzstation", p.ecef, p.id)),
            ]
            .into_iter()
            .flatten()
            {
                ui.separator();
                ui.strong(format!("{title} {id}"));
                let g = ecef_to_geo(ecef);
                let u = geo_to_utm(&g, UTM_ZONE);
                let away = st.solution.as_ref().map(|s| {
                    let r = geo_to_ecef(&Geographic {
                        lat: s.lat,
                        lon: s.lon,
                        h: s.h,
                    });
                    (0..3).map(|i| (ecef[i] - r[i]).powi(2)).sum::<f64>().sqrt()
                });
                table(
                    ui,
                    title,
                    &[
                        (
                            "X / Y / Z [m]",
                            format!("{:.4}  {:.4}  {:.4}", ecef[0], ecef[1], ecef[2]),
                        ),
                        ("Breite / Länge", format!("{:.8}°  {:.8}°", g.lat, g.lon)),
                        (
                            "UTM32 Ost / Nord",
                            format!("{:.3}  {:.3}", u.east_prefixed(), u.north),
                        ),
                        ("Höhe ellipsoidisch [m]", format!("{:.3}", g.h)),
                        (
                            "Abstand zum Empfänger",
                            away.map_or("–".into(), |d| format!("{:.3} km", d / 1000.0)),
                        ),
                    ],
                );
            }
            if let Some(h) = c.station.as_ref().and_then(|s| s.antenna_height) {
                ui.label(format!("Antennenhöhe der Station: {h:.4} m"));
            }
            if !c.observations.is_empty() {
                ui.separator();
                ui.strong("Beobachtungsdaten je System (letzte Nachricht)");
                egui::Grid::new("msm")
                    .num_columns(5)
                    .striped(true)
                    .spacing([16.0, 4.0])
                    .show(ui, |ui| {
                        for h in [
                            "System",
                            "Nachricht",
                            "Anzahl",
                            "Signale (RINEX)",
                            "Satelliten",
                        ] {
                            ui.strong(h);
                        }
                        ui.end_row();
                        for (name, o) in &c.observations {
                            ui.label(*name);
                            ui.monospace(o.message.to_string())
                                .on_hover_text(format!("vor {} s", o.at.elapsed().as_secs()));
                            ui.monospace(o.satellites.len().to_string());
                            ui.monospace(o.signals.join(" "));
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(o.satellites.join(" ")).monospace(),
                                )
                                .wrap(),
                            );
                            ui.end_row();
                        }
                    });
            }
            if !c.equipment.is_empty() {
                ui.separator();
                ui.strong("Antenne und Empfänger der Station");
                let rows: Vec<(&str, String)> =
                    c.equipment.iter().map(|(k, v)| (*k, v.clone())).collect();
                table(ui, "equipment", &rows);
            }
            if let Some(l) = c.leap_seconds {
                ui.label(format!("Schaltsekunden laut Dienst: {l}"));
            }
            if !c.intervals.is_empty() {
                let list: Vec<String> = c
                    .intervals
                    .iter()
                    .map(|(n, s)| format!("{n} alle {s:.1} s"))
                    .collect();
                ui.label(format!("Angekündigte Intervalle: {}", list.join(", ")));
            }
            if !c.texts.is_empty() {
                ui.separator();
                ui.strong("Textmeldungen des Dienstes");
                for t in &c.texts {
                    ui.label(t);
                }
            }
        });
}

fn communication(ui: &mut Ui, st: &State, v: &mut NtripView, actions: &mut Vec<Action>) {
    ui.horizontal_wrapped(|ui| {
        ui.checkbox(&mut v.data, "Datenpakete");
        ui.add(
            egui::TextEdit::singleline(&mut v.search)
                .hint_text("Suchtext")
                .desired_width(140.0),
        );
        let mut paused = v.paused.is_some();
        if ui.toggle_value(&mut paused, "⏸ Anhalten").changed() {
            v.paused = paused.then(|| st.comm.iter().cloned().collect());
        }
        if ui.button("Löschen").clicked() {
            v.paused = None;
            actions.push(Action::ClearComm);
        }
        if ui.button("Protokoll speichern …").clicked() {
            actions.push(Action::SaveComm);
        }
        match &st.rtcm_file {
            Some((_, path, size)) => {
                if ui
                    .button("⏹ RTCM-Mitschnitt beenden")
                    .on_hover_text(path.display().to_string())
                    .clicked()
                {
                    actions.push(Action::StopRtcmRecording);
                }
                ui.label(format!("● {:.1} kB", *size as f64 / 1024.0));
            }
            None => {
                if ui.button("⏺ RTCM-Rohdaten aufzeichnen …").clicked() {
                    actions.push(Action::StartRtcmRecording);
                }
            }
        }
    });
    ui.small("→ gesendet   ← empfangen   · Hinweis. Zugangsdaten werden nicht angezeigt.");
    let needle = v.search.to_lowercase();
    let show = |e: &&CommEntry| {
        (v.data || !e.data) && (needle.is_empty() || line(e).to_lowercase().contains(&needle))
    };
    let list: Vec<&CommEntry> = match &v.paused {
        Some(p) => p.iter().filter(show).collect(),
        None => st.comm.iter().filter(show).collect(),
    };
    if list.is_empty() {
        ui.weak("Noch keine Einträge. Das Protokoll beginnt mit der SAPOS-Verbindung.");
        return;
    }
    if ui.button("Angezeigte Zeilen kopieren").clicked() {
        ui.ctx().copy_text(
            list.iter()
                .map(|e| line(e))
                .collect::<Vec<_>>()
                .join("\r\n"),
        );
    }
    // jede Textzeile einzeln, damit nur sichtbare Zeilen gezeichnet werden
    let rows: Vec<(Direction, String)> = list
        .iter()
        .flat_map(|e| {
            line(e)
                .split('\n')
                .map(|l| (e.direction, l.to_string()))
                .collect::<Vec<_>>()
        })
        .collect();
    let height = ui.text_style_height(&egui::TextStyle::Monospace);
    egui::ScrollArea::both()
        .id_salt("comm")
        .auto_shrink(false)
        .stick_to_bottom(true)
        .show_rows(ui, height, rows.len(), |ui, range| {
            for (direction, text) in &rows[range] {
                let mut t = egui::RichText::new(text).monospace();
                match direction {
                    Direction::Sent => t = t.color(BLUE),
                    Direction::Info => t = t.color(WARN),
                    Direction::Received => {}
                }
                ui.add(egui::Label::new(t).extend());
            }
        });
}

pub fn show(ui: &mut Ui, st: &State, v: &mut NtripView, settings: &mut Settings, running: bool, actions: &mut Vec<Action>) {
    let n = &st.ntrip;
    ui.horizontal(|ui| {
        // Button grün bei bestehender Verbindung, sonst rot
        let fill = if n.phase == NtripPhase::Active { GOOD } else { BAD };
        let caption = if running { "SAPOS trennen" } else { "SAPOS verbinden" };
        let button = egui::Button::new(egui::RichText::new(caption).color(egui::Color32::WHITE)).fill(fill);
        if ui.add(button).clicked() {
            actions.push(if running { Action::NtripStop } else { Action::NtripStart });
        }
        let color = match n.phase {
            NtripPhase::Active => GOOD,
            NtripPhase::Error | NtripPhase::AuthError => BAD,
            NtripPhase::Off => STALE,
            _ => WARN,
        };
        ui.colored_label(color, format!("● {}", n.phase.label()));
        ui.separator();
        ui.selectable_value(&mut v.tab, NtripTab::Overview, "Übersicht");
        ui.selectable_value(&mut v.tab, NtripTab::Content, "Inhalt");
        ui.selectable_value(&mut v.tab, NtripTab::Communication, "Kommunikation");
    });
    match v.tab {
        NtripTab::Content => return content(ui, st, settings),
        NtripTab::Communication => return communication(ui, st, v, actions),
        NtripTab::Overview => {}
    }
    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        let age = |t: Option<std::time::Instant>| t.map_or("–".into(), |t| format!("{} s", t.elapsed().as_secs()));
        // Abstand zur Referenzstation (bei VRS nahe null)
        let distance = n.content.station.as_ref().map(|s| s.ecef).zip(st.solution.as_ref()).map(|(a, s)| {
            let r = geo_to_ecef(&Geographic { lat: s.lat, lon: s.lon, h: s.h });
            (0..3).map(|i| (a[i] - r[i]).powi(2)).sum::<f64>().sqrt()
        });
        table(ui, "ntrip", &[
            ("Mountpoint", n.mountpoint.clone()),
            ("verbunden seit", age(n.connected_since)),
            ("Datenmenge", format!("{:.1} kB", n.bytes as f64 / 1024.0)),
            ("Datenrate", format!("{:.0} Byte/s", n.rate)),
            ("letzte Daten vor", age(n.last_data)),
            ("Neuverbindungen", n.reconnects.to_string()),
            ("Prüfsummenfehler", n.scanner.crc_errors.to_string()),
            ("Referenzstation, Abstand", distance.map_or("–".into(), |d| format!("{:.3} km", d / 1000.0))),
            ("letzter Fehler", n.last_error.clone().unwrap_or_else(|| "–".into())),
        ]);
        if n.phase == NtripPhase::Active && n.last_data.is_none_or(|t| t.elapsed().as_secs() > 10) {
            ui.colored_label(WARN, "Verbunden, aber seit mehr als 10 s keine Daten.");
        }
        ui.separator();
        let types: BTreeSet<u16> = n.scanner.counts.keys().chain(st.rtcm_feedback.keys()).copied().collect();
        ui.strong("RTCM-Nachrichten");
        egui::Grid::new("rtcm").num_columns(7).striped(true).spacing([18.0, 3.0]).show(ui, |ui| {
            for h in ["Nr.", "Inhalt", "empfangen", "zuletzt vor", "verwendet", "nicht verw.", "CRC-Fehler"] {
                ui.strong(h);
            }
            ui.end_row();
            for t in &types {
                ui.monospace(t.to_string());
                ui.label(message_name(*t));
                ui.monospace(n.scanner.counts.get(t).map_or(0, |c| *c).to_string());
                ui.monospace(age(n.scanner.last_seen.get(t).copied()));
                let f = st.rtcm_feedback.get(t).cloned().unwrap_or_default();
                for v in [f.used, f.unused, f.crc_errors] {
                    ui.monospace(v.to_string());
                }
                ui.end_row();
            }
        });
        ui.small("„verwendet“ ist die Rückmeldung des Empfängers (RXM-RTCM).");
        if types.iter().any(|t| matches!(t, 1021 | 1023)) {
            ui.colored_label(BLUE, "Der Strom enthält Transformationsnachrichten 1021/1023 (noch nicht ausgewertet).");
        }
        ui.separator();
        plot(ui, "Datenrate [Byte/s]", &[("Rate", BLUE, st.history.iter().map(|h| Some(h.rate)).collect())], 90.0);
        plot(ui, "Korrekturalter [s]", &[("Alter", WARN, st.history.iter().map(|h| h.age).collect())], 90.0);
        ui.separator();
        ui.small("Hinweis: NTRIP überträgt Benutzername und Passwort unverschlüsselt.");
    });
}
