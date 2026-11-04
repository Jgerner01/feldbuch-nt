// Zweck:    Berechnungsauswahl mit Rückwärtsschnitt, Vorwärtsschnitt, Bogenschnitt und Hochpunktherablegung
//           (Eingabetabelle, Ergebnis, Residuen mit Ampelfarben)
// Hardware: keine
// Autor:    Johann Gerner

use eframe::egui::{self, Color32, RichText};

use super::koord_transformation::KoordTransformationAnsicht;
use super::tabelle::Raster;
use std::path::PathBuf;

use crate::daten::modelle::{FeldbuchPunkt, TYP_NEUPUNKT, TYP_STANDPUNKT, heute_text, jetzt};
use crate::daten::projekt::Projekt;
use crate::grundlagen::format::{f, parse_zahl};
use crate::grundlagen::{fehler_log, protokoll};
use crate::protokolle::{Protokoll, ausgabeverzeichnis, berechnungen as prot_berechnungen, schreibe_fuer_projekt};
use crate::rechnen::{bogenschnitt, hochpunkt, rueckwaertsschnitt, vorwaertsschnitt};

/// Breite der Zeilenköpfe.
const ZEILENKOPF_BREITE: f32 = 30.0;
/// Breite der Nummernspalte und der Aktiv-Spalte.
const BREITE_NR: f32 = 110.0;
const BREITE_AKTIV: f32 = 50.0;
/// Breite einer Wertespalte.
const BREITE_WERT: f32 = 90.0;
/// Breite der Koordinatenspalten.
const BREITE_KOORD: f32 = 110.0;
/// Breite der Spalten der Residuentabelle.
const BREITE_RESIDUUM: f32 = 90.0;
/// Startwerte der Orientierung, Instrumenten- und Zielhöhe.
const Z_STARTWERT: &str = "0.0000";
const HOEHE_STARTWERT: &str = "0.000";
/// Ampelgrenzen für Richtungsresiduen [cc].
const RICHTUNG_AMPEL: [f64; 3] = [60.0, 20.0, 5.0];
/// Ampelgrenzen für Streckenresiduen [mm].
const STRECKE_AMPEL: [f64; 3] = [30.0, 10.0, 3.0];
/// Fehlerfarbe der Meldung.
const FEHLER_FARBE: Color32 = Color32::from_rgb(180, 40, 40);

/// Art der Berechnung.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Art {
    Rueckwaerts,
    Vorwaerts,
    Bogen,
    Hochpunkt,
}

/// Beschreibung einer Wertespalte: Titel und Startwert.
struct Spalte {
    titel: &'static str,
    start: &'static str,
}

