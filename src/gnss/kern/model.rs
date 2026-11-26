//! Zweck:    Gemeinsamer Zustand: Lösungsepochen, Satelliten, Zeit, Rohdaten und Verläufe.
//! Hardware: simpleRTK2B (u-blox ZED-F9P)
//! Autor:    Johann Gerner
//!
//! Übernommen aus gnss-vermessung 0.4.0 (src/model.rs), eigenständig gepflegt in Feldbuch-NT.

use crate::gnss::kern::config::*;
use crate::gnss::kern::demux::Item;
use crate::gnss::kern::dop;
use crate::gnss::kern::measure::Measurement;
use crate::gnss::kern::nmea::{self, Gga, Sentence};
use crate::gnss::kern::ntrip::Direction;
use crate::gnss::kern::rtcm::RtcmScanner;
use crate::gnss::kern::rtcm_decode::Content;
use crate::gnss::kern::spp::{self, Ephemerides, Observation};
use crate::gnss::kern::ubxmsg::{
    self, Cov, Dop, HpPos, Msg, NavPvt, Orbit, RfBlock, Sat, Signal, TimeGps, TimeLs, TimeUtc,
};
use chrono::{DateTime, Local, Utc};
use std::collections::{BTreeMap, VecDeque};
use std::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Fix {
    #[default]
    None,
    Auto2d,
    Auto3d,
    Dgnss,
    Float,
    Fixed,
}

impl Fix {
    pub fn from_pvt(p: &NavPvt) -> Self {
        // Fix-Typen: 2 = 2D, 3 = 3D, 4 = GNSS + Koppelnavigation
        if !p.fix_ok || !(2..=4).contains(&p.fix_type) {
            return Fix::None;
        }
        match p.carrier {
            2 => Fix::Fixed,
            1 => Fix::Float,
            _ if p.diff => Fix::Dgnss,
            _ if p.fix_type == 2 => Fix::Auto2d,
            _ => Fix::Auto3d,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Fix::None => "KEIN FIX",
            Fix::Auto2d => "AUTONOM 2D",
            Fix::Auto3d => "AUTONOM 3D",
            Fix::Dgnss => "DGNSS",
            Fix::Float => "RTK FLOAT",
            Fix::Fixed => "RTK FIX",
        }
    }
}

/// Alle Angaben einer Messepoche (über iTOW zusammengeführt).
#[derive(Debug, Clone, Default)]
pub struct Solution {
    pub itow: u32,
    pub fix: Fix,
    pub lat: f64,
    pub lon: f64,
    /// ellipsoidische Höhe [m]
    pub h: f64,
    pub num_sv: u8,
    pub pdop: f64,
    pub h_acc: f64,
    pub v_acc: f64,
    pub speed: f64,
    pub cov: Option<Cov>,
    /// Korrekturdatenalter [s] aus der GGA-Meldung
    pub age: Option<f64>,
    pub ref_station: Option<String>,
    /// Position hochauflösend und Kovarianz gültig
    pub complete: bool,
}

impl Solution {
    pub fn sigma_e(&self) -> Option<f64> {
        self.cov.as_ref().map(|c| c.ee.max(0.0).sqrt())
    }
    pub fn sigma_n(&self) -> Option<f64> {
        self.cov.as_ref().map(|c| c.nn.max(0.0).sqrt())
    }
    pub fn sigma_2d(&self) -> Option<f64> {
        self.cov.as_ref().map(|c| (c.ee + c.nn).max(0.0).sqrt())
    }
    pub fn sigma_h(&self) -> Option<f64> {
        self.cov.as_ref().map(|c| c.dd.max(0.0).sqrt())
    }
    /// Standardellipse: große und kleine Halbachse [m], Richtung der großen Achse ab Nord [°].
    pub fn ellipse(&self) -> Option<(f64, f64, f64)> {
        let c = self.cov.as_ref()?;
        Some(ellipse(c.nn, c.ee, c.ne))
    }
}

