// Zweck:    Gemeinsame Bausteine der Absteckungsformulare: Lageplan, Einweiser, Anschlusspunkte, Protokollziel
// Hardware: keine
// Autor:    Johann Gerner

use std::path::PathBuf;

use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, Sense, Shape, Stroke, Vec2};

use crate::daten::modelle::jetzt;
use crate::daten::projekt::Projekt;
use crate::grundlagen::fehler_log;
use crate::grundlagen::format::{f, parse_zahl};
use crate::protokolle::absteckung::AbsteckProtokoll;
use crate::protokolle::ausgabeverzeichnis;
use crate::rechnen::absteckung::{Abbildung, AbsteckPunkt, StandpunktInfo, berechne_einweisung};

/// Höhe des Lageplans [px].
pub const PLAN_HOEHE: f32 = 380.0;
/// Größe der Einweiser-Grafik [px].
pub const EINWEISER_GROESSE: f32 = 230.0;
/// Fangradius für Klicks im Lageplan [px].
const FANG_RADIUS: f32 = 12.0;
/// Radien der Sollpunkte im Lageplan [px].
const RADIUS_NORMAL: f32 = 5.0;
const RADIUS_AKTUELL: f32 = 8.0;
const RADIUS_STANDPUNKT: f32 = 8.0;
/// Toleranz des Winkels, unter der der Einweiser „in Toleranz" anzeigt [gon].
const HZ_TOLERANZ_GON: f64 = 0.05;
/// Standardtoleranz der Strecke [mm].
const TOLERANZ_STANDARD: &str = "10";
/// Höchstzahl der Zeilen einer Punktliste.
pub const MAX_ZEILEN: usize = 20;

const FARBE_PLAN: Color32 = Color32::from_rgb(245, 248, 252);
const FARBE_RAHMEN: Color32 = Color32::from_rgb(160, 170, 190);
const FARBE_GITTER: Color32 = Color32::from_rgb(210, 220, 230);
const FARBE_OFFEN: Color32 = Color32::from_rgb(65, 105, 225);
const FARBE_AKTUELL: Color32 = Color32::from_rgb(255, 215, 0);
const FARBE_ERLEDIGT: Color32 = Color32::from_rgb(46, 139, 87);
const FARBE_STANDPUNKT: Color32 = Color32::from_rgb(220, 20, 60);
const FARBE_STANDPUNKT_RAND: Color32 = Color32::from_rgb(139, 0, 0);
const FARBE_ZIELLINIE: Color32 = Color32::from_rgb(255, 165, 0);
const FARBE_POLYGON: Color32 = Color32::RED;
const FARBE_SCHNURGERUEST: Color32 = Color32::BLUE;
const FARBE_HINTERGRUND: Color32 = Color32::from_rgb(150, 150, 160);
const FARBE_GEMESSEN: Color32 = Color32::from_rgb(0, 160, 160);
const FARBE_ZEILE_ERLEDIGT: Color32 = Color32::from_rgb(180, 230, 180);
const FARBE_ZEILE_AKTUELL: Color32 = Color32::from_rgb(255, 240, 170);

/// Hintergrundfarbe einer Tabellenzeile für Absteckpunkte (None = Standard).
pub fn zeilenfarbe(erledigt: bool, aktuell: bool) -> Option<Color32> {
    if erledigt {
        Some(FARBE_ZEILE_ERLEDIGT)
    } else if aktuell {
        Some(FARBE_ZEILE_AKTUELL)
    } else {
        None
    }
}

/// Standpunkt für die Absteckung: die gültig stationierte aktive Aufstellung (Vorgabe) oder,
/// ausdrücklich gewählt, der letzte Rückwärtsschnitt.
pub fn standpunkt_fuer_absteckung(projekt: &Projekt, rueckwaerts: bool) -> Option<StandpunktInfo> {
    if rueckwaerts {
        return crate::rechnen::absteckung::lade_standpunkt_aus(&projekt.projektdaten, crate::rechnen::absteckung::KATEGORIE_RUECKWAERTS);
    }
    let e = projekt.station.ergebnis.as_ref().filter(|_| projekt.hat_gueltige_station())?;
    Some(StandpunktInfo { punkt_nr: projekt.station.standpunkt_nr.clone(), r: e.r, h: e.h, orientierung_gon: e.orientierung_gon })
}

