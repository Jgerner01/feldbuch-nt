// Zweck:    Formular Koordinatentransformation: Ausgangs- und Zielsystem, Passpunktzuordnung, Berechnung,
//           Residuen mit Ampel und Aktivierung je Punkt, Transformation aller Punkte, KOR-Export
// Hardware: keine
// Autor:    Johann Gerner

use std::collections::HashSet;

use eframe::egui::{self, Color32, RichText};

use super::tabelle::Raster;
use crate::daten::modelle::{ImportPunkt, jetzt};
use crate::protokolle::{schreibe_fuer_projekt, transformation};
use crate::daten::projekt::Projekt;
use crate::daten::punktlisten::{lese_csv, lese_kor};
use crate::grundlagen::format::{f, parse_zahl};
use crate::grundlagen::{datei_speicher, protokoll};
use crate::rechnen::koord_transformation::{self as kt, Ergebnis, Residuum, TransformPunkt, TransformationsTyp};

/// Anzahl leerer Zeilen beim Start.
const START_ZEILEN: usize = 8;
/// Spaltenbreiten der Punktlisten: Nr, X, Y, Z, Löschen.
const BREITEN_LISTE: [f32; 5] = [80.0, 100.0, 100.0, 80.0, 26.0];
/// Breite der Zeilenköpfe.
const ZEILENKOPF_BREITE: f32 = 30.0;
/// Ampelgrenzen [cm].
const AMPEL_GRUEN_CM: f64 = 1.0;
const AMPEL_GELB_CM: f64 = 3.0;
const AMPEL_TIEFROT_CM: f64 = 9.0;
/// Millimeter je Zentimeter.
const MM_PRO_CM: f64 = 10.0;
/// Millionstel (ppm).
const PPM: f64 = 1e6;
/// Zeilenfarben: Passpunkt und Neupunkt.
const FARBE_PASSPUNKT: Color32 = Color32::from_rgb(255, 210, 210);
const FARBE_NEUPUNKT: Color32 = Color32::from_rgb(200, 240, 200);
/// Farbe inaktiver Residuenzeilen.
const FARBE_INAKTIV: Color32 = Color32::from_rgb(228, 228, 228);
/// Code der Passpunkte im Export.
const CODE_PASSPUNKT: &str = "T";
/// Typen in der Reihenfolge des Auswahlfelds.
const TYPEN: [TransformationsTyp; 4] =
    [TransformationsTyp::Helmert2D, TransformationsTyp::Helmert3D, TransformationsTyp::Parameter7, TransformationsTyp::Parameter9];
/// Breiten der Residuentabelle: Aktiv, Z, Nr, vX, vY, vZ, vGes.
const BREITEN_RESIDUEN_3D: [f32; 7] = [44.0, 34.0, 80.0, 80.0, 80.0, 80.0, 80.0];
const BREITEN_RESIDUEN_2D: [f32; 5] = [44.0, 80.0, 80.0, 80.0, 80.0];

/// Eingabezeile einer Punktliste.
#[derive(Clone, Default)]
struct Zeile {
    nr: String,
    x: String,
    y: String,
    z: String,
}

/// Formular.
pub struct KoordTransformationAnsicht {
    typ_index: usize,
    fester_massstab: bool,
    quelle: Vec<Zeile>,
    ziel: Vec<Zeile>,
    ergebnis: Option<Ergebnis>,
    quelle_zuletzt: Vec<TransformPunkt>,
    ziel_zuletzt: Vec<TransformPunkt>,
    quelle_alle: Vec<TransformPunkt>,
    identische: HashSet<String>,
    aktiv: Vec<bool>,
    hoehe_aktiv: Vec<bool>,
    residuen: Vec<Option<Residuum>>,
    ziel_ist_resultat: bool,
    transformiert: Vec<TransformPunkt>,
    info: String,
    meldung: String,
    meldung_ist_fehler: bool,
}

fn schluessel(nr: &str) -> String {
    nr.trim().to_lowercase()
}

fn vorzeichen(wert: f64, stellen: usize) -> String {
    if wert < 0.0 { f(wert, stellen) } else { format!("+{}", f(wert, stellen)) }
}

