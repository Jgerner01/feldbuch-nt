// Zweck:    Fenster „Aufnahmetest“: Koordinaten messen (lokales System, Instrument = Ursprung), Tabelle bearbeiten,
//           Punkte motorisch anfahren und als KOR/DAT exportieren
// Hardware: Tachymeter über gui::geraet (Messen: GeoCOM/GSI Online, Anfahren: nur GeoCOM, motorisiert)
// Autor:    Johann Gerner

use std::path::PathBuf;

use eframe::egui::{self, Color32, RichText};

use super::geraet::Geraet;
use super::tabelle::Raster;
use crate::daten::aufnahmetest::{self as daten, AufnahmePunkt, ExportKopf, V_WAAGERECHT};
use crate::daten::modelle::jetzt;
use crate::daten::projekt::Projekt;
use crate::grundlagen::format::{f, parse_zahl};
use crate::grundlagen::{datei_speicher, fehler_log, pfade};
use crate::tachymeter::messablauf::MessErgebnis;

/// Spaltenbreiten: Wahl, Nr, PunktNr, Code, X, Y, Z, D, Hz, V.
const BREITEN: [f32; 10] = [34.0, 38.0, 80.0, 70.0, 90.0, 90.0, 80.0, 75.0, 90.0, 80.0];
/// Höhe des Tabellenbereichs.
const TABELLEN_HOEHE: f32 = 320.0;
const FARBE_OK: Color32 = Color32::from_rgb(40, 150, 60);
const FARBE_WARNUNG: Color32 = Color32::from_rgb(190, 130, 20);
const FARBE_FEHLER: Color32 = Color32::from_rgb(190, 50, 40);
const FARBE_INFO: Color32 = Color32::from_rgb(30, 110, 170);
const FARBE_AUSWAHL: Color32 = Color32::from_rgb(255, 240, 170);
/// Anzahl der Nachkommastellen in der Tabelle.
const STELLEN_KOORD: usize = 4;

/// Eine Tabellenzeile: der Punkt und die Eingabetexte der bearbeitbaren Zahlen.
struct Zeile {
    punkt: AufnahmePunkt,
    x: String,
    y: String,
    z: String,
}

impl Zeile {
    fn neu(punkt: AufnahmePunkt) -> Self {
        let mut z = Self { punkt, x: String::new(), y: String::new(), z: String::new() };
        z.setze_texte();
        z
    }

    fn setze_texte(&mut self) {
        self.x = f(self.punkt.x, STELLEN_KOORD);
        self.y = f(self.punkt.y, STELLEN_KOORD);
        self.z = f(self.punkt.z, STELLEN_KOORD);
    }

    /// Übernimmt gültige Texte in den Punkt und berechnet Strecke und Richtung neu.
    fn uebernimm_texte(&mut self) {
        if let (Some(x), Some(y), Some(z)) = (parse_zahl(&self.x), parse_zahl(&self.y), parse_zahl(&self.z)) {
            self.punkt.x = x;
            self.punkt.y = y;
            self.punkt.z = z;
            self.punkt.berechne_richtung();
        }
    }
}

/// Meldung mit Farbe.
#[derive(Clone, Default)]
struct Hinweis {
    text: String,
    farbe: Option<Color32>,
}

impl Hinweis {
    fn neu(text: impl Into<String>, farbe: Color32) -> Self {
        Self { text: text.into(), farbe: Some(farbe) }
    }
}

/// Die Ansicht „Aufnahmetest“.
pub struct AufnahmetestAnsicht {
    zeilen: Vec<Zeile>,
    datei: PathBuf,
    projekt_name: String,
    bearbeiter: String,
    export_ordner: PathBuf,
    punkt_nr: String,
    code: String,
    v_neu: String,
    auswahl: Option<usize>,
    messen_wartet: bool,
    mess_info: Hinweis,
    anfahr_info: Hinweis,
    meldung: String,
    geaendert: bool,
}

impl Drop for AufnahmetestAnsicht {
    fn drop(&mut self) {
        self.speichere();
    }
}