impl Art {
    fn titel(self) -> &'static str {
        match self {
            Self::Rueckwaerts => "Rückwärtsschnitt",
            Self::Vorwaerts => "Vorwärtsschnitt",
            Self::Bogen => "Bogenschnitt",
            Self::Hochpunkt => "Hochpunktherablegung",
        }
    }

    fn beschreibung(self) -> &'static str {
        match self {
            Self::Rueckwaerts => "Standpunkt aus mindestens 3 gemessenen Richtungen zu Anschlusspunkten.",
            Self::Vorwaerts => "Neupunkt aus mindestens 2 Stationen mit Richtung und Orientierung z.",
            Self::Bogen => "Neupunkt aus mindestens 2 gemessenen Strecken zu Anschlusspunkten.",
            Self::Hochpunkt => "Lage und Höhe eines Hochpunkts aus mindestens 2 Stationen (Richtung, Zenitwinkel).",
        }
    }

    fn zeilen(self) -> usize {
        match self {
            Self::Rueckwaerts => 12,
            _ => 10,
        }
    }

    fn punkt_name(self) -> &'static str {
        match self {
            Self::Rueckwaerts => "9000",
            Self::Vorwaerts | Self::Bogen => "8000",
            Self::Hochpunkt => "9001",
        }
    }

    fn name_label(self) -> &'static str {
        match self {
            Self::Rueckwaerts => "Standpunkt-Nr:",
            Self::Hochpunkt => "Punkt-Nr:",
            _ => "Neupunkt-Nr:",
        }
    }

    fn nr_titel(self) -> &'static str {
        match self {
            Self::Rueckwaerts => "PunktNr",
            _ => "PunktNr (Station)",
        }
    }

    /// Wertespalten nach der Punktnummer: R, H, dann je nach Art.
    fn spalten(self) -> Vec<Spalte> {
        let s = |titel, start| Spalte { titel, start };
        let mut liste = vec![s("R [m]", ""), s("H [m]", "")];
        match self {
            Self::Rueckwaerts => liste.push(s("Hz [gon]", "")),
            Self::Vorwaerts => liste.extend([s("Hz [gon]", ""), s("z [gon]", Z_STARTWERT)]),
            Self::Bogen => liste.push(s("Strecke [m]", "")),
            Self::Hochpunkt => liste.extend([
                s("Höhe [m]", ""),
                s("iH [m]", HOEHE_STARTWERT),
                s("Hz [gon]", ""),
                s("z [gon]", Z_STARTWERT),
                s("V [gon]", ""),
                s("zh [m]", HOEHE_STARTWERT),
            ]),
        }
        liste
    }

    /// Index der Orientierungsspalte (z), falls vorhanden.
    fn z_spalte(self) -> Option<usize> {
        match self {
            Self::Vorwaerts => Some(3),
            Self::Hochpunkt => Some(5),
            _ => None,
        }
    }

    fn kategorie(self) -> &'static str {
        match self {
            Self::Rueckwaerts => "Rückwärtschnitt",
            Self::Vorwaerts => "Vorwärtschnitt",
            Self::Bogen => "Bogenschnitt",
            Self::Hochpunkt => "Hochpunktherablegung",
        }
    }
}

/// Eine Eingabezeile.
#[derive(Clone)]
struct Zeile {
    nr: String,
    werte: Vec<String>,
    aktiv: bool,
}

/// Zelle der Residuentabelle: Text und optionale Ampelfarbe.
struct ResZelle {
    text: String,
    farbe: Option<Color32>,
}

/// Ergebnis für die Anzeige.
struct Anzeige {
    zeilen: Vec<String>,
    info: Vec<String>,
    warnung: String,
    kopf: Vec<&'static str>,
    residuen: Vec<Vec<ResZelle>>,
}