/// Zeilen aus importierten Punkten.
fn zeilen_aus_punkten(punkte: &[ImportPunkt]) -> Vec<Zeile> {
    punkte
        .iter()
        .map(|p| Zeile {
            nr: p.punkt_nr.clone(),
            x: f(p.r, 3),
            y: f(p.h, 3),
            z: if p.hoehe != 0.0 { f(p.hoehe, 3) } else { String::new() },
        })
        .collect()
}

fn zeilen_aus_transform(punkte: &[TransformPunkt]) -> Vec<Zeile> {
    punkte
        .iter()
        .map(|p| Zeile {
            nr: p.punkt_nr.clone(),
            x: f(p.x, 3),
            y: f(p.y, 3),
            z: if p.z != 0.0 { f(p.z, 3) } else { String::new() },
        })
        .collect()
}

fn leere_zeilen() -> Vec<Zeile> {
    vec![Zeile::default(); START_ZEILEN]
}

/// Liest eine Liste; Zeilen ohne Nummer oder gültige X/Y werden übersprungen.
fn lese_punkte(zeilen: &[Zeile]) -> Vec<TransformPunkt> {
    zeilen
        .iter()
        .filter(|z| !z.nr.trim().is_empty())
        .filter_map(|z| {
            let (x, y) = (parse_zahl(&z.x)?, parse_zahl(&z.y)?);
            Some(TransformPunkt::neu(z.nr.trim(), x, y, parse_zahl(&z.z).unwrap_or(0.0)))
        })
        .collect()
}

/// Ampelfarbe für einen Betrag in cm: (Hintergrund, Schrift).
fn ampel(betrag_cm: f64) -> (Color32, Color32) {
    if betrag_cm <= AMPEL_GRUEN_CM {
        (Color32::from_rgb(200, 240, 200), Color32::BLACK)
    } else if betrag_cm <= AMPEL_GELB_CM {
        (Color32::from_rgb(255, 245, 165), Color32::BLACK)
    } else if betrag_cm <= AMPEL_TIEFROT_CM {
        (Color32::from_rgb(255, 185, 185), Color32::BLACK)
    } else {
        (Color32::from_rgb(180, 20, 20), Color32::WHITE)
    }
}

impl KoordTransformationAnsicht {
    pub fn neu(projekt: &Projekt) -> Self {
        let e = &projekt.einstellungen;
        Self {
            typ_index: usize::try_from(e.transform_typ_index).unwrap_or(0).min(TYPEN.len() - 1),
            fester_massstab: e.transform_fester_massstab,
            quelle: leere_zeilen(),
            ziel: leere_zeilen(),
            ergebnis: None,
            quelle_zuletzt: Vec::new(),
            ziel_zuletzt: Vec::new(),
            quelle_alle: Vec::new(),
            identische: HashSet::new(),
            aktiv: Vec::new(),
            hoehe_aktiv: Vec::new(),
            residuen: Vec::new(),
            ziel_ist_resultat: false,
            transformiert: Vec::new(),
            info: String::new(),
            meldung: String::new(),
            meldung_ist_fehler: false,
        }
    }

    fn typ(&self) -> TransformationsTyp {
        TYPEN[self.typ_index]
    }

    fn hinweis(&mut self, text: impl Into<String>) {
        self.meldung = text.into();
        self.meldung_ist_fehler = false;
    }

    fn fehler(&mut self, text: impl Into<String>) {
        self.meldung = text.into();
        self.meldung_ist_fehler = true;
    }

    fn speichere_einstellungen(&self, projekt: &mut Projekt) {
        projekt.einstellungen.transform_typ_index = self.typ_index as i32;
        projekt.einstellungen.transform_fester_massstab = self.fester_massstab;
        let _ = projekt.einstellungen.speichern();
    }

    // ── Import ────────────────────────────────────────────────────────────────

    fn importiere(&mut self, quelle: bool, nur_csv: bool) {
        let mut dialog = rfd::FileDialog::new().set_title(if nur_csv { "CSV-Datei importieren" } else { "KOR-Datei importieren" });
        dialog = if nur_csv {
            dialog.add_filter("CSV-Datei", &["csv"])
        } else {
            dialog.add_filter("KOR / KOR-CSV", &["kor", "csv"])
        };
        let Some(pfad) = dialog.add_filter("Alle Dateien", &["*"]).pick_file() else {
            return;
        };
        let ist_csv = nur_csv || pfad.extension().is_some_and(|e| e.eq_ignore_ascii_case("csv"));
        let gelesen = if ist_csv { lese_csv(&pfad) } else { lese_kor(&pfad) };
        match gelesen {
            Ok(punkte) => {
                let zeilen = zeilen_aus_punkten(&punkte);
                if quelle {
                    self.quelle = zeilen;
                } else {
                    self.ziel = zeilen;
                    self.ziel_ist_resultat = false;
                }
                self.hinweis(format!("{} Punkte importiert.", punkte.len()));
            }
            Err(fehler) => self.fehler(format!("Fehler beim Import:\n{fehler}")),
        }
    }