/// Kopfzeile mit dem Standpunkt und – falls ein Rückwärtsschnitt vorliegt – der Wahl der Quelle.
/// Liefert `true`, wenn die Quelle umgeschaltet wurde (dann ist der Standpunkt neu zu laden).
pub fn zeige_standpunkt(ui: &mut egui::Ui, projekt: &Projekt, st: Option<&StandpunktInfo>, rueckwaerts: &mut bool) -> bool {
    let rw_vorhanden = crate::rechnen::absteckung::lade_standpunkt_aus(&projekt.projektdaten, crate::rechnen::absteckung::KATEGORIE_RUECKWAERTS).is_some();
    let mut geaendert = false;
    if rw_vorhanden || *rueckwaerts {
        ui.horizontal(|ui| {
            ui.label("Standpunkt aus:");
            geaendert |= ui.radio_value(rueckwaerts, false, "aktiver Aufstellung").changed();
            geaendert |= ui.radio_value(rueckwaerts, true, "Rückwärtsschnitt").changed();
        });
    }
    zeige_standpunkt_zeile(ui, st);
    geaendert
}

fn zeige_standpunkt_zeile(ui: &mut egui::Ui, st: Option<&StandpunktInfo>) {
    match st {
        Some(s) => {
            ui.label(egui::RichText::new(format!(
                "Standpunkt:  {}    R = {}    H = {}    z = {} gon",
                s.punkt_nr,
                f(s.r, 3),
                f(s.h, 3),
                f(s.orientierung_gon, 4)
            ))
            .monospace());
        }
        None => {
            ui.colored_label(
                Color32::from_rgb(180, 40, 40),
                "Kein Standpunkt – bitte Rückwärtsschnitt oder Freie Stationierung durchführen.",
            );
        }
    }
}

/// Liest die Anschlusspunkte (Nummer, R, H) aus der Anschlusspunktdatei, höchstens `MAX_ZEILEN`.
pub fn lade_anschlusspunkte(projekt: &Projekt) -> Result<Vec<(String, String, String)>, String> {
    let pfad = projekt.anschlusspunkte_pfad();
    if !pfad.exists() {
        return Err("Keine Punkte verfügbar.".to_string());
    }
    let text = std::fs::read_to_string(&pfad).map_err(|e| format!("Fehler beim Laden:\n{e}"))?;
    Ok(text
        .lines()
        .skip(1)
        .filter(|z| !z.trim().is_empty())
        .filter_map(|z| {
            let teile: Vec<&str> = z.split(',').collect();
            (teile.len() >= 3).then(|| (teile[0].trim().to_string(), teile[1].trim().to_string(), teile[2].trim().to_string()))
        })
        .take(MAX_ZEILEN)
        .collect())
}

/// Zahl aus einem Eingabefeld, sonst Fehlermeldung.
pub fn lese_zahl(text: &str, name: &str) -> Result<f64, String> {
    parse_zahl(text).ok_or_else(|| format!("Ungültiger Wert für {name}: '{text}'"))
}

// ── Protokollziel ────────────────────────────────────────────────────────────

/// Ziel und Zustand des Protokolls; schreibt beim Verlassen, falls noch nicht geschehen (wie im Original beim Schließen).
pub struct ProtokollZiel {
    verzeichnis: PathBuf,
    aktiv: bool,
    pub bearbeiter: String,
    pub geschrieben: bool,
}

impl ProtokollZiel {
    pub fn neu(projekt: &Projekt) -> Self {
        let mut ziel = Self { verzeichnis: PathBuf::new(), aktiv: false, bearbeiter: String::new(), geschrieben: false };
        ziel.aktualisiere(projekt);
        ziel
    }

    /// Mit festen Werten (ohne Projekt).
    #[cfg(test)]
    pub fn aus_werten(verzeichnis: PathBuf, aktiv: bool, bearbeiter: &str) -> Self {
        Self { verzeichnis, aktiv, bearbeiter: bearbeiter.to_string(), geschrieben: false }
    }

    /// Übernimmt Zielordner, Schalter und Bearbeiter aus dem Projekt.
    pub fn aktualisiere(&mut self, projekt: &Projekt) {
        self.verzeichnis = ausgabeverzeichnis(projekt);
        self.aktiv = projekt.einstellungen.protokoll_aktiv;
        self.bearbeiter = projekt.projektdaten.bearbeiter.clone();
        self.geschrieben = false;
    }

