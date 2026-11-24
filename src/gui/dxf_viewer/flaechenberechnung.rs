// Zweck:    Flächenberechnung im DXF-Viewer: Eckpunkte werden im Plan angeklickt, die Fläche wird in der unter den
//           Rechenparametern gewählten Farbe gefüllt, ein Fenster zeigt Fläche und Umfang; das Protokoll (RTF) wird
//           auf Knopfdruck erstellt, wahlweise mit Lageplan samt Punktnummern (Rechenparameter, standardmäßig an)
// Hardware: keine
// Autor:    Johann Gerner

use super::*;
use crate::daten::modelle::jetzt;
use crate::protokolle::flaeche::{self as protokoll, FlaechenPunkt};
use crate::protokolle::{self, ausgabeverzeichnis};
use crate::rechnen::flaeche as rechnung;

/// Durchsichtigkeit der Flächenfüllung im Plan (0–255).
const FUELL_ALPHA: u8 = 95;
/// Radius der Eckpunktmarken [px].
const MARKE_RADIUS: f32 = 6.0;
/// Zwei Klicks näher als dieser Abstand [m] gelten als derselbe Punkt.
const GLEICHER_PUNKT_M: f64 = 1e-6;
/// Höhe der Punktliste im Fenster [px].
const LISTE_HOEHE: f32 = 150.0;

/// Zustand der Flächenberechnung.
#[derive(Default)]
pub(super) struct FlaechenModus {
    punkte: Vec<FlaechenPunkt>,
    meldung: String,
    fehler: bool,
    /// Zuletzt geschriebenes Protokoll.
    protokoll_pfad: Option<PathBuf>,
}

impl FlaechenModus {
    fn koordinaten(&self) -> Vec<(f64, f64)> {
        self.punkte.iter().map(|p| (p.r, p.h)).collect()
    }
}

/// Anzeigename eines Eckpunkts: Punktnummer, sonst „F" mit laufender Nummer (frei gewählte Stelle).
fn anzeigename(p: &FlaechenPunkt, lfd: usize) -> String {
    if p.nr.is_empty() { format!("F{lfd}") } else { p.nr.clone() }
}

impl DxfViewer {
    /// Schaltet die Flächenberechnung ein oder aus.
    pub(super) fn flaeche_umschalten(&mut self) {
        if self.flaeche.is_some() {
            self.flaeche = None;
            self.status = "Flächenberechnung beendet.".to_string();
            return;
        }
        if self.digital_modus || self.absteckung.is_some() {
            self.dialog = Some(Dialog::Hinweis {
                titel: "Flächenberechnung".into(),
                text: "Bitte zuerst Digitalisierung bzw. Absteckung beenden.".into(),
            });
            return;
        }
        self.loesch_modus = false;
        self.flaeche = Some(Box::default());
        self.status = "Flächenberechnung: Eckpunkte im Plan anklicken.".to_string();
    }

    /// Punktnummer der gewählten Stelle (leer, wenn kein nummerierter Punkt getroffen wurde).
    fn flaechen_punktnummer(&self, x: f64, y: f64) -> String {
        let index = self.punkt_index.as_ref();
        let Some(entity) = self.auswahl.and_then(|a| self.entity(a)) else {
            return index.and_then(|i| i.get_punkt_nr(x, y)).map(str::to_string).unwrap_or_default();
        };
        match &entity.art {
            Art::Import { nr, .. } | Art::Marker { nr, .. } => Some(nr.clone()),
            Art::Standpunkt(p) | Art::Neupunkt(p) => Some(p.punkt_nr.clone()),
            Art::Gemessen(e) => Some(e.punkt_nr.clone()),
            Art::Insert { x, y, .. } | Art::Point { x, y } => index.and_then(|i| i.get_punkt_nr(*x, *y)).map(str::to_string),
            Art::Circle { cx, cy, .. } => index.and_then(|i| i.get_punkt_nr(*cx, *cy)).map(str::to_string),
            Art::Line { .. } => index.and_then(|i| i.get_punkt_nr(x, y)).map(str::to_string),
            _ => None,
        }
        .unwrap_or_default()
    }

