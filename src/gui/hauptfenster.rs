// Zweck:    Hauptfenster: Projektanzeige, Optionen, Modulauswahl und Umschaltung der Ansichten
// Hardware: keine
// Autor:    Johann Gerner

use eframe::egui;

use super::{Ansicht, FeldbuchApp, projekt_dialog, projektdaten_ansicht, rechenparameter_ansicht};
use crate::grundlagen::protokoll;

/// Programmversion aus Cargo.toml.
const VERSION: &str = env!("CARGO_PKG_VERSION");
const BUILDNUMMER: &str = env!("FELDBUCH_BUILD_NUMMER");
/// Zusatz für Testversionen (z. B. „Test1"), gesetzt über die Umgebungsvariable FELDBUCH_ZUSATZ beim Bauen.
const VERSIONS_ZUSATZ: Option<&str> = option_env!("FELDBUCH_ZUSATZ");

/// Versionstext für Anzeige: „Version 0.6.8 Test1 · Build 46".
fn versions_text() -> String {
    match VERSIONS_ZUSATZ.filter(|z| !z.is_empty()) {
        Some(zusatz) => format!("Version {VERSION} {zusatz} · Build {BUILDNUMMER}"),
        None => format!("Version {VERSION} · Build {BUILDNUMMER}"),
    }
}
/// Höhe der Modulknöpfe und des zentralen DXF-Knopfs.
const KNOPF_HOEHE: f32 = 44.0;
const DXF_KNOPF_HOEHE: f32 = 64.0;
/// Größe der Knöpfe für Projekt und Projektdaten.
const KLEIN_KNOPF_BREITE: f32 = 130.0;
const KLEIN_KNOPF_HOEHE: f32 = 28.0;

/// Zeichnet das Hauptfenster bzw. die aktive Ansicht.
pub fn zeichne(app: &mut FeldbuchApp, ui: &mut egui::Ui) {
    if !matches!(app.ansicht, Ansicht::Start) {
        egui::Panel::top("navigation").show(ui, |ui| {
            if ui.button("← Zurück zum Hauptmenü").clicked() {
                app.setze_ansicht(Ansicht::Start);
            }
        });
    }

    egui::CentralPanel::default().show(ui, |ui| {
        let mut zurueck = false;
        match &mut app.ansicht {
            Ansicht::Start => zeichne_start(app, ui),
            Ansicht::Projekt(dialog) => {
                zurueck = dialog.zeichne(ui, &mut app.projekt);
            }
            Ansicht::Projektdaten(ansicht) => ansicht.zeichne(ui, &mut app.projekt),
            Ansicht::Rechenparameter(ansicht) => ansicht.zeichne(ui, &mut app.projekt),
            Ansicht::Kommunikation(ansicht) => {
                zurueck = ansicht.zeichne(ui, &mut app.projekt, &mut app.geraet);
            }
            Ansicht::Berechnungen(ansicht) => ansicht.zeichne(ui, &mut app.projekt),
            Ansicht::Absteckung(ansicht) => ansicht.zeichne(ui, &app.projekt, &mut app.geraet),
            Ansicht::Datenmanager(ansicht) => ansicht.zeichne(ui, &app.projekt),
            Ansicht::Testmessungen(ansicht) => {
                egui::ScrollArea::both().show(ui, |ui| ansicht.zeichne(ui, &mut app.projekt, &mut app.geraet));
            }
            // Ohne äußere Scrollfläche: die Tabelle scrollt selbst und füllt die Fensterhöhe
            Ansicht::Konvertierung(ansicht) => ansicht.zeichne(ui, &app.projekt),
            Ansicht::Aufnahmetest(ansicht) => {
                egui::ScrollArea::both().show(ui, |ui| ansicht.zeichne(ui, &mut app.geraet));
            }
            Ansicht::Info => {
                if zeichne_info(ui, app.update_suche.is_some()) {
                    app.update_pruefen_wunsch = true;
                }
            }
            Ansicht::NochNichtPortiert(name) => {
                ui.heading(*name);
                ui.label("Dieses Modul ist in Feldbuch-NT noch nicht umgesetzt.");
            }
        }
        if zurueck {
            app.setze_ansicht(Ansicht::Start);
        }
    });
}

