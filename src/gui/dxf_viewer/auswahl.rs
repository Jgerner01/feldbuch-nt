// Zweck:    Auswahlmodus des DXF-Viewers für die Konvertierung: eigene Viewer-Instanz, in der Tabellenpunkte per
//           Klick, Rechteck oder frei gezeichnetem (automatisch geschlossenem) Polygon gewählt werden. Messdaten
//           ohne Koordinaten erscheinen hilfsweise koordiniert (Standpunkt 0/0, t = 0). Nur grafische Funktionen.
// Hardware: keine
// Autor:    Johann Gerner

use super::*;
use crate::daten::konvertierung::{AuswahlAnfrage, AuswahlErgebnis, AuswahlKandidat, punkt_im_polygon};

/// Farben der Auswahlpunkte.
const FARBE_PUNKT: Color32 = Color32::from_rgb(30, 90, 200);
const FARBE_HILFSPUNKT: Color32 = Color32::from_rgb(140, 50, 170);
const FARBE_GEWAEHLT: Color32 = Color32::from_rgb(220, 60, 0);
const FARBE_POLYGON: Color32 = Color32::from_rgb(0, 140, 90);
/// Leiste der Auswahlwerkzeuge.
pub(super) const LEISTE_FARBE: Color32 = Color32::from_rgb(235, 238, 245);
/// Radius der Punktsymbole [px].
const PUNKT_RADIUS: f32 = 4.5;
/// Kleinste Ausdehnung beim Einpassen [m] (einzelner Punkt).
const EINPASSEN_MIN_M: f64 = 20.0;

/// Werkzeug der Zeichenfläche.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Werkzeug {
    /// Ziehen verschiebt, Klick schaltet den nächsten Punkt um.
    Verschieben,
    /// Ziehen zieht ein Rechteck auf.
    Rechteck,
    /// Klicks setzen Ecken; Doppelklick, Enter oder „Polygon schließen“ wendet es an.
    Polygon,
}

/// Zustand des Auswahlmodus.
pub(super) struct AuswahlModus {
    kandidaten: Vec<AuswahlKandidat>,
    gewaehlt: Vec<bool>,
    ohne_lage: usize,
    tabellen_version: u64,
    werkzeug: Werkzeug,
    /// Rechteck/Polygon entfernt Punkte aus der Auswahl statt sie hinzuzufügen.
    entfernen: bool,
    /// Ecken des Polygons in Weltkoordinaten.
    polygon: Vec<(f64, f64)>,
    /// Mausposition in Weltkoordinaten (Gummiband des Polygons).
    maus: Option<(f64, f64)>,
    /// Rückfrage beim Schließen ist offen.
    frage_offen: bool,
    ergebnis: Option<bool>,
}

impl AuswahlModus {
    fn anzahl_gewaehlt(&self) -> usize {
        self.gewaehlt.iter().filter(|g| **g).count()
    }

    fn hat_hilfspunkte(&self) -> bool {
        self.kandidaten.iter().any(|k| k.hilfskoordinate)
    }

    fn hat_koordinaten(&self) -> bool {
        self.kandidaten.iter().any(|k| !k.hilfskoordinate)
    }

    /// Setzt bzw. entfernt alle Kandidaten, für die `im_bereich` gilt; liefert ihre Anzahl.
    fn wende_bereich_an(&mut self, im_bereich: impl Fn(f64, f64) -> bool) -> usize {
        let mut n = 0;
        for (k, g) in self.kandidaten.iter().zip(self.gewaehlt.iter_mut()) {
            if im_bereich(k.r, k.h) {
                *g = !self.entfernen;
                n += 1;
            }
        }
        n
    }

