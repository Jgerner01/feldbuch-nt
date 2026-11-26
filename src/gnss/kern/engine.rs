//! Zweck:    Threads für Empfang, Senden, NTRIP, Konfiguration, Aufzeichnung und Wiedergabe.
//! Hardware: simpleRTK2B (u-blox ZED-F9P) an USB
//! Autor:    Johann Gerner
//!
//! Übernommen aus gnss-vermessung 0.4.0 (src/engine.rs), eigenständig gepflegt in Feldbuch-NT.

use crate::gnss::kern::config::*;
use crate::gnss::kern::demux::{Demux, Item};
use crate::gnss::kern::model::{NtripPhase, NtripState, State};
use crate::gnss::kern::ntrip::{Direction, NtripConfig};
use crate::gnss::kern::ubx::{self, ReceiverSetup};
use anyhow::{Context, Result};
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const READ_BUFFER: usize = 4096;
/// kleine Blöcke bei der Wiedergabe, damit die Taktung greift
const REPLAY_BUFFER: usize = 256;
const SLEEP_SLICE: Duration = Duration::from_millis(50);
/// längste Pause zwischen zwei Epochen bei der Wiedergabe
const REPLAY_MAX_GAP_MS: u32 = 2000;
/// Nach so vielen unbeantworteten Befehlen wird die Konfiguration abgebrochen.
const MAX_UNANSWERED: u32 = 3;
/// Höchstes Alter der Position für den VRS-Upload.
const GGA_UPLOAD_MAX_AGE: Duration = Duration::from_secs(10);

pub enum Source {
    Serial { device: String, baud: u32 },
    Replay { path: PathBuf, speed: f64 },
}

pub type Notify = Arc<dyn Fn() + Send + Sync>;
type Recorder = Arc<Mutex<Option<(File, PathBuf, u64)>>>;

/// Sperrt auch nach einem Absturz eines anderen Threads weiter.
fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Wartet unterbrechbar; `false`, wenn angehalten wurde.
fn pause(duration: Duration, stopped: impl Fn() -> bool) -> bool {
    let start = Instant::now();
    while start.elapsed() < duration {
        if stopped() {
            return false;
        }
        thread::sleep(SLEEP_SLICE.min(duration));
    }
    !stopped()
}

pub struct Engine {
    state: Arc<Mutex<State>>,
    tx: Sender<Vec<u8>>,
    stop: Arc<AtomicBool>,
    ntrip_stop: Option<Arc<AtomicBool>>,
    /// Korrekturdaten an den Empfänger weiterleiten
    forward: Arc<AtomicBool>,
    recorder: Recorder,
    notify: Notify,
    threads: Vec<JoinHandle<()>>,
    pub replay: bool,
}

impl Engine {
    pub fn start(source: Source, notify: Notify) -> Result<Self> {
        let state = Arc::new(Mutex::new(State::default()));
        let stop = Arc::new(AtomicBool::new(false));
        let recorder: Recorder = Arc::default();
        let (tx, rx) = mpsc::channel::<Vec<u8>>();
        let mut threads = Vec::new();

        let (reader, replay_speed): (Box<dyn Read + Send>, Option<f64>) = match source {
            Source::Serial { device, baud } => {
                let port = serialport::new(&device, baud)
                    .timeout(SERIAL_TIMEOUT)
                    .open()
                    .with_context(|| format!("Schnittstelle {device} nicht zu öffnen"))?;
                let mut writer = port.try_clone()?;
                let (st, sp, nt) = (state.clone(), stop.clone(), notify.clone());
                // einziger Schreiber: Konfiguration und Korrekturdaten mischen sich nicht
                threads.push(thread::spawn(move || {
                    while !sp.load(Ordering::Relaxed) {
                        match rx.recv_timeout(SERIAL_TIMEOUT) {
                            Ok(data) => {
                                if let Err(e) = writer.write_all(&data) {
                                    lock(&st).link_error =
                                        Some(format!("Schreiben an Empfänger: {e}"));
                                    nt();
                                    break;
                                }
                            }
                            Err(RecvTimeoutError::Timeout) => {}
                            Err(RecvTimeoutError::Disconnected) => break,
                        }
                    }
                }));
                (Box::new(port), None)
            }
            Source::Replay { path, speed } => {
                let f = File::open(&path)
                    .with_context(|| format!("Datei {} nicht lesbar", path.display()))?;
                (Box::new(f), Some(speed.max(0.01)))
            }
        };

        lock(&state).replay = replay_speed.is_some();
        let (st, sp, rec, nt) = (
            state.clone(),
            stop.clone(),
            recorder.clone(),
            notify.clone(),
        );
        threads.push(thread::spawn(move || {
            read_loop(reader, replay_speed, st, sp, rec, nt)
        }));

        Ok(Engine {
            state,
            tx,
            stop,
            ntrip_stop: None,
            forward: Arc::new(AtomicBool::new(true)),
            recorder,
            notify,
            threads,
            replay: replay_speed.is_some(),
        })
    }

