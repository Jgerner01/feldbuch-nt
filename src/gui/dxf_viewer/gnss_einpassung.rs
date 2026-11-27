// Zweck:    Einpassungsfenster im DXF-Viewer (GNSS): T-Punkt im Plan wählen, mit GNSS messen und übernehmen,
//           Methode und Höheneinpassung wählen, Residuen mit Ampel und Aktiv-Schaltern, Kontrollpunkt, Übernahme als
//           aktive Einpassung, RTF-Protokoll und Neuberechnung der GNSS-Punkte (GNSS.md Abschnitt 8)
// Hardware: GNSS-Empfänger u-blox ZED-F9P (über das GNSS-Modul)
// Autor:    Johann Gerner

use chrono::Local;
use eframe::egui::{self, Color32, RichText, Vec2};

use super::DxfViewer;
use super::gnss::{MM_PRO_M, Messwert, Soll, TYP_GNSS_KONTROLLE, TYP_GNSS_TPUNKT};
use crate::daten::messprotokoll::{self, Eintrag, Ereignis};
use crate::daten::projekt::Projekt;
use crate::gnss::einpassung::{EinpassMethode, Einpassung, EinpassungsDatei, GnssBezug, HoehenEinpassung, TPunkt};
use crate::gnss::punkte as gp;
use crate::grundlagen::format::f;
use crate::grundlagen::{protokoll, ton};
use crate::protokolle::{schreibe_fuer_projekt, transformation};
use crate::rechnen::koord_transformation::TransformPunkt;

/// Anfangsgröße des Fensters.
const FENSTER_GROESSE: [f32; 2] = [760.0, 520.0];
/// Ampelfarben der Residuen.
const AMPEL_GRUEN: Color32 = Color32::from_rgb(40, 170, 60);
const AMPEL_GELB: Color32 = Color32::from_rgb(220, 170, 0);
const AMPEL_ROT: Color32 = Color32::from_rgb(220, 60, 40);
const AMPEL_TIEFROT: Color32 = Color32::from_rgb(140, 0, 0);
/// Millimeter je Zentimeter.
const MM_PRO_CM: f64 = 10.0;

/// Zustand des Einpassungsfensters (Arbeitskopie der Einpassung).
pub(in crate::gui::dxf_viewer) struct EinpassFenster {
    pub ep: Einpassung,
    pub soll: Option<Soll>,
    /// Messen-Knopf misst einen Kontrollpunkt statt eines T-Punkts
    pub kontrolle: bool,
    pub meldung: String,
    pub kontrolle_ergebnis: Option<(String, Color32)>,
    /// ungespeicherte Änderungen
    pub geaendert: bool,
}

/// Ampelfarbe eines Residuums.
fn ampel(v_mm: f64, g: &crate::daten::rechenparameter::GnssParameter) -> Color32 {
    let cm = v_mm.abs() / MM_PRO_CM;
    if cm <= g.ampel_gruen_cm {
        AMPEL_GRUEN
    } else if cm <= g.ampel_gelb_cm {
        AMPEL_GELB
    } else if cm <= g.ampel_rot_cm {
        AMPEL_ROT
    } else {
        AMPEL_TIEFROT
    }
}

impl DxfViewer {
    /// Öffnet oder schließt das Einpassungsfenster.
    pub(super) fn einpassung_umschalten(&mut self, projekt: &Projekt) {
        if self.gnss.fenster.is_some() {
            self.gnss.fenster = None;
            return;
        }
        // Arbeitskopie: aktive Einpassung oder eine neue mit den Vorgaben der Rechenparameter
        let g = &projekt.rechenparameter.params.gnss;
        let ep = match self.gnss.einpassungen.aktive() {
            Some(ep) => ep.clone(),
            None => {
                let datum = Local::now().format("%Y-%m-%d").to_string();
                Einpassung::neu(self.gnss.einpassungen.neue_id(&datum), g.methode, g.hoehe, &self.gnss.hoehenbezug)
            }
        };
        self.gnss.absteckung = None;
        self.gnss.fenster = Some(EinpassFenster {
            ep,
            soll: None,
            kontrolle: false,
            meldung: "Identischen Punkt im Plan antippen, zum Punkt gehen und messen.".into(),
            kontrolle_ergebnis: None,
            geaendert: false,
        });
        if g.bezug != GnssBezug::Einpassung {
            self.status = "Hinweis: Koordinatenbezug ist nicht „Einpassung“ (Rechenparameter → GNSS)".into();
        }
    }

