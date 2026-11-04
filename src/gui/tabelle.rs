// Zweck:    Tabellenbaustein im Stil des DataGridView der Originalversion: Rasterlinien zwischen den Zellen,
//           graue Kopfzeile und Zeilenköpfe, abwechselnde Zeilenfarbe (AliceBlue)
// Hardware: keine
// Autor:    Johann Gerner

use eframe::egui::{self, Align, Color32, Layout, Pos2, Rect, Sense, Stroke, UiBuilder, Vec2};

/// Farbe der Rasterlinien (ControlDark).
pub const RASTER_FARBE: Color32 = Color32::from_rgb(160, 160, 160);
/// Hintergrund der Kopfzeile und der Zeilenköpfe (Control).
const KOPF_FARBE: Color32 = Color32::from_rgb(240, 240, 240);
/// Hintergrund gerader Zeilen.
const ZEILE_WEISS: Color32 = Color32::WHITE;
/// Hintergrund ungerader Zeilen (AliceBlue).
const ZEILE_BLAU: Color32 = Color32::from_rgb(240, 248, 255);
/// Textfarbe in der Tabelle.
const TEXT_FARBE: Color32 = Color32::from_rgb(20, 20, 20);
/// Höhe einer Zeile in Pixeln.
const ZEILEN_HOEHE: f32 = 24.0;
/// Innenabstand der Zellen.
const ZELLEN_RAND: f32 = 3.0;
/// Breite der Rasterlinie.
const LINIEN_BREITE: f32 = 1.0;

/// Tabelle mit festen Spaltenbreiten.
pub struct Raster {
    spalten: Vec<f32>,
    zeilenkopf: Option<f32>,
}

impl Raster {
    /// Neue Tabelle mit den Breiten der Datenspalten.
    pub fn neu(spalten: &[f32]) -> Self {
        Self { spalten: spalten.to_vec(), zeilenkopf: None }
    }

    /// Fügt links eine Spalte für Zeilenköpfe hinzu (wie RowHeadersWidth im DataGridView).
    pub fn mit_zeilenkopf(mut self, breite: f32) -> Self {
        self.zeilenkopf = Some(breite);
        self
    }

    fn gesamt_breite(&self) -> f32 {
        self.zeilenkopf.unwrap_or(0.0) + self.spalten.iter().sum::<f32>()
    }

    /// Zeichnet die Kopfzeile.
    pub fn kopf(&self, ui: &mut egui::Ui, titel: &[&str]) {
        let (rect, _) = ui.allocate_exact_size(Vec2::new(self.gesamt_breite(), ZEILEN_HOEHE), Sense::hover());
        let maler = ui.painter();
        maler.rect_filled(rect, 0.0, KOPF_FARBE);
        self.zeichne_linien(ui, rect, true);
        let mut x = rect.left() + self.zeilenkopf.unwrap_or(0.0);
        for (i, breite) in self.spalten.iter().enumerate() {
            let text = titel.get(i).copied().unwrap_or("");
            let zelle = Rect::from_min_size(Pos2::new(x, rect.top()), Vec2::new(*breite, rect.height()));
            ui.painter().with_clip_rect(ui.clip_rect().intersect(zelle)).text(
                Pos2::new(x + ZELLEN_RAND, rect.center().y),
                egui::Align2::LEFT_CENTER,
                text,
                egui::FontId::proportional(13.0),
                TEXT_FARBE,
            );
            x += breite;
        }
    }

    /// Zeichnet eine Datenzeile; der Inhalt der Zellen kommt aus `inhalt`.
    pub fn zeile(
        &self,
        ui: &mut egui::Ui,
        index: usize,
        zeilenkopf_text: &str,
        inhalt: impl FnOnce(&mut Zellen<'_>),
    ) {
        self.zeile_farbig(ui, index, zeilenkopf_text, None, inhalt);
    }

    /// Wie `zeile`, aber mit eigener Hintergrundfarbe der Zeile.
    pub fn zeile_farbig(
        &self,
        ui: &mut egui::Ui,
        index: usize,
        zeilenkopf_text: &str,
        hintergrund: Option<Color32>,
        inhalt: impl FnOnce(&mut Zellen<'_>),
    ) {
        let (rect, _) = ui.allocate_exact_size(Vec2::new(self.gesamt_breite(), ZEILEN_HOEHE), Sense::hover());
        let farbe = hintergrund.unwrap_or(if index % 2 == 0 { ZEILE_WEISS } else { ZEILE_BLAU });
        ui.painter().rect_filled(rect, 0.0, farbe);
        if let Some(breite) = self.zeilenkopf {
            let kopf = Rect::from_min_size(rect.min, Vec2::new(breite, rect.height()));
            ui.painter().rect_filled(kopf, 0.0, KOPF_FARBE);
            ui.painter().with_clip_rect(ui.clip_rect().intersect(kopf)).text(
                kopf.center(),
                egui::Align2::CENTER_CENTER,
                zeilenkopf_text,
                egui::FontId::proportional(12.0),
                TEXT_FARBE,
            );
        }
        self.zeichne_linien(ui, rect, false);
        let mut zellen = Zellen {
            ui,
            rect,
            breiten: &self.spalten,
            naechste: 0,
            x: rect.left() + self.zeilenkopf.unwrap_or(0.0),
        };
        inhalt(&mut zellen);
    }

    fn zeichne_linien(&self, ui: &egui::Ui, rect: Rect, mit_oberkante: bool) {
        let strich = Stroke::new(LINIEN_BREITE, RASTER_FARBE);
        let maler = ui.painter();
        maler.line_segment([rect.left_bottom(), rect.right_bottom()], strich);
        maler.line_segment([rect.left_top(), rect.left_bottom()], strich);
        if mit_oberkante {
            maler.line_segment([rect.left_top(), rect.right_top()], strich);
        }
        let mut x = rect.left();
        if let Some(breite) = self.zeilenkopf {
            x += breite;
            maler.line_segment([Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())], strich);
        }
        for breite in &self.spalten {
            x += breite;
            maler.line_segment([Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())], strich);
        }
    }
}

