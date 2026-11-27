// Zweck:    Ergänzungen des GNSS-Betriebs im DXF-Viewer (GNSS.md 6.2, 6.3 und Abschnitt 12):
//           Rückfrage zur Stabhöhenkorrektur der letzten Messung, Warnung bei zu großer Streuung („Trotzdem übernehmen /
//           Wiederholen“), zweite unabhängige Belegung mit Vergleich und Mittel, Kontrolle am Sitzungsbeginn,
//           Höhenbezug sichtbar und erzwungen, UBX-Rohdatenaufzeichnung je Sitzung, Exzentrum (Längs/Quer, Bogenschnitt,
//           Geradenschnitt), Linienaufnahme, Höhenanschluss an einen Höhenfestpunkt, Satellitenvorhersage,
//           GNSS-Punkte als Festpunkte für die Freie Stationierung und Umschalten der Messquelle
// Hardware: GNSS-Empfänger u-blox ZED-F9P (über das GNSS-Modul), Tachymeter (Umschaltung der Messquelle)
// Autor:    Johann Gerner

use std::time::Instant;

use chrono::{Local, NaiveDateTime};
use eframe::egui::{self, Align2, Color32, FontId, Pos2, RichText, Sense, Stroke, Vec2};

use super::gnss::{Auftrag, MM_PRO_M, Messwert, Soll, TYP_GNSS, Zweck, fix_farbe};
use super::{Dialog, DxfViewer};
use crate::daten::messprotokoll::{self, Eintrag, Ereignis};
use crate::daten::projekt::Projekt;
use crate::gnss::ansicht::app::App;
use crate::gnss::einpassung::HoehenAnschluss;
use crate::gnss::kern::geo::Geographic;
use crate::gnss::kern::measure::Criteria;
use crate::gnss::kern::model::Fix;
use crate::gnss::punkte::{self as gp, GnssPunkt};
use crate::gnss::vorhersage::{self, PDOP_SCHLECHT, PDOP_SCHWACH, Zeitpunkt};
use crate::grundlagen::format::{f, parse_zahl};
use crate::grundlagen::{protokoll, ton};
use crate::gui::geraet::Geraet;
use crate::tachymeter::modell::TachymeterModell;

/// Quelle berechneter Exzenterpunkte.
pub(super) const QUELLE_EXZENTRUM: &str = "EXZENTRUM";
/// Quelle von Punkten aus der Wiedergabe.
const QUELLE_WIEDERGABE: &str = "WIEDERGABE";
/// Typ der Exzenterpunkte im Messprotokoll.
const TYP_GNSS_EXZENTRUM: &str = "GNSS-EXZ";
/// Unterordner der Rohdaten im Projekt.
const ROHDATEN_ORDNER: &str = "GNSS";
/// Sekunden je Minute.
const S_PRO_MIN: f64 = 60.0;
/// Millisekunden je Sekunde.
const MS_PRO_S: f64 = 1000.0;
/// Bytes je Megabyte (Anzeige).
const BYTE_PRO_MB: f64 = 1_048_576.0;
/// Satellitenvorhersage: Dauer [min] und Schritt [min].
const VORHERSAGE_DAUER_MIN: f64 = 360.0;
const VORHERSAGE_SCHRITT_MIN: f64 = 10.0;
/// Größe der Vorhersagegrafik [px] und angezeigter PDOP-Bereich.
const VORHERSAGE_GRAFIK: Vec2 = Vec2::new(560.0, 180.0);
const VORHERSAGE_PDOP_MAX: f64 = 10.0;
/// Lage der Fenster unter der Werkzeugleiste und neben der Statusanzeige [px].
const FENSTER_VERSATZ: Vec2 = Vec2::new(20.0, 140.0);
/// Kleinste sinnvolle Strecke für Richtungen [m].
const MIN_STRECKE_M: f64 = 1e-6;
/// Farben der Bewertung.
const FARBE_GUT: Color32 = Color32::from_rgb(40, 170, 60);
const FARBE_WARNUNG: Color32 = Color32::from_rgb(220, 170, 0);
const FARBE_SCHLECHT: Color32 = Color32::from_rgb(220, 60, 40);

/// Rückfragen des GNSS-Betriebs (als ein Eintrag im Dialog-Enum des Viewers).
pub(super) enum GnssDialog {
    /// Stabhöhe geändert, nachdem der Punkt `nr` gemessen wurde
    Stab { nr: String, neu: f64 },
    /// Streuung der Mittelung über den Grenzen
    Streuung { zweck: Zweck, m: Box<Messwert> },
    /// Punktnummer ist schon gemessen: zweite Belegung, Ersetzen oder Verwerfen
    Zweitbelegung { nr: String, code: String, m: Box<Messwert> },
    /// Höhenbezug bestätigen (erste Messung) oder abweichenden Bezug melden; danach messen bzw. die Linie starten
    Hoehenbezug { im_projekt: Option<String>, aktuell: String, linie: bool },
    /// Kontrollpunkt am Sitzungsbeginn vorschlagen
    Sitzungskontrolle,
}

/// Laufende Linienaufnahme (Stop-and-go).
#[derive(Default)]
pub(super) struct Linie {
    pub nummern: Vec<String>,
    letzte_pos: Option<(f64, f64)>,
    letzte_zeit: Option<Instant>,
}

/// Verfahren der Exzenterberechnung.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum ExzMethode {
    #[default]
    LaengsQuer,
    Bogenschnitt,
    Geradenschnitt,
}

impl ExzMethode {
    const ALLE: [Self; 3] = [Self::LaengsQuer, Self::Bogenschnitt, Self::Geradenschnitt];

    fn anzeige(self) -> &'static str {
        match self {
            Self::LaengsQuer => "Längs/Quer zu A→B",
            Self::Bogenschnitt => "Bogenschnitt aus A und B",
            Self::Geradenschnitt => "Geradenschnitt A–B mit C–D",
        }
    }
}

/// Fenster „Exzentrum“.
#[derive(Default)]
pub(super) struct ExzentrumFenster {
    methode: ExzMethode,
    a: String,
    b: String,
    c: String,
    d: String,
    laengs: String,
    quer: String,
    s_a: String,
    s_b: String,
    rechts: bool,
    nr: String,
    code: String,
    meldung: String,
}

/// Fenster „Satellitenvorhersage“.
#[derive(Default)]
pub(super) struct VorhersageFenster {
    /// beim nächsten Abgleich mit der Sitzung neu rechnen
    pub anfordern: bool,
    ergebnis: Vec<Zeitpunkt>,
    satelliten_mit_bahn: usize,
    berechnet: String,
    meldung: String,
}

// ── Geometrie der Exzenterberechnung (Plan- und UTM-Koordinaten gleichermaßen) ─────────────────────────────

/// Einheitsvektor A→B und Länge.
fn richtung(a: (f64, f64), b: (f64, f64)) -> Option<((f64, f64), f64)> {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let s = dx.hypot(dy);
    (s > MIN_STRECKE_M).then_some(((dx / s, dy / s), s))
}

/// Punkt `laengs` Meter hinter B in Richtung A→B und `quer` Meter rechts davon.
pub(super) fn laengs_quer(a: (f64, f64), b: (f64, f64), laengs: f64, quer: f64) -> Option<(f64, f64)> {
    let ((ux, uy), _) = richtung(a, b)?;
    // rechts der Richtung (Rechtswert nach Osten, Hochwert nach Norden)
    let (nx, ny) = (uy, -ux);
    Some((b.0 + laengs * ux + quer * nx, b.1 + laengs * uy + quer * ny))
}

/// Bogenschnitt aus den Strecken `s_a` (von A) und `s_b` (von B); `rechts`: Lösung rechts von A→B.
pub(super) fn bogenschnitt(a: (f64, f64), b: (f64, f64), s_a: f64, s_b: f64, rechts: bool) -> Option<(f64, f64)> {
    let ((ux, uy), d) = richtung(a, b)?;
    let x = (s_a * s_a - s_b * s_b + d * d) / (2.0 * d);
    let q = s_a * s_a - x * x;
    if q < 0.0 {
        return None;
    }
    let y = q.sqrt() * if rechts { 1.0 } else { -1.0 };
    Some((a.0 + x * ux + y * uy, a.1 + x * uy - y * ux))
}

