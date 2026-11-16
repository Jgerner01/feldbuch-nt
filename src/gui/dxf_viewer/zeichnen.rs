// Zweck:    Oberfläche des DXF-Viewers: Statusleiste, Seitenleiste, Zeichenfläche, Aktionen und Dialoge
// Hardware: keine
// Autor:    Johann Gerner

use super::*;

/// Eingabefelder der Toolbar.
#[derive(Clone, Copy)]
pub enum FeldArt {
    Code,
    PunktNr,
    Zielhoehe,
    ExzLaengs,
    ExzQuer,
    Standpunkt,
}

/// Senkrechte Trennlinie der Toolbar.
pub fn trenner(ui: &mut egui::Ui) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(1.0, 28.0), Sense::hover());
    ui.painter().rect_filled(rect, 0.0, TRENNER_FARBE);
}

/// Signallampe: Farbe nach Stationierungsqualität, schwarz ohne Stationierung.
pub fn zeichne_lampe(ui: &egui::Ui, rect: Rect, farbe: Option<Color32>) {
    let maler = ui.painter();
    let c = rect.center();
    let r = rect.width().min(rect.height()) / 2.0 - 2.0;
    match farbe {
        None => {
            maler.circle_filled(c, r, Color32::BLACK);
        }
        Some(f) => {
            maler.circle_filled(c, r, f);
            maler.circle_filled(c, r * 0.55, Color32::from_rgba_unmultiplied(255, 255, 255, 90));
            let g = r * 0.35;
            maler.circle_filled(Pos2::new(c.x - r * 0.5, c.y - r * 0.55), g, Color32::from_rgba_unmultiplied(255, 255, 255, 120));
        }
    }
    maler.circle_stroke(c, r, Stroke::new(1.5, Color32::from_rgb(80, 80, 80)));
}

/// Farben eines Seitenleistenknopfes.
#[derive(Clone, Copy)]
struct KnopfFarben {
    fill: Color32,
    fore: Color32,
    rand: Color32,
}

const fn farben(fill: (u8, u8, u8), fore: (u8, u8, u8), rand: (u8, u8, u8)) -> KnopfFarben {
    KnopfFarben { fill: Color32::from_rgb(fill.0, fill.1, fill.2), fore: Color32::from_rgb(fore.0, fore.1, fore.2), rand: Color32::from_rgb(rand.0, rand.1, rand.2) }
}

const BASIS: (u8, u8, u8) = (68, 74, 92);
const BASIS_RAND: (u8, u8, u8) = (85, 92, 115);
const AKTIV_FARBEN: KnopfFarben = farben((200, 120, 30), (255, 255, 255), (220, 140, 50));
const INAKTIV_FARBEN: KnopfFarben = farben((190, 190, 190), (30, 30, 30), (160, 160, 160));
const BLAU_FARBEN: KnopfFarben = farben((52, 110, 190), (255, 255, 255), (40, 90, 165));
const ROT_FARBEN: KnopfFarben = farben((150, 38, 38), (255, 255, 255), (120, 28, 28));

/// Inhalt eines Seitenleistenknopfes.
enum Inhalt {
    Bild(egui::ImageSource<'static>),
    /// Bild, das den Knopf bis auf einen schmalen Rand ausfüllt.
    BildVoll(egui::ImageSource<'static>),
    Text(&'static str, f32),
}

/// Abstand des vollflächigen Bildes zum Knopfrand [pt].
const BILD_VOLL_RAND: f32 = 2.0;

/// Icon der Module Freie Stationierung und Polarpunkte: füllt den ganzen Knopf.
fn modul_bild(name: &str) -> Inhalt {
    use egui::include_image;
    Inhalt::BildVoll(match name {
        "freie_stationierung" => include_image!("../../../icons/freie_stationierung.png"),
        _ => include_image!("../../../icons/polarpunkte.png"),
    })
}

fn bild(name: &str) -> Inhalt {
    use egui::include_image;
    Inhalt::Bild(match name {
        "open" => include_image!("../../../icons/sidebar_open.png"),
        "zoom_in" => include_image!("../../../icons/sidebar_zoom_in.png"),
        "zoom_out" => include_image!("../../../icons/sidebar_zoom_out.png"),
        "fit" => include_image!("../../../icons/sidebar_fit.png"),
        "snap" => include_image!("../../../icons/sidebar_snap.png"),
        "points" => include_image!("../../../icons/sidebar_points.png"),
        "dxf_toggle" => include_image!("../../../icons/sidebar_dxf_toggle.png"),
        "dxf_export" => include_image!("../../../icons/sidebar_dxf_export.png"),
        "new" => include_image!("../../../icons/sidebar_new.png"),
        "import_kor" => include_image!("../../../icons/sidebar_import_kor.png"),
        _ => include_image!("../../../icons/sidebar_import_json.png"),
    })
}

fn seitenknopf(ui: &mut egui::Ui, inhalt: Inhalt, f: KnopfFarben, tooltip: &str) -> bool {
    let groesse = Vec2::splat(KNOPF_GROESSE);
    if let Inhalt::BildVoll(quelle) = inhalt {
        let antwort = ui.add_sized(groesse, egui::Button::new("").fill(f.fill).stroke(Stroke::new(1.0, f.rand)));
        egui::Image::new(quelle).paint_at(ui, antwort.rect.shrink(BILD_VOLL_RAND));
        return antwort.on_hover_text(tooltip).clicked();
    }
    let knopf = match inhalt {
        Inhalt::BildVoll(_) => unreachable!(),
        Inhalt::Bild(quelle) => egui::Button::image(egui::Image::new(quelle).fit_to_exact_size(Vec2::splat(26.0))),
        Inhalt::Text(text, groesse_pt) => egui::Button::new(egui::RichText::new(text).strong().size(groesse_pt).color(f.fore)),
    }
    .fill(f.fill)
    .stroke(Stroke::new(1.0, f.rand))
    .min_size(groesse);
    ui.add_sized(groesse, knopf).on_hover_text(tooltip).clicked()
}

/// Koordinaten als Statustext.
fn koordinaten_text(x: f64, y: f64) -> String {
    format!("R: {}   H: {}", de(x, 3), de(y, 3))
}

impl DxfViewer {
    // ── Eingabefelder mit Nebenwirkung ────────────────────────────────────────

    pub(super) fn standpunkt_verlassen(&mut self, projekt: &mut Projekt) {
        let sp = self.txt_standpunkt.trim().to_string();
        if sp.is_empty() {
            return;
        }
        projekt.einstellungen.letzte_standpunkt_nr = sp.clone();
        let _ = projekt.einstellungen.speichern();
        projekt.station_laden(&sp);
        if self.lokales_system {
            self.baue_overlay(projekt);
        }
    }

    pub(super) fn instr_hoehe_geaendert(&mut self, projekt: &mut Projekt) {
        let ih = match parse_zahl(&self.txt_instr_hoehe).filter(|v| *v >= 0.0) {
            Some(v) => {
                projekt.einstellungen.instrumenten_hoehe = v;
                let _ = projekt.einstellungen.speichern();
                v
            }
            None => projekt.einstellungen.instrumenten_hoehe,
        };
        if let Err(fehler) = projekt.set_instrumenten_hoehe(ih) {
            self.status = format!("Instrumentenhöhe konnte nicht gespeichert werden: {fehler}");
        }
    }

    // ── Statusleiste ──────────────────────────────────────────────────────────

    pub(super) fn zeichne_statusleiste(&mut self, ui: &mut egui::Ui, projekt: &mut Projekt) {
        ui.horizontal_centered(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(8.0, 0.0);
            ui.add_space(6.0);
            let (rect, _) = ui.allocate_exact_size(Vec2::new(STATUS_LABEL_BREITE - 6.0, 20.0), Sense::hover());
            ui.painter().with_clip_rect(rect).text(Pos2::new(rect.left(), rect.center().y), Align2::LEFT_CENTER, &self.status, FontId::monospace(12.0), STATUS_TEXT);

            let text = |s: &str| egui::RichText::new(s.to_string()).size(12.0).color(Color32::from_rgb(30, 30, 30));
            if self.punkt_index.is_some() {
                ui.checkbox(&mut self.marker_sichtbar, text("DXF-Punkte")).on_hover_text("DXF-Punktnummern ein-/ausblenden");
            }
            if ui.checkbox(&mut self.neupunkte_sichtbar, text("Neupunkte")).on_hover_text("Gemessene Neupunkte ein-/ausblenden").changed() {
                projekt.einstellungen.neupunkte_visible = self.neupunkte_sichtbar;
                let _ = projekt.einstellungen.speichern();
            }
            if ui
                .checkbox(&mut self.residuen_sichtbar, text("Residuen"))
                .on_hover_text("Residuen, Residuenellipsen und Konfidenzellipse des Standpunkts ein-/ausblenden\nWenn aktiv: Mausrad skaliert die Ellipsen")
                .changed()
            {
                projekt.einstellungen.residual_visible = self.residuen_sichtbar;
                let _ = projekt.einstellungen.speichern();
                if self.residuen_sichtbar && !self.ellipsen.is_empty() {
                    self.auto_skaliere_ellipsen();
                }
            }
            for layer in &mut self.import_layer {
                ui.checkbox(&mut layer.sichtbar, text(&layer.name)).on_hover_text(layer.name.clone());
            }

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if self.lokales_system {
                    ui.add_space(8.0);
                    ui.label(egui::RichText::new("Lokales System").strong().size(12.0).color(Color32::from_rgb(160, 40, 20)));
                }
            });
        });
    }