/// Eigenwerte einer 2x2-Kovarianz als Ellipse (a, b, Azimut in Grad).
pub fn ellipse(nn: f64, ee: f64, ne: f64) -> (f64, f64, f64) {
    let mean = (nn + ee) / 2.0;
    let r = ((nn - ee) / 2.0).hypot(ne);
    let azimuth = 0.5 * (2.0 * ne).atan2(nn - ee);
    (
        (mean + r).max(0.0).sqrt(),
        (mean - r).max(0.0).sqrt(),
        azimuth.to_degrees().rem_euclid(180.0),
    )
}

#[derive(Default)]
struct Assembler {
    itow: u32,
    pvt: Option<NavPvt>,
    hp: Option<HpPos>,
    cov: Option<Cov>,
    emitted: bool,
}

impl Assembler {
    /// Beginnt bei neuer Epoche von vorn und liefert eine unvollständig gebliebene.
    fn advance(&mut self, itow: u32) -> Option<Solution> {
        if itow == self.itow {
            return None;
        }
        let rest = if self.emitted { None } else { self.build() };
        *self = Assembler {
            itow,
            ..Default::default()
        };
        rest
    }

    fn build(&self) -> Option<Solution> {
        let p = self.pvt.as_ref()?;
        let hp = self.hp.as_ref().filter(|h| h.valid);
        let cov = self.cov.clone().filter(|c| c.valid);
        Some(Solution {
            itow: p.itow,
            fix: Fix::from_pvt(p),
            lat: hp.map_or(p.lat, |h| h.lat),
            lon: hp.map_or(p.lon, |h| h.lon),
            h: hp.map_or(p.height, |h| h.height),
            num_sv: p.num_sv,
            pdop: p.pdop,
            h_acc: p.h_acc,
            v_acc: p.v_acc,
            speed: p.speed,
            complete: hp.is_some() && cov.is_some(),
            cov,
            age: None,
            ref_station: None,
        })
    }

