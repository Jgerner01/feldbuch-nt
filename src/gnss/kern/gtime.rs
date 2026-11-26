//! Zweck:    Zeitrechnung GPS-Zeit, UTC und weitere Zeitsysteme.
//! Hardware: simpleRTK2B (u-blox ZED-F9P)
//! Autor:    Johann Gerner
//!
//! Übernommen aus gnss-vermessung 0.4.0 (src/gtime.rs), eigenständig gepflegt in Feldbuch-NT.

use crate::gnss::kern::ubxmsg::{TimeGps, TimeUtc};
use chrono::{DateTime, NaiveDate, NaiveDateTime, TimeDelta, Utc};

/// BeiDou-Zeit liegt fest hinter der GPS-Zeit.
pub const BDT_MINUS_GPS_S: i64 = -14;
/// GLONASS-Zeit läuft mit festem Versatz zu UTC.
pub const GLONASS_MINUS_UTC_H: i64 = 3;
const SECONDS_PER_WEEK: i64 = 604_800;

/// Beginn der GPS-Zeitzählung.
fn gps_epoch() -> NaiveDateTime {
    NaiveDate::from_ymd_opt(1980, 1, 6)
        .and_then(|d| d.and_hms_opt(0, 0, 0))
        .unwrap_or_default()
}

/// GPS-Zeit als Kalenderdatum (ohne Schaltsekunden).
pub fn gps_datetime(t: &TimeGps) -> Option<NaiveDateTime> {
    if !t.tow_valid || !t.week_valid {
        return None;
    }
    let d = TimeDelta::seconds(t.week as i64 * SECONDS_PER_WEEK)
        + TimeDelta::milliseconds(t.itow as i64)
        + TimeDelta::nanoseconds(t.ftow_ns as i64);
    gps_epoch().checked_add_signed(d)
}

/// Sekunde der GPS-Woche.
pub fn seconds_of_week(t: &TimeGps) -> f64 {
    t.itow as f64 * 1e-3 + t.ftow_ns as f64 * 1e-9
}

/// UTC aus NAV-TIMEUTC; der Nanosekundenanteil darf negativ sein.
pub fn utc_datetime(t: &TimeUtc) -> Option<DateTime<Utc>> {
    if !t.valid {
        return None;
    }
    let date = NaiveDate::from_ymd_opt(t.year as i32, t.month as u32, t.day as u32)?;
    // Schaltsekunde (sec = 60) wird als 59 + 1 s dargestellt
    let leap = t.sec == 60;
    let base = date.and_hms_opt(
        t.hour as u32,
        t.min as u32,
        if leap { 59 } else { t.sec as u32 },
    )?;
    let extra = TimeDelta::nanoseconds(t.nano as i64) + TimeDelta::seconds(leap as i64);
    Some(base.checked_add_signed(extra)?.and_utc())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn negative_nanoseconds() {
        let t = TimeGps {
            itow: 0,
            ftow_ns: -1,
            week: 2400,
            tow_valid: true,
            week_valid: true,
            ..Default::default()
        };
        let d = gps_datetime(&t).unwrap();
        assert_eq!(d.format("%H:%M:%S%.9f").to_string(), "23:59:59.999999999");
        let u = TimeUtc {
            year: 2026,
            month: 11,
            day: 26,
            hour: 12,
            nano: -500_000_000,
            valid: true,
            ..Default::default()
        };
        assert_eq!(
            utc_datetime(&u).unwrap().format("%H:%M:%S%.3f").to_string(),
            "11:59:59.500"
        );
    }

    #[test]
    fn known_week() {
        // Woche 2400 beginnt am 4. Januar 2026
        let t = TimeGps {
            week: 2400,
            tow_valid: true,
            week_valid: true,
            ..Default::default()
        };
        assert_eq!(
            gps_datetime(&t).unwrap().date(),
            NaiveDate::from_ymd_opt(2026, 1, 4).unwrap()
        );
    }
}
