//! Zweck:    GNSS-Fenster in Feldbuch-NT: Menüs, Reiter, Statusleiste und Ausführung der Benutzeraktionen;
//!           dazu die Schnittstelle für Feldbuch-NT (Live-Position, Messung aus dem DXF-Viewer).
//! Hardware: simpleRTK2B (u-blox ZED-F9P)
//! Autor:    Johann Gerner
//!
//! Übernommen aus gnss-vermessung 0.4.0 (src/bin/gnss-gui/app.rs), eigenständig gepflegt in Feldbuch-NT.

use crate::gnss::ansicht::view_track::{self, Track};
use crate::gnss::ansicht::view_config::{self, ConfigTab, ConfigView, ToolsView};
use crate::gnss::ansicht::view_dop_anim::DopAnim;
use crate::gnss::ansicht::view_map::{self, MapView};
use crate::gnss::ansicht::view_measure::{self, MeasureView, Pending};
use crate::gnss::ansicht::view_ntrip::NtripView;
use crate::gnss::ansicht::view_raw::{self, RawView};
use crate::gnss::ansicht::view_sky::{self, SkyView};
use crate::gnss::ansicht::widgets::*;
use crate::gnss::ansicht::{view_ntrip, view_status, view_time};
use chrono::Local;
use eframe::egui::{self, Key};
use crate::gnss::kern::config::*;
use crate::gnss::kern::engine::{Engine, Source};
use crate::gnss::kern::export;
use crate::gnss::kern::height::{Grid, HeightModel};
use crate::gnss::kern::measure::{Criteria, Measurement, PointRequest};
use crate::gnss::kern::model::{Fix, State};
use crate::gnss::kern::ntrip::NtripConfig;
use crate::gnss::kern::settings::{self, HeightMode, Settings};
use crate::gnss::kern::survey::PointResult;
use crate::gnss::kern::{survey, ubx, version};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

const IDLE_REPAINT: Duration = Duration::from_secs(1);
const SIDE_WIDTH: f32 = 250.0;
const REPLAY_SPEEDS: [f64; 4] = [1.0, 2.0, 10.0, 50.0];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tab {
    Status,
    Field,
    Measure,
    Sky,
    Dop,
    Time,
    Map,
    Track,
    Ntrip,
    Raw,
    Config,
    Tools,
    Help,
}

const TABS: [(Tab, &str); 13] = [
    (Tab::Status, "Status"),
    (Tab::Field, "Großanzeige"),
    (Tab::Measure, "Messen"),
    (Tab::Sky, "Satelliten"),
    (Tab::Dop, "DOP"),
    (Tab::Time, "Zeit"),
    (Tab::Map, "Karte"),
    (Tab::Track, "Spur"),
    (Tab::Ntrip, "Korrekturdaten"),
    (Tab::Raw, "Rohdaten"),
    (Tab::Config, "Konfiguration"),
    (Tab::Tools, "Werkzeuge"),
    (Tab::Help, "Hilfe"),
];

/// Wünsche der Ansichten; ausgeführt, nachdem der Zustand freigegeben ist.
pub enum Action {
    Connect,
    Disconnect,
    NtripStart,
    NtripStop,
    Configure { persist: bool },
    OpenReplay,
    StartRecording,
    StopRecording,
    ClearRaw,
    ClearComm,
    ClearTrack,
    SetForward(bool),
    SaveComm,
    StartRtcmRecording,
    StopRtcmRecording,
    StartMeasure,
    CancelMeasure,
    SavePoint { replace: bool },
    RepeatPoint,
    DiscardPoint,
    ChooseOutput { new: bool },
    ChooseGeoid,
    ReloadPoints,
    SaveSettings,
    ReloadSettings,
    AskDefaults,
    FetchSourcetable,
}

type Sourcetable = Result<Vec<(String, String)>, String>;

pub struct App {
    settings: Settings,
    password: String,
    engine: Option<Engine>,
    /// vom Benutzer gewünschte Verbindung (für die Wiederverbindung)
    want_connection: bool,
    last_attempt: Instant,
    /// `false`, wenn die vorhandene Datei fehlerhaft ist und nicht überschrieben werden darf
    settings_ok: bool,
    replay_speed: f64,
    tab: Tab,
    message: Option<(String, bool)>,
    height: HeightModel,
    height_key: Option<(HeightMode, String, u64)>,
    raw: RawView,
    sky: SkyView,
    dop_anim: DopAnim,
    track: Track,
    ntrip_view: NtripView,
    map: MapView,
    measure: MeasureView,
    config: ConfigView,
    tools: ToolsView,
    sourcetable: Option<Receiver<Sourcetable>>,
    confirm_defaults: bool,
    /// Angebot, die Einstellungen des eigenständigen gnss-vermessung zu übernehmen
    import_offer: bool,
    last_fix: Fix,
    /// Die laufende Messung gehört Feldbuch-NT (DXF-Viewer), nicht dem Reiter „Messen“
    extern_measure: bool,
    /// Ergebnis einer Messung für Feldbuch-NT
    extern_result: Option<ExternResult>,
}

/// Ergebnis einer Messung, die Feldbuch-NT gestartet hat.
pub struct ExternResult {
    pub measurement: Measurement,
    pub result: PointResult,
    /// Mountpoint, Firmware, Höhenbezug usw. für das Protokoll
    pub protocol: Vec<(&'static str, String)>,
    pub replay: bool,
}

/// Fortschritt einer laufenden Messung für Feldbuch-NT.
pub struct ExternProgress {
    pub accepted: usize,
    pub epochs: usize,
    pub seconds: f64,
    pub min_seconds: f64,
    pub last_rejection: Option<&'static str>,
}

#[cfg(windows)]
fn tone(sequence: &'static [(u32, u32)]) {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn Beep(frequency: u32, duration_ms: u32) -> i32;
    }
    std::thread::spawn(move || {
        for &(f, d) in sequence {
            // SAFETY: Beep erhält nur Zahlenwerte
            unsafe { Beep(f, d) };
        }
    });
}

