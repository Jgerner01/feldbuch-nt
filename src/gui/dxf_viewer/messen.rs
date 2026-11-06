// Zweck:    Messablauf im DXF-Viewer: Messergebnis des Tachymeters verarbeiten (Neupunkt / Stationierung),
//           Messdaten schreiben, Zielrichtung (Live-Hz) anzeigen
// Hardware: Tachymeter über gui::geraet (GeoCOM / GSI Online)
// Autor:    Johann Gerner

use eframe::egui::{self, Align2, Color32, FontId, Pos2, Stroke, Vec2};

use crate::daten::messdaten;
use crate::daten::modelle::{NeupunktRohdaten, StationierungsErgebnis, StationierungsPunkt};
use crate::daten::projekt::Projekt;
use crate::grundlagen::format::{f, parse_zahl};
use crate::grundlagen::{fehler_log, protokoll};
use crate::rechnen::neupunkt;
use crate::rechnen::polar::{exzentrizitaet_gross, ist_massstab_eins, EXZ_INFO_GRENZE_M};
use crate::tachymeter::konfiguration::Konfiguration;
use crate::tachymeter::messung::TachymeterMessung;

use super::{Dialog, DxfViewer};
use crate::gui::geraet::Geraet;
use crate::tachymeter::messablauf::MessErgebnis;

/// Standard-Standpunktnummer, wenn das Feld leer ist.
const STANDARD_STANDPUNKT: &str = "SP01";
/// Präfix der automatisch vergebenen Anschlusspunktnummer.
const ANSCHLUSS_PRAEFIX: &str = "AP";
/// Mindestanzahl Anschlusspunkte für eine Stationierung.
const MIN_ANSCHLUSSPUNKTE: usize = 2;
/// Nachkommastellen der Exzentrizitätsfelder nach dem Zurücksetzen.
const EXZ_NULL_TEXT: &str = "0.000";
/// Vollkreis in gon.
const VOLLKREIS_GON: f64 = 400.0;
/// Gon → Radiant.
const GON_ZU_RAD: f64 = std::f64::consts::PI / 200.0;
/// Länge der Zielrichtungslinie als Anteil der kleineren Zeichenflächenseite.
const ZIEL_LINIE_ANTEIL: f32 = 0.45;
/// Zeilenabstand der beiden Winkelwerte am Ende der Zielrichtung [px].
const ZIEL_TEXT_ZEILE_PX: f32 = 13.0;
/// Farbe der Zielrichtung.
const ZIEL_FARBE: Color32 = Color32::from_rgb(220, 30, 30);
/// Strichlänge und Lücke der gestrichelten Zielrichtung [px].
const ZIEL_STRICH_PX: f32 = 8.0;
const ZIEL_LUECKE_PX: f32 = 5.0;

/// Leica-Prismen: (Name, Konstante [mm], Beschreibung).
const PRISMEN: [(&str, f64, &str); 5] = [
    ("GPR1 / GPH1P", 0.0, "Standard-Vollprisma"),
    ("GRZ4 / GRZ122", 0.0, "360°-Rundprisma"),
    ("GMP111 / GMP101", -17.5, "Miniaturprisma"),
    ("Folie / Kleintarget", 34.4, "Reflexfolie (EDM Tape)"),
    ("Reflektorlos", 0.0, "Messung ohne Prisma (RL)"),
];

/// Abbildung zum Prismentyp (rechts im Auswahlknopf).
fn prisma_bild(name: &str) -> egui::ImageSource<'static> {
    match name {
        "GPR1 / GPH1P" => egui::include_image!("../../../icons/prisma_standard.png"),
        "GRZ4 / GRZ122" => egui::include_image!("../../../icons/prisma_360.png"),
        "GMP111 / GMP101" => egui::include_image!("../../../icons/prisma_mini.png"),
        "Folie / Kleintarget" => egui::include_image!("../../../icons/prisma_folie.png"),
        _ => egui::include_image!("../../../icons/prisma_reflektorlos.png"),
    }
}
/// Kantenlänge der Abbildung im Auswahlknopf [px].
const PRISMA_BILD_PX: f32 = 40.0;
/// Abstand der Abbildung zum rechten Knopfrand [px].
const PRISMA_BILD_RAND: f32 = 8.0;
/// Zulässiger Bereich der Prismenkonstante [mm].
const PRISMA_GRENZE_MM: f64 = 50.0;
/// Name bei manuell eingegebener Konstante.
const PRISMA_MANUELL: &str = "Manuell";
/// Toleranz beim Vergleich der Konstanten [mm].
const PRISMA_TOLERANZ_MM: f64 = 1e-6;
/// Abstand der Dialogtasten [px].
const PRISMA_KNOPF: Vec2 = Vec2::new(150.0, 52.0);

