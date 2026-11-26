//! Zweck:    Autor und Versionsangabe des GNSS-Moduls in Feldbuch-NT.
//! Hardware: keine
//! Autor:    Johann Gerner
//!
//! Übernommen aus gnss-vermessung 0.4.0, eigenständig gepflegt in Feldbuch-NT.

pub const AUTHOR: &str = "Johann Gerner";
/// Herkunft des GNSS-Moduls
pub const ORIGIN: &str = "abgeleitet von gnss-vermessung 0.4.0";
/// z. B. „Feldbuch-NT 0.11.0 (Build 55), GNSS-Modul abgeleitet von gnss-vermessung 0.4.0“
pub const FULL: &str = concat!(
    "Feldbuch-NT ",
    env!("CARGO_PKG_VERSION"),
    " (Build ",
    env!("FELDBUCH_BUILD_NUMMER"),
    "), GNSS-Modul abgeleitet von gnss-vermessung 0.4.0"
);