    /// Schreibt auf Knopfdruck; liefert die Meldung für den Benutzer.
    pub fn schreibe(&mut self, protokoll: Option<AbsteckProtokoll>) -> Result<String, String> {
        let Some(p) = protokoll else {
            return Err("Es gibt noch nichts zu protokollieren.".to_string());
        };
        if !self.aktiv {
            return Err("Die Protokollierung ist ausgeschaltet (Hauptmenü: „Protokollieren“).".to_string());
        }
        let (rtf, png) = p.schreibe(&self.verzeichnis)?;
        self.geschrieben = true;
        Ok(AbsteckProtokoll::meldung(&rtf, png.as_deref()))
    }

    /// Schreibt beim Verlassen, wenn die Protokollierung an ist und noch nichts geschrieben wurde.
    pub fn schreibe_beim_verlassen(&mut self, protokoll: Option<AbsteckProtokoll>, kontext: &str) {
        if !self.aktiv || self.geschrieben {
            return;
        }
        if let Some(p) = protokoll {
            match p.schreibe(&self.verzeichnis) {
                Ok(_) => self.geschrieben = true,
                Err(fehler) => fehler_log::log(kontext, &fehler),
            }
        }
    }

    /// Zeitpunkt für das Protokoll.
    pub fn jetzt() -> chrono::NaiveDateTime {
        jetzt()
    }
}

// ── Lageplan ─────────────────────────────────────────────────────────────────

/// Alles, was der Lageplan zeigt.
#[derive(Default)]
pub struct PlanDaten<'a> {
    pub station: Option<&'a StandpunktInfo>,
    pub punkte: &'a [AbsteckPunkt],
    pub aktuell: Option<usize>,
    pub polygon: &'a [(f64, f64)],
    pub schnurgeruest: &'a [AbsteckPunkt],
    pub achse: Option<((f64, f64), (f64, f64))>,
    /// Hervorgehobene Polygonkante (Index des Anfangspunkts).
    pub kante: Option<usize>,
    pub gemessen: Option<(f64, f64)>,
    /// Bekannte Projektpunkte (Nummer, R, H) als anklickbarer Hintergrund.
    pub hintergrund: &'a [(String, f64, f64)],
}

/// Klick im Lageplan.
pub struct PlanKlick {
    pub welt: (f64, f64),
    /// Index des angeklickten Sollpunkts in `PlanDaten::punkte`.
    pub punkt: Option<usize>,
    /// Index der angeklickten Polygonkante.
    pub kante: Option<usize>,
    /// Index des angeklickten Hintergrundpunkts in `PlanDaten::hintergrund`.
    pub hintergrund: Option<usize>,
}

fn abstand_zur_strecke(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let laenge2 = ab.length_sq();
    if laenge2 < 1e-9 {
        return (p - a).length();
    }
    let t = (((p - a).x * ab.x + (p - a).y * ab.y) / laenge2).clamp(0.0, 1.0);
    (p - (a + ab * t)).length()
}

/// Bekannte Projektpunkte: Importpunkte, Feldbuchpunkte und Anschlusspunktdatei (Nummer, R, H), ohne doppelte Nummern.
pub fn hintergrundpunkte(projekt: &Projekt) -> Vec<(String, f64, f64)> {
    let mut liste: Vec<(String, f64, f64)> = Vec::new();
    let mut neu = |nr: &str, r: f64, h: f64| {
        let nr = nr.trim();
        if !nr.is_empty() && !liste.iter().any(|(n, _, _)| n.eq_ignore_ascii_case(nr)) {
            liste.push((nr.to_string(), r, h));
        }
    };
    for p in projekt.importpunkte.punkte() {
        neu(&p.punkt_nr, p.r, p.h);
    }
    for p in projekt.feldbuchpunkte.punkte() {
        neu(&p.punkt_nr, p.r, p.h);
    }
    if let Ok(text) = std::fs::read_to_string(projekt.anschlusspunkte_pfad()) {
        for zeile in text.lines().skip(1) {
            let t: Vec<&str> = zeile.split(',').collect();
            if let (true, Some(r), Some(h)) = (t.len() >= 3, t.get(1).and_then(|x| parse_zahl(x)), t.get(2).and_then(|x| parse_zahl(x))) {
                neu(t[0], r, h);
            }
        }
    }
    liste
}

/// Zoom und Verschiebung des Lageplans gegenüber der einpassenden Grundansicht.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sicht {
    pub zoom: f32,
    pub pan: Vec2,
}