    /// Sortiert das Zielsystem nach der Reihenfolge der Passpunkte des Ausgangssystems.
    fn ordne_zu(&mut self) {
        let q = lese_punkte(&self.quelle);
        let z = lese_punkte(&self.ziel);
        if q.is_empty() || z.is_empty() {
            self.hinweis("Beide Listen müssen Punkte enthalten.");
            return;
        }
        let mut gefunden = Vec::new();
        let mut fehlend = Vec::new();
        for qp in &q {
            match z.iter().find(|p| schluessel(&p.punkt_nr) == schluessel(&qp.punkt_nr)) {
                Some(p) => gefunden.push(p.clone()),
                None => fehlend.push(qp.punkt_nr.clone()),
            }
        }
        if gefunden.is_empty() {
            self.fehler("Keine übereinstimmenden Punktnummern gefunden.");
            return;
        }
        self.ziel = zeilen_aus_transform(&gefunden);
        self.ziel_ist_resultat = false;
        let mut text = format!("{} Passpunkte im Zielsystem zugeordnet.\nAusgangssystem ({} Punkte) unverändert.", gefunden.len(), q.len());
        if !fehlend.is_empty() {
            text.push_str(&format!("\nNicht im Zielsystem gefunden: {}", fehlend.join(", ")));
        }
        self.hinweis(text);
    }

    // ── Berechnung ────────────────────────────────────────────────────────────

    fn berechne(&mut self) {
        if self.ziel_ist_resultat && !self.quelle_zuletzt.is_empty() {
            self.neuberechnen();
            return;
        }
        let alle_quelle = lese_punkte(&self.quelle);
        let alle_ziel = lese_punkte(&self.ziel);
        if alle_quelle.is_empty() || alle_ziel.is_empty() {
            self.fehler("Bitte Punkte in beiden Listen eingeben.");
            return;
        }
        let mut quelle = Vec::new();
        let mut ziel = Vec::new();
        for q in &alle_quelle {
            if let Some(z) = alle_ziel.iter().find(|p| schluessel(&p.punkt_nr) == schluessel(&q.punkt_nr)) {
                quelle.push(q.clone());
                ziel.push(z.clone());
            }
        }
        if quelle.is_empty() {
            self.fehler("Keine übereinstimmenden Punktnummern gefunden.\nBitte sicherstellen, dass Passpunkte in beiden Listen gleiche Nummern haben.");
            return;
        }
        self.identische = quelle.iter().map(|p| schluessel(&p.punkt_nr)).collect();
        self.aktiv = vec![true; quelle.len()];
        self.hoehe_aktiv = vec![true; quelle.len()];
        self.quelle_zuletzt = quelle;
        self.ziel_zuletzt = ziel;
        self.quelle_alle = alle_quelle;
        self.neuberechnen();
    }

    /// Rechnet mit den aktuell aktiven Passpunkten neu und transformiert alle Punkte.
    fn neuberechnen(&mut self) {
        if self.quelle_zuletzt.is_empty() {
            return;
        }
        let typ = self.typ();
        let ist_3d = typ != TransformationsTyp::Helmert2D;
        let mut quelle = Vec::new();
        let mut ziel = Vec::new();
        let mut indizes = Vec::new();
        for (i, q) in self.quelle_zuletzt.iter().enumerate() {
            if !self.aktiv[i] {
                continue;
            }
            let mut p = q.clone();
            p.use_z = ist_3d && self.hoehe_aktiv[i];
            quelle.push(p);
            ziel.push(self.ziel_zuletzt[i].clone());
            indizes.push(i);
        }
        if quelle.is_empty() {
            self.info = "Keine aktiven Punkte für Berechnung.".to_string();
            return;
        }
        match kt::berechnen(typ, &quelle, &ziel, self.fester_massstab) {
            Err(fehler) => {
                self.info = format!("Fehler: {fehler}");
            }
            Ok(erg) => {
                self.meldung.clear();
                self.residuen = vec![None; self.quelle_zuletzt.len()];
                for (k, i) in indizes.iter().enumerate() {
                    self.residuen[*i] = Some(erg.residuen[k].clone());
                }
                self.info = info_text(&erg, typ);
                self.ergebnis = Some(erg);
                self.transformiere_alle();
            }
        }
    }

