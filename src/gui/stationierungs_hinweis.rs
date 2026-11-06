// Zweck:    Bestätigungsfenster nach einer Neuberechnung der Stationierung: Koordinatendifferenz des Standpunkts,
//           Änderung der Orientierung t und Folgen für die Neupunkte
// Hardware: keine
// Autor:    Johann Gerner

use eframe::egui::{self, Color32, RichText};

use crate::daten::projekt::StationierungsAenderung;
use crate::grundlagen::format::f;

/// Farbe für Werte, die sich geändert haben.
const FARBE_GEAENDERT: Color32 = Color32::from_rgb(200, 130, 40);
/// Farbe für unveränderte Werte.
const FARBE_GLEICH: Color32 = Color32::from_rgb(40, 150, 60);
/// Farbe für den Hinweis zum Abbrechen.
const FARBE_HINWEIS: Color32 = Color32::from_rgb(120, 120, 120);
/// Differenzen unter dieser Schwelle gelten als unverändert [m bzw. gon].
const SCHWELLE_UNVERAENDERT: f64 = 0.000_05;
/// Vollkreis [gon].
const VOLLKREIS_GON: f64 = 400.0;
/// Umrechnung Meter → Millimeter und gon → mgon.
const FAKTOR_MILLI: f64 = 1000.0;
/// Nachkommastellen der Anzeige.
const STELLEN_METER: usize = 3;
const STELLEN_GON: usize = 4;

/// Differenz zweier Richtungen [gon], auf ±200 gon normiert.
fn winkel_differenz_gon(neu: f64, alt: f64) -> f64 {
    let halb = VOLLKREIS_GON / 2.0;
    (neu - alt + halb).rem_euclid(VOLLKREIS_GON) - halb
}

/// Zeile der Tabelle: Name, alter und neuer Wert, Differenz (mit Vorzeichen) in der Einheit.
fn zeile(ui: &mut egui::Ui, name: &str, alt: f64, neu: f64, diff: f64, einheit: &str, stellen: usize, milli: &str) {
    let farbe = if diff.abs() < SCHWELLE_UNVERAENDERT { FARBE_GLEICH } else { FARBE_GEAENDERT };
    ui.label(name);
    ui.label(RichText::new(f(alt, stellen)).monospace());
    ui.label(RichText::new(f(neu, stellen)).monospace());
    let text = format!("{diff:+.stellen$} {einheit}   ({:+.1} {milli})", diff * FAKTOR_MILLI);
    ui.label(RichText::new(text).monospace().strong().color(farbe));
    ui.end_row();
}

/// Inhalt des Fensters; liefert `Some(true)` bei „Übernehmen“, `Some(false)` bei „Abbrechen“.
/// `warnung`: Hinweis, dass die Stationierung ohne Kontrolle übernommen würde; `aenderung`: Vergleich mit der bisherigen Stationierung.
pub fn zeichne(ui: &mut egui::Ui, standpunkt_nr: &str, warnung: Option<&str>, aenderung: Option<&StationierungsAenderung>) -> Option<bool> {
    if let Some(text) = warnung {
        ui.label(RichText::new(format!("⚠ {text}")).strong().color(FARBE_GEAENDERT));
        ui.add_space(6.0);
    }
    match aenderung {
        Some(a) => zeichne_aenderung(ui, a),
        None => {
            ui.label(RichText::new(format!("Stationierung von Standpunkt {standpunkt_nr} übernehmen?")).strong());
        }
    }
    ui.add_space(8.0);
    let mut antwort = None;
    ui.horizontal(|ui| {
        if ui.button(RichText::new("Übernehmen").strong()).clicked() {
            antwort = Some(true);
        }
        if ui.button("Abbrechen").clicked() {
            antwort = Some(false);
        }
    });
    antwort
}

/// Tabelle mit alt, neu und Differenz sowie den Folgen für die Neupunkte.
fn zeichne_aenderung(ui: &mut egui::Ui, a: &StationierungsAenderung) {
    ui.label(RichText::new(format!("Die neue Stationierung von Standpunkt {} weicht von der bisherigen ab.", a.standpunkt_nr)).strong());
    ui.add_space(6.0);
    egui::Grid::new("stationierungs_aenderung").num_columns(4).spacing([14.0, 4.0]).show(ui, |ui| {
        ui.label("");
        ui.label(RichText::new("alt").strong());
        ui.label(RichText::new("neu").strong());
        ui.label(RichText::new("Differenz (neu − alt)").strong());
        ui.end_row();
        zeile(ui, "R [m]", a.alt.r, a.neu.r, a.neu.r - a.alt.r, "m", STELLEN_METER, "mm");
        zeile(ui, "H [m]", a.alt.h, a.neu.h, a.neu.h - a.alt.h, "m", STELLEN_METER, "mm");
        if a.alt.berechnung_3d && a.neu.berechnung_3d {
            zeile(ui, "Höhe [m]", a.alt.hoehe, a.neu.hoehe, a.neu.hoehe - a.alt.hoehe, "m", STELLEN_METER, "mm");
        }
        let dt = winkel_differenz_gon(a.neu.orientierung_gon, a.alt.orientierung_gon);
        zeile(ui, "Orientierung t [gon]", a.alt.orientierung_gon, a.neu.orientierung_gon, dt, "gon", STELLEN_GON, "mgon");
    });
    let lage = (a.neu.r - a.alt.r).hypot(a.neu.h - a.alt.h);
    ui.add_space(4.0);
    ui.label(format!("Lageänderung des Standpunkts: {} mm", f(lage * FAKTOR_MILLI, 1)));
    ui.label(format!("Standardabweichung s0: {} mm → {} mm", f(a.alt.s0_mm, 2), f(a.neu.s0_mm, 2)));
    ui.separator();
    ui.label(format!(
        "Beim Übernehmen werden {} Neupunkt(e) neu berechnet und gespeichert (davon {} Polarpunkte).",
        a.neupunkte, a.davon_polarpunkte
    ));
    ui.label(RichText::new("Abbrechen: Die bisherigen Koordinaten und die bisherige Orientierung bleiben erhalten.").color(FARBE_HINWEIS));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn winkeldifferenz_laeuft_ueber_den_nullpunkt() {
        assert!((winkel_differenz_gon(0.001, 399.999) - 0.002).abs() < 1e-9);
        assert!((winkel_differenz_gon(399.999, 0.001) + 0.002).abs() < 1e-9);
        assert!((winkel_differenz_gon(10.0, 10.0)).abs() < 1e-12);
    }
}