/// Schnittpunkt der Geraden A–B und C–D.
pub(super) fn geradenschnitt(a: (f64, f64), b: (f64, f64), c: (f64, f64), d: (f64, f64)) -> Option<(f64, f64)> {
    let (r, s) = ((b.0 - a.0, b.1 - a.1), (d.0 - c.0, d.1 - c.1));
    let nenner = r.0 * s.1 - r.1 * s.0;
    if nenner.abs() < MIN_STRECKE_M {
        return None;
    }
    let t = ((c.0 - a.0) * s.1 - (c.1 - a.1) * s.0) / nenner;
    Some((a.0 + t * r.0, a.1 + t * r.1))
}

/// Minuten zwischen zwei Zeitstempeln des Messprotokolls.
fn minuten_zwischen(alt: &str, neu: &str) -> Option<f64> {
    const FORMAT: &str = "%Y-%m-%dT%H:%M:%S";
    let a = NaiveDateTime::parse_from_str(alt, FORMAT).ok()?;
    let b = NaiveDateTime::parse_from_str(neu, FORMAT).ok()?;
    Some((b - a).num_seconds() as f64 / S_PRO_MIN)
}

/// Höhe im GNSS-Bezug (Bodenpunkt) eines Messwerts.
fn roh_hoehe(m: &Messwert) -> f64 {
    m.h_bezug.unwrap_or(m.h_ell)
}

fn fenster(titel: &str) -> egui::Window<'static> {
    egui::Window::new(titel.to_string()).collapsible(false).resizable(false).anchor(Align2::CENTER_CENTER, Vec2::ZERO)
}

/// Großer Dialogknopf (Toughpad).
fn gross(ui: &mut egui::Ui, text: &str, haupt: bool) -> bool {
    let t = RichText::new(text).size(15.0);
    ui.add(egui::Button::new(if haupt { t.strong() } else { t }).min_size(Vec2::new(170.0, 40.0))).clicked()
}

impl DxfViewer {
    // ── Sitzungsbeginn, Rohdaten, Linienaufnahme, Vorhersage (vor dem Zeichnen) ──────────────────────────────

    /// Abgleich mit der Sitzung in jedem Bild: Sitzungsbeginn, Rohdatenaufzeichnung, Linienaufnahme, Vorhersage.
    pub(super) fn gnss_extras_vorher(&mut self, app: &mut App, projekt: &Projekt, neue_epoche: bool) {
        let verbunden = self.gnss.status.connected;
        if verbunden && !self.gnss.war_verbunden {
            self.gnss.aufzeichnung_versucht = false;
            let g = &projekt.rechenparameter.params.gnss;
            if g.sitzungskontrolle && self.gnss.einpassungen.aktive().is_some_and(|e| e.ergebnis.is_some()) && self.dialog.is_none() {
                self.dialog = Some(Dialog::Gnss(Box::new(GnssDialog::Sitzungskontrolle)));
            }
        }
        self.gnss.war_verbunden = verbunden;
        // UBX-Rohdaten je Sitzung im Projekt mitschreiben
        if verbunden && !self.gnss.status.replay && projekt.rechenparameter.params.gnss.rohdaten && !self.gnss.aufzeichnung_versucht && projekt.einstellungen.ist_geladen() {
            self.gnss.aufzeichnung_versucht = true;
            if app.aufzeichnung().is_none() {
                let ordner = projekt.einstellungen.get_pfad(ROHDATEN_ORDNER);
                let pfad = ordner.join(format!("{}.ubx", Local::now().format("%Y-%m-%d_%H%M")));
                let ergebnis = std::fs::create_dir_all(&ordner).map_err(|e| e.to_string()).and_then(|()| app.starte_aufzeichnung(&pfad));
                match ergebnis {
                    Ok(()) => {
                        messprotokoll::protokolliere(
                            &projekt.einstellungen,
                            Eintrag::neu(Ereignis::GnssVerbindung).quelle(messprotokoll::QUELLE_PROGRAMM).text(format!("UBX-Rohdatenaufzeichnung {}", pfad.display())),
                        );
                        protokoll::log("GNSS", &format!("Rohdatenaufzeichnung {}", pfad.display()));
                    }
                    Err(e) => self.status = format!("Rohdatenaufzeichnung nicht möglich: {e}"),
                }
            }
        }
        self.gnss.aufzeichnung = app.aufzeichnung();
        if neue_epoche {
            self.gnss_linie_epoche(projekt);
        }
        if self.gnss.vorhersage.as_ref().is_some_and(|v| v.anfordern) {
            self.gnss_vorhersage_rechnen(app, projekt);
        }
    }

    /// Linienaufnahme: nach Weg oder Zeit automatisch eine Einfachmessung auslösen.
    fn gnss_linie_epoche(&mut self, projekt: &Projekt) {
        let (Some(linie), Some(pos)) = (self.gnss.linie.as_ref(), self.gnss.pos) else {
            return;
        };
        let fix = self.gnss.live.as_ref().is_some_and(|l| l.fix == Fix::Fixed && !l.stale);
        if !fix || self.gnss.laufend.is_some() || self.dialog.is_some() || !self.gnss.status.connected {
            return;
        }
        let g = &projekt.rechenparameter.params.gnss;
        let faellig = if g.linie_zeit_s > 0.0 {
            linie.letzte_zeit.is_none_or(|t| t.elapsed().as_secs_f64() >= g.linie_zeit_s)
        } else {
            linie.letzte_pos.is_none_or(|(r, h)| (pos.r - r).hypot(pos.h - h) >= g.linie_abstand_m.max(MIN_STRECKE_M))
        };
        if !faellig {
            return;
        }
        let nr = self.txt_punkt_nr.trim().to_string();
        if nr.parse::<i64>().is_err() {
            self.gnss.linie = None;
            self.status = "Linienaufnahme beendet: die Punktnummer muss eine Zahl sein (wird hochgezählt).".into();
            ton::fehler();
            return;
        }
        let code = self.txt_code.trim().to_string();
        let kriterien = Criteria { epochs: 1, min_seconds: 0.0, ..self.gnss.kriterien.clone().unwrap_or_default() };
        self.gnss_starte(Zweck::Linie { nr: nr.clone(), code: code.clone() }, nr, code, kriterien);
    }

    /// Ein Linienpunkt ist gespeichert: Lage und Zeit merken.
    pub(super) fn gnss_linie_gemessen(&mut self, nr: &str, m: &Messwert) {
        if let Some(l) = self.gnss.linie.as_mut() {
            l.nummern.push(nr.to_string());
            l.letzte_pos = Some((m.pos.r, m.pos.h));
            l.letzte_zeit = Some(Instant::now());
        }
    }

    /// Linienaufnahme ein- oder ausschalten.
    pub(super) fn gnss_linie_umschalten(&mut self, projekt: &Projekt) {
        if let Some(l) = self.gnss.linie.take() {
            self.status = format!("Linienaufnahme beendet: {} Punkte.", l.nummern.len());
            return;
        }
        if !self.gnss_hoehenbezug_frei(true) {
            return;
        }
        let g = &projekt.rechenparameter.params.gnss;
        self.gnss.linie = Some(Linie::default());
        self.status = if g.linie_zeit_s > 0.0 {
            format!("Linienaufnahme: ein Punkt alle {} s (Nummer und Code aus der Werkzeugleiste).", f(g.linie_zeit_s, 1))
        } else {
            format!("Linienaufnahme: ein Punkt alle {} m (Nummer und Code aus der Werkzeugleiste).", f(g.linie_abstand_m, 2))
        };
    }

    // ── Rückfragen ───────────────────────────────────────────────────────────────────────────────────────────

    /// Prüft vor einer Messung (bzw. vor dem Start der Linienaufnahme) den Höhenbezug; `true`, wenn gemessen werden darf.
    pub(super) fn gnss_hoehenbezug_frei(&mut self, linie: bool) -> bool {
        if self.gnss.hoehenbezug_ok {
            return true;
        }
        let aktuell = self.gnss.hoehenbezug.clone();
        let gemessen: Vec<&GnssPunkt> = self.gnss.punkte.iter().filter(|p| p.quelle != QUELLE_EXZENTRUM && !p.hoehenbezug.is_empty()).collect();
        if let Some(p) = gemessen.iter().find(|p| p.hoehenbezug != aktuell) {
            self.dialog = Some(Dialog::Gnss(Box::new(GnssDialog::Hoehenbezug { im_projekt: Some(p.hoehenbezug.clone()), aktuell, linie })));
            return false;
        }
        if gemessen.is_empty() {
            self.dialog = Some(Dialog::Gnss(Box::new(GnssDialog::Hoehenbezug { im_projekt: None, aktuell, linie })));
            return false;
        }
        self.gnss.hoehenbezug_ok = true;
        true
    }