impl AufnahmetestAnsicht {
    pub fn neu(projekt: &Projekt) -> Self {
        let verzeichnis = projekt.einstellungen.ist_geladen().then(|| projekt.einstellungen.projekt_verzeichnis.clone());
        let datei = daten::pfad(verzeichnis.as_deref(), &pfade::basis());
        let zeilen = daten::lade(&datei).into_iter().map(Zeile::neu).collect();
        Self {
            zeilen,
            datei,
            projekt_name: projekt.einstellungen.projekt_name.clone(),
            bearbeiter: projekt.projektdaten.bearbeiter.clone(),
            export_ordner: verzeichnis.unwrap_or_else(pfade::basis),
            punkt_nr: String::new(),
            code: String::new(),
            v_neu: f(V_WAAGERECHT, 4),
            auswahl: None,
            messen_wartet: false,
            mess_info: Hinweis::default(),
            anfahr_info: Hinweis::default(),
            meldung: String::new(),
            geaendert: false,
        }
    }

    fn speichere(&mut self) {
        let punkte: Vec<AufnahmePunkt> = self.zeilen.iter().map(|z| z.punkt.clone()).collect();
        match daten::speichere(&self.datei, &punkte) {
            Ok(()) => self.geaendert = false,
            Err(fehler) => fehler_log::log("Aufnahmetest.speichern", &fehler.to_string()),
        }
    }

    fn nummeriere(&mut self) {
        for (i, z) in self.zeilen.iter_mut().enumerate() {
            z.punkt.nr = i as i64 + 1;
        }
    }

    /// Fügt einen gemessenen Punkt an und zählt eine numerische Punktnummer hoch.
    fn uebernimm_messung(&mut self, hz: f64, v: f64, d: f64) {
        let nr = self.zeilen.len() as i64 + 1;
        let mut punkt_nr = self.punkt_nr.trim().to_string();
        if punkt_nr.is_empty() {
            punkt_nr = format!("P{nr:03}");
        }
        let punkt = AufnahmePunkt::aus_messung(nr, &punkt_nr, self.code.trim(), hz, v, d);
        self.mess_info = Hinweis::neu(
            format!(
                "Hz={} gon   V={} gon   D={} m   →  X={}  Y={}  Z={}",
                f(hz, 4),
                f(v, 4),
                f(d, 3),
                f(punkt.x, 4),
                f(punkt.y, 4),
                f(punkt.z, 4)
            ),
            FARBE_OK,
        );
        self.zeilen.push(Zeile::neu(punkt));
        if let Ok(zahl) = punkt_nr.parse::<i64>() {
            self.punkt_nr = (zahl + 1).to_string();
        }
        self.auswahl = Some(self.zeilen.len() - 1);
        self.speichere();
    }

    fn verarbeite_ergebnis(&mut self, ergebnis: MessErgebnis) {
        self.messen_wartet = false;
        match ergebnis {
            MessErgebnis::Messung(m) => match (m.hz_gon, m.v_gon, m.schraegstrecke_m) {
                (Some(hz), Some(v), Some(d)) => self.uebernimm_messung(hz, v, d),
                _ => self.mess_info = Hinweis::neu("Unvollständige Messung (Hz/V/D fehlen).", FARBE_WARNUNG),
            },
            MessErgebnis::Fehler(text) => self.mess_info = Hinweis::neu(format!("Messfehler: {text}"), FARBE_FEHLER),
        }
    }

    /// Wählt den Punkt mit dieser Nummer und startet das Anfahren; `false`, wenn er in der Tabelle fehlt.
    pub fn anfahren_nach(&mut self, punkt_nr: &str, geraet: &mut Geraet) -> bool {
        let Some(index) = self.zeilen.iter().position(|z| z.punkt.punkt_nr == punkt_nr) else {
            self.anfahr_info = Hinweis::neu(format!("Punkt '{punkt_nr}' ist nicht in der Tabelle des Aufnahmetests."), FARBE_FEHLER);
            return false;
        };
        self.auswahl = Some(index);
        self.starte_anfahren(geraet);
        true
    }

