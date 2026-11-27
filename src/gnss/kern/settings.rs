//! Zweck:    Benutzereinstellungen des GNSS-Moduls (TOML in den Feldbuch-Benutzerdaten) und SAPOS-Passwort in der
//!           Windows-Anmeldeinformationsverwaltung; einmalige Übernahme aus dem eigenständigen gnss-vermessung.
//! Hardware: Windows-PC
//! Autor:    Johann Gerner
//!
//! Übernommen aus gnss-vermessung 0.4.0 (src/settings.rs), eigenständig gepflegt in Feldbuch-NT.

use crate::gnss::kern::config::*;
use crate::gnss::kern::measure::Criteria;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const KEYRING_SERVICE: &str = "feldbuch-nt-sapos";
/// Tresor-Eintrag des eigenständigen Programms (nur für die Übernahme gelesen)
const KEYRING_SERVICE_ORIGINAL: &str = "gnss-vermessung-sapos";
/// Dateiname der Einstellungen in den Feldbuch-Benutzerdaten
const SETTINGS_FILE: &str = "gnss.toml";
const DEFAULT_RATE_MS: u16 = 1000;
const DEFAULT_ELEVATION_MASK: u8 = 10;
const DEFAULT_POLE_M: f64 = 2.0;
const DEFAULT_ZOOM: f64 = 17.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HeightMode {
    Ellipsoidal,
    Grid,
    Constant,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    // Schnittstelle
    pub device: String,
    pub baud: u32,
    pub auto_connect: bool,
    pub auto_reconnect: bool,
    pub configure_on_connect: bool,
    // Empfänger
    pub rate_ms: u16,
    pub dyn_model: u8,
    pub elevation_mask: u8,
    // SAPOS
    pub caster: String,
    pub caster_port: u16,
    pub mountpoint: String,
    pub user: String,
    pub rev1: bool,
    pub gga_interval_s: u64,
    pub ntrip_autostart: bool,
    // Messvorgaben
    pub epochs: usize,
    pub min_seconds: f64,
    pub max_sigma_2d: f64,
    pub max_sigma_h: f64,
    pub max_age: f64,
    pub max_pdop: f64,
    pub min_sats: u8,
    pub restart_on_fix_loss: bool,
    // Antenne und Höhenbezug
    pub pole_height: f64,
    pub antenna_offset: f64,
    pub height_mode: HeightMode,
    pub geoid_file: String,
    pub zeta: f64,
    /// Höhe DHHN2016 aus den SAPOS-Transformationsnachrichten (hat Vorrang vor `height_mode`)
    pub sapos_transform: bool,
    // Ausgabe
    pub output: String,
    pub project: String,
    pub operator: String,
    pub zone_prefix: bool,
    // Darstellung
    pub dark: bool,
    pub scale: f32,
    pub sounds: bool,
    pub display_mask: f64,
    pub clock_per_system: bool,
    pub map_layer: usize,
    pub map_zoom: f64,
}

impl Default for Settings {
    fn default() -> Self {
        let c = Criteria::default();
        let output = std::env::var_os("USERPROFILE")
            .map(PathBuf::from)
            .unwrap_or_default()
            .join("Documents")
            .join("GNSS")
            .join("messung-kor.csv");
        Settings {
            device: String::new(),
            baud: DEFAULT_BAUD,
            auto_connect: false,
            auto_reconnect: true,
            configure_on_connect: true,
            rate_ms: DEFAULT_RATE_MS,
            dyn_model: 0,
            elevation_mask: DEFAULT_ELEVATION_MASK,
            caster: DEFAULT_CASTER.into(),
            caster_port: DEFAULT_CASTER_PORT,
            mountpoint: DEFAULT_MOUNTPOINT.into(),
            user: String::new(),
            rev1: false,
            gga_interval_s: DEFAULT_GGA_INTERVAL_S,
            ntrip_autostart: true,
            epochs: c.epochs,
            min_seconds: c.min_seconds,
            max_sigma_2d: c.max_sigma_2d,
            max_sigma_h: c.max_sigma_h,
            max_age: c.max_age,
            max_pdop: c.max_pdop,
            min_sats: c.min_sats,
            restart_on_fix_loss: c.restart_on_fix_loss,
            pole_height: DEFAULT_POLE_M,
            antenna_offset: 0.0,
            height_mode: HeightMode::Ellipsoidal,
            geoid_file: String::new(),
            zeta: 0.0,
            sapos_transform: false,
            output: output.display().to_string(),
            project: String::new(),
            operator: String::new(),
            zone_prefix: false,
            dark: false,
            scale: 1.0,
            sounds: true,
            display_mask: f64::from(DEFAULT_ELEVATION_MASK),
            clock_per_system: false,
            map_layer: 0,
            map_zoom: DEFAULT_ZOOM,
        }
    }
}