    /// Ein gemessener T-Punkt wird der Einpassung hinzugefügt (weitere Belegung: gemittelt).
    pub(super) fn einpassung_tpunkt(&mut self, projekt: &Projekt, soll: &Soll, m: &Messwert) {
        let utm_k = self.gnss.utm_k;
        let Some(fenster) = self.gnss.fenster.as_mut() else {
            return;
        };
        let punkt = TPunkt {
            nr: soll.nr.clone(),
            soll_r: soll.r,
            soll_h: soll.h,
            soll_hoehe: soll.hoehe,
            ist_e: m.e,
            ist_n: m.n,
            ist_hoehe: Some(m.h_bezug.unwrap_or(m.h_ell)),
            s_lage_mm: m.s_lage_mm(),
            s_hoehe_mm: m.s_h_mm,
            epochen: m.epochen,
            belegungen: 1,
            lage_aktiv: true,
            hoehe_aktiv: soll.hoehe.is_some(),
            zeit: messprotokoll::zeit_text(Local::now().naive_local()),
        };
        let differenz = fenster.ep.setze_punkt(punkt);
        fenster.ep.hoehenbezug.clone_from(&self.gnss.hoehenbezug);
        fenster.ep.berechne(utm_k);
        fenster.geaendert = true;
        fenster.soll = None;
        fenster.meldung = match differenz {
            Some((lage, hoehe)) => format!(
                "T-Punkt {} erneut gemessen und gemittelt: Differenz Lage {} mm{}",
                soll.nr,
                f(lage * MM_PRO_M, 0),
                hoehe.map_or(String::new(), |h| format!(", Höhe {} mm", f(h * MM_PRO_M, 0)))
            ),
            None => format!("T-Punkt {} übernommen ({} Epochen, sL {} mm).", soll.nr, m.epochen, f(m.s_lage_mm(), 0)),
        };
        let text = format!("T-Punkt der Einpassung {}{}", fenster.ep.id, if differenz.is_some() { ", weitere Belegung gemittelt" } else { "" });
        messprotokoll::protokolliere(
            &projekt.einstellungen,
            Eintrag::neu(Ereignis::Messung).punkt(&soll.nr, TYP_GNSS_TPUNKT, "").koordinaten(soll.r, soll.h, soll.hoehe.unwrap_or(f64::NAN)).mit_gnss(&m.werte).text(text),
        );
        ton::bestaetigung();
    }

    /// Kontrollpunkt: Abweichung zwischen eingepasster GNSS-Messung und Soll, ohne die Einpassung zu verändern.
    pub(super) fn einpassung_kontrolle(&mut self, projekt: &Projekt, soll: &Soll, m: &Messwert) {
        let g = projekt.rechenparameter.params.gnss.clone();
        // Kontrolle gegen die Einpassung im Fenster (sonst die aktive)
        let pos = match self.gnss.fenster.as_ref().and_then(|f| f.ep.ergebnis.as_ref()) {
            Some(erg) => {
                let (r, h, hoehe) = erg.transformiere(m.e, m.n, Some(m.h_bezug.unwrap_or(m.h_ell)));
                (r, h, hoehe)
            }
            None => (m.pos.r, m.pos.h, m.pos.hoehe),
        };
        let d_lage = (pos.0 - soll.r).hypot(pos.1 - soll.h) * MM_PRO_M;
        let d_hoehe = pos.2.zip(soll.hoehe).map(|(a, b)| (a - b) * MM_PRO_M);
        let ok = d_lage <= g.kontrolle_lage_mm && d_hoehe.is_none_or(|d| d.abs() <= g.kontrolle_hoehe_mm);
        let text = format!(
            "Kontrolle {}: Lage {} mm{}  →  {}",
            soll.nr,
            f(d_lage, 0),
            d_hoehe.map_or(String::new(), |d| format!(", Höhe {} mm", f(d, 0))),
            if ok { "in Ordnung" } else { "ÜBERSCHRITTEN" }
        );
        let farbe = if ok { AMPEL_GRUEN } else { AMPEL_ROT };
        if let Some(fenster) = self.gnss.fenster.as_mut() {
            fenster.kontrolle_ergebnis = Some((text.clone(), farbe));
            fenster.soll = None;
        }
        messprotokoll::protokolliere(
            &projekt.einstellungen,
            Eintrag::neu(Ereignis::Messung)
                .punkt(&soll.nr, TYP_GNSS_KONTROLLE, "")
                .koordinaten(pos.0, pos.1, pos.2.unwrap_or(f64::NAN))
                .mit_gnss(&m.werte)
                .text(format!("{text} (Grenzen {} / {} mm)", f(g.kontrolle_lage_mm, 0), f(g.kontrolle_hoehe_mm, 0))),
        );
        if ok {
            ton::bestaetigung();
        } else {
            ton::fehler();
        }
        self.status = text;
    }