    /// Nächster Kandidat innerhalb von `radius` [m].
    fn naechster(&self, welt: (f64, f64), radius: f64) -> Option<usize> {
        self.kandidaten
            .iter()
            .enumerate()
            .map(|(i, k)| (i, (k.r - welt.0).hypot(k.h - welt.1)))
            .filter(|(_, d)| *d <= radius)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _)| i)
    }

    fn polygon_schliessen(&mut self) -> String {
        if self.polygon.len() < 3 {
            return "Polygon: mindestens drei Ecken setzen.".into();
        }
        let polygon = std::mem::take(&mut self.polygon);
        let n = self.wende_bereich_an(|r, h| punkt_im_polygon((r, h), &polygon));
        format!("Polygon: {n} Punkt(e) {}.", if self.entfernen { "abgewählt" } else { "gewählt" })
    }
}

impl DxfViewer {
    /// Neue, eigenständige Viewer-Instanz zum Auswählen von Tabellenpunkten. Lädt die DXF-Datei des Projekts nur
    /// zur Anzeige (kein Punkt-Index, keine gespeicherten Einstellungen) und lässt nur grafische Funktionen zu.
    pub fn fuer_auswahl(projekt: &Projekt, anfrage: AuswahlAnfrage) -> Self {
        let mut v = Self { initialisiert: true, ..Self::default() };
        v.status = format!("{} Punkte wählbar – Werkzeug wählen: Klick, Rechteck oder Polygon.", anfrage.kandidaten.len());
        if let Some(pfad) = projekt.einstellungen.dxf_pfad().filter(|_| projekt.einstellungen.hat_dxf_datei()) {
            match leser::lese(&pfad) {
                Ok(e) => {
                    v.entities = e;
                    v.dxf_pfad = pfad.to_string_lossy().into_owned();
                }
                Err(fehler) => v.status = format!("DXF-Datei konnte nicht gelesen werden: {fehler}"),
            }
        }
        v.auswahl_modus = Some(Box::new(AuswahlModus {
            gewaehlt: anfrage.vorauswahl,
            kandidaten: anfrage.kandidaten,
            ohne_lage: anfrage.ohne_lage,
            tabellen_version: anfrage.tabellen_version,
            werkzeug: Werkzeug::Verschieben,
            entfernen: false,
            polygon: Vec::new(),
            maus: None,
            frage_offen: false,
            ergebnis: None,
        }));
        v.fit_ausstehend = true;
        v
    }

    pub(super) fn ist_auswahl(&self) -> bool {
        self.auswahl_modus.is_some()
    }

    /// Ergebnis nach dem Schließen (einmalig).
    pub fn nimm_auswahl_ergebnis(&mut self) -> Option<AuswahlErgebnis> {
        let m = self.auswahl_modus.as_mut()?;
        let uebernehmen = m.ergebnis.take()?;
        Some(AuswahlErgebnis {
            tabellen_version: m.tabellen_version,
            uebernehmen,
            zeilen: m.kandidaten.iter().zip(&m.gewaehlt).map(|(k, g)| (k.zeile, *g)).collect(),
        })
    }

    /// Schließen-Wunsch des Fensters: erst nachfragen, ob die Auswahl übernommen wird.
    pub(super) fn auswahl_schliessen_angefragt(&mut self, ctx: &egui::Context) {
        if let Some(m) = &mut self.auswahl_modus {
            if m.ergebnis.is_none() {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                m.frage_offen = true;
            }
        }
    }

    /// Ist eine Entscheidung gefallen, schließt das Fenster.
    pub(super) fn auswahl_entschieden(&self) -> bool {
        self.auswahl_modus.as_ref().is_some_and(|m| m.ergebnis.is_some())
    }

    /// Einpassen auf die wählbaren Punkte (`nur_hilfspunkte`: Some(true) nur Messdaten, Some(false) nur Koordinaten).
    pub(super) fn auswahl_einpassen(&mut self, nur_hilfspunkte: Option<bool>) -> bool {
        let Some(m) = &self.auswahl_modus else {
            return false;
        };
        let punkte: Vec<(f64, f64)> =
            m.kandidaten.iter().filter(|k| nur_hilfspunkte.is_none_or(|h| k.hilfskoordinate == h)).map(|k| (k.r, k.h)).collect();
        if punkte.is_empty() {
            return false;
        }
        let (mut min_x, mut min_y, mut max_x, mut max_y) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
        for (x, y) in &punkte {
            min_x = min_x.min(*x);
            min_y = min_y.min(*y);
            max_x = max_x.max(*x);
            max_y = max_y.max(*y);
        }
        let (b, h) = ((max_x - min_x).max(EINPASSEN_MIN_M), (max_y - min_y).max(EINPASSEN_MIN_M));
        let (sb, sh) = (f64::from(self.canvas_groesse.x).max(1.0), f64::from(self.canvas_groesse.y).max(1.0));
        self.scale = (sb / b).min(sh / h) * FIT_RAND;
        self.mitte = [(min_x + max_x) / 2.0, (min_y + max_y) / 2.0];
        true
    }