fn zeichne_start(app: &mut FeldbuchApp, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.heading("Feldbuch");
        ui.label(versions_text());
    });
    ui.separator();

    let e = &app.projekt.einstellungen;
    if e.ist_geladen() {
        ui.label(format!("Projekt:  {}   •   {}", e.projekt_name, e.projekt_verzeichnis.display()));
    } else {
        ui.label("Kein Projekt gewählt  –  bitte Projekt auswählen");
    }
    ui.add_space(8.0);

    // Optionen
    let mut geaendert = false;
    let e = &mut app.projekt.einstellungen;
    let mut protokoll_geaendert = false;
    ui.horizontal_wrapped(|ui| {
        protokoll_geaendert |= ui.checkbox(&mut e.protokoll_aktiv, "Protokollieren").changed();
        geaendert |= protokoll_geaendert;
        geaendert |= ui.checkbox(&mut e.auto_backup, "Auto-Backup").changed();
        geaendert |= ui.checkbox(&mut e.koordinaten_tooltip, "Koordinaten-Tooltip").changed();
        geaendert |= ui.checkbox(&mut e.ton_bei_berechnung, "Ton bei Berechnung").changed();
        geaendert |= ui.checkbox(&mut e.erweiterte_protokollierung, "Erweitertes Protokoll").changed();
    });
    if geaendert {
        match app.projekt.einstellungen.speichern() {
            Ok(()) => {
                let aktiv = app.projekt.einstellungen.protokoll_aktiv;
                protokoll::konfiguriere(
                    aktiv,
                    app.projekt
                        .einstellungen
                        .ist_geladen()
                        .then(|| app.projekt.einstellungen.projekt_verzeichnis.clone()),
                );
                if protokoll_geaendert && aktiv {
                    protokoll::log("EINST", "Protokollierung aktiviert");
                }
            }
            Err(fehler) => app.meldungen.push(format!("Einstellungen konnten nicht gespeichert werden:\n{fehler}")),
        }
    }
    ui.add_space(12.0);

    // Projekt und Projektdaten: neben der Projektanzeige
    ui.horizontal(|ui| {
        for name in ["Projekt", "Projektdaten"] {
            if ui.add(egui::Button::new(name).min_size(egui::Vec2::new(KLEIN_KNOPF_BREITE, KLEIN_KNOPF_HOEHE))).clicked() {
                oeffne_modul(app, name);
            }
        }
    });
    ui.add_space(8.0);

    // Module: farblich nach Funktion getrennt, der DXF-Viewer als zentrale Funktion über die ganze Breite
    let abstand = ui.spacing().item_spacing.x;
    let halbe_breite = (ui.available_width() - abstand) / 2.0;
    let mut gewaehlt: Option<&str> = None;
    let mut knopf = |ui: &mut egui::Ui, modul: &Modul, breite: f32, hoehe: f32| {
        let text = egui::RichText::new(modul.name).color(egui::Color32::WHITE).size(modul.schriftgroesse);
        let button = match modul_icon(modul.name) {
            Some(icon) => egui::Button::image_and_text(egui::Image::new(icon).fit_to_exact_size(egui::Vec2::splat(hoehe - ICON_RAND)), text),
            None => egui::Button::new(text),
        }
        .fill(modul.farbe)
        .min_size(egui::Vec2::new(breite, hoehe));
        if ui.add(button).clicked() {
            gewaehlt = Some(modul.name);
        }
    };
    knopf(ui, &DXF_VIEWER, ui.available_width(), DXF_KNOPF_HOEHE);
    ui.add_space(abstand);
    for zeile in &MODUL_ZEILEN {
        ui.horizontal(|ui| {
            for modul in zeile {
                knopf(ui, modul, halbe_breite, KNOPF_HOEHE);
            }
        });
    }
    if let Some(name) = gewaehlt {
        oeffne_modul(app, name);
    }
}

/// Ein Modulknopf: Name, Farbe der Funktionsgruppe und Schriftgröße.
struct Modul {
    name: &'static str,
    farbe: egui::Color32,
    schriftgroesse: f32,
}

const SCHRIFT_NORMAL: f32 = 15.0;
const SCHRIFT_ZENTRAL: f32 = 20.0;
/// Farben der Funktionsgruppen.
const FARBE_ZENTRAL: egui::Color32 = egui::Color32::from_rgb(30, 70, 140);
const FARBE_STATIONIERUNG: egui::Color32 = egui::Color32::from_rgb(40, 120, 80);
const FARBE_RECHNEN: egui::Color32 = egui::Color32::from_rgb(190, 110, 20);
const FARBE_DATEN: egui::Color32 = egui::Color32::from_rgb(120, 70, 150);
const FARBE_GERAET: egui::Color32 = egui::Color32::from_rgb(20, 120, 140);
const FARBE_SONSTIGES: egui::Color32 = egui::Color32::from_rgb(100, 105, 115);

const DXF_VIEWER: Modul = Modul { name: "DXF-Viewer", farbe: FARBE_ZENTRAL, schriftgroesse: SCHRIFT_ZENTRAL };