impl Settings {
    pub fn criteria(&self) -> Criteria {
        Criteria {
            epochs: self.epochs,
            min_seconds: self.min_seconds,
            max_sigma_2d: self.max_sigma_2d,
            max_sigma_h: self.max_sigma_h,
            max_age: self.max_age,
            max_pdop: self.max_pdop,
            min_sats: self.min_sats,
            restart_on_fix_loss: self.restart_on_fix_loss,
        }
    }

    /// Übernimmt Messvorgaben (z. B. die Standardwerte aus dem Einrichtungsassistenten).
    pub fn set_criteria(&mut self, c: &Criteria) {
        self.epochs = c.epochs;
        self.min_seconds = c.min_seconds;
        self.max_sigma_2d = c.max_sigma_2d;
        self.max_sigma_h = c.max_sigma_h;
        self.max_age = c.max_age;
        self.max_pdop = c.max_pdop;
        self.min_sats = c.min_sats;
        self.restart_on_fix_loss = c.restart_on_fix_loss;
    }

    pub fn path() -> PathBuf {
        crate::grundlagen::pfade::benutzer_get(SETTINGS_FILE)
    }

    /// Einstellungsdatei des eigenständigen Programms gnss-vermessung.
    pub fn original_path() -> PathBuf {
        std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_default()
            .join(APP_NAME)
            .join("settings.toml")
    }

    /// Eine Übernahme ist möglich: eigene Datei fehlt, die des eigenständigen Programms ist vorhanden.
    pub fn import_possible() -> bool {
        !Self::path().exists() && Self::original_path().is_file()
    }

    /// Liest die Einstellungen des eigenständigen Programms (ohne etwas zu speichern).
    pub fn load_original() -> Result<Settings> {
        let p = Self::original_path();
        let t = std::fs::read_to_string(&p).with_context(|| format!("{} nicht lesbar", p.display()))?;
        toml::from_str(&t).with_context(|| format!("{} ist fehlerhaft", p.display()))
    }

    /// Lädt die Einstellungen; eine fehlende Datei ergibt die Vorgaben.
    pub fn load() -> Result<Settings> {
        let p = Self::path();
        match std::fs::read_to_string(&p) {
            Ok(t) => toml::from_str(&t).with_context(|| format!("{} ist fehlerhaft", p.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Settings::default()),
            Err(e) => Err(e).with_context(|| format!("{} nicht lesbar", p.display())),
        }
    }

    pub fn save(&self) -> Result<()> {
        let p = Self::path();
        if let Some(d) = p.parent() {
            std::fs::create_dir_all(d)?;
        }
        std::fs::write(&p, toml::to_string_pretty(self)?)
            .with_context(|| format!("{} nicht schreibbar", p.display()))
    }
}

pub fn load_password(user: &str) -> Option<String> {
    if user.is_empty() {
        return None;
    }
    keyring::Entry::new(KEYRING_SERVICE, user)
        .ok()?
        .get_password()
        .ok()
}

/// SAPOS-Passwort aus dem Tresor-Eintrag des eigenständigen Programms.
pub fn load_original_password(user: &str) -> Option<String> {
    if user.is_empty() {
        return None;
    }
    keyring::Entry::new(KEYRING_SERVICE_ORIGINAL, user)
        .ok()?
        .get_password()
        .ok()
}

pub fn save_password(user: &str, password: &str) -> Result<()> {
    if user.is_empty() || password.is_empty() {
        return Ok(());
    }
    keyring::Entry::new(KEYRING_SERVICE, user)?
        .set_password(password)
        .context("Passwort konnte nicht in Windows gespeichert werden")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_missing_fields() {
        let s = Settings {
            user: "abc".into(),
            ..Default::default()
        };
        let t = toml::to_string_pretty(&s).unwrap();
        assert!(!t.to_lowercase().contains("passw"));
        assert_eq!(toml::from_str::<Settings>(&t).unwrap().user, "abc");
        assert_eq!(toml::from_str::<Settings>("epochs = 5").unwrap().epochs, 5);
    }
}
