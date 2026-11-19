// Zweck:    Dialog „Messdaten eingeben" für einen im Viewer gewählten Anschlusspunkt: Koordinaten aus dem Snap,
//           Messwerte (Hz, V, Strecke, Zielhöhe, Code) manuell oder per Tachymeter, Übernahme in die Anschlusspunktdatei
// Hardware: Tachymeter über gui::geraet (GeoCOM / GSI Online)
// Autor:    Johann Gerner

use eframe::egui::{self, Align2, Color32, RichText, Vec2};

use super::{Dialog, DxfViewer};
use crate::daten::modelle::StationierungsPunkt;
use crate::daten::projekt::Projekt;
use crate::grundlagen::format::{f, parse_zahl};
use crate::gui::geraet::Geraet;
use crate::tachymeter::messablauf::MessErgebnis;
use crate::tachymeter::messung::TachymeterMessung;

/// Standardcode, wenn noch keiner verwendet wurde.
pub const STANDARD_CODE: &str = "030";
/// Zielhöhe, wenn noch keine verwendet wurde.
pub const STANDARD_ZIELHOEHE: &str = "1.700";
/// Kopfzeile der Anschlusspunktdatei.
/// Farben der Meldungen.
const FARBE_OK: Color32 = Color32::from_rgb(40, 150, 60);
const FARBE_WARNUNG: Color32 = Color32::from_rgb(200, 130, 40);
const FARBE_FEHLER: Color32 = Color32::from_rgb(200, 60, 40);
/// Anzahl Zeichen, die ein Eingabefeld mindestens aufnehmen muss.
const FELD_ZEICHEN: f32 = 9.0;
/// Breite eines Zeichens der Monospace-Schrift im Verhältnis zur Schriftgröße.
const ZEICHEN_BREITE_FAKTOR: f32 = 0.6;
/// Innenabstand links und rechts im Eingabefeld.
const FELD_INNENABSTAND: f32 = 16.0;
/// Ab dieser Winkeländerung gegenüber der Messung gilt Hz/V als getrennt erfasst [mgon].
const WINKEL_SCHWELLE_MGON: f64 = 2.0;
/// Vollkreis [gon] und Umrechnung gon → mgon.
const VOLLKREIS_GON: f64 = 400.0;
const MGON_JE_GON: f64 = 1000.0;

/// Differenz zweier Richtungen [mgon], auf ±200 gon normiert.
fn winkel_differenz_mgon(a_gon: f64, b_gon: f64) -> f64 {
    let halb = VOLLKREIS_GON / 2.0;
    ((a_gon - b_gon + halb).rem_euclid(VOLLKREIS_GON) - halb) * MGON_JE_GON
}

/// Zustand des Dialogs.
#[derive(Clone, Debug)]
pub(super) struct MessdatenEingabe {
    pub r: f64,
    pub h: f64,
    pub punkt_nr: String,
    pub hoehe: String,
    pub hz: String,
    pub v: String,
    pub strecke: String,
    pub zielhoehe: String,
    pub code: String,
    /// Die Messwertfelder gelten als gefüllt (Messung oder Tastatureingabe).
    pub aktiv: bool,
    /// Eine Messung wurde gestartet und das Ergebnis steht noch aus.
    pub wartet: bool,
    pub info: String,
    pub info_farbe: Option<Color32>,
    /// Hz und V [gon] der letzten Messung (Bezug für die Erkennung getrennter Erfassung).
    pub mess_hz: Option<f64>,
    pub mess_v: Option<f64>,
    /// Zuletzt aus der Live-Abfrage in die Felder übernommene Werte (Hz, V [gon]).
    pub folge_hz: Option<f64>,
    pub folge_v: Option<f64>,
}

impl MessdatenEingabe {
    /// Hinweistext, wenn sich Hz oder V am Gerät seit der Messung um mehr als 2 mgon geändert haben.
    pub fn getrennt_hinweis(&self, live_hz: Option<f64>, live_v: Option<f64>) -> Option<String> {
        let (mess_hz, mess_v) = (self.mess_hz?, self.mess_v?);
        let dhz = winkel_differenz_mgon(live_hz?, mess_hz);
        let dv = live_v.map_or(0.0, |v| winkel_differenz_mgon(v, mess_v));
        (dhz.abs() > WINKEL_SCHWELLE_MGON || dv.abs() > WINKEL_SCHWELLE_MGON)
            .then(|| format!("⚠ Winkel seit Messung geändert (ΔHz {dhz:+.1}, ΔV {dv:+.1} mgon) – getrennt gemessen"))
    }