impl Default for Sicht {
    fn default() -> Self {
        Self { zoom: 1.0, pan: Vec2::ZERO }
    }
}

/// Zoomfaktor (Exponent) je Pixel Rollbewegung: ln(1,25) / 50.
const ZOOM_PRO_PIXEL: f32 = 0.0045;
const ZOOM_MIN: f32 = 0.2;
const ZOOM_MAX: f32 = 400.0;

impl Sicht {
    /// Grundansicht → Bildschirm; `mitte` ist der Mittelpunkt der Zeichenfläche.
    pub fn zu_schirm(&self, mitte: Pos2, grund: Pos2) -> Pos2 {
        mitte + (grund - mitte) * self.zoom + self.pan
    }

    /// Bildschirm → Grundansicht.
    pub fn zu_grund(&self, mitte: Pos2, schirm: Pos2) -> Pos2 {
        mitte + (schirm - mitte - self.pan) / self.zoom
    }

    /// Zoomt um `faktor`, sodass der Punkt unter der Maus an seinem Platz bleibt.
    pub fn zoome(&mut self, mitte: Pos2, maus: Pos2, faktor: f32) {
        let neu = (self.zoom * faktor).clamp(ZOOM_MIN, ZOOM_MAX);
        let verhaeltnis = neu / self.zoom;
        self.pan = (maus - mitte) - (maus - mitte - self.pan) * verhaeltnis;
        self.zoom = neu;
    }
}

