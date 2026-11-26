//! Zweck:    Ausgabefenster für die unveränderten Daten des Empfängers.
//! Hardware: simpleRTK2B (u-blox ZED-F9P)
//! Autor:    Johann Gerner
//!
//! Übernommen aus gnss-vermessung 0.4.0 (src/bin/gnss-gui/view_raw.rs), eigenständig gepflegt in Feldbuch-NT.

use crate::gnss::ansicht::app::Action;
use crate::gnss::ansicht::view_ephem::{self, EphemView};
use eframe::egui::{self, Ui};
use crate::gnss::kern::model::{RawEntry, RawKind, State};
use crate::gnss::kern::ubxmsg;

/// Bytes je Zeile in der Hex-Darstellung
const HEX_WIDTH: usize = 16;
/// in der Listenzeile gezeigte Bytes
const PREVIEW_BYTES: usize = 40;

#[derive(Default, PartialEq, Clone, Copy)]
pub enum RawTab {
    #[default]
    Lines,
    Hex,
    Stats,
    Ephemerides,
}

#[derive(Default)]
pub struct RawView {
    tab: RawTab,
    kind: Option<RawKind>,
    search: String,
    /// Standbild bei angehaltener Anzeige
    paused: Option<Vec<RawEntry>>,
    selected: Option<u64>,
    ephem: EphemView,
}

