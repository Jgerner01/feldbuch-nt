//! Zweck:    Liste der GPS-Ephemeriden (Bahndaten aus UBX-RXM-SFRBX) mit Detailanzeige.
//!           Azimut und Elevation aus der Ephemeride werden zur Kontrolle der Dekodierung
//!           neben die Werte des Empfängers (NAV-SAT) gestellt.
//! Hardware: simpleRTK2B (u-blox ZED-F9P)
//! Autor:    Johann Gerner
//!
//! Übernommen aus gnss-vermessung 0.4.0 (src/bin/gnss-gui/view_ephem.rs), eigenständig gepflegt in Feldbuch-NT.

use crate::gnss::ansicht::widgets::*;
use eframe::egui::{self, RichText, Ui};
use crate::gnss::kern::geo::{Geographic, geo_to_ecef};
use crate::gnss::kern::model::State;
use crate::gnss::kern::spp::{self, Ephemeris};

const SECONDS_PER_DAY: f64 = 86_400.0;
const SECONDS_PER_HOUR: f64 = 3_600.0;
const HALF_WEEK_S: f64 = 302_400.0;
const WEEK_S: f64 = 604_800.0;
/// Abweichung zum Empfänger, ab der Azimut/Elevation als auffällig gelten [°]
const DIRECTION_TOLERANCE_DEG: f64 = 2.0;
const WEEKDAYS: [&str; 7] = ["So", "Mo", "Di", "Mi", "Do", "Fr", "Sa"];

#[derive(Default)]
pub struct EphemView {
    selected: Option<u8>,
}

/// Sekunde der GPS-Woche als „Wochentag hh:mm:ss“.
fn week_time(s: f64) -> String {
    let day = (s / SECONDS_PER_DAY).floor();
    let rest = s - day * SECONDS_PER_DAY;
    let (h, m) = ((rest / SECONDS_PER_HOUR).floor(), ((rest % SECONDS_PER_HOUR) / 60.0).floor());
    format!("{} {:02}:{:02}:{:02}", WEEKDAYS[(day as usize) % 7], h as u32, m as u32, (rest % 60.0) as u32)
}

fn signed_age(now: f64, t: f64) -> f64 {
    let d = now - t;
    if d > HALF_WEEK_S {
        d - WEEK_S
    } else if d < -HALF_WEEK_S {
        d + WEEK_S
    } else {
        d
    }
}

/// Azimut und Elevation [°] des Satelliten vom Empfänger aus.
fn direction(e: &Ephemeris, tow: f64, receiver: [f64; 3]) -> (f64, f64) {
    let (sat, _) = spp::position(e, tow);
    let d = spp::enu(receiver, sat);
    (d[0].atan2(d[1]).to_degrees().rem_euclid(360.0), d[2].atan2(d[0].hypot(d[1])).to_degrees())
}

fn details(prn: u8, e: &Ephemeris, tow: Option<f64>) -> Vec<(&'static str, String)> {
    let mut rows = vec![
        ("Satellit", format!("G{prn:02}")),
        ("IODE", e.iode.to_string()),
        ("Gesundheit", if e.health == 0 { "gesund (0)".into() } else { format!("krank ({})", e.health) }),
        ("toe (Bezugszeit Bahn)", format!("{:.0} s  ({})", e.toe, week_time(e.toe))),
        ("toc (Bezugszeit Uhr)", format!("{:.0} s  ({})", e.toc, week_time(e.toc))),
        ("√a [√m]", format!("{:.6}", e.sqrt_a)),
        ("große Halbachse a [km]", format!("{:.3}", e.sqrt_a * e.sqrt_a / 1000.0)),
        ("Exzentrizität e", format!("{:.10}", e.e)),
        ("Inklination i0 [°]", format!("{:.6}", e.i0.to_degrees())),
        ("Rektaszension Ω0 [°]", format!("{:.6}", e.omega0.to_degrees())),
        ("Argument des Perigäums ω [°]", format!("{:.6}", e.omega.to_degrees())),
        ("mittlere Anomalie M0 [°]", format!("{:.6}", e.m0.to_degrees())),
        ("Δn [rad/s]", format!("{:.6e}", e.delta_n)),
        ("Ω̇ [rad/s]", format!("{:.6e}", e.omega_dot)),
        ("IDOT [rad/s]", format!("{:.6e}", e.idot)),
        ("Cuc / Cus [rad]", format!("{:.4e} / {:.4e}", e.cuc, e.cus)),
        ("Crc / Crs [m]", format!("{:.3} / {:.3}", e.crc, e.crs)),
        ("Cic / Cis [rad]", format!("{:.4e} / {:.4e}", e.cic, e.cis)),
        ("af0 [s]", format!("{:.6e}", e.af0)),
        ("af1 [s/s]", format!("{:.6e}", e.af1)),
        ("af2 [s/s²]", format!("{:.6e}", e.af2)),
        ("TGD [s]", format!("{:.4e}", e.tgd)),
    ];
    if let Some(t) = tow {
        let (sat, clock) = spp::position(e, t);
        rows.push(("Position jetzt X/Y/Z [km]", format!("{:.3}  {:.3}  {:.3}", sat[0] / 1000.0, sat[1] / 1000.0, sat[2] / 1000.0)));
        rows.push(("Uhrkorrektur jetzt", format!("{:.3} µs ({:.1} m)", clock * 1e6, clock * spp::CLIGHT)));
    }
    rows
}