    /// Stabhöhe wurde geändert: Rückfrage, ob die letzte Messung korrigiert werden soll.
    pub(super) fn gnss_stab_geaendert(&mut self, neu: f64) {
        let Some(nr) = self.gnss.letzte.clone() else {
            return;
        };
        let gemessen_mit = self.gnss.punkte.iter().find(|p| p.nr == nr).map(|p| p.stab_m);
        if gemessen_mit.is_some_and(|alt| (alt - neu).abs() > f64::EPSILON) && self.dialog.is_none() {
            self.dialog = Some(Dialog::Gnss(Box::new(GnssDialog::Stab { nr, neu })));
        }
    }

    /// Streuung der Mittelung über den Grenzen der Messvorgaben?
    pub(super) fn gnss_streuung_zu_gross(m: &Messwert) -> bool {
        m.epochen > 1 && (m.s_lage_mm() > m.grenze_lage_mm || m.s_h_mm > m.grenze_hoehe_mm)
    }

    /// Zeichnet eine GNSS-Rückfrage; liefert sie zurück, solange sie offen bleibt.
    pub(super) fn zeichne_gnss_dialog(&mut self, ctx: &egui::Context, projekt: &mut Projekt, d: GnssDialog) -> Option<GnssDialog> {
        match d {
            GnssDialog::Stab { nr, neu } => self.dialog_stab(ctx, projekt, nr, neu),
            GnssDialog::Streuung { zweck, m } => self.dialog_streuung(ctx, projekt, zweck, m),
            GnssDialog::Zweitbelegung { nr, code, m } => self.dialog_zweitbelegung(ctx, projekt, nr, code, m),
            GnssDialog::Hoehenbezug { im_projekt, aktuell, linie } => self.dialog_hoehenbezug(ctx, projekt, im_projekt, aktuell, linie),
            GnssDialog::Sitzungskontrolle => self.dialog_sitzungskontrolle(ctx, projekt),
        }
    }