    pub fn state(&self) -> MutexGuard<'_, State> {
        lock(&self.state)
    }

    /// Überträgt die Konfiguration schrittweise und wartet je Schritt auf die Bestätigung.
    pub fn configure(&self, setup: ReceiverSetup, layers: u8) {
        if !self.begin_config() {
            return;
        }
        let steps = ubx::gui_config(&setup, layers);
        let (state, tx, stop, notify) = self.config_handles();
        thread::spawn(move || run_config(&state, &tx, &stop, &notify, steps));
    }

    /// Konfiguration dauerhaft im Empfänger (RAM, BBR und Flash). Um den Flash zu schonen, werden erst die
    /// gespeicherten Werte gelesen; dauerhaft geschrieben werden nur Schritte, deren Werte im Flash fehlen oder
    /// abweichen. Alle übrigen Schritte gehen nur in den RAM.
    pub fn configure_persistent(&self, setup: ReceiverSetup) {
        if !self.begin_config() {
            return;
        }
        let items = ubx::gui_config_items(&setup);
        let (state, tx, stop, notify) = self.config_handles();
        thread::spawn(move || {
            let keys: Vec<u32> = items.iter().flat_map(|(_, i)| i.iter().map(|k| k.0)).collect();
            let flash = read_layer(&state, &tx, &stop, ubx::VALGET_FLASH, &keys);
            let persistent = ubx::LAYER_RAM | ubx::LAYER_BBR | ubx::LAYER_FLASH;
            let mut changed = 0;
            let steps: Vec<ubx::Step> = items
                .into_iter()
                .map(|(label, values)| {
                    let stored = values.iter().all(|(k, v)| flash.iter().any(|(fk, fv)| fk == k && fv == v));
                    let layers = if stored { ubx::LAYER_RAM } else { persistent };
                    if !stored {
                        changed += 1;
                    }
                    ubx::Step { label, frame: ubx::valset(layers, &values) }
                })
                .collect();
            lock(&state).event(if changed == 0 {
                "Konfiguration im Flash aktuell, nur RAM gesetzt".to_string()
            } else {
                format!("Konfiguration: {changed} Schritte dauerhaft gespeichert (BBR + Flash)")
            });
            {
                let mut s = lock(&state);
                s.persisted_steps = Some(changed);
            }
            run_config(&state, &tx, &stop, &notify, steps);
        });
    }

    /// Markiert den Beginn einer Konfiguration; `false`, wenn schon eine läuft oder eine Wiedergabe aktiv ist.
    fn begin_config(&self) -> bool {
        if self.replay {
            return false;
        }
        let mut s = self.state();
        if s.config_running {
            return false;
        }
        s.config_running = true;
        s.config_log.clear();
        true
    }

    fn config_handles(&self) -> (Arc<Mutex<State>>, Sender<Vec<u8>>, Arc<AtomicBool>, Notify) {
        (self.state.clone(), self.tx.clone(), self.stop.clone(), self.notify.clone())
    }

    pub fn start_ntrip(&mut self, cfg: NtripConfig, gga_interval: Duration) {
        if self.replay {
            return;
        }
        self.stop_ntrip();
        let own_stop = Arc::new(AtomicBool::new(false));
        self.ntrip_stop = Some(own_stop.clone());
        {
            let mut s = self.state();
            s.ntrip = NtripState::default();
            s.ntrip.phase = NtripPhase::WaitPosition;
            s.ntrip.mountpoint = cfg.mountpoint.clone();
            s.rtcm_feedback.clear();
        }
        let (state, tx, stop, notify) = (
            self.state.clone(),
            self.tx.clone(),
            self.stop.clone(),
            self.notify.clone(),
        );
        let forward = self.forward.clone();
        thread::spawn(move || {
            let stopped = move || stop.load(Ordering::Relaxed) || own_stop.load(Ordering::Relaxed);
            ntrip_loop(cfg, gga_interval, state, tx, notify, forward, stopped);
        });
    }

    pub fn stop_ntrip(&mut self) {
        if let Some(flag) = self.ntrip_stop.take() {
            // unter der Sperre, damit der alte Thread den Zustand nicht mehr ändert
            let mut s = self.state();
            flag.store(true, Ordering::Relaxed);
            s.ntrip.phase = NtripPhase::Off;
            s.ntrip.connected_since = None;
            s.comm_log(Direction::Info, false, "Verbindung beendet");
        }
    }

    /// Schaltet die Weitergabe der Korrekturdaten an den Empfänger; SAPOS bleibt verbunden.
    pub fn set_forward(&self, on: bool) {
        if self.forward.swap(on, Ordering::Relaxed) != on {
            let text = if on { "Korrekturen werden wieder an den Empfänger gegeben" } else { "Korrekturen werden nicht an den Empfänger gegeben" };
            let mut s = self.state();
            s.comm_log(Direction::Info, false, text);
            s.event(text);
        }
    }

    pub fn forwarding(&self) -> bool {
        self.forward.load(Ordering::Relaxed)
    }

    pub fn ntrip_running(&self) -> bool {
        self.ntrip_stop.is_some() && self.state().ntrip.phase != NtripPhase::AuthError
    }

    /// Zeichnet alle Bytes des Empfängers unverändert auf.
    pub fn start_recording(&self, path: &Path) -> Result<()> {
        let f = File::create(path)
            .with_context(|| format!("Aufzeichnung {} nicht anlegbar", path.display()))?;
        *lock(&self.recorder) = Some((f, path.to_path_buf(), 0));
        Ok(())
    }

    pub fn stop_recording(&self) {
        *lock(&self.recorder) = None;
    }

    /// Pfad und bisherige Größe der laufenden Aufzeichnung.
    pub fn recording(&self) -> Option<(PathBuf, u64)> {
        lock(&self.recorder).as_ref().map(|r| (r.1.clone(), r.2))
    }
}

