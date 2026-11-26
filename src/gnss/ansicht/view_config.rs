//! Zweck:    Konfigurationsmenü, Koordinatenwerkzeug und Kurzanleitung.
//! Hardware: simpleRTK2B (u-blox ZED-F9P)
//! Autor:    Johann Gerner
//!
//! Übernommen aus gnss-vermessung 0.4.0 (src/bin/gnss-gui/view_config.rs), eigenständig gepflegt in Feldbuch-NT.

use crate::gnss::ansicht::app::Action;
use crate::gnss::ansicht::widgets::*;
use eframe::egui::{self, Ui};
use crate::gnss::kern::config::UTM_ZONE;
use crate::gnss::kern::geo::{Geographic, Utm, geo_to_utm, utm_to_geo};
use crate::gnss::kern::model::State;
use crate::gnss::kern::settings::{HeightMode, Settings};
use crate::gnss::kern::ubx;
use crate::gnss::kern::version;

#[derive(Default, PartialEq, Clone, Copy)]
pub enum ConfigTab {
    #[default]
    Port,
    Sapos,
    Receiver,
    Measure,
    Antenna,
    Output,
    Display,
}

#[derive(Default)]
pub struct ConfigView {
    pub tab: ConfigTab,
    pub ports: Vec<(String, String)>,
    pub mountpoints: Vec<(String, String)>,
    pub sourcetable_status: String,
    pub show_password: bool,
}

/// Verfügbare Schnittstellen mit Gerätename.
pub fn list_ports() -> Vec<(String, String)> {
    serialport::available_ports()
        .unwrap_or_default()
        .into_iter()
        .map(|p| {
            let info = match p.port_type {
                serialport::SerialPortType::UsbPort(u) => u.product.unwrap_or_else(|| "USB".into()),
                serialport::SerialPortType::BluetoothPort => "Bluetooth".into(),
                _ => String::new(),
            };
            (p.port_name, info)
        })
        .collect()
}

fn grid(ui: &mut Ui, id: &str, add: impl FnOnce(&mut Ui)) {
    egui::Grid::new(id)
        .num_columns(2)
        .spacing([16.0, 6.0])
        .show(ui, add);
}

fn text(ui: &mut Ui, label: &str, value: &mut String) {
    ui.label(label);
    ui.add(egui::TextEdit::singleline(value).desired_width(320.0));
    ui.end_row();
}

fn num<N: egui::emath::Numeric>(
    ui: &mut Ui,
    label: &str,
    v: &mut N,
    range: std::ops::RangeInclusive<N>,
    speed: f64,
) {
    ui.label(label);
    number(ui, v, range, speed);
    ui.end_row();
}

fn check(ui: &mut Ui, label: &str, v: &mut bool) {
    ui.label(label);
    ui.checkbox(v, "");
    ui.end_row();
}