#[cfg(not(windows))]
fn tone(_: &'static [(u32, u32)]) {}

const TONE_FIX: &[(u32, u32)] = &[(880, 120), (1320, 160)];
const TONE_LOST: &[(u32, u32)] = &[(660, 200), (440, 300)];
const TONE_DONE: &[(u32, u32)] = &[(1000, 100), (1000, 100), (1500, 200)];

impl App {
    pub fn new(ctx: &egui::Context) -> Self {
        let import_offer = Settings::import_possible();
        let (settings, message) = match Settings::load() {
            Ok(s) => (s, None),
            Err(e) => (
                Settings::default(),
                Some((format!("Einstellungen: {e:#}"), true)),
            ),
        };
        let settings_ok = message.is_none();
        let password = settings::load_password(&settings.user).unwrap_or_default();
        let mut app = App {
            password,
            engine: None,
            want_connection: false,
            last_attempt: Instant::now(),
            settings_ok,
            replay_speed: 1.0,
            tab: Tab::Status,
            message,
            height: HeightModel::None,
            height_key: None,
            raw: RawView::default(),
            sky: SkyView {
                trails: true,
                ..Default::default()
            },
            dop_anim: DopAnim::default(),
            track: Track::default(),
            ntrip_view: NtripView::default(),
            map: MapView::default(),
            measure: MeasureView::default(),
            config: ConfigView {
                ports: view_config::list_ports(),
                ..Default::default()
            },
            tools: ToolsView::default(),
            sourcetable: None,
            confirm_defaults: false,
            import_offer,
            last_fix: Fix::None,
            extern_measure: false,
            extern_result: None,
            settings,
        };
        app.reload_points();
        if app.settings.auto_connect && !app.import_offer {
            app.connect(ctx);
        }
        app
    }

    fn info(&mut self, text: impl Into<String>) {
        self.message = Some((text.into(), false));
    }

    fn error(&mut self, text: impl Into<String>) {
        self.message = Some((text.into(), true));
    }

    fn notify(ctx: &egui::Context) -> crate::gnss::kern::engine::Notify {
        let ctx = ctx.clone();
        Arc::new(move || ctx.request_repaint())
    }

    fn connect(&mut self, ctx: &egui::Context) {
        if self.settings.device.is_empty() {
            self.tab = Tab::Config;
            self.config.tab = ConfigTab::Port;
            self.error("Bitte zuerst eine Schnittstelle wählen.");
            return;
        }
        self.engine = None;
        self.last_attempt = Instant::now();
        let source = Source::Serial {
            device: self.settings.device.clone(),
            baud: self.settings.baud,
        };
        match Engine::start(source, Self::notify(ctx)) {
            Ok(engine) => {
                self.engine = Some(engine);
                self.want_connection = true;
                self.info(format!("Verbunden mit {}", self.settings.device));
                // Konfiguration dauerhaft im Empfänger, Flash nur bei Änderungen
                if self.settings.configure_on_connect
                    && let Some(e) = &self.engine
                {
                    e.configure_persistent(self.receiver_setup());
                }
                if self.settings.ntrip_autostart && !self.settings.user.is_empty() {
                    self.ntrip_start();
                }
            }
            Err(e) => self.error(format!("{e:#}")),
        }
    }

    fn disconnect(&mut self) {
        self.track.clear();
        self.want_connection = false;
        self.engine = None;
        self.info("Getrennt");
    }

    fn start_replay(&mut self, ctx: &egui::Context, path: PathBuf) {
        self.track.clear();
        self.want_connection = false;
        self.engine = None;
        match Engine::start(
            Source::Replay {
                path: path.clone(),
                speed: self.replay_speed,
            },
            Self::notify(ctx),
        ) {
            Ok(e) => {
                self.engine = Some(e);
                self.info(format!("Wiedergabe {}", path.display()));
            }
            Err(e) => self.error(format!("{e:#}")),
        }
    }

    fn receiver_setup(&self) -> ubx::ReceiverSetup {
        ubx::ReceiverSetup {
            port: ubx::Port::Usb,
            rate_ms: self.settings.rate_ms,
            dyn_model: self.settings.dyn_model,
            elevation_mask: self.settings.elevation_mask,
        }
    }

    fn configure(&mut self, persist: bool) {
        let setup = self.receiver_setup();
        let layers = ubx::LAYER_RAM
            | if persist {
                ubx::LAYER_BBR | ubx::LAYER_FLASH
            } else {
                0
            };
        if let Some(e) = &self.engine {
            e.configure(setup, layers);
        }
    }

    fn ntrip_config(&self) -> NtripConfig {
        NtripConfig {
            host: self.settings.caster.trim().to_string(),
            port: self.settings.caster_port,
            mountpoint: self.settings.mountpoint.trim().to_string(),
            user: self.settings.user.trim().to_string(),
            password: self.password.clone(),
            rev2: !self.settings.rev1,
        }
    }

    fn ntrip_start(&mut self) {
        if self.settings.user.is_empty() || self.password.is_empty() {
            self.error("SAPOS-Benutzer und Passwort fehlen (Konfiguration → SAPOS).");
            return;
        }
        let cfg = self.ntrip_config();
        let interval = Duration::from_secs(self.settings.gga_interval_s.max(1));
        match &mut self.engine {
            Some(e) if !e.replay => e.start_ntrip(cfg, interval),
            _ => self.error("SAPOS braucht eine Verbindung zum Empfänger."),
        }
    }

    /// Lädt das Höhenmodell neu, wenn sich die Einstellungen geändert haben.
    fn update_height(&mut self) {
        if self.settings.sapos_transform {
            // Gitter wechselt mit der Position: bei jedem Durchlauf neu übernehmen
            self.height_key = None;
            self.height = match &self.engine {
                Some(e) => {
                    let st = e.state();
                    let geoid_sep = st.gga.as_ref().map(|g| g.0.geoid_sep);
                    HeightModel::Sapos(Box::new((st.ntrip.content.transform.clone(), geoid_sep)))
                }
                None => HeightModel::None,
            };
            return;
        }
        let s = &self.settings;
        let key = (s.height_mode, s.geoid_file.clone(), s.zeta.to_bits());
        if self.height_key.as_ref() == Some(&key) {
            return;
        }
        self.height_key = Some(key);
        self.height = match s.height_mode {
            HeightMode::Ellipsoidal => HeightModel::None,
            HeightMode::Constant => HeightModel::Constant(s.zeta),
            HeightMode::Grid if s.geoid_file.trim().is_empty() => HeightModel::None,
            HeightMode::Grid => {
                match Grid::load(Path::new(s.geoid_file.trim().trim_matches('"'))) {
                    Ok(g) => HeightModel::Grid(g),
                    Err(e) => {
                        self.message = Some((format!("{e:#}"), true));
                        HeightModel::None
                    }
                }
            }
        };
    }

    fn reload_points(&mut self) {
        match export::load(Path::new(&self.settings.output)) {
            Ok(p) => {
                self.measure.points = p;
                self.measure.points_error = None;
            }
            Err(e) => {
                self.measure.points.clear();
                self.measure.points_error = Some(format!("{e:#}"));
            }
        }
    }

    fn start_measure(&mut self) {
        if self.extern_measure {
            self.error("Eine Messung aus dem DXF-Viewer läuft.");
            return;
        }
        let nr = export::clean(&self.measure.nr);
        if nr.is_empty() {
            self.error("Punktnummer fehlt.");
            return;
        }
        if let Err(e) = self.check_height_ready() {
            self.error(e);
            return;
        }
        let Some(engine) = &self.engine else {
            self.error("Nicht verbunden.");
            return;
        };
        let request = PointRequest {
            nr: nr.clone(),
            code: self.measure.code.clone(),
            remark: self.measure.remark.clone(),
            pole_height: self.settings.pole_height,
            antenna_offset: self.settings.antenna_offset,
        };
        self.measure.pending = None;
        engine.state().measurement = Some(Measurement::new(request, self.settings.criteria()));
        self.info(format!("Messung Punkt {nr} läuft"));
    }

    /// Prüft, ob der eingestellte Höhenbezug verfügbar ist.
    fn check_height_ready(&self) -> Result<(), String> {
        if self.settings.sapos_transform {
            let check = self.engine.as_ref().map(|e| {
                let st = e.state();
                let geoid_sep = st.gga.as_ref().map(|g| g.0.geoid_sep);
                match &st.solution {
                    Some(sol) => st.ntrip.content.transform.height(sol.lat, sol.lon, geoid_sep).map(|_| ()),
                    None => Err("keine Position".to_string()),
                }
            });
            if let Some(Err(why)) = check {
                return Err(format!("SAPOS-Höhentransformation nicht verfügbar: {why}. Messung nicht gestartet."));
            }
        }
        if self.settings.height_mode == HeightMode::Grid
            && !matches!(self.height, HeightModel::Grid(_))
        {
            return Err("Quasigeoid-Gitter ist nicht geladen. Messung nicht gestartet.".into());
        }
        Ok(())
    }

    /// Macht aus einer abgeschlossenen Messung ein Ergebnis zum Speichern.
    fn collect_result(&mut self) {
        if self.extern_measure {
            self.collect_extern();
            return;
        }
        let Some(engine) = &self.engine else { return };
        let (measurement, mountpoint, firmware, gdop) = {
            let mut st = engine.state();
            if !st.measurement.as_ref().is_some_and(|m| m.done) {
                return;
            }
            let firmware = st
                .version
                .as_ref()
                .map(|v| v.2.join(" "))
                .unwrap_or_default();
            let gdop = st.receiver_dop.map(|d| d.g);
            let mountpoint = st.ntrip.mountpoint.clone();
            match st.measurement.take() {
                Some(m) => (m, mountpoint, firmware, gdop),
                None => return,
            }
        };
        let replay = engine.replay;
        let Some(r) = measurement.result(&self.height) else {
            return;
        };
        if (self.settings.sapos_transform || self.settings.height_mode != HeightMode::Ellipsoidal) && r.h_dhhn.is_none() {
            self.error("Punkt liegt außerhalb des Quasigeoid-Gitters. Ergebnis verworfen.");
            return;
        }
        let x = if self.settings.zone_prefix {
            r.east
        } else {
            r.east - f64::from(UTM_ZONE) * 1e6
        };
        let protocol = self.protocol(&measurement, mountpoint, firmware, gdop);
        let point = export::Point {
            nr: measurement.request.nr.clone(),
            x,
            y: r.north,
            z: r.h_dhhn.unwrap_or(r.h_ell),
            code: export::clean(&measurement.request.code),
            remark: measurement.request.remark.clone(),
            source: if replay {
                "WIEDERGABE".into()
            } else {
                "GNSS RTK".into()
            },
            timestamp: Local::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, false),
            protocol,
        };
        self.reload_points();
        let existing = export::load(&self.target(&point.source))
            .unwrap_or_default()
            .into_iter()
            .find(|p| p.nr.eq_ignore_ascii_case(&point.nr));
        self.measure.pending = Some(Pending {
            point,
            sd: (r.sd_e, r.sd_n, r.sd_h),
            measurement,
            existing,
        });
        self.tab = Tab::Measure;
        if self.settings.sounds {
            tone(TONE_DONE);
        }
    }

    /// Bezeichnung des eingestellten Höhenbezugs.
    pub fn height_text(&self) -> String {
        match self.settings.height_mode {
            _ if self.settings.sapos_transform => "SAPOS RTCM 1021/1023 (GCG2016)".to_string(),
            HeightMode::Ellipsoidal => "ellipsoidisch GRS80".to_string(),
            HeightMode::Grid => format!("Gitter {}", self.settings.geoid_file),
            HeightMode::Constant => format!("konstant zeta {:.4}", self.settings.zeta),
        }
    }

    /// Protokollangaben einer abgeschlossenen Messung.
    fn protocol(
        &self,
        measurement: &Measurement,
        mountpoint: String,
        firmware: String,
        gdop: Option<f64>,
    ) -> Vec<(&'static str, String)> {
        let Some(r) = measurement.result(&self.height) else {
            return Vec::new();
        };
        let epochs = &measurement.accepted;
        let mean_pdop = epochs.iter().map(|e| e.1.pdop).sum::<f64>() / epochs.len() as f64;
        let min_sats = epochs.iter().map(|e| e.1.num_sv).min().unwrap_or(0);
        let max_age = epochs.iter().filter_map(|e| e.1.age).fold(0.0, f64::max);
        let mean = |f: fn(&crate::gnss::kern::model::Solution) -> Option<f64>| {
            epochs.iter().filter_map(|e| f(&e.1)).sum::<f64>() / epochs.len() as f64
        };
        let c = &measurement.criteria;
        let height = self.height_text();
        vec![
            ("h_ell", format!("{:.4}", r.h_ell)),
            ("sOst_mm", format!("{:.1}", r.sd_e * 1e3)),
            ("sNord_mm", format!("{:.1}", r.sd_n * 1e3)),
            ("sHoehe_mm", format!("{:.1}", r.sd_h * 1e3)),
            (
                "Sigma2D_Empf_mm",
                format!("{:.1}", mean(|s| s.sigma_2d()) * 1e3),
            ),
            (
                "SigmaH_Empf_mm",
                format!("{:.1}", mean(|s| s.sigma_h()) * 1e3),
            ),
            ("Epochen", r.n.to_string()),
            ("verworfen", r.rejected.to_string()),
            ("Neubeginne", measurement.restarts.to_string()),
            ("Dauer_s", format!("{:.1}", measurement.seconds())),
            (
                "Stabhoehe_m",
                format!("{:.4}", measurement.request.pole_height),
            ),
            (
                "Antennenoffset_m",
                format!("{:.4}", measurement.request.antenna_offset),
            ),
            ("Hoehenbezug", height),
            ("PDOP_Mittel", format!("{mean_pdop:.2}")),
            (
                "GDOP_Ende",
                gdop.map_or_else(String::new, |g| format!("{g:.2}")),
            ),
            ("Sat_min", min_sats.to_string()),
            ("Korrekturalter_max_s", format!("{max_age:.1}")),
            ("Mountpoint", mountpoint),
            (
                "Referenzstation",
                epochs
                    .last()
                    .and_then(|e| e.1.ref_station.clone())
                    .unwrap_or_default(),
            ),
            (
                "Grenzen",
                format!(
                    "s2D {} sH {} Alter {} PDOP {} Sat {}",
                    c.max_sigma_2d, c.max_sigma_h, c.max_age, c.max_pdop, c.min_sats
                ),
            ),
            ("Firmware", firmware),
            ("Programm", version::FULL.to_string()),
        ]
    }

    /// Zieldatei; Punkte aus der Wiedergabe kommen in eine eigene Datei.
    fn target(&self, source: &str) -> PathBuf {
        let files = export::files(Path::new(&self.settings.output));
        if source != "WIEDERGABE" {
            return files.csv;
        }
        let name = files
            .kor
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        files.kor.with_file_name(format!("{name}-wiedergabe"))
    }

    fn save_point(&mut self, replace: bool) {
        let Some(p) = &self.measure.pending else {
            return;
        };
        let target = self.target(&p.point.source);
        let meta = export::Meta {
            project: self.settings.project.clone(),
            operator: self.settings.operator.clone(),
            date: Local::now().format("%Y-%m-%d").to_string(),
            coord_system: "ETRS89/UTM32".into(),
            height_system: if !self.settings.sapos_transform && self.settings.height_mode == HeightMode::Ellipsoidal {
                "ellipsoidisch GRS80".into()
            } else {
                "DHHN2016".into()
            },
        };
        match export::save(&target, &meta, &p.point, replace) {
            Ok(()) => {
                let nr = p.point.nr.clone();
                self.info(format!(
                    "Punkt {nr} gespeichert in {}",
                    export::files(&target).csv.display()
                ));
                self.measure.nr = survey::next_pnr(&nr);
                self.measure.remark.clear();
                self.measure.pending = None;
                self.reload_points();
            }
            // Ergebnis bleibt erhalten, damit erneut gespeichert werden kann
            Err(e) => self.error(format!("Nicht gespeichert: {e:#}")),
        }
    }

    fn save_settings(&mut self) {
        match self
            .settings
            .save()
            .and_then(|()| settings::save_password(self.settings.user.trim(), &self.password))
        {
            Ok(()) => {
                self.settings_ok = true;
                self.info("Einstellungen gespeichert");
            }
            Err(e) => self.error(format!("{e:#}")),
        }
    }

    fn run_action(&mut self, ctx: &egui::Context, action: Action) {
        match action {
            Action::Connect => self.connect(ctx),
            Action::Disconnect => self.disconnect(),
            Action::NtripStart => self.ntrip_start(),
            Action::NtripStop => {
                if let Some(e) = &mut self.engine {
                    e.stop_ntrip();
                }
            }
            Action::Configure { persist } => self.configure(persist),
            Action::OpenReplay => {
                if let Some(p) = rfd::FileDialog::new()
                    .add_filter("Aufzeichnung", &["ubx", "bin", "log"])
                    .pick_file()
                {
                    self.start_replay(ctx, p);
                }
            }
            Action::StartRecording => {
                if self.engine.is_none() {
                    self.error("Nicht verbunden.");
                    return;
                }
                let name = format!("rohdaten_{}.ubx", Local::now().format("%Y%m%d_%H%M%S"));
                if let Some(p) = rfd::FileDialog::new()
                    .add_filter("Rohdaten", &["ubx"])
                    .set_file_name(name)
                    .save_file()
                    && let Some(Err(e)) = self.engine.as_ref().map(|en| en.start_recording(&p))
                {
                    self.error(format!("{e:#}"));
                }
            }
            Action::StopRecording => {
                if let Some(e) = &self.engine {
                    e.stop_recording();
                }
            }
            Action::ClearTrack => self.track.clear(),
            Action::SetForward(on) => {
                if let Some(e) = &self.engine {
                    e.set_forward(on);
                }
            }
            Action::ClearComm => {
                if let Some(e) = &self.engine {
                    e.state().comm.clear();
                }
            }
            Action::SaveComm => {
                let Some(e) = &self.engine else { return };
                let text = view_ntrip::log_text(&e.state());
                let name = format!("sapos_{}.txt", Local::now().format("%Y%m%d_%H%M%S"));
                if let Some(p) = rfd::FileDialog::new()
                    .add_filter("Text", &["txt"])
                    .set_file_name(name)
                    .save_file()
                {
                    match std::fs::write(&p, text) {
                        Ok(()) => self.info(format!("Protokoll gespeichert: {}", p.display())),
                        Err(e) => self.error(format!("Protokoll nicht gespeichert: {e}")),
                    }
                }
            }
            Action::StartRtcmRecording => {
                if self.engine.is_none() {
                    self.error("Nicht verbunden.");
                    return;
                }
                let name = format!("sapos_{}.rtcm3", Local::now().format("%Y%m%d_%H%M%S"));
                let Some(p) = rfd::FileDialog::new()
                    .add_filter("RTCM", &["rtcm3"])
                    .set_file_name(name)
                    .save_file()
                else {
                    return;
                };
                match std::fs::File::create(&p) {
                    Ok(file) => {
                        if let Some(e) = &self.engine {
                            e.state().rtcm_file = Some((file, p, 0));
                        }
                    }
                    Err(e) => self.error(format!("RTCM-Mitschnitt nicht anlegbar: {e}")),
                }
            }
            Action::StopRtcmRecording => {
                if let Some(e) = &self.engine {
                    e.state().rtcm_file = None;
                }
            }
            Action::ClearRaw => {
                if let Some(e) = &self.engine {
                    e.state().raw.clear();
                }
            }
            Action::StartMeasure => self.start_measure(),
            Action::CancelMeasure => {
                if let Some(e) = &self.engine {
                    e.state().measurement = None;
                }
                self.extern_measure = false;
                self.info("Messung abgebrochen");
            }
            Action::SavePoint { replace } => self.save_point(replace),
            Action::RepeatPoint => {
                if self.measure.pending.take().is_some() {
                    self.start_measure();
                }
            }
            Action::DiscardPoint => {
                self.measure.pending = None;
                self.info("Ergebnis verworfen");
            }
            Action::ChooseOutput { new } => {
                let current = export::files(Path::new(&self.settings.output)).csv;
                let mut d = rfd::FileDialog::new().add_filter("Koordinaten", &["csv", "kor"]);
                if let Some(dir) = current.parent().filter(|d| d.is_dir()) {
                    d = d.set_directory(dir);
                }
                let chosen = if new {
                    d.set_file_name("messung-kor.csv").save_file()
                } else {
                    d.pick_file()
                };
                if let Some(p) = chosen {
                    self.settings.output = export::files(&p).csv.display().to_string();
                    self.reload_points();
                }
            }
            Action::ChooseGeoid => {
                if let Some(p) = rfd::FileDialog::new().pick_file() {
                    self.settings.geoid_file = p.display().to_string();
                }
            }
            Action::ReloadPoints => self.reload_points(),
            Action::SaveSettings => self.save_settings(),
            Action::ReloadSettings => match Settings::load() {
                Ok(s) => {
                    self.settings = s;
                    self.password =
                        settings::load_password(&self.settings.user).unwrap_or_default();
                    self.reload_points();
                    self.info("Einstellungen neu geladen");
                }
                Err(e) => self.error(format!("{e:#}")),
            },
            Action::AskDefaults => self.confirm_defaults = true,
            Action::FetchSourcetable => {
                let cfg = self.ntrip_config();
                let (tx, rx) = mpsc::channel();
                let ctx = ctx.clone();
                std::thread::spawn(move || {
                    let result = cfg
                        .sourcetable()
                        .map_err(|e| format!("{e:#}"))
                        .map(|lines| {
                            lines
                                .iter()
                                .map(|l| {
                                    let f: Vec<&str> = l.split(';').collect();
                                    let get =
                                        |i: usize| f.get(i).copied().unwrap_or("").to_string();
                                    (get(1), format!("{} {}", get(3), get(6)))
                                })
                                .collect()
                        });
                    let _ = tx.send(result);
                    ctx.request_repaint();
                });
                self.sourcetable = Some(rx);
                self.config.sourcetable_status = "frage ab …".into();
            }
        }
    }

    fn poll(&mut self, ctx: &egui::Context) {
        if let Some(rx) = &self.sourcetable
            && let Ok(result) = rx.try_recv()
        {
            match result {
                Ok(list) if list.is_empty() => {
                    self.config.sourcetable_status = "Antwort ohne Mountpoints".into()
                }
                Ok(list) => {
                    self.config.sourcetable_status =
                        format!("{} Mountpoints empfangen", list.len());
                    self.config.mountpoints = list;
                }
                Err(e) => self.config.sourcetable_status = e,
            }
            self.sourcetable = None;
        }
        // Verbindungsabbruch erkennen und neu verbinden
        if let Some(e) = &self.engine {
            self.track.tick(&e.state());
        }
        let lost = self
            .engine
            .as_ref()
            .and_then(|e| e.state().link_error.clone());
        if let Some(e) = lost {
            self.engine = None;
            self.extern_measure = false;
            self.error(e);
        }
        if self.want_connection
            && self.engine.is_none()
            && self.settings.auto_reconnect
            && self.last_attempt.elapsed() >= RECONNECT_INTERVAL
        {
            self.connect(ctx);
        }
        let fix = self
            .engine
            .as_ref()
            .and_then(|e| e.state().solution.as_ref().map(|s| s.fix))
            .unwrap_or_default();
        if self.settings.sounds {
            if fix == Fix::Fixed && self.last_fix != Fix::Fixed {
                tone(TONE_FIX);
            } else if fix != Fix::Fixed && self.last_fix == Fix::Fixed {
                tone(TONE_LOST);
            }
        }
        self.last_fix = fix;
        if let Some(e) = &self.engine {
            let mut st = e.state();
            st.dop_mask = self.settings.display_mask;
            st.dop_clock_per_system = self.settings.clock_per_system;
        }
        self.update_height();
        self.collect_result();
    }

    fn keys(&mut self, ctx: &egui::Context, actions: &mut Vec<Action>) {
        // Tasten gelten nicht, solange ein Eingabefeld oder Element den Fokus hat
        let focused = ctx.memory(|m| m.focused().is_some());
        let (f5, space, enter, esc) = ctx.input(|i| {
            (
                i.key_pressed(Key::F5),
                i.key_pressed(Key::Space),
                i.key_pressed(Key::Enter),
                i.key_pressed(Key::Escape),
            )
        });
        if f5 {
            actions.push(if self.engine.is_some() {
                Action::Disconnect
            } else {
                Action::Connect
            });
        }
        if focused {
            return;
        }
        let measuring = self
            .engine
            .as_ref()
            .is_some_and(|e| e.state().measurement.is_some());
        if space && !measuring && self.measure.pending.is_none() {
            self.tab = Tab::Measure;
            actions.push(Action::StartMeasure);
        }
        // Ersetzen vorhandener Punkte nur per Mausklick
        if enter
            && self
                .measure
                .pending
                .as_ref()
                .is_some_and(|p| p.existing.is_none())
        {
            actions.push(Action::SavePoint { replace: false });
        }
        if esc && measuring {
            actions.push(Action::CancelMeasure);
        }
    }

    fn menu(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        let connected = self.engine.is_some();
        egui::MenuBar::new().ui(ui, |ui| {
            let item = |ui: &mut egui::Ui, label: &str, enabled: bool| {
                let clicked = ui.add_enabled(enabled, egui::Button::new(label)).clicked();
                if clicked {
                    ui.close();
                }
                clicked
            };
            ui.menu_button("Datei", |ui| {
                if item(ui, "Punktdatei öffnen …", true) {
                    actions.push(Action::ChooseOutput { new: false });
                }
                if item(ui, "Neue Punktdatei …", true) {
                    actions.push(Action::ChooseOutput { new: true });
                }
                ui.separator();
                ui.menu_button("Wiedergabegeschwindigkeit", |ui| {
                    for v in REPLAY_SPEEDS {
                        ui.radio_value(&mut self.replay_speed, v, format!("{v}-fach"));
                    }
                });
                if item(ui, "Rohdaten wiedergeben …", true) {
                    actions.push(Action::OpenReplay);
                }
            });
            ui.menu_button("Verbindung", |ui| {
                if item(ui, "Verbinden (F5)", !connected) {
                    actions.push(Action::Connect);
                }
                if item(ui, "Trennen (F5)", connected) {
                    actions.push(Action::Disconnect);
                }
                ui.separator();
                if item(ui, "SAPOS starten", connected) {
                    actions.push(Action::NtripStart);
                }
                if item(ui, "SAPOS stoppen", connected) {
                    actions.push(Action::NtripStop);
                }
            });
            ui.menu_button("Konfiguration", |ui| {
                for (tab, label) in [
                    (ConfigTab::Port, "Schnittstelle"),
                    (ConfigTab::Sapos, "SAPOS"),
                    (ConfigTab::Receiver, "Empfänger"),
                    (ConfigTab::Measure, "Messvorgaben"),
                    (ConfigTab::Antenna, "Antenne und Höhe"),
                    (ConfigTab::Output, "Ausgabe"),
                    (ConfigTab::Display, "Darstellung"),
                ] {
                    if item(ui, label, true) {
                        self.tab = Tab::Config;
                        self.config.tab = tab;
                    }
                }
                ui.separator();
                if item(ui, "Einstellungen speichern", true) {
                    actions.push(Action::SaveSettings);
                }
            });
            ui.menu_button("Messung", |ui| {
                if item(ui, "Punkt messen (Leertaste)", connected) {
                    self.tab = Tab::Measure;
                    actions.push(Action::StartMeasure);
                }
                if item(ui, "Abbrechen (Esc)", connected) {
                    actions.push(Action::CancelMeasure);
                }
                if item(ui, "Punktliste", true) {
                    self.tab = Tab::Measure;
                }
            });
            ui.menu_button("Ansicht", |ui| {
                for (tab, label) in TABS {
                    if item(ui, label, true) {
                        self.tab = tab;
                    }
                }
            });
            ui.menu_button("Werkzeuge", |ui| {
                if item(ui, "Koordinatenumrechnung", true) {
                    self.tab = Tab::Tools;
                }
                if item(ui, "Rohdaten aufzeichnen …", connected) {
                    actions.push(Action::StartRecording);
                }
            });
            ui.menu_button("Hilfe", |ui| {
                if item(ui, "Kurzanleitung und Info", true) {
                    self.tab = Tab::Help;
                }
            });
        });
    }

    fn status_bar(&self, ui: &mut egui::Ui, st: &State) {
        ui.horizontal(|ui| {
            match &self.engine {
                Some(e) if e.replay => {
                    let end = if st.replay_done { " (Ende)" } else { "" };
                    ui.colored_label(PURPLE, format!("▶ WIEDERGABE{end}"));
                }
                Some(_) if st.is_stale() => {
                    ui.colored_label(WARN, format!("● {} ohne Daten", self.settings.device));
                }
                Some(_) => {
                    ui.colored_label(GOOD, format!("● {}", self.settings.device));
                }
                None => {
                    ui.colored_label(STALE, "○ nicht verbunden");
                }
            }
            ui.separator();
            ui.label(format!(
                "SAPOS {} {:.1} kB/s",
                st.ntrip.phase.label(),
                st.ntrip.rate / 1024.0
            ));
            if let Some(s) = &st.solution {
                ui.separator();
                ui.colored_label(fix_color(s.fix), s.fix.label());
                ui.label(format!("Alter {} s", opt(s.age, 1)));
                ui.label(format!(
                    "Sat {}/{}",
                    s.num_sv,
                    st.sats.iter().filter(|s| s.cno > 0).count()
                ));
                ui.label(format!("GDOP {}", opt(st.receiver_dop.map(|d| d.g), 2)));
            }
            if let Some(t) = st
                .time_utc
                .as_ref()
                .and_then(|t| crate::gnss::kern::gtime::utc_datetime(&t.0))
            {
                ui.separator();
                ui.label(format!("UTC {}", t.format("%H:%M:%S")));
            }
            if let Some((text, is_error)) = &self.message {
                ui.separator();
                let color = if *is_error {
                    BAD
                } else {
                    ui.visuals().text_color()
                };
                ui.add(egui::Label::new(egui::RichText::new(text).color(color)).truncate())
                    .on_hover_text(text);
            }
        });
    }

    fn dialogs(&mut self, ctx: &egui::Context) {
        if self.confirm_defaults {
            egui::Window::new("Werkseinstellungen").collapsible(false).show(ctx, |ui| {
                ui.label("Alle Einstellungen auf die Vorgaben zurücksetzen?\nDas gespeicherte Passwort und die Punktdateien bleiben erhalten.");
                ui.horizontal(|ui| {
                    if ui.button("Zurücksetzen").clicked() {
                        self.settings = Settings::default();
                        self.confirm_defaults = false;
                        self.reload_points();
                        self.save_settings();
                    }
                    if ui.button("Abbrechen").clicked() {
                        self.confirm_defaults = false;
                    }
                });
            });
        }
        if self.import_offer {
            let mut done = false;
            egui::Window::new("GNSS-Einstellungen übernehmen")
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.label(format!(
                        "Es gibt Einstellungen des eigenständigen Programms gnss-vermessung:\n{}\n\n\
                         Sollen Schnittstelle, SAPOS-Zugang, Messvorgaben, Antenne und Höhenbezug einmalig \
                         übernommen werden? Danach sind beide Programme getrennt.",
                        Settings::original_path().display()
                    ));
                    ui.horizontal(|ui| {
                        if ui.button("Übernehmen").clicked() {
                            self.import_original();
                            done = true;
                        }
                        if ui.button("Nicht übernehmen").clicked() {
                            // eigene Datei anlegen, damit nicht erneut gefragt wird
                            self.save_settings();
                            done = true;
                        }
                    });
                });
            if done {
                self.import_offer = false;
            }
        }
    }

    /// Übernimmt Einstellungen und Passwort des eigenständigen Programms.
    fn import_original(&mut self) {
        match Settings::load_original() {
            Ok(s) => {
                self.settings = s;
                if let Some(pw) = settings::load_original_password(self.settings.user.trim()) {
                    self.password = pw;
                }
                self.reload_points();
                self.save_settings();
                self.info("Einstellungen aus gnss-vermessung übernommen");
            }
            Err(e) => self.error(format!("Übernahme fehlgeschlagen: {e:#}")),
        }
    }
}