pub fn show(ui: &mut Ui, st: &State, v: &mut EphemView) {
    let eph = &st.ephemerides;
    let tow = st.time_gps.as_ref().filter(|t| t.tow_valid).map(|t| f64::from(t.itow) / 1000.0);
    let receiver = st.solution.as_ref().map(|s| geo_to_ecef(&Geographic { lat: s.lat, lon: s.lon, h: s.h }));
    ui.label(format!(
        "{} GPS-Ephemeriden vollständig · {} Unterrahmen empfangen · noch unvollständig: {}",
        eph.gps.len(),
        eph.subframes_received,
        {
            let pending = eph.pending();
            if pending.is_empty() { "–".to_string() } else { pending.iter().map(|p| format!("G{p:02}")).collect::<Vec<_>>().join(" ") }
        }
    ));
    if eph.gps.is_empty() {
        ui.weak("Noch keine Bahndaten. Der Empfänger sendet sie (RXM-SFRBX) nach der Konfiguration; ein vollständiger Satz dauert 18–30 s je Satellit.");
        return;
    }
    if let Some(prn) = v.selected.filter(|p| eph.gps.contains_key(p)) {
        let rows = details(prn, &eph.gps[&prn], tow);
        egui::Panel::bottom("ephemdetail").resizable(true).default_size(300.0).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.strong(format!("Ephemeride G{prn:02}"));
                if ui.button("Kopieren").clicked() {
                    ui.ctx().copy_text(rows.iter().map(|r| format!("{}\t{}", r.0, r.1)).collect::<Vec<_>>().join("\r\n"));
                }
                if ui.button("Schließen").clicked() {
                    v.selected = None;
                }
            });
            egui::ScrollArea::vertical().id_salt("ephemrows").auto_shrink(false).show(ui, |ui| table(ui, "ephemdetail", &rows));
        });
    }
    egui::ScrollArea::both().id_salt("ephem").auto_shrink(false).show(ui, |ui| {
        egui::Grid::new("ephemlist").num_columns(9).striped(true).spacing([16.0, 4.0]).show(ui, |ui| {
            for h in ["Sat", "IODE", "toe", "Alter", "Gesundheit", "a [km]", "e", "Az/El aus Ephemeride", "Az/El Empfänger"] {
                ui.strong(h);
            }
            ui.end_row();
            for (prn, e) in &eph.gps {
                let chosen = v.selected == Some(*prn);
                if ui.add(egui::Button::selectable(chosen, RichText::new(format!("G{prn:02}")).monospace())).clicked() {
                    v.selected = if chosen { None } else { Some(*prn) };
                }
                ui.monospace(e.iode.to_string());
                ui.monospace(week_time(e.toe));
                match tow {
                    Some(t) => {
                        let age = signed_age(t, e.toe);
                        let text = format!("{:+.1} h", age / SECONDS_PER_HOUR);
                        if age.abs() > 2.0 * SECONDS_PER_HOUR { ui.colored_label(WARN, text) } else { ui.monospace(text) };
                    }
                    None => {
                        ui.monospace("–");
                    }
                }
                if e.health == 0 { ui.colored_label(GOOD, "gesund") } else { ui.colored_label(BAD, format!("krank ({})", e.health)) };
                ui.monospace(format!("{:.1}", e.sqrt_a * e.sqrt_a / 1000.0));
                ui.monospace(format!("{:.5}", e.e));
                let computed = tow.zip(receiver).map(|(t, r)| direction(e, t, r));
                ui.monospace(computed.map_or("–".into(), |(az, el)| format!("{az:5.1}° / {el:5.1}°")));
                let reported = st.sats.iter().find(|s| s.gnss == 0 && s.sv == *prn).and_then(|s| s.elev.map(|el| (s.azim, el)));
                match (reported, computed) {
                    (Some((az, el)), Some((caz, cel))) => {
                        let daz = ((az - caz + 180.0).rem_euclid(360.0) - 180.0).abs();
                        let off = daz.max((el - cel).abs()) > DIRECTION_TOLERANCE_DEG;
                        let text = format!("{az:5.0}° / {el:5.0}°");
                        if off { ui.colored_label(BAD, format!("{text}  abweichend")) } else { ui.monospace(text) };
                    }
                    (Some((az, el)), None) => {
                        ui.monospace(format!("{az:5.0}° / {el:5.0}°"));
                    }
                    _ => {
                        ui.monospace("–");
                    }
                }
                ui.end_row();
            }
        });
        ui.small("Zeile anklicken für alle Bahnparameter. Az/El des Empfängers sind ganze Grad; Abweichungen über 2° deuten auf fehlerhafte Bahndaten hin.");
    });
}