/// Zeichnet den Lageplan (Raster, Polygon, Schnurgerüst, Achse, Sollpunkte, Standpunkt); liefert einen Klick.
/// Mausrad zoomt, Ziehen verschiebt, Doppelklick stellt die Grundansicht wieder her.
pub fn zeichne_lageplan(ui: &mut egui::Ui, plan: &PlanDaten, groesse: Vec2) -> Option<PlanKlick> {
    let (antwort, maler) = ui.allocate_painter(groesse, Sense::click_and_drag());
    let antwort = antwort.on_hover_text("Mausrad: Zoom · Ziehen: Verschieben · Doppelklick: Ansicht zurücksetzen");
    let rect = antwort.rect;
    let mitte = rect.center();
    let sicht_id = antwort.id.with("sicht");
    let mut sicht: Sicht = ui.ctx().data_mut(|d| d.get_temp(sicht_id)).unwrap_or_default();
    if antwort.double_clicked() {
        sicht = Sicht::default();
    }
    if antwort.dragged() {
        sicht.pan += antwort.drag_delta();
    }
    if antwort.hovered() {
        let rad = ui.input(|i| i.smooth_scroll_delta.y);
        if rad != 0.0 {
            if let Some(maus) = ui.input(|i| i.pointer.hover_pos()) {
                // Stufenlos: ein Mausradraster (etwa 50 px) entspricht ungefähr einer Stufe von 1,25
                sicht.zoome(mitte, maus, (rad * ZOOM_PRO_PIXEL).exp());
            }
        }
    }
    ui.ctx().data_mut(|d| d.insert_temp(sicht_id, sicht));

    maler.rect_filled(rect, 0.0, FARBE_PLAN);
    let mut alle: Vec<(f64, f64)> = plan.punkte.iter().chain(plan.schnurgeruest).map(|p| (p.r_soll, p.h_soll)).collect();
    alle.extend_from_slice(plan.polygon);
    if let Some(s) = plan.station {
        alle.push((s.r, s.h));
    }
    if let Some((a, e)) = plan.achse {
        alle.push(a);
        alle.push(e);
    }
    if let Some(m) = plan.gemessen {
        alle.push(m);
    }
    if alle.is_empty() {
        // Ohne eigene Punkte bestimmen die Projektpunkte den Ausschnitt
        alle.extend(plan.hintergrund.iter().map(|(_, r, h)| (*r, *h)));
    }
    let mut klick = None;
    if let Some(abb) = Abbildung::neu(&alle, f64::from(rect.width()), f64::from(rect.height())) {
        let bild = |r: f64, h: f64| -> Pos2 {
            let (x, y) = abb.bild(r, h);
            sicht.zu_schirm(mitte, rect.min + Vec2::new(x as f32, y as f32))
        };
        // Gitter
        let (min_r, max_r, min_h, max_h) = abb.grenzen();
        let weite = abb.raster_weite();
        let strich = Stroke::new(1.0, FARBE_GITTER);
        let mut r = (min_r / weite).ceil() * weite;
        while r <= max_r {
            maler.extend(Shape::dashed_line(&[bild(r, min_h), bild(r, max_h)], strich, 2.0, 3.0));
            r += weite;
        }
        let mut h = (min_h / weite).ceil() * weite;
        while h <= max_h {
            maler.extend(Shape::dashed_line(&[bild(min_r, h), bild(max_r, h)], strich, 2.0, 3.0));
            h += weite;
        }
        // Projektpunkte im Hintergrund
        for (nr, r, h) in plan.hintergrund {
            let pos = bild(*r, *h);
            maler.circle_stroke(pos, 3.0, Stroke::new(1.0, FARBE_HINTERGRUND));
            maler.text(pos + Vec2::new(5.0, -2.0), Align2::LEFT_CENTER, nr, FontId::proportional(9.0), FARBE_HINTERGRUND);
        }
        // Achse
        if let Some((a, e)) = plan.achse {
            maler.extend(Shape::dashed_line(&[bild(a.0, a.1), bild(e.0, e.1)], Stroke::new(1.5, FARBE_ZIELLINIE), 6.0, 4.0));
        }
        // Gebäudepolygon, ausgewählte Kante dicker
        if plan.polygon.len() >= 2 {
            let n = plan.polygon.len();
            let kanten = if n == 2 { 1 } else { n };
            for i in 0..kanten {
                let (a, b) = (plan.polygon[i], plan.polygon[(i + 1) % n]);
                let gewaehlt = plan.kante == Some(i);
                let stift = if gewaehlt { Stroke::new(4.0, FARBE_ZIELLINIE) } else { Stroke::new(2.0, FARBE_POLYGON) };
                maler.line_segment([bild(a.0, a.1), bild(b.0, b.1)], stift);
            }
        }
        // Schnurgerüst
        if plan.schnurgeruest.len() >= 2 {
            let n = plan.schnurgeruest.len();
            for i in 0..n {
                let (a, b) = (&plan.schnurgeruest[i], &plan.schnurgeruest[(i + 1) % n]);
                maler.extend(Shape::dashed_line(
                    &[bild(a.r_soll, a.h_soll), bild(b.r_soll, b.h_soll)],
                    Stroke::new(1.5, FARBE_SCHNURGERUEST),
                    6.0,
                    4.0,
                ));
            }
        }
        // Verbindung Standpunkt → aktueller Punkt
        if let (Some(s), Some(i)) = (plan.station, plan.aktuell) {
            if let Some(p) = plan.punkte.get(i) {
                maler.extend(Shape::dashed_line(&[bild(s.r, s.h), bild(p.r_soll, p.h_soll)], Stroke::new(1.5, FARBE_ZIELLINIE), 6.0, 4.0));
            }
        }
        // Sollpunkte
        for (i, p) in plan.punkte.iter().chain(plan.schnurgeruest).enumerate() {
            let pos = bild(p.r_soll, p.h_soll);
            let aktuell = plan.aktuell == Some(i) && i < plan.punkte.len();
            let radius = if aktuell { RADIUS_AKTUELL } else { RADIUS_NORMAL };
            let fuellung = if p.ist_abgesteckt() {
                FARBE_ERLEDIGT
            } else if aktuell {
                FARBE_AKTUELL
            } else {
                FARBE_OFFEN
            };
            maler.circle_filled(pos, radius, fuellung);
            maler.circle_stroke(pos, radius, Stroke::new(1.0, if aktuell { Color32::from_rgb(255, 140, 0) } else { Color32::DARK_GRAY }));
            maler.text(pos + Vec2::new(radius + 2.0, -2.0), Align2::LEFT_CENTER, &p.punkt_nr, FontId::proportional(10.0), Color32::BLACK);
        }
        // Gemessener Punkt
        if let Some(m) = plan.gemessen {
            let pos = bild(m.0, m.1);
            maler.circle_stroke(pos, 7.0, Stroke::new(2.0, FARBE_GEMESSEN));
            maler.line_segment([pos - Vec2::new(10.0, 0.0), pos + Vec2::new(10.0, 0.0)], Stroke::new(1.5, FARBE_GEMESSEN));
            maler.line_segment([pos - Vec2::new(0.0, 10.0), pos + Vec2::new(0.0, 10.0)], Stroke::new(1.5, FARBE_GEMESSEN));
        }
        // Standpunkt
        if let Some(s) = plan.station {
            let pos = bild(s.r, s.h);
            let rand = Stroke::new(1.5, FARBE_STANDPUNKT_RAND);
            maler.circle_filled(pos, RADIUS_STANDPUNKT, FARBE_STANDPUNKT);
            maler.circle_stroke(pos, RADIUS_STANDPUNKT, rand);
            let (innen, aussen) = (RADIUS_STANDPUNKT, RADIUS_STANDPUNKT + 5.0);
            for richtung in [Vec2::X, -Vec2::X, Vec2::Y, -Vec2::Y] {
                maler.line_segment([pos + richtung * innen, pos + richtung * aussen], rand);
            }
            maler.text(
                pos + Vec2::new(RADIUS_STANDPUNKT + 4.0, -4.0),
                Align2::LEFT_CENTER,
                &s.punkt_nr,
                FontId::proportional(12.0),
                FARBE_STANDPUNKT_RAND,
            );
        }
        // Klick auswerten (Sollpunkt vor Gebäudekante vor Projektpunkt)
        if antwort.clicked() {
            if let Some(maus) = antwort.interact_pointer_pos() {
                let grund = sicht.zu_grund(mitte, maus) - rect.min;
                let welt = abb.welt(f64::from(grund.x), f64::from(grund.y));
                let naechster = |punkte: &mut dyn Iterator<Item = (usize, Pos2)>| {
                    punkte
                        .map(|(i, pos)| (i, (pos - maus).length()))
                        .filter(|(_, d)| *d <= FANG_RADIUS)
                        .min_by(|a, b| a.1.total_cmp(&b.1))
                        .map(|(i, _)| i)
                };
                let punkt = naechster(&mut plan.punkte.iter().enumerate().map(|(i, p)| (i, bild(p.r_soll, p.h_soll))));
                let n = plan.polygon.len();
                let kante = (n >= 2 && punkt.is_none())
                    .then(|| {
                        (0..if n == 2 { 1 } else { n })
                            .map(|i| {
                                let (a, b) = (plan.polygon[i], plan.polygon[(i + 1) % n]);
                                (i, abstand_zur_strecke(maus, bild(a.0, a.1), bild(b.0, b.1)))
                            })
                            .filter(|(_, d)| *d <= FANG_RADIUS)
                            .min_by(|a, b| a.1.total_cmp(&b.1))
                            .map(|(i, _)| i)
                    })
                    .flatten();
                let hintergrund = (punkt.is_none() && kante.is_none())
                    .then(|| naechster(&mut plan.hintergrund.iter().enumerate().map(|(i, (_, r, h))| (i, bild(*r, *h)))))
                    .flatten();
                klick = Some(PlanKlick { welt, punkt, kante, hintergrund });
            }
        }
    } else {
        maler.text(rect.center(), Align2::CENTER_CENTER, "Keine Punkte", FontId::proportional(13.0), Color32::GRAY);
    }
    if sicht != Sicht::default() {
        maler.text(rect.right_top() + Vec2::new(-6.0, 6.0), Align2::RIGHT_TOP, format!("×{:.1}", sicht.zoom), FontId::proportional(11.0), Color32::GRAY);
    }
    maler.rect_stroke(rect, 0.0, Stroke::new(1.0, FARBE_RAHMEN), egui::StrokeKind::Inside);
    klick
}