/// Aktuelle Position für Feldbuch-NT (Antenne bereits auf den Boden reduziert).
#[derive(Debug, Clone)]
pub struct Live {
    pub fix: Fix,
    /// UTM32 Ostwert ohne Zonenkennzahl [m]
    pub east: f64,
    pub north: f64,
    /// Höhe des Bodenpunkts im eingestellten Höhenbezug, falls verfügbar [m]
    pub h: Option<f64>,
    pub lat: f64,
    pub lon: f64,
    pub sigma_e: Option<f64>,
    pub sigma_n: Option<f64>,
    pub sigma_h: Option<f64>,
    /// Standardellipse: große, kleine Halbachse [m], Richtung ab Nord [°]
    pub ellipse: Option<(f64, f64, f64)>,
    pub pdop: f64,
    pub num_sv: u8,
    pub age: Option<f64>,
    /// GPS-Wochensekunde [ms]
    pub itow: u32,
    /// Position ist älter als die Anzeigegrenze
    pub stale: bool,
}

/// Zustand der Verbindung für Statusanzeigen.
#[derive(Debug, Clone, Default)]
pub struct LinkStatus {
    pub connected: bool,
    pub replay: bool,
    pub device: String,
    pub ntrip: String,
    pub mountpoint: String,
    pub fix: Option<Fix>,
    /// Sekunden seit dem letzten Fix-Gewinn
    pub fix_seconds: Option<f64>,
    pub measuring: bool,
}