    /// Zeichnet das Einpassungsfenster.
    pub(super) fn zeige_einpassfenster(&mut self, ctx: &egui::Context, projekt: &mut Projekt) {
        let Some(mut fenster) = self.gnss.fenster.take() else {
            return;
        };
        let g = projekt.rechenparameter.params.gnss.clone();
        let utm_k = self.gnss.utm_k;
        let mut offen = true;
        let mut aktion = None;
        let misst = self.gnss.misst();
        egui::Window::new(format!("Einpassung {}", fenster.ep.id))
            .open(&mut offen)
            .default_size(FENSTER_GROESSE)
            .default_pos(ctx.content_rect().right_top() + Vec2::new(-FENSTER_GROESSE[0] - 20.0, 90.0))
            .show(ctx, |ui| {
                aktion = zeichne_inhalt(ui, &mut fenster, &self.gnss.einpassungen, &g, utm_k, misst);
            });
        self.gnss.fenster = Some(fenster);
        match aktion {
            Some(Aktion::Messen) => self.gnss_messen_geklickt(projekt),
            Some(Aktion::Uebernehmen) => self.einpassung_uebernehmen(projekt),
            Some(Aktion::Protokoll) => self.einpassung_protokoll(projekt),
            Some(Aktion::NeuRechnen) => self.gnss_punkte_neu_rechnen(projekt),
            Some(Aktion::Neu) => {
                let datum = Local::now().format("%Y-%m-%d").to_string();
                if let Some(f) = self.gnss.fenster.as_mut() {
                    f.ep = Einpassung::neu(self.gnss.einpassungen.neue_id(&datum), g.methode, g.hoehe, &self.gnss.hoehenbezug);
                    f.geaendert = false;
                    f.meldung = "Neue Einpassung – identische Punkte im Plan wählen und messen.".into();
                }
            }
            Some(Aktion::Laden(id)) => {
                if let (Some(f), Some(ep)) = (self.gnss.fenster.as_mut(), self.gnss.einpassungen.einpassungen.iter().find(|e| e.id == id)) {
                    f.ep = ep.clone();
                    f.geaendert = false;
                    f.meldung = format!("Einpassung {id} geladen.");
                }
            }
            None => {}
        }
        if !offen {
            self.gnss.fenster = None;
        }
    }