// ── Einweiser ────────────────────────────────────────────────────────────────

/// Zielanzeige mit Abweichungsberechnung und Einweiser-Grafik (Zielscheibe).
pub struct Einweiser {
    punkt_nr: Option<String>,
    ziel: Option<(f64, f64)>,
    hz_ist: String,
    s_ist: String,
    toleranz: String,
    abweichung: Option<(f64, f64)>,
}

impl Default for Einweiser {
    fn default() -> Self {
        Self { punkt_nr: None, ziel: None, hz_ist: String::new(), s_ist: String::new(), toleranz: TOLERANZ_STANDARD.to_string(), abweichung: None }
    }
}

impl Einweiser {
    /// Setzt das Absteckziel; ohne Standpunkt gibt es keine Sollwerte.
    pub fn setze_ziel(&mut self, nr: Option<&str>, soll: Option<(f64, f64)>) {
        self.punkt_nr = nr.map(str::to_string);
        self.ziel = soll;
        self.abweichung = None;
        self.hz_ist.clear();
        self.s_ist.clear();
    }

    pub fn zuruecksetzen(&mut self) {
        self.setze_ziel(None, None);
    }

    fn toleranz_mm(&self) -> f64 {
        parse_zahl(&self.toleranz).unwrap_or(10.0)
    }