    fn take_if_complete(&mut self) -> Option<Solution> {
        if self.emitted || self.pvt.is_none() || self.hp.is_none() || self.cov.is_none() {
            return None;
        }
        self.emitted = true;
        self.build()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RawKind {
    Nmea,
    Ubx,
    Rtcm,
    Junk,
}

#[derive(Debug, Clone)]
pub struct RawEntry {
    pub seq: u64,
    pub time: String,
    pub kind: RawKind,
    pub name: String,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, Default)]
pub struct RawStat {
    pub count: u64,
    pub bytes: u64,
    pub first: Option<Instant>,
    pub last: Option<Instant>,
}

#[derive(Default)]
pub struct RawLog {
    pub entries: VecDeque<RawEntry>,
    pub stats: BTreeMap<String, RawStat>,
    next_seq: u64,
}

impl RawLog {
    pub fn push(&mut self, kind: RawKind, name: String, data: Vec<u8>) {
        let now = Instant::now();
        let s = self.stats.entry(name.clone()).or_default();
        s.count += 1;
        s.bytes += data.len() as u64;
        s.first.get_or_insert(now);
        s.last = Some(now);
        self.next_seq += 1;
        let time = Local::now().format("%H:%M:%S%.3f").to_string();
        self.entries.push_back(RawEntry {
            seq: self.next_seq,
            time,
            kind,
            name,
            data,
        });
        while self.entries.len() > RAW_CAPACITY {
            self.entries.pop_front();
        }
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.stats.clear();
    }
}

#[derive(Debug, Clone)]
pub struct HistorySample {
    pub fix: Fix,
    pub sigma_2d: Option<f64>,
    pub sigma_h: Option<f64>,
    pub age: Option<f64>,
    pub used: u8,
    pub receiver_dop: Option<Dop>,
    pub computed_dop: Option<Dop>,
    /// Datenrate der Korrekturen [Byte/s]
    pub rate: f64,
    pub pc_offset_ms: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub enum NtripPhase {
    #[default]
    Off,
    WaitPosition,
    Connecting,
    Active,
    Error,
    AuthError,
}

impl NtripPhase {
    pub fn label(&self) -> &'static str {
        match self {
            NtripPhase::Off => "getrennt",
            NtripPhase::WaitPosition => "wartet auf Position",
            NtripPhase::Connecting => "verbindet",
            NtripPhase::Active => "aktiv",
            NtripPhase::Error => "Fehler",
            NtripPhase::AuthError => "Anmeldung abgelehnt",
        }
    }
}

#[derive(Default)]
pub struct NtripState {
    pub phase: NtripPhase,
    pub mountpoint: String,
    pub connected_since: Option<Instant>,
    pub bytes: u64,
    pub last_data: Option<Instant>,
    pub reconnects: u32,
    pub last_error: Option<String>,
    pub scanner: RtcmScanner,
    /// ausgewerteter Inhalt der Korrekturdaten
    pub content: Content,
    /// Byte seit Beginn der laufenden Sekunde und zuletzt ermittelte Rate
    window_bytes: u64,
    window_start: Option<Instant>,
    pub rate: f64,
}

impl NtripState {
    pub fn count(&mut self, n: usize) {
        const WINDOW_S: f64 = 2.0;
        self.bytes += n as u64;
        self.window_bytes += n as u64;
        self.last_data = Some(Instant::now());
        let start = *self.window_start.get_or_insert_with(Instant::now);
        let dt = start.elapsed().as_secs_f64();
        if dt >= WINDOW_S {
            self.rate = self.window_bytes as f64 / dt;
            self.window_bytes = 0;
            self.window_start = Some(Instant::now());
        }
    }
}

/// Abweichung der aus den SAPOS-Codemessungen berechneten Position vom Sollort der Station.
#[derive(Debug, Clone, Copy)]
pub struct StationFix {
    pub tow_ms: u32,
    pub east: f64,
    pub north: f64,
    pub up: f64,
    pub sats: usize,
}

/// Eintrag im Protokoll der Kommunikation mit dem Caster.
#[derive(Debug, Clone)]
pub struct CommEntry {
    pub time: String,
    pub direction: Direction,
    /// Eintrag beschreibt empfangene Korrekturdaten
    pub data: bool,
    pub text: String,
}

/// Verwendung der Korrekturnachrichten laut Empfänger (RXM-RTCM).
#[derive(Debug, Clone, Default)]
pub struct RtcmFeedback {
    pub used: u64,
    pub unused: u64,
    pub crc_errors: u64,
}

#[derive(Default)]
pub struct State {
    pub raw: RawLog,
    assembler: Assembler,
    pub solution: Option<Solution>,
    pub solution_at: Option<Instant>,
    pub gga: Option<(Gga, Instant)>,
    pub sats: Vec<Sat>,
    pub sats_at: Option<Instant>,
    pub signals: Vec<Signal>,
    pub orbits: Vec<Orbit>,
    pub receiver_dop: Option<Dop>,
    pub computed_dop: Option<Dop>,
    pub dop_mask: f64,
    pub dop_clock_per_system: bool,
    pub time_gps: Option<TimeGps>,
    pub time_utc: Option<(TimeUtc, Instant)>,
    pub pc_offset_ms: Option<f64>,
    pub time_ls: Option<TimeLs>,
    pub ttff_s: Option<f64>,
    pub uptime_s: Option<f64>,
    pub rf: Vec<RfBlock>,
    pub version: Option<(String, String, Vec<String>)>,
    pub history: VecDeque<HistorySample>,
    /// zurückgelegter Weg (Breite, Länge)
    pub track: VecDeque<(f64, f64)>,
    /// Bahnspuren je Satellit (Azimut, Elevation)
    pub trails: BTreeMap<(u8, u8), VecDeque<(f64, f64)>>,
    last_trail_itow: Option<u32>,
    pub events: VecDeque<String>,
    pub fix_since: Option<Instant>,
    pub fix_losses: u32,
    pub ntrip: NtripState,
    /// GPS-Bahndaten aus dem Empfänger
    pub ephemerides: Ephemerides,
    /// Fehler der aus SAPOS-Codemessungen berechneten Stationsposition
    pub station_track: VecDeque<StationFix>,
    pub station_status: String,
    last_range_epoch: u64,
    /// Protokoll der SAPOS-Kommunikation (bleibt über Neuverbindungen erhalten)
    pub comm: VecDeque<CommEntry>,
    /// Mitschnitt der empfangenen RTCM-Bytes (Datei, Pfad, Größe)
    pub rtcm_file: Option<(std::fs::File, std::path::PathBuf, u64)>,
    pub rtcm_feedback: BTreeMap<u16, RtcmFeedback>,
    pub acks: VecDeque<(u8, u8, bool)>,
    pub config_log: Vec<(String, Option<bool>)>,
    pub config_running: bool,
    pub readback: Vec<(u32, u64)>,
    /// Anzahl der zuletzt dauerhaft gespeicherten Konfigurationsschritte (Feldbuch-NT)
    pub persisted_steps: Option<usize>,
    pub measurement: Option<Measurement>,
    pub bytes_received: u64,
    pub link_error: Option<String>,
    pub replay_done: bool,
    /// Daten stammen aus einer Datei
    pub replay: bool,
}

impl State {
    pub fn comm_log(&mut self, direction: Direction, data: bool, text: impl Into<String>) {
        let time = Local::now().format("%H:%M:%S%.3f").to_string();
        self.comm.push_back(CommEntry {
            time,
            direction,
            data,
            text: text.into(),
        });
        while self.comm.len() > COMM_CAPACITY {
            self.comm.pop_front();
        }
    }

