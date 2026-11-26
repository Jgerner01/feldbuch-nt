//! Zweck:    Benannte Konstanten und Vorgabewerte.
//! Hardware: Windows-PC, simpleRTK2B (u-blox ZED-F9P)
//! Autor:    Johann Gerner
//!
//! Übernommen aus gnss-vermessung 0.4.0 (src/config.rs), eigenständig gepflegt in Feldbuch-NT.

use std::time::Duration;

pub const APP_NAME: &str = "gnss-vermessung";
pub const UTM_ZONE: u8 = 32;

// Schnittstelle
pub const DEFAULT_BAUD: u32 = 115_200;
pub const SERIAL_TIMEOUT: Duration = Duration::from_millis(100);
pub const RECONNECT_INTERVAL: Duration = Duration::from_secs(3);
pub const ACK_TIMEOUT: Duration = Duration::from_millis(1500);

// SAPOS
pub const DEFAULT_CASTER: &str = "sapos-rtk.bayern.de";
pub const DEFAULT_CASTER_PORT: u16 = 2101;
pub const DEFAULT_MOUNTPOINT: &str = "VRS_3_4G_BY";
pub const DEFAULT_GGA_INTERVAL_S: u64 = 10;
pub const NTRIP_BACKOFF_MIN: Duration = Duration::from_secs(2);
pub const NTRIP_BACKOFF_MAX: Duration = Duration::from_secs(60);

// Messung
pub const DEFAULT_EPOCHS: usize = 30;
pub const DEFAULT_MAX_SIGMA_2D_M: f64 = 0.02;
pub const DEFAULT_MAX_SIGMA_H_M: f64 = 0.04;
pub const DEFAULT_MAX_AGE_S: f64 = 5.0;
pub const DEFAULT_MAX_PDOP: f64 = 4.0;
pub const DEFAULT_MIN_SATS: u8 = 8;
/// So lange gilt das Korrekturalter aus der letzten GGA-Meldung.
pub const GGA_VALID: Duration = Duration::from_secs(3);
/// Ab diesem Alter gelten Anzeigewerte als veraltet.
pub const STALE_AFTER: Duration = Duration::from_secs(3);

// Speichergrenzen
/// Zeilen des SAPOS-Protokolls
pub const COMM_CAPACITY: usize = 2000;
pub const RAW_CAPACITY: usize = 3000;
pub const HISTORY_CAPACITY: usize = 600;
pub const TRACK_CAPACITY: usize = 5000;
pub const TRAIL_CAPACITY: usize = 60;
pub const TRAIL_INTERVAL_MS: u32 = 60_000;
pub const EVENT_CAPACITY: usize = 200;

// Karte
pub const TILE_USER_AGENT: &str = concat!("gnss-vermessung/", env!("CARGO_PKG_VERSION"));
pub const TILE_WORKERS: usize = 2;
pub const TILE_TEXTURE_LIMIT: usize = 400;
pub const TILE_RETRY: Duration = Duration::from_secs(30);
pub const TILE_TIMEOUT: Duration = Duration::from_secs(15);
