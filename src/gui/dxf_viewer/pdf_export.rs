use super::*;
use crate::daten::rechenparameter::PdfParameter;
use crate::dxf::pdf::{self, Blatt, PX_MM};
use egui::emath::TSTransform;
use std::hash::Hasher;
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

#[derive(Clone, PartialEq)]
struct VorschauSchluessel {
    parameter: PdfParameter,
    mitte: [f64; 2],
    inhalt: u64,
    titel: String,
    ellipsen: Option<f64>,
}

#[derive(Default)]
pub(super) struct Vorschau {
    letzte_pruefung: Option<Instant>,
    inhalt: u64,
    fertig: Option<(VorschauSchluessel, Arc<pdf::Szene>)>,
    arbeit: Option<mpsc::Receiver<(VorschauSchluessel, Arc<pdf::Szene>)>>,
}

/// Debug-Ausgabe direkt hashen, ohne für jede Entity einen großen String anzulegen.
struct InhaltsHash(std::collections::hash_map::DefaultHasher);
impl std::fmt::Write for InhaltsHash {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        self.0.write(s.as_bytes());
        Ok(())
    }
}

/// Die Greiffläche bleibt unabhängig von Zoom und Papiermaßstab gut erreichbar.
const ANKER_GREIFFLAECHE: f32 = 22.0;

fn anker_flaechen(seite: Rect) -> [Rect; 4] {
    [
        seite.left_top(),
        seite.right_top(),
        seite.left_bottom(),
        seite.right_bottom(),
    ]
    .map(|pos| Rect::from_center_size(pos, Vec2::splat(ANKER_GREIFFLAECHE)))
}

/// Eigene Drag-Widgets liegen über der allgemeinen Pan-Fläche des Viewers.
fn anker_eingabe(ui: &mut egui::Ui, seite: Rect, canvas: Rect) -> (bool, Vec2) {
    let mut aktiv = false;
    let mut delta = Vec2::ZERO;
    for (i, rect) in anker_flaechen(seite).into_iter().enumerate() {
        let rect = rect.intersect(canvas);
        if !rect.is_positive() {
            continue;
        }
        let antwort = ui
            .interact(rect, ui.scope_id().with(("pdf_eckanker", i)), Sense::drag())
            .on_hover_text("A4-Rahmen verschieben");
        if antwort.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
        }
        if antwort.dragged() {
            delta += antwort.drag_delta();
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
        }
        aktiv |= antwort.drag_started() || antwort.dragged() || antwort.drag_stopped();
    }
    (aktiv, delta)
}