/// Rechenergebnis vor der Ablage: Anzeige plus zu speichernde Werte.
struct Ablage {
    anzeige: Anzeige,
    werte: Vec<(&'static str, String)>,
    punkt: FeldbuchPunkt,
    protokoll: Protokoll,
}

/// Eine Berechnung mit Eingabetabelle.
pub struct SchnittAnsicht {
    art: Art,
    zeilen: Vec<Zeile>,
    punkt_name: String,
    anzeige: Option<Anzeige>,
    meldung: String,
    info: String,
    /// Protokoll der letzten Berechnung und Zielordner (für das automatische Schreiben beim Verlassen).
    protokoll: Option<Protokoll>,
    protokoll_ziel: Option<(PathBuf, bool)>,
    protokoll_geschrieben: bool,
}

impl Drop for SchnittAnsicht {
    /// Wie im Original: Beim Schließen wird das Protokoll der letzten Berechnung geschrieben, wenn es noch fehlt.
    fn drop(&mut self) {
        if let (Some(p), Some((ziel, true)), false) = (&self.protokoll, &self.protokoll_ziel, self.protokoll_geschrieben) {
            if let Err(fehler) = p.schreibe(ziel) {
                fehler_log::log("Berechnungen.protokoll", &fehler);
            }
        }
    }
}

/// Eine ausgewertete Eingabezeile: Nummer, Werte, aktiv.
struct Eingabe {
    nr: String,
    w: Vec<f64>,
    aktiv: bool,
}

fn vorzeichen(wert: f64) -> String {
    if wert > 0.0 { format!("+{}", f(wert, 1)) } else { f(wert, 1) }
}

fn ampel_farbe(betrag: f64, grenzen: [f64; 3]) -> Color32 {
    if betrag > grenzen[0] {
        Color32::from_rgb(200, 60, 60)
    } else if betrag > grenzen[1] {
        Color32::from_rgb(220, 100, 60)
    } else if betrag > grenzen[2] {
        Color32::from_rgb(255, 200, 80)
    } else {
        Color32::from_rgb(160, 220, 130)
    }
}

fn res_zelle(text: impl Into<String>) -> ResZelle {
    ResZelle { text: text.into(), farbe: None }
}

fn res_ampel(wert: f64, grenzen: [f64; 3]) -> ResZelle {
    ResZelle { text: vorzeichen(wert), farbe: Some(ampel_farbe(wert.abs(), grenzen)) }
}

fn res_leer() -> ResZelle {
    res_zelle("-")
}

/// Genauigkeitszeile: s0, Redundanz, Iterationen.
fn s0_zeile(redundanz: i32, s0_mm: f64, iterationen: usize, konvergiert: bool) -> String {
    if redundanz > 0 {
        let mut text = format!("s₀ = {} mm     r = {redundanz}     {iterationen} Iter.", f(s0_mm, 2));
        if !konvergiert {
            text.push_str("  !! NICHT konvergiert !!");
        }
        text
    } else {
        format!("r = 0  (eindeutig bestimmt, {iterationen} Iter.)")
    }
}

impl SchnittAnsicht {
    fn neu(art: Art, projekt: &Projekt) -> Self {
        let mut ansicht = Self {
            art,
            zeilen: Vec::new(),
            punkt_name: art.punkt_name().to_string(),
            anzeige: None,
            meldung: String::new(),
            info: String::new(),
            protokoll: None,
            protokoll_ziel: None,
            protokoll_geschrieben: false,
        };
        ansicht.leere_zeilen();
        ansicht.lade_anschlusspunkte(projekt);
        ansicht
    }

    fn leere_zeile(&self) -> Zeile {
        Zeile { nr: String::new(), werte: self.art.spalten().iter().map(|s| s.start.to_string()).collect(), aktiv: true }
    }

    fn leere_zeilen(&mut self) {
        self.zeilen = (0..self.art.zeilen()).map(|_| self.leere_zeile()).collect();
    }

    /// Koordinaten (und bei der Hochpunktherablegung die Höhe) aus der Anschlusspunktdatei übernehmen.
    fn lade_anschlusspunkte(&mut self, projekt: &Projekt) {
        let Ok(text) = std::fs::read_to_string(projekt.anschlusspunkte_pfad()) else {
            return;
        };
        let eintraege: Vec<Vec<String>> = text
            .lines()
            .skip(1)
            .filter(|z| !z.trim().is_empty())
            .map(|z| z.split(',').map(|t| t.trim().to_string()).collect::<Vec<_>>())
            .filter(|t| t.len() >= 3)
            .collect();
        if eintraege.is_empty() {
            return;
        }
        self.leere_zeilen();
        for (zeile, t) in self.zeilen.iter_mut().zip(&eintraege) {
            zeile.nr.clone_from(&t[0]);
            zeile.werte[0].clone_from(&t[1]);
            zeile.werte[1].clone_from(&t[2]);
            if self.art == Art::Hochpunkt && t.len() >= 4 {
                zeile.werte[2].clone_from(&t[3]);
            }
        }
    }

    /// Orientierung der letzten Freien Stationierung in alle belegten Zeilen eintragen.
    fn lade_orientierung(&mut self, projekt: &Projekt) {
        let Some(z_index) = self.art.z_spalte() else {
            return;
        };
        match projekt.projektdaten.get_value("Freie Stationierung", "Orientierung [gon]") {
            Some(wert) if !wert.trim().is_empty() => {
                let wert = wert.to_string();
                for zeile in self.zeilen.iter_mut().filter(|z| !z.nr.trim().is_empty()) {
                    zeile.werte[z_index].clone_from(&wert);
                }
                self.meldung = format!("z = {wert} gon in alle Zeilen eingetragen.");
            }
            _ => self.meldung = "Keine Orientierung aus Freier Stationierung verfügbar.".to_string(),
        }
    }