    /// Verarbeitet einen Block Korrekturdaten: Statistik, Auswertung, Protokoll, Mitschnitt.
    pub fn corrections(&mut self, bytes: &[u8]) {
        self.ntrip.count(bytes.len());
        if let Some((file, _, size)) = self.rtcm_file.as_mut() {
            match std::io::Write::write_all(file, bytes) {
                Ok(()) => *size += bytes.len() as u64,
                Err(e) => {
                    self.rtcm_file = None;
                    self.event(format!("RTCM-Mitschnitt beendet: {e}"));
                }
            }
        }
        let frames = self.ntrip.scanner.feed_frames(bytes);
        let mut numbers = Vec::new();
        for payload in frames {
            numbers.push(((u16::from(payload[0]) << 4) | u16::from(payload[1] >> 4)).to_string());
            let line = self.ntrip.content.apply(&payload);
            self.comm_log(Direction::Received, true, line);
        }
        self.station_solution();
        if !numbers.is_empty() {
            self.raw.push(
                RawKind::Rtcm,
                format!("RTCM {}", numbers.join(",")),
                bytes.to_vec(),
            );
        }
    }

    /// Berechnet die Stationsposition aus neuen Codemessungen.
    fn station_solution(&mut self) {
        let content = &self.ntrip.content;
        if content.range_epochs == self.last_range_epoch {
            return;
        }
        self.last_range_epoch = content.range_epochs;
        let (Some(station), Some(epoch)) = (&content.station, &content.gps_ranges) else {
            self.station_status = "Warte auf Stationsposition (1005/1006) und GPS-Messungen (MSM)".into();
            return;
        };
        let obs: Vec<Observation> = epoch.ranges.iter().map(|&(prn, pseudorange)| Observation { prn, pseudorange }).collect();
        let with_orbit = obs.iter().filter(|o| self.ephemerides.gps.contains_key(&o.prn)).count();
        let tow_s = f64::from(epoch.tow_ms) / 1000.0;
        match spp::solve(&obs, tow_s, &self.ephemerides.gps, station.ecef) {
            Some(sol) => {
                let d = spp::enu(station.ecef, sol.ecef);
                self.station_status = format!("Stationslösung aus {} GPS-Satelliten", sol.used);
                self.station_track.push_back(StationFix { tow_ms: epoch.tow_ms, east: d[0], north: d[1], up: d[2], sats: sol.used });
                while self.station_track.len() > HISTORY_CAPACITY {
                    self.station_track.pop_front();
                }
            }
            None => {
                self.station_status = format!(
                    "Zu wenige GPS-Satelliten für die Stationslösung: {} Messungen, {with_orbit} mit Bahndaten (Bahndaten kommen alle 30 s)",
                    obs.len()
                );
            }
        }
    }