    fn dialog_stab(&mut self, ctx: &egui::Context, projekt: &Projekt, nr: String, neu: f64) -> Option<GnssDialog> {
        let Some(alt) = self.gnss.punkte.iter().find(|p| p.nr == nr).map(|p| p.stab_m) else {
            return None;
        };
        let mut antwort = None;
        fenster("Stabhöhe geändert").show(ctx, |ui| {
            ui.label(format!(
                "Die Stabhöhe wurde von {} m auf {} m geändert.\n\nLetzte Messung {nr} (gemessen mit {} m) mit der neuen Stabhöhe korrigieren?\nIhre Höhe ändert sich um {} m.",
                f(alt, 3),
                f(neu, 3),
                f(alt, 3),
                f(alt - neu, 3)
            ));
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if gross(ui, "Letzte Messung korrigieren", true) {
                    antwort = Some(true);
                }
                if gross(ui, "Nur künftige Messungen", false) {
                    antwort = Some(false);
                }
            });
        });
        match antwort {
            None => Some(GnssDialog::Stab { nr, neu }),
            Some(true) => {
                self.gnss_stab_korrigieren(projekt, &nr, neu);
                None
            }
            Some(false) => {
                self.gnss.letzte = None;
                None
            }
        }
    }

    /// Korrigiert die Höhen des Punkts `nr` auf die neue Stabhöhe (Lage bleibt).
    fn gnss_stab_korrigieren(&mut self, projekt: &Projekt, nr: &str, neu: f64) {
        let Some(p) = self.gnss.punkte.iter_mut().find(|p| p.nr == nr) else {
            return;
        };
        let alt = p.stab_m;
        // Boden = Antenne − Stab: ein längerer Stab ergibt einen tieferen Bodenpunkt
        let d = alt - neu;
        let hoehe_alt = p.hoehe;
        p.hoehe = p.hoehe.map(|h| h + d);
        p.h_ell += d;
        p.h_bezug = p.h_bezug.map(|h| h + d);
        p.stab_m = neu;
        let hoehe_neu = p.hoehe;
        let text = |h: Option<f64>| h.map_or_else(String::new, |h| f(h, 4));
        messprotokoll::protokolliere(
            &projekt.einstellungen,
            Eintrag::neu(Ereignis::Korrektur)
                .punkt(nr, TYP_GNSS, "")
                .aenderung("Hoehe", &text(hoehe_alt), &text(hoehe_neu))
                .quelle(messprotokoll::QUELLE_HAND)
                .text(format!("Stabhöhe nachträglich von {} m auf {} m korrigiert", f(alt, 3), f(neu, 3))),
        );
        self.gnss.letzte = None;
        self.status = match gp::schreibe(&gp::pfad(&projekt.einstellungen), &self.gnss.punkte) {
            Ok(()) => format!("{nr}: Höhe mit Stabhöhe {} m korrigiert ({} m).", f(neu, 3), f(d, 3)),
            Err(e) => format!("GNSS-Punkte nicht gespeichert: {e}"),
        };
    }

    fn dialog_streuung(&mut self, ctx: &egui::Context, projekt: &mut Projekt, zweck: Zweck, m: Box<Messwert>) -> Option<GnssDialog> {
        let mut antwort = None;
        fenster("Streuung zu groß").show(ctx, |ui| {
            ui.colored_label(FARBE_WARNUNG, RichText::new("⚠ Die Streuung der Mittelung überschreitet die Grenzen.").strong());
            ui.add_space(4.0);
            egui::Grid::new("gnss_streuung").spacing([16.0, 3.0]).show(ui, |ui| {
                let zeile = |ui: &mut egui::Ui, a: &str, b: String| {
                    ui.label(a);
                    ui.monospace(b);
                    ui.end_row();
                };
                zeile(ui, "R / H", format!("{} / {}", f(m.pos.r, 3), f(m.pos.h, 3)));
                zeile(ui, "Höhe", m.pos.hoehe.map_or("–".into(), |h| f(h, 3)));
                let farbe = |w: f64, g: f64| if w > g { FARBE_SCHLECHT } else { FARBE_GUT };
                ui.label("Streuung Lage");
                ui.colored_label(farbe(m.s_lage_mm(), m.grenze_lage_mm), format!("{} mm (Grenze {} mm)", f(m.s_lage_mm(), 1), f(m.grenze_lage_mm, 0)));
                ui.end_row();
                ui.label("Streuung Höhe");
                ui.colored_label(farbe(m.s_h_mm, m.grenze_hoehe_mm), format!("{} mm (Grenze {} mm)", f(m.s_h_mm, 1), f(m.grenze_hoehe_mm, 0)));
                ui.end_row();
                zeile(ui, "Epochen / verworfen", format!("{} / {}", m.epochen, m.verworfen));
                zeile(ui, "Dauer", format!("{} s", f(m.dauer_s, 0)));
            });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if gross(ui, "Wiederholen", true) {
                    antwort = Some(1);
                }
                if gross(ui, "Trotzdem übernehmen", false) {
                    antwort = Some(2);
                }
                if gross(ui, "Verwerfen", false) {
                    antwort = Some(3);
                }
            });
        });
        match antwort {
            None => Some(GnssDialog::Streuung { zweck, m }),
            Some(1) => {
                self.gnss_wiederholen();
                None
            }
            Some(2) => {
                self.gnss_verteile(projekt, zweck, *m);
                None
            }
            _ => {
                messprotokoll::protokolliere(
                    &projekt.einstellungen,
                    Eintrag::neu(Ereignis::Verworfen)
                        .punkt(&zweck.nummer(), TYP_GNSS, "")
                        .mit_gnss(&m.werte)
                        .quelle(messprotokoll::QUELLE_HAND)
                        .text(format!("Streuung zu groß (sL {} mm, sH {} mm)", f(m.s_lage_mm(), 1), f(m.s_h_mm, 1))),
                );
                self.status = "GNSS-Messung verworfen.".into();
                None
            }
        }
    }

    fn dialog_zweitbelegung(&mut self, ctx: &egui::Context, projekt: &mut Projekt, nr: String, code: String, m: Box<Messwert>) -> Option<GnssDialog> {
        let Some(alt) = self.gnss.punkte.iter().find(|p| p.nr.eq_ignore_ascii_case(&nr)).cloned() else {
            self.gnss_neupunkt(projekt, &nr, &code, &m);
            return None;
        };
        let g = projekt.rechenparameter.params.gnss.clone();
        let d_lage = (m.e - alt.e_utm).hypot(m.n - alt.n_utm) * MM_PRO_M;
        let d_hoehe = (roh_hoehe(&m) - alt.h_bezug.unwrap_or(alt.h_ell)) * MM_PRO_M;
        let jetzt = messprotokoll::zeit_text(Local::now().naive_local());
        let abstand_min = minuten_zwischen(&alt.zeit, &jetzt);
        let ok = d_lage <= g.zweitbelegung_lage_mm && d_hoehe.abs() <= g.zweitbelegung_hoehe_mm;
        let mut antwort = None;
        fenster(&format!("Punkt {nr} ist bereits gemessen")).show(ctx, |ui| {
            ui.label(format!("{nr} wurde bereits {}× gemessen ({}). Vergleich mit der neuen Messung:", alt.belegungen.max(1), alt.zeit));
            ui.add_space(4.0);
            let farbe = |w: f64, g: f64| if w.abs() > g { FARBE_SCHLECHT } else { FARBE_GUT };
            ui.colored_label(farbe(d_lage, g.zweitbelegung_lage_mm), format!("Differenz Lage {} mm (Grenze {} mm)", f(d_lage, 0), f(g.zweitbelegung_lage_mm, 0)));
            ui.colored_label(farbe(d_hoehe, g.zweitbelegung_hoehe_mm), format!("Differenz Höhe {} mm (Grenze {} mm)", f(d_hoehe, 0), f(g.zweitbelegung_hoehe_mm, 0)));
            match abstand_min {
                Some(min) if min < g.zweitbelegung_min => {
                    ui.colored_label(FARBE_WARNUNG, format!("⚠ Nur {} min seit der ersten Belegung (Vorgabe {} min) – nicht unabhängig.", f(min, 0), f(g.zweitbelegung_min, 0)));
                }
                Some(min) => {
                    ui.label(format!("Abstand zur ersten Belegung: {} min", f(min, 0)));
                }
                None => {}
            }
            if !ok {
                ui.colored_label(FARBE_SCHLECHT, "Die Belegungen passen nicht zusammen – möglicherweise eine falsche Fix-Lösung. Neu initialisieren (Antenne kurz abdecken) und erneut messen.");
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if gross(ui, "Mitteln (2. Belegung)", ok) {
                    antwort = Some(1);
                }
                if gross(ui, "Ersetzen", false) {
                    antwort = Some(2);
                }
                if gross(ui, "Verwerfen", !ok) {
                    antwort = Some(3);
                }
            });
        });
        match antwort {
            None => Some(GnssDialog::Zweitbelegung { nr, code, m }),
            Some(1) => {
                self.gnss_zweitbelegung_mitteln(projekt, &alt, &m, d_lage, d_hoehe, abstand_min);
                None
            }
            Some(2) => {
                self.gnss_neupunkt(projekt, &nr, &code, &m);
                None
            }
            _ => {
                messprotokoll::protokolliere(
                    &projekt.einstellungen,
                    Eintrag::neu(Ereignis::Verworfen)
                        .punkt(&nr, TYP_GNSS, &code)
                        .mit_gnss(&m.werte)
                        .quelle(messprotokoll::QUELLE_HAND)
                        .text(format!("weitere Belegung verworfen: ΔL {} mm, ΔH {} mm", f(d_lage, 0), f(d_hoehe, 0))),
                );
                self.status = format!("Weitere Belegung von {nr} verworfen.");
                None
            }
        }
    }

    /// Mittelt eine weitere Belegung mit dem gespeicherten Punkt (Rohwerte gewichtet nach Anzahl der Belegungen).
    fn gnss_zweitbelegung_mitteln(&mut self, projekt: &Projekt, alt: &GnssPunkt, m: &Messwert, d_lage: f64, d_hoehe: f64, abstand_min: Option<f64>) {
        let g = projekt.rechenparameter.params.gnss.clone();
        let b = alt.belegungen.max(1) as f64;
        let mittel = |a: f64, n: f64| (a * b + n) / (b + 1.0);
        let e = mittel(alt.e_utm, m.e);
        let n = mittel(alt.n_utm, m.n);
        let h_ell = mittel(alt.h_ell, m.h_ell);
        let h_bezug = alt.h_bezug.zip(m.h_bezug).map(|(a, n)| mittel(a, n));
        let pos = self.gnss.plan(g.bezug, g.extrapolation_prozent, e, n, Some(h_bezug.unwrap_or(h_ell)));
        let punkt = GnssPunkt {
            r: pos.r,
            h: pos.h,
            hoehe: pos.hoehe,
            e_utm: e,
            n_utm: n,
            h_ell,
            h_bezug,
            s_r_mm: alt.s_r_mm.max(m.s_e_mm),
            s_h_mm: alt.s_h_mm.max(m.s_n_mm),
            s_hoehe_mm: alt.s_hoehe_mm.max(m.s_h_mm),
            epochen: alt.epochen + m.epochen,
            dauer_s: alt.dauer_s + m.dauer_s,
            stab_m: m.stab,
            pco_m: m.pco,
            zeit: messprotokoll::zeit_text(Local::now().naive_local()),
            zeit_gnss: m.zeit_gnss.clone(),
            belegungen: alt.belegungen.max(1) + 1,
            einpassung_id: if pos.ohne_einpassung { String::new() } else { self.gnss.einpassung_id(g.bezug) },
            ..alt.clone()
        };
        let belegungen = punkt.belegungen;
        messprotokoll::protokolliere(
            &projekt.einstellungen,
            Eintrag::neu(Ereignis::Messung)
                .punkt(&alt.nr, TYP_GNSS, &alt.code)
                .koordinaten(pos.r, pos.h, pos.hoehe.unwrap_or(f64::NAN))
                .mit_gnss(&m.werte)
                .text(format!(
                    "{belegungen}. Belegung gemittelt: ΔL {} mm, ΔH {} mm{} (Grenzen {} / {} mm)",
                    f(d_lage, 0),
                    f(d_hoehe, 0),
                    abstand_min.map_or(String::new(), |a| format!(", Abstand {} min", f(a, 0))),
                    f(g.zweitbelegung_lage_mm, 0),
                    f(g.zweitbelegung_hoehe_mm, 0)
                )),
        );
        gp::setze(&mut self.gnss.punkte, punkt);
        self.gnss.letzte = Some(alt.nr.clone());
        match gp::schreibe(&gp::pfad(&projekt.einstellungen), &self.gnss.punkte) {
            Ok(()) => {
                ton::bestaetigung();
                self.status = format!("{}: {belegungen} Belegungen gemittelt (ΔL {} mm, ΔH {} mm).", alt.nr, f(d_lage, 0), f(d_hoehe, 0));
            }
            Err(e) => {
                ton::fehler();
                self.status = format!("GNSS-Punkt konnte nicht gespeichert werden: {e}");
            }
        }
    }

    fn dialog_hoehenbezug(&mut self, ctx: &egui::Context, projekt: &mut Projekt, im_projekt: Option<String>, aktuell: String, linie: bool) -> Option<GnssDialog> {
        let mut antwort = None;
        fenster("GNSS – Höhenbezug").show(ctx, |ui| {
            match &im_projekt {
                Some(b) => {
                    ui.colored_label(FARBE_WARNUNG, RichText::new("⚠ Abweichender Höhenbezug").strong());
                    ui.label(format!(
                        "Die GNSS-Höhen dieses Projekts sind im Bezug „{b}“ gemessen.\nEingestellt ist jetzt „{aktuell}“.\n\nGemischte Höhen im Projekt vermeiden: den Höhenbezug im GNSS-Fenster (Konfiguration → Höhe) prüfen."
                    ));
                }
                None => {
                    ui.label(format!(
                        "Erste GNSS-Messung in diesem Projekt.\n\nHöhenbezug: „{aktuell}“\n\nAlle weiteren GNSS-Höhen des Projekts sollten diesen Bezug haben. Ändern im GNSS-Fenster (Konfiguration → Höhe)."
                    ));
                }
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let ja = if im_projekt.is_some() { "Trotzdem messen" } else { "Bezug bestätigen und messen" };
                if gross(ui, ja, im_projekt.is_none()) {
                    antwort = Some(true);
                }
                if gross(ui, "Abbrechen", im_projekt.is_some()) {
                    antwort = Some(false);
                }
            });
        });
        match antwort {
            None => Some(GnssDialog::Hoehenbezug { im_projekt, aktuell, linie }),
            Some(true) => {
                self.gnss.hoehenbezug_ok = true;
                if let Some(b) = &im_projekt {
                    messprotokoll::protokolliere(
                        &projekt.einstellungen,
                        Eintrag::neu(Ereignis::Bemerkung).quelle(messprotokoll::QUELLE_HAND).text(format!("Höhenbezug „{aktuell}“ abweichend vom Projekt („{b}“) bestätigt")),
                    );
                }
                if linie {
                    self.gnss_linie_umschalten(projekt);
                } else {
                    self.gnss_messen_geklickt(projekt);
                }
                None
            }
            Some(false) => None,
        }
    }

    fn dialog_sitzungskontrolle(&mut self, ctx: &egui::Context, projekt: &mut Projekt) -> Option<GnssDialog> {
        let mut antwort = None;
        let id = self.gnss.einpassungen.aktiv.clone().unwrap_or_default();
        fenster("Kontrolle am Sitzungsbeginn").show(ctx, |ui| {
            ui.label(format!(
                "Der Empfänger ist verbunden. Vor dem Messen einen bekannten Punkt (T-Punkt der Einpassung {id} oder Festpunkt) als Kontrolle messen?\n\nDas deckt eine falsche Einpassung oder einen falschen Stab sofort auf."
            ));
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if gross(ui, "Kontrollpunkt messen", true) {
                    antwort = Some(true);
                }
                if gross(ui, "Später", false) {
                    antwort = Some(false);
                }
            });
        });
        match antwort {
            None => Some(GnssDialog::Sitzungskontrolle),
            Some(true) => {
                self.gnss_kontrolle_beginnen(projekt);
                None
            }
            Some(false) => None,
        }
    }

    /// Öffnet das Einpassungsfenster im Modus „Kontrollpunkt messen“.
    pub(super) fn gnss_kontrolle_beginnen(&mut self, projekt: &Projekt) {
        if self.gnss.fenster.is_none() {
            self.einpassung_umschalten(projekt);
        }
        if let Some(f) = self.gnss.fenster.as_mut() {
            f.kontrolle = true;
            f.meldung = "Kontrolle: bekannten Punkt im Plan antippen, Stab darauf stellen und messen.".into();
        }
    }

    // ── Höhenanschluss ───────────────────────────────────────────────────────────────────────────────────────

    /// Höhenanschluss: Höhenfestpunkt im Plan wählen (nächster Klick), dann messen.
    pub(super) fn gnss_hoehenanschluss_waehlen(&mut self) {
        self.gnss.fenster = None;
        self.gnss.absteckung = None;
        self.gnss.anschluss_wahl = true;
        self.gnss.anschluss_soll = None;
        self.status = "Höhenanschluss: Höhenfestpunkt im Plan antippen (der Punkt braucht eine Höhe).".into();
    }

    /// Klick in den Plan während der Wahl des Höhenfestpunkts.
    pub(super) fn gnss_hoehenanschluss_punkt(&mut self, soll: Soll) {
        if soll.hoehe.is_none() {
            self.status = format!("{} hat keine Höhe – bitte einen Höhenfestpunkt wählen.", soll.nr);
            ton::fehler();
            return;
        }
        self.status = format!("Höhenanschluss an {} (Höhe {} m): Stab auf den Punkt stellen und messen.", soll.nr, soll.hoehe.map_or_else(String::new, |h| f(h, 3)));
        self.gnss.anschluss_wahl = false;
        self.gnss.anschluss_soll = Some(soll);
    }

    /// Ergebnis der Messung auf dem Höhenfestpunkt: konstante Höhenverschiebung setzen.
    pub(super) fn gnss_hoehenanschluss(&mut self, projekt: &Projekt, soll: &Soll, m: &Messwert) {
        self.gnss.anschluss_soll = None;
        let Some(soll_hoehe) = soll.hoehe else {
            return;
        };
        let dz = soll_hoehe - roh_hoehe(m);
        self.gnss.einpassungen.hoehen_anschluss = Some(HoehenAnschluss {
            nr: soll.nr.clone(),
            dz_m: dz,
            hoehenbezug: self.gnss.hoehenbezug.clone(),
            zeit: messprotokoll::zeit_text(Local::now().naive_local()),
        });
        let gespeichert = self.gnss.einpassungen.speichere(&crate::gnss::einpassung::EinpassungsDatei::pfad(&projekt.einstellungen));
        let mit_hoeheneinpassung = self.gnss.einpassungen.aktive().and_then(|e| e.ergebnis.as_ref()).is_some_and(|e| e.hoehe.is_some());
        let text = format!("Höhenanschluss an {}: Soll {} m, gemessen {} m, Verschiebung {} m", soll.nr, f(soll_hoehe, 3), f(roh_hoehe(m), 3), f(dz, 3));
        messprotokoll::protokolliere(
            &projekt.einstellungen,
            Eintrag::neu(Ereignis::Einpassung).punkt(&soll.nr, TYP_GNSS, "").koordinaten(soll.r, soll.h, soll_hoehe).mit_gnss(&m.werte).text(text.clone()),
        );
        ton::bestaetigung();
        self.status = match gespeichert {
            Err(e) => e,
            Ok(()) if mit_hoeheneinpassung => format!("{text}. Hinweis: die aktive Einpassung hat eine Höheneinpassung – sie hat Vorrang."),
            Ok(()) => format!("{text}. Gilt für alle weiteren GNSS-Höhen (GNSS-Punkte ggf. neu rechnen)."),
        };
    }

    /// Hebt den Höhenanschluss auf.
    pub(super) fn gnss_hoehenanschluss_aufheben(&mut self, projekt: &Projekt) {
        if let Some(a) = self.gnss.einpassungen.hoehen_anschluss.take() {
            let _ = self.gnss.einpassungen.speichere(&crate::gnss::einpassung::EinpassungsDatei::pfad(&projekt.einstellungen));
            messprotokoll::protokolliere(&projekt.einstellungen, Eintrag::neu(Ereignis::Einpassung).punkt(&a.nr, TYP_GNSS, "").quelle(messprotokoll::QUELLE_HAND).text("Höhenanschluss aufgehoben"));
            self.status = format!("Höhenanschluss an {} aufgehoben.", a.nr);
        }
    }

    // ── Festpunkte und Messquelle (GNSS und Tachymeter kombinieren) ──────────────────────────────────────────

    /// Übernimmt alle GNSS-Punkte (ohne Wiedergabe) als Festpunkte, z. B. als Anschluss für die Freie Stationierung.
    pub(super) fn gnss_als_festpunkte(&mut self, projekt: &mut Projekt) {
        let punkte: Vec<GnssPunkt> = self.gnss.punkte.iter().filter(|p| p.quelle != QUELLE_WIEDERGABE).cloned().collect();
        if punkte.is_empty() {
            self.status = "Keine GNSS-Punkte vorhanden.".into();
            return;
        }
        match projekt.gnss_festpunkte_uebernehmen(&punkte) {
            Ok(e) => {
                messprotokoll::protokolliere(
                    &projekt.einstellungen,
                    Eintrag::neu(Ereignis::Bemerkung).quelle(messprotokoll::QUELLE_HAND).text(format!("GNSS-Punkte als Festpunkte übernommen: {} neu, {} ersetzt", e.neu, e.ersetzt)),
                );
                self.status = format!("GNSS-Punkte als Festpunkte übernommen: {} neu, {} ersetzt. Sie stehen jetzt als Anschlusspunkte zur Verfügung.", e.neu, e.ersetzt);
            }
            Err(e) => self.status = format!("Festpunkte nicht gespeichert: {e}"),
        }
    }

    /// Wechselt die Messquelle zwischen GNSS und dem zuletzt verwendeten Tachymeter, ohne die GNSS-Sitzung zu beenden.
    pub(super) fn wechsle_messquelle(&mut self, projekt: &mut Projekt, geraet: &mut Geraet) {
        let e = &mut projekt.einstellungen;
        if e.tachymeter_modell.ist_gnss() {
            e.tachymeter_modell = e.letztes_tachymeter_modell;
        } else {
            e.letztes_tachymeter_modell = e.tachymeter_modell;
            e.tachymeter_modell = TachymeterModell::GnssSapos;
        }
        let modell = e.tachymeter_modell;
        if let Err(fehler) = e.speichern() {
            self.status = format!("Einstellungen nicht gespeichert: {fehler}");
        }
        if !geraet.ist_verbunden() {
            *geraet = Geraet::aus_einstellungen(&projekt.einstellungen);
        }
        messprotokoll::protokolliere(&projekt.einstellungen, Eintrag::neu(Ereignis::Geraet).quelle(messprotokoll::QUELLE_HAND).text(format!("Messquelle gewechselt: {}", modell.name())));
        self.status = if modell.ist_gnss() {
            "Messquelle: GNSS. Die Tachymeterverbindung bleibt bestehen.".into()
        } else {
            format!("Messquelle: Tachymeter ({}). Die GNSS-Sitzung läuft weiter.{}", modell.name(), if geraet.ist_verbunden() { "" } else { " Tachymeter über die Messgerät-Kommunikation verbinden." })
        };
    }

    // ── Exzentrum ────────────────────────────────────────────────────────────────────────────────────────────

    /// Öffnet oder schließt das Fenster „Exzentrum“.
    pub(super) fn gnss_exzentrum_umschalten(&mut self) {
        self.gnss.exzentrum = match self.gnss.exzentrum.take() {
            Some(_) => None,
            None => Some(ExzentrumFenster {
                laengs: "0".into(),
                quer: "0".into(),
                rechts: true,
                nr: self.txt_punkt_nr.trim().to_string(),
                code: self.txt_code.trim().to_string(),
                meldung: "Hilfspunkte mit GNSS messen, Nummern eintragen, Maße eingeben.".into(),
                ..Default::default()
            }),
        };
    }

    fn zeige_exzentrum(&mut self, ctx: &egui::Context, projekt: &Projekt) {
        let Some(mut fe) = self.gnss.exzentrum.take() else {
            return;
        };
        let mut offen = true;
        let mut rechnen = false;
        let nummern: Vec<String> = self.gnss.punkte.iter().filter(|p| p.quelle != QUELLE_EXZENTRUM).map(|p| p.nr.clone()).collect();
        egui::Window::new("Exzentrum (Abschattung)").open(&mut offen).default_width(420.0).default_pos(ctx.content_rect().left_top() + FENSTER_VERSATZ).show(ctx, |ui| {
            egui::ComboBox::from_id_salt("gnss_exz_methode").selected_text(fe.methode.anzeige()).show_ui(ui, |ui| {
                for m in ExzMethode::ALLE {
                    ui.selectable_value(&mut fe.methode, m, m.anzeige());
                }
            });
            ui.add_space(4.0);
            egui::Grid::new("gnss_exz").num_columns(2).spacing([10.0, 4.0]).show(ui, |ui| {
                let punktfeld = |ui: &mut egui::Ui, name: &str, wert: &mut String| {
                    ui.label(name);
                    ui.horizontal(|ui| {
                        ui.add(egui::TextEdit::singleline(wert).desired_width(90.0));
                        egui::ComboBox::from_id_salt(format!("gnss_exz_{name}")).selected_text("…").width(40.0).show_ui(ui, |ui| {
                            for nr in &nummern {
                                if ui.selectable_label(nr == wert, nr).clicked() {
                                    wert.clone_from(nr);
                                }
                            }
                        });
                    });
                    ui.end_row();
                };
                punktfeld(ui, "Hilfspunkt A", &mut fe.a);
                punktfeld(ui, "Hilfspunkt B", &mut fe.b);
                let zahlfeld = |ui: &mut egui::Ui, name: &str, wert: &mut String| {
                    ui.label(name);
                    ui.add(egui::TextEdit::singleline(wert).desired_width(90.0));
                    ui.end_row();
                };
                match fe.methode {
                    ExzMethode::LaengsQuer => {
                        zahlfeld(ui, "Längs hinter B [m]", &mut fe.laengs);
                        zahlfeld(ui, "Quer, + rechts [m]", &mut fe.quer);
                    }
                    ExzMethode::Bogenschnitt => {
                        zahlfeld(ui, "Strecke von A [m]", &mut fe.s_a);
                        zahlfeld(ui, "Strecke von B [m]", &mut fe.s_b);
                        ui.label("Lösung");
                        ui.horizontal(|ui| {
                            ui.radio_value(&mut fe.rechts, true, "rechts von A→B");
                            ui.radio_value(&mut fe.rechts, false, "links");
                        });
                        ui.end_row();
                    }
                    ExzMethode::Geradenschnitt => {
                        punktfeld(ui, "Hilfspunkt C", &mut fe.c);
                        punktfeld(ui, "Hilfspunkt D", &mut fe.d);
                    }
                }
                zahlfeld(ui, "Neue Punktnummer", &mut fe.nr);
                zahlfeld(ui, "Code", &mut fe.code);
            });
            ui.add_space(6.0);
            if gross(ui, "Berechnen und speichern", true) {
                rechnen = true;
            }
            ui.label(&fe.meldung);
        });
        if rechnen {
            fe.meldung = match self.gnss_exzentrum_rechnen(projekt, &fe) {
                Ok(text) => text,
                Err(text) => {
                    ton::fehler();
                    text
                }
            };
        }
        if offen {
            self.gnss.exzentrum = Some(fe);
        }
    }

    /// Berechnet den Exzenterpunkt in Plan- und UTM-Koordinaten und speichert ihn als GNSS-Punkt.
    fn gnss_exzentrum_rechnen(&mut self, projekt: &Projekt, fe: &ExzentrumFenster) -> Result<String, String> {
        let hole = |nr: &str| -> Result<GnssPunkt, String> {
            self.gnss.punkte.iter().find(|p| p.nr.eq_ignore_ascii_case(nr.trim())).cloned().ok_or_else(|| format!("GNSS-Punkt „{}“ nicht gefunden.", nr.trim()))
        };
        let zahl = |t: &str, name: &str| parse_zahl(t).filter(|w| w.is_finite()).ok_or_else(|| format!("{name}: ungültige Zahl."));
        let nr = fe.nr.trim().to_string();
        if nr.is_empty() {
            return Err("Neue Punktnummer fehlt.".into());
        }
        let (a, b) = (hole(&fe.a)?, hole(&fe.b)?);
        let plan = |p: &GnssPunkt| (p.r, p.h);
        let utm = |p: &GnssPunkt| (p.e_utm, p.n_utm);
        let hoehe_roh = |p: &GnssPunkt| p.h_bezug.unwrap_or(p.h_ell);
        let mittel = |werte: &[Option<f64>]| -> Option<f64> {
            let w: Option<Vec<f64>> = werte.iter().copied().collect();
            w.map(|w| w.iter().sum::<f64>() / w.len() as f64)
        };
        let (p_plan, p_utm, hoehe, roh, beschreibung, hilfs) = match fe.methode {
            ExzMethode::LaengsQuer => {
                let (l, q) = (zahl(&fe.laengs, "Längs")?, zahl(&fe.quer, "Quer")?);
                let pp = laengs_quer(plan(&a), plan(&b), l, q).ok_or("A und B fallen zusammen.")?;
                let pu = laengs_quer(utm(&a), utm(&b), l, q).ok_or("A und B fallen zusammen.")?;
                // Höhe linear entlang A→B fortgesetzt
                let s = (b.e_utm - a.e_utm).hypot(b.n_utm - a.n_utm);
                let t = (s + l) / s;
                let hoehe = a.hoehe.zip(b.hoehe).map(|(ha, hb)| ha + t * (hb - ha));
                let roh = hoehe_roh(&a) + t * (hoehe_roh(&b) - hoehe_roh(&a));
                (pp, pu, hoehe, roh, format!("Längs {} m hinter {} (von {}), quer {} m", f(l, 3), b.nr, a.nr, f(q, 3)), vec![a.clone(), b.clone()])
            }
            ExzMethode::Bogenschnitt => {
                let (sa, sb) = (zahl(&fe.s_a, "Strecke von A")?, zahl(&fe.s_b, "Strecke von B")?);
                let fehler = "Die Kreise schneiden sich nicht (Strecken prüfen).";
                let pp = bogenschnitt(plan(&a), plan(&b), sa, sb, fe.rechts).ok_or(fehler)?;
                let pu = bogenschnitt(utm(&a), utm(&b), sa, sb, fe.rechts).ok_or(fehler)?;
                let hoehe = mittel(&[a.hoehe, b.hoehe]);
                let roh = (hoehe_roh(&a) + hoehe_roh(&b)) / 2.0;
                (pp, pu, hoehe, roh, format!("Bogenschnitt {} m von {}, {} m von {} ({})", f(sa, 3), a.nr, f(sb, 3), b.nr, if fe.rechts { "rechts" } else { "links" }), vec![a.clone(), b.clone()])
            }
            ExzMethode::Geradenschnitt => {
                let (c, d) = (hole(&fe.c)?, hole(&fe.d)?);
                let fehler = "Die Geraden sind parallel.";
                let pp = geradenschnitt(plan(&a), plan(&b), plan(&c), plan(&d)).ok_or(fehler)?;
                let pu = geradenschnitt(utm(&a), utm(&b), utm(&c), utm(&d)).ok_or(fehler)?;
                let hoehe = mittel(&[a.hoehe, b.hoehe, c.hoehe, d.hoehe]);
                let roh = (hoehe_roh(&a) + hoehe_roh(&b) + hoehe_roh(&c) + hoehe_roh(&d)) / 4.0;
                (pp, pu, hoehe, roh, format!("Geradenschnitt {}–{} mit {}–{}", a.nr, b.nr, c.nr, d.nr), vec![a.clone(), b.clone(), c.clone(), d.clone()])
            }
        };
        let s_max = |sel: fn(&GnssPunkt) -> f64| hilfs.iter().map(sel).fold(0.0, f64::max);
        let punkt = GnssPunkt {
            nr: nr.clone(),
            code: fe.code.trim().to_string(),
            r: p_plan.0,
            h: p_plan.1,
            hoehe,
            e_utm: p_utm.0,
            n_utm: p_utm.1,
            h_ell: roh,
            h_bezug: a.h_bezug.map(|_| roh),
            s_r_mm: s_max(|p| p.s_r_mm),
            s_h_mm: s_max(|p| p.s_h_mm),
            s_hoehe_mm: s_max(|p| p.s_hoehe_mm),
            hoehenbezug: a.hoehenbezug.clone(),
            einpassung_id: a.einpassung_id.clone(),
            zeit: messprotokoll::zeit_text(Local::now().naive_local()),
            quelle: QUELLE_EXZENTRUM.into(),
            ..Default::default()
        };
        messprotokoll::protokolliere(
            &projekt.einstellungen,
            Eintrag::neu(Ereignis::Messung)
                .punkt(&nr, TYP_GNSS_EXZENTRUM, &punkt.code)
                .koordinaten(punkt.r, punkt.h, punkt.hoehe.unwrap_or(f64::NAN))
                .quelle(messprotokoll::QUELLE_PROGRAMM)
                .text(format!("Exzentrum: {beschreibung}")),
        );
        let ersetzt = gp::setze(&mut self.gnss.punkte, punkt);
        gp::schreibe(&gp::pfad(&projekt.einstellungen), &self.gnss.punkte).map_err(|e| format!("GNSS-Punkt nicht gespeichert: {e}"))?;
        ton::bestaetigung();
        Ok(format!("{nr}{}: R={}  H={}{}  ({beschreibung})", if ersetzt { " (ersetzt)" } else { "" }, f(p_plan.0, 3), f(p_plan.1, 3), hoehe.map_or(String::new(), |h| format!("  Höhe={}", f(h, 3)))))
    }

    // ── Satellitenvorhersage ─────────────────────────────────────────────────────────────────────────────────

    pub(super) fn gnss_vorhersage_umschalten(&mut self) {
        self.gnss.vorhersage = match self.gnss.vorhersage.take() {
            Some(_) => None,
            None => Some(VorhersageFenster { anfordern: true, ..Default::default() }),
        };
    }

    fn gnss_vorhersage_rechnen(&mut self, app: &App, projekt: &Projekt) {
        let maske = projekt.rechenparameter.params.gnss.vorhersage_maske_grad;
        let live = self.gnss.live.clone();
        let Some(v) = self.gnss.vorhersage.as_mut() else {
            return;
        };
        v.anfordern = false;
        let Some(l) = live else {
            v.meldung = "Keine Position – Empfänger verbinden und auf eine Lösung warten.".into();
            return;
        };
        let bahndaten = app.gps_bahndaten();
        v.satelliten_mit_bahn = bahndaten.len();
        if bahndaten.is_empty() {
            v.meldung = "Noch keine GPS-Bahndaten empfangen (sie kommen alle 30 s, vollständig nach einigen Minuten).".into();
            v.ergebnis.clear();
            return;
        }
        let ort = Geographic { lat: l.lat, lon: l.lon, h: l.h.unwrap_or(0.0) };
        v.ergebnis = vorhersage::berechne(&bahndaten, &ort, f64::from(l.itow) / MS_PRO_S, VORHERSAGE_DAUER_MIN, VORHERSAGE_SCHRITT_MIN, maske);
        v.berechnet = Local::now().format("%H:%M").to_string();
        v.meldung = format!("GPS-Satelliten mit Bahndaten: {}. Elevationsmaske {}°.", v.satelliten_mit_bahn, f(maske, 0));
    }

    fn zeige_vorhersage(&mut self, ctx: &egui::Context) {
        let Some(mut v) = self.gnss.vorhersage.take() else {
            return;
        };
        let mut offen = true;
        egui::Window::new("Satellitenvorhersage (GPS)").open(&mut offen).default_width(VORHERSAGE_GRAFIK.x + 20.0).default_pos(ctx.content_rect().left_top() + FENSTER_VERSATZ).show(ctx, |ui| {
            ui.label("PDOP und sichtbare GPS-Satelliten der nächsten 6 Stunden am aktuellen Ort. Der Empfänger nutzt zusätzlich Galileo, GLONASS und BeiDou – die Vorhersage zeigt vor allem, wann die Geometrie schwach ist (enge Lagen dann meiden).");
            ui.add_space(4.0);
            let (rect, _) = ui.allocate_exact_size(VORHERSAGE_GRAFIK, Sense::hover());
            let maler = ui.painter_at(rect);
            maler.rect_filled(rect, 4.0, Color32::from_rgb(24, 28, 36));
            let n = v.ergebnis.len().max(1) as f32;
            let breite = rect.width() / n;
            let y = |pdop: f64| rect.bottom() - (pdop.min(VORHERSAGE_PDOP_MAX) / VORHERSAGE_PDOP_MAX) as f32 * rect.height();
            for grenze in [PDOP_SCHWACH, PDOP_SCHLECHT] {
                maler.line_segment([Pos2::new(rect.left(), y(grenze)), Pos2::new(rect.right(), y(grenze))], Stroke::new(1.0, Color32::from_gray(90)));
                maler.text(Pos2::new(rect.left() + 2.0, y(grenze) - 2.0), Align2::LEFT_BOTTOM, format!("PDOP {}", f(grenze, 0)), FontId::proportional(10.0), Color32::from_gray(160));
            }
            for (i, z) in v.ergebnis.iter().enumerate() {
                let x = rect.left() + i as f32 * breite;
                let (oben, farbe) = match z.pdop {
                    Some(p) if p <= PDOP_SCHWACH => (y(p), FARBE_GUT),
                    Some(p) if p <= PDOP_SCHLECHT => (y(p), FARBE_WARNUNG),
                    Some(p) => (y(p), FARBE_SCHLECHT),
                    None => (rect.top(), FARBE_SCHLECHT),
                };
                maler.rect_filled(egui::Rect::from_min_max(Pos2::new(x + 1.0, oben), Pos2::new(x + breite - 1.0, rect.bottom())), 1.0, farbe);
                if (z.minuten % S_PRO_MIN).abs() < f64::EPSILON {
                    maler.text(Pos2::new(x, rect.bottom() - 2.0), Align2::LEFT_BOTTOM, format!("+{} h", f(z.minuten / S_PRO_MIN, 0)), FontId::proportional(10.0), Color32::WHITE);
                }
            }
            if let Some(schlecht) = v.ergebnis.iter().find(|z| z.pdop.is_none_or(|p| p > PDOP_SCHWACH)) {
                let start = Local::now() + chrono::Duration::minutes(schlecht.minuten as i64);
                ui.colored_label(FARBE_WARNUNG, format!("Erste schwache GPS-Geometrie gegen {} Uhr ({} Satelliten).", start.format("%H:%M"), schlecht.satelliten));
            } else if !v.ergebnis.is_empty() {
                ui.colored_label(FARBE_GUT, "Durchgehend gute GPS-Geometrie.");
            }
            ui.label(&v.meldung);
            ui.horizontal(|ui| {
                if ui.button("Neu berechnen").clicked() {
                    v.anfordern = true;
                }
                if !v.berechnet.is_empty() {
                    ui.label(format!("berechnet {} Uhr", v.berechnet));
                }
            });
        });
        if offen {
            self.gnss.vorhersage = Some(v);
        }
    }

    /// Zeichnet die Fenster „Exzentrum“ und „Satellitenvorhersage“.
    pub(super) fn zeige_gnss_extras(&mut self, ctx: &egui::Context, projekt: &Projekt) {
        self.zeige_exzentrum(ctx, projekt);
        self.zeige_vorhersage(ctx);
    }

    // ── Zeichnen und Anzeige ─────────────────────────────────────────────────────────────────────────────────

    /// Linie der Linienaufnahme und Höhenfestpunkt im Plan.
    pub(super) fn zeichne_gnss_extras(&self, maler: &egui::Painter, a: &crate::dxf::entity::Ansicht) {
        if let Some(l) = &self.gnss.linie {
            let punkte: Vec<Pos2> = l.nummern.iter().filter_map(|nr| self.gnss.punkte.iter().find(|p| &p.nr == nr)).map(|p| a.zu_bildschirm(p.r, p.h)).collect();
            if punkte.len() > 1 {
                maler.add(egui::Shape::line(punkte, Stroke::new(2.0, fix_farbe(Some(Fix::Fixed), false))));
            }
        }
        if let Some(s) = &self.gnss.anschluss_soll {
            let q = a.zu_bildschirm(s.r, s.h);
            maler.circle_stroke(q, 11.0, Stroke::new(2.0, Color32::from_rgb(120, 200, 255)));
            maler.text(q + Vec2::new(12.0, -12.0), Align2::LEFT_BOTTOM, format!("Höhenanschluss {}", s.nr), FontId::proportional(11.0), Color32::from_rgb(120, 200, 255));
        }
    }

    /// Zusätzliche Zeilen der Statusanzeige.
    pub(super) fn gnss_extras_info(&self) -> Vec<(String, Color32)> {
        let mut zeilen = Vec::new();
        if let Some((pfad, bytes)) = &self.gnss.aufzeichnung {
            let name = pfad.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned());
            zeilen.push((format!("● UBX-Aufzeichnung {name} ({} MB)", f(*bytes as f64 / BYTE_PRO_MB, 1)), Color32::from_rgb(255, 110, 110)));
        }
        if let Some(a) = &self.gnss.einpassungen.hoehen_anschluss {
            zeilen.push((format!("Höhenanschluss {}: {} m", a.nr, f(a.dz_m, 3)), Color32::from_rgb(120, 200, 255)));
        }
        if let Some(l) = &self.gnss.linie {
            zeilen.push((format!("Linienaufnahme: {} Punkte", l.nummern.len()), fix_farbe(Some(Fix::Fixed), false)));
        }
        zeilen
    }

    /// Einträge des Ausklappmenüs „Extras“ der GNSS-Werkzeugleiste.
    pub(super) fn gnss_extras_menue(&mut self, ui: &mut egui::Ui, projekt: &mut Projekt) {
        let knopf = |ui: &mut egui::Ui, text: &str, tip: &str| ui.add(egui::Button::new(RichText::new(text).size(14.0)).min_size(Vec2::new(280.0, 36.0))).on_hover_text(tip).clicked();
        if knopf(ui, "Kontrollpunkt messen", "Bekannten Punkt messen und mit der Einpassung vergleichen") {
            self.gnss_kontrolle_beginnen(projekt);
        }
        if knopf(ui, "Exzentrum berechnen …", "Punkt, den GNSS nicht erreicht: aus GNSS-Hilfspunkten und Bandmaß") {
            self.gnss_exzentrum_umschalten();
        }
        if knopf(ui, "Höhenanschluss messen …", "Höhenfestpunkt im Plan wählen und messen; die Differenz gilt für alle GNSS-Höhen") {
            self.gnss_hoehenanschluss_waehlen();
        }
        if self.gnss.einpassungen.hoehen_anschluss.is_some() && knopf(ui, "Höhenanschluss aufheben", "Konstante Höhenverschiebung entfernen") {
            self.gnss_hoehenanschluss_aufheben(projekt);
        }
        if knopf(ui, "Satellitenvorhersage …", "PDOP und GPS-Satelliten der nächsten Stunden am Messort") {
            self.gnss_vorhersage_umschalten();
        }
        if knopf(ui, "GNSS-Punkte als Festpunkte übernehmen", "Für die Freie Stationierung mit dem Tachymeter (Anschlusspunkte)") {
            self.gnss_als_festpunkte(projekt);
        }
        let tachymeter = projekt.einstellungen.letztes_tachymeter_modell;
        if knopf(ui, &format!("Messquelle: Tachymeter ({})", tachymeter.name()), "Auf das Tachymeter umschalten; die GNSS-Sitzung läuft weiter") {
            self.messquelle_wechseln = true;
        }
    }

    /// Wiederholt die letzte Messung mit gleichem Zweck und gleichen Vorgaben.
    pub(super) fn gnss_wiederholen(&mut self) {
        if let Some((zweck, nr, code, kriterien)) = self.gnss.wiederholung.clone() {
            self.status = format!("GNSS-Messung {nr} wird wiederholt …");
            self.gnss_starte(zweck, nr, code, kriterien);
        }
    }

    /// Gibt eine Messung an die Sitzung (nach dem Zeichnen) und merkt sie für „Wiederholen“.
    pub(super) fn gnss_starte(&mut self, zweck: Zweck, nr: String, code: String, kriterien: Criteria) {
        self.gnss.wiederholung = Some((zweck.clone(), nr.clone(), code.clone(), kriterien.clone()));
        self.gnss.auftraege.push(Auftrag::Messen { nr, code, kriterien });
        self.gnss.laufend = Some(zweck);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nah(a: Option<(f64, f64)>, b: (f64, f64)) -> bool {
        a.is_some_and(|a| (a.0 - b.0).abs() < 1e-9 && (a.1 - b.1).abs() < 1e-9)
    }

    #[test]
    fn exzentrum_laengs_quer() {
        // A→B nach Norden: rechts ist Osten
        assert!(nah(laengs_quer((0.0, 0.0), (0.0, 10.0), 1.25, 0.30), (0.30, 11.25)));
        // A→B nach Osten: rechts ist Süden
        assert!(nah(laengs_quer((0.0, 0.0), (10.0, 0.0), 0.0, 2.0), (10.0, -2.0)));
        assert!(laengs_quer((1.0, 1.0), (1.0, 1.0), 1.0, 1.0).is_none());
    }

    #[test]
    fn exzentrum_bogenschnitt() {
        // gleichseitig: A(0,0), B(0,10) nach Norden, rechts = Osten
        let p = bogenschnitt((0.0, 0.0), (0.0, 10.0), 10.0, 10.0, true).unwrap();
        assert!((p.0 - 75.0_f64.sqrt()).abs() < 1e-9 && (p.1 - 5.0).abs() < 1e-9);
        let p = bogenschnitt((0.0, 0.0), (0.0, 10.0), 10.0, 10.0, false).unwrap();
        assert!(p.0 < 0.0);
        assert!(bogenschnitt((0.0, 0.0), (0.0, 10.0), 2.0, 2.0, true).is_none());
    }

    #[test]
    fn exzentrum_geradenschnitt() {
        assert!(nah(geradenschnitt((0.0, 0.0), (10.0, 10.0), (0.0, 10.0), (10.0, 0.0)), (5.0, 5.0)));
        assert!(geradenschnitt((0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)).is_none());
    }

    #[test]
    fn minuten_aus_zeitstempeln() {
        assert_eq!(minuten_zwischen("2026-11-27T10:00:00", "2026-11-27T10:12:30"), Some(12.5));
        assert_eq!(minuten_zwischen("", "2026-11-27T10:00:00"), None);
    }
}