impl App {
    /// Verbindet mit dem Empfänger (wie „Verbinden“ im GNSS-Fenster).
    pub fn verbinde(&mut self, ctx: &egui::Context) {
        self.connect(ctx);
    }

    /// Trennt die Verbindung zum Empfänger.
    pub fn trenne(&mut self) {
        self.extern_measure = false;
        self.disconnect();
    }

    pub fn ist_verbunden(&self) -> bool {
        self.engine.is_some()
    }

    /// Letzte Meldung des GNSS-Moduls (Text, Fehler).
    pub fn meldung(&self) -> Option<&(String, bool)> {
        self.message.as_ref()
    }

    /// Verbindungs- und Lösungsstatus.
    pub fn status(&self) -> LinkStatus {
        let Some(e) = &self.engine else {
            return LinkStatus { device: self.settings.device.clone(), ..Default::default() };
        };
        let st = e.state();
        LinkStatus {
            connected: true,
            replay: e.replay,
            device: self.settings.device.clone(),
            ntrip: st.ntrip.phase.label().to_string(),
            mountpoint: st.ntrip.mountpoint.clone(),
            fix: (!st.is_stale()).then(|| st.solution.as_ref().map(|s| s.fix)).flatten(),
            fix_seconds: st.fix_since.map(|t| t.elapsed().as_secs_f64()),
            measuring: st.measurement.is_some(),
        }
    }