    /// Liest die Tabelle; `None` mit Meldung bei ungültiger Eingabe.
    fn lese_eingaben(&mut self) -> Option<Vec<Eingabe>> {
        let mut liste = Vec::new();
        for (i, zeile) in self.zeilen.iter().enumerate() {
            if zeile.nr.trim().is_empty() {
                continue;
            }
            let werte: Option<Vec<f64>> = zeile.werte.iter().map(|w| parse_zahl(w)).collect();
            let Some(w) = werte else {
                self.meldung = format!("Ungültige Eingabe in Zeile {}.", i + 1);
                return None;
            };
            liste.push(Eingabe { nr: zeile.nr.trim().to_string(), w, aktiv: zeile.aktiv });
        }
        Some(liste)
    }

    fn berechne(&mut self, projekt: &mut Projekt) {
        self.meldung.clear();
        self.anzeige = None;
        self.info.clear();
        let Some(eingaben) = self.lese_eingaben() else {
            return;
        };
        let mindest = match self.art {
            Art::Rueckwaerts => 3,
            _ => 2,
        };
        if eingaben.len() < mindest {
            self.meldung = format!("Mindestens {mindest} vollständige Zeilen erforderlich.");
            return;
        }
        let aktiv: Vec<bool> = eingaben.iter().map(|e| e.aktiv).collect();
        let name = self.punkt_name.trim().to_string();
        let bearbeiter = projekt.projektdaten.bearbeiter.clone();
        let ergebnis = match self.art {
            Art::Rueckwaerts => rechne_rueckwaerts(&eingaben, &aktiv, &name, &bearbeiter),
            Art::Vorwaerts => rechne_vorwaerts(&eingaben, &aktiv, &name, &bearbeiter),
            Art::Bogen => rechne_bogen(&eingaben, &aktiv, &name, &bearbeiter),
            Art::Hochpunkt => rechne_hochpunkt(&eingaben, &aktiv, &name, &bearbeiter),
        };
        match ergebnis.and_then(|a| self.speichere(projekt, a)) {
            Ok(anzeige) => self.anzeige = Some(anzeige),
            Err(fehler) => self.meldung = format!("Berechnungsfehler: {fehler}"),
        }
    }

    /// Legt Ergebnis in den Projektdaten und als Feldbuchpunkt ab und merkt sich das Protokoll.
    fn speichere(&mut self, projekt: &mut Projekt, a: Ablage) -> Result<Anzeige, String> {
        projekt.projektdaten.set_values(self.art.kategorie(), &a.werte).map_err(|e| e.to_string())?;
        let text = format!("{}: R={} H={}", a.punkt.punkt_nr, f(a.punkt.r, 3), f(a.punkt.h, 3));
        projekt.feldbuchpunkte.add_or_update(a.punkt).map_err(|e| e.to_string())?;
        protokoll::log("BERECHNUNG", &format!("{}: {text}", self.art.titel()));
        self.protokoll = Some(a.protokoll);
        self.protokoll_ziel = Some((ausgabeverzeichnis(projekt), projekt.einstellungen.protokoll_aktiv));
        self.protokoll_geschrieben = false;
        self.info.clear();
        Ok(a.anzeige)
    }

    /// Schreibt das Protokoll der letzten Berechnung.
    fn schreibe_protokoll(&mut self, projekt: &Projekt) {
        let Some(p) = &self.protokoll else {
            return;
        };
        match schreibe_fuer_projekt(p, projekt) {
            Ok(pfad) => {
                self.protokoll_geschrieben = true;
                self.info = format!("Protokoll gespeichert:\n{}", pfad.display());
            }
            Err(text) => self.meldung = text,
        }
    }