impl Drop for Engine {
    /// Beendet Lese- und Schreib-Thread und gibt damit die Schnittstelle frei.
    fn drop(&mut self) {
        self.stop_ntrip();
        self.stop.store(true, Ordering::Relaxed);
        for t in self.threads.drain(..) {
            let _ = t.join();
        }
    }
}

/// Liest die Werte einer Konfigurationsebene; leer, wenn der Empfänger nicht oder ablehnend antwortet.
fn read_layer(
    state: &Arc<Mutex<State>>,
    tx: &Sender<Vec<u8>>,
    stop: &Arc<AtomicBool>,
    layer: u8,
    keys: &[u32],
) -> Vec<(u32, u64)> {
    /// höchstens 64 Schlüssel je CFG-VALGET
    const MAX_KEYS: usize = 64;
    let mut result = Vec::new();
    for chunk in keys.chunks(MAX_KEYS) {
        {
            let mut s = lock(state);
            s.readback.clear();
            s.acks.clear();
        }
        if tx.send(ubx::valget_layer(layer, chunk)).is_err() {
            break;
        }
        let start = Instant::now();
        while start.elapsed() < ACK_TIMEOUT && !stop.load(Ordering::Relaxed) {
            thread::sleep(Duration::from_millis(10));
            let s = lock(state);
            if !s.readback.is_empty() {
                result.extend(s.readback.iter().copied());
                break;
            }
            // abgelehnt: Schlüssel fehlen in dieser Ebene
            if s.acks.iter().any(|a| a.0 == 0x06 && a.1 == 0x8B) {
                break;
            }
        }
    }
    result
}

/// Sendet die Schritte nacheinander, wartet je Schritt auf ACK/NAK und liest danach die RAM-Werte zurück.
fn run_config(
    state: &Arc<Mutex<State>>,
    tx: &Sender<Vec<u8>>,
    stop: &Arc<AtomicBool>,
    notify: &Notify,
    steps: Vec<ubx::Step>,
) {
    let mut unanswered = 0;
    for step in steps {
        lock(state).acks.clear();
        if tx.send(step.frame).is_err() {
            break;
        }
        let start = Instant::now();
        let mut answer = None;
        while answer.is_none() && start.elapsed() < ACK_TIMEOUT && !stop.load(Ordering::Relaxed) {
            thread::sleep(Duration::from_millis(10));
            answer = lock(state).acks.iter().find(|a| a.0 == 0x06 && a.1 == 0x8A).map(|a| a.2);
        }
        let mut s = lock(state);
        s.config_log.push((step.label.to_string(), answer));
        if answer != Some(true) {
            let why = if answer.is_some() { "abgelehnt" } else { "ohne Antwort" };
            s.event(format!("Konfiguration „{}“ {why}", step.label));
        }
        drop(s);
        notify();
        unanswered = if answer.is_none() { unanswered + 1 } else { 0 };
        if unanswered >= MAX_UNANSWERED || stop.load(Ordering::Relaxed) {
            break;
        }
    }
    let keys: Vec<u32> = ubx::READBACK_KEYS.iter().map(|k| k.0).collect();
    let _ = tx.send(ubx::valget(&keys));
    let _ = tx.send(ubx::frame(0x0A, 0x04, &[]));
    let mut s = lock(state);
    s.config_running = false;
    s.event("Konfiguration beendet");
    drop(s);
    notify();
}