    /// Aktuelle Lösung, auf den Bodenpunkt reduziert.
    pub fn live(&self) -> Option<Live> {
        let e = self.engine.as_ref()?;
        let st = e.state();
        let s = st.solution.as_ref()?;
        if s.fix == Fix::None {
            return None;
        }
        let u = crate::gnss::kern::geo::geo_to_utm(
            &crate::gnss::kern::geo::Geographic { lat: s.lat, lon: s.lon, h: 0.0 },
            UTM_ZONE,
        );
        let h_ell = s.h - self.settings.pole_height - self.settings.antenna_offset;
        let h = match &self.height {
            HeightModel::None => Some(h_ell),
            m => m.zeta(s.lat, s.lon).map(|z| h_ell - z),
        };
        Some(Live {
            fix: s.fix,
            east: u.east,
            north: u.north,
            h,
            lat: s.lat,
            lon: s.lon,
            sigma_e: s.sigma_e(),
            sigma_n: s.sigma_n(),
            sigma_h: s.sigma_h(),
            ellipse: s.ellipse(),
            pdop: s.pdop,
            num_sv: s.num_sv,
            age: s.age,
            itow: s.itow,
            stale: st.is_stale(),
        })
    }

    /// Stabhöhe (Boden bis Antennenbezugspunkt) [m].
    pub fn pole_height(&self) -> f64 {
        self.settings.pole_height
    }