    /// Speichert die Einpassung, macht sie aktiv und vermerkt das im Messprotokoll.
    fn einpassung_uebernehmen(&mut self, projekt: &Projekt) {
        let Some(fenster) = self.gnss.fenster.as_mut() else {
            return;
        };
        let Some(erg) = fenster.ep.ergebnis.as_ref() else {
            fenster.meldung = fenster.ep.fehler.clone().unwrap_or_else(|| "Einpassung ist noch nicht berechnet.".into());
            return;
        };
        let text = format!(
            "{} · {} · {} T-Punkte · s0 Lage {} mm{} · Maßstab {} · Schwerpunkt UTM {} / {}",
            fenster.ep.methode.anzeige(),
            if fenster.ep.methode.ist_3d() { "Höhe 3D" } else { fenster.ep.hoehe_modus.anzeige() },
            fenster.ep.aktive_lage(),
            f(erg.lage.s0_mm, 1),
            erg.hoehe.as_ref().map_or(String::new(), |h| format!(", s0 Höhe {} mm", f(h.s0_mm, 1))),
            f(erg.lage.massstab(), 7),
            f(erg.lage.quelle_sp.0, 3),
            f(erg.lage.quelle_sp.1, 3),
        );
        fenster.ep.uebernommen = messprotokoll::zeit_text(Local::now().naive_local());
        let ep = fenster.ep.clone();
        fenster.geaendert = false;
        let id = ep.id.clone();
        self.gnss.einpassungen.setze(ep);
        self.gnss.einpassungen.aktiv = Some(id.clone());
        match self.gnss.einpassungen.speichere(&EinpassungsDatei::pfad(&projekt.einstellungen)) {
            Ok(()) => {
                if let Some(f) = self.gnss.fenster.as_mut() {
                    f.meldung = format!("Einpassung {id} ist aktiv. GNSS-Punkte ohne oder mit anderer Einpassung lassen sich neu rechnen.");
                }
            }
            Err(e) => {
                if let Some(f) = self.gnss.fenster.as_mut() {
                    f.meldung = e;
                }
            }
        }
        messprotokoll::protokolliere(&projekt.einstellungen, Eintrag::neu(Ereignis::Einpassung).punkt(&id, "", "").text(text));
        protokoll::log("GNSS", &format!("Einpassung {id} übernommen"));
    }

    /// RTF-Protokoll der Lageeinpassung (über das Protokoll der Koordinatentransformation).
    fn einpassung_protokoll(&mut self, projekt: &Projekt) {
        let Some(fenster) = self.gnss.fenster.as_mut() else {
            return;
        };
        let Some(kt_erg) = fenster.ep.ergebnis.as_ref().and_then(|e| e.lage.kt.as_ref()) else {
            fenster.meldung = "Protokoll nur nach einer Berechnung mit Helmert- oder 3D-Methode.".into();
            return;
        };
        let aktive: Vec<&TPunkt> = fenster.ep.punkte.iter().filter(|p| p.lage_aktiv).collect();
        let quelle: Vec<TransformPunkt> = aktive.iter().map(|p| TransformPunkt::neu(&p.nr, p.ist_e, p.ist_n, p.ist_hoehe.unwrap_or(0.0))).collect();
        let ziel: Vec<TransformPunkt> = aktive.iter().map(|p| TransformPunkt::neu(&p.nr, p.soll_r, p.soll_h, p.soll_hoehe.unwrap_or(0.0))).collect();
        let p = transformation::protokoll(kt_erg, &quelle, &ziel, None, &projekt.einstellungen.projekt_name, &projekt.projektdaten.bearbeiter, Local::now().naive_local());
        fenster.meldung = match schreibe_fuer_projekt(&p, projekt) {
            Ok(pfad) => format!("Protokoll gespeichert: {} (Parameter bezogen auf die Schwerpunkte)", pfad.display()),
            Err(e) => e,
        };
    }

    /// Rechnet alle GNSS-Punkte des Projekts mit dem aktuellen Bezug bzw. der aktiven Einpassung neu.
    fn gnss_punkte_neu_rechnen(&mut self, projekt: &Projekt) {
        let g = &projekt.rechenparameter.params.gnss;
        let id = self.gnss.einpassung_id(g.bezug);
        let mut anzahl = 0;
        let mut punkte = std::mem::take(&mut self.gnss.punkte);
        for p in &mut punkte {
            let pos = self.gnss.plan(g.bezug, g.extrapolation_prozent, p.e_utm, p.n_utm, Some(p.h_bezug.unwrap_or(p.h_ell)));
            let neu_id = if pos.ohne_einpassung { String::new() } else { id.clone() };
            if (pos.r - p.r).abs() > f64::EPSILON || (pos.h - p.h).abs() > f64::EPSILON || neu_id != p.einpassung_id {
                (p.r, p.h, p.hoehe) = (pos.r, pos.h, pos.hoehe);
                p.einpassung_id = neu_id;
                anzahl += 1;
            }
        }
        self.gnss.punkte = punkte;
        let meldung = match gp::schreibe(&gp::pfad(&projekt.einstellungen), &self.gnss.punkte) {
            Ok(()) => {
                messprotokoll::protokolliere(
                    &projekt.einstellungen,
                    Eintrag::neu(Ereignis::Bemerkung).quelle(messprotokoll::QUELLE_PROGRAMM).text(format!("{anzahl} GNSS-Punkte mit Einpassung „{id}“ neu gerechnet")),
                );
                format!("{anzahl} GNSS-Punkte neu gerechnet.")
            }
            Err(e) => format!("GNSS-Punkte nicht gespeichert: {e}"),
        };
        if let Some(f) = self.gnss.fenster.as_mut() {
            f.meldung = meldung;
        }
    }
}