    pub fn event(&mut self, text: impl Into<String>) {
        let line = format!("{}  {}", Local::now().format("%H:%M:%S"), text.into());
        self.events.push_front(line);
        self.events.truncate(EVENT_CAPACITY);
    }

    /// GGA-Meldung, sofern sie nicht älter als `max_age` ist.
    pub fn fresh_gga(&self, max_age: std::time::Duration) -> Option<&Gga> {
        self.gga
            .as_ref()
            .filter(|(_, t)| t.elapsed() <= max_age)
            .map(|(g, _)| g)
    }

    pub fn is_stale(&self) -> bool {
        self.solution_at.is_none_or(|t| t.elapsed() > STALE_AFTER)
    }

    pub fn apply(&mut self, item: Item) {
        match item {
            Item::Nmea(line) => {
                let kind = line
                    .get(1..line.find(',').unwrap_or(1))
                    .unwrap_or("NMEA")
                    .to_string();
                if let Ok(Sentence::Gga(g)) = nmea::parse(&line) {
                    self.gga = Some((g, Instant::now()));
                }
                self.raw.push(RawKind::Nmea, kind, line.into_bytes());
            }
            Item::Junk(data) => self.raw.push(RawKind::Junk, "unbekannt".into(), data),
            Item::Ubx { class, id, raw } => {
                let msg = raw
                    .get(6..raw.len() - 2)
                    .and_then(|p| ubxmsg::decode(class, id, p));
                self.raw.push(RawKind::Ubx, ubxmsg::name(class, id), raw);
                if let Some(m) = msg {
                    self.apply_msg(m);
                }
            }
        }
    }

    fn apply_msg(&mut self, msg: Msg) {
        match msg {
            Msg::Pvt(p) => self.assemble(p.itow, |a| a.pvt = Some(p)),
            Msg::HpPos(h) => self.assemble(h.itow, |a| a.hp = Some(h)),
            Msg::Cov(c) => self.assemble(c.itow, |a| a.cov = Some(c)),
            Msg::Dop(d) => self.receiver_dop = Some(d),
            Msg::Sat { itow, sats } => {
                let dirs = dop::directions(&sats, self.dop_mask, true);
                self.computed_dop = dop::compute(&dirs, self.dop_clock_per_system);
                self.update_trails(itow, &sats);
                self.sats = sats;
                self.sats_at = Some(Instant::now());
            }
            Msg::Sig(s) => self.signals = s,
            Msg::Orb(o) => self.orbits = o,
            Msg::TimeGps(t) => self.time_gps = Some(t),
            Msg::TimeUtc(t) => {
                // Näherung: enthält die Übertragungsverzögerung der Schnittstelle
                self.pc_offset_ms = crate::gnss::kern::gtime::utc_datetime(&t).filter(|_| !self.replay).map(
                    |u: DateTime<Utc>| {
                        (Utc::now() - u).num_microseconds().unwrap_or(0) as f64 / 1000.0
                    },
                );
                self.time_utc = Some((t, Instant::now()));
            }
            Msg::TimeLs(t) => self.time_ls = Some(t),
            Msg::Status { ttff_s, uptime_s } => {
                self.ttff_s = Some(ttff_s);
                self.uptime_s = Some(uptime_s);
            }
            Msg::RtcmUse(r) => {
                let f = self.rtcm_feedback.entry(r.msg_type).or_default();
                if r.crc_failed {
                    f.crc_errors += 1;
                } else if r.used == 2 {
                    f.used += 1;
                } else {
                    f.unused += 1;
                }
            }
            Msg::Rf(r) => self.rf = r,
            Msg::Sfrbx { gnss: 0, sv, words } => self.ephemerides.add_gps_words(sv, &words),
            Msg::Sfrbx { .. } => {}
            Msg::Version {
                software,
                hardware,
                extensions,
            } => self.version = Some((software, hardware, extensions)),
            Msg::Ack { class, id, ok } => self.acks.push_back((class, id, ok)),
            Msg::ValGet(v) => self.readback = v,
        }
    }

