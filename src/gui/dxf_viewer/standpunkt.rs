// Zweck:    Standpunkt im DXF-Viewer: Infofeld oben links in der Zeichenfläche, Zielstrahlen zu den Anschlusspunkten,
//           Dialog „Neuer Stp", Prüfung der Aufstellung beim Öffnen, Korrektur der Instrumentenhöhe und
//           Orientierungskontrolle auf einen Anschlusspunkt (siehe Standpunkt.md, V1, V2, V6, V10)
// Hardware: Tachymeter für die Kontrollmessung (über gui::geraet)
// Autor:    Johann Gerner

use super::*;
use crate::daten::messdaten;
use crate::daten::modelle::StationierungsPunkt;
use crate::gui::standpunkt_dialog::{self, Ergebnis as DialogErgebnis, StandpunktDialog};
use crate::rechnen::polar::{self, KontrollErgebnis};
use crate::tachymeter::messablauf::MessErgebnis;
use crate::tachymeter::messung::TachymeterMessung;

/// Aufstellungen, die älter sind, werden beim Öffnen des Viewers hinterfragt [h].
const AUFSTELLUNG_MAX_STUNDEN: i64 = 8;
/// Abstand des Infofelds vom Rand der Zeichenfläche und Innenabstand [px].
const INFO_RAND: f32 = 10.0;
const INFO_INNEN: f32 = 8.0;
const INFO_ZEILE: f32 = 17.0;
const INFO_LAMPE: f32 = 18.0;
/// Farben des Infofelds.
const INFO_HINTERGRUND: Color32 = Color32::from_rgba_premultiplied(242, 244, 250, 225);
const INFO_RAHMEN: Color32 = Color32::from_rgb(120, 130, 160);
const INFO_TEXT: Color32 = Color32::from_rgb(25, 30, 55);
const INFO_GRAU: Color32 = Color32::from_rgb(95, 100, 120);
const INFO_WARNUNG: Color32 = Color32::from_rgb(190, 100, 0);
const INFO_FEHLER: Color32 = Color32::from_rgb(180, 30, 30);
const LAMPE_OFFEN: Color32 = Color32::from_rgb(230, 190, 30);
/// Zielstrahlen: Farben der Ampel und Strichbreite.
const STRAHL_GRUEN: Color32 = Color32::from_rgba_premultiplied(30, 150, 50, 200);
const STRAHL_GELB: Color32 = Color32::from_rgba_premultiplied(210, 150, 0, 210);
const STRAHL_ROT: Color32 = Color32::from_rgba_premultiplied(200, 30, 30, 220);
const STRAHL_GRAU: Color32 = Color32::from_rgba_premultiplied(140, 140, 140, 160);
const STRAHL_BREITE: f32 = 1.5;
/// Ampel der Zielstrahlen und der Kontrolle: gelb bis Grenze + Zuschlag [mm].
const AMPEL_GELB_ZUSCHLAG_MM: f64 = 5.0;
/// Gemessene Anschlusspunkte: roter Punkt, darüber blaues „T“ (Transformationspunkt).
const TP_PUNKT: Color32 = Color32::from_rgb(220, 20, 20);
const TP_TEXT: Color32 = Color32::from_rgb(20, 60, 220);
const TP_RADIUS: f32 = 4.5;
const TP_SCHRIFT: f32 = 15.0;
const TP_ABSTAND: f32 = 4.0;
/// Farbe des Knopfs „Neuer Stp".
pub(super) const NEUER_STP_FARBE: Color32 = Color32::from_rgb(220, 120, 20);
pub(super) const NEUER_STP_RAND: Color32 = Color32::from_rgb(180, 90, 10);

/// Ampelstufe einer Abweichung [mm] gegenüber der Fehlergrenze.
fn ampel_stufe(wert_mm: f64, grenze_mm: f64) -> u8 {
    let w = wert_mm.abs();
    if w <= grenze_mm {
        0
    } else if w <= grenze_mm + AMPEL_GELB_ZUSCHLAG_MM {
        1
    } else {
        2
    }
}