    // ── Seitenleiste ──────────────────────────────────────────────────────────

    pub(super) fn zeichne_seitenleiste(&mut self, ui: &mut egui::Ui, projekt: &mut Projekt) {
        ui.spacing_mut().item_spacing = Vec2::ZERO;
        let basis = farben(BASIS, (210, 215, 230), BASIS_RAND);
        // Im Auswahlmodus nur grafische Funktionen (Zoom, Einpassen, Snap, Sichtbarkeit); Rest ausgegraut
        let frei = !self.ist_auswahl();
        let knopf = |ui: &mut egui::Ui, inhalt: Inhalt, f: KnopfFarben, tooltip: &str| ui.add_enabled_ui(frei, |ui| seitenknopf(ui, inhalt, f, tooltip)).inner;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(6.0);
                if knopf(ui, bild("open"), BLAU_FARBEN, "DXF-Datei öffnen") {
                    self.datei_oeffnen(projekt);
                }
                ui.add_space(6.0);
                if seitenknopf(ui, bild("zoom_in"), basis, "Zoom +") {
                    self.zoom(ZOOM_KNOPF_FAKTOR, self.canvas_rect.center(), self.canvas_rect.center());
                }
                ui.add_space(2.0);
                if seitenknopf(ui, bild("zoom_out"), basis, "Zoom −") {
                    self.zoom(1.0 / ZOOM_KNOPF_FAKTOR, self.canvas_rect.center(), self.canvas_rect.center());
                }
                ui.add_space(2.0);
                if seitenknopf(ui, bild("fit"), basis, "Einpassen") {
                    self.fit_to_view();
                }
                ui.add_space(6.0);

                let snap = if self.snap_aktiv { AKTIV_FARBEN } else { INAKTIV_FARBEN };
                let tip = format!("Punktfang (Snap) – aktuell {}", if self.snap_aktiv { "EIN" } else { "AUS" });
                if seitenknopf(ui, bild("snap"), snap, &tip) {
                    self.snap_aktiv = !self.snap_aktiv;
                    if frei {
                        let _ = projekt.projektdaten.set_value("Einstellungen", "Snap", if self.snap_aktiv { "Aktiv" } else { "Inaktiv" });
                    }
                }
                ui.add_space(2.0);
                let pk = if self.punkte_sichtbar { AKTIV_FARBEN } else { INAKTIV_FARBEN };
                let tip = format!("Katasterpunkte – aktuell {}", if self.punkte_sichtbar { "EIN" } else { "AUS" });
                if seitenknopf(ui, bild("points"), pk, &tip) {
                    self.punkte_sichtbar = !self.punkte_sichtbar;
                    if frei {
                        let _ = projekt.projektdaten.set_value("Einstellungen", "Katasterpunkte", if self.punkte_sichtbar { "Aktiv" } else { "Inaktiv" });
                    }
                }
                ui.add_space(6.0);
                let dxf = if self.dxf_sichtbar { farben((60, 100, 160), (255, 255, 255), (40, 80, 140)) } else { INAKTIV_FARBEN };
                let tip = format!("DXF-Darstellung – aktuell {}", if self.dxf_sichtbar { "EIN" } else { "AUS" });
                if seitenknopf(ui, bild("dxf_toggle"), dxf, &tip) {
                    self.dxf_sichtbar = !self.dxf_sichtbar;
                }
                ui.add_space(6.0);
                if knopf(ui, bild("new"), farben(BASIS, (200, 205, 225), BASIS_RAND), "DXF leeren") {
                    self.dialog = Some(Dialog::DxfLeeren);
                }
                ui.add_space(6.0);
                if knopf(ui, bild("import_kor"), ROT_FARBEN, "KOR / CSV importieren") {
                    self.import_kor_csv(projekt);
                }
                ui.add_space(2.0);
                if knopf(ui, bild("import_json"), ROT_FARBEN, "JSON importieren") {
                    self.import_json(projekt);
                }
                ui.add_space(6.0);
                let loesch = if self.loesch_modus { farben((160, 40, 40), (255, 255, 255), (130, 20, 20)) } else { farben(BASIS, (220, 200, 200), BASIS_RAND) };
                if knopf(ui, Inhalt::Text("⌫", 16.0), loesch, "Punkte löschen (Fenster aufziehen)") {
                    self.loesch_modus = !self.loesch_modus;
                    self.status =
                        if self.loesch_modus { "Lösch-Modus: Fenster aufziehen  |  Erneut klicken zum Abbrechen".to_string() } else { "Lösch-Modus beendet.".to_string() };
                }
                ui.add_space(6.0);
                if knopf(ui, Inhalt::Text("↻", 16.0), farben(BASIS, (200, 210, 230), BASIS_RAND), "Punkt-Index zurücksetzen (neu nummerieren)") {
                    self.dialog = Some(Dialog::IndexReset);
                }
                ui.add_space(6.0);
                let dig = if self.digital_modus { farben((36, 120, 60), (255, 255, 255), (20, 90, 40)) } else { farben(BASIS, (200, 230, 210), BASIS_RAND) };
                if knopf(ui, Inhalt::Text("✎", 16.0), dig, "Digitalisierung: Klick → Punkt in KOR/CSV schreiben") {
                    self.digitalisierung_umschalten(projekt);
                }
                ui.add_space(6.0);
                if knopf(ui, Inhalt::Text("⊕A", 11.0), farben((50, 110, 60), (255, 255, 255), (30, 80, 40)), "Absteckung") {
                    self.dialog = Some(Dialog::Hinweis { titel: "Absteckung".into(), text: "Die Absteckung ist in Feldbuch-NT noch nicht umgesetzt.".into() });
                }
                ui.add_space(6.0);
                if knopf(ui, Inhalt::Text("⊙", 16.0), farben((40, 70, 110), (180, 210, 255), (25, 50, 85)), "Aufnahmetest: Koordinaten messen & Punkt anfahren (motorisiert)")
                {
                    self.wunsch = Some(Wunsch::Aufnahmetest);
                }
                ui.add_space(6.0);
                if knopf(ui, modul_bild("freie_stationierung"), farben((80, 55, 20), (240, 210, 150), (60, 40, 10)), "Freie Stationierung") {
                    self.wunsch = Some(Wunsch::FreieStationierung);
                }
                ui.add_space(6.0);
                if knopf(ui, modul_bild("polarpunkte"), farben((80, 55, 20), (240, 210, 150), (60, 40, 10)), "Polarpunkte") {
                    self.wunsch = Some(Wunsch::Polarpunkte);
                }
                ui.add_space(6.0);
                if knopf(ui, Inhalt::Text("→FP", 11.0), farben((40, 90, 70), (255, 255, 255), (25, 65, 50)), "Gemessene Neupunkte als Festpunkte übernehmen (lokales System)")
                {
                    self.festpunkte_dialog_oeffnen(projekt);
                }
            });
        });
    }

    // ── Aktionen ──────────────────────────────────────────────────────────────

    fn datei_oeffnen(&mut self, projekt: &mut Projekt) {
        let mut dialog = rfd::FileDialog::new().set_title("DXF-Datei öffnen").add_filter("DXF-Dateien (*.dxf)", &["dxf"]).add_filter("Alle Dateien (*.*)", &["*"]);
        if projekt.einstellungen.projekt_verzeichnis.is_dir() {
            dialog = dialog.set_directory(&projekt.einstellungen.projekt_verzeichnis);
        }
        if let Some(pfad) = dialog.pick_file() {
            self.lade_dxf(projekt, &pfad);
            self.aktualisiere_lokales_system(projekt);
        }
    }

    fn add_import_layer(&mut self, projekt: &Projekt, pfad: &Path, punkte: &[crate::daten::modelle::ImportPunkt]) {
        let name = pfad.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let entities: Vec<DxfEntity> = punkte.iter().map(|p| DxfEntity::overlay("Import", Art::Import { nr: p.punkt_nr.clone(), wx: p.r, wy: p.h, hoehe: p.hoehe })).collect();
        match self.import_layer.iter_mut().find(|l| l.name.eq_ignore_ascii_case(&name)) {
            Some(layer) => layer.entities = entities,
            None => self.import_layer.push(ImportLayer { name, entities, sichtbar: true }),
        }
        self.aktualisiere_lokales_system(projekt);
    }

    fn import_kor_csv(&mut self, projekt: &mut Projekt) {
        let Some(datei) = rfd::FileDialog::new()
            .set_title("KOR- oder CSV-Datei importieren")
            .add_filter("KOR / CSV", &["kor", "csv"])
            .add_filter("KOR-Datei", &["kor"])
            .add_filter("CSV-Datei", &["csv"])
            .add_filter("Alle Dateien", &["*"])
            .pick_file()
        else {
            return;
        };
        let name = datei.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let ext = datei.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
        let ist_kor_csv = name.to_lowercase().ends_with("-kor.csv");

        let gelesen = if ext == "csv" || ist_kor_csv { lese_csv(&datei) } else { crate::daten::punktlisten::lese_kor_roh(&datei) };
        let mut punkte = match gelesen {
            Ok(p) => p,
            Err(fehler) => {
                self.dialog = Some(Dialog::Hinweis { titel: "Importfehler".into(), text: format!("Fehler beim Lesen:\n{fehler}") });
                return;
            }
        };
        let mut quelle = datei.clone();

        // Auto-Erkennung: Begleit-CSV bevorzugen, wenn sie mindestens ebenso viele Punkte enthält
        if ext == "kor" && !ist_kor_csv {
            let begleit = kor_csv_pfad(&datei);
            if begleit.exists() {
                if let Ok(csv) = lese_csv(&begleit) {
                    if csv.len() >= punkte.len() {
                        punkte = csv;
                        quelle = begleit;
                    }
                }
            }
        }
        self.import_abschliessen(projekt, &quelle, punkte);
    }

    fn import_json(&mut self, projekt: &mut Projekt) {
        let Some(datei) = rfd::FileDialog::new().set_title("JSON-Datei importieren").add_filter("JSON-Dateien", &["json"]).add_filter("Alle Dateien", &["*"]).pick_file() else {
            return;
        };
        match lese_json(&datei) {
            Ok(p) if p.is_empty() => {
                self.dialog = Some(Dialog::Hinweis { titel: "Import".into(), text: "Keine Punkte in der JSON-Datei gefunden.".into() });
            }
            Ok(p) => self.import_abschliessen(projekt, &datei, p),
            Err(fehler) => {
                self.dialog = Some(Dialog::Hinweis { titel: "Importfehler".into(), text: format!("Fehler beim Lesen:\n{fehler}") });
            }
        }
    }

    fn import_abschliessen(&mut self, projekt: &mut Projekt, quelle: &Path, punkte: Vec<crate::daten::modelle::ImportPunkt>) {
        let (neu, doppelt) = match projekt.importpunkte.add_range(punkte.clone()) {
            Ok(r) => r,
            Err(fehler) => {
                self.dialog = Some(Dialog::Hinweis { titel: "Importfehler".into(), text: format!("Punkte konnten nicht gespeichert werden:\n{fehler}") });
                return;
            }
        };
        self.add_import_layer(projekt, quelle, &punkte);
        self.fit_to_view();
        let name = quelle.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        self.status = format!("{neu} Punkte importiert aus {name}");
        if doppelt > 0 {
            self.status.push_str(&format!("  ({doppelt} Duplikat(e) übersprungen)"));
        }
    }

    // ── Digitalisierung ───────────────────────────────────────────────────────

    fn digitalisierung_umschalten(&mut self, projekt: &mut Projekt) {
        if self.digital_modus {
            self.digital_modus = false;
            if let Some(kor) = &self.digital_kor_pfad {
                self.status = format!(
                    "Digitalisierung beendet – {} und {} gespeichert",
                    kor_csv_pfad(kor).display(),
                    kor.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
                );
            }
            return;
        }
        if self.digital_kor_pfad.is_none() {
            let projekt_dir = projekt.einstellungen.projekt_verzeichnis.clone();
            let datei_name = format!("{}-dig.kor", projekt.einstellungen.projekt_name);
            let kor_pfad = projekt_dir.join(&datei_name);
            if kor_pfad.exists() {
                self.digital_zaehler = naechste_punkt_nr(&kor_pfad);
                self.digital_kor_pfad = Some(kor_pfad);
                self.digital_layer = None;
            } else {
                let mut dialog = rfd::FileDialog::new()
                    .set_title("Digitalisierte Punkte speichern als …")
                    .add_filter("KOR-Datei", &["kor"])
                    .add_filter("Alle Dateien", &["*"])
                    .set_file_name(&datei_name);
                if projekt_dir.is_dir() {
                    dialog = dialog.set_directory(&projekt_dir);
                }
                let Some(gewaehlt) = dialog.save_file() else {
                    return;
                };
                let jetzt = chrono::Local::now();
                if let Err(e) = datei_speicher::schreibe_text(&gewaehlt, &format!("% Digitalisierte Punkte  {}\n", jetzt.format("%Y-%m-%d %H:%M"))) {
                    self.dialog = Some(Dialog::Hinweis { titel: "Fehler".into(), text: e.to_string() });
                    return;
                }
                let kopf = match crate::daten::kor_csv::erzeuge(&projekt.einstellungen.projekt_name, "Digitalisierung", &jetzt.format("%Y-%m-%d").to_string(), &[]) {
                    Ok(text) => text,
                    Err(e) => {
                        self.dialog = Some(Dialog::Hinweis { titel: "Fehler".into(), text: e.to_string() });
                        return;
                    }
                };
                if let Err(e) = datei_speicher::schreibe_text(&kor_csv_pfad(&gewaehlt), &kopf) {
                    self.dialog = Some(Dialog::Hinweis { titel: "Fehler".into(), text: e.to_string() });
                    return;
                }
                self.digital_zaehler = 1;
                self.digital_kor_pfad = Some(gewaehlt);
                self.digital_layer = None;
            }
        }
        self.digital_modus = true;
        let name = self.digital_kor_pfad.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        self.status = format!("Digitalisierung aktiv  → {name}  [Klick = Punkt]");
    }

    /// Schreibt einen digitalisierten Punkt parallel in .kor und -kor.csv.
    fn schreibe_digitalisier_punkt(&self, nr: &str, r: f64, h: f64, hoehe: f64, code: &str) -> std::io::Result<()> {
        let Some(kor) = &self.digital_kor_pfad else {
            return Ok(());
        };
        let (x, y, z) = (f(r, 3), f(h, 3), f(hoehe, 3));
        let kor_zeile = format!("{nr:<8}{x:>16}{y:>16}{z:>10}    {code}\n");
        OpenOptions::new().create(true).append(true).open(kor)?.write_all(kor_zeile.as_bytes())?;
        let csv_zeile = format!("{}\r\n", crate::daten::csv_basis::zeile([nr, &f(r, 4), &f(h, 4), &f(hoehe, 4), code], ';'));
        OpenOptions::new().create(true).append(true).open(kor_csv_pfad(kor))?.write_all(csv_zeile.as_bytes())
    }

    // ── Zeichenfläche ─────────────────────────────────────────────────────────

    pub(super) fn zeichne_canvas(&mut self, ui: &mut egui::Ui, projekt: &mut Projekt) {
        let (antwort, maler) = ui.allocate_painter(ui.available_size(), Sense::click_and_drag());
        let rect = antwort.rect;
        self.canvas_rect = rect;
        self.canvas_groesse = rect.size();
        let ursprung = rect.center();
        maler.rect_filled(rect, 0.0, HINTERGRUND);

        if self.fit_ausstehend {
            self.fit_ausstehend = false;
            self.fit_to_view();
        }
        if self.lokal_zeigen_ausstehend {
            self.lokal_zeigen_ausstehend = false;
            self.zeige_lokales_system();
        }
        if self.loesch_modus && antwort.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
        }

        if self.ist_auswahl() {
            self.verarbeite_auswahl_eingabe(ui, &antwort, ursprung);
        } else if !self.verarbeite_pdf_anker(ui, projekt) {
            self.verarbeite_eingabe(ui, &antwort, projekt, ursprung);
        }

        let maler = maler.with_clip_rect(rect);
        let a = self.ansicht(ursprung);
        let hat_daten = (self.dxf_sichtbar && !self.entities.is_empty())
            || !self.overlay.is_empty()
            || !self.import_layer.is_empty()
            || !self.neupunkte.is_empty()
            || self.hat_auswahl_kandidaten();
        if !hat_daten {
            if self.pdf_mitte.is_some() {
                self.zeichne_pdf_maske(&maler, projekt);
                return;
            }
            maler.text(Pos2::new(rect.center().x - 90.0, rect.center().y - 10.0), Align2::LEFT_TOP, "DXF-Datei öffnen …", FontId::proportional(18.0), Color32::GRAY);
            return;
        }

        let groesse = rect.size();
        let zeichne = |e: &DxfEntity, farbe: Color32, breite: f32, ausgewaehlt: bool| {
            if self.ist_zeichenbar(e, ursprung, groesse) {
                let (f, b) = if ausgewaehlt { (AUSWAHL_FARBE, 2.0) } else { (farbe, breite) };
                e.zeichne(&maler, &a, f, b);
            }
        };

        for (e, farbe, auswahl) in self.sichtbare_entities() {
            zeichne(e, farbe, 1.0, auswahl.is_some() && self.auswahl == auswahl);
        }
        if self.residuen_sichtbar && !self.ellipsen.is_empty() {
            self.zeichne_ellipsen_massstab(&maler, rect);
        }

        // Auswahlmodus: wählbare Punkte, Rechteck und Polygon; keine Zielrichtung, kein Fadenkreuz
        if self.ist_auswahl() {
            self.zeichne_auswahl(&maler, &a);
            return;
        }

        if self.pdf_mitte.is_some() {
            self.zeichne_pdf_maske(&maler, projekt);
            return;
        }

        self.zeichne_zielrichtung(&maler, &a, projekt);

        // Fadenkreuz
        let kreuz = Stroke::new(1.0, FADENKREUZ_FARBE);
        maler.line_segment([Pos2::new(rect.center().x, rect.top()), Pos2::new(rect.center().x, rect.bottom())], kreuz);
        maler.line_segment([Pos2::new(rect.left(), rect.center().y), Pos2::new(rect.right(), rect.center().y)], kreuz);

        // Snap-Marker
        if let Some((sx, sy)) = self.snap_punkt {
            let sp = a.zu_bildschirm(sx, sy);
            let s = 9.0;
            let stift = Stroke::new(2.0, SNAP_FARBE);
            maler.rect_stroke(Rect::from_center_size(sp, Vec2::splat(s * 2.0)), 0.0, stift, egui::StrokeKind::Middle);
            maler.line_segment([Pos2::new(sp.x - s - 4.0, sp.y), Pos2::new(sp.x - s, sp.y)], stift);
            maler.line_segment([Pos2::new(sp.x + s, sp.y), Pos2::new(sp.x + s + 4.0, sp.y)], stift);
            maler.line_segment([Pos2::new(sp.x, sp.y - s - 4.0), Pos2::new(sp.x, sp.y - s)], stift);
            maler.line_segment([Pos2::new(sp.x, sp.y + s), Pos2::new(sp.x, sp.y + s + 4.0)], stift);
        }

        // Lösch-Auswahlrechteck
        if let (Some(s), Some(e)) = (self.auswahl_start, self.auswahl_ende) {
            let r = Rect::from_two_pos(s, e);
            if r.width() > 0.0 && r.height() > 0.0 {
                maler.rect_filled(r, 0.0, Color32::from_rgba_unmultiplied(210, 0, 0, 55));
                maler.rect_stroke(r, 0.0, Stroke::new(1.5, Color32::from_rgba_unmultiplied(180, 0, 0, 210)), egui::StrokeKind::Middle);
            }
        }
    }

    /// Maßstabsbalken für die Ellipsen-Skalierung (links unten).
    fn zeichne_ellipsen_massstab(&self, maler: &egui::Painter, rect: Rect) {
        let px_pro_mm = 0.001 * self.ellipsen_skalierung * self.scale;
        if px_pro_mm <= 0.0 {
            return;
        }
        let bar_mm = schoene_balken_mm(80.0 / px_pro_mm);
        let bar_px = (bar_mm * px_pro_mm) as f32;
        if bar_px < 3.0 || bar_px > rect.width() * 0.6 {
            return;
        }
        let text = if bar_mm >= 1.0 { format!("{} mm", de(bar_mm, 0)) } else { format!("{}×0,1 mm", de(bar_mm * 10.0, 1)) };
        let farbe = Color32::from_rgb(20, 80, 200);
        let (x_links, y_mitte) = (rect.left() + 12.0, rect.bottom() - 20.0);
        let text_breite = 7.0 * text.chars().count() as f32;
        let box_w = bar_px + 16.0 + text_breite;
        let box_r = Rect::from_min_size(Pos2::new(x_links - 4.0, y_mitte - 11.0), Vec2::new(box_w, 22.0));
        maler.rect_filled(box_r, 0.0, Color32::from_rgba_unmultiplied(255, 255, 255, 210));
        maler.rect_stroke(box_r, 0.0, Stroke::new(1.0, Color32::from_rgb(160, 160, 160)), egui::StrokeKind::Middle);
        let stift = Stroke::new(2.5, farbe);
        maler.line_segment([Pos2::new(x_links, y_mitte), Pos2::new(x_links + bar_px, y_mitte)], stift);
        maler.line_segment([Pos2::new(x_links, y_mitte - 5.0), Pos2::new(x_links, y_mitte + 5.0)], stift);
        maler.line_segment([Pos2::new(x_links + bar_px, y_mitte - 5.0), Pos2::new(x_links + bar_px, y_mitte + 5.0)], stift);
        maler.text(Pos2::new(x_links + bar_px + 6.0, y_mitte), Align2::LEFT_CENTER, text, FontId::proportional(12.0), farbe);
    }

    /// Zwei-Finger-Geste auf dem Touchscreen: Spreizen/Zusammenziehen zoomt um den Mittelpunkt zwischen den Fingern,
    /// gemeinsames Bewegen verschiebt die Grafik. Gibt `true` zurück, solange die Geste läuft.
    pub(super) fn verarbeite_zwei_finger(&mut self, ui: &egui::Ui, antwort: &egui::Response, ursprung: Pos2) -> bool {
        let Some(geste) = ui.input(|i| i.multi_touch()) else {
            return false;
        };
        if !antwort.rect.contains(geste.center_pos) {
            return false;
        }
        // Begonnene Ein-Finger-Aktionen verwerfen (kein Löschrechteck, kein Verschieben des PDF-Rahmens)
        self.auswahl_start = None;
        self.auswahl_ende = None;
        self.pdf_ziehen = false;
        let massstab = self.ansicht(ursprung).scale;
        let d = geste.translation_delta;
        self.mitte[0] -= f64::from(d.x) / massstab;
        self.mitte[1] += f64::from(d.y) / massstab;
        let faktor = f64::from(geste.zoom_delta);
        if faktor.is_finite() && faktor > 0.0 && (faktor - 1.0).abs() > f64::EPSILON {
            self.zoom(faktor, geste.center_pos, ursprung);
        }
        ui.ctx().request_repaint();
        true
    }

    fn verarbeite_eingabe(&mut self, ui: &mut egui::Ui, antwort: &egui::Response, projekt: &mut Projekt, ursprung: Pos2) {
        if self.verarbeite_zwei_finger(ui, antwort, ursprung) {
            return;
        }
        if self.pdf_mitte.is_some() {
            let a = self.ansicht(ursprung);
            if antwort.drag_started() {
                self.pdf_ziehen = ui
                    .input(|i| i.pointer.press_origin())
                    .is_some_and(|pos| self.pdf_rahmen(projekt).is_some_and(|(seite, karte, _)| seite.expand(8.0).contains(pos) && !karte.shrink(8.0).contains(pos)));
            }
            if antwort.dragged_by(egui::PointerButton::Primary) {
                let d = antwort.drag_delta();
                if self.pdf_ziehen {
                    if let Some(m) = &mut self.pdf_mitte {
                        m[0] += f64::from(d.x) / a.scale;
                        m[1] -= f64::from(d.y) / a.scale;
                    }
                } else {
                    self.mitte[0] -= f64::from(d.x) / a.scale;
                    self.mitte[1] += f64::from(d.y) / a.scale;
                }
            }
            if antwort.drag_stopped() {
                self.pdf_ziehen = false;
            }
            if antwort.hovered() {
                let scroll = ui.input(|i| i.raw.events.iter().filter_map(|e| if let egui::Event::MouseWheel { delta, .. } = e { Some(delta.y) } else { None }).sum::<f32>());
                if scroll != 0.0 {
                    if let Some(pos) = antwort.hover_pos() {
                        self.zoom(if scroll > 0.0 { ZOOM_RAD_FAKTOR } else { 1.0 / ZOOM_RAD_FAKTOR }, pos, ursprung);
                    }
                }
            }
            return;
        }
        // Löschfenster aufziehen
        if self.loesch_modus {
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
                        let a = self.ansicht(ursprung);
                        let (x1, y1) = a.zu_welt(r.left_top());
                        let (x2, y2) = a.zu_welt(r.right_bottom());
                        self.rechteck_gewaehlt(projekt, Rahmen::neu(x1.min(x2), y1.min(y2), x1.max(x2), y1.max(y2)));
                    }
                }
            }
        } else if antwort.dragged_by(egui::PointerButton::Primary) {
            let d = antwort.drag_delta();
            self.mitte[0] -= f64::from(d.x) / self.scale;
            self.mitte[1] += f64::from(d.y) / self.scale;
        }

        // Mausrad: Zoom oder Ellipsen-Skalierung
        if antwort.hovered() {
            // Rohe Mausrad-Ereignisse: genau ein Zoomschritt pro Raste
            let scroll: f32 = ui.input(|i| {
                i.raw
                    .events
                    .iter()
                    .filter_map(|e| match e {
                        egui::Event::MouseWheel { delta, .. } => Some(delta.y),
                        _ => None,
                    })
                    .sum()
            });
            if scroll != 0.0 {
                let faktor = if scroll > 0.0 { ZOOM_RAD_FAKTOR } else { 1.0 / ZOOM_RAD_FAKTOR };
                if self.residuen_sichtbar && !self.ellipsen.is_empty() {
                    self.setze_ellipsen_skalierung((self.ellipsen_skalierung * faktor).clamp(1e-9, 1e9));
                } else if let Some(pos) = antwort.hover_pos() {
                    self.zoom(faktor, pos, ursprung);
                }
            }
        }

        // Mausbewegung: Snap und Koordinatenanzeige
        if let Some(pos) = antwort.hover_pos() {
            let a = self.ansicht(ursprung);
            let (wx, wy) = a.zu_welt(pos);
            self.snap_punkt = None;
            if self.snap_aktiv && !self.entities.is_empty() {
                if let Some(treffer) = self.pick((wx, wy)).and_then(|t| self.entity(t)) {
                    self.snap_punkt = Some(treffer.snap_point(wx, wy));
                }
            }
            if ui.input(|i| i.pointer.delta() != Vec2::ZERO) {
                let (x, y) = self.snap_punkt.unwrap_or((wx, wy));
                let hinweis = if self.snap_punkt.is_some() { " [SNAP]" } else { "" };
                self.status = if !self.entities.is_empty() && !self.digital_modus {
                    format!("{}{hinweis}   |   Kein Objekt ausgewählt", koordinaten_text(x, y))
                } else {
                    koordinaten_text(x, y)
                };
            }
        } else {
            self.snap_punkt = None;
        }

        // Klick: Auswahl bzw. Digitalisieren
        if antwort.clicked() && !self.loesch_modus {
            if let Some(pos) = antwort.interact_pointer_pos() {
                let a = self.ansicht(ursprung);
                let (wx, wy) = a.zu_welt(pos);
                self.auswahl = self.pick((wx, wy));
                let punkt = match self.auswahl.and_then(|s| self.entity(s)) {
                    Some(e) if self.snap_aktiv => e.snap_point(wx, wy),
                    Some(e) => e.nearest_point(wx, wy),
                    None => (wx, wy),
                };
                self.punkt_gewaehlt(projekt, punkt);
            }
        }

        // Rechtsklick: Kontextmenü
        if antwort.secondary_clicked() {
            if let Some(pos) = antwort.interact_pointer_pos() {
                self.kontext_welt = self.ansicht(ursprung).zu_welt(pos);
            }
        }
        let (kx, ky) = self.kontext_welt;
        let import_nr = self.pick((kx, ky)).and_then(|s| self.entity(s)).and_then(|e| match &e.art {
            Art::Import { nr, .. } => Some(nr.clone()),
            _ => None,
        });
        let mut anfahren_wunsch = None;
        antwort.context_menu(|ui| {
            if let Some(nr) = &import_nr {
                if ui.button(format!("Anfahren: {nr}")).on_hover_text("Öffnet den Aufnahmetest und fährt den Punkt gleichen Namens aus dessen Tabelle motorisch an").clicked() {
                    anfahren_wunsch = Some(nr.clone());
                    ui.close();
                }
                ui.separator();
            }
            ui.add_enabled(false, egui::Button::new(format!("R={}  H={}", de(kx, 3), de(ky, 3))));
        });
        if let Some(nr) = anfahren_wunsch {
            self.wunsch = Some(Wunsch::Anfahren(nr));
        }
    }

    /// Wird aufgerufen, wenn ein Punkt per Klick gewählt wurde (PointPicked mit isClick).
    fn punkt_gewaehlt(&mut self, projekt: &Projekt, punkt: (f64, f64)) {
        let (x, y) = punkt;
        if self.digital_modus {
            let mut nr = self.txt_punkt_nr.trim().to_string();
            let code = self.txt_code.trim().to_string();
            if nr.is_empty() {
                nr = self.digital_zaehler.to_string();
            }
            if let Err(fehler) = self.schreibe_digitalisier_punkt(&nr, x, y, 0.0, &code) {
                self.dialog = Some(Dialog::Hinweis { titel: "Fehler".into(), text: format!("Punkt konnte nicht geschrieben werden:\n{fehler}") });
                return;
            }
            let entity = DxfEntity::overlay("Import", Art::Import { nr: nr.clone(), wx: x, wy: y, hoehe: 0.0 });
            match self.digital_layer {
                Some(i) => {
                    if let Some(l) = self.import_layer.get_mut(i) {
                        l.entities.push(entity);
                    }
                }
                None => {
                    self.import_layer.push(ImportLayer { name: "Digitalisierung".into(), entities: vec![entity], sichtbar: true });
                    self.digital_layer = Some(self.import_layer.len() - 1);
                }
            }
            self.status = format!("Dig. {nr}: R={}  H={}{}", de(x, 3), de(y, 3), if code.is_empty() { String::new() } else { format!("  Code={code}") });
            if let Ok(n) = nr.parse::<i32>() {
                self.digital_zaehler = n + 1;
                self.txt_punkt_nr = self.digital_zaehler.to_string();
            }
            return;
        }

        let hinweis = if self.snap_aktiv { " [SNAP]" } else { "" };
        let Some(auswahl) = self.auswahl else {
            self.status = if self.entities.is_empty() { koordinaten_text(x, y) } else { format!("{}{hinweis}   |   Kein Objekt ausgewählt", koordinaten_text(x, y)) };
            return;
        };
        let Some(entity) = self.entity(auswahl) else {
            return;
        };
        let status = format!("{}{hinweis}   |   {}", koordinaten_text(x, y), entity.info());

        // Punktnummer aus der gewählten Entity ins Eingabefeld übernehmen
        let index = self.punkt_index.as_ref();
        let nr = match &entity.art {
            Art::Import { nr, .. } | Art::Marker { nr, .. } => Some(nr.clone()),
            Art::Standpunkt(p) | Art::Neupunkt(p) => Some(p.punkt_nr.clone()),
            Art::Gemessen(e) => Some(e.punkt_nr.clone()),
            Art::Insert { x, y, .. } | Art::Point { x, y } => index.and_then(|i| i.get_punkt_nr(*x, *y)).map(str::to_string),
            Art::Circle { cx, cy, .. } => index.and_then(|i| i.get_punkt_nr(*cx, *cy)).map(str::to_string),
            Art::Line { x1, y1, .. } => index.and_then(|i| i.get_punkt_nr(*x1, *y1)).map(str::to_string),
            _ => None,
        };
        let hoehe = match &entity.art {
            Art::Import { hoehe, .. } => *hoehe,
            Art::Standpunkt(p) | Art::Neupunkt(p) => p.hoehe,
            _ => 0.0,
        };
        self.status = status;
        let nr = nr.filter(|n| !n.is_empty());
        if let Some(nr) = &nr {
            self.txt_punkt_nr.clone_from(nr);
        }
        // Der Dialog ist im Original modal: Klicks bei offenem Dialog öffnen keinen neuen
        if self.dialog.is_none() {
            self.oeffne_messdaten_dialog(projekt, x, y, nr.as_deref().unwrap_or(""), hoehe);
        }
    }

    // ── Löschen per Fenster ───────────────────────────────────────────────────

    fn rechteck_gewaehlt(&mut self, projekt: &Projekt, r: Rahmen) {
        let im_rahmen = |x: f64, y: f64| x >= r.min_x && x <= r.max_x && y >= r.min_y && y <= r.max_y;
        let feldbuch: Vec<String> = projekt.feldbuchpunkte.punkte().iter().filter(|p| im_rahmen(p.r, p.h)).map(|p| p.punkt_nr.clone()).collect();
        let neu: Vec<(String, String)> = projekt
            .neupunkte
            .rohdaten()
            .iter()
            .filter(|p| {
                let (kr, kh) = projekt.neupunkte.finde_koordinate(&p.punkt_nr, &p.standpunkt_nr).map_or((0.0, 0.0), |e| (e.r, e.h));
                im_rahmen(kr, kh)
            })
            .map(|p| (p.punkt_nr.clone(), p.standpunkt_nr.clone()))
            .collect();
        let importiert: Vec<String> = projekt.importpunkte.punkte().iter().filter(|p| im_rahmen(p.r, p.h)).map(|p| p.punkt_nr.clone()).collect();
        let gesamt = feldbuch.len() + neu.len() + importiert.len();
        if gesamt == 0 {
            self.status = "Keine Punkte im markierten Bereich.".to_string();
            return;
        }
        let liste = |titel: &str, nummern: Vec<&String>| -> String {
            if nummern.is_empty() {
                return String::new();
            }
            let n = nummern.len();
            let namen: Vec<&str> = nummern.iter().take(LOESCH_LISTE_MAX).map(|s| s.as_str()).collect();
            format!("{titel} ({n}):  {}{}\n", namen.join(", "), if n > LOESCH_LISTE_MAX { " …" } else { "" })
        };
        let text = format!(
            "{gesamt} Punkt(e) löschen?\n\n{}{}{}",
            liste("Feldbuchpunkte", feldbuch.iter().collect()),
            liste("Neupunkte", neu.iter().map(|(n, _)| n).collect()),
            liste("Importpunkte", importiert.iter().collect()),
        );
        self.dialog = Some(Dialog::Loeschen { text, anzahl: gesamt, ziele: LoeschZiele { feldbuch, neupunkte: neu, importpunkte: importiert } });
    }

    fn loesche(&mut self, projekt: &mut Projekt, anzahl: usize, ziele: LoeschZiele) {
        let (mut fb, mut np, mut imp) = (0, 0, 0);
        if !ziele.feldbuch.is_empty() {
            let nrs: std::collections::HashSet<&str> = ziele.feldbuch.iter().map(String::as_str).collect();
            fb = projekt.feldbuchpunkte.remove_where(|p| nrs.contains(p.punkt_nr.as_str())).unwrap_or(0);
            self.baue_overlay(projekt);
        }
        if !ziele.neupunkte.is_empty() {
            let schluessel: std::collections::HashSet<(&str, &str)> = ziele.neupunkte.iter().map(|(n, s)| (n.as_str(), s.as_str())).collect();
            np = projekt.neupunkte.remove_where(|p| schluessel.contains(&(p.punkt_nr.as_str(), p.standpunkt_nr.as_str()))).unwrap_or(0);
            self.baue_neupunkte_overlay(projekt);
        }
        if !ziele.importpunkte.is_empty() {
            let nrs: std::collections::HashSet<&str> = ziele.importpunkte.iter().map(String::as_str).collect();
            imp = projekt.importpunkte.remove_where(|p| nrs.contains(p.punkt_nr.as_str())).unwrap_or(0);
            if let Some(layer) = self.import_layer.iter_mut().find(|l| l.name.eq_ignore_ascii_case(IMPORT_LAYER_NAME)) {
                layer.entities.clear();
            }
            self.baue_import_overlay(projekt);
        }
        self.loesch_modus = false;
        self.status = format!("{anzahl} Punkt(e) gelöscht (FB:{fb} NP:{np} Imp:{imp}).");
        protokoll::log("LÖSCH", &format!("{anzahl} Punkte gelöscht"));
    }

    // ── Festpunkte ────────────────────────────────────────────────────────────

    fn festpunkte_dialog_oeffnen(&mut self, projekt: &Projekt) {
        let zeilen: Vec<FestpunktZeile> = projekt
            .festpunkt_kandidaten()
            .into_iter()
            .map(|e| {
                let vorhanden = projekt.festpunkt_existiert(&e.punkt_nr);
                FestpunktZeile { ergebnis: e, gewaehlt: !vorhanden, vorhanden }
            })
            .collect();
        self.dialog = Some(Dialog::Festpunkte { zeilen, rueckfrage: None, fehler: String::new() });
    }

    // ── Dialoge ───────────────────────────────────────────────────────────────

    pub(super) fn zeichne_dialog(&mut self, ctx: egui::Context, projekt: &mut Projekt, geraet: &mut Geraet) {
        let Some(dialog) = self.dialog.take() else {
            return;
        };
        let dialog = match dialog {
            Dialog::Hinweis { titel, text } if titel.to_lowercase().contains("fehler") => Dialog::Fehler { titel, text },
            anderer => anderer,
        };
        let fenster = |titel: &str| egui::Window::new(titel.to_string()).collapsible(false).resizable(false).anchor(Align2::CENTER_CENTER, Vec2::ZERO);
        match dialog {
            Dialog::Fehler { titel, text } => {
                let mut offen = true;
                let mut schliessen = false;
                egui::Window::new(&titel)
                    .id(egui::Id::unique("dxf_fehlerfenster"))
                    .open(&mut offen)
                    .collapsible(false)
                    .resizable(true)
                    .default_width(640.0)
                    .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
                    .frame(egui::Frame::window(&ctx.style_of(egui::Theme::Light)).fill(Color32::from_rgb(255, 247, 247)))
                    .show(&ctx, |ui| {
                        ui.label(RichText::new("Der Vorgang konnte nicht abgeschlossen werden.").strong().size(17.0).color(Color32::from_rgb(160, 30, 30)));
                        ui.add_space(8.0);
                        egui::ScrollArea::vertical().id_salt("dxf_fehlertext").max_height(360.0).show(ui, |ui| {
                            ui.add(egui::Label::new(RichText::new(&text).size(15.0).color(Color32::from_rgb(25, 25, 25))).wrap());
                        });
                        ui.add_space(12.0);
                        ui.horizontal(|ui| {
                            if ui.button("Fehlermeldung kopieren").clicked() {
                                ui.ctx().copy_text(format!("{titel}\n\n{text}"));
                            }
                            if ui.button("Schließen").clicked() {
                                schliessen = true;
                            }
                        });
                    });
                if offen && !schliessen {
                    self.dialog = Some(Dialog::Fehler { titel, text });
                }
            }
            Dialog::Hinweis { titel, text } => {
                let mut offen = true;
                fenster(&titel).show(&ctx, |ui| {
                    ui.label(&text);
                    ui.add_space(8.0);
                    if ui.button("OK").clicked() {
                        offen = false;
                    }
                });
                if offen {
                    self.dialog = Some(Dialog::Hinweis { titel, text });
                }
            }
            Dialog::DxfLeeren => {
                let mut antwort = None;
                fenster("DXF leeren").show(&ctx, |ui| {
                    ui.label("Die aktuelle DXF-Datei aus dem Viewer entfernen?\n(Datei auf der Festplatte bleibt erhalten.)");
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button("Ja").clicked() {
                            antwort = Some(true);
                        }
                        if ui.button("Nein").clicked() {
                            antwort = Some(false);
                        }
                    });
                });
                match antwort {
                    Some(true) => {
                        self.entities.clear();
                        self.marker.clear();
                        self.punkt_index = None;
                        self.dxf_pfad.clear();
                        self.auswahl = None;
                        self.status = "DXF-Inhalt geleert.".to_string();
                        self.aktualisiere_lokales_system(projekt);
                    }
                    Some(false) => {}
                    None => self.dialog = Some(Dialog::DxfLeeren),
                }
            }
            Dialog::IndexReset => {
                let mut antwort = None;
                fenster("Punkt-Index zurücksetzen").show(&ctx, |ui| {
                    ui.label(
                        "Alle DXF-Punkte erhalten neue Nummern.\n\n\
                         Bereits erstellte Protokolle, Messdaten und\n\
                         Stationierungen können dadurch nicht mehr den\n\
                         richtigen Punkten zugeordnet werden!\n\n\
                         Dies sollte nur getan werden, wenn noch keine\n\
                         Messungen mit den aktuellen Punktnummern\n\
                         durchgeführt wurden.",
                    );
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        let abbrechen = egui::Button::new(egui::RichText::new("Abbrechen").strong().color(Color32::WHITE))
                            .fill(Color32::from_rgb(52, 100, 175))
                            .min_size(Vec2::new(120.0, 30.0));
                        if ui.add(abbrechen).clicked() {
                            antwort = Some(false);
                        }
                        let reset =
                            egui::Button::new(egui::RichText::new("Zurücksetzen").color(Color32::WHITE)).fill(Color32::from_rgb(180, 50, 40)).min_size(Vec2::new(140.0, 30.0));
                        if ui.add(reset).clicked() {
                            antwort = Some(true);
                        }
                    });
                });
                match antwort {
                    Some(true) => self.punkt_index_zuruecksetzen(projekt),
                    Some(false) => {}
                    None => self.dialog = Some(Dialog::IndexReset),
                }
            }
            Dialog::Prisma { auswahl, konstante } => self.zeichne_prisma_dialog(&ctx, projekt, auswahl, konstante),
            Dialog::DxfExport { massstab, symbol, text } => self.zeichne_export_dialog(&ctx, projekt, massstab, symbol, text),
            Dialog::Messdaten(d) => self.zeichne_messdaten_dialog(&ctx, projekt, geraet, d),
            Dialog::Loeschen { text, anzahl, ziele } => {
                let mut antwort = None;
                fenster("Punkte löschen").show(&ctx, |ui| {
                    ui.label(&text);
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button("Nein").clicked() {
                            antwort = Some(false);
                        }
                        if ui.button("Ja").clicked() {
                            antwort = Some(true);
                        }
                    });
                });
                match antwort {
                    Some(true) => self.loesche(projekt, anzahl, ziele),
                    Some(false) => {}
                    None => self.dialog = Some(Dialog::Loeschen { text, anzahl, ziele }),
                }
            }
            Dialog::Festpunkte { mut zeilen, mut rueckfrage, mut fehler } => {
                let mut schliessen = false;
                let mut uebernehmen = false;
                let mut bestaetigt = false;
                fenster("Neupunkte als Festpunkte übernehmen").show(&ctx, |ui| {
                    ui.set_width(520.0);
                    if zeilen.is_empty() {
                        ui.label("Es sind keine gemessenen Neupunkte mit Koordinaten vorhanden.");
                    } else {
                        ui.label(
                            "Die gewählten Koordinaten werden als feste Anschlusspunkte nach Feldbuchpunkte.json \
                             kopiert (Quelle „Lokal“) und ändern sich danach nicht mehr automatisch.",
                        );
                        ui.add_space(6.0);
                        egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
                            for z in &mut zeilen {
                                let e = &z.ergebnis;
                                let hoehe = if e.ist_3d { f(e.hoehe, 3) } else { "-".to_string() };
                                let text = format!(
                                    "{:<8} {:<6} R {:>12}  H {:>12}  Hoehe {:>9}{}",
                                    e.punkt_nr,
                                    e.standpunkt_nr,
                                    f(e.r, 3),
                                    f(e.h, 3),
                                    hoehe,
                                    if z.vorhanden { "   [bereits Festpunkt]" } else { "" }
                                );
                                ui.checkbox(&mut z.gewaehlt, egui::RichText::new(text).monospace());
                            }
                        });
                    }
                    if !fehler.is_empty() {
                        ui.colored_label(Color32::RED, &fehler);
                    }
                    if let Some(frage) = &rueckfrage {
                        ui.separator();
                        ui.label(frage);
                        ui.horizontal(|ui| {
                            if ui.button("Nein").clicked() {
                                rueckfrage = None;
                            }
                            if ui.button("Ja").clicked() {
                                bestaetigt = true;
                            }
                        });
                    } else {
                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            let hat = !zeilen.is_empty();
                            if ui.add_enabled(hat, egui::Button::new("Alle wählen")).clicked() {
                                zeilen.iter_mut().for_each(|z| z.gewaehlt = true);
                            }
                            if ui.add_enabled(hat, egui::Button::new("Keine")).clicked() {
                                zeilen.iter_mut().for_each(|z| z.gewaehlt = false);
                            }
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if ui.button("Abbrechen").clicked() {
                                    schliessen = true;
                                }
                                if ui.add_enabled(hat, egui::Button::new("Übernehmen")).clicked() {
                                    uebernehmen = true;
                                }
                            });
                        });
                    }
                });

                if uebernehmen {
                    let gewaehlt: Vec<&FestpunktZeile> = zeilen.iter().filter(|z| z.gewaehlt).collect();
                    if gewaehlt.is_empty() {
                        schliessen = true;
                    } else {
                        let mut gesehen = std::collections::HashSet::new();
                        let doppelt: Vec<&str> = gewaehlt.iter().map(|z| z.ergebnis.punkt_nr.as_str()).filter(|nr| !gesehen.insert(*nr)).collect();
                        if !doppelt.is_empty() {
                            fehler = format!(
                                "Folgende Punktnummern sind mehrfach (von verschiedenen Standpunkten) gewählt: {}. \
                                 Bitte je Nummer nur einen Punkt wählen.",
                                doppelt.join(", ")
                            );
                        } else {
                            fehler.clear();
                            let ersetzen: Vec<&str> = gewaehlt.iter().filter(|z| projekt.festpunkt_existiert(&z.ergebnis.punkt_nr)).map(|z| z.ergebnis.punkt_nr.as_str()).collect();
                            if ersetzen.is_empty() {
                                bestaetigt = true;
                            } else {
                                let nummern = ersetzen.iter().take(10).copied().collect::<Vec<_>>().join(", ");
                                rueckfrage = Some(format!(
                                    "{} Festpunkt(e) sind bereits vorhanden und werden mit den aktuellen Koordinaten \
                                     überschrieben:\n{nummern}{}\n\nFortfahren?",
                                    ersetzen.len(),
                                    if ersetzen.len() > 10 { ", …" } else { "" }
                                ));
                            }
                        }
                    }
                }
                if bestaetigt {
                    let liste: Vec<NeupunktErgebnis> = zeilen.iter().filter(|z| z.gewaehlt).map(|z| z.ergebnis.clone()).collect();
                    match projekt.festpunkte_uebernehmen(&liste) {
                        Ok(erg) => {
                            self.status = format!("Festpunkte übernommen: {} neu, {} ersetzt (Feldbuchpunkte.json).", erg.neu, erg.ersetzt);
                            self.baue_overlay(projekt);
                        }
                        Err(e) => {
                            self.dialog = Some(Dialog::Hinweis { titel: "Festpunkte".into(), text: format!("Übernahme fehlgeschlagen:\n{e}") });
                        }
                    }
                    schliessen = true;
                }
                if !schliessen && self.dialog.is_none() {
                    self.dialog = Some(Dialog::Festpunkte { zeilen, rueckfrage, fehler });
                }
            }
        }
    }
}