    fn zeichne(&mut self, ui: &mut egui::Ui, projekt: &mut Projekt) {
        ui.heading(self.art.titel());
        ui.label(self.art.beschreibung());
        ui.horizontal(|ui| {
            ui.label(self.art.name_label());
            ui.add(egui::TextEdit::singleline(&mut self.punkt_name).desired_width(90.0));
            if ui.button("Anschlusspunkte laden").clicked() {
                self.lade_anschlusspunkte(projekt);
            }
            if self.art.z_spalte().is_some() && ui.button("z aus Freier Stationierung").clicked() {
                self.lade_orientierung(projekt);
            }
        });
        ui.add_space(6.0);

        let spalten = self.art.spalten();
        let mut breiten = vec![BREITE_NR];
        breiten.extend(spalten.iter().enumerate().map(|(i, _)| if i < 2 { BREITE_KOORD } else { BREITE_WERT }));
        breiten.push(BREITE_AKTIV);
        let raster = Raster::neu(&breiten).mit_zeilenkopf(ZEILENKOPF_BREITE);
        let mut kopf = vec![self.art.nr_titel()];
        kopf.extend(spalten.iter().map(|s| s.titel));
        kopf.push("Aktiv");
        raster.kopf(ui, &kopf);
        for (i, zeile) in self.zeilen.iter_mut().enumerate() {
            raster.zeile(ui, i, &(i + 1).to_string(), |z| {
                z.eingabe(&mut zeile.nr);
                for wert in &mut zeile.werte {
                    z.eingabe(wert);
                }
                z.haken(&mut zeile.aktiv);
            });
        }
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui.button(RichText::new("Berechnen").strong()).clicked() {
                self.berechne(projekt);
            }
            if ui.add_enabled(self.protokoll.is_some(), egui::Button::new("Protokoll")).clicked() {
                self.schreibe_protokoll(projekt);
            }
        });
        if !self.meldung.is_empty() {
            ui.colored_label(FEHLER_FARBE, &self.meldung);
        }
        if !self.info.is_empty() {
            ui.label(&self.info);
        }
        if let Some(anzeige) = &self.anzeige {
            zeichne_anzeige(ui, anzeige);
        }
    }
}

fn zeichne_anzeige(ui: &mut egui::Ui, anzeige: &Anzeige) {
    ui.add_space(10.0);
    ui.separator();
    for text in &anzeige.zeilen {
        ui.label(RichText::new(text).monospace().strong());
    }
    for text in &anzeige.info {
        ui.label(text);
    }
    if !anzeige.warnung.is_empty() {
        ui.colored_label(Color32::DARK_RED, format!("⚠  {}", anzeige.warnung));
    }
    ui.add_space(6.0);
    let breiten: Vec<f32> = std::iter::once(BREITE_NR).chain(std::iter::repeat_n(BREITE_RESIDUUM, anzeige.kopf.len() - 1)).collect();
    let raster = Raster::neu(&breiten);
    raster.kopf(ui, &anzeige.kopf);
    for (i, zeile) in anzeige.residuen.iter().enumerate() {
        raster.zeile(ui, i, "", |z| {
            for (k, zelle) in zeile.iter().enumerate() {
                match zelle.farbe {
                    Some(farbe) => z.zelle(|ui| {
                        ui.painter().rect_filled(ui.max_rect().expand(1.0), 0.0, farbe);
                        ui.label(RichText::new(&zelle.text).monospace().color(Color32::BLACK));
                    }),
                    None if k == 0 => z.text(&zelle.text),
                    None => z.zahl(&zelle.text),
                }
            }
        });
    }
}

fn neuer_punkt(nr: &str, typ: &str, r: f64, h: f64, hoehe: f64, ori: f64, ist_3d: bool, quelle: &str) -> FeldbuchPunkt {
    FeldbuchPunkt {
        punkt_nr: nr.to_string(),
        typ: typ.to_string(),
        r,
        h,
        hoehe,
        orientierung_gon: ori,
        ist_berechnung_3d: ist_3d,
        datum: heute_text(),
        quelle: quelle.to_string(),
    }
}