/// Dialog der Orientierungskontrolle (V2).
#[derive(Clone, Debug)]
pub(super) struct KontrollDialog {
    /// Anschlusspunkte der Aufstellung mit Koordinaten: (Nr, R, H).
    ziele: Vec<(String, f64, f64)>,
    auswahl: usize,
    zielhoehe: String,
    pub wartet: bool,
    messung: Option<TachymeterMessung>,
    ergebnis: Option<KontrollErgebnis>,
    info: String,
    gespeichert: bool,
    /// Stationierung beim Öffnen (Bezug der Kontrolle).
    station: Option<StationierungsErgebnis>,
}

impl KontrollDialog {
    fn neu(projekt: &Projekt, zielhoehe: &str) -> Self {
        let mut ziele: Vec<(String, f64, f64)> = Vec::new();
        for m in &projekt.station.messungen {
            if ziele.iter().any(|z| z.0 == m.punkt_nr) {
                continue;
            }
            if let Some((r, h, _)) = projekt.finde_anschlusspunkt(&m.punkt_nr) {
                ziele.push((m.punkt_nr.clone(), r, h));
            }
        }
        Self {
            ziele,
            auswahl: 0,
            zielhoehe: zielhoehe.to_string(),
            wartet: false,
            messung: None,
            ergebnis: None,
            info: String::new(),
            gespeichert: false,
            station: projekt.station.ergebnis.clone(),
        }
    }

    /// Übernimmt das Messergebnis des Geräts.
    pub fn uebernimm(&mut self, ergebnis: MessErgebnis) {
        self.wartet = false;
        match ergebnis {
            MessErgebnis::Messung(m) => {
                let (Some(hz), Some(v), Some(s)) = (m.hz_gon, m.v_gon, m.schraegstrecke_m) else {
                    self.info = "Unvollständige Messung (Hz/V/D fehlen).".into();
                    crate::grundlagen::ton::fehler();
                    return;
                };
                let Some((_, r, h)) = self.ziele.get(self.auswahl).cloned() else {
                    return;
                };
                let Some(station) = self.station.as_ref() else {
                    self.info = "Keine Stationierung vorhanden.".into();
                    return;
                };
                self.ergebnis = polar::kontrolle(station, r, h, hz, v, s);
                self.messung = Some(*m);
                self.gespeichert = false;
                self.info.clear();
                crate::grundlagen::ton::bestaetigung();
            }
            MessErgebnis::Fehler(text) => {
                self.info = format!("Messfehler: {text}");
                crate::grundlagen::ton::fehler();
            }
        }
    }
}

impl DxfViewer {
    // ── Neuer Standpunkt ──────────────────────────────────────────────────────

    /// Öffnet den Dialog „Neuer Stp".
    pub(super) fn oeffne_standpunkt_dialog(&mut self, projekt: &Projekt, geraet: &Geraet) {
        let zustand = standpunkt_dialog::geraetezustand(projekt, Some(geraet));
        self.dialog = Some(Dialog::Standpunkt(Box::new(StandpunktDialog::neu(projekt, zustand))));
    }

    /// Öffnet die Orientierungskontrolle.
    pub(super) fn oeffne_kontrolle(&mut self, projekt: &Projekt) {
        self.dialog = Some(Dialog::Kontrolle(Box::new(KontrollDialog::neu(projekt, &self.letzte_zielhoehe))));
    }

    /// Nach einem Wechsel der Aufstellung: Anzeigen und Eingabefelder neu aufbauen.
    pub(super) fn nach_standpunktwechsel(&mut self, projekt: &Projekt) {
        self.gesehene_station_version = projekt.station_version;
        self.txt_instr_hoehe = f(if projekt.hat_aktive_aufstellung() { projekt.station.instrumenten_hoehe } else { projekt.einstellungen.instrumenten_hoehe }, 3);
        self.baue_overlay(projekt);
        self.baue_neupunkte_overlay(projekt);
        self.baue_qualitaets_overlay(projekt);
        self.aktualisiere_lokales_system(projekt);
    }