fn hex(data: &[u8]) -> String {
    data.iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn content(e: &RawEntry) -> String {
    if e.kind == RawKind::Nmea {
        return String::from_utf8_lossy(&e.data).into_owned();
    }
    let more = if e.data.len() > PREVIEW_BYTES {
        " …"
    } else {
        ""
    };
    format!(
        "[{:>4} B] {}{more}",
        e.data.len(),
        hex(&e.data[..e.data.len().min(PREVIEW_BYTES)])
    )
}

fn line(e: &RawEntry) -> String {
    format!("{}  {:<14} {}", e.time, e.name, content(e))
}

fn hexdump(data: &[u8]) -> Vec<String> {
    data.chunks(HEX_WIDTH)
        .enumerate()
        .map(|(i, c)| {
            let ascii: String = c
                .iter()
                .map(|&b| {
                    if (0x20..0x7F).contains(&b) {
                        b as char
                    } else {
                        '.'
                    }
                })
                .collect();
            format!("{:04X}  {:<47}  {ascii}", i * HEX_WIDTH, hex(c))
        })
        .collect()
}

/// Vollständiger Inhalt einer Meldung samt dekodierter Felder.
fn details(e: &RawEntry) -> String {
    let mut out = format!("{}  {}  {} Byte\n", e.time, e.name, e.data.len());
    if e.kind == RawKind::Ubx && e.data.len() >= 8 {
        match ubxmsg::decode(e.data[2], e.data[3], &e.data[6..e.data.len() - 2]) {
            Some(m) => out.push_str(&format!("{m:#?}\n")),
            None => out.push_str("(keine Dekodierung für diese Nachricht)\n"),
        }
    }
    if e.kind == RawKind::Nmea {
        out.push_str(&String::from_utf8_lossy(&e.data));
        out.push('\n');
    }
    out.push_str(&hexdump(&e.data).join("\n"));
    out
}

pub fn show(
    ui: &mut Ui,
    st: &State,
    v: &mut RawView,
    recording: Option<(String, u64)>,
    actions: &mut Vec<Action>,
) {
    ui.horizontal_wrapped(|ui| {
        ui.selectable_value(&mut v.tab, RawTab::Lines, "Meldungen");
        ui.selectable_value(&mut v.tab, RawTab::Hex, "Hex");
        ui.selectable_value(&mut v.tab, RawTab::Stats, "Statistik");
        ui.selectable_value(&mut v.tab, RawTab::Ephemerides, "Ephemeriden");
        ui.separator();
        for (k, n) in [
            (None, "Alles"),
            (Some(RawKind::Nmea), "NMEA"),
            (Some(RawKind::Ubx), "UBX"),
            (Some(RawKind::Rtcm), "RTCM"),
            (Some(RawKind::Junk), "Fehler"),
        ] {
            ui.selectable_value(&mut v.kind, k, n);
        }
        ui.add(
            egui::TextEdit::singleline(&mut v.search)
                .hint_text("Suchtext")
                .desired_width(140.0),
        );
        let mut paused = v.paused.is_some();
        if ui.toggle_value(&mut paused, "⏸ Anhalten").changed() {
            v.paused = paused.then(|| st.raw.entries.iter().cloned().collect());
        }
        if ui.button("Löschen").clicked() {
            v.paused = None;
            v.selected = None;
            actions.push(Action::ClearRaw);
        }
    });
    ui.horizontal(|ui| match &recording {
        Some((path, bytes)) => {
            if ui.button("⏹ Aufzeichnung beenden").clicked() {
                actions.push(Action::StopRecording);
            }
            ui.label(format!("● {path}  ({:.1} kB)", *bytes as f64 / 1024.0));
        }
        None => {
            if ui.button("⏺ In Datei aufzeichnen …").clicked() {
                actions.push(Action::StartRecording);
            }
        }
    });
    ui.separator();

    if v.tab == RawTab::Ephemerides {
        view_ephem::show(ui, st, &mut v.ephem);
        return;
    }
    if v.tab == RawTab::Stats {
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                ui.label(format!(
                    "Vom Empfänger gelesen: {:.1} kB",
                    st.bytes_received as f64 / 1024.0
                ));
                egui::Grid::new("rawstats")
                    .num_columns(5)
                    .striped(true)
                    .spacing([20.0, 3.0])
                    .show(ui, |ui| {
                        for h in ["Meldung", "Anzahl", "Byte", "je Sekunde", "zuletzt vor"] {
                            ui.strong(h);
                        }
                        ui.end_row();
                        for (name, s) in &st.raw.stats {
                            let rate = match (s.first, s.last) {
                                (Some(a), Some(b)) if s.count > 1 && b > a => {
                                    Some((s.count - 1) as f64 / (b - a).as_secs_f64())
                                }
                                _ => None,
                            };
                            ui.monospace(name);
                            ui.monospace(s.count.to_string());
                            ui.monospace(s.bytes.to_string());
                            ui.monospace(crate::gnss::ansicht::widgets::opt(rate, 2));
                            ui.monospace(
                                s.last.map_or("–".into(), |t| {
                                    format!("{} s", t.elapsed().as_secs())
                                }),
                            );
                            ui.end_row();
                        }
                    });
            });
        return;
    }

    let needle = v.search.to_lowercase();
    let matches = |e: &&RawEntry| {
        v.kind.is_none_or(|k| e.kind == k)
            && (needle.is_empty() || line(e).to_lowercase().contains(&needle))
    };
    let list: Vec<&RawEntry> = match &v.paused {
        Some(p) => p.iter().filter(matches).collect(),
        None => st.raw.entries.iter().filter(matches).collect(),
    };
    let row = ui.text_style_height(&egui::TextStyle::Monospace);

    if v.tab == RawTab::Hex {
        let rows: Vec<String> = list
            .iter()
            .flat_map(|e| {
                std::iter::once(format!("— {} {}", e.time, e.name)).chain(hexdump(&e.data))
            })
            .collect();
        egui::ScrollArea::both()
            .id_salt("hex")
            .auto_shrink(false)
            .stick_to_bottom(true)
            .show_rows(ui, row, rows.len(), |ui, range| {
                for l in &rows[range] {
                    ui.add(egui::Label::new(egui::RichText::new(l).monospace()).extend());
                }
            });
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
    let selected = v
        .selected
        .and_then(|seq| list.iter().find(|e| e.seq == seq).map(|e| details(e)));
    if let Some(text) = &selected {
        egui::Panel::bottom("rawdetail")
            .resizable(true)
            .default_size(230.0)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.strong("Einzelheiten");
                    if ui.button("Kopieren").clicked() {
                        ui.ctx().copy_text(text.clone());
                    }
                    if ui.button("Schließen").clicked() {
                        v.selected = None;
                    }
                });
                egui::ScrollArea::both()
                    .id_salt("detail")
                    .auto_shrink(false)
                    .show(ui, |ui| {
                        ui.add(egui::Label::new(egui::RichText::new(text).monospace()).extend());
                    });
            });
    }
    egui::ScrollArea::both()
        .id_salt("raw")
        .auto_shrink(false)
        .stick_to_bottom(true)
        .show_rows(ui, row, list.len(), |ui, range| {
            for e in &list[range] {
                let mut text = egui::RichText::new(line(e)).monospace();
                if e.kind == RawKind::Junk {
                    text = text.color(crate::gnss::ansicht::widgets::BAD);
                }
                let chosen = v.selected == Some(e.seq);
                if ui
                    .add(
                        egui::Button::selectable(chosen, text)
                            .wrap_mode(egui::TextWrapMode::Extend),
                    )
                    .clicked()
                {
                    v.selected = Some(e.seq);
                }
            }
        });
}