fn rechne_rueckwaerts(eingaben: &[Eingabe], aktiv: &[bool], name: &str, bearbeiter: &str) -> Result<Ablage, String> {
    let punkte: Vec<rueckwaertsschnitt::RueckwaertsPunkt> = eingaben
        .iter()
        .map(|e| rueckwaertsschnitt::RueckwaertsPunkt { punkt_nr: e.nr.clone(), r: e.w[0], h: e.w[1], hz: e.w[2] })
        .collect();
    let e = rueckwaertsschnitt::berechnen(&punkte, Some(aktiv), rueckwaertsschnitt::STANDARD_RICHTUNG_CC, rueckwaertsschnitt::STANDARD_M0_MM)?;
    let protokoll = prot_berechnungen::rueckwaertsschnitt(&e, &punkte, name, bearbeiter, jetzt());
    Ok(Ablage {
        protokoll,
        anzeige: Anzeige {
            zeilen: vec![
                format!("R:  {} m", f(e.r, 3)),
                format!("H:  {} m", f(e.h, 3)),
                format!("Orientierung z:  {} gon", f(e.orientierung_gon, 4)),
            ],
            info: vec![s0_zeile(e.redundanz, e.s0_mm, e.iterationen, e.konvergiert)],
            warnung: e.kritischer_kreis,
            kopf: vec!["PunktNr", "s [m]", "v [cc]"],
            residuen: e
                .residuen
                .iter()
                .map(|r| {
                    let v = if r.aktiv { res_ampel(r.v_winkel_cc, RICHTUNG_AMPEL) } else { res_leer() };
                    vec![res_zelle(&r.punkt_nr), res_zelle(f(r.strecke_h, 1)), v]
                })
                .collect(),
        },
        werte: vec![
            ("Standpunkt", name.to_string()),
            ("R [m]", f(e.r, 3)),
            ("H [m]", f(e.h, 3)),
            ("Orientierung [gon]", f(e.orientierung_gon, 4)),
            ("s0 [mm]", f(e.s0_mm, 2)),
        ],
        punkt: neuer_punkt(name, TYP_STANDPUNKT, e.r, e.h, 0.0, e.orientierung_gon, false, "Rückwärtschnitt"),
    })
}

fn rechne_vorwaerts(eingaben: &[Eingabe], aktiv: &[bool], name: &str, bearbeiter: &str) -> Result<Ablage, String> {
    let messungen: Vec<vorwaertsschnitt::VorwaertsMessung> = eingaben
        .iter()
        .map(|e| vorwaertsschnitt::VorwaertsMessung { punkt_nr: e.nr.clone(), r: e.w[0], h: e.w[1], hz: e.w[2], z: e.w[3] })
        .collect();
    let e = vorwaertsschnitt::berechnen(&messungen, Some(aktiv), rueckwaertsschnitt::STANDARD_RICHTUNG_CC, rueckwaertsschnitt::STANDARD_M0_MM)?;
    let protokoll = prot_berechnungen::vorwaertsschnitt(&e, &messungen, name, bearbeiter, jetzt());
    Ok(Ablage {
        protokoll,
        anzeige: Anzeige {
            zeilen: vec![format!("R:  {} m", f(e.r, 3)), format!("H:  {} m", f(e.h, 3))],
            info: vec![s0_zeile(e.redundanz, e.s0_mm, e.iterationen, e.konvergiert)],
            warnung: String::new(),
            kopf: vec!["PunktNr", "s [m]", "v [cc]"],
            residuen: e
                .residuen
                .iter()
                .map(|r| {
                    let v = if r.aktiv { res_ampel(r.v_winkel_cc, RICHTUNG_AMPEL) } else { res_leer() };
                    vec![res_zelle(&r.punkt_nr), res_zelle(f(r.strecke_h, 1)), v]
                })
                .collect(),
        },
        werte: vec![("PunktNr", name.to_string()), ("R [m]", f(e.r, 3)), ("H [m]", f(e.h, 3)), ("s0 [mm]", f(e.s0_mm, 2))],
        punkt: neuer_punkt(name, TYP_NEUPUNKT, e.r, e.h, 0.0, 0.0, false, "Vorwärtschnitt"),
    })
}

