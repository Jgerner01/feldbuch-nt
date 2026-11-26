//! Zweck:    Oberfläche des GNSS-Moduls (Reiter Status, Messen, Satelliten, DOP, Zeit, Karte, Spur,
//!           Korrekturdaten, Rohdaten, Konfiguration, Werkzeuge, Hilfe) als Fenster in Feldbuch-NT.
//! Hardware: simpleRTK2B (u-blox ZED-F9P)
//! Autor:    Johann Gerner
//!
//! Übernommen aus gnss-vermessung 0.4.0 (src/bin/gnss-gui), eigenständig gepflegt in Feldbuch-NT.

pub mod app;
mod map_stations;
mod tiles;
mod view_config;
mod view_dop_anim;
mod view_ephem;
mod view_map;
mod view_measure;
mod view_ntrip;
mod view_raw;
mod view_sky;
mod view_status;
mod view_time;
mod view_track;
mod widgets;