impl DxfViewer {
    fn melde_pdf_fehler(&mut self, text: String) {
        crate::grundlagen::fehler_log::log("DXF-Viewer.PDF", &text);
        self.status = "PDF-Export fehlgeschlagen – Details im Fehlerfenster.".into();
        self.dialog = Some(Dialog::Fehler {
            titel: "Fehler beim PDF-Export".into(),
            text,
        });
    }
    pub(super) fn verarbeite_pdf_anker(&mut self, ui: &mut egui::Ui, projekt: &Projekt) -> bool {
        let Some((seite, _, _)) = self.pdf_rahmen(projekt) else {
            return false;
        };
        let (aktiv, delta) = anker_eingabe(ui, seite, self.canvas_rect);
        if aktiv {
            self.pdf_ziehen = false;
            if let Some(mitte) = &mut self.pdf_mitte {
                mitte[0] += f64::from(delta.x) / self.scale;
                mitte[1] -= f64::from(delta.y) / self.scale;
            }
        }
        aktiv
    }
    /// Gemeinsame Szenenauswahl für Bildschirm, Druckvorschau und PDF.
    pub(super) fn sichtbare_entities(&self) -> Vec<(&DxfEntity, Color32, Option<Auswahl>)> {
        let mut out = Vec::new();
        if self.dxf_sichtbar {
            for (i, e) in self
                .entities
                .iter()
                .enumerate()
                .filter(|(_, e)| !e.ist_insert())
            {
                out.push((
                    e,
                    layer_farbe(&e.layer).or(e.farbe).unwrap_or(STANDARD_LINIE),
                    Some(Auswahl::Zeichnung(i)),
                ));
            }
            if self.punkte_sichtbar {
                for (i, e) in self
                    .entities
                    .iter()
                    .enumerate()
                    .filter(|(_, e)| e.ist_insert())
                {
                    out.push((e, STANDARD_LINIE, Some(Auswahl::Zeichnung(i))));
                }
            }
        }
        for (l, layer) in self
            .import_layer
            .iter()
            .enumerate()
            .filter(|(_, l)| l.sichtbar)
        {
            for (i, e) in layer.entities.iter().enumerate() {
                out.push((e, STANDARD_LINIE, Some(Auswahl::Import(l, i))));
            }
        }
        for (i, e) in self.overlay.iter().enumerate() {
            out.push((e, STANDARD_LINIE, Some(Auswahl::Overlay(i))));
        }
        if self.neupunkte_sichtbar {
            for (i, e) in self.neupunkte.iter().enumerate() {
                out.push((e, STANDARD_LINIE, Some(Auswahl::Neupunkt(i))));
            }
        }
        if self.residuen_sichtbar {
            for e in self.residuen.iter().chain(&self.ellipsen) {
                out.push((e, STANDARD_LINIE, None));
            }
        }
        if self.marker_sichtbar {
            for (i, e) in self.marker.iter().enumerate() {
                out.push((e, STANDARD_LINIE, Some(Auswahl::Marker(i))));
            }
        }
        out
    }

    pub(super) fn pdf_leiste(&mut self, ui: &mut egui::Ui, projekt: &mut Projekt) {
        let p = &mut projekt.rechenparameter.params.pdf;
        let mut speichern = false;
        ui.horizontal_wrapped(|ui| {
            ui.radio_value(&mut p.querformat, false, "A4 hoch");
            ui.radio_value(&mut p.querformat, true, "A4 quer");
            ui.label("1 :");
            ui.add(
                egui::DragValue::new(&mut p.massstab)
                    .range(1.0..=1_000_000.0)
                    .speed(10.0)
                    .fixed_decimals(0),
            );
            if ui.button("Maske einpassen").clicked() {
                let b = Blatt::neu(p);
                self.mitte = self.pdf_mitte.unwrap_or(self.mitte);
                self.scale = (f64::from(self.canvas_groesse.x / b.seite.width())
                    .min(f64::from(self.canvas_groesse.y / b.seite.height()))
                    * 0.9)
                    * f64::from(PX_MM)
                    * 1000.0
                    / p.normalisiert().massstab;
            }
            if ui.button("Auf Ansichtsmitte").clicked() {
                self.pdf_mitte = Some(self.mitte);
            }
            if ui.button("PDF speichern …").clicked() {
                speichern = true;
            }
            ui.label("Rahmen ziehen · Mausrad: Zoom");
        });
        if speichern {
            if let Err(e) = projekt.rechenparameter.speichern() {
                self.melde_pdf_fehler(format!(
                    "Die PDF-Vorgaben konnten nicht gespeichert werden.\n\n{e}"
                ));
                return;
            }
            self.speichere_pdf(projekt);
        }
    }

    fn pdf_szene(&self, projekt: &Projekt) -> pdf::Szene {
        let entities = self
            .sichtbare_entities()
            .into_iter()
            .map(|(e, f, _)| (e, f))
            .collect::<Vec<_>>();
        pdf::erzeuge(
            &egui::Context::default(),
            &projekt.rechenparameter.params.pdf,
            self.pdf_mitte.unwrap_or(self.mitte),
            &entities,
            &format!(
                "{} · {}",
                projekt.einstellungen.projekt_name,
                chrono::Local::now().format("%d.%m.%Y")
            ),
            (self.residuen_sichtbar && !self.ellipsen.is_empty())
                .then_some(self.ellipsen_skalierung),
        )
    }