fn rechne_bogen(eingaben: &[Eingabe], aktiv: &[bool], name: &str, bearbeiter: &str) -> Result<Ablage, String> {
    let messungen: Vec<bogenschnitt::BogenschnittMessung> = eingaben
        .iter()
        .map(|e| bogenschnitt::BogenschnittMessung { punkt_nr: e.nr.clone(), r: e.w[0], h: e.w[1], strecke: e.w[2] })
        .collect();
    let e = bogenschnitt::berechnen(&messungen, Some(aktiv), bogenschnitt::StreckenGenauigkeit::default())?;
    let protokoll = prot_berechnungen::bogenschnitt(&e, &messungen, name, bearbeiter, jetzt());
    let mut info = vec![s0_zeile(e.redundanz, e.s0_mm, e.iterationen, e.konvergiert)];
    if e.zwei_loesungen {
        info.push(format!(
            "2. Lösung: R = {} m   H = {} m  (3. Punkt für Eindeutigkeit empfohlen)",
            f(e.r2, 3),
            f(e.h2, 3)
        ));
    }
    Ok(Ablage {
        protokoll,
        anzeige: Anzeige {
            zeilen: vec![format!("R:  {} m", f(e.r, 3)), format!("H:  {} m", f(e.h, 3))],
            info,
            warnung: String::new(),
            kopf: vec!["PunktNr", "v [mm]"],
            residuen: e
                .residuen
                .iter()
                .map(|r| {
                    let v = if r.aktiv { res_ampel(r.v_strecke_mm, STRECKE_AMPEL) } else { res_leer() };
                    vec![res_zelle(&r.punkt_nr), v]
                })
                .collect(),
        },
        werte: vec![("PunktNr", name.to_string()), ("R [m]", f(e.r, 3)), ("H [m]", f(e.h, 3)), ("s0 [mm]", f(e.s0_mm, 2))],
        punkt: neuer_punkt(name, TYP_NEUPUNKT, e.r, e.h, 0.0, 0.0, false, "Bogenschnitt"),
    })
}