    pub fn neu(r: f64, h: f64, punkt_nr: &str, hoehe: f64, code: &str, zielhoehe: &str) -> Self {
        Self {
            r,
            h,
            punkt_nr: punkt_nr.to_string(),
            hoehe: if hoehe != 0.0 { f(hoehe, 3) } else { f(0.0, 3) },
            hz: String::new(),
            v: String::new(),
            strecke: String::new(),
            zielhoehe: zielhoehe.to_string(),
            code: if code.is_empty() { STANDARD_CODE.to_string() } else { code.to_string() },
            aktiv: false,
            wartet: false,
            info: String::new(),
            info_farbe: None,
            mess_hz: None,
            mess_v: None,
            folge_hz: None,
            folge_v: None,
        }
    }

    /// Übernehmen ist erlaubt, wenn Messwerte vorliegen und die Strecke positiv ist.
    pub fn kann_uebernehmen(&self) -> bool {
        self.aktiv && parse_zahl(&self.strecke).is_some_and(|s| s > 0.0)
    }

    /// Trägt eine vollständige Messung in die Felder ein.
    pub fn uebernimm_messung(&mut self, m: &TachymeterMessung) {
        match (m.hz_gon, m.v_gon, m.schraegstrecke_m) {
            (Some(hz), Some(v), Some(s)) => {
                self.aktiv = true;
                self.mess_hz = Some(hz);
                self.mess_v = Some(v);
                self.folge_hz = Some(hz);
                self.folge_v = Some(v);
                self.hz = f(hz, 4);
                self.v = f(v, 4);
                self.strecke = f(s, 3);
                self.info = format!("Hz={} gon   V={} gon   D={} m", f(hz, 4), f(v, 4), f(s, 3));
                self.info_farbe = Some(FARBE_OK);
            }
            _ => {
                self.info = "Unvollständige Messung (Hz/V/D fehlen).".to_string();
                self.info_farbe = Some(FARBE_WARNUNG);
            }
        }
    }

    /// Der Punkt für die Anschlusspunktdatei; `None` mit Fehlertext bei ungültigen Eingaben.
    pub fn ergebnis(&self) -> Result<StationierungsPunkt, String> {
        if self.punkt_nr.trim().is_empty() {
            return Err("Bitte eine Punktnummer eingeben.".to_string());
        }
        let strecke = parse_zahl(&self.strecke).unwrap_or(0.0);
        if strecke <= 0.0 {
            return Err("Strecke muss größer als 0 sein.\nBitte Messdaten eingeben oder Messen auslösen.".to_string());
        }
        Ok(StationierungsPunkt {
            punkt_nr: self.punkt_nr.trim().to_string(),
            r: self.r,
            h: self.h,
            hoehe: parse_zahl(&self.hoehe).unwrap_or(0.0),
            hz: parse_zahl(&self.hz).unwrap_or(0.0),
            v: parse_zahl(&self.v).unwrap_or(0.0),
            strecke,
            zielhoehe: parse_zahl(&self.zielhoehe).unwrap_or(0.0),
        })
    }
}

/// Zeile der Anschlusspunktdatei.
#[cfg(test)]
pub(super) fn anschluss_zeile(p: &StationierungsPunkt) -> String {
    crate::daten::csv_basis::zeile(crate::daten::stationierung_csv::zeile(p), ';')
}