    /// Überträgt alle Punkte des Ausgangssystems ins Zielsystem.
    fn transformiere_alle(&mut self) {
        let Some(erg) = &self.ergebnis else {
            return;
        };
        if self.quelle_alle.is_empty() {
            return;
        }
        let (typ, parameter) = (erg.typ, erg.parameter.clone());
        self.transformiert = self.quelle_alle.iter().map(|p| kt::transformiere(typ, &parameter, p)).collect();
        let identisch = self.transformiert.iter().filter(|p| self.identische.contains(&schluessel(&p.punkt_nr))).count();
        self.ziel = zeilen_aus_transform(&self.transformiert);
        self.ziel_ist_resultat = true;
        protokoll::log(
            "TRANSF",
            &format!("{}: {identisch} Passpunkte (rot), {} Neupunkte transformiert (grün)", typ.name(), self.transformiert.len() - identisch),
        );
    }

    /// Schreibt das RTF-Protokoll der letzten Berechnung.
    fn schreibe_protokoll(&mut self, projekt: &Projekt) {
        let Some(erg) = &self.ergebnis else {
            return;
        };
        let alle = (!self.transformiert.is_empty()).then_some(self.transformiert.as_slice());
        let p = transformation::protokoll(
            erg,
            &self.quelle_zuletzt,
            &self.ziel_zuletzt,
            alle,
            &projekt.einstellungen.projekt_name,
            &projekt.projektdaten.bearbeiter,
            jetzt(),
        );
        match schreibe_fuer_projekt(&p, projekt) {
            Ok(pfad) => self.hinweis(format!("Protokoll gespeichert:\n{}", pfad.display())),
            Err(text) => self.fehler(text),
        }
    }

    // ── Export ────────────────────────────────────────────────────────────────

    fn exportiere(&mut self, projekt: &Projekt) {
        let Some(erg) = &self.ergebnis else {
            return;
        };
        let punkte = if self.transformiert.is_empty() { erg.transformierte_punkte.clone() } else { self.transformiert.clone() };
        if punkte.is_empty() {
            return;
        }
        let mut dialog = rfd::FileDialog::new()
            .set_title("Transformierte Punkte speichern")
            .add_filter("KOR-Datei", &["kor"])
            .set_file_name("Transformiert.kor");
        if projekt.einstellungen.ist_geladen() {
            dialog = dialog.set_directory(&projekt.einstellungen.projekt_verzeichnis);
        }
        let Some(pfad) = dialog.save_file() else {
            return;
        };
        let jetzt = chrono::Local::now();
        let typ_name = erg.typ.name();
        let mut kor = format!("% Koordinatentransformation  {}\r\n% Typ: {typ_name}\r\n", jetzt.format("%Y-%m-%d %H:%M"));
        let mut csv = format!(
            "# METADATA\r\nProjekt: {}\r\nSensor: {typ_name}\r\nBearbeiter: \r\nDatum: {}\r\n---\r\n# DATENTYP\r\nKoordinaten\r\n# DATEN\r\nPunktNr; X; Y; Z; Code\r\n",
            projekt.einstellungen.projekt_name,
            jetzt.format("%Y-%m-%d")
        );
        for p in &punkte {
            let code = if self.identische.contains(&schluessel(&p.punkt_nr)) { CODE_PASSPUNKT } else { "" };
            kor.push_str(&format!("{:<8}{:>16}{:>16}{:>10}    {code}\r\n", p.punkt_nr, f(p.x, 3), f(p.y, 3), f(p.z, 3)));
            csv.push_str(&format!("{}; {}; {}; {}; {code}\r\n", p.punkt_nr, f(p.x, 3), f(p.y, 3), f(p.z, 3)));
        }
        let stamm = pfad.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let csv_pfad = pfad.with_file_name(format!("{stamm}-kor.csv"));
        let ergebnis = datei_speicher::schreibe_text(&pfad, &kor).and_then(|()| datei_speicher::schreibe_text(&csv_pfad, &csv));
        match ergebnis {
            Ok(()) => self.hinweis(format!("Exportiert:\n{}\n{}", pfad.display(), csv_pfad.display())),
            Err(fehler) => self.fehler(format!("Fehler beim Export:\n{fehler}")),
        }
    }