    // ── Leisten ───────────────────────────────────────────────────────────────

    /// Werkzeugleiste des Auswahlmodus (unter der gesperrten Toolbar).
    pub(super) fn zeichne_auswahl_leiste(&mut self, ui: &mut egui::Ui) {
        let mut einpassen: Option<Option<bool>> = None;
        let Some(m) = self.auswahl_modus.as_mut() else {
            return;
        };
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(6.0, 4.0);
            ui.label(RichText::new("Auswahl:").strong());
            for (w, text, tip) in [
                (Werkzeug::Verschieben, "✋ Verschieben / Klick", "Ziehen verschiebt die Grafik, Klick auf einen Punkt wählt ihn an oder ab"),
                (Werkzeug::Rechteck, "▭ Rechteck", "Rechteck aufziehen: alle Punkte darin wählen (bzw. abwählen)"),
                (Werkzeug::Polygon, "⬠ Polygon", "Ecken per Klick setzen, Doppelklick oder Enter schließt das Polygon automatisch"),
            ] {
                if ui.selectable_label(m.werkzeug == w, text).on_hover_text(tip).clicked() {
                    m.werkzeug = w;
                    m.polygon.clear();
                    self.auswahl_start = None;
                    self.auswahl_ende = None;
                }
            }
            ui.separator();
            ui.radio_value(&mut m.entfernen, false, "hinzufügen");
            ui.radio_value(&mut m.entfernen, true, "entfernen");
            if m.werkzeug == Werkzeug::Polygon {
                ui.separator();
                if ui.add_enabled(m.polygon.len() >= 3, egui::Button::new("Polygon schließen")).clicked() {
                    self.status = m.polygon_schliessen();
                }
                if ui.add_enabled(!m.polygon.is_empty(), egui::Button::new("Polygon verwerfen")).clicked() {
                    m.polygon.clear();
                }
            }
            ui.separator();
            if ui.button("Alle").clicked() {
                m.gewaehlt.iter_mut().for_each(|g| *g = true);
            }
            if ui.button("Keine").clicked() {
                m.gewaehlt.iter_mut().for_each(|g| *g = false);
            }
            if m.hat_hilfspunkte() && m.hat_koordinaten() {
                ui.separator();
                if ui.button("Koordinaten einpassen").clicked() {
                    einpassen = Some(Some(false));
                }
                if ui.button("Messdaten einpassen").on_hover_text("Messdaten hilfsweise koordiniert: Standpunkt 0/0, t = 0").clicked() {
                    einpassen = Some(Some(true));
                }
            }
            ui.separator();
            ui.label(RichText::new(format!("{} von {} gewählt", m.anzahl_gewaehlt(), m.kandidaten.len())).strong());
            if ui.button(RichText::new("Übernehmen und schließen").strong()).on_hover_text("Markierungen der Konvertierungstabelle durch diese Auswahl ersetzen").clicked() {
                m.ergebnis = Some(true);
            }
            if ui.button("Verwerfen").clicked() {
                m.ergebnis = Some(false);
            }
        });
        if let Some(art) = einpassen {
            self.auswahl_einpassen(art);
        }
    }

    /// Statusleiste des Auswahlmodus: Status und Legende.
    pub(super) fn zeichne_auswahl_status(&mut self, ui: &mut egui::Ui) {
        let Some(m) = &self.auswahl_modus else {
            return;
        };
        ui.horizontal_centered(|ui| {
            ui.add_space(6.0);
            ui.label(RichText::new(&self.status).monospace().size(12.0).color(STATUS_TEXT));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.add_space(8.0);
                if m.ohne_lage > 0 {
                    ui.label(RichText::new(format!("{} Zeile(n) ohne Lage nicht wählbar", m.ohne_lage)).size(12.0).color(STATUS_TEXT));
                }
                if m.hat_hilfspunkte() {
                    ui.label(RichText::new("● Messdaten (hilfsweise: Standpunkt 0/0, t = 0)").size(12.0).color(FARBE_HILFSPUNKT));
                }
                ui.label(RichText::new("● Koordinaten").size(12.0).color(FARBE_PUNKT));
                ui.label(RichText::new("● gewählt").size(12.0).color(FARBE_GEWAEHLT));
            });
        });
    }

    // ── Zeichenfläche ─────────────────────────────────────────────────────────

    pub(super) fn hat_auswahl_kandidaten(&self) -> bool {
        self.auswahl_modus.as_ref().is_some_and(|m| !m.kandidaten.is_empty())
    }

    /// Eingaben der Zeichenfläche im Auswahlmodus.
    pub(super) fn verarbeite_auswahl_eingabe(&mut self, ui: &mut egui::Ui, antwort: &egui::Response, ursprung: Pos2) {
        if self.verarbeite_zwei_finger(ui, antwort, ursprung) {
            return;
        }
        // Mausrad: Zoom
        if antwort.hovered() {
            let scroll: f32 = ui.input(|i| i.raw.events.iter().filter_map(|e| if let egui::Event::MouseWheel { delta, .. } = e { Some(delta.y) } else { None }).sum());
            if scroll != 0.0 {
                if let Some(pos) = antwort.hover_pos() {
                    self.zoom(if scroll > 0.0 { ZOOM_RAD_FAKTOR } else { 1.0 / ZOOM_RAD_FAKTOR }, pos, ursprung);
                }
            }
        }
        let Some(mut m) = self.auswahl_modus.take() else {
            return;
        };
        let a = self.ansicht(ursprung);
        m.maus = antwort.hover_pos().map(|p| a.zu_welt(p));
        if let Some((x, y)) = m.maus {
            if ui.input(|i| i.pointer.delta() != Vec2::ZERO) && m.polygon.is_empty() {
                self.status = koordinaten_anzeige(x, y);
            }
        }
        let pan = |v: &mut Self, d: Vec2| {
            v.mitte[0] -= f64::from(d.x) / v.scale;
            v.mitte[1] += f64::from(d.y) / v.scale;
        };
        let klick_welt = antwort.interact_pointer_pos().map(|p| a.zu_welt(p));

        match m.werkzeug {
            Werkzeug::Verschieben => {
                if antwort.dragged_by(egui::PointerButton::Primary) {
                    pan(self, antwort.drag_delta());
                }
            }
            Werkzeug::Rechteck => {
                if antwort.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
                }
                if antwort.drag_started() {
                    self.auswahl_start = antwort.interact_pointer_pos();
                    self.auswahl_ende = self.auswahl_start;
                }
                if antwort.dragged() {
                    self.auswahl_ende = antwort.interact_pointer_pos().or(self.auswahl_ende);
                }
                if antwort.drag_stopped() {
                    if let (Some(s), Some(e)) = (self.auswahl_start.take(), self.auswahl_ende.take()) {
                        let r = Rect::from_two_pos(s, e);
                        if r.width() > MIN_AUSWAHL_PX && r.height() > MIN_AUSWAHL_PX {
                            let (x1, y1) = a.zu_welt(r.left_top());
                            let (x2, y2) = a.zu_welt(r.right_bottom());
                            let (min_x, max_x, min_y, max_y) = (x1.min(x2), x1.max(x2), y1.min(y2), y1.max(y2));
                            let n = m.wende_bereich_an(|x, y| x >= min_x && x <= max_x && y >= min_y && y <= max_y);
                            self.status = format!("Rechteck: {n} Punkt(e) {}.", if m.entfernen { "abgewählt" } else { "gewählt" });
                        }
                    }
                }
            }
            Werkzeug::Polygon => {
                if antwort.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
                }
                if antwort.dragged_by(egui::PointerButton::Primary) {
                    pan(self, antwort.drag_delta());
                }
                if antwort.double_clicked() {
                    self.status = m.polygon_schliessen();
                } else if antwort.clicked() {
                    if let Some(w) = klick_welt {
                        m.polygon.push(w);
                        self.status = format!("Polygon: {} Ecke(n) – Doppelklick oder Enter schließt.", m.polygon.len());
                    }
                }
                if ui.input(|i| i.key_pressed(egui::Key::Enter)) && !m.polygon.is_empty() {
                    self.status = m.polygon_schliessen();
                }
                if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    m.polygon.clear();
                    self.status = "Polygon verworfen.".into();
                }
            }
        }
        // Klick auf einen Punkt schaltet ihn um (nicht beim Polygonzeichnen)
        if m.werkzeug != Werkzeug::Polygon && antwort.clicked() {
            if let Some(w) = klick_welt {
                match m.naechster(w, PICK_RADIUS_PX / self.scale) {
                    Some(i) => {
                        m.gewaehlt[i] = !m.gewaehlt[i];
                        let k = &m.kandidaten[i];
                        self.status = format!("Punkt {} {}.", if k.nr.is_empty() { "ohne Nummer" } else { &k.nr }, if m.gewaehlt[i] { "gewählt" } else { "abgewählt" });
                    }
                    None => self.status = "Kein Punkt an dieser Stelle.".into(),
                }
            }
        }
        self.auswahl_modus = Some(m);
    }

    /// Zeichnet Punkte, Rechteck und Polygon des Auswahlmodus.
    pub(super) fn zeichne_auswahl(&self, maler: &egui::Painter, a: &Ansicht) {
        let Some(m) = &self.auswahl_modus else {
            return;
        };
        let rect = maler.clip_rect();
        for (k, g) in m.kandidaten.iter().zip(&m.gewaehlt) {
            let p = a.zu_bildschirm(k.r, k.h);
            if !rect.expand(20.0).contains(p) {
                continue;
            }
            let farbe = if k.hilfskoordinate { FARBE_HILFSPUNKT } else { FARBE_PUNKT };
            if *g {
                maler.circle_filled(p, PUNKT_RADIUS + 1.5, FARBE_GEWAEHLT);
                maler.circle_stroke(p, PUNKT_RADIUS + 4.0, Stroke::new(1.5, FARBE_GEWAEHLT));
            } else {
                maler.circle_stroke(p, PUNKT_RADIUS, Stroke::new(1.8, farbe));
            }
            if !k.nr.is_empty() {
                maler.text(p + Vec2::new(PUNKT_RADIUS + 4.0, -PUNKT_RADIUS - 2.0), Align2::LEFT_BOTTOM, &k.nr, FontId::proportional(11.0), if *g { FARBE_GEWAEHLT } else { farbe });
            }
        }
        // Rechteck beim Aufziehen
        if let (Some(s), Some(e)) = (self.auswahl_start, self.auswahl_ende) {
            let r = Rect::from_two_pos(s, e);
            maler.rect_filled(r, 0.0, Color32::from_rgba_unmultiplied(220, 60, 0, 40));
            maler.rect_stroke(r, 0.0, Stroke::new(1.5, FARBE_GEWAEHLT), egui::StrokeKind::Middle);
        }
        // Polygon: gesetzte Ecken, Gummiband zur Maus und automatische Schlusskante
        if !m.polygon.is_empty() {
            let mut ecken: Vec<Pos2> = m.polygon.iter().map(|(x, y)| a.zu_bildschirm(*x, *y)).collect();
            if let Some((x, y)) = m.maus {
                ecken.push(a.zu_bildschirm(x, y));
            }
            // Ohne Füllung: frei gezeichnete Polygone können konkav sein
            let stift = Stroke::new(1.8, FARBE_POLYGON);
            for w in ecken.windows(2) {
                maler.line_segment([w[0], w[1]], stift);
            }
            if ecken.len() >= 3 {
                maler.line_segment([ecken[ecken.len() - 1], ecken[0]], Stroke::new(1.0, Color32::from_rgba_unmultiplied(0, 140, 90, 150)));
            }
            for e in &ecken[..m.polygon.len()] {
                maler.circle_filled(*e, 3.5, FARBE_POLYGON);
            }
        }
    }

    // ── Rückfrage beim Schließen ──────────────────────────────────────────────

    pub(super) fn zeichne_auswahl_frage(&mut self, ctx: &egui::Context) {
        let Some(m) = self.auswahl_modus.as_mut() else {
            return;
        };
        if !m.frage_offen {
            return;
        }
        egui::Window::new("Auswahl übernehmen?").collapsible(false).resizable(false).anchor(Align2::CENTER_CENTER, Vec2::ZERO).show(ctx, |ui| {
            ui.label(format!(
                "{} von {} Punkten sind gewählt.\n\nSollen die Markierungen der Konvertierungstabelle\ndurch diese Auswahl ersetzt werden?",
                m.anzahl_gewaehlt(),
                m.kandidaten.len()
            ));
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button(RichText::new("Übernehmen").strong()).clicked() {
                    m.ergebnis = Some(true);
                }
                if ui.button("Verwerfen").clicked() {
                    m.ergebnis = Some(false);
                }
                if ui.button("Weiter auswählen").clicked() {
                    m.frage_offen = false;
                }
            });
        });
    }
}

