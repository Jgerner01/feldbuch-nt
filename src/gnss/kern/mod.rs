//! Zweck:    Rechen- und Empfängerteil des GNSS-Moduls: Empfang, NTRIP/SAPOS, Messung, Koordinaten, Höhen.
//! Hardware: simpleRTK2B (u-blox ZED-F9P) an USB
//! Autor:    Johann Gerner
//!
//! Übernommen aus gnss-vermessung 0.4.0 (src/lib.rs), eigenständig gepflegt in Feldbuch-NT.

// Teile der übernommenen Bibliothek nutzt nur das Kommandozeilenprogramm des Originals (z. B. CSV-Export der
// Punktaufnahme); sie bleiben erhalten, damit der Code mit dem Original vergleichbar bleibt.
#![allow(dead_code)]

pub mod config;
pub mod demux;
pub mod dop;
pub mod engine;
pub mod export;
pub mod geo;
pub mod gtime;
pub mod height;
pub mod measure;
pub mod model;
pub mod nmea;
pub mod ntrip;
pub mod rtcm;
pub mod rtcm_decode;
pub mod settings;
pub mod spp;
pub mod survey;
pub mod transform;
pub mod ubx;
pub mod ubxmsg;
pub mod version;