    /// Setzt die Stabhöhe; gilt auch für den Reiter „Messen“.
    pub fn set_pole_height(&mut self, value: f64) {
        if (self.settings.pole_height - value).abs() > f64::EPSILON {
            self.settings.pole_height = value;
            if self.settings_ok
                && let Err(e) = self.settings.save()
            {
                self.error(format!("{e:#}"));
            }
        }
    }

    /// Offset Antennenbezugspunkt → Phasenzentrum [m].
    pub fn antenna_offset(&self) -> f64 {
        self.settings.antenna_offset
    }

    /// Messvorgaben aus der GNSS-Konfiguration.
    pub fn criteria(&self) -> Criteria {
        self.settings.criteria()
    }

    /// Startet eine Messung für Feldbuch-NT. Das Ergebnis holt [`App::take_extern_result`] ab.
    pub fn start_extern(&mut self, nr: &str, code: &str, criteria: Criteria) -> Result<(), String> {
        self.check_height_ready()?;
        let Some(engine) = &self.engine else {
            return Err("GNSS-Empfänger ist nicht verbunden.".into());
        };
        let mut st = engine.state();
        if st.measurement.is_some() {
            return Err("Es läuft bereits eine GNSS-Messung.".into());
        }
        let request = PointRequest {
            nr: nr.to_string(),
            code: code.to_string(),
            remark: String::new(),
            pole_height: self.settings.pole_height,
            antenna_offset: self.settings.antenna_offset,
        };
        st.measurement = Some(Measurement::new(request, criteria));
        drop(st);
        self.extern_measure = true;
        self.extern_result = None;
        Ok(())
    }