    /// V1: Beim Öffnen fragen, ob eine ältere Aufstellung (anderer Tag oder älter als 8 h) noch gilt.
    pub(super) fn pruefe_aufstellung_beim_start(&mut self, projekt: &Projekt) {
        if !projekt.hat_aktive_aufstellung() || self.dialog.is_some() {
            return;
        }
        let Some(beginn) = projekt.aufstellung_beginn() else {
            return;
        };
        let jetzt = chrono::Local::now().naive_local();
        if beginn.date() == jetzt.date() && (jetzt - beginn).num_hours() < AUFSTELLUNG_MAX_STUNDEN {
            return;
        }
        let text = format!(
            "Aktive Aufstellung {} vom {} (IH {} m).\n\nSteht das Instrument noch unverändert?",
            projekt.station.standpunkt_nr,
            beginn.format("%d.%m.%Y, %H:%M"),
            f(projekt.station.instrumenten_hoehe, 3)
        );
        self.dialog = Some(Dialog::Aufstellungspruefung { text });
    }

    /// Korrektur der Instrumentenhöhe aus dem Toolbar-Feld (F2): Rückfrage, keine neue Aufstellung.
    pub(super) fn instr_hoehe_geaendert(&mut self, projekt: &mut Projekt) {
        let aktuell = if projekt.hat_aktive_aufstellung() { projekt.station.instrumenten_hoehe } else { projekt.einstellungen.instrumenten_hoehe };
        let Some(neu) = parse_zahl(&self.txt_instr_hoehe).filter(|v| v.is_finite() && *v >= 0.0) else {
            self.txt_instr_hoehe = f(aktuell, 3);
            self.status = "Ungültige Instrumentenhöhe – alter Wert bleibt.".into();
            return;
        };
        if !projekt.hat_aktive_aufstellung() {
            projekt.einstellungen.instrumenten_hoehe = neu;
            let _ = projekt.einstellungen.speichern();
            self.status = "Kein Standpunkt aktiv – die Höhe gilt als Vorgabe für „Neuer Stp“.".into();
            return;
        }
        if (neu - aktuell).abs() > 0.0005 {
            self.dialog = Some(Dialog::IhKorrektur { alt: aktuell, neu });
        }
    }

    // ── Dialoge ───────────────────────────────────────────────────────────────