    fn starte_anfahren(&mut self, geraet: &mut Geraet) {
        let Some(zeile) = self.auswahl.and_then(|i| self.zeilen.get(i)) else {
            self.anfahr_info = Hinweis::neu("Keine Zeile ausgewählt.", FARBE_FEHLER);
            return;
        };
        match geraet.starte_anfahren(zeile.punkt.hz, zeile.punkt.v) {
            Ok(()) => self.anfahr_info = Hinweis::neu("Lese Ist-Position …", FARBE_WARNUNG),
            Err(fehler) => self.anfahr_info = Hinweis::neu(fehler, FARBE_FEHLER),
        }
    }

    fn exportiere(&mut self, geraet: &Geraet) {
        if self.zeilen.is_empty() {
            self.meldung = "Keine Messpunkte vorhanden.".to_string();
            return;
        }
        let Some(kor) = rfd::FileDialog::new()
            .set_title("KOR-Datei speichern")
            .add_filter("CSV-Koordinaten", &["csv"])
            .add_filter("Alle Dateien", &["*"])
            .set_directory(&self.export_ordner)
            .set_file_name("aufnahme-kor.csv")
            .save_file()
        else {
            return;
        };
        let (kor, dat) = daten::export_pfade(&kor);
        let jetzt = jetzt();
        let datum = jetzt.format("%Y-%m-%d").to_string();
        let zeit = jetzt.format("%Y-%m-%d %H:%M").to_string();
        let kopf = ExportKopf {
            projekt: &self.projekt_name,
            sensor: geraet.modell.anzeige(),
            bearbeiter: &self.bearbeiter,
            datum: &datum,
            zeitstempel: &zeit,
        };
        let punkte: Vec<AufnahmePunkt> = self.zeilen.iter().map(|z| z.punkt.clone()).collect();
        let ergebnis = datei_speicher::schreibe_zeilen(&kor, daten::kor_zeilen(&punkte, &kopf))
            .and_then(|()| datei_speicher::schreibe_zeilen(&dat, daten::dat_zeilen(&punkte, &kopf)));
        self.meldung = match ergebnis {
            Ok(()) => format!(
                "Export abgeschlossen:\n  {}\n  {}",
                kor.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
                dat.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
            ),
            Err(fehler) => format!("Exportfehler:\n{fehler}"),
        };
    }

