//! Zweck:    GPS-Zeit, UTC, Ortszeit, Zeitdifferenzen und Schaltsekunden.
//! Hardware: simpleRTK2B (u-blox ZED-F9P)
//! Autor:    Johann Gerner
//!
//! Übernommen aus gnss-vermessung 0.4.0 (src/bin/gnss-gui/view_time.rs), eigenständig gepflegt in Feldbuch-NT.

use crate::gnss::ansicht::widgets::*;
use chrono::{Datelike, Local, TimeDelta};
use eframe::egui::{self, RichText, Ui};
use crate::gnss::kern::config::STALE_AFTER;
use crate::gnss::kern::gtime;
use crate::gnss::kern::model::State;

pub fn show(ui: &mut Ui, st: &State) {
    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        let gps = st.time_gps.as_ref().and_then(gtime::gps_datetime);
        let utc = st.time_utc.as_ref().and_then(|t| gtime::utc_datetime(&t.0));
        let local = utc.map(|u| u.with_timezone(&Local));
        if st.time_utc.as_ref().is_some_and(|t| t.1.elapsed() > STALE_AFTER) {
            ui.colored_label(WARN, "Zeitangaben sind veraltet (keine neuen Daten).");
        }
        egui::Grid::new("clocks").num_columns(3).spacing([30.0, 10.0]).show(ui, |ui| {
            let mut clock = |name: &str, time: String, date: String| {
                ui.label(RichText::new(name).size(24.0));
                ui.label(RichText::new(time).size(44.0).monospace().strong());
                ui.label(RichText::new(date).size(18.0));
                ui.end_row();
            };
            let none = || "--:--:--".to_string();
            clock(
                "GPS-Zeit",
                gps.map_or_else(none, |t| t.format("%H:%M:%S").to_string()),
                gps.map_or_else(String::new, |t| t.format("%d.%m.%Y").to_string()),
            );
            clock(
                "UTC",
                utc.map_or_else(none, |t| t.format("%H:%M:%S").to_string()),
                utc.map_or_else(String::new, |t| t.format("%d.%m.%Y").to_string()),
            );
            clock(
                "Ortszeit",
                local.map_or_else(none, |t| t.format("%H:%M:%S").to_string()),
                local.map_or_else(String::new, |t| t.format("%d.%m.%Y  UTC%:z").to_string()),
            );
        });
        ui.separator();
        let mut rows: Vec<(&str, String)> = Vec::new();
        if let Some(t) = &st.time_gps {
            rows.push(("GPS-Woche", if t.week_valid { t.week.to_string() } else { "ungültig".into() }));
            rows.push(("Sekunde der Woche", format!("{:.3}", gtime::seconds_of_week(t))));
            let leap = format!("{} s{}", t.leap_s, if t.leap_valid { "" } else { "  (noch nicht gesichert)" });
            rows.push(("GPS − UTC (Schaltsekunden)", leap));
            rows.push(("Genauigkeit GPS-Zeit", format!("{} ns", t.acc_ns)));
        }
        if let Some(g) = gps {
            rows.push(("Tag des Jahres (GPS)", format!("{:03}", g.ordinal())));
        }
        if let Some((t, _)) = &st.time_utc {
            rows.push(("Genauigkeit UTC", format!("{} ns", t.acc_ns)));
        }
        if let Some(l) = local {
            let offset = l.offset().local_minus_utc();
            rows.push(("Ortszeit − UTC", format!("{:+.1} h", f64::from(offset) / 3600.0)));
            if let Some(t) = st.time_gps.as_ref().filter(|t| t.leap_valid) {
                rows.push(("Ortszeit − GPS-Zeit", format!("{:+} s", offset - i32::from(t.leap_s))));
            }
        }
        rows.push(("PC-Uhr − GNSS (Näherung)", st.pc_offset_ms.map_or("–".into(), |v| format!("{v:+.0} ms"))));
        let leap_event = match &st.time_ls {
            Some(t) if t.event_valid && t.change != 0 => format!("{:+} s in {} Tagen", t.change, t.seconds_to_event / 86_400),
            Some(_) => "keine angekündigt".into(),
            None => "noch nicht empfangen".into(),
        };
        rows.push(("Schaltsekunde", leap_event));
        table(ui, "time", &rows);
        ui.separator();
        ui.strong("Weitere Zeitsysteme");
        let fmt = |t: Option<chrono::NaiveDateTime>| t.map_or("–".into(), |t| t.format("%H:%M:%S").to_string());
        table(ui, "other", &[
            ("Galileo (nominell = GPS)", fmt(gps)),
            ("BeiDou (GPS − 14 s)", fmt(gps.map(|g| g + TimeDelta::seconds(gtime::BDT_MINUS_GPS_S)))),
            ("GLONASS (UTC + 3 h)", fmt(utc.map(|u| u.naive_utc() + TimeDelta::hours(gtime::GLONASS_MINUS_UTC_H)))),
        ]);
        ui.separator();
        let offsets = st.history.iter().map(|h| h.pc_offset_ms.map(f64::abs)).collect();
        plot(ui, "Betrag der Abweichung PC-Uhr [ms]", &[("Abweichung", BLUE, offsets)], 110.0);
        ui.small("Der Vergleich mit der PC-Uhr enthält die Übertragungsverzögerung der Schnittstelle.");
    });
}