    /// Bricht eine Messung von Feldbuch-NT ab.
    pub fn cancel_extern(&mut self) {
        if self.extern_measure
            && let Some(e) = &self.engine
        {
            e.state().measurement = None;
        }
        self.extern_measure = false;
    }

    /// Fortschritt der Messung von Feldbuch-NT.
    pub fn extern_progress(&self) -> Option<ExternProgress> {
        if !self.extern_measure {
            return None;
        }
        let e = self.engine.as_ref()?;
        let st = e.state();
        let m = st.measurement.as_ref()?;
        Some(ExternProgress {
            accepted: m.accepted.len(),
            epochs: m.criteria.epochs,
            seconds: m.seconds(),
            min_seconds: m.criteria.min_seconds,
            last_rejection: m.last_rejection,
        })
    }

    /// Holt das Ergebnis einer abgeschlossenen Messung von Feldbuch-NT ab.
    pub fn take_extern_result(&mut self) -> Option<ExternResult> {
        self.extern_result.take()
    }

    /// Übernimmt eine abgeschlossene Messung von Feldbuch-NT.
    fn collect_extern(&mut self) {
        let Some(engine) = &self.engine else { return };
        let (measurement, mountpoint, firmware, gdop) = {
            let mut st = engine.state();
            if !st.measurement.as_ref().is_some_and(|m| m.done) {
                return;
            }
            let firmware = st.version.as_ref().map(|v| v.2.join(" ")).unwrap_or_default();
            let gdop = st.receiver_dop.map(|d| d.g);
            let mountpoint = st.ntrip.mountpoint.clone();
            match st.measurement.take() {
                Some(m) => (m, mountpoint, firmware, gdop),
                None => return,
            }
        };
        let replay = engine.replay;
        self.extern_measure = false;
        let Some(result) = measurement.result(&self.height) else {
            return;
        };
        if (self.settings.sapos_transform || self.settings.height_mode != HeightMode::Ellipsoidal)
            && result.h_dhhn.is_none()
        {
            self.error("Punkt liegt außerhalb des Quasigeoid-Gitters. Ergebnis verworfen.");
            return;
        }
        let protocol = self.protocol(&measurement, mountpoint, firmware, gdop);
        self.extern_result = Some(ExternResult { measurement, result, protocol, replay });
        if self.settings.sounds {
            tone(TONE_DONE);
        }
    }
}