fn vorzeichen_text(mm: f64) -> String {
    if mm > 0.0 { format!("+{}", f(mm, 1)) } else { f(mm, 1) }
}

fn finde_prisma(mm: f64) -> Option<usize> {
    PRISMEN.iter().position(|p| (p.1 - mm).abs() < PRISMA_TOLERANZ_MM)
}

/// Gespeicherter Prismenname hat Vorrang, weil mehrere Typen die Konstante 0,0 mm haben.
fn finde_prisma_nach_name(name: &str, mm: f64) -> Option<usize> {
    PRISMEN
        .iter()
        .position(|p| p.0 == name && (p.1 - mm).abs() < PRISMA_TOLERANZ_MM)
        .or_else(|| finde_prisma(mm))
}

impl DxfViewer {
    /// Öffnet die Prismenauswahl mit der aktuellen Konstante.
    pub(super) fn oeffne_prisma_dialog(&mut self, projekt: &Projekt) {
        let aktuell = projekt.einstellungen.prisma_konstante_mm;
        let auswahl = finde_prisma_nach_name(&projekt.einstellungen.prisma_name, aktuell);
        self.dialog = Some(Dialog::Prisma { auswahl, konstante: f(aktuell, 1) });
    }

    pub(super) fn zeichne_prisma_dialog(
        &mut self,
        ctx: &egui::Context,
        projekt: &mut Projekt,
        mut auswahl: Option<usize>,
        mut konstante: String,
    ) {
        let mut ergebnis: Option<bool> = None;
        egui::Window::new("Prismenkonstante")
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                for (i, (name, mm, beschreibung)) in PRISMEN.iter().enumerate() {
                    let aktiv = auswahl == Some(i);
                    let text = format!("{name}\n{beschreibung}  ({} mm)", vorzeichen_text(*mm));
                    let fill = if aktiv { Color32::from_rgb(40, 100, 175) } else { Color32::from_rgb(52, 64, 90) };
                    let rand = if aktiv { Color32::from_rgb(120, 180, 255) } else { Color32::from_rgb(80, 95, 130) };
                    let knopf = egui::Button::new(egui::RichText::new(text).color(Color32::WHITE))
                        .fill(fill)
                        .stroke(Stroke::new(1.0, rand))
                        .min_size(Vec2::new(PRISMA_KNOPF.x * 2.0, PRISMA_KNOPF.y));
                    let antwort = ui.add(knopf);
                    let bild_rect = egui::Rect::from_min_size(
                        egui::pos2(
                            antwort.rect.right() - PRISMA_BILD_RAND - PRISMA_BILD_PX,
                            antwort.rect.center().y - PRISMA_BILD_PX / 2.0,
                        ),
                        Vec2::splat(PRISMA_BILD_PX),
                    );
                    egui::Image::new(prisma_bild(name)).paint_at(ui, bild_rect);
                    if antwort.clicked() {
                        auswahl = Some(i);
                        konstante = f(*mm, 1);
                    }
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label("Konstante [mm]:");
                    if ui.add(egui::TextEdit::singleline(&mut konstante).desired_width(70.0)).changed() {
                        auswahl = parse_zahl(&konstante).and_then(finde_prisma);
                    }
                });
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.button("Übernehmen").clicked() {
                        ergebnis = Some(true);
                    }
                    if ui.button("Abbrechen").clicked() {
                        ergebnis = Some(false);
                    }
                });
            });
        match ergebnis {
            Some(true) => {
                let mm = parse_zahl(&konstante).unwrap_or(0.0).clamp(-PRISMA_GRENZE_MM, PRISMA_GRENZE_MM);
                let name = auswahl.map_or(PRISMA_MANUELL, |i| PRISMEN[i].0);
                projekt.einstellungen.prisma_name = name.to_string();
                projekt.einstellungen.prisma_konstante_mm = mm;
                let _ = projekt.einstellungen.speichern();
                self.status = format!("Prisma: {name}  –  {} mm", vorzeichen_text(mm));
            }
            Some(false) => {}
            None => self.dialog = Some(Dialog::Prisma { auswahl, konstante }),
        }
    }

    /// Wird jeden Bildaufbau aufgerufen: Live-Hz anfordern, Status und Messergebnis übernehmen.
    pub(super) fn bediene_geraet(&mut self, projekt: &mut Projekt, geraet: &mut Geraet) {
        // Wurde die Stationierung außerhalb des Viewers geändert (Fenster Freie Stationierung), Anzeigen erneuern
        if self.gesehene_station_version != projekt.station_version {
            self.gesehene_station_version = projekt.station_version;
            self.baue_overlay(projekt);
            self.baue_neupunkte_overlay(projekt);
            self.baue_qualitaets_overlay(projekt);
        }
        geraet.live_hz_aktiv = true;
        // V wird nur für die getrennte Messung (hier) und den Messdaten-Dialog (schaltet selbst ein) abgefragt
        geraet.live_v_aktiv = self.getrennt_messung.is_some();
        self.ziel_hz = geraet.live_hz;
        if let Some(text) = geraet.messung_status() {
            self.status = text.to_string();
        }
        let ergebnis = geraet.nimm_messergebnis().and_then(|e| self.messergebnis_fuer_dialog(e));
        match ergebnis {
            Some(MessErgebnis::Messung(m)) if self.neupunkt_modus && self.getrennt_modus => {
                // Getrennte Messung: nur die Strecke merken, Hz/V folgen beim Übernehmen
                if m.schraegstrecke_m.is_some() {
                    self.getrennt_messung = Some(*m);
                    self.status = "Strecke gemessen – Ziel anpeilen und „Übernehmen“ drücken.".to_string();
                } else {
                    self.status = "Messung unvollständig (Strecke fehlt).".to_string();
                }
            }
            Some(MessErgebnis::Messung(m)) => self.verarbeite_messung(projekt, geraet, &m),
            Some(MessErgebnis::Fehler(text)) => self.status = format!("Messung fehlgeschlagen: {text}"),
            None => {}
        }
    }

    /// Reaktion auf den Messen-Button.
    pub(super) fn messen_geklickt(&mut self, geraet: &mut Geraet) {
        let zielhoehe = parse_zahl(&self.txt_zielhoehe).unwrap_or(0.0);
        match geraet.starte_messung(zielhoehe) {
            Ok(()) => self.status = "Messung startet …".to_string(),
            Err(fehler) => self.status = fehler,
        }
    }

    /// „Übernehmen“ der getrennten Messung: Strecke aus der Messung, Hz und V von den aktuellen Gerätewerten.
    pub(super) fn uebernimm_getrennte_messung(&mut self, projekt: &mut Projekt, geraet: &Geraet) {
        let Some(mut m) = self.getrennt_messung.clone() else {
            return;
        };
        let (Some(hz), Some(v)) = (geraet.live_hz, geraet.live_v) else {
            self.status = "Keine aktuellen Hz/V vom Gerät – bitte kurz warten oder Verbindung prüfen.".to_string();
            return;
        };
        m.hz_gon = Some(hz);
        m.v_gon = Some(v);
        self.getrennt_messung = None;
        self.verarbeite_messung(projekt, geraet, &m);
    }

    fn lese_exzentrizitaet(text: &str) -> f64 {
        parse_zahl(text).unwrap_or(0.0)
    }

    fn exzentrizitaet_zuruecksetzen(&mut self) {
        self.txt_exz_laengs = EXZ_NULL_TEXT.to_string();
        self.txt_exz_quer = EXZ_NULL_TEXT.to_string();
    }

    /// Lokales System als Station: Standpunkt im Ursprung, keine Drehung, Maßstab 1.
    fn lokale_station() -> StationierungsErgebnis {
        StationierungsErgebnis {
            massstab: 1.0,
            konvergiert: true,
            ist_lokales_system: true,
            ..StationierungsErgebnis::default()
        }
    }

    /// Port von VerarbeiteMiessung des Originals.
    fn verarbeite_messung(&mut self, projekt: &mut Projekt, geraet: &Geraet, m: &TachymeterMessung) {
        let (Some(hz), Some(v), Some(strecke)) = (m.hz_gon, m.v_gon, m.schraegstrecke_m) else {
            self.status = "Messung unvollständig (Hz, V oder Strecke fehlt).".to_string();
            return;
        };

        let konfig = Konfiguration::aus_einstellungen(&projekt.einstellungen);
        let warnung = konfig.pruefe_messung(geraet.protokoll(), m);
        if let Some(w) = &warnung {
            protokoll::log("KONFIG", w);
        }

        let mut punkt_nr = self.txt_punkt_nr.trim().to_string();
        let code = self.txt_code.trim().to_string();
        let instr_hoehe = parse_zahl(&self.txt_instr_hoehe)
            .filter(|v| *v >= 0.0)
            .unwrap_or(projekt.einstellungen.instrumenten_hoehe);
        let mut standpunkt = self.txt_standpunkt.trim().to_string();
        if standpunkt.is_empty() {
            standpunkt = STANDARD_STANDPUNKT.to_string();
        }
        let zielhoehe = m.zielhoehe_m.unwrap_or(0.0);

        // Lückenloses Tagesprotokoll: jede Messung sofort sichern
        let protokoll_nr = if self.neupunkt_modus && punkt_nr.is_empty() {
            self.neupunkt_zaehler.to_string()
        } else {
            punkt_nr.clone()
        };
        let methode = if self.neupunkt_modus { "Neupunkt" } else { "Stationierung" };
        if let Err(fehler) =
            messdaten::schreibe_tagesprotokoll(&projekt.einstellungen, methode, &standpunkt, instr_hoehe, &protokoll_nr, &code, m)
        {
            fehler_log::log("DxfViewer.verarbeite_messung", &format!("Tagesprotokoll: {fehler}"));
        }

        if self.neupunkt_modus {
            if punkt_nr.is_empty() {
                punkt_nr = self.neupunkt_zaehler.to_string();
            }
            self.verarbeite_neupunkt(projekt, m, punkt_nr, code, standpunkt, instr_hoehe, (hz, v, strecke, zielhoehe));
        } else {
            self.verarbeite_stationierung(projekt, m, punkt_nr, standpunkt, instr_hoehe, (hz, v, strecke, zielhoehe));
        }

        if let Some(w) = warnung {
            self.status.push_str(&format!("  ⚠ {w}"));
        }
    }

    fn verarbeite_neupunkt(
        &mut self,
        projekt: &mut Projekt,
        m: &TachymeterMessung,
        punkt_nr: String,
        code: String,
        standpunkt: String,
        instr_hoehe: f64,
        (hz, v, strecke, zielhoehe): (f64, f64, f64, f64),
    ) {
        let roh = NeupunktRohdaten {
            punkt_nr: punkt_nr.clone(),
            code,
            hz_gon: hz,
            v_gon: v,
            strecke_m: strecke,
            zielhoehe_m: zielhoehe,
            zeitstempel: m.zeitstempel,
            standpunkt_nr: standpunkt.clone(),
            exz_laengs_m: Self::lese_exzentrizitaet(&self.txt_exz_laengs),
            exz_quer_m: Self::lese_exzentrizitaet(&self.txt_exz_quer),
        };
        messdaten::schreibe_neupunktmessung(&projekt.einstellungen, &roh, &standpunkt, instr_hoehe);

        let mut info = format!("NP {punkt_nr}: Hz={}  V={}  D={} m", f(hz, 4), f(v, 4), f(strecke, 3));

        let station = if self.lokales_system {
            let _ = projekt.set_instrumenten_hoehe(instr_hoehe);
            self.ziel_hz = Some(hz);
            Some(Self::lokale_station())
        } else if projekt.hat_gueltige_station() {
            projekt.station.ergebnis.clone()
        } else {
            None
        };

        match station {
            Some(station) => {
                let params = projekt.rechenparameter.params.clone();
                if let Some(erg) = neupunkt::berechnen(&roh, &station, &params, None, instr_hoehe) {
                    if let Err(fehler) = projekt.neupunkte.hinzufuegen_oder_ersetzen(roh.clone(), erg.clone()) {
                        self.status = format!("Neupunkt konnte nicht gespeichert werden: {fehler}");
                        return;
                    }
                    self.baue_neupunkte_overlay(projekt);
                    if self.lokales_system && self.ausserhalb_sichtbar(erg.r, erg.h) {
                        self.lokal_zeigen_ausstehend = true;
                    }
                    info = format!("NP {punkt_nr}:  R={}  H={}", f(erg.r, 3), f(erg.h, 3));
                    if erg.ist_3d {
                        info.push_str(&format!("  Höhe={}", f(erg.hoehe, 3)));
                    }
                    if !ist_massstab_eins(erg.verwendeter_massstab) {
                        info.push_str(&format!("  (m = {})", f(erg.verwendeter_massstab, 6)));
                    }
                }
            }
            None => info = format!("NP {punkt_nr} gespeichert (keine Stationierung – nur Rohdaten)"),
        }

        if exzentrizitaet_gross(roh.exz_laengs_m, roh.exz_quer_m) {
            info.push_str(&format!(
                "  ⚠ Exzentrizität eL={} eQ={} m (> {} m)",
                f(roh.exz_laengs_m, 3),
                f(roh.exz_quer_m, 3),
                f(EXZ_INFO_GRENZE_M, 0)
            ));
        }
        self.exzentrizitaet_zuruecksetzen();
        self.status = info;

        // Zähler erhöhen, wenn die Punktnummer numerisch ist
        if let Ok(nummer) = punkt_nr.parse::<i32>() {
            self.neupunkt_zaehler = nummer.saturating_add(1);
            projekt.einstellungen.neupunkt_zaehler = self.neupunkt_zaehler;
            let _ = projekt.einstellungen.speichern();
            self.txt_punkt_nr = self.neupunkt_zaehler.to_string();
        }
    }

    fn verarbeite_stationierung(
        &mut self,
        projekt: &mut Projekt,
        m: &TachymeterMessung,
        punkt_nr: String,
        standpunkt: String,
        instr_hoehe: f64,
        (hz, v, strecke, zielhoehe): (f64, f64, f64, f64),
    ) {
        let anschluss_nr = if punkt_nr.is_empty() {
            format!("{ANSCHLUSS_PRAEFIX}{}", projekt.station.messungen.len() + 1)
        } else {
            punkt_nr
        };
        let (r, h, hoehe) = projekt.finde_anschlusspunkt(&anschluss_nr).unwrap_or((0.0, 0.0, 0.0));
        let punkt = StationierungsPunkt { punkt_nr: anschluss_nr.clone(), r, h, hoehe, hz, v, strecke, zielhoehe };

        if projekt.station.standpunkt_nr.is_empty() || projekt.station.standpunkt_nr != standpunkt {
            if let Err(fehler) = projekt.neuer_standpunkt(&standpunkt, instr_hoehe) {
                self.status = format!("Standpunkt konnte nicht angelegt werden: {fehler}");
                return;
            }
            projekt.einstellungen.letzte_standpunkt_nr = standpunkt.clone();
            let _ = projekt.einstellungen.speichern();
        }
        let _ = projekt.set_instrumenten_hoehe(instr_hoehe);

        messdaten::schreibe_stationierungsmessung(&projekt.einstellungen, &punkt, &standpunkt, instr_hoehe, m.zeitstempel);

        let ergebnis = match projekt.hinzufuegen_und_berechnen(punkt) {
            Ok(e) => e,
            Err(fehler) => {
                self.status = format!("Stationierung nicht möglich: {fehler}");
                return;
            }
        };
        self.baue_overlay(projekt);

        match ergebnis {
            Some(erg) => {
                if !projekt.hat_gueltige_station() {
                    self.status = "Stationierung ungültig – Anschlusspunkte und Messungen prüfen.".to_string();
                    return;
                }
                self.baue_residual_overlay(projekt, &erg);
                // Die Neupunkte hat das Projekt bereits mit der neuen Stationierung neu berechnet
                let params = projekt.rechenparameter.params.clone();
                self.baue_neupunkte_overlay(projekt);
                messdaten::schreibe_standpunktinfo(
                    &projekt.einstellungen,
                    &standpunkt,
                    instr_hoehe,
                    &erg,
                    params.sigma_faktor_wirksam(),
                    params.sigma_wert(erg.s0_mm),
                );
                self.status = format!(
                    "Stationierung {standpunkt}: {}  r={}  n={}",
                    params.sigma_text(erg.s0_mm, 2),
                    erg.redundanz,
                    projekt.station.messungen.len()
                );
                if !erg.warnung_hoehe.is_empty() {
                    self.status.push_str(&format!(" | {}", erg.warnung_hoehe));
                }
                // Die gespeicherte Beobachtungsauswahl wurde angewendet: abgeschaltete Beobachtungen sichtbar machen
                let abgeschaltet = projekt.station.aktivierung().anzahl_abgeschaltet();
                if abgeschaltet > 0 {
                    self.status.push_str(&format!(" | {abgeschaltet} Beobachtung(en) abgeschaltet"));
                }
            }
            None => {
                let fehlend = MIN_ANSCHLUSSPUNKTE.saturating_sub(projekt.station.messungen.len());
                self.status = format!("Anschlusspunkt {anschluss_nr} gespeichert – noch {fehlend} Punkt(e) benötigt.");
            }
        }
    }

    /// Liegt der Weltpunkt außerhalb der Zeichenfläche?
    fn ausserhalb_sichtbar(&self, r: f64, h: f64) -> bool {
        let halb_x = f64::from(self.canvas_groesse.x) / 2.0 / self.scale;
        let halb_y = f64::from(self.canvas_groesse.y) / 2.0 / self.scale;
        (r - self.mitte[0]).abs() > halb_x || (h - self.mitte[1]).abs() > halb_y
    }

    /// Standpunkt und Orientierung für die Zielrichtung: (R, H, Orientierung [gon]).
    fn ziel_standpunkt(&self, projekt: &Projekt) -> Option<(f64, f64, f64)> {
        if self.lokales_system {
            return Some((0.0, 0.0, 0.0));
        }
        projekt
            .station
            .ergebnis
            .as_ref()
            .filter(|_| projekt.hat_gueltige_station())
            .map(|e| (e.r, e.h, e.orientierung_gon))
    }

    /// Zeichnet die Zielrichtung als rot gestrichelte Linie vom Standpunkt aus.
    pub(super) fn zeichne_zielrichtung(&self, maler: &egui::Painter, ansicht: &crate::dxf::entity::Ansicht, projekt: &Projekt) {
        let (Some(hz), Some((r, h, ori))) = (self.ziel_hz, self.ziel_standpunkt(projekt)) else {
            return;
        };
        let richtung = (hz + ori).rem_euclid(VOLLKREIS_GON);
        let winkel = richtung * GON_ZU_RAD;
        let start = ansicht.zu_bildschirm(r, h);
        // Textposition wie bisher; die Linie läuft darüber hinaus bis sicher über den Bildschirmrand
        let text_abstand = self.canvas_groesse.x.min(self.canvas_groesse.y) * ZIEL_LINIE_ANTEIL;
        let mitte_zeichenflaeche = Pos2::new(self.canvas_groesse.x / 2.0, self.canvas_groesse.y / 2.0);
        let laenge = self.canvas_groesse.length() * 2.0 + (start - mitte_zeichenflaeche).length();
        let einheit = Vec2::new(winkel.sin() as f32, -(winkel.cos() as f32));
        let stift = Stroke::new(1.5, ZIEL_FARBE);
        let mut pos = 0.0;
        while pos < laenge {
            let ende = (pos + ZIEL_STRICH_PX).min(laenge);
            maler.line_segment([start + einheit * pos, start + einheit * ende], stift);
            pos += ZIEL_STRICH_PX + ZIEL_LUECKE_PX;
        }
        // Hz und t untereinander
        let text_pos: Pos2 = start + einheit * text_abstand;
        let schrift = FontId::monospace(11.0);
        maler.text(text_pos, Align2::CENTER_BOTTOM, format!("t  {}", f(richtung, 4)), schrift.clone(), ZIEL_FARBE);
        let obere_zeile = text_pos - Vec2::new(0.0, ZIEL_TEXT_ZEILE_PX);
        maler.text(obere_zeile, Align2::CENTER_BOTTOM, format!("Hz {}", f(hz, 4)), schrift, ZIEL_FARBE);
    }
}