    pub fn zeichne(&mut self, ui: &mut egui::Ui, geraet: &mut Geraet) {
        // Ergebnisse der Geräteabläufe abholen
        if self.messen_wartet {
            if let Some(ergebnis) = geraet.nimm_messergebnis() {
                self.verarbeite_ergebnis(ergebnis);
            } else if let Some(text) = geraet.messung_status() {
                self.mess_info = Hinweis::neu(text, FARBE_WARNUNG);
            }
        }
        if let Some(e) = geraet.nimm_anfahr_ergebnis() {
            self.anfahr_info = Hinweis::neu(e.text, if e.ok { FARBE_INFO } else { FARBE_FEHLER });
        } else if let Some(status) = geraet.anfahren_status() {
            self.anfahr_info = Hinweis::neu(status, FARBE_WARNUNG);
        }
        if self.messen_wartet || geraet.anfahren.is_some() {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
        }

        ui.heading("Aufnahmetest");
        let verbunden = geraet.ist_verbunden();
        if verbunden {
            ui.colored_label(Color32::from_rgb(40, 140, 60), geraet.modell.anzeige());
        } else {
            ui.colored_label(FARBE_FEHLER, "Nicht verbunden");
        }
        ui.add_space(4.0);

        let frei = verbunden && !geraet.beschaeftigt() && !self.messen_wartet;
        ui.horizontal(|ui| {
            ui.label("Punktnummer:");
            ui.add(egui::TextEdit::singleline(&mut self.punkt_nr).desired_width(80.0));
            ui.label("Code:");
            ui.add(egui::TextEdit::singleline(&mut self.code).desired_width(70.0));
            let messen = egui::Button::new(RichText::new("► Messen").strong().color(Color32::WHITE)).fill(Color32::from_rgb(160, 60, 20));
            if ui.add_enabled(frei, messen).clicked() {
                match geraet.starte_messung(0.0) {
                    Ok(()) => {
                        self.messen_wartet = true;
                        self.mess_info = Hinweis::neu("Messung läuft …", FARBE_WARNUNG);
                    }
                    Err(fehler) => self.mess_info = Hinweis::neu(fehler, FARBE_FEHLER),
                }
            }
        });
        if !verbunden && self.mess_info.text.is_empty() {
            self.mess_info = Hinweis::neu("Kein Tachymeter verbunden.", FARBE_FEHLER);
        }
        zeige(ui, &self.mess_info);

        ui.horizontal(|ui| {
            let anfahren = egui::Button::new(RichText::new("Anfahren").strong().color(Color32::WHITE)).fill(Color32::from_rgb(30, 100, 160));
            if ui.add_enabled(frei && self.auswahl.is_some(), anfahren).clicked() {
                self.starte_anfahren(geraet);
            }
            let stopp = egui::Button::new(RichText::new("Stopp").strong().color(Color32::WHITE)).fill(Color32::from_rgb(160, 40, 40));
            if ui.add_enabled(geraet.anfahren.is_some(), stopp).clicked() {
                geraet.stoppe_anfahren();
                self.anfahr_info = Hinweis::neu("Abgebrochen (Gerät fährt ggf. noch).", FARBE_WARNUNG);
            }
            ui.label("Das Gerät fährt motorisch auf Hz/V der gewählten Zeile.");
        });
        zeige(ui, &self.anfahr_info);
        ui.separator();
        self.zeichne_tabelle(ui);
        ui.horizontal(|ui| {
            ui.label("V neue Zeile [gon]:");
            ui.add(egui::TextEdit::singleline(&mut self.v_neu).desired_width(70.0));
            if ui.button("Zeile hinzufügen").clicked() {
                let nr = self.zeilen.len() as i64 + 1;
                self.zeilen.push(Zeile::neu(AufnahmePunkt::leer(nr, parse_zahl(&self.v_neu).unwrap_or(V_WAAGERECHT))));
                self.auswahl = Some(self.zeilen.len() - 1);
                self.speichere();
            }
            if ui.add_enabled(self.auswahl.is_some(), egui::Button::new("Zeile löschen")).clicked() {
                if let Some(i) = self.auswahl.filter(|i| *i < self.zeilen.len()) {
                    self.zeilen.remove(i);
                    self.nummeriere();
                    self.auswahl = None;
                    self.speichere();
                }
            }
            if ui.button("Exportieren (KOR + DAT)").clicked() {
                self.exportiere(geraet);
            }
        });
        if !self.meldung.is_empty() {
            ui.label(&self.meldung);
        }
    }

    fn zeichne_tabelle(&mut self, ui: &mut egui::Ui) {
        let raster = Raster::neu(&BREITEN);
        raster.kopf(ui, &["", "Nr", "PunktNr", "Code", "X [m]", "Y [m]", "Z [m]", "D [m]", "Hz [gon]", "V [gon]"]);
        let mut gewaehlt = None;
        let mut bearbeitet = false;
        egui::ScrollArea::both().id_salt("aufnahme_tabelle").max_height(TABELLEN_HOEHE).show(ui, |ui| {
            for (i, zeile) in self.zeilen.iter_mut().enumerate() {
                let hintergrund = (self.auswahl == Some(i)).then_some(FARBE_AUSWAHL);
                raster.zeile_farbig(ui, i, "", hintergrund, |z| {
                    if z.knopf(if self.auswahl == Some(i) { "●" } else { "○" }) {
                        gewaehlt = Some(i);
                    }
                    z.zahl(&zeile.punkt.nr.to_string());
                    bearbeitet |= z.eingabe(&mut zeile.punkt.punkt_nr);
                    bearbeitet |= z.eingabe(&mut zeile.punkt.code);
                    let koordinaten = z.eingabe(&mut zeile.x) | z.eingabe(&mut zeile.y) | z.eingabe(&mut zeile.z);
                    if koordinaten {
                        zeile.uebernimm_texte();
                        bearbeitet = true;
                    }
                    z.zahl(&f(zeile.punkt.d, 3));
                    z.zahl(&f(zeile.punkt.hz, 4));
                    z.zahl(&f(zeile.punkt.v, 4));
                });
            }
        });
        if let Some(i) = gewaehlt {
            self.auswahl = Some(i);
        }
        if bearbeitet {
            self.geaendert = true;
        }
        // Gespeichert wird, sobald die Eingabe abgeschlossen ist (kein Feld mehr aktiv)
        if self.geaendert && !ui.ctx().egui_wants_keyboard_input() {
            self.speichere();
        }
    }
}