    /// Zeichnet die Standpunkt-Dialoge; liefert den Dialog zurück, solange er offen bleibt.
    pub(super) fn zeichne_standpunkt_dialog(&mut self, ctx: &egui::Context, projekt: &mut Projekt, geraet: &mut Geraet, dialog: Dialog) -> Option<Dialog> {
        let fenster = |titel: &str| egui::Window::new(titel.to_string()).collapsible(false).resizable(false).anchor(Align2::CENTER_CENTER, Vec2::ZERO);
        match dialog {
            Dialog::Standpunkt(mut d) => match d.zeige(ctx, projekt) {
                DialogErgebnis::Offen => Some(Dialog::Standpunkt(d)),
                DialogErgebnis::Abgebrochen => None,
                DialogErgebnis::Fertig { meldung, fortgesetzt } => {
                    self.status = meldung;
                    self.nach_standpunktwechsel(projekt);
                    // Neue Aufstellung: zuerst stationieren; Fortsetzen: Orientierung kontrollieren (V2)
                    if fortgesetzt && projekt.hat_gueltige_station() && !self.lokales_system {
                        Some(Dialog::Kontrolle(Box::new(KontrollDialog::neu(projekt, &self.letzte_zielhoehe))))
                    } else {
                        if !fortgesetzt {
                            self.neupunkt_modus = false;
                        }
                        None
                    }
                }
            },
            Dialog::IhKorrektur { alt, neu } => {
                let mut antwort = None;
                fenster("Instrumentenhöhe korrigieren").show(ctx, |ui| {
                    ui.label(format!(
                        "Instrumentenhöhe für {} von {} m auf {} m korrigieren?\n\nDie Aufstellung bleibt dieselbe. Vorhandene Neupunkte dieser Aufstellung werden neu berechnet.",
                        projekt.station.standpunkt_nr,
                        f(alt, 3),
                        f(neu, 3)
                    ));
                    if let Some(h) = projekt.ih_hinweis(neu) {
                        ui.colored_label(INFO_WARNUNG, format!("⚠ {h}"));
                    }
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button(RichText::new("Korrigieren").strong()).clicked() {
                            antwort = Some(true);
                        }
                        if ui.button("Abbrechen").clicked() {
                            antwort = Some(false);
                        }
                    });
                });
                match antwort {
                    None => Some(Dialog::IhKorrektur { alt, neu }),
                    Some(true) => {
                        match projekt.set_instrumenten_hoehe(neu) {
                            Ok(()) => self.status = format!("Instrumentenhöhe {} m übernommen.", f(neu, 3)),
                            Err(e) => self.status = format!("Instrumentenhöhe konnte nicht gespeichert werden: {e}"),
                        }
                        self.nach_standpunktwechsel(projekt);
                        None
                    }
                    Some(false) => {
                        self.txt_instr_hoehe = f(alt, 3);
                        None
                    }
                }
            }
            Dialog::Aufstellungspruefung { text } => {
                let mut antwort = None;
                fenster("Aufstellung prüfen").show(ctx, |ui| {
                    ui.label(&text);
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button(RichText::new("Instrument unverändert – fortsetzen").strong()).clicked() {
                            antwort = Some(1);
                        }
                        let knopf = egui::Button::new(RichText::new("Neuer Stp").strong().color(Color32::WHITE)).fill(NEUER_STP_FARBE);
                        if ui.add(knopf).clicked() {
                            antwort = Some(2);
                        }
                        if ui.button("Später").clicked() {
                            antwort = Some(0);
                        }
                    });
                });
                match antwort {
                    None => Some(Dialog::Aufstellungspruefung { text }),
                    Some(1) => {
                        let nr = projekt.station.standpunkt_nr.clone();
                        if let Err(e) = projekt.standpunkt_fortsetzen(&nr) {
                            self.status = e;
                            return None;
                        }
                        self.nach_standpunktwechsel(projekt);
                        self.status = format!("Aufstellung {nr} fortgesetzt.");
                        (projekt.hat_gueltige_station() && !self.lokales_system).then(|| Dialog::Kontrolle(Box::new(KontrollDialog::neu(projekt, &self.letzte_zielhoehe))))
                    }
                    Some(2) => {
                        let zustand = standpunkt_dialog::geraetezustand(projekt, Some(geraet));
                        Some(Dialog::Standpunkt(Box::new(StandpunktDialog::neu(projekt, zustand))))
                    }
                    _ => None,
                }
            }
            Dialog::Kontrolle(d) => self.zeichne_kontrolle(ctx, projekt, geraet, d),
            anderer => Some(anderer),
        }
    }

    /// Dialog der Orientierungskontrolle.
    fn zeichne_kontrolle(&mut self, ctx: &egui::Context, projekt: &mut Projekt, geraet: &mut Geraet, mut d: Box<KontrollDialog>) -> Option<Dialog> {
        let grenze = projekt.rechenparameter.params.fehlergrenze_mm_strecke;
        let verbunden = geraet.ist_verbunden();
        let beschaeftigt = geraet.beschaeftigt();
        let mut messen = false;
        let mut speichern = false;
        let mut schliessen = false;
        if d.wartet {
            if let Some(text) = geraet.messung_status() {
                d.info = text.to_string();
            }
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        let mut offen = true;
        egui::Window::new("Orientierungskontrolle").open(&mut offen).collapsible(false).resizable(false).anchor(Align2::CENTER_CENTER, Vec2::ZERO).show(ctx, |ui| {
            ui.set_width(460.0);
            ui.label(format!("Standpunkt {}: einen Anschlusspunkt anzielen und messen.", projekt.station.standpunkt_nr));
            ui.add_space(4.0);
            if d.ziele.is_empty() {
                ui.colored_label(INFO_FEHLER, "Die Aufstellung hat keine Anschlusspunkte mit Koordinaten.");
            } else {
                let vorher = d.auswahl;
                egui::ComboBox::from_label("Anschlusspunkt").selected_text(d.ziele[d.auswahl].0.clone()).show_ui(ui, |ui| {
                    for (i, z) in d.ziele.iter().enumerate() {
                        ui.selectable_value(&mut d.auswahl, i, &z.0);
                    }
                });
                if vorher != d.auswahl {
                    d.ergebnis = None;
                    d.messung = None;
                }
                ui.horizontal(|ui| {
                    ui.label("Zielhöhe [m]:");
                    ui.add_sized([80.0, 22.0], egui::TextEdit::singleline(&mut d.zielhoehe).font(egui::TextStyle::Monospace));
                    let knopf = egui::Button::new(RichText::new("► Messen").strong().color(Color32::WHITE)).fill(Color32::from_rgb(160, 60, 20));
                    messen = ui.add_enabled(verbunden && !beschaeftigt && !d.wartet, knopf).clicked();
                });
                if !verbunden {
                    ui.colored_label(INFO_WARNUNG, "Kein Tachymeter verbunden.");
                }
            }
            if let Some(k) = &d.ergebnis {
                ui.add_space(6.0);
                let zeile = |ui: &mut egui::Ui, titel: &str, wert: String, stufe: Option<u8>| {
                    let farbe = match stufe {
                        Some(0) => STRAHL_GRUEN,
                        Some(1) => STRAHL_GELB,
                        Some(_) => STRAHL_ROT,
                        None => INFO_TEXT,
                    };
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(format!("{titel:<22}")).monospace());
                        ui.label(RichText::new(wert).monospace().strong().color(farbe));
                    });
                };
                zeile(ui, "Richtung (Δ Hz):", format!("{:+.1} mgon", k.d_hz_mgon), None);
                zeile(ui, "Querabweichung:", format!("{:+.1} mm", k.quer_mm), Some(ampel_stufe(k.quer_mm, grenze)));
                if k.laengs_mm.is_finite() {
                    zeile(ui, "Längsabweichung:", format!("{:+.1} mm", k.laengs_mm), Some(ampel_stufe(k.laengs_mm, grenze)));
                }
                zeile(ui, "Strecke (Soll):", format!("{} m", f(k.strecke_soll_m, 3)), None);
                let stufe = ampel_stufe(k.quer_mm, grenze).max(if k.laengs_mm.is_finite() { ampel_stufe(k.laengs_mm, grenze) } else { 0 });
                let (text, farbe) = match stufe {
                    0 => ("Orientierung in Ordnung.", STRAHL_GRUEN),
                    1 => ("Abweichung erhöht – bitte prüfen.", STRAHL_GELB),
                    _ => ("Abweichung zu groß – neu stationieren („Neuer Stp“).", STRAHL_ROT),
                };
                ui.colored_label(farbe, RichText::new(text).strong());
            }
            if !d.info.is_empty() {
                ui.label(&d.info);
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                speichern = ui.add_enabled(d.ergebnis.is_some() && !d.gespeichert, egui::Button::new("Kontrolle speichern")).clicked();
                schliessen = ui.button("Schließen").clicked();
            });
        });
        if self.mess_taste && verbunden && !beschaeftigt && !d.wartet {
            messen = true;
        }
        if messen {
            match geraet.starte_messung(parse_zahl(&d.zielhoehe).unwrap_or(0.0)) {
                Ok(()) => {
                    d.wartet = true;
                    d.info = "Messung startet …".into();
                }
                Err(e) => d.info = e,
            }
        }
        if speichern {
            if let (Some(m), Some(k), Some((nr, r, h))) = (&d.messung, &d.ergebnis, d.ziele.get(d.auswahl)) {
                let p = StationierungsPunkt {
                    punkt_nr: nr.clone(),
                    r: *r,
                    h: *h,
                    hoehe: 0.0,
                    hz: m.hz_gon.unwrap_or(0.0),
                    v: m.v_gon.unwrap_or(0.0),
                    strecke: m.schraegstrecke_m.unwrap_or(0.0),
                    zielhoehe: m.zielhoehe_m.unwrap_or(0.0),
                };
                let s = &projekt.station;
                {
                    use crate::daten::messprotokoll::{self as mp, Eintrag, Ereignis};
                    let laengs = if k.laengs_mm.is_finite() { format!("{:+.1} mm", k.laengs_mm) } else { "-".into() };
                    mp::protokolliere(
                        &projekt.einstellungen,
                        Eintrag::neu(Ereignis::Messung)
                            .mit_messung(m)
                            .mit_prisma(&projekt.einstellungen)
                            .mit_aufstellung(s)
                            .punkt(nr, messdaten::TYP_KONTROLLPUNKT, "")
                            .koordinaten(*r, *h, f64::NAN)
                            .text(format!("Orientierungskontrolle: dHz {:+.1} mgon, quer {:+.1} mm, laengs {laengs}", k.d_hz_mgon, k.quer_mm)),
                    );
                }
                match messdaten::schreibe_kontrollmessung(&projekt.einstellungen, &p, &s.standpunkt_nr, s.instrumenten_hoehe, &s.aufstellung_id, m.zeitstempel) {
                    Ok(()) => {
                        let laengs = if k.laengs_mm.is_finite() { format!("{:+.1} mm", k.laengs_mm) } else { "–".into() };
                        crate::grundlagen::protokoll::log(
                            "KONTROLLE",
                            &format!("SP={} Ziel {nr}: ΔHz {:+.1} mgon, quer {:+.1} mm, längs {laengs}", s.standpunkt_nr, k.d_hz_mgon, k.quer_mm),
                        );
                        d.gespeichert = true;
                        d.info = "Kontrolle in Messdatei und Protokoll gespeichert.".into();
                        self.status = format!("Orientierungskontrolle {nr}: quer {:+.1} mm", k.quer_mm);
                    }
                    Err(e) => d.info = format!("Speichern fehlgeschlagen: {e}"),
                }
            }
        }
        (offen && !schliessen).then_some(Dialog::Kontrolle(d))
    }

    // ── Zeichenfläche ─────────────────────────────────────────────────────────

    /// Infofeld oben links: Standpunkt, Instrumentenhöhe, Stationierungsqualität, Aufstellungsbeginn, Hinweise.
    pub(super) fn zeichne_standpunkt_info(&self, maler: &egui::Painter, rect: Rect, projekt: &Projekt) {
        let mut zeilen: Vec<(String, Color32, bool)> = Vec::new();
        let lampe: Option<Color32>;
        if !projekt.hat_aktive_aufstellung() {
            lampe = None;
            zeilen.push(("Kein Standpunkt".into(), INFO_FEHLER, true));
            zeilen.push(("„Neuer Stp“ drücken, um zu beginnen".into(), INFO_GRAU, false));
        } else {
            let s = &projekt.station;
            let mut kopf = format!("{}   IH {} m", s.standpunkt_nr, f(s.instrumenten_hoehe, 3));
            if !s.code.is_empty() {
                kopf.push_str(&format!("   Code {}", s.code));
            }
            zeilen.push((kopf, INFO_TEXT, true));
            if self.lokales_system {
                lampe = Some(Color32::from_rgb(30, 190, 50));
                zeilen.push(("Lokales System 0/0/0, Orientierung 0 gon".into(), INFO_TEXT, false));
            } else if projekt.hat_gueltige_station() {
                let (farbe, text, _) = self.signal(projekt);
                lampe = farbe;
                zeilen.push((text, INFO_TEXT, false));
            } else {
                lampe = Some(LAMPE_OFFEN);
                let min = projekt.min_anschlusspunkte();
                zeilen.push((format!("Stationierung offen – mind. {min} Anschlusspunkt(e) messen, dann stationieren"), INFO_WARNUNG, false));
                let offene = projekt.offene_neupunkte();
                if offene > 0 {
                    zeilen.push((format!("{offene} Neupunkt(e) ohne Stationierung"), INFO_WARNUNG, false));
                }
            }
            if let Some(b) = &s.bekannt {
                zeilen.push((format!("Bekannter Standpunkt {}", b.punkt_nr), INFO_GRAU, false));
            }
            if let Some(beginn) = projekt.aufstellung_beginn() {
                zeilen.push((format!("Aufstellung seit {}", beginn.format("%d.%m.%Y %H:%M")), INFO_GRAU, false));
            }
            if let Some(h) = projekt.ih_hinweis(s.instrumenten_hoehe) {
                zeilen.push((format!("⚠ {h}"), INFO_WARNUNG, false));
            }
        }
        let schrift = |fett: bool| FontId::proportional(if fett { 14.0 } else { 12.5 });
        let breite = zeilen
            .iter()
            .map(|(t, _, fett)| maler.layout_no_wrap(t.clone(), schrift(*fett), INFO_TEXT).size().x)
            .fold(0.0_f32, f32::max)
            .min(rect.width() - 2.0 * INFO_RAND - INFO_LAMPE - 3.0 * INFO_INNEN);
        let hoehe = zeilen.len() as f32 * INFO_ZEILE + 2.0 * INFO_INNEN;
        let kasten = Rect::from_min_size(rect.left_top() + Vec2::splat(INFO_RAND), Vec2::new(breite + INFO_LAMPE + 3.0 * INFO_INNEN, hoehe));
        maler.rect_filled(kasten, 4.0, INFO_HINTERGRUND);
        maler.rect_stroke(kasten, 4.0, Stroke::new(1.0, INFO_RAHMEN), egui::StrokeKind::Inside);
        let lampen_mitte = Pos2::new(kasten.left() + INFO_INNEN + INFO_LAMPE / 2.0, kasten.top() + INFO_INNEN + INFO_ZEILE / 2.0);
        zeichne_lampe(maler, Rect::from_center_size(lampen_mitte, Vec2::splat(INFO_LAMPE)), lampe);
        let x = kasten.left() + 2.0 * INFO_INNEN + INFO_LAMPE;
        let innen = maler.with_clip_rect(kasten);
        for (i, (text, farbe, fett)) in zeilen.into_iter().enumerate() {
            let y = kasten.top() + INFO_INNEN + i as f32 * INFO_ZEILE + INFO_ZEILE / 2.0;
            innen.text(Pos2::new(x, y), Align2::LEFT_CENTER, text, schrift(fett), farbe);
        }
    }

    /// Gemessene Anschlusspunkte der aktiven Aufstellung (jede Nummer einmal, nur mit Koordinaten).
    fn gemessene_anschlusspunkte(projekt: &Projekt) -> Vec<(f64, f64)> {
        let mut nummern: Vec<&str> = Vec::new();
        let mut punkte = Vec::new();
        for m in &projekt.station.messungen {
            if nummern.contains(&m.punkt_nr.as_str()) {
                continue;
            }
            nummern.push(&m.punkt_nr);
            if let Some((r, h, _)) = projekt.finde_anschlusspunkt(&m.punkt_nr).filter(|(r, h, _)| *r != 0.0 || *h != 0.0) {
                punkte.push((r, h));
            }
        }
        punkte
    }

    /// Gemessene Anschlusspunkte als rote Punkte mit blauem „T“ (Transformationspunkt) darüber.
    pub(super) fn zeichne_transformationspunkte(&self, maler: &egui::Painter, a: &crate::dxf::entity::Ansicht, projekt: &Projekt) {
        if self.lokales_system || !projekt.hat_aktive_aufstellung() {
            return;
        }
        for (r, h) in Self::gemessene_anschlusspunkte(projekt) {
            let p = a.zu_bildschirm(r, h);
            maler.circle(p, TP_RADIUS, TP_PUNKT, Stroke::new(1.0, Color32::WHITE));
            maler.text(Pos2::new(p.x, p.y - TP_RADIUS - TP_ABSTAND), Align2::CENTER_BOTTOM, "T", egui::FontId::proportional(TP_SCHRIFT), TP_TEXT);
        }
    }

    /// V10: Zielstrahlen vom Standpunkt zu den Anschlusspunkten, farbig nach Querabweichung.
    pub(super) fn zeichne_zielstrahlen(&self, maler: &egui::Painter, a: &crate::dxf::entity::Ansicht, projekt: &Projekt) {
        if !self.zielstrahlen_sichtbar || self.lokales_system || !projekt.hat_gueltige_station() {
            return;
        }
        let Some(st) = projekt.station.ergebnis.as_ref() else {
            return;
        };
        let grenze = projekt.rechenparameter.params.fehlergrenze_mm_strecke;
        let start = a.zu_bildschirm(st.r, st.h);
        for e in &self.residuen {
            let Art::Residual { res, wx, wy } = &e.art else {
                continue;
            };
            let farbe = if !res.richtung_aktiv || res.v_quer_mm.is_nan() {
                STRAHL_GRAU
            } else {
                match ampel_stufe(res.v_quer_mm, grenze) {
                    0 => STRAHL_GRUEN,
                    1 => STRAHL_GELB,
                    _ => STRAHL_ROT,
                }
            };
            let ziel = a.zu_bildschirm(*wx, *wy);
            if res.richtung_aktiv {
                maler.line_segment([start, ziel], Stroke::new(STRAHL_BREITE, farbe));
            } else {
                maler.extend(egui::Shape::dashed_line(&[start, ziel], Stroke::new(STRAHL_BREITE, farbe), 6.0, 4.0));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ampelstufen() {
        assert_eq!(ampel_stufe(3.0, 5.0), 0);
        assert_eq!(ampel_stufe(-8.0, 5.0), 1);
        assert_eq!(ampel_stufe(11.0, 5.0), 2);
    }
}