    fn assemble(&mut self, itow: u32, set: impl FnOnce(&mut Assembler)) {
        if let Some(rest) = self.assembler.advance(itow) {
            self.on_solution(rest);
        }
        set(&mut self.assembler);
        if let Some(s) = self.assembler.take_if_complete() {
            self.on_solution(s);
        }
    }

    fn on_solution(&mut self, mut s: Solution) {
        if let Some(g) = self.fresh_gga(GGA_VALID) {
            s.age = g.age_corr;
            s.ref_station = g.ref_station.clone();
        }
        let previous = self.solution.as_ref().map(|p| p.fix);
        if previous != Some(s.fix) {
            self.event(format!("Status: {}", s.fix.label()));
            if s.fix == Fix::Fixed {
                self.fix_since = Some(Instant::now());
            } else if previous == Some(Fix::Fixed) {
                self.fix_losses += 1;
                self.fix_since = None;
            }
        }
        self.history.push_back(HistorySample {
            fix: s.fix,
            sigma_2d: s.sigma_2d(),
            sigma_h: s.sigma_h(),
            age: s.age,
            used: s.num_sv,
            receiver_dop: self.receiver_dop,
            computed_dop: self.computed_dop,
            rate: if self.ntrip.phase == NtripPhase::Active {
                self.ntrip.rate
            } else {
                0.0
            },
            pc_offset_ms: self.pc_offset_ms,
        });
        while self.history.len() > HISTORY_CAPACITY {
            self.history.pop_front();
        }
        if s.fix != Fix::None {
            self.track.push_back((s.lat, s.lon));
            while self.track.len() > TRACK_CAPACITY {
                self.track.pop_front();
            }
        }
        if let Some(m) = self.measurement.as_mut() {
            m.add(&s);
        }
        self.solution = Some(s);
        self.solution_at = Some(Instant::now());
    }

    fn update_trails(&mut self, itow: u32, sats: &[Sat]) {
        if self
            .last_trail_itow
            .is_some_and(|l| itow.abs_diff(l) < TRAIL_INTERVAL_MS)
        {
            return;
        }
        self.last_trail_itow = Some(itow);
        for s in sats {
            if let Some(e) = s.elev {
                let t = self.trails.entry((s.gnss, s.sv)).or_default();
                t.push_back((s.azim, e));
                while t.len() > TRAIL_CAPACITY {
                    t.pop_front();
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn epochs_are_joined_by_itow() {
        let mut s = State::default();
        let pvt = |itow| {
            Msg::Pvt(NavPvt {
                itow,
                fix_type: 3,
                fix_ok: true,
                carrier: 2,
                ..Default::default()
            })
        };
        let hp = |itow| {
            Msg::HpPos(HpPos {
                itow,
                valid: true,
                lat: 48.5,
                ..Default::default()
            })
        };
        let cov = |itow| {
            Msg::Cov(Cov {
                itow,
                valid: true,
                nn: 1e-4,
                ee: 1e-4,
                dd: 4e-4,
                ne: 0.0,
            })
        };
        s.apply_msg(pvt(1000));
        s.apply_msg(hp(1000));
        assert!(s.solution.is_none());
        s.apply_msg(cov(1000));
        assert!(s.solution.as_ref().unwrap().complete);
        assert_eq!(s.history.len(), 1);
        // fehlende Kovarianz: Epoche wird beim Wechsel als unvollständig weitergegeben
        s.apply_msg(pvt(2000));
        s.apply_msg(hp(2000));
        s.apply_msg(pvt(3000));
        assert!(!s.solution.as_ref().unwrap().complete);
        assert_eq!(s.history.len(), 2);
    }

    #[test]
    fn ellipse_axes() {
        let (a, b, az) = ellipse(1.0, 4.0, 0.0);
        assert!((a - 2.0).abs() < 1e-12 && (b - 1.0).abs() < 1e-12 && (az - 90.0).abs() < 1e-9);
    }
}