    /// Ein Klick im Plan: Punkt zur Fläche hinzufügen. Liefert `false`, wenn die Flächenberechnung nicht aktiv ist.
    pub(super) fn flaeche_punkt_gewaehlt(&mut self, x: f64, y: f64) -> bool {
        if self.flaeche.is_none() {
            return false;
        }
        let nr = self.flaechen_punktnummer(x, y);
        let Some(modus) = self.flaeche.as_mut() else {
            return false;
        };
        if modus.punkte.last().is_some_and(|p| (p.r - x).abs() < GLEICHER_PUNKT_M && (p.h - y).abs() < GLEICHER_PUNKT_M) {
            return true;
        }
        modus.punkte.push(FlaechenPunkt { nr, r: x, h: y });
        modus.protokoll_pfad = None;
        modus.meldung.clear();
        let a = rechnung::flaeche(&modus.koordinaten());
        self.status = format!("Fläche: {} m²  ({} Punkte)", f(a, 3), modus.punkte.len());
        true
    }

    /// Füllung, Umriss und Punktnummern der Fläche im Plan.
    pub(super) fn zeichne_flaeche(&self, maler: &egui::Painter, a: &crate::dxf::entity::Ansicht, projekt: &Projekt) {
        let Some(modus) = self.flaeche.as_ref() else {
            return;
        };
        let [r, g, b] = projekt.rechenparameter.params.flaeche_farbe;
        let px: Vec<Pos2> = modus.punkte.iter().map(|p| a.zu_bildschirm(p.r, p.h)).collect();
        if px.len() >= 3 {
            let mut netz = egui::Mesh::default();
            for q in &px {
                netz.colored_vertex(*q, Color32::from_rgba_unmultiplied(r, g, b, FUELL_ALPHA));
            }
            for t in rechnung::dreiecke(&modus.koordinaten()) {
                netz.add_triangle(t[0] as u32, t[1] as u32, t[2] as u32);
            }
            maler.add(egui::Shape::mesh(netz));
        }
        let linie = Stroke::new(2.0, Color32::from_rgb(r / 2, g / 2, b / 2));
        for i in 0..px.len().saturating_sub(1) {
            maler.line_segment([px[i], px[i + 1]], linie);
        }
        if px.len() >= 3 {
            maler.line_segment([px[px.len() - 1], px[0]], linie);
        }
        for (i, (q, p)) in px.iter().zip(&modus.punkte).enumerate() {
            maler.circle_filled(*q, MARKE_RADIUS, Color32::from_rgb(255, 235, 60));
            maler.circle_stroke(*q, MARKE_RADIUS, Stroke::new(1.5, Color32::from_rgb(200, 90, 0)));
            let text = anzeigename(p, i + 1);
            let ort = Pos2::new(q.x + MARKE_RADIUS + 3.0, q.y - MARKE_RADIUS - 2.0);
            // Heller Saum hinter dem Text (lesbar auf der Füllung)
            for (dx, dy) in [(-1.0, 0.0), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0)] {
                maler.text(ort + Vec2::new(dx, dy), Align2::LEFT_BOTTOM, &text, FontId::proportional(14.0), Color32::WHITE);
            }
            maler.text(ort, Align2::LEFT_BOTTOM, &text, FontId::proportional(14.0), Color32::BLACK);
        }
    }

    /// Fenster der Flächenberechnung.
    pub(super) fn zeige_flaechenfenster(&mut self, ctx: &egui::Context, projekt: &Projekt) {
        let Some(mut modus) = self.flaeche.take() else {
            return;
        };
        let mut beenden = false;
        let grafik = projekt.rechenparameter.params.flaeche_grafik_im_protokoll;
        let koord = modus.koordinaten();
        let flaeche = rechnung::flaeche(&koord);
        let hinweis = protokoll::hinweis(&modus.punkte);
        egui::Window::new("Flächenberechnung")
            .collapsible(true)
            .resizable(false)
            .pivot(Align2::RIGHT_TOP)
            .default_pos(self.canvas_rect.right_top() + Vec2::new(-12.0, 12.0))
            .show(ctx, |ui| {
                ui.label("Eckpunkte im Plan anklicken (Umlaufsinn beliebig).");
                ui.label(RichText::new("F… = frei gewählte Stelle ohne Punktnummer").small());
                ui.add_space(4.0);
                egui::ScrollArea::vertical().id_salt("flaeche_liste").max_height(LISTE_HOEHE).auto_shrink([true, true]).show(ui, |ui| {
                    if modus.punkte.is_empty() {
                        ui.label("Noch keine Punkte.");
                    }
                    for (i, p) in modus.punkte.iter().enumerate() {
                        ui.label(RichText::new(format!("{:>2}  {:<8} {:>12} {:>12}", i + 1, anzeigename(p, i + 1), f(p.r, 3), f(p.h, 3))).monospace());
                    }
                });
                ui.add_space(6.0);
                ui.label(RichText::new(format!("Fläche:  {} m²", f(flaeche, 3))).strong().size(16.0));
                ui.label(format!("= {} ha", f(flaeche / 10_000.0, 6)));
                ui.label(format!("Umfang:  {} m", f(rechnung::umfang(&koord), 3)));
                if !hinweis.is_empty() && modus.punkte.len() >= 3 {
                    ui.colored_label(Color32::from_rgb(190, 40, 40), &hinweis);
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.add_enabled(!modus.punkte.is_empty(), egui::Button::new("↶ Letzter Punkt")).clicked() {
                        modus.punkte.pop();
                        modus.protokoll_pfad = None;
                    }
                    if ui.add_enabled(!modus.punkte.is_empty(), egui::Button::new("Alle löschen")).clicked() {
                        modus.punkte.clear();
                        modus.protokoll_pfad = None;
                    }
                });
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    let knopf = egui::Button::new(RichText::new("Protokoll erstellen").strong()).min_size(Vec2::new(150.0, 30.0));
                    if ui.add_enabled(modus.punkte.len() >= 3, knopf).clicked() {
                        let farbe = projekt.rechenparameter.params.flaeche_farbe;
                        let benannt: Vec<FlaechenPunkt> =
                            modus.punkte.iter().enumerate().map(|(i, q)| FlaechenPunkt { nr: anzeigename(q, i + 1), ..q.clone() }).collect();
                        let p = protokoll::protokoll(&benannt, farbe, grafik, &projekt.projektdaten.bearbeiter, jetzt());
                        match p.schreibe(&ausgabeverzeichnis(projekt)) {
                            Ok(pfad) => {
                                modus.meldung = format!("Protokoll gespeichert: {}", pfad.display());
                                modus.fehler = false;
                                modus.protokoll_pfad = Some(pfad);
                            }
                            Err(e) => {
                                modus.meldung = e;
                                modus.fehler = true;
                                modus.protokoll_pfad = None;
                            }
                        }
                    }
                    if let Some(pfad) = modus.protokoll_pfad.clone() {
                        if ui.button("Öffnen").clicked() {
                            if let Err(e) = protokolle::oeffne(&pfad) {
                                modus.meldung = e;
                                modus.fehler = true;
                            }
                        }
                    }
                    if ui.button("Beenden").clicked() {
                        beenden = true;
                    }
                });
                ui.label(RichText::new(format!("Grafik im Protokoll: {} (Rechenparameter)", if grafik { "ja" } else { "nein" })).small());
                if !modus.meldung.is_empty() {
                    let farbe = if modus.fehler { Color32::from_rgb(190, 40, 40) } else { ui.visuals().text_color() };
                    ui.add(egui::Label::new(RichText::new(&modus.meldung).color(farbe).small()).wrap());
                }
            });
        if beenden {
            self.status = "Flächenberechnung beendet.".to_string();
        } else {
            self.flaeche = Some(modus);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn klicks_werden_nur_im_modus_gesammelt() {
        let mut v = DxfViewer::default();
        assert!(!v.flaeche_punkt_gewaehlt(1.0, 2.0));
        v.flaeche_umschalten();
        assert!(v.flaeche_punkt_gewaehlt(0.0, 0.0));
        assert!(v.flaeche_punkt_gewaehlt(0.0, 0.0), "doppelter Klick wird verschluckt");
        assert!(v.flaeche_punkt_gewaehlt(10.0, 0.0));
        assert!(v.flaeche_punkt_gewaehlt(10.0, 10.0));
        let modus = v.flaeche.as_ref().unwrap();
        assert_eq!(modus.punkte.len(), 3);
        assert!((rechnung::flaeche(&modus.koordinaten()) - 50.0).abs() < 1e-9);
        assert!(v.status.contains("50.000"), "{}", v.status);
        v.flaeche_umschalten();
        assert!(v.flaeche.is_none());
    }

    #[test]
    fn punkte_ohne_nummer_bekommen_einen_anzeigenamen() {
        let p = FlaechenPunkt { nr: String::new(), r: 0.0, h: 0.0 };
        assert_eq!(anzeigename(&p, 3), "F3");
        let p = FlaechenPunkt { nr: "A7".into(), r: 0.0, h: 0.0 };
        assert_eq!(anzeigename(&p, 3), "A7");
    }
}
