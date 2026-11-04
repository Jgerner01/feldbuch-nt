// Zweck:    Hauptfenster: Projektanzeige, Optionen, Modulauswahl und Umschaltung der Ansichten
// Hardware: keine
// Autor:    Johann Gerner

use eframe::egui;

use super::{Ansicht, FeldbuchApp, projekt_dialog, projektdaten_ansicht, rechenparameter_ansicht};
use crate::grundlagen::protokoll;

/// Programmversion aus Cargo.toml.
const VERSION: &str = env!("CARGO_PKG_VERSION");
/// Breite der Modulknöpfe.
const KNOPF_BREITE: f32 = 260.0;
/// Höhe der Modulknöpfe.
const KNOPF_HOEHE: f32 = 36.0;

/// Zeichnet das Hauptfenster bzw. die aktive Ansicht.
pub fn zeichne(app: &mut FeldbuchApp, ui: &mut egui::Ui) {
    if !matches!(app.ansicht, Ansicht::Start) {
        egui::Panel::top("navigation").show(ui, |ui| {
            if ui.button("← Zurück zum Hauptmenü").clicked() {
                app.ansicht = Ansicht::Start;
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
            Ansicht::FreieStationierung(ansicht) => ansicht.zeichne(ui, &mut app.projekt),
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
            Ansicht::Konvertierung(ansicht) => {
                egui::ScrollArea::both().show(ui, |ui| ansicht.zeichne(ui, &app.projekt));
            }
            Ansicht::Polarpunkte(ansicht) => {
                egui::ScrollArea::both().show(ui, |ui| ansicht.zeichne(ui, &mut app.projekt));
            }
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
            app.ansicht = Ansicht::Start;
        }
    });
}

fn zeichne_start(app: &mut FeldbuchApp, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.heading("Feldbuch");
        ui.label(format!("Version {VERSION}"));
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

    // Module
    let module: [(&str, fn() -> Ansicht); 13] = [
        ("Projekt", || Ansicht::Projekt(projekt_dialog::ProjektDialog::neu())),
        ("Freie Stationierung", || Ansicht::FreieStationierung(Box::default())),
        ("Polarpunkte", || Ansicht::NochNichtPortiert("Polarpunkte")),
        ("Absteckung", || Ansicht::Absteckung(Default::default())),
        ("DXF-Viewer", || Ansicht::Start),
        ("Projektdaten", || Ansicht::Projektdaten(projektdaten_ansicht::ProjektdatenAnsicht::default())),
        ("Rechenparameter", || Ansicht::Rechenparameter(rechenparameter_ansicht::RechenparameterAnsicht::default())),
        ("Konvertierung", || Ansicht::NochNichtPortiert("Konvertierung")),
        ("Datenmanager", || Ansicht::Datenmanager(Default::default())),
        ("Berechnungen", || Ansicht::Berechnungen(Default::default())),
        ("Tachymeter-Kommunikation", || Ansicht::Kommunikation(Default::default())),
        ("Testmessungen", || Ansicht::NochNichtPortiert("Testmessungen")),
        ("Info", || Ansicht::Info),
    ];
    egui::Grid::new("module").num_columns(2).spacing([12.0, 12.0]).show(ui, |ui| {
        for (i, (name, erzeuge)) in module.iter().enumerate() {
            if ui.add_sized([KNOPF_BREITE, KNOPF_HOEHE], egui::Button::new(*name)).clicked() {
                protokoll::log("FORM", &format!("Modul geöffnet: {name}"));
                if *name == "DXF-Viewer" {
                    app.dxf_viewer.get_or_insert_with(Box::default);
                } else if *name == "Testmessungen" {
                    app.ansicht = Ansicht::Testmessungen(Box::new(super::testmessungen::TestmessungenAnsicht::neu(&app.projekt)));
                } else if *name == "Konvertierung" {
                    app.ansicht = Ansicht::Konvertierung(Box::new(super::konvertierung::KonvertierungAnsicht::neu(&app.projekt)));
                } else if *name == "Polarpunkte" {
                    app.ansicht = Ansicht::Polarpunkte(Box::new(super::polarpunkte::PolarpunkteAnsicht::neu(&app.projekt)));
                } else {
                    app.ansicht = erzeuge();
                }
            }
            if i % 2 == 1 {
                ui.end_row();
            }
        }
    });
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
            ui.label(egui::RichText::new(format!("Version {VERSION}")).size(15.0).color(egui::Color32::from_gray(90)));
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