/// Hängt eine Messung an die Anschlusspunktdatei an (gleiche Nummer bleibt als Mehrfachmessung erhalten).
pub(super) fn schreibe_anschlusspunkt(projekt: &Projekt, p: &StationierungsPunkt, code: &str) -> std::io::Result<()> {
    let quelle = projekt.anschlusspunkte_pfad();
    let mut d = if quelle.exists() {
        crate::daten::csv_datei::CsvDatei::laden(&quelle)?
    } else {
        crate::daten::stationierung_csv::datei(&projekt.station.standpunkt_nr, projekt.einstellungen.instrumenten_hoehe, &[])
    };
    d.datentyp = crate::daten::csv_datei::TYP_STANDPUNKT.into();
    d.projekt = projekt.einstellungen.projekt_name.clone();
    let i = d.zeilen.len();
    d.zeilen.push(vec![String::new(); d.spalten.len()]);
    for (spalte, wert) in crate::daten::csv_datei::SPALTEN_STP.iter().zip(crate::daten::stationierung_csv::zeile(p)) {
        d.set_zelle(i, spalte, wert);
    }
    d.set_zelle(i, "Code", code.into());
    let ih = if projekt.station.standpunkt_nr.is_empty() { projekt.einstellungen.instrumenten_hoehe } else { projekt.station.instrumenten_hoehe };
    d.standpunkt_nr = projekt.station.standpunkt_nr.clone();
    d.instr_hoehe = f(ih, 3);
    d.set_meta("AufstellungId", &crate::daten::messdaten::aufstellung_id(&projekt.einstellungen, &d.standpunkt_nr.clone(), ih));
    d.stationierung.clear();
    d.pfad = projekt.anschlusspunkte_zielpfad();
    d.speichern()
}

impl DxfViewer {
    /// Öffnet den Dialog für einen gewählten Punkt.
    pub(super) fn oeffne_messdaten_dialog(&mut self, projekt: &Projekt, r: f64, h: f64, nr: &str, hoehe: f64) {
        let reflektorlos = projekt.einstellungen.mess_modus_name != "Prisma";
        let zielhoehe = if reflektorlos { f(0.0, 3) } else { self.letzte_zielhoehe.clone() };
        self.dialog = Some(Dialog::Messdaten(Box::new(MessdatenEingabe::neu(r, h, nr, hoehe, &self.letzter_code, &zielhoehe))));
    }

    /// Reicht ein Messergebnis an den offenen Dialog weiter, wenn dieser auf eines wartet.
    pub(super) fn messergebnis_fuer_dialog(&mut self, ergebnis: MessErgebnis) -> Option<MessErgebnis> {
        match &mut self.dialog {
            Some(Dialog::Messdaten(d)) if d.wartet => {
                d.wartet = false;
                match &ergebnis {
                    MessErgebnis::Messung(m) => {
                        d.uebernimm_messung(m);
                        if m.hz_gon.is_some() && m.v_gon.is_some() && m.schraegstrecke_m.is_some() {
                            crate::grundlagen::ton::bestaetigung();
                        } else {
                            crate::grundlagen::ton::fehler();
                        }
                    }
                    MessErgebnis::Fehler(text) => {
                        d.info = format!("Messfehler: {text}");
                        crate::grundlagen::ton::fehler();
                        d.info_farbe = Some(FARBE_FEHLER);
                    }
                }
                None
            }
            _ => Some(ergebnis),
        }
    }