    /// Eingegebene Ist-Werte auswerten.
    fn berechne(&mut self) {
        let Some((hz_soll, s_soll)) = self.ziel else {
            return;
        };
        if let (Some(hz), Some(s)) = (parse_zahl(&self.hz_ist), parse_zahl(&self.s_ist)) {
            self.abweichung = Some(berechne_einweisung(hz_soll, s_soll, hz, s));
        }
    }

    /// Zeichnet den Einweiser; liefert die Punktnummer, wenn „Punkt markiert" geklickt wurde.
    pub fn zeichne(&mut self, ui: &mut egui::Ui) -> Option<String> {
        let mut markiert = None;
        ui.label(egui::RichText::new(self.punkt_nr.as_deref().map_or("Punkt: –".to_string(), |n| format!("Punkt: {n}"))).strong());
        match self.ziel {
            Some((hz, s)) => {
                ui.label(egui::RichText::new(format!("Hz soll:  {} gon", f(hz, 4))).monospace());
                ui.label(egui::RichText::new(format!("s soll:  {} m", f(s, 3))).monospace());
            }
            None => {
                ui.label("Hz soll: –");
                ui.label("s soll: –");
            }
        }
        self.zeichne_scheibe(ui);
        egui::Grid::new("einweiser_eingabe").num_columns(2).show(ui, |ui| {
            ui.label("Hz_ist [gon]:");
            ui.add(egui::TextEdit::singleline(&mut self.hz_ist).desired_width(100.0));
            ui.end_row();
            ui.label("s_ist [m]:");
            ui.add(egui::TextEdit::singleline(&mut self.s_ist).desired_width(100.0));
            ui.end_row();
            ui.label("Tol. [mm]:");
            ui.add(egui::TextEdit::singleline(&mut self.toleranz).desired_width(60.0));
            ui.end_row();
        });
        ui.horizontal(|ui| {
            if ui.add_enabled(self.ziel.is_some(), egui::Button::new("Abweichung")).clicked() {
                self.berechne();
            }
            if ui.add_enabled(self.punkt_nr.is_some(), egui::Button::new(egui::RichText::new("✓ Punkt markiert").strong())).clicked() {
                markiert = self.punkt_nr.clone();
            }
        });
        if let Some((dhz, ds)) = self.abweichung {
            let hz = if dhz < 0.0 { f(dhz, 4) } else { format!("+{}", f(dhz, 4)) };
            let s = if ds < 0.0 { f(ds, 1) } else { format!("+{}", f(ds, 1)) };
            ui.label(egui::RichText::new(format!("ΔHz = {hz} gon    Δs = {s} mm")).strong().color(Color32::from_rgb(200, 120, 0)));
        }
        markiert
    }