    // ── Darstellung ───────────────────────────────────────────────────────────

    pub fn zeichne(&mut self, ui: &mut egui::Ui, projekt: &mut Projekt) {
        ui.heading("Koordinatentransformation");
        ui.label("Links Ausgangssystem, rechts Zielsystem. Passpunkte werden über gleiche Punktnummern zugeordnet.");
        ui.add_space(4.0);

        let mut neu_rechnen = false;
        ui.horizontal(|ui| {
            ui.label("Transformation:");
            let vorher = self.typ_index;
            egui::ComboBox::from_id_salt("transformationstyp")
                .selected_text(self.typ().name())
                .show_ui(ui, |ui| {
                    for (i, t) in TYPEN.iter().enumerate() {
                        ui.selectable_value(&mut self.typ_index, i, t.name());
                    }
                });
            if vorher != self.typ_index {
                self.speichere_einstellungen(projekt);
                neu_rechnen |= self.ziel_ist_resultat && self.ergebnis.is_some();
            }
            if ui.checkbox(&mut self.fester_massstab, "Fester Maßstab (m = 1)").changed() {
                self.speichere_einstellungen(projekt);
                neu_rechnen |= self.ergebnis.is_some();
            }
        });
        ui.add_space(4.0);

        let mut ziel_geaendert = false;
        let faerbe = self.ergebnis.is_some();
        ui.columns(2, |spalten| {
            spalten[0].label(RichText::new("Ausgangssystem").strong());
            zeichne_liste(&mut spalten[0], "quelle", &mut self.quelle, faerbe.then_some(&self.identische), &mut false);
            spalten[0].horizontal(|ui| {
                if ui.button("KOR laden …").clicked() {
                    self.importiere(true, false);
                }
                if ui.button("CSV laden …").clicked() {
                    self.importiere(true, true);
                }
                if ui.button("Leeren").clicked() {
                    self.quelle = leere_zeilen();
                }
            });
            spalten[1].label(RichText::new("Zielsystem").strong());
            let identisch = (faerbe && self.ziel_ist_resultat).then_some(&self.identische);
            zeichne_liste(&mut spalten[1], "ziel", &mut self.ziel, identisch, &mut ziel_geaendert);
            spalten[1].horizontal(|ui| {
                if ui.button("KOR laden …").clicked() {
                    self.importiere(false, false);
                }
                if ui.button("CSV laden …").clicked() {
                    self.importiere(false, true);
                }
                if ui.button("Leeren").clicked() {
                    self.ziel = leere_zeilen();
                    self.ziel_ist_resultat = false;
                }
            });
        });
        if ziel_geaendert {
            self.ziel_ist_resultat = false;
        }

        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui.button("Punkte zuordnen").clicked() {
                self.ordne_zu();
            }
            if ui.button(RichText::new("Berechnen").strong()).clicked() {
                self.berechne();
            }
            if ui.add_enabled(self.ergebnis.is_some(), egui::Button::new("Transformieren →")).clicked() {
                self.transformiere_alle();
            }
            if ui.add_enabled(self.ergebnis.is_some(), egui::Button::new("Export KOR …")).clicked() {
                self.exportiere(projekt);
            }
            if ui.add_enabled(self.ergebnis.is_some(), egui::Button::new("Protokoll")).clicked() {
                self.schreibe_protokoll(projekt);
            }
        });
        if neu_rechnen {
            self.neuberechnen();
        }
        if !self.meldung.is_empty() {
            let farbe = if self.meldung_ist_fehler { Color32::from_rgb(180, 40, 40) } else { ui.visuals().text_color() };
            ui.colored_label(farbe, &self.meldung);
        }
        self.zeichne_ergebnis(ui);
    }

    fn zeichne_ergebnis(&mut self, ui: &mut egui::Ui) {
        if self.ergebnis.is_none() && self.info.is_empty() {
            return;
        }
        ui.add_space(8.0);
        ui.separator();
        ui.label(RichText::new(&self.info).monospace());
        if self.ergebnis.is_none() {
            return;
        }
        let ist_3d = self.typ() != TransformationsTyp::Helmert2D;
        let breiten: &[f32] = if ist_3d { &BREITEN_RESIDUEN_3D } else { &BREITEN_RESIDUEN_2D };
        let raster = Raster::neu(breiten);
        let kopf: &[&str] = if ist_3d {
            &["Aktiv", "Z", "PunktNr", "vX [cm]", "vY [cm]", "vZ [cm]", "vGes. [cm]"]
        } else {
            &["Aktiv", "PunktNr", "vX [cm]", "vY [cm]", "vGes. [cm]"]
        };
        ui.add_space(4.0);
        raster.kopf(ui, kopf);
        let vorher = (self.aktiv.clone(), self.hoehe_aktiv.clone());
        for i in 0..self.quelle_zuletzt.len() {
            let aktiv = self.aktiv[i];
            let res = self.residuen.get(i).cloned().flatten().filter(|_| aktiv);
            let hintergrund = (!aktiv).then_some(FARBE_INAKTIV);
            let nr = self.quelle_zuletzt[i].punkt_nr.clone();
            let (aktiv_bit, hoehe_bit) = (&mut self.aktiv[i], &mut self.hoehe_aktiv[i]);
            raster.zeile_farbig(ui, i, "", hintergrund, |z| {
                z.haken(aktiv_bit);
                if ist_3d {
                    z.haken(hoehe_bit);
                }
                z.text(&nr);
                zeichne_residuum(z, res.as_ref(), ist_3d);
            });
        }
        if vorher != (self.aktiv.clone(), self.hoehe_aktiv.clone()) {
            self.neuberechnen();
        }
    }
}