pub fn show(
    ui: &mut Ui,
    st: &State,
    v: &mut ConfigView,
    s: &mut Settings,
    password: &mut String,
    connected: bool,
    actions: &mut Vec<Action>,
) {
    ui.horizontal(|ui| {
        if ui.button("💾 Speichern").clicked() {
            actions.push(Action::SaveSettings);
        }
        if ui
            .button("Verwerfen")
            .on_hover_text("gespeicherte Werte neu laden")
            .clicked()
        {
            actions.push(Action::ReloadSettings);
        }
        if ui.button("Werkseinstellungen …").clicked() {
            actions.push(Action::AskDefaults);
        }
    });
    ui.horizontal_wrapped(|ui| {
        for (t, n) in [
            (ConfigTab::Port, "Schnittstelle"),
            (ConfigTab::Sapos, "SAPOS"),
            (ConfigTab::Receiver, "Empfänger"),
            (ConfigTab::Measure, "Messvorgaben"),
            (ConfigTab::Antenna, "Antenne und Höhe"),
            (ConfigTab::Output, "Ausgabe"),
            (ConfigTab::Display, "Darstellung"),
        ] {
            ui.selectable_value(&mut v.tab, t, n);
        }
    });
    ui.separator();
    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| match v.tab {
        ConfigTab::Port => {
            grid(ui, "port", |ui| {
                ui.label("Schnittstelle");
                ui.horizontal(|ui| {
                    let selected = if s.device.is_empty() { "wählen" } else { &s.device };
                    egui::ComboBox::from_id_salt("ports").selected_text(selected).width(280.0).show_ui(ui, |ui| {
                        for (name, info) in &v.ports {
                            ui.selectable_value(&mut s.device, name.clone(), format!("{name}  {info}"));
                        }
                    });
                    if ui.button("Aktualisieren").clicked() {
                        v.ports = list_ports();
                    }
                });
                ui.end_row();
                num(ui, "Baudrate", &mut s.baud, 9600..=921_600, 100.0);
                check(ui, "Beim Programmstart verbinden", &mut s.auto_connect);
                check(ui, "Nach Abbruch neu verbinden", &mut s.auto_reconnect);
                check(ui, "Beim Verbinden Empfänger einstellen (nur RAM)", &mut s.configure_on_connect);
            });
            if v.ports.is_empty() {
                ui.colored_label(WARN, "Keine Schnittstelle gefunden. Empfänger angeschlossen?");
            }
            ui.small("simpleRTK2B: USB-Buchse des ZED-F9P verwenden. Die Baudrate ist bei USB ohne Bedeutung.");
        }
        ConfigTab::Sapos => {
            grid(ui, "sapos", |ui| {
                text(ui, "Caster", &mut s.caster);
                num(ui, "Port", &mut s.caster_port, 1..=u16::MAX, 1.0);
                ui.label("Mountpoint");
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut s.mountpoint).desired_width(180.0));
                    if !v.mountpoints.is_empty() {
                        egui::ComboBox::from_id_salt("mounts").selected_text("aus Liste").width(150.0).show_ui(ui, |ui| {
                            for (name, info) in &v.mountpoints {
                                ui.selectable_value(&mut s.mountpoint, name.clone(), format!("{name}  {info}"));
                            }
                        });
                    }
                });
                ui.end_row();
                text(ui, "Benutzer", &mut s.user);
                ui.label("Passwort");
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(password).password(!v.show_password).desired_width(250.0));
                    ui.checkbox(&mut v.show_password, "anzeigen");
                });
                ui.end_row();
                check(ui, "NTRIP Rev1 statt Rev2", &mut s.rev1);
                num(ui, "GGA-Upload alle [s]", &mut s.gga_interval_s, 1..=60, 1.0);
                check(ui, "Beim Verbinden SAPOS starten", &mut s.ntrip_autostart);
            });
            ui.horizontal(|ui| {
                if ui.button("Verbindung testen / Mountpoints abrufen").clicked() {
                    actions.push(Action::FetchSourcetable);
                }
                ui.label(&v.sourcetable_status);
            });
            ui.small("Das Passwort wird beim Speichern in der Windows-Anmeldeinformationsverwaltung abgelegt, nicht in der Einstellungsdatei.");
        }
        ConfigTab::Receiver => {
            grid(ui, "receiver", |ui| {
                num(ui, "Messrate [ms]", &mut s.rate_ms, 100..=10_000, 10.0);
                ui.label("Dynamikmodell");
                let models = [(0u8, "tragbar"), (2, "stationär"), (3, "Fußgänger"), (4, "Fahrzeug")];
                let name = models.iter().find(|m| m.0 == s.dyn_model).map_or("anderes", |m| m.1);
                egui::ComboBox::from_id_salt("dyn").selected_text(name).show_ui(ui, |ui| {
                    for (id, n) in models {
                        ui.selectable_value(&mut s.dyn_model, id, n);
                    }
                });
                ui.end_row();
                num(ui, "Elevationsmaske [°]", &mut s.elevation_mask, 0..=45, 1.0);
            });
            ui.horizontal(|ui| {
                let enabled = connected && !st.config_running;
                if ui.add_enabled(enabled, egui::Button::new("Übertragen (bis zum Ausschalten)")).clicked() {
                    actions.push(Action::Configure { persist: false });
                }
                if ui.add_enabled(enabled, egui::Button::new("Übertragen und dauerhaft speichern")).clicked() {
                    actions.push(Action::Configure { persist: true });
                }
                if st.config_running {
                    ui.spinner();
                }
            });
            if !st.readback.is_empty() {
                ui.separator();
                ui.strong("Aus dem Empfänger gelesen");
                let wanted = [u64::from(s.rate_ms), u64::from(s.dyn_model), u64::from(s.elevation_mask), 3, 1];
                egui::Grid::new("readback").num_columns(3).striped(true).show(ui, |ui| {
                    for ((key, name), want) in ubx::READBACK_KEYS.iter().zip(wanted) {
                        let Some((_, value)) = st.readback.iter().find(|r| r.0 == *key) else { continue };
                        ui.label(*name);
                        ui.monospace(value.to_string());
                        if *value == want {
                            ui.colored_label(GOOD, "wie eingestellt");
                        } else {
                            ui.colored_label(WARN, format!("abweichend, Eingabe {want}"));
                        }
                        ui.end_row();
                    }
                });
            }
            if !st.config_log.is_empty() {
                ui.separator();
                ui.strong("Antworten des Empfängers");
                for (label, answer) in &st.config_log {
                    match answer {
                        Some(true) => ui.colored_label(GOOD, format!("ACK   {label}")),
                        Some(false) => ui.colored_label(BAD, format!("NAK   {label} (abgelehnt)")),
                        None => ui.colored_label(BAD, format!("–     {label} (keine Antwort)")),
                    };
                }
            }
        }
        ConfigTab::Measure => {
            grid(ui, "criteria", |ui| {
                num(ui, "Epochen je Punkt", &mut s.epochs, 2..=3600, 1.0);
                num(ui, "Mindestdauer [s]", &mut s.min_seconds, 0.0..=3600.0, 1.0);
                num(ui, "max. σ 2D [m]", &mut s.max_sigma_2d, 0.001..=10.0, 0.001);
                num(ui, "max. σ Höhe [m]", &mut s.max_sigma_h, 0.001..=10.0, 0.001);
                num(ui, "max. Korrekturalter [s]", &mut s.max_age, 0.5..=120.0, 0.5);
                num(ui, "max. PDOP", &mut s.max_pdop, 1.0..=20.0, 0.1);
                num(ui, "min. Satelliten", &mut s.min_sats, 4..=40, 1.0);
                check(ui, "Bei Fix-Verlust neu beginnen", &mut s.restart_on_fix_loss);
            });
            ui.small("Eine Epoche zählt nur bei RTK Fix mit vollständigen Genauigkeitsangaben. Änderungen gelten ab der nächsten Messung.");
        }
        ConfigTab::Antenna => {
            grid(ui, "antenna", |ui| {
                num(ui, "Stabhöhe [m]", &mut s.pole_height, 0.0..=10.0, 0.001);
                num(ui, "Antennenoffset ARP → Phasenzentrum [m]", &mut s.antenna_offset, -1.0..=1.0, 0.001);
                ui.label("Höhenbezug");
                ui.vertical(|ui| {
                    ui.checkbox(&mut s.sapos_transform, "DHHN2016 aus SAPOS-Transformationsnachrichten (RTCM 1023), hat Vorrang");
                    ui.add_enabled_ui(!s.sapos_transform, |ui| {
                    ui.radio_value(&mut s.height_mode, HeightMode::Ellipsoidal, "ellipsoidisch (GRS80)");
                    ui.radio_value(&mut s.height_mode, HeightMode::Grid, "DHHN2016 über Quasigeoid-Gitter");
                    ui.radio_value(&mut s.height_mode, HeightMode::Constant, "DHHN2016 mit konstanter Höhenanomalie");
                    });
                });
                ui.end_row();
                match s.height_mode {
                    HeightMode::Grid => {
                        ui.label("Gitterdatei (lat lon zeta)");
                        ui.horizontal(|ui| {
                            ui.add(egui::TextEdit::singleline(&mut s.geoid_file).desired_width(320.0));
                            if ui.button("…").clicked() {
                                actions.push(Action::ChooseGeoid);
                            }
                        });
                        ui.end_row();
                    }
                    HeightMode::Constant => num(ui, "Höhenanomalie ζ [m]", &mut s.zeta, -100.0..=100.0, 0.001),
                    HeightMode::Ellipsoidal => {}
                }
            });
        }
        ConfigTab::Output => {
            grid(ui, "output", |ui| {
                text(ui, "Punktdatei", &mut s.output);
                text(ui, "Projekt", &mut s.project);
                text(ui, "Bearbeiter", &mut s.operator);
                check(ui, "Rechtswert mit Zonenkennzahl (32…)", &mut s.zone_prefix);
            });
            ui.small("Gespeichert werden <Name>-kor.csv, <Name>.kor und <Name>-protokoll.csv.");
        }
        ConfigTab::Display => {
            ui.small("Farbschema und Vergrößerung richten sich nach Feldbuch-NT.");
            grid(ui, "display", |ui| {
                check(ui, "Töne bei Fix-Gewinn, Fix-Verlust und fertigem Punkt", &mut s.sounds);
            });
        }
    });
}