/// Wünsche aus dem Fenster.
enum Aktion {
    Messen,
    Uebernehmen,
    Protokoll,
    NeuRechnen,
    Neu,
    Laden(String),
}

fn zeichne_inhalt(
    ui: &mut egui::Ui,
    fenster: &mut EinpassFenster,
    datei: &EinpassungsDatei,
    g: &crate::daten::rechenparameter::GnssParameter,
    utm_k: Option<f64>,
    misst: bool,
) -> Option<Aktion> {
    let mut aktion = None;
    let mut neu_rechnen = false;
    // Auswahl der Einpassung
    ui.horizontal(|ui| {
        ui.label("Einpassung:");
        egui::ComboBox::from_id_salt("gnss_einpassung_wahl").selected_text(fenster.ep.id.clone()).show_ui(ui, |ui| {
            for ep in &datei.einpassungen {
                let aktiv = datei.aktiv.as_deref() == Some(ep.id.as_str());
                if ui.selectable_label(ep.id == fenster.ep.id, format!("{}{}", ep.id, if aktiv { "  (aktiv)" } else { "" })).clicked() {
                    aktion = Some(Aktion::Laden(ep.id.clone()));
                }
            }
        });
        if ui.button("Neue Einpassung").clicked() {
            aktion = Some(Aktion::Neu);
        }
    });
    ui.horizontal(|ui| {
        ui.label("Lage:");
        egui::ComboBox::from_id_salt("gnss_einpass_methode").selected_text(fenster.ep.methode.anzeige()).show_ui(ui, |ui| {
            for m in EinpassMethode::ALLE {
                neu_rechnen |= ui.selectable_value(&mut fenster.ep.methode, m, m.anzeige()).changed();
            }
        });
        ui.label("Höhe:");
        ui.add_enabled_ui(!fenster.ep.methode.ist_3d(), |ui| {
            egui::ComboBox::from_id_salt("gnss_einpass_hoehe").selected_text(fenster.ep.hoehe_modus.anzeige()).show_ui(ui, |ui| {
                for h in HoehenEinpassung::ALLE {
                    neu_rechnen |= ui.selectable_value(&mut fenster.ep.hoehe_modus, h, h.anzeige()).changed();
                }
            });
        });
    });
    ui.separator();
    // Soll und Messen
    ui.horizontal(|ui| {
        ui.radio_value(&mut fenster.kontrolle, false, "T-Punkt messen");
        ui.radio_value(&mut fenster.kontrolle, true, "Kontrollpunkt messen");
        let text = if misst { "■ Abbrechen" } else if fenster.kontrolle { "► Kontrolle messen" } else { "► T-Punkt messen" };
        let frei = misst || fenster.soll.is_some();
        if ui.add_enabled(frei, egui::Button::new(RichText::new(text).strong().size(16.0)).min_size(Vec2::new(210.0, 46.0))).on_disabled_hover_text("Erst einen Punkt im Plan antippen").clicked() {
            aktion = Some(Aktion::Messen);
        }
    });
    ui.label(&fenster.meldung);
    if let Some((text, farbe)) = &fenster.kontrolle_ergebnis {
        ui.colored_label(*farbe, text);
    }
    ui.separator();
    // T-Punkte mit Residuen
    let residuen = fenster.ep.ergebnis.as_ref().map(|e| e.residuen.clone()).unwrap_or_default();
    let mut loeschen = None;
    egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
        egui::Grid::new("gnss_tpunkte").striped(true).spacing([10.0, 3.0]).show(ui, |ui| {
            for kopf in ["L", "H", "Nr", "R soll", "H soll", "Höhe soll", "sL mm", "n", "vR mm", "vH mm", "vHöhe mm", "v mm", ""] {
                ui.strong(kopf);
            }
            ui.end_row();
            for (i, p) in fenster.ep.punkte.iter_mut().enumerate() {
                neu_rechnen |= ui.checkbox(&mut p.lage_aktiv, "").on_hover_text("Lage in der Einpassung verwenden").changed();
                ui.add_enabled_ui(p.soll_hoehe.is_some() && p.ist_hoehe.is_some(), |ui| {
                    neu_rechnen |= ui.checkbox(&mut p.hoehe_aktiv, "").on_hover_text("Höhe in der Einpassung verwenden").changed();
                });
                ui.label(&p.nr);
                ui.monospace(f(p.soll_r, 3));
                ui.monospace(f(p.soll_h, 3));
                ui.monospace(p.soll_hoehe.map_or("–".into(), |h| f(h, 3)));
                ui.monospace(f(p.s_lage_mm, 0));
                ui.monospace(if p.belegungen > 1 { format!("{}×{}", p.belegungen, p.epochen / p.belegungen.max(1)) } else { p.epochen.to_string() });
                let r = residuen.iter().find(|r| r.nr == p.nr);
                let zahl = |ui: &mut egui::Ui, w: Option<f64>| match w {
                    Some(v) => {
                        ui.colored_label(ampel(v, g), f(v, 0));
                    }
                    None => {
                        ui.label("–");
                    }
                };
                zahl(ui, r.and_then(|r| r.v_r));
                zahl(ui, r.and_then(|r| r.v_h));
                zahl(ui, r.and_then(|r| r.v_hoehe));
                zahl(ui, r.and_then(|r| r.v_lage()));
                if ui.small_button("🗑").on_hover_text("T-Punkt entfernen").clicked() {
                    loeschen = Some(i);
                }
                ui.end_row();
            }
        });
    });
    if let Some(i) = loeschen {
        fenster.ep.punkte.remove(i);
        neu_rechnen = true;
    }
    if neu_rechnen {
        fenster.ep.berechne(utm_k);
        fenster.geaendert = true;
    }
    ui.separator();
    // Ergebnis
    match (&fenster.ep.ergebnis, &fenster.ep.fehler) {
        (Some(erg), _) => {
            let mut zeile = format!("s0 Lage {} mm · Maßstab {}", f(erg.lage.s0_mm, 1), f(erg.lage.massstab(), 7));
            if let Some(w) = erg.lage.drehung_gon() {
                zeile.push_str(&format!(" · Drehung {} gon", f(w, 5)));
            }
            if erg.lage.fest {
                zeile.push_str(&format!(" · Maßstab fest (UTM am Ort {})", f(erg.lage.massstab_vorgabe, 7)));
            }
            if let Some(h) = &erg.hoehe {
                zeile.push_str(&format!(" · Höhe: Verschiebung {} m, s0 {} mm", f(h.a0, 3), f(h.s0_mm, 1)));
            }
            ui.label(zeile);
            for h in &erg.hinweise {
                ui.colored_label(if h.starts_with("Maßstab") { ui.visuals().text_color() } else { AMPEL_ROT }, h);
            }
        }
        (None, Some(fehler)) => {
            ui.colored_label(AMPEL_GELB, fehler);
        }
        (None, None) => {
            ui.label(format!("Noch keine T-Punkte – mindestens {} für {}.", fenster.ep.methode.min_punkte(), fenster.ep.methode.anzeige()));
        }
    }
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        let bereit = fenster.ep.ergebnis.is_some();
        let text = if fenster.geaendert { RichText::new("Übernehmen (aktiv setzen) *").strong() } else { RichText::new("Übernehmen (aktiv setzen)").strong() };
        if ui.add_enabled(bereit, egui::Button::new(text)).clicked() {
            aktion = Some(Aktion::Uebernehmen);
        }
        if ui.add_enabled(bereit, egui::Button::new("Protokoll (RTF)")).clicked() {
            aktion = Some(Aktion::Protokoll);
        }
        if ui.button("GNSS-Punkte neu rechnen").on_hover_text("Alle GNSS-Punkte des Projekts mit der aktiven Einpassung neu rechnen (Rohwerte bleiben erhalten)").clicked() {
            aktion = Some(Aktion::NeuRechnen);
        }
    });
    aktion
}