    pub(super) fn zeichne_messdaten_dialog(&mut self, ctx: &egui::Context, projekt: &Projekt, geraet: &mut Geraet, mut d: Box<MessdatenEingabe>) {
        let mut aktion: Option<bool> = None;
        let mut messen = false;
        let mut umschalten = false;
        let verbunden = geraet.ist_verbunden();
        let beschaeftigt = geraet.beschaeftigt();
        // Solange der Dialog offen ist, fragt das Gerät Hz und V zyklisch ab
        geraet.live_v_aktiv = true;
        let (live_hz, live_v) = (geraet.live_hz, geraet.live_v);
        // Nach einer Messung laufen Hz und V mit, wenn sich die Gerätewerte ändern
        if !d.wartet && d.mess_hz.is_some() {
            if let Some(hz) = live_hz.filter(|hz| d.folge_hz != Some(*hz)) {
                d.hz = f(hz, 4);
                d.folge_hz = Some(hz);
            }
            if let Some(v) = live_v.filter(|v| d.folge_v != Some(*v)) {
                d.v = f(v, 4);
                d.folge_v = Some(v);
            }
        }
        let hinweis = if d.wartet { None } else { d.getrennt_hinweis(live_hz, live_v) };
        if d.wartet {
            if let Some(text) = geraet.messung_status() {
                d.info = text.to_string();
                d.info_farbe = Some(FARBE_WARNUNG);
            }
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        egui::Window::new("Messdaten eingeben").collapsible(false).resizable(false).anchor(Align2::CENTER_CENTER, Vec2::ZERO).show(ctx, |ui| {
            ui.label(RichText::new(format!("R (Rechtswert):   {} m", f(d.r, 3))).monospace());
            ui.label(RichText::new(format!("H (Hochwert):     {} m", f(d.h, 3))).monospace());
            ui.add_space(4.0);
            egui::Grid::new("messdaten_felder").num_columns(3).spacing([8.0, 4.0]).show(ui, |ui| {
                let mut manuell = false;
                let schrift_groesse = egui::TextStyle::Monospace.resolve(ui.style()).size;
                let feld_groesse = Vec2::new(FELD_ZEICHEN * schrift_groesse * ZEICHEN_BREITE_FAKTOR + FELD_INNENABSTAND, ui.spacing().interact_size.y);
                let mut feld = |ui: &mut egui::Ui, titel: &str, text: &mut String, einheit: &str, messwert: bool| {
                    ui.label(titel);
                    let antwort = ui.add_sized(feld_groesse, egui::TextEdit::singleline(text).font(egui::TextStyle::Monospace));
                    if messwert && antwort.changed() {
                        manuell = true;
                    }
                    ui.label(einheit);
                    ui.end_row();
                };
                feld(ui, "Punktnummer:", &mut d.punkt_nr, "", false);
                feld(ui, "Höhe:", &mut d.hoehe, "m", false);
                feld(ui, "Hz:", &mut d.hz, "gon", true);
                feld(ui, "V:", &mut d.v, "gon", true);
                feld(ui, "Strecke:", &mut d.strecke, "m", true);
                feld(ui, "Zielhöhe:", &mut d.zielhoehe, "m", true);
                feld(ui, "Code:", &mut d.code, "", true);
                if manuell {
                    d.aktiv = true;
                }
            });
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                let (name, fill) = if geraet.reflektorlos { ("Reflektorlos", Color32::from_rgb(130, 80, 20)) } else { ("Prisma", Color32::from_rgb(38, 100, 58)) };
                if ui.add_enabled(verbunden && !beschaeftigt, egui::Button::new(RichText::new(name).color(Color32::WHITE)).fill(fill)).clicked() {
                    umschalten = true;
                }
                let messen_knopf = egui::Button::new(RichText::new("► Messen").strong().color(Color32::WHITE)).fill(Color32::from_rgb(160, 60, 20));
                if ui.add_enabled(verbunden && !beschaeftigt && !d.wartet, messen_knopf).clicked() {
                    messen = true;
                }
            });
            if !verbunden && d.info.is_empty() {
                ui.colored_label(FARBE_WARNUNG, "Kein Tachymeter verbunden – Messwerte können von Hand eingegeben werden.");
            }
            if !d.info.is_empty() {
                ui.colored_label(d.info_farbe.unwrap_or(ui.visuals().text_color()), &d.info);
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.add_enabled(d.kann_uebernehmen(), egui::Button::new(RichText::new("Übernehmen").strong())).clicked() {
                    aktion = Some(true);
                }
                if ui.button("Abbrechen").clicked() {
                    aktion = Some(false);
                }
            });
            if let Some(text) = &hinweis {
                ui.add_space(4.0);
                ui.colored_label(FARBE_WARNUNG, text);
            }
        });

        if umschalten {
            if let Err(fehler) = geraet.starte_ziel_umschaltung() {
                d.info = fehler;
                d.info_farbe = Some(FARBE_FEHLER);
            }
        }
        // A-Taste des Toughpad löst wie „Messen" die Messung aus
        if verbunden && !beschaeftigt && !d.wartet && self.mess_taste {
            messen = true;
        }
        if messen {
            let zielhoehe = parse_zahl(&d.zielhoehe).unwrap_or(0.0);
            match geraet.starte_messung(zielhoehe) {
                Ok(()) => {
                    d.wartet = true;
                    d.info = "Messung startet …".to_string();
                    d.info_farbe = Some(FARBE_WARNUNG);
                }
                Err(fehler) => {
                    d.info = fehler;
                    d.info_farbe = Some(FARBE_FEHLER);
                    crate::grundlagen::ton::fehler();
                }
            }
        }
        match aktion {
            Some(false) => {
                if d.wartet {
                    // Eine laufende Messung wird weiter abgearbeitet; das Ergebnis geht dann ins Leere
                    d.wartet = false;
                }
            }
            Some(true) => match d.ergebnis() {
                Ok(p) => {
                    self.letzter_code = d.code.trim().to_string();
                    if projekt.einstellungen.mess_modus_name == "Prisma" {
                        self.letzte_zielhoehe.clone_from(&d.zielhoehe);
                    }
                    match schreibe_anschlusspunkt(projekt, &p, &d.code) {
                        Ok(()) => self.status = format!("Punkt '{}' gespeichert  |  R: {}   H: {}", p.punkt_nr, f(p.r, 3), f(p.h, 3)),
                        Err(fehler) => {
                            self.dialog = Some(Dialog::Hinweis { titel: "Fehler".into(), text: format!("Punkt konnte nicht geschrieben werden:\n{fehler}") });
                        }
                    }
                }
                Err(text) => {
                    d.info = text;
                    d.info_farbe = Some(FARBE_FEHLER);
                    self.dialog = Some(Dialog::Messdaten(d));
                }
            },
            None => self.dialog = Some(Dialog::Messdaten(d)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tachymeter::messung::TachymeterMessung;

    fn voll() -> TachymeterMessung {
        let mut m = TachymeterMessung::neu("GeoCOM", "");
        m.hz_gon = Some(12.34567);
        m.v_gon = Some(98.7654321);
        m.schraegstrecke_m = Some(25.12345);
        m
    }

    #[test]
    fn messung_wird_eingetragen_und_freigegeben() {
        let mut d = MessdatenEingabe::neu(100.0, 200.0, "A1", 0.0, "", STANDARD_ZIELHOEHE);
        assert_eq!(d.code, STANDARD_CODE);
        assert!(!d.kann_uebernehmen());
        d.uebernimm_messung(&voll());
        assert_eq!((d.hz.as_str(), d.v.as_str(), d.strecke.as_str()), ("12.3457", "98.7654", "25.123"));
        assert!(d.kann_uebernehmen());
        let p = d.ergebnis().unwrap();
        assert_eq!((p.punkt_nr.as_str(), p.r, p.h, p.strecke, p.zielhoehe), ("A1", 100.0, 200.0, 25.123, 1.7));
    }

    #[test]
    fn unvollstaendige_messung_und_eingabefehler() {
        let mut d = MessdatenEingabe::neu(1.0, 2.0, "", 5.0, "X", "0.000");
        let mut halb = voll();
        halb.schraegstrecke_m = None;
        d.uebernimm_messung(&halb);
        assert!(!d.aktiv && d.info.contains("Unvollständig"));
        assert!(d.ergebnis().unwrap_err().contains("Punktnummer"));
        d.punkt_nr = "7".into();
        assert!(d.ergebnis().unwrap_err().contains("Strecke"));
        d.strecke = "0".into();
        d.aktiv = true;
        assert!(!d.kann_uebernehmen());
        assert_eq!(d.hoehe, "5.000");
    }

    #[test]
    fn getrennte_erfassung_wird_erkannt() {
        let mut d = MessdatenEingabe::neu(1.0, 2.0, "A", 0.0, "", "0.000");
        assert!(d.getrennt_hinweis(Some(10.0), Some(100.0)).is_none());
        d.uebernimm_messung(&voll());
        let (hz, v) = (d.mess_hz.unwrap(), d.mess_v.unwrap());
        assert!(d.getrennt_hinweis(Some(hz + 0.0015), Some(v)).is_none());
        assert!(d.getrennt_hinweis(Some(hz + 0.0030), Some(v)).is_some());
        assert!(d.getrennt_hinweis(Some(hz), Some(v - 0.0030)).is_some());
        // Übergang über den Nullpunkt: 399.9990 gegen 0.0005 sind nur 1,5 mgon
        d.mess_hz = Some(399.999);
        assert!(d.getrennt_hinweis(Some(0.0005), Some(v)).is_none());
    }

    #[test]
    fn anschlusszeile_und_datei() {
        let p = StationierungsPunkt { punkt_nr: "A1".into(), r: 1.5, h: 2.25, hoehe: 3.0, hz: 4.0, v: 99.5, strecke: 10.0, zielhoehe: 1.7 };
        assert_eq!(anschluss_zeile(&p), "A1;1.5000;2.2500;3.0000;4.0000;99.5000;10.000;1.700;;;0.000;0.000;AP");
    }
}