/// Zeichnet die Residuenzellen einer Zeile.
fn zeichne_residuum(z: &mut super::tabelle::Zellen<'_>, res: Option<&Residuum>, ist_3d: bool) {
    let Some(r) = res else {
        z.zahl("–");
        z.zahl("–");
        if ist_3d {
            z.zahl("–");
        }
        z.zahl("–");
        return;
    };
    farbige_zahl(z, vorzeichen(r.vx_mm / MM_PRO_CM, 1), r.vx_mm.abs() / MM_PRO_CM);
    farbige_zahl(z, vorzeichen(r.vy_mm / MM_PRO_CM, 1), r.vy_mm.abs() / MM_PRO_CM);
    if ist_3d {
        if r.hoehe_aktiv {
            farbige_zahl(z, vorzeichen(r.vz_mm / MM_PRO_CM, 1), r.vz_mm.abs() / MM_PRO_CM);
        } else {
            z.zahl_farbig("–", Color32::GRAY);
        }
    }
    farbige_zahl(z, f(r.v_gesamt_mm / MM_PRO_CM, 1), r.v_gesamt_mm / MM_PRO_CM);
}

fn farbige_zahl(z: &mut super::tabelle::Zellen<'_>, text: String, betrag_cm: f64) {
    let (hintergrund, schrift) = ampel(betrag_cm);
    z.zelle(|ui| {
        ui.painter().rect_filled(ui.max_rect().expand(1.0), 0.0, hintergrund);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(RichText::new(text).monospace().color(schrift));
        });
    });
}

/// Punktliste als Tabelle mit optionaler Färbung (Passpunkt rot, Neupunkt grün).
fn zeichne_liste(ui: &mut egui::Ui, id: &str, zeilen: &mut Vec<Zeile>, faerbung: Option<&HashSet<String>>, geaendert: &mut bool) {
    let raster = Raster::neu(&BREITEN_LISTE).mit_zeilenkopf(ZEILENKOPF_BREITE);
    ui.push_id(id, |ui| {
        raster.kopf(ui, &["PunktNr", "X (R) [m]", "Y (H) [m]", "Z [m]", ""]);
        let mut loeschen = None;
        egui::ScrollArea::vertical().max_height(260.0).id_salt(id).show(ui, |ui| {
            for (i, zeile) in zeilen.iter_mut().enumerate() {
                let hintergrund = faerbung.and_then(|menge| {
                    (!zeile.nr.trim().is_empty()).then(|| if menge.contains(&schluessel(&zeile.nr)) { FARBE_PASSPUNKT } else { FARBE_NEUPUNKT })
                });
                raster.zeile_farbig(ui, i, &(i + 1).to_string(), hintergrund, |z| {
                    *geaendert |= z.eingabe(&mut zeile.nr);
                    *geaendert |= z.eingabe(&mut zeile.x);
                    *geaendert |= z.eingabe(&mut zeile.y);
                    *geaendert |= z.eingabe(&mut zeile.z);
                    if z.knopf("✕") {
                        loeschen = Some(i);
                    }
                });
            }
        });
        if let Some(i) = loeschen {
            zeilen.remove(i);
            *geaendert = true;
        }
        if ui.small_button("+ Zeile").clicked() {
            zeilen.push(Zeile::default());
        }
    });
}