fn read_loop(
    mut reader: Box<dyn Read + Send>,
    replay_speed: Option<f64>,
    state: Arc<Mutex<State>>,
    stop: Arc<AtomicBool>,
    recorder: Recorder,
    notify: Notify,
) {
    let stopped = || stop.load(Ordering::Relaxed);
    let mut demux = Demux::default();
    let mut buf = [0u8; READ_BUFFER];
    let size = if replay_speed.is_some() {
        REPLAY_BUFFER
    } else {
        READ_BUFFER
    };
    let mut last_itow = None;
    while !stopped() {
        match reader.read(&mut buf[..size]) {
            Ok(0) if replay_speed.is_some() => {
                let mut s = lock(&state);
                s.replay_done = true;
                s.event("Wiedergabe beendet");
                break;
            }
            Ok(0) => {}
            Ok(n) => {
                {
                    let mut r = lock(&recorder);
                    if let Some((f, _, bytes)) = r.as_mut() {
                        match f.write_all(&buf[..n]) {
                            Ok(()) => *bytes += n as u64,
                            Err(e) => {
                                lock(&state).event(format!("Aufzeichnung beendet: {e}"));
                                *r = None;
                            }
                        }
                    }
                }
                lock(&state).bytes_received += n as u64;
                for item in demux.feed(&buf[..n]) {
                    if let Some(speed) = replay_speed {
                        // neue Epoche (NAV-PVT): Abstand zur vorigen abwarten
                        if let Item::Ubx {
                            class: 0x01,
                            id: 0x07,
                            raw,
                        } = &item
                        {
                            if let Some(b) = raw.get(6..10) {
                                let itow = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
                                if let Some(prev) = last_itow {
                                    let gap = itow.saturating_sub(prev).min(REPLAY_MAX_GAP_MS);
                                    notify();
                                    if !pause(
                                        Duration::from_secs_f64(gap as f64 / 1000.0 / speed),
                                        stopped,
                                    ) {
                                        return;
                                    }
                                }
                                last_itow = Some(itow);
                            }
                        }
                    }
                    lock(&state).apply(item);
                }
                notify();
            }
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::TimedOut | std::io::ErrorKind::Interrupted
                ) => {}
            Err(e) => {
                let mut s = lock(&state);
                s.link_error = Some(format!("Verbindung zum Empfänger verloren: {e}"));
                s.event("Verbindung zum Empfänger verloren");
                break;
            }
        }
    }
    notify();
}