fn rechne_hochpunkt(eingaben: &[Eingabe], aktiv: &[bool], name: &str, bearbeiter: &str) -> Result<Ablage, String> {
    let messungen: Vec<hochpunkt::HochpunktMessung> = eingaben
        .iter()
        .map(|e| hochpunkt::HochpunktMessung {
            punkt_nr: e.nr.clone(),
            r: e.w[0],
            h: e.w[1],
            hoehe: e.w[2],
            ih: e.w[3],
            hz: e.w[4],
            z: e.w[5],
            v: e.w[6],
            zielhoehe: e.w[7],
        })
        .collect();
    let e = hochpunkt::berechnen(&messungen, Some(aktiv), rueckwaertsschnitt::STANDARD_RICHTUNG_CC, rueckwaertsschnitt::STANDARD_M0_MM)?;
    let protokoll = prot_berechnungen::hochpunkt(&e, &messungen, name, bearbeiter, jetzt());
    let richtung = if e.redundanz_dir > 0 {
        let mut t = format!("Richtungen – s₀ = {} mm    r = {}    {} Iter.", f(e.s0_dir_mm, 2), e.redundanz_dir, e.iterationen);
        if !e.konvergiert {
            t.push_str("  !! NICHT konvergiert !!");
        }
        t
    } else {
        format!("Richtungen – r = 0  (eindeutig bestimmt, {} Iter.)", e.iterationen)
    };
    let hoehe = if e.redundanz_h > 0 {
        format!("Höhe – s₀ = {} mm    r = {}", f(e.s0_h_mm, 2), e.redundanz_h)
    } else {
        "Höhe – r = 0  (eindeutig bestimmt)".to_string()
    };
    Ok(Ablage {
        protokoll,
        anzeige: Anzeige {
            zeilen: vec![format!("R:  {} m", f(e.r, 3)), format!("H:  {} m", f(e.h, 3)), format!("Höhe:  {} m", f(e.hoehe, 3))],
            info: vec![richtung, hoehe],
            warnung: String::new(),
            kopf: vec!["PunktNr", "s [m]", "v Dir [cc]", "Höhe PI [m]", "v H [mm]"],
            residuen: e
                .residuen
                .iter()
                .map(|r| {
                    vec![
                        res_zelle(&r.punkt_nr),
                        if r.aktiv_dir { res_zelle(f(r.s_horiz, 1)) } else { res_leer() },
                        if r.aktiv_dir { res_ampel(r.v_dir_cc, RICHTUNG_AMPEL) } else { res_leer() },
                        if r.aktiv_h { res_zelle(f(r.hoehe_pi, 3)) } else { res_leer() },
                        if r.aktiv_h { res_ampel(r.v_h_mm, STRECKE_AMPEL) } else { res_leer() },
                    ]
                })
                .collect(),
        },
        werte: vec![
            ("PunktNr", name.to_string()),
            ("R [m]", f(e.r, 3)),
            ("H [m]", f(e.h, 3)),
            ("Höhe [m]", f(e.hoehe, 3)),
            ("s0Dir [mm]", f(e.s0_dir_mm, 2)),
            ("s0H [mm]", f(e.s0_h_mm, 2)),
        ],
        punkt: neuer_punkt(name, TYP_NEUPUNKT, e.r, e.h, e.hoehe, 0.0, true, "Hochpunktherablegung"),
    })
}

/// Auswahl der Berechnungen.
#[derive(Default)]
pub struct BerechnungenAnsicht {
    offen: Option<Box<SchnittAnsicht>>,
    transformation: Option<Box<KoordTransformationAnsicht>>,
    hinweis: Option<&'static str>,
}

impl BerechnungenAnsicht {
    pub fn zeichne(&mut self, ui: &mut egui::Ui, projekt: &mut Projekt) {
        if self.offen.is_some() || self.transformation.is_some() {
            if ui.button("← Berechnungen").clicked() {
                self.offen = None;
                self.transformation = None;
                return;
            }
            ui.add_space(4.0);
            egui::ScrollArea::both().show(ui, |ui| {
                if let Some(schnitt) = self.offen.as_mut() {
                    schnitt.zeichne(ui, projekt);
                }
                if let Some(transformation) = self.transformation.as_mut() {
                    transformation.zeichne(ui, projekt);
                }
            });
            return;
        }
        ui.heading("Berechnungen");
        ui.add_space(8.0);
        if ui.add_sized([260.0, 34.0], egui::Button::new("Koordinatentransformation")).clicked() {
            self.hinweis = None;
            self.transformation = Some(Box::new(KoordTransformationAnsicht::neu(projekt)));
        }
        let knoepfe: [(&str, Option<Art>); 4] = [
            ("Rückwärtsschnitt", Some(Art::Rueckwaerts)),
            ("Vorwärtsschnitt", Some(Art::Vorwaerts)),
            ("Bogenschnitt", Some(Art::Bogen)),
            ("Hochpunktherablegung", Some(Art::Hochpunkt)),
        ];
        let mut neue = None;
        for (name, art) in knoepfe {
            if ui.add_sized([260.0, 34.0], egui::Button::new(name)).clicked() {
                match art {
                    Some(a) => neue = Some(a),
                    None => self.hinweis = Some("Dieses Modul ist in Feldbuch-NT noch nicht umgesetzt."),
                }
            }
        }
        if let Some(art) = neue {
            self.hinweis = None;
            self.offen = Some(Box::new(SchnittAnsicht::neu(art, projekt)));
        }
        if let Some(hinweis) = self.hinweis {
            ui.add_space(8.0);
            ui.label(hinweis);
        }
    }
}