/// Anordnung der übrigen Module in zwei Spalten.
const MODUL_ZEILEN: [[Modul; 2]; 5] = [
    [
        Modul { name: "Freie Stationierung", farbe: FARBE_STATIONIERUNG, schriftgroesse: SCHRIFT_NORMAL },
        Modul { name: "Polarpunkte", farbe: FARBE_STATIONIERUNG, schriftgroesse: SCHRIFT_NORMAL },
    ],
    [
        Modul { name: "Berechnungen", farbe: FARBE_RECHNEN, schriftgroesse: SCHRIFT_NORMAL },
        Modul { name: "Absteckung", farbe: FARBE_RECHNEN, schriftgroesse: SCHRIFT_NORMAL },
    ],
    [
        Modul { name: "Datenmanager", farbe: FARBE_DATEN, schriftgroesse: SCHRIFT_NORMAL },
        Modul { name: "Konvertierung", farbe: FARBE_DATEN, schriftgroesse: SCHRIFT_NORMAL },
    ],
    [
        Modul { name: "Tachymeter-Kommunikation", farbe: FARBE_GERAET, schriftgroesse: SCHRIFT_NORMAL },
        Modul { name: "Rechenparameter", farbe: FARBE_GERAET, schriftgroesse: SCHRIFT_NORMAL },
    ],
    [
        Modul { name: "Testmessungen", farbe: FARBE_SONSTIGES, schriftgroesse: SCHRIFT_NORMAL },
        Modul { name: "Info", farbe: FARBE_SONSTIGES, schriftgroesse: SCHRIFT_NORMAL },
    ],
];

/// Abstand des Modul-Icons zum Knopfrand (Summe oben und unten) in Punkten.
const ICON_RAND: f32 = 10.0;

/// Icon eines Moduls, falls vorhanden.
fn modul_icon(name: &str) -> Option<egui::ImageSource<'static>> {
    match name {
        "Freie Stationierung" => Some(egui::include_image!("../../icons/freie_stationierung.png")),
        "Polarpunkte" => Some(egui::include_image!("../../icons/polarpunkte.png")),
        _ => None,
    }
}

/// Öffnet das Modul mit diesem Namen.
fn oeffne_modul(app: &mut FeldbuchApp, name: &str) {
    protokoll::log("FORM", &format!("Modul geöffnet: {name}"));
    let neu = match name {
        "DXF-Viewer" => {
            app.dxf_viewer.get_or_insert_with(Box::default);
            return;
        }
        "Projekt" => Ansicht::Projekt(projekt_dialog::ProjektDialog::neu()),
        "Projektdaten" => Ansicht::Projektdaten(projektdaten_ansicht::ProjektdatenAnsicht::default()),
        "Freie Stationierung" => {
            app.oeffne_freie_stationierung();
            return;
        }
        "Polarpunkte" => {
            app.oeffne_polarpunkte();
            return;
        }
        "Berechnungen" => Ansicht::Berechnungen(Default::default()),
        "Absteckung" => Ansicht::Absteckung(Default::default()),
        "Datenmanager" => Ansicht::Datenmanager(Default::default()),
        "Konvertierung" => app.konvertierung_oeffnen(),
        "Tachymeter-Kommunikation" => Ansicht::Kommunikation(Default::default()),
        "Rechenparameter" => Ansicht::Rechenparameter(rechenparameter_ansicht::RechenparameterAnsicht::default()),
        "Testmessungen" => Ansicht::Testmessungen(Box::new(super::testmessungen::TestmessungenAnsicht::neu(&app.projekt))),
        _ => Ansicht::Info,
    };
    app.setze_ansicht(neu);
}

/// Hilfehinweise der Info-Seite (wie im Original).
const HILFE: [&str; 4] = [
    "• Bluetooth-Gerät zuerst über die Windows-Einstellungen koppeln, dann unter Tachymeter-Kommunikation den COM-Port wählen.",
    "• DXF-Datei über den DXF-Viewer öffnen.",
    "• Freie Stationierung benötigt mindestens 2 bekannte Punkte im DXF.",
    "• Das Protokollieren (Hauptmenü) schreibt RTF-Protokolle der Berechnungen ins Projektverzeichnis.",
];

/// Zeichnet die Info-Seite; liefert `true`, wenn „Nach Updates suchen“ geklickt wurde.
fn zeichne_info(ui: &mut egui::Ui, sucht: bool) -> bool {
    ui.horizontal(|ui| {
        ui.add(egui::Image::new(egui::include_image!("../../icons/app_icon.png")).fit_to_exact_size(egui::Vec2::splat(52.0)));
        ui.vertical(|ui| {
            ui.label(egui::RichText::new("Feldbuch-NT").size(26.0).strong().color(egui::Color32::from_rgb(30, 60, 110)));
            ui.label(egui::RichText::new(versions_text()).size(15.0).color(egui::Color32::from_gray(90)));
        });
    });
    ui.separator();
    ui.label("Geodätisches Feldbuch mit Freier Stationierung, Tachymeteranbindung und DXF-Viewer.");
    ui.label("Rust-Neuimplementierung (egui) des C#-Programms Feldbuch.");
    ui.add_space(8.0);
    ui.label("Autor:   Johann Gerner");
    ui.label(egui::RichText::new("© 2026 Johann Gerner").color(egui::Color32::from_gray(110)));
    ui.separator();
    ui.label(egui::RichText::new("Hilfe").strong());
    for zeile in HILFE {
        ui.label(zeile);
    }
    ui.separator();
    let text = if sucht { "Suche läuft …" } else { "Nach Updates suchen" };
    ui.add_enabled(!sucht, egui::Button::new(text)).clicked()
}