fn ntrip_loop(
    cfg: NtripConfig,
    gga_interval: Duration,
    state: Arc<Mutex<State>>,
    tx: Sender<Vec<u8>>,
    notify: Notify,
    forward: Arc<AtomicBool>,
    stopped: impl Fn() -> bool,
) {
    // Zustandsänderung nur, solange dieser Thread noch gültig ist
    let update = |f: &mut dyn FnMut(&mut State)| {
        let mut s = lock(&state);
        if !stopped() {
            f(&mut s);
        }
        drop(s);
        notify();
    };
    let mut backoff = NTRIP_BACKOFF_MIN;
    while !stopped() {
        // VRS braucht zuerst eine gültige Position
        let gga = loop {
            if let Some(g) = lock(&state)
                .fresh_gga(GGA_UPLOAD_MAX_AGE)
                .map(|g| g.raw.clone())
            {
                break g;
            }
            update(&mut |s| s.ntrip.phase = NtripPhase::WaitPosition);
            if !pause(Duration::from_millis(500), &stopped) {
                return;
            }
        };
        update(&mut |s| s.ntrip.phase = NtripPhase::Connecting);
        let mut transcript = Vec::new();
        let opened = cfg.open(Some(&gga), &mut transcript);
        update(&mut |s| {
            for (direction, line) in &transcript {
                s.comm_log(*direction, false, line.clone());
            }
        });
        let mut stream = match opened {
            Ok(s) => s,
            Err(e) => {
                let text = format!("{e:#}");
                // falsche Zugangsdaten nicht wiederholen (Gefahr der Sperre)
                let auth = text.contains("Authentifizierung");
                update(&mut |s| {
                    s.ntrip.phase = if auth {
                        NtripPhase::AuthError
                    } else {
                        NtripPhase::Error
                    };
                    s.ntrip.last_error = Some(text.clone());
                    s.event(text.clone());
                    s.comm_log(Direction::Info, false, text.clone());
                });
                if auth || !pause(backoff, &stopped) {
                    return;
                }
                backoff = (backoff * 2).min(NTRIP_BACKOFF_MAX);
                continue;
            }
        };
        update(&mut |s| {
            s.ntrip.phase = NtripPhase::Active;
            s.ntrip.connected_since = Some(Instant::now());
            s.ntrip.last_error = None;
            s.event("SAPOS verbunden");
        });
        let mut last_upload = Instant::now();
        let mut buf = [0u8; READ_BUFFER];
        let error = loop {
            let n = match stream.read(&mut buf) {
                Ok(0) => break "Verbindung vom Caster geschlossen".to_string(),
                Ok(n) => n,
                Err(e) => break format!("Lesefehler: {e}"),
            };
            if stopped() {
                return;
            }
            if forward.load(Ordering::Relaxed) && tx.send(buf[..n].to_vec()).is_err() {
                return;
            }
            backoff = NTRIP_BACKOFF_MIN;
            update(&mut |s| s.corrections(&buf[..n]));
            if last_upload.elapsed() >= gga_interval {
                // erst kopieren: die Sperre darf beim Senden und Protokollieren nicht mehr gehalten werden
                let gga = lock(&state)
                    .fresh_gga(GGA_UPLOAD_MAX_AGE)
                    .map(|g| g.raw.clone());
                if let Some(g) = gga {
                    if let Err(e) = stream.send_gga(&g) {
                        break format!("GGA-Upload: {e}");
                    }
                    update(&mut |s| s.comm_log(Direction::Sent, false, g.clone()));
                }
                last_upload = Instant::now();
            }
        };
        update(&mut |s| {
            s.ntrip.phase = NtripPhase::Error;
            s.ntrip.connected_since = None;
            s.ntrip.reconnects += 1;
            s.ntrip.last_error = Some(error.clone());
            s.event(format!("SAPOS: {error}"));
            s.comm_log(Direction::Info, false, error.clone());
        });
        if !pause(backoff, &stopped) {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gnss::kern::nmea;
    use std::net::TcpListener;

    /// Nachgebildeter Caster: Nach dem GGA-Upload muss der Zustand weiter zugänglich bleiben.
    #[test]
    fn gga_upload_does_not_block_state() {
        const WAIT: Duration = Duration::from_secs(3);
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            let mut request = [0u8; 2048];
            let _ = socket.read(&mut request).unwrap();
            socket.write_all(b"ICY 200 OK\r\n").unwrap();
            while socket.write_all(&[0u8; 16]).is_ok() {
                thread::sleep(Duration::from_millis(50));
            }
        });
        let state = Arc::new(Mutex::new(State::default()));
        let body =
            "GNGGA,101530.00,4848.1234567,N,01248.7654321,E,4,24,0.55,320.123,M,47.456,M,1.0,0000";
        let line = format!("${body}*{:02X}", nmea::checksum(body));
        lock(&state).apply(Item::Nmea(line.clone()));
        let stop = Arc::new(AtomicBool::new(false));
        let (tx, _rx) = mpsc::channel();
        let (st, sp) = (state.clone(), stop.clone());
        let cfg = NtripConfig {
            host: "127.0.0.1".into(),
            port,
            mountpoint: "m".into(),
            user: "u".into(),
            password: "p".into(),
            rev2: false,
        };
        thread::spawn(move || {
            ntrip_loop(
                cfg,
                Duration::from_millis(100),
                st,
                tx,
                Arc::new(|| {}),
                Arc::new(AtomicBool::new(true)),
                move || sp.load(Ordering::Relaxed),
            );
        });
        // der erste Upload steht nach der Anfrage im Protokoll, der zweite folgt im Abstand
        let uploads = || {
            let s = state.try_lock().ok()?;
            Some(
                s.comm
                    .iter()
                    .filter(|e| e.direction == Direction::Sent && e.text == line)
                    .count(),
            )
        };
        let start = Instant::now();
        let mut seen = 0;
        while seen < 3 && start.elapsed() < WAIT {
            thread::sleep(Duration::from_millis(20));
            seen = uploads().unwrap_or(seen);
        }
        stop.store(true, Ordering::Relaxed);
        assert!(seen >= 3, "nur {seen} GGA-Uploads, Zustand blockiert");
    }
}