    fn aktuelle_pdf_vorschau(
        &self,
        parameter: &PdfParameter,
        titel: String,
        ellipsen: Option<f64>,
    ) -> Option<Arc<pdf::Szene>> {
        let mut v = self.pdf_vorschau.borrow_mut();
        if let Some(rx) = &v.arbeit {
            match rx.try_recv() {
                Ok(fertig) => {
                    v.fertig = Some(fertig);
                    v.arbeit = None;
                }
                Err(mpsc::TryRecvError::Disconnected) => v.arbeit = None,
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        let entities = self.sichtbare_entities();
        if v.letzte_pruefung
            .is_none_or(|t| t.elapsed() >= Duration::from_millis(250))
        {
            use std::fmt::Write as _;
            let mut hash = InhaltsHash(std::collections::hash_map::DefaultHasher::new());
            for (e, farbe, _) in &entities {
                let _ = write!(hash, "{e:?}{farbe:?}");
            }
            v.inhalt = hash.0.finish();
            v.letzte_pruefung = Some(Instant::now());
        }
        let key = VorschauSchluessel {
            parameter: parameter.normalisiert(),
            mitte: self.pdf_mitte.unwrap_or(self.mitte),
            inhalt: v.inhalt,
            titel,
            ellipsen,
        };
        if let Some((fertig_key, szene)) = &v.fertig {
            if *fertig_key == key {
                return Some(Arc::clone(szene));
            }
        }
        // Höchstens ein Auftrag gleichzeitig. Nach dessen Ende gilt nur der neueste Maßstab.
        if v.arbeit.is_none() {
            let snapshot: Vec<_> = entities
                .into_iter()
                .map(|(e, f, _)| (e.clone(), f))
                .collect();
            let (tx, rx) = mpsc::channel();
            v.arbeit = Some(rx);
            std::thread::spawn(move || {
                let refs = snapshot.iter().map(|(e, f)| (e, *f)).collect::<Vec<_>>();
                let szene = pdf::erzeuge(
                    &egui::Context::default(),
                    &key.parameter,
                    key.mitte,
                    &refs,
                    &key.titel,
                    key.ellipsen,
                );
                let _ = tx.send((key, Arc::new(szene)));
            });
        }
        None
    }

    pub(super) fn pdf_rahmen(&self, projekt: &Projekt) -> Option<(Rect, Rect, TSTransform)> {
        let mitte = self.pdf_mitte?;
        let b = Blatt::neu(&projekt.rechenparameter.params.pdf);
        let faktor = (self.scale / b.ansicht(mitte).scale) as f32;
        let c = self
            .ansicht(self.canvas_rect.center())
            .zu_bildschirm(mitte[0], mitte[1]);
        let t = TSTransform {
            scaling: faktor,
            translation: c.to_vec2() - b.karte.center().to_vec2() * faktor,
        };
        Some((t * b.seite, t * b.karte, t))
    }

    pub(super) fn zeichne_pdf_maske(&self, maler: &egui::Painter, projekt: &Projekt) {
        let Some((seite, karte, t)) = self.pdf_rahmen(projekt) else {
            return;
        };
        let szene = self.aktuelle_pdf_vorschau(
            &projekt.rechenparameter.params.pdf,
            format!(
                "{} · {}",
                projekt.einstellungen.projekt_name,
                chrono::Local::now().format("%d.%m.%Y")
            ),
            (self.residuen_sichtbar && !self.ellipsen.is_empty())
                .then_some(self.ellipsen_skalierung),
        );
        let canvas = self.canvas_rect;
        // Weißes Papier mit tatsächlichem Druckbild; außerhalb abgedunkelte Originalansicht.
        for r in [
            Rect::from_min_max(
                canvas.min,
                Pos2::new(
                    canvas.right(),
                    seite.top().clamp(canvas.top(), canvas.bottom()),
                ),
            ),
            Rect::from_min_max(
                Pos2::new(
                    canvas.left(),
                    seite.bottom().clamp(canvas.top(), canvas.bottom()),
                ),
                canvas.max,
            ),
            Rect::from_min_max(
                Pos2::new(canvas.left(), seite.top().max(canvas.top())),
                Pos2::new(
                    seite.left().clamp(canvas.left(), canvas.right()),
                    seite.bottom().min(canvas.bottom()),
                ),
            ),
            Rect::from_min_max(
                Pos2::new(
                    seite.right().clamp(canvas.left(), canvas.right()),
                    seite.top().max(canvas.top()),
                ),
                Pos2::new(canvas.right(), seite.bottom().min(canvas.bottom())),
            ),
        ] {
            if r.is_positive() {
                maler.rect_filled(r, 0.0, Color32::from_black_alpha(90));
            }
        }
        maler.rect_filled(seite, 0.0, Color32::WHITE);
        if let Some(szene) = &szene {
            for form in &szene.formen {
                let transform = t * form.transform;
                let mut shape = form.shape.clone();
                if form.karteninhalt {
                    pdf::mindestbreite(
                        &mut shape,
                        szene.blatt.parameter.linie_min_mm as f32 * PX_MM / form.transform.scaling,
                    );
                }
                shape.transform(transform);
                maler
                    .with_clip_rect((t * form.clip).intersect(canvas))
                    .add(shape);
            }
        } else {
            maler.with_clip_rect(karte).text(
                karte.center(),
                Align2::CENTER_CENTER,
                "Druckvorschau wird aktualisiert …",
                FontId::proportional(14.0),
                Color32::DARK_GRAY,
            );
            maler.ctx().request_repaint_after(Duration::from_millis(50));
        }
        let stift = Stroke::new(2.0, Color32::from_rgb(40, 100, 180));
        maler.rect_stroke(seite, 0.0, stift, egui::StrokeKind::Middle);
        maler.rect_stroke(
            karte,
            0.0,
            Stroke::new(1.0, Color32::from_rgb(40, 100, 180)),
            egui::StrokeKind::Middle,
        );
        for pos in [
            seite.left_top(),
            seite.right_top(),
            seite.left_bottom(),
            seite.right_bottom(),
        ] {
            maler.rect_filled(
                Rect::from_center_size(pos, Vec2::splat(7.0)),
                0.0,
                stift.color,
            );
        }
    }

    fn speichere_pdf(&mut self, projekt: &Projekt) {
        if self.sichtbare_entities().is_empty() {
            self.melde_pdf_fehler(
                "Keine sichtbaren Zeichnungsobjekte für die PDF vorhanden.".into(),
            );
            return;
        }
        let mut dialog = rfd::FileDialog::new()
            .set_title("Vektor-PDF speichern")
            .add_filter("PDF", &["pdf"])
            .set_file_name(format!("{}_Plan.pdf", projekt.einstellungen.projekt_name));
        if projekt.einstellungen.ist_geladen() {
            dialog = dialog.set_directory(&projekt.einstellungen.projekt_verzeichnis);
        }
        let Some(pfad) = dialog.save_file() else {
            return;
        };
        let ergebnis = pdf::pdf(&self.pdf_szene(projekt)).and_then(|bytes| {
            crate::grundlagen::datei_speicher::schreibe_bytes(&pfad, &bytes)
                .map_err(|e| e.to_string())
        });
        match ergebnis {
            Ok(()) => {
                self.pdf_mitte = None;
                self.pdf_ziehen = false;
                self.status = format!(
                "Vektor-PDF gespeichert: {} · Drucken bei 100 %",
                pfad.display()
                );
            }
            Err(e) => self.melde_pdf_fehler(format!("Die PDF-Datei konnte nicht gespeichert werden.\n\nZieldatei:\n{}\n\nFehlerdetails:\n{e}",pfad.display())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn massstabwechsel_verwirft_alte_auftraege_und_verwendet_den_cache() {
        let v = DxfViewer::default();
        let alt = PdfParameter {
            massstab: 1.0,
            ..Default::default()
        };
        let neu = PdfParameter {
            massstab: 500.0,
            ..Default::default()
        };
        // Die erste Anfrage startet nur den Hintergrundauftrag.
        assert!(v.aktuelle_pdf_vorschau(&alt, "Test".into(), None).is_none());
        let start = Instant::now();
        let fertig = loop {
            if let Some(szene) = v.aktuelle_pdf_vorschau(&neu, "Test".into(), None) {
                break szene;
            }
            assert!(start.elapsed() < Duration::from_secs(10));
            std::thread::sleep(Duration::from_millis(1));
        };
        assert_eq!(fertig.blatt.parameter.massstab, 500.0);
        let cache = v.aktuelle_pdf_vorschau(&neu, "Test".into(), None).unwrap();
        assert!(Arc::ptr_eq(&fertig, &cache));
        assert!(v.pdf_vorschau.borrow().arbeit.is_none());
    }

    #[test]
    fn alle_vier_eckanker_uebernehmen_das_ziehen_von_der_pan_flaeche() {
        let seite = Rect::from_min_max(Pos2::new(100.0, 100.0), Pos2::new(300.0, 400.0));
        let canvas = Rect::from_min_size(Pos2::ZERO, Vec2::splat(500.0));
        for griff in anker_flaechen(seite) {
            let ctx = egui::Context::default();
            let start = griff.center();
            let ende = start + Vec2::new(30.0, -20.0);
            let schritt = |events| {
                let mut ergebnis = (false, Vec2::ZERO, false);
                let mut out = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(canvas),
                        events,
                        ..Default::default()
                    },
                    |ui| {
                        let (hintergrund, _) =
                            ui.allocate_painter(ui.available_size(), Sense::click_and_drag());
                        let (aktiv, delta) = anker_eingabe(ui, seite, canvas);
                        ergebnis = (aktiv, delta, hintergrund.dragged());
                    },
                );
                out.textures_delta.clear();
                ergebnis
            };
            schritt(vec![]);
            schritt(vec![
                egui::Event::PointerMoved(start),
                egui::Event::PointerButton {
                    pos: start,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ]);
            let (aktiv, delta, pan) = schritt(vec![egui::Event::PointerMoved(ende)]);
            assert!(aktiv, "Eckanker bei {start:?} muss das Ziehen übernehmen");
            assert_eq!(delta, Vec2::new(30.0, -20.0));
            assert!(!pan, "Die Zeichnung darf dabei nicht verschoben werden");
        }
    }

    #[test]
    fn pdf_und_bildschirm_verwenden_dieselbe_sichtbarkeit() {
        let mut v = DxfViewer::default();
        let line = DxfEntity {
            layer: "Gebaeude".into(),
            farbe: Some(Color32::RED),
            art: Art::Line {
                x1: 0.0,
                y1: 0.0,
                x2: 1.0,
                y2: 1.0,
            },
        };
        let point = DxfEntity {
            layer: "0".into(),
            farbe: None,
            art: Art::Insert {
                block: "P".into(),
                x: 0.0,
                y: 0.0,
            },
        };
        v.entities = vec![line.clone(), point];
        v.import_layer.push(ImportLayer {
            name: "verborgen".into(),
            entities: vec![line.clone()],
            sichtbar: false,
        });
        v.residuen.push(line);
        assert_eq!(v.sichtbare_entities().len(), 3);
        assert_eq!(v.sichtbare_entities()[0].1, Color32::BLACK);
        v.punkte_sichtbar = false;
        assert_eq!(v.sichtbare_entities().len(), 2);
        v.residuen_sichtbar = false;
        assert_eq!(v.sichtbare_entities().len(), 1);
        v.dxf_sichtbar = false;
        assert!(v.sichtbare_entities().is_empty());
    }
}