fn zeige(ui: &mut egui::Ui, hinweis: &Hinweis) {
    if !hinweis.text.is_empty() {
        ui.colored_label(hinweis.farbe.unwrap_or(ui.visuals().text_color()), &hinweis.text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ansicht() -> AufnahmetestAnsicht {
        static DATEI_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let datei_id = DATEI_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        AufnahmetestAnsicht {
            zeilen: Vec::new(),
            datei: std::env::temp_dir().join(format!("fb-aufn-gui-{}-{datei_id}.json", std::process::id())),
            projekt_name: "T".into(),
            bearbeiter: "jg".into(),
            export_ordner: PathBuf::new(),
            punkt_nr: String::new(),
            code: " KS ".into(),
            v_neu: "100.0000".into(),
            auswahl: None,
            messen_wartet: true,
            mess_info: Hinweis::default(),
            anfahr_info: Hinweis::default(),
            meldung: String::new(),
            geaendert: false,
        }
    }

    #[test]
    fn messung_wird_angehaengt_und_nummer_zaehlt_hoch() {
        let mut a = ansicht();
        a.punkt_nr = "7".into();
        a.uebernimm_messung(100.0, 100.0, 10.0);
        a.uebernimm_messung(0.0, 100.0, 5.0);
        assert_eq!(a.zeilen.len(), 2);
        assert_eq!(a.zeilen[0].punkt.punkt_nr, "7");
        assert_eq!(a.zeilen[1].punkt.punkt_nr, "8");
        assert_eq!(a.punkt_nr, "9");
        assert_eq!(a.zeilen[0].punkt.code, "KS");
        assert_eq!((a.zeilen[0].x.as_str(), a.zeilen[1].y.as_str()), ("10.0000", "5.0000"));
        assert_eq!(a.auswahl, Some(1));
        // Datei wurde geschrieben und ist lesbar
        assert_eq!(daten::lade(&a.datei).len(), 2);
        let _ = std::fs::remove_file(&a.datei);
    }

    #[test]
    fn ohne_nummer_wird_p_nr_vergeben_und_nichts_hochgezaehlt() {
        let mut a = ansicht();
        a.uebernimm_messung(0.0, 100.0, 1.0);
        assert_eq!(a.zeilen[0].punkt.punkt_nr, "P001");
        assert_eq!(a.punkt_nr, "");
        let _ = std::fs::remove_file(&a.datei);
    }

    #[test]
    fn bearbeiten_der_koordinaten_rechnet_richtung_neu() {
        let mut z = Zeile::neu(AufnahmePunkt::leer(1, 100.0));
        z.x = "10,5".into();
        z.y = "0".into();
        z.z = "0".into();
        z.uebernimm_texte();
        assert_eq!((z.punkt.x, z.punkt.d, z.punkt.hz, z.punkt.v), (10.5, 10.5, 100.0, 100.0));
        // Ungültige Eingabe lässt den Punkt unverändert
        z.x = "abc".into();
        z.uebernimm_texte();
        assert_eq!(z.punkt.x, 10.5);
    }

    #[test]
    fn unvollstaendige_messung_und_fehler_werden_gemeldet() {
        let mut a = ansicht();
        let mut m = crate::tachymeter::messung::TachymeterMessung::default();
        m.hz_gon = Some(1.0);
        a.verarbeite_ergebnis(MessErgebnis::Messung(Box::new(m)));
        assert!(a.zeilen.is_empty() && !a.messen_wartet);
        assert_eq!(a.mess_info.text, "Unvollständige Messung (Hz/V/D fehlen).");
        a.verarbeite_ergebnis(MessErgebnis::Fehler("Kein Ziel".into()));
        assert_eq!(a.mess_info.text, "Messfehler: Kein Ziel");
    }
}