fn koordinaten_anzeige(x: f64, y: f64) -> String {
    format!("R: {}   H: {}", de(x, 3), de(y, 3))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn modus() -> AuswahlModus {
        let k = |zeile, r, h| AuswahlKandidat { zeile, nr: zeile.to_string(), r, h, hilfskoordinate: false };
        AuswahlModus {
            kandidaten: vec![k(0, 0.0, 0.0), k(1, 5.0, 5.0), k(2, 20.0, 20.0)],
            gewaehlt: vec![false, false, true],
            ohne_lage: 0,
            tabellen_version: 3,
            werkzeug: Werkzeug::Polygon,
            entfernen: false,
            polygon: Vec::new(),
            maus: None,
            frage_offen: false,
            ergebnis: None,
        }
    }

    #[test]
    fn polygon_waehlt_und_entfernt() {
        let mut m = modus();
        m.polygon = vec![(3.0, 2.0), (9.0, 4.0), (4.0, 9.0)];
        m.polygon_schliessen();
        assert_eq!(m.gewaehlt, [false, true, true], "nur (5/5) liegt im Dreieck; (0/0) liegt außerhalb");
        assert!(m.polygon.is_empty());
        m.entfernen = true;
        m.polygon = vec![(15.0, 15.0), (25.0, 15.0), (25.0, 25.0), (15.0, 25.0)];
        m.polygon_schliessen();
        assert_eq!(m.gewaehlt, [false, true, false]);
        m.polygon = vec![(0.0, 0.0), (1.0, 1.0)];
        assert!(m.polygon_schliessen().contains("mindestens drei"));
    }

    #[test]
    fn naechster_punkt_im_fangradius() {
        let m = modus();
        assert_eq!(m.naechster((4.0, 4.5), 2.0), Some(1));
        assert_eq!(m.naechster((12.0, 12.0), 2.0), None);
    }

    #[test]
    fn ergebnis_wird_einmal_geliefert() {
        let mut v = DxfViewer { auswahl_modus: Some(Box::new(modus())), ..DxfViewer::default() };
        assert!(v.nimm_auswahl_ergebnis().is_none(), "noch nicht entschieden");
        v.auswahl_modus.as_mut().unwrap().ergebnis = Some(true);
        assert!(v.auswahl_entschieden());
        let e = v.nimm_auswahl_ergebnis().unwrap();
        assert_eq!((e.uebernehmen, e.tabellen_version), (true, 3));
        assert_eq!(e.zeilen, [(0, false), (1, false), (2, true)]);
        assert!(v.nimm_auswahl_ergebnis().is_none());
    }
}