#[derive(Default)]
pub struct ToolsView {
    lat: String,
    lon: String,
    east: String,
    north: String,
}

fn parse(s: &str) -> Option<f64> {
    s.trim().replace(',', ".").parse().ok()
}

pub fn tools(ui: &mut Ui, t: &mut ToolsView) {
    ui.heading("Koordinatenumrechnung ETRS89 ↔ UTM32");
    ui.strong("Geographisch → UTM");
    grid(ui, "g2u", |ui| {
        text(ui, "Breite [°]", &mut t.lat);
        text(ui, "Länge [°]", &mut t.lon);
    });
    if let (Some(lat), Some(lon)) = (parse(&t.lat), parse(&t.lon)) {
        if (-80.0..=84.0).contains(&lat) && (3.0..=15.0).contains(&lon) {
            let u = geo_to_utm(&Geographic { lat, lon, h: 0.0 }, UTM_ZONE);
            ui.monospace(format!(
                "Ost {:.3}   Nord {:.3}   Maßstab {:.8}   Meridiankonvergenz {:.6}°",
                u.east_prefixed(),
                u.north,
                u.k,
                u.gamma
            ));
        } else {
            ui.colored_label(WARN, "außerhalb des Bereichs der Zone 32");
        }
    }
    ui.separator();
    ui.strong("UTM → geographisch");
    grid(ui, "u2g", |ui| {
        text(ui, "Ost (mit oder ohne 32)", &mut t.east);
        text(ui, "Nord", &mut t.north);
    });
    if let (Some(mut east), Some(north)) = (parse(&t.east), parse(&t.north)) {
        let prefix = f64::from(UTM_ZONE) * 1e6;
        if east >= prefix {
            east -= prefix;
        }
        if (100_000.0..900_000.0).contains(&east) && (0.0..9_400_000.0).contains(&north) {
            let g = utm_to_geo(
                &Utm {
                    zone: UTM_ZONE,
                    east,
                    north,
                    k: 1.0,
                    gamma: 0.0,
                },
                0.0,
            );
            ui.monospace(format!("Breite {:.9}°   Länge {:.9}°", g.lat, g.lon));
        } else {
            ui.colored_label(WARN, "keine gültigen UTM-Koordinaten");
        }
    }
}