impl App {
    /// Hintergrundarbeit in jedem Bild, auch wenn das GNSS-Fenster geschlossen ist.
    pub fn aktualisiere(&mut self, ctx: &egui::Context) {
        self.poll(ctx);
        if self.engine.is_some() {
            ctx.request_repaint_after(IDLE_REPAINT);
        }
    }

    /// Zeichnet die GNSS-Oberfläche in das übergebene Fenster.
    pub fn zeichne(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let mut actions = Vec::new();
        self.keys(&ctx, &mut actions);

        egui::Panel::top("menu").show(ui, |ui| {
            self.menu(ui, &mut actions);
            ui.horizontal_wrapped(|ui| {
                if self.engine.is_some() {
                    if ui.button("⏹ Trennen").clicked() {
                        actions.push(Action::Disconnect);
                    }
                } else if ui.button("▶ Verbinden").clicked() {
                    actions.push(Action::Connect);
                }
                ui.separator();
                for (tab, label) in TABS {
                    ui.selectable_value(&mut self.tab, tab, label);
                }
            });
        });

        let recording = self
            .engine
            .as_ref()
            .and_then(|e| e.recording())
            .map(|(p, n)| (p.display().to_string(), n));
        let ntrip_running = self.engine.as_ref().is_some_and(Engine::ntrip_running);
        let connected = self.engine.is_some();
        let forwarding = self.engine.as_ref().filter(|e| !e.replay && e.ntrip_running()).map(Engine::forwarding);
        {
            let guard = self.engine.as_ref().map(Engine::state);
            let empty;
            let st: &State = match &guard {
                Some(g) => g,
                None => {
                    empty = State::default();
                    &empty
                }
            };
            egui::Panel::bottom("statusbar").show(ui, |ui| self.status_bar(ui, st));
            if !matches!(self.tab, Tab::Field | Tab::Map | Tab::Track) {
                egui::Panel::right("side")
                    .resizable(false)
                    .exact_size(SIDE_WIDTH)
                    .show(ui, |ui| {
                        view_status::side(ui, st, &self.height);
                    });
            }
            egui::CentralPanel::default().show(ui, |ui| match self.tab {
                Tab::Status => view_status::status(ui, st, &self.height),
                Tab::Field => view_status::field(ui, st, &self.height),
                Tab::Measure => view_measure::show(
                    ui,
                    st,
                    &mut self.measure,
                    &mut self.settings,
                    connected,
                    &mut actions,
                ),
                Tab::Sky => view_sky::show(ui, st, &mut self.sky, &mut self.settings),
                Tab::Dop => view_sky::show_dop(ui, st, &mut self.dop_anim, &mut self.settings),
                Tab::Time => view_time::show(ui, st),
                Tab::Track => view_track::show(ui, &mut self.track, forwarding, &mut actions),
                Tab::Map => view_map::show(
                    ui,
                    st,
                    &self.measure.points,
                    &mut self.map,
                    &mut self.settings,
                ),
                Tab::Ntrip => {
                    view_ntrip::show(ui, st, &mut self.ntrip_view, &mut self.settings, ntrip_running, &mut actions)
                }
                Tab::Raw => view_raw::show(ui, st, &mut self.raw, recording, &mut actions),
                Tab::Config => view_config::show(
                    ui,
                    st,
                    &mut self.config,
                    &mut self.settings,
                    &mut self.password,
                    connected,
                    &mut actions,
                ),
                Tab::Tools => view_config::tools(ui, &mut self.tools),
                Tab::Help => view_config::help(ui, st),
            });
        }
        self.dialogs(&ctx);
        for a in actions {
            self.run_action(&ctx, a);
        }
    }

    /// Beim Programmende: zuletzt verwendete Werte behalten (ohne Passwort), Empfänger trennen.
    pub fn beenden(&mut self) {
        if self.settings_ok
            && let Err(e) = self.settings.save()
        {
            crate::grundlagen::fehler_log::log("GNSS-Einstellungen", &format!("{e:#}"));
        }
        self.engine = None;
    }
}