    fn zeichne_scheibe(&self, ui: &mut egui::Ui) {
        let (antwort, maler) = ui.allocate_painter(Vec2::splat(EINWEISER_GROESSE), Sense::hover());
        let rect = antwort.rect;
        maler.rect_filled(rect, 0.0, Color32::from_rgb(28, 32, 42));
        let mitte = rect.center();
        let r = rect.width().min(rect.height()) / 2.0 - 18.0;
        let toleranz = self.toleranz_mm();
        let in_toleranz = self.abweichung.is_some_and(|(dhz, ds)| dhz.abs() < HZ_TOLERANZ_GON && ds.abs() < toleranz);
        if r >= 30.0 {
            let grund = if in_toleranz { Color32::from_rgb(0, 60, 15) } else { Color32::from_rgb(28, 32, 42) };
            maler.circle_filled(mitte, r, grund);
            for (teil, farbe) in [
                (1.0, Color32::from_rgb(100, 110, 130)),
                (3.0, Color32::from_rgb(150, 160, 180)),
                (7.0, Color32::YELLOW),
                (10.0, Color32::from_rgb(50, 205, 50)),
            ] {
                maler.circle_stroke(mitte, r * teil / 10.0, Stroke::new(1.2, farbe));
            }
            let faden = Stroke::new(0.8, Color32::from_rgb(140, 150, 170));
            maler.line_segment([mitte - Vec2::new(r, 0.0), mitte + Vec2::new(r, 0.0)], faden);
            maler.line_segment([mitte - Vec2::new(0.0, r), mitte + Vec2::new(0.0, r)], faden);
            maler.circle_filled(mitte, 4.0, Color32::WHITE);
            if in_toleranz {
                maler.text(mitte, Align2::CENTER_CENTER, "✓ IN TOLERANZ", FontId::proportional(15.0), Color32::from_rgb(50, 205, 50));
            } else if let Some((dhz, ds)) = self.abweichung {
                let orange = Color32::from_rgb(255, 165, 0);
                if dhz.abs() >= HZ_TOLERANZ_GON {
                    let pfeil = if dhz > 0.0 { "◄ rechts drehen" } else { "► links drehen" };
                    maler.text(rect.min + Vec2::new(8.0, 8.0), Align2::LEFT_TOP, pfeil, FontId::proportional(14.0), orange);
                    maler.text(rect.min + Vec2::new(8.0, 28.0), Align2::LEFT_TOP, format!("{} gon", f(dhz.abs(), 2)), FontId::proportional(14.0), orange);
                }
                if ds.abs() >= toleranz {
                    let pfeil = if ds > 0.0 { "▲  vorwärts" } else { "▼  zurück" };
                    maler.text(rect.left_bottom() + Vec2::new(8.0, -52.0), Align2::LEFT_TOP, pfeil, FontId::proportional(14.0), Color32::from_rgb(0, 255, 255));
                    maler.text(rect.left_bottom() + Vec2::new(8.0, -32.0), Align2::LEFT_TOP, format!("{} mm", f(ds.abs(), 0)), FontId::proportional(14.0), Color32::from_rgb(0, 255, 255));
                }
            }
        }
        if let Some((hz, s)) = self.ziel {
            maler.text(
                rect.left_bottom() + Vec2::new(4.0, -10.0),
                Align2::LEFT_CENTER,
                format!("Hz: {} gon    s: {} m", f(hz, 4), f(s, 3)),
                FontId::monospace(11.0),
                Color32::from_rgb(180, 185, 200),
            );
        }
        maler.rect_stroke(Rect::from_min_size(rect.min, rect.size()), 0.0, Stroke::new(1.0, Color32::from_rgb(80, 90, 110)), egui::StrokeKind::Inside);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grundansicht_aendert_nichts() {
        let s = Sicht::default();
        let (m, p) = (Pos2::new(100.0, 100.0), Pos2::new(30.0, 70.0));
        assert_eq!(s.zu_schirm(m, p), p);
        assert_eq!(s.zu_grund(m, p), p);
    }

    #[test]
    fn zoom_haelt_den_punkt_unter_der_maus() {
        let mitte = Pos2::new(200.0, 150.0);
        let maus = Pos2::new(260.0, 90.0);
        let mut s = Sicht::default();
        let grund = s.zu_grund(mitte, maus);
        for faktor in [1.25, 1.25, 0.8, 1.25] {
            s.zoome(mitte, maus, faktor);
            let schirm = s.zu_schirm(mitte, grund);
            assert!((schirm - maus).length() < 1e-3, "{schirm:?}");
        }
        let p = Pos2::new(12.0, 34.0);
        assert!((s.zu_grund(mitte, s.zu_schirm(mitte, p)) - p).length() < 1e-3);
    }

    #[test]
    fn zoom_ist_begrenzt() {
        let (mitte, maus) = (Pos2::ZERO, Pos2::new(10.0, 10.0));
        let mut s = Sicht::default();
        for _ in 0..100 {
            s.zoome(mitte, maus, 1.25);
        }
        assert_eq!(s.zoom, ZOOM_MAX);
        for _ in 0..200 {
            s.zoome(mitte, maus, 0.8);
        }
        assert_eq!(s.zoom, ZOOM_MIN);
    }

    #[test]
    fn strecken_abstand() {
        let (a, b) = (Pos2::new(0.0, 0.0), Pos2::new(10.0, 0.0));
        assert_eq!(abstand_zur_strecke(Pos2::new(5.0, 3.0), a, b), 3.0);
        assert_eq!(abstand_zur_strecke(Pos2::new(-4.0, 3.0), a, b), 5.0);
        assert_eq!(abstand_zur_strecke(Pos2::new(1.0, 1.0), a, a), 2f32.sqrt());
    }
}