pub fn help(ui: &mut Ui, st: &State) {
    egui::ScrollArea::vertical()
        .auto_shrink(false)
        .show(ui, |ui| {
            ui.heading("GNSS-Vermessung");
            ui.label(version::FULL);
            ui.label(format!("Autor: {}", version::AUTHOR));
            ui.label("RTK-Punktbestimmung mit simpleRTK2B (u-blox ZED-F9P) und SAPOS HEPS Bayern");
            ui.label("Updates kommen mit Feldbuch-NT (Info → Nach Updates suchen).");
            ui.separator();
            ui.strong("Kurzanleitung");
            for l in [
                "1. Konfiguration → Schnittstelle: COM-Port wählen.",
                "2. Konfiguration → SAPOS: Benutzer und Passwort eingeben, Speichern.",
                "3. Antenne und Höhe sowie Punktdatei einstellen.",
                "4. Verbinden (F5). Der Empfänger wird eingestellt, SAPOS startet.",
                "5. Bei RTK FIX im Reiter Messen den Punkt aufnehmen und speichern.",
            ] {
                ui.label(l);
            }
            ui.separator();
            ui.strong("Tastenkürzel");
            table(
                ui,
                "keys",
                &[
                    ("F5", "Verbinden / Trennen".into()),
                    ("Leertaste", "Punkt messen".into()),
                    ("Eingabetaste", "Ergebnis speichern (neuer Punkt)".into()),
                    ("Esc", "Messung abbrechen".into()),
                ],
            );
            if let Some((sw, hw, ext)) = &st.version {
                ui.separator();
                ui.strong("Empfänger");
                ui.monospace(format!("Software {sw}\nHardware {hw}\n{}", ext.join("\n")));
            }
        });
}