/// Textzeile mit Parametern und Genauigkeit.
fn info_text(erg: &Ergebnis, typ: TransformationsTyp) -> String {
    let p = &erg.parameter;
    let translation = format!("  dx = {} m    dy = {} m", vorzeichen(p.dx, 3), vorzeichen(p.dy, 3));
    let info = match typ {
        TransformationsTyp::Helmert2D => format!("{translation}    α = {} gon    m = {}", f(p.alpha_gon, 5), f(p.massstab_2d, 8)),
        TransformationsTyp::Parameter9 => format!(
            "{translation}    dz = {} m\n  mx = {} ppm    my = {} ppm    mz = {} ppm",
            vorzeichen(p.dz, 3),
            vorzeichen(p.mx * PPM, 2),
            vorzeichen(p.my * PPM, 2),
            vorzeichen(p.mz * PPM, 2)
        ),
        _ => format!("{translation}    dz = {} m\n  m  = {} ppm", vorzeichen(p.dz, 3), vorzeichen(p.m * PPM, 2)),
    };
    let genauigkeit = if erg.redundanz > 0 {
        format!("   |   s₀ = {} mm   (r = {})", f(erg.s0_mm, 2), erg.redundanz)
    } else {
        "   |   eindeutig bestimmt (r = 0)".to_string()
    };
    info + &genauigkeit
}

#[cfg(test)]
mod tests {
    use super::*;

    fn zeile(nr: &str, x: &str, y: &str, z: &str) -> Zeile {
        Zeile { nr: nr.into(), x: x.into(), y: y.into(), z: z.into() }
    }

    #[test]
    fn liest_punkte_und_ueberspringt_ungueltige() {
        let zeilen = [zeile("A", "1,5", "2.25", ""), zeile("", "1", "2", ""), zeile("B", "x", "2", ""), zeile("C", "3", "4", "5.5")];
        let p = lese_punkte(&zeilen);
        assert_eq!(p.len(), 2);
        assert_eq!(p[0].x, 1.5);
        assert_eq!(p[1].z, 5.5);
    }

    #[test]
    fn zuordnung_und_berechnung() {
        let mut ansicht = KoordTransformationAnsicht {
            typ_index: 0,
            fester_massstab: false,
            quelle: vec![zeile("A", "0", "0", ""), zeile("B", "100", "0", ""), zeile("C", "0", "100", ""), zeile("N", "50", "50", "")],
            ziel: vec![zeile("c", "1000", "1100", ""), zeile("a", "1000", "1000", ""), zeile("b", "1100", "1000", "")],
            ergebnis: None,
            quelle_zuletzt: Vec::new(),
            ziel_zuletzt: Vec::new(),
            quelle_alle: Vec::new(),
            identische: HashSet::new(),
            aktiv: Vec::new(),
            hoehe_aktiv: Vec::new(),
            residuen: Vec::new(),
            ziel_ist_resultat: false,
            transformiert: Vec::new(),
            info: String::new(),
            meldung: String::new(),
            meldung_ist_fehler: false,
        };
        ansicht.berechne();
        let erg = ansicht.ergebnis.as_ref().expect("Ergebnis");
        assert!(erg.s0_mm < 1e-6);
        assert_eq!(ansicht.quelle_zuletzt.len(), 3);
        let neu = ansicht.transformiert.iter().find(|p| p.punkt_nr == "N").unwrap();
        assert!((neu.x - 1050.0).abs() < 1e-6 && (neu.y - 1050.0).abs() < 1e-6);
        assert!(ansicht.ziel_ist_resultat);
        // Punkt abwählen: Rechnung mit zwei Punkten bleibt möglich
        ansicht.aktiv[2] = false;
        ansicht.neuberechnen();
        assert!(ansicht.residuen[2].is_none());
        assert!(ansicht.info.contains("r = 1") || ansicht.info.contains("(r = "));
    }
}