/// Zellen einer Zeile; jeder Aufruf füllt die nächste Spalte.
pub struct Zellen<'a> {
    ui: &'a mut egui::Ui,
    rect: Rect,
    breiten: &'a [f32],
    naechste: usize,
    x: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tabelleninhalte_bleiben_beim_scrollen_im_sichtbereich() {
        for versatz in [Vec2::ZERO, Vec2::new(-45.0, -30.0), Vec2::new(30.0, 65.0)] {
            let ctx = egui::Context::default();
            let sichtbereich = Rect::from_min_size(Pos2::new(50.0, 50.0), Vec2::new(120.0, 70.0));
            let mut ausgabe = ctx.run_ui(egui::RawInput::default(), |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    let mut tabelle = ui.new_child(UiBuilder::new().max_rect(sichtbereich.translate(versatz)));
                    tabelle.set_clip_rect(sichtbereich);
                    let raster = Raster::neu(&[60.0, 60.0]).mit_zeilenkopf(30.0);
                    raster.kopf(&mut tabelle, &["Sehr lange Überschrift", "Zweite Spalte"]);
                    for i in 0..5 {
                        raster.zeile(&mut tabelle, i, "123456789", |z| {
                            z.zahl("12345678901234567890");
                            z.zelle(|ui| {
                                assert!(sichtbereich.contains_rect(ui.clip_rect()) || !ui.clip_rect().is_positive());
                                ui.label("Sehr langer Zellinhalt");
                            });
                        });
                    }
                });
            });
            // Der Test prüft Zeichenbefehle ohne GPU-Backend.
            ausgabe.textures_delta.clear();
            let mut texte = 0;
            for form in ausgabe.shapes {
                if matches!(form.shape, egui::epaint::Shape::Text(_)) {
                    texte += 1;
                    assert!(sichtbereich.contains_rect(form.clip_rect) || !form.clip_rect.is_positive());
                    assert!(form.clip_rect.width() <= 60.0);
                }
            }
            assert!(texte > 0);
        }
    }
}

impl Zellen<'_> {
    /// Fläche der nächsten Zelle (vorgerückt).
    fn naechste_flaeche(&mut self) -> Option<Rect> {
        let breite = *self.breiten.get(self.naechste)?;
        let zelle = Rect::from_min_size(Pos2::new(self.x, self.rect.top()), Vec2::new(breite, self.rect.height()));
        self.x += breite;
        self.naechste += 1;
        Some(zelle.shrink2(Vec2::new(ZELLEN_RAND, 1.0)))
    }

    /// Beliebiger Inhalt in der nächsten Zelle.
    pub fn zelle(&mut self, inhalt: impl FnOnce(&mut egui::Ui)) {
        if let Some(flaeche) = self.naechste_flaeche() {
            let mut kind = self.ui.new_child(
                UiBuilder::new()
                    .max_rect(flaeche)
                    .layout(Layout::left_to_right(Align::Center)),
            );
            // Die Zellgrenze darf den Clip des übergeordneten Scrollbereichs nicht erweitern.
            kind.set_clip_rect(self.ui.clip_rect().intersect(flaeche.expand2(Vec2::new(ZELLEN_RAND - 1.0, 0.0))));
            inhalt(&mut kind);
        }
    }

    /// Linksbündiger Text.
    pub fn text(&mut self, text: &str) {
        self.zelle(|ui| {
            ui.label(egui::RichText::new(text).color(TEXT_FARBE));
        });
    }

    /// Rechtsbündiger Text in Festschrift (Zahlen).
    pub fn zahl(&mut self, text: &str) {
        self.zahl_farbig(text, TEXT_FARBE);
    }

    /// Wie `zahl`, aber mit eigener Farbe.
    pub fn zahl_farbig(&mut self, text: &str, farbe: Color32) {
        if let Some(flaeche) = self.naechste_flaeche() {
            self.ui.painter().with_clip_rect(self.ui.clip_rect().intersect(flaeche)).text(
                Pos2::new(flaeche.right(), flaeche.center().y),
                egui::Align2::RIGHT_CENTER,
                text,
                egui::FontId::monospace(13.0),
                farbe,
            );
        }
    }

    /// Eingabefeld ohne eigenen Rahmen.
    /// Liefert `true`, wenn der Text geändert wurde.
    pub fn eingabe(&mut self, wert: &mut String) -> bool {
        let mut geaendert = false;
        self.zelle(|ui| {
            let breite = ui.available_width();
            geaendert = ui
                .add(egui::TextEdit::singleline(wert).frame(egui::Frame::NONE).desired_width(breite))
                .changed();
        });
        geaendert
    }

    /// Kontrollkästchen, zentriert.
    pub fn haken(&mut self, wert: &mut bool) {
        self.zelle(|ui| {
            ui.checkbox(wert, "");
        });
    }

    /// Schaltfläche in der Zelle; liefert `true` bei Klick.
    pub fn knopf(&mut self, beschriftung: &str) -> bool {
        let mut geklickt = false;
        self.zelle(|ui| {
            geklickt = ui.small_button(beschriftung).clicked();
        });
        geklickt
    }
}