/// Pfad der Begleit-CSV zu einer .kor-Datei.
fn kor_csv_pfad(kor: &Path) -> PathBuf {
    let stamm = kor.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    kor.with_file_name(format!("{stamm}-kor.csv"))
}

/// Nächste freie Punktnummer aus einer bestehenden KOR-Datei.
fn naechste_punkt_nr(kor: &Path) -> i32 {
    let Ok(text) = std::fs::read_to_string(kor) else {
        return 1;
    };
    let max =
        text.lines().filter(|z| !z.starts_with('%') && !z.trim().is_empty()).filter_map(|z| z.chars().take(8).collect::<String>().trim().parse::<i32>().ok()).max().unwrap_or(0);
    if max > 0 { max + 1 } else { 1 }
}

/// Nächstliegender „schöner" Wert für den Ellipsen-Maßstabsbalken [mm].
fn schoene_balken_mm(roh: f64) -> f64 {
    const SCHOEN: [f64; 13] = [0.1, 0.2, 0.5, 1.0, 2.0, 5.0, 10.0, 20.0, 50.0, 100.0, 200.0, 500.0, 1000.0];
    SCHOEN.iter().copied().min_by(|a, b| (roh - a).abs().total_cmp(&(roh - b).abs())).unwrap_or(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn begleit_csv_pfad() {
        assert_eq!(kor_csv_pfad(Path::new("C:/p/punkte.kor")), Path::new("C:/p/punkte-kor.csv"));
    }

    #[test]
    fn naechste_nummer_aus_kor() {
        let dir = std::env::temp_dir().join(format!("fb-kor-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let pfad = dir.join("x.kor");
        std::fs::write(&pfad, "% Kopf\n1             10.000          20.000     0.000    \n7             11.000          21.000     0.000    A\n").unwrap();
        assert_eq!(naechste_punkt_nr(&pfad), 8);
        assert_eq!(naechste_punkt_nr(&dir.join("fehlt.kor")), 1);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn balken_waehlt_naechsten_schoenen_wert() {
        assert_eq!(schoene_balken_mm(4.0), 5.0);
        assert_eq!(schoene_balken_mm(0.12), 0.1);
        assert_eq!(schoene_balken_mm(5000.0), 1000.0);
    }

    #[test]
    fn massstab_wird_geruendet() {
        assert_eq!(schoen_runden(7.4), 7);
        assert_eq!(schoen_runden(486.0), 490);
        assert_eq!(schoen_runden(2431.0), 2450);
        assert_eq!(schoen_runden(12_345.0), 12_300);
    }
}
