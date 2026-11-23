// Zweck:    Punktabsteckung im DXF-Viewer (siehe Absteckung.md): Zielpunkt im Plan wählen, eigenes Absteckfenster auf
//           der rechten Hälfte des Viewers (folgt dessen Rändern) mit Animation – Tachymeter unten, Ziel oben,
//           Fernrohrrichtung live, Korrekturen nach der Messung –, Messen per Knopf oder Messtaste („€“),
//           Anfahren (GeoCOM), Toleranz-Rückfrage und Absteckprotokoll beim Schließen
// Hardware: Tachymeter über gui::geraet (Messen: GeoCOM/GSI Online, Anfahren: GeoCOM motorisiert)
// Autor:    Johann Gerner

use super::*;
use crate::daten::messdaten;
use crate::daten::messprotokoll::{self as mp, Eintrag, Ereignis};
use crate::daten::modelle::{NeupunktRohdaten, StationierungsPunkt};
use crate::gui::absteckung::gemeinsam::ProtokollZiel;
use crate::protokolle::absteckung::punktabsteckung;
use crate::rechnen::absteckung::{
    AbsteckAbweichung, AbsteckPunkt, STATUS_ABGESTECKT, StandpunktInfo, absteck_abweichung, berechne_einweisung, polar_zu, querablage_am_ziel,
};
use crate::tachymeter::befehle::Protokoll as GeraeteProtokoll;
use crate::tachymeter::messablauf::MessErgebnis;
use crate::tachymeter::messung::TachymeterMessung;

/// Fenstergröße, solange die Lage des Viewers unbekannt ist, und Mindestgröße.
const FENSTER_STANDARD: [f32; 2] = [760.0, 860.0];
const FENSTER_MIN: [f32; 2] = [420.0, 520.0];
/// Platz unter der Animation für Texte und Knöpfe.
const UNTEN_HOEHE: f32 = 330.0;
/// Abstände der Animation: oben über dem Zoomkreis, unten unter dem Standpunkt [px].
const RAND_OBEN: f32 = 34.0;
const RAND_UNTEN: f32 = 46.0;
/// Radius des Zoomkreises um das Ziel [px].
const ZOOM_RADIUS: f32 = 95.0;
/// Mögliche Maßstäbe des Zoomkreises (Außenring) [m].
const ZOOM_STUFEN_M: [f64; 14] = [0.01, 0.02, 0.05, 0.1, 0.2, 0.5, 1.0, 2.0, 5.0, 10.0, 20.0, 50.0, 100.0, 1000.0];
/// Knopfhöhe (Fingerbedienung).
const KNOPF_HOEHE: f32 = 44.0;
/// Schriftgröße der Korrekturen.
const KORREKTUR_SCHRIFT: f32 = 22.0;
/// Farben.
const FARBE_OK: Color32 = Color32::from_rgb(30, 150, 50);
const FARBE_WARNUNG: Color32 = Color32::from_rgb(215, 110, 0);
const FARBE_FEHLER: Color32 = Color32::from_rgb(200, 0, 0);
const FARBE_FERNROHR: Color32 = Color32::from_rgb(30, 90, 210);
const FARBE_SOLL: Color32 = Color32::from_rgb(130, 130, 130);
const FARBE_STANDPUNKT: Color32 = Color32::from_rgb(200, 30, 30);
const FARBE_ZIEL: Color32 = Color32::from_rgb(20, 60, 220);
const FARBE_TOLERANZ: Color32 = Color32::from_rgba_premultiplied(120, 200, 120, 70);
const FARBE_ANIMATION: Color32 = Color32::from_rgb(250, 251, 254);
/// Markierung im Plan: aktuelles Ziel orange, abgesteckt grün.
const PLAN_ZIEL: Color32 = Color32::from_rgb(235, 120, 0);
const PLAN_ABGESTECKT: Color32 = Color32::from_rgb(30, 150, 50);

/// Sollpunkt der Absteckung.
#[derive(Clone, Debug)]
pub(super) struct Ziel {
    pub nr: String,
    pub r: f64,
    pub h: f64,
    pub hoehe: Option<f64>,
    pub hz_soll: f64,
    pub s_soll: f64,
}

/// Letzte Messung zum aktuellen Ziel.
#[derive(Clone, Debug)]
struct Messung {
    roh: TachymeterMessung,
    ist: (f64, f64, Option<f64>),
    abw: AbsteckAbweichung,
    lfd: Option<u64>,
    uebernommen: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stufe {
    Info,
    Warnung,
    Fehler,
}

/// Was nach dem Abschließen der offenen Messung geschieht.
#[derive(Clone, Debug)]
enum Danach {
    Nichts,
    Schliessen,
    Ziel(Ziel),
}

/// Zustand der Absteckung (lebt, solange der Abstecken-Modus aktiv ist).
pub(super) struct Absteckung {
    pub ziel: Option<Ziel>,
    pub wartet: bool,
    eingang: Option<MessErgebnis>,
    messung: Option<Messung>,
    abgesteckt: Vec<AbsteckPunkt>,
    zielhoehe: String,
    code: String,
    /// Offene Rückfrage „Toleranz überschritten“ und was danach geschehen soll.
    frage: Option<Danach>,
    meldung: (String, Stufe),
    protokoll: ProtokollZiel,
    anfahren: bool,
    /// Die Absteckung ist beendet (Viewer entfernt sie).
    pub fertig: bool,
}

/// Text einer Korrektur: Betrag in m mit der Bewegungsrichtung.
fn korrektur_text(wert_m: f64, positiv: &str, negativ: &str) -> String {
    format!("{} m  {}", f(wert_m.abs(), 3), if wert_m > 0.0 { positiv } else { negativ })
}

/// Farbe nach Toleranz: grün innerhalb, orange bis zum Doppelten, sonst rot.
fn ampel(betrag_mm: f64, toleranz_mm: f64) -> Color32 {
    if betrag_mm <= toleranz_mm {
        FARBE_OK
    } else if betrag_mm <= 2.0 * toleranz_mm {
        FARBE_WARNUNG
    } else {
        FARBE_FEHLER
    }
}

/// Kleinste Zoomstufe, in die ein Versatz [m] passt (mindestens das Doppelte der Toleranz).
fn zoom_stufe(versatz_m: f64, toleranz_m: f64) -> f64 {
    let noetig = (versatz_m.abs() * 1.1).max(2.0 * toleranz_m);
    ZOOM_STUFEN_M.iter().copied().find(|s| *s >= noetig).unwrap_or(ZOOM_STUFEN_M[ZOOM_STUFEN_M.len() - 1])
}

/// Bildschirmabstand [px] eines Versatzes: linear im Zoomkreis, außerhalb gestaucht bis `max_px`.
fn versatz_px(versatz_m: f64, stufe_m: f64, max_px: f32) -> f32 {
    let a = versatz_m.abs();
    let px = if a <= stufe_m { (a / stufe_m) as f32 * ZOOM_RADIUS } else { ZOOM_RADIUS + (max_px - ZOOM_RADIUS).max(0.0) * (1.0 - (stufe_m / a) as f32) };
    px.copysign(versatz_m as f32)
}

/// Station für die Absteckung: aktive Stationierung oder lokales System.
fn station(projekt: &Projekt, lokal: bool) -> Option<StationierungsErgebnis> {
    if lokal {
        return Some(StationierungsErgebnis { massstab: 1.0, konvergiert: true, ist_lokales_system: true, ..StationierungsErgebnis::default() });
    }
    projekt.station.ergebnis.clone().filter(|_| projekt.hat_gueltige_station())
}

impl Absteckung {
    pub fn neu(projekt: &Projekt, zielhoehe: &str, code: &str) -> Self {
        Self {
            ziel: None,
            wartet: false,
            eingang: None,
            messung: None,
            abgesteckt: Vec::new(),
            zielhoehe: zielhoehe.to_string(),
            code: code.to_string(),
            frage: None,
            meldung: ("Zielpunkt im Plan antippen.".into(), Stufe::Info),
            protokoll: ProtokollZiel::neu(projekt),
            anfahren: false,
            fertig: false,
        }
    }

    fn melde(&mut self, text: impl Into<String>, stufe: Stufe) {
        self.meldung = (text.into(), stufe);
    }

    /// Übernimmt ein Messergebnis des Geräts (verarbeitet im nächsten Bild mit Projektzugriff).
    pub fn uebernimm(&mut self, ergebnis: MessErgebnis) {
        self.wartet = false;
        self.eingang = Some(ergebnis);
    }

    /// Sollpunkte für Protokoll und Planmarkierung.
    pub fn abgesteckte(&self) -> &[AbsteckPunkt] {
        &self.abgesteckt
    }

    /// Wertet eine eingegangene Messung aus: Ist-Punkt, Abweichung, Messprotokoll und dat.csv.
    fn verarbeite_eingang(&mut self, projekt: &Projekt, lokal: bool) {
        let Some(eingang) = self.eingang.take() else {
            return;
        };
        let m = match eingang {
            MessErgebnis::Fehler(text) => {
                self.melde(format!("Messung fehlgeschlagen: {text}"), Stufe::Fehler);
                crate::grundlagen::ton::fehler();
                return;
            }
            MessErgebnis::Messung(m) => *m,
        };
        let (Some(ziel), Some(st)) = (self.ziel.clone(), station(projekt, lokal)) else {
            return;
        };
        let (Some(hz), Some(v), Some(s)) = (m.hz_gon, m.v_gon, m.schraegstrecke_m) else {
            self.melde("Unvollständige Messung (Hz, V oder Strecke fehlt).", Stufe::Fehler);
            crate::grundlagen::ton::fehler();
            return;
        };
        let zh = m.zielhoehe_m.unwrap_or_else(|| parse_zahl(&self.zielhoehe).unwrap_or(0.0));
        let ih = projekt.station.instrumenten_hoehe;
        let roh = NeupunktRohdaten {
            aufstellung_id: projekt.station.aufstellung_id.clone(),
            punkt_nr: ziel.nr.clone(),
            code: self.code.trim().to_string(),
            hz_gon: hz,
            v_gon: v,
            strecke_m: s,
            zielhoehe_m: zh,
            zeitstempel: m.zeitstempel,
            standpunkt_nr: projekt.station.standpunkt_nr.clone(),
            exz_laengs_m: 0.0,
            exz_quer_m: 0.0,
        };
        let Some(ist) = crate::rechnen::neupunkt::berechnen(&roh, &st, &projekt.rechenparameter.params, None, ih) else {
            self.melde("Der Ist-Punkt ist mit dieser Messung nicht berechenbar.", Stufe::Fehler);
            crate::grundlagen::ton::fehler();
            return;
        };
        let ist_hoehe = ist.ist_3d.then_some(ist.hoehe);
        let hoehen = ziel.hoehe.zip(ist_hoehe);
        let abw = absteck_abweichung((st.r, st.h), (ziel.r, ziel.h), (ist.r, ist.h), hoehen);
        let p = &projekt.rechenparameter.params;
        let in_toleranz = abw.lage_in_toleranz(p.absteck_toleranz_lage_mm) && abw.hoehe_in_toleranz(p.absteck_toleranz_hoehe_mm);
        // Messprotokoll (Typ AB) und Messdatei
        let hoehe_text = abw.hoehe_m.map_or_else(|| "-".to_string(), |h| format!("{:+.1}", h * 1000.0));
        let text = format!(
            "Absteckung {}: Ist R={} H={}; längs {:+.1} quer {:+.1} Höhe {hoehe_text} mm",
            ziel.nr,
            f(ist.r, 3),
            f(ist.h, 3),
            abw.laengs_m * 1000.0,
            abw.quer_m * 1000.0
        );
        let lfd = mp::protokolliere(
            &projekt.einstellungen,
            Eintrag::neu(Ereignis::Messung)
                .mit_messung(&m)
                .mit_prisma(&projekt.einstellungen)
                .mit_aufstellung(&projekt.station)
                .punkt(&ziel.nr, messdaten::TYP_ABSTECKUNG, self.code.trim())
                .werte(hz, v, s, zh)
                .koordinaten(ziel.r, ziel.h, ziel.hoehe.unwrap_or(f64::NAN))
                .text(text),
        );
        let sp = StationierungsPunkt { punkt_nr: ziel.nr.clone(), r: ziel.r, h: ziel.h, hoehe: ziel.hoehe.unwrap_or(0.0), hz, v, strecke: s, zielhoehe: zh };
        let s_ = &projekt.station;
        if let Err(e) = messdaten::schreibe_absteckmessung(&projekt.einstellungen, &sp, self.code.trim(), &s_.standpunkt_nr, s_.instrumenten_hoehe, &s_.aufstellung_id, m.zeitstempel) {
            self.melde(format!("Messung ausgewertet, aber nicht in die Messdatei geschrieben: {e}"), Stufe::Warnung);
        } else if in_toleranz {
            self.melde(format!("{}: innerhalb der Toleranz – „Abgesteckt“ drücken.", ziel.nr), Stufe::Info);
        } else {
            self.melde(format!("{}: Reflektor versetzen und erneut messen.", ziel.nr), Stufe::Warnung);
        }
        crate::grundlagen::ton::bestaetigung();
        self.messung = Some(Messung { roh: m, ist: (ist.r, ist.h, ist_hoehe), abw, lfd, uebernommen: false });
    }

    /// Schließt die offene Messung ab: innerhalb der Toleranz automatisch, sonst mit Rückfrage. Danach folgt `danach`.
    fn abschliessen(&mut self, projekt: &Projekt, danach: Danach) {
        let p = &projekt.rechenparameter.params;
        match self.messung.as_ref().filter(|m| !m.uebernommen) {
            Some(m) if !(m.abw.lage_in_toleranz(p.absteck_toleranz_lage_mm) && m.abw.hoehe_in_toleranz(p.absteck_toleranz_hoehe_mm)) => {
                self.frage = Some(danach);
            }
            Some(_) => {
                self.uebernehmen(projekt, false);
                self.fuehre_aus(projekt, danach);
            }
            None => self.fuehre_aus(projekt, danach),
        }
    }

    /// Übernimmt die letzte Messung als abgesteckt (Protokollliste, Messprotokoll mit Bezug).
    fn uebernehmen(&mut self, projekt: &Projekt, ausser_toleranz: bool) {
        let (Some(ziel), Some(m)) = (self.ziel.clone(), self.messung.as_mut()) else {
            return;
        };
        m.uebernommen = true;
        let hoehe = m.abw.hoehe_m.map_or_else(|| "-".to_string(), |h| format!("{:+.0}", h * 1000.0));
        let werte = format!("l {:+.0} / q {:+.0} / h {hoehe} mm", m.abw.laengs_m * 1000.0, m.abw.quer_m * 1000.0);
        let mut punkt = AbsteckPunkt::neu(&ziel.nr, ziel.r, ziel.h);
        punkt.hz_soll_gon = ziel.hz_soll;
        punkt.s_soll_m = ziel.s_soll;
        punkt.status = if ausser_toleranz { format!("außerhalb Toleranz übernommen ({werte})") } else { format!("{STATUS_ABGESTECKT} ({werte})") };
        self.abgesteckt.retain(|a| a.punkt_nr != ziel.nr || a.r_soll != ziel.r || a.h_soll != ziel.h);
        self.abgesteckt.push(punkt);
        let text = format!("Punkt {} abgesteckt ({werte}){}", ziel.nr, if ausser_toleranz { " – außerhalb der Toleranz übernommen" } else { "" });
        let mut e = Eintrag::neu(Ereignis::Bemerkung).mit_aufstellung(&projekt.station).punkt(&ziel.nr, messdaten::TYP_ABSTECKUNG, "").bezug(m.lfd).quelle(mp::QUELLE_HAND).text(text.clone());
        e.feld = "Absteckung".into();
        mp::protokolliere(&projekt.einstellungen, e);
        self.melde(text, if ausser_toleranz { Stufe::Warnung } else { Stufe::Info });
    }

    fn fuehre_aus(&mut self, projekt: &Projekt, danach: Danach) {
        match danach {
            Danach::Nichts => {}
            Danach::Ziel(z) => {
                self.messung = None;
                self.melde(format!("Ziel {}: Fernrohr nach der Anzeige drehen, dann messen.", z.nr), Stufe::Info);
                self.ziel = Some(z);
            }
            Danach::Schliessen => {
                self.schreibe_protokoll(projekt);
                self.fertig = true;
            }
        }
    }

    /// Der Viewer wird geschlossen: eine offene Messung innerhalb der Toleranz wird übernommen (eine Rückfrage ist
    /// nicht mehr möglich), dann das Protokoll geschrieben.
    pub fn beim_schliessen_des_viewers(&mut self, projekt: &Projekt) {
        let p = &projekt.rechenparameter.params;
        let in_toleranz = self.messung.as_ref().is_some_and(|m| !m.uebernommen && m.abw.lage_in_toleranz(p.absteck_toleranz_lage_mm) && m.abw.hoehe_in_toleranz(p.absteck_toleranz_hoehe_mm));
        if in_toleranz {
            self.uebernehmen(projekt, false);
        }
        self.schreibe_protokoll(projekt);
    }

    /// Absteckprotokoll (RTF mit Lageplan) aller abgesteckten Punkte, beim Schließen automatisch.
    pub fn schreibe_protokoll(&mut self, projekt: &Projekt) {
        if self.abgesteckt.is_empty() {
            return;
        }
        let info = station(projekt, false).map(|e| StandpunktInfo { punkt_nr: projekt.station.standpunkt_nr.clone(), r: e.r, h: e.h, orientierung_gon: e.orientierung_gon });
        self.protokoll.aktualisiere(projekt);
        let p = punktabsteckung(info.as_ref(), &self.abgesteckt, &self.protokoll.bearbeiter, ProtokollZiel::jetzt());
        self.protokoll.schreibe_beim_verlassen(Some(p), "DxfViewer.Absteckprotokoll");
    }

    /// Fernrohrrichtung relativ zum Ziel: (ΔHz [gon], Querablage am Ziel [m]); > 0: Fernrohr zeigt rechts am Ziel vorbei.
    fn fernrohr(&self, live_hz: Option<f64>) -> Option<(f64, f64)> {
        let (ziel, hz) = (self.ziel.as_ref()?, live_hz?);
        let (d_hz_soll_minus_ist, _) = berechne_einweisung(ziel.hz_soll, ziel.s_soll, hz, ziel.s_soll);
        Some((-d_hz_soll_minus_ist, querablage_am_ziel(ziel.s_soll, -d_hz_soll_minus_ist)))
    }
}

impl DxfViewer {
    /// Schaltet den Abstecken-Modus ein bzw. beendet ihn (mit Abschluss der offenen Messung).
    pub(super) fn absteckung_umschalten(&mut self, projekt: &Projekt) {
        match self.absteckung.as_mut() {
            Some(a) => a.abschliessen(projekt, Danach::Schliessen),
            None => {
                self.digital_modus = false;
                self.absteckung = Some(Box::new(Absteckung::neu(projekt, &self.letzte_zielhoehe, &self.letzter_code)));
                self.status = "Abstecken: Zielpunkt im Plan antippen.".into();
            }
        }
        self.pruefe_absteckung_fertig();
    }

    fn pruefe_absteckung_fertig(&mut self) {
        if self.absteckung.as_ref().is_some_and(|a| a.fertig) {
            let anzahl = self.absteckung.as_ref().map_or(0, |a| a.abgesteckte().len());
            self.absteckung = None;
            self.status = format!("Absteckung beendet – {anzahl} Punkt(e) abgesteckt{}.", if anzahl > 0 { ", Protokoll geschrieben" } else { "" });
        }
    }

    /// Ein im Plan gewählter Punkt wird Absteckziel; der Plan rückt ihn in die linke Hälfte.
    pub(super) fn setze_absteckziel(&mut self, projekt: &Projekt, r: f64, h: f64, nr: Option<String>, hoehe_plan: f64) {
        let Some(st) = station(projekt, self.lokales_system) else {
            self.dialog = Some(Dialog::Hinweis { titel: "Keine Stationierung".into(), text: "Abstecken ist erst nach einer gültigen Stationierung möglich.".into() });
            return;
        };
        let nr = nr.unwrap_or_else(|| format!("R{}H{}", f(r, 2), f(h, 2)));
        let hoehe_projekt = projekt.finde_punkt_mit_standpunkten(&nr).map(|(_, _, z)| z).filter(|z| z.is_finite() && *z != 0.0);
        let hoehe = hoehe_projekt.or_else(|| (hoehe_plan.is_finite() && hoehe_plan != 0.0).then_some(hoehe_plan));
        let info = StandpunktInfo { punkt_nr: projekt.station.standpunkt_nr.clone(), r: st.r, h: st.h, orientierung_gon: st.orientierung_gon };
        let (hz_soll, s_soll) = polar_zu(&info, r, h);
        let ziel = Ziel { nr, r, h, hoehe, hz_soll, s_soll };
        // Plan: Ziel in die Mitte der linken Hälfte
        self.mitte = [r + f64::from(self.canvas_groesse.x) / 4.0 / self.scale, h];
        if let Some(a) = self.absteckung.as_mut() {
            a.abschliessen(projekt, Danach::Ziel(ziel));
        }
    }

    /// Löst die Absteckmessung aus (Knopf oder Messtaste).
    pub(super) fn absteck_messen(&mut self, projekt: &Projekt, geraet: &mut Geraet) {
        let Some(a) = self.absteckung.as_mut() else {
            return;
        };
        if a.ziel.is_none() || a.wartet || a.frage.is_some() {
            return;
        }
        if !projekt.hat_aktive_aufstellung() {
            a.melde("Kein Standpunkt – bitte zuerst „Neuer Stp“.", Stufe::Fehler);
            return;
        }
        let zh = parse_zahl(&a.zielhoehe).unwrap_or(0.0);
        match geraet.starte_messung(zh) {
            Ok(()) => {
                a.wartet = true;
                a.melde("Messung läuft …", Stufe::Info);
                self.letzte_zielhoehe.clone_from(&a.zielhoehe);
                self.letzter_code.clone_from(&a.code);
            }
            Err(e) => {
                a.melde(e, Stufe::Fehler);
                crate::grundlagen::ton::fehler();
            }
        }
    }

    /// Zielpunkt und abgesteckte Punkte im Plan markieren.
    pub(super) fn zeichne_absteckziele(&self, maler: &egui::Painter, a: &crate::dxf::entity::Ansicht) {
        let Some(abst) = self.absteckung.as_ref() else {
            return;
        };
        for p in abst.abgesteckte() {
            let q = a.zu_bildschirm(p.r_soll, p.h_soll);
            maler.circle_stroke(q, 8.0, Stroke::new(2.0, PLAN_ABGESTECKT));
            maler.text(Pos2::new(q.x + 9.0, q.y - 9.0), Align2::LEFT_BOTTOM, "✓", egui::FontId::proportional(16.0), PLAN_ABGESTECKT);
        }
        if let Some(z) = &abst.ziel {
            let q = a.zu_bildschirm(z.r, z.h);
            let stift = Stroke::new(2.5, PLAN_ZIEL);
            maler.circle_stroke(q, 11.0, stift);
            maler.line_segment([Pos2::new(q.x - 16.0, q.y), Pos2::new(q.x + 16.0, q.y)], stift);
            maler.line_segment([Pos2::new(q.x, q.y - 16.0), Pos2::new(q.x, q.y + 16.0)], stift);
        }
    }

    /// Absteckfenster als eigenes Fenster auf der rechten Hälfte des Viewers (folgt dessen Lage und Größe).
    pub(super) fn zeige_absteckfenster(&mut self, ctx: &egui::Context, projekt: &mut Projekt, geraet: &mut Geraet) {
        let Some(mut a) = self.absteckung.take() else {
            return;
        };
        a.verarbeite_eingang(projekt, self.lokales_system);
        if a.anfahren {
            if let Some(e) = geraet.nimm_anfahr_ergebnis() {
                a.anfahren = false;
                a.melde(e.text, if e.ok { Stufe::Info } else { Stufe::Fehler });
            }
        }
        if a.ziel.is_none() {
            self.absteckung = Some(a);
            return;
        }
        let (aussen, innen) = ctx.input(|i| (i.viewport().outer_rect, i.viewport().inner_rect));
        let mut builder = egui::ViewportBuilder::default()
            .with_title(format!("Absteckung – {}", a.ziel.as_ref().map_or("", |z| z.nr.as_str())))
            .with_min_inner_size(FENSTER_MIN)
            .with_window_level(egui::WindowLevel::AlwaysOnTop);
        builder = match (aussen, innen) {
            (Some(o), Some(i)) => {
                let (rand_x, rand_y) = ((o.width() - i.width()).max(0.0), (o.height() - i.height()).max(0.0));
                let halb = o.width() / 2.0;
                builder.with_position(Pos2::new(o.left() + halb, o.top())).with_inner_size(Vec2::new((halb - rand_x).max(FENSTER_MIN[0]), (o.height() - rand_y).max(FENSTER_MIN[1])))
            }
            _ => builder.with_inner_size(FENSTER_STANDARD),
        };
        let mut messen = false;
        let mut schliessen = false;
        let lokal = self.lokales_system;
        ctx.show_viewport_immediate(egui::ViewportId::from_hash_of("dxf_absteckung"), builder, |ui, _| {
            messen |= Self::nimm_mess_taste(ui.ctx());
            egui::CentralPanel::default().frame(egui::Frame::new().fill(FENSTER_FARBE).inner_margin(egui::Margin::same(10))).show(ui, |ui| {
                let (m, s) = zeichne_inhalt(ui, &mut a, projekt, geraet, lokal);
                messen |= m;
                schliessen |= s;
            });
            if ui.ctx().input(|i| i.viewport().close_requested()) {
                schliessen = true;
            }
            ui.ctx().request_repaint_after(REPAINT_INTERVALL);
        });
        if schliessen && a.frage.is_none() {
            a.abschliessen(projekt, Danach::Schliessen);
        }
        self.absteckung = Some(a);
        if messen {
            self.absteck_messen(projekt, geraet);
        }
        self.pruefe_absteckung_fertig();
    }
}

/// Inhalt des Absteckfensters; liefert (Messen gedrückt, Schließen gedrückt).
fn zeichne_inhalt(ui: &mut egui::Ui, a: &mut Absteckung, projekt: &mut Projekt, geraet: &mut Geraet, lokal: bool) -> (bool, bool) {
    let Some(ziel) = a.ziel.clone() else {
        return (false, false);
    };
    let p = projekt.rechenparameter.params.clone();
    let (tol_l, tol_h) = (p.absteck_toleranz_lage_mm, p.absteck_toleranz_hoehe_mm);
    let fernrohr = a.fernrohr(geraet.live_hz);
    let (mut messen, mut schliessen) = (false, false);

    // Kopfzeile: Sollwerte
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(format!("Ziel {}", ziel.nr)).strong().size(18.0).color(FARBE_ZIEL));
        ui.add_space(10.0);
        ui.label(RichText::new(format!("Hz soll {} gon   s {} m", f(ziel.hz_soll, 4), f(ziel.s_soll, 3))).monospace().size(15.0));
        if let Some(hz) = ziel.hoehe {
            ui.label(RichText::new(format!("   H soll {} m", f(hz, 3))).monospace().size(15.0));
        }
    });
    ui.add_space(4.0);

    // Animation
    let hoehe = (ui.available_height() - UNTEN_HOEHE).max(220.0);
    let (_antwort, maler) = ui.allocate_painter(Vec2::new(ui.available_width(), hoehe), Sense::hover());
    zeichne_animation(&maler, a, &ziel, fernrohr, tol_l, &projekt.station.standpunkt_nr);
    ui.add_space(6.0);

    // Fernrohr live
    match fernrohr {
        Some((_, q)) if q.abs() * 1000.0 <= tol_l => {
            ui.label(RichText::new("Fernrohr steht auf dem Ziel ✓").size(18.0).strong().color(FARBE_OK));
        }
        Some((d_hz, q)) => {
            let seite = if d_hz > 0.0 { "nach LINKS" } else { "nach RECHTS" };
            ui.label(RichText::new(format!("Drehen: {} gon {seite}  (am Ziel {} m)", f(d_hz.abs(), 4), f(q.abs(), 3))).size(18.0).color(ampel(q.abs() * 1000.0, tol_l)));
        }
        None => {
            ui.label(RichText::new("Keine Fernrohrrichtung vom Gerät").size(16.0).color(FARBE_WARNUNG));
        }
    }

    // Korrekturen der letzten Messung
    if let Some(m) = &a.messung {
        let zeile = |ui: &mut egui::Ui, titel: &str, text: String, betrag_mm: f64, toleranz: f64| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("{titel:<6}")).monospace().size(KORREKTUR_SCHRIFT));
                ui.label(RichText::new(text).strong().size(KORREKTUR_SCHRIFT).color(ampel(betrag_mm, toleranz)));
            });
        };
        zeile(ui, "Längs", korrektur_text(m.abw.laengs_m, "ZURÜCK (zum Gerät)", "VOR (vom Gerät weg)"), m.abw.laengs_m.abs() * 1000.0, tol_l);
        zeile(ui, "Quer", korrektur_text(m.abw.quer_m, "nach LINKS", "nach RECHTS"), m.abw.quer_m.abs() * 1000.0, tol_l);
        match m.abw.hoehe_m {
            Some(h) => zeile(ui, "Höhe", format!("{} m  {}", f(h.abs(), 3), if h > 0.0 { "zu hoch (Abtrag)" } else { "zu tief (Auftrag)" }), h.abs() * 1000.0, tol_h),
            None => {
                ui.label(RichText::new("Höhe   –").monospace().size(16.0).color(FARBE_SOLL));
            }
        }
        let ist_h = m.ist.2.map_or_else(|| "–".to_string(), |h| f(h, 3));
        ui.label(
            RichText::new(format!("Ist: R {}  H {}  Höhe {ist_h}   ({}){}", f(m.ist.0, 3), f(m.ist.1, 3), m.roh.zeitstempel.format("%H:%M:%S"), if m.uebernommen { "  ✓ abgesteckt" } else { "" }))
                .monospace()
                .color(FARBE_INFO_TEXT),
        );
    }

    // Eingaben und Knöpfe
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.label("ZH [m]");
        ui.add(egui::TextEdit::singleline(&mut a.zielhoehe).desired_width(70.0));
        ui.add_space(10.0);
        ui.label("Code");
        bildschirmtastatur::text_feld(&ui.add(egui::TextEdit::singleline(&mut a.code).desired_width(90.0)));
        ui.add_space(10.0);
        ui.label(RichText::new(format!("Toleranz Lage {} mm / Höhe {} mm", f(tol_l, 0), f(tol_h, 0))).color(FARBE_SOLL)).on_hover_text("Einstellbar in den Rechenparametern");
    });
    ui.add_space(6.0);
    let gesperrt = a.frage.is_some();
    ui.add_enabled_ui(!gesperrt, |ui| {
        ui.horizontal(|ui| {
            let knopf = |text: &str, farbe: Color32| egui::Button::new(RichText::new(text).strong().size(15.0).color(Color32::WHITE)).fill(farbe).min_size(Vec2::new(120.0, KNOPF_HOEHE));
            let messen_text = if a.wartet { "Messung läuft …" } else { "► Messen (€)" };
            if ui.add_enabled(geraet.ist_verbunden() && !a.wartet && !geraet.beschaeftigt(), knopf(messen_text, Color32::from_rgb(160, 60, 20))).clicked() {
                messen = true;
            }
            if geraet.protokoll() == GeraeteProtokoll::GeoCom {
                let text = if a.anfahren { "Fährt …" } else { "Anfahren" };
                if ui.add_enabled(geraet.ist_verbunden() && !a.anfahren && !geraet.beschaeftigt(), knopf(text, Color32::from_rgb(30, 100, 160))).clicked() {
                    let v = anfahr_v(&ziel, projekt, &a.zielhoehe, geraet.live_v, lokal);
                    match geraet.starte_anfahren(ziel.hz_soll, v) {
                        Ok(()) => a.anfahren = true,
                        Err(e) => a.melde(e, Stufe::Fehler),
                    }
                }
            }
            let abgesteckt_frei = a.messung.as_ref().is_some_and(|m| !m.uebernommen);
            if ui.add_enabled(abgesteckt_frei, knopf("✓ Abgesteckt", FARBE_OK)).clicked() {
                a.abschliessen(projekt, Danach::Nichts);
            }
            if ui.add(knopf("Schließen", Color32::from_rgb(90, 95, 110))).clicked() {
                schliessen = true;
            }
        });
    });
    if let Some(text) = geraet.anfahren_status().filter(|_| a.anfahren) {
        ui.label(RichText::new(text).color(FARBE_SOLL));
    }

    // Meldung
    ui.add_space(4.0);
    let (text, stufe) = &a.meldung;
    if !text.is_empty() {
        let r = match stufe {
            Stufe::Fehler => RichText::new(format!("✖ {text}")).strong().color(FARBE_FEHLER),
            Stufe::Warnung => RichText::new(format!("⚠ {text}")).color(FARBE_WARNUNG),
            Stufe::Info => RichText::new(text).color(Color32::BLACK),
        };
        ui.add(egui::Label::new(r.size(15.0)).wrap());
    }

    // Rückfrage bei Toleranzüberschreitung
    if let Some(danach) = a.frage.clone() {
        let werte = a.messung.as_ref().map(|m| {
            let h = m.abw.hoehe_m.map_or_else(|| "–".to_string(), |h| format!("{:+.0}", h * 1000.0));
            format!("längs {:+.0} mm, quer {:+.0} mm, Höhe {h} mm", m.abw.laengs_m * 1000.0, m.abw.quer_m * 1000.0)
        });
        let mut antwort: Option<u8> = None;
        egui::Window::new("Toleranz überschritten").collapsible(false).resizable(false).anchor(Align2::CENTER_CENTER, Vec2::ZERO).show(ui.ctx(), |ui| {
            ui.label(RichText::new(format!("Punkt {}: {}", ziel.nr, werte.unwrap_or_default())).strong().color(FARBE_FEHLER));
            ui.label(format!("Toleranz: Lage {} mm, Höhe {} mm.", f(tol_l, 0), f(tol_h, 0)));
            ui.add_space(4.0);
            ui.label("Soll der Punkt trotzdem als abgesteckt übernommen werden?");
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.add(egui::Button::new("Trotzdem übernehmen").min_size(Vec2::new(150.0, KNOPF_HOEHE))).clicked() {
                    antwort = Some(0);
                }
                if !matches!(danach, Danach::Nichts) && ui.add(egui::Button::new("Nicht übernehmen").min_size(Vec2::new(140.0, KNOPF_HOEHE))).clicked() {
                    antwort = Some(1);
                }
                if ui.add(egui::Button::new("Zurück (nachmessen)").min_size(Vec2::new(150.0, KNOPF_HOEHE))).clicked() {
                    antwort = Some(2);
                }
            });
        });
        match antwort {
            Some(0) => {
                a.frage = None;
                a.uebernehmen(projekt, true);
                a.fuehre_aus(projekt, danach);
            }
            Some(1) => {
                a.frage = None;
                if let Some(m) = a.messung.as_ref() {
                    let e = Eintrag::neu(Ereignis::Bemerkung).mit_aufstellung(&projekt.station).punkt(&ziel.nr, messdaten::TYP_ABSTECKUNG, "").bezug(m.lfd).quelle(mp::QUELLE_HAND).text("Absteckmessung außerhalb der Toleranz nicht übernommen");
                    mp::protokolliere(&projekt.einstellungen, e);
                }
                a.messung = None;
                a.fuehre_aus(projekt, danach);
            }
            Some(_) => a.frage = None,
            None => {}
        }
    }
    (messen, schliessen)
}

/// Farbe der Ist-Zeile.
const FARBE_INFO_TEXT: Color32 = Color32::from_rgb(60, 60, 70);

/// Zenitwinkel zum Anfahren: bei 3D aus Soll- und Stationshöhe (Prisma in Zielhöhe), sonst aktuelles V bzw. 100 gon.
fn anfahr_v(ziel: &Ziel, projekt: &Projekt, zielhoehe: &str, live_v: Option<f64>, lokal: bool) -> f64 {
    let st = station(projekt, lokal);
    match (ziel.hoehe, st.filter(|s| s.berechnung_3d && s.hoehe != 0.0)) {
        (Some(h_soll), Some(s)) if ziel.s_soll > 0.0 => {
            let zh = parse_zahl(zielhoehe).unwrap_or(0.0);
            let dh = (h_soll + zh) - (s.hoehe + projekt.station.instrumenten_hoehe);
            ziel.s_soll.atan2(dh) * 200.0 / std::f64::consts::PI
        }
        _ => live_v.unwrap_or(100.0),
    }
}

/// Zeichnet die Animation: Standpunkt unten, Ziel oben, Sollrichtung, Fernrohrrichtung, Zoomkreis mit Ist-Punkt.
fn zeichne_animation(maler: &egui::Painter, a: &Absteckung, ziel: &Ziel, fernrohr: Option<(f64, f64)>, tol_mm: f64, standpunkt_nr: &str) {
    let rect = maler.clip_rect();
    maler.rect_filled(rect, 6.0, FARBE_ANIMATION);
    maler.rect_stroke(rect, 6.0, Stroke::new(1.0, FARBE_SOLL), egui::StrokeKind::Inside);
    let cx = rect.center().x;
    let y_st = rect.bottom() - RAND_UNTEN;
    let y_ziel = rect.top() + RAND_OBEN + ZOOM_RADIUS;
    let st = Pos2::new(cx, y_st);
    let zp = Pos2::new(cx, y_ziel);
    let max_px = rect.width() / 2.0 - 18.0;
    let tol_m = tol_mm / 1000.0;

    // Zoomstufe aus Messabweichung und Fernrohrablage
    let versatz = a.messung.as_ref().map_or(0.0, |m| m.abw.laengs_m.abs().max(m.abw.quer_m.abs()));
    let ablage = fernrohr.map_or(0.0, |(_, q)| q.abs());
    let stufe = zoom_stufe(versatz.max(ablage), tol_m);

    // Sollrichtung (gestrichelt) und Standpunkt
    maler.extend(egui::Shape::dashed_line(&[st, Pos2::new(cx, y_ziel + ZOOM_RADIUS)], Stroke::new(1.5, FARBE_SOLL), 8.0, 6.0));
    let dreieck = vec![Pos2::new(cx, y_st - 14.0), Pos2::new(cx - 12.0, y_st + 8.0), Pos2::new(cx + 12.0, y_st + 8.0)];
    maler.add(egui::Shape::convex_polygon(dreieck, FARBE_STANDPUNKT, Stroke::new(1.0, Color32::BLACK)));
    maler.text(Pos2::new(cx + 18.0, y_st), Align2::LEFT_CENTER, format!("{standpunkt_nr} (Tachymeter)"), egui::FontId::proportional(14.0), Color32::BLACK);
    maler.text(Pos2::new(cx - 18.0, (y_st + y_ziel) / 2.0), Align2::RIGHT_CENTER, format!("{} m", f(ziel.s_soll, 3)), egui::FontId::proportional(13.0), FARBE_SOLL);

    // Zoomkreis um das Ziel mit Toleranzkreis
    let tol_px = (tol_m / stufe) as f32 * ZOOM_RADIUS;
    maler.circle_filled(zp, tol_px.min(ZOOM_RADIUS), FARBE_TOLERANZ);
    maler.circle_stroke(zp, ZOOM_RADIUS, Stroke::new(1.0, FARBE_SOLL));
    maler.circle_stroke(zp, ZOOM_RADIUS / 2.0, Stroke::new(0.5, FARBE_SOLL));
    maler.text(Pos2::new(zp.x + ZOOM_RADIUS * 0.72, zp.y - ZOOM_RADIUS * 0.72), Align2::LEFT_BOTTOM, format!("Ring {} m", f(stufe, if stufe < 0.1 { 2 } else { 1 })), egui::FontId::proportional(12.0), FARBE_SOLL);
    let kreuz = Stroke::new(2.0, FARBE_ZIEL);
    maler.line_segment([Pos2::new(zp.x - 10.0, zp.y), Pos2::new(zp.x + 10.0, zp.y)], kreuz);
    maler.line_segment([Pos2::new(zp.x, zp.y - 10.0), Pos2::new(zp.x, zp.y + 10.0)], kreuz);
    maler.circle_stroke(zp, 5.0, kreuz);
    maler.text(Pos2::new(zp.x - 12.0, zp.y - 12.0), Align2::RIGHT_BOTTOM, &ziel.nr, egui::FontId::proportional(15.0), FARBE_ZIEL);

    // Fernrohrrichtung (live): Strahl vom Standpunkt, am Ziel um die (im Kreis maßstäbliche) Querablage versetzt
    if let Some((_, q)) = fernrohr {
        let x = cx + versatz_px(q, stufe, max_px);
        let ende = Pos2::new(x, y_ziel);
        let richtung = (ende - st).normalized();
        let weiter = ende + richtung * (ZOOM_RADIUS + 10.0);
        maler.line_segment([st, weiter], Stroke::new(2.5, FARBE_FERNROHR));
        maler.text(weiter + Vec2::new(4.0, 0.0), Align2::LEFT_CENTER, "Fernrohr", egui::FontId::proportional(13.0), FARBE_FERNROHR);
    }

    // Ist-Punkt der letzten Messung und Korrekturpfeil (Bewegung zum Ziel)
    if let Some(m) = &a.messung {
        let farbe = ampel(m.abw.laengs_m.abs().max(m.abw.quer_m.abs()) * 1000.0, tol_mm);
        let ist = Pos2::new(cx + versatz_px(m.abw.quer_m, stufe, max_px), y_ziel - versatz_px(m.abw.laengs_m, stufe, max_px));
        let stift = Stroke::new(3.0, farbe);
        maler.line_segment([ist + Vec2::new(-8.0, -8.0), ist + Vec2::new(8.0, 8.0)], stift);
        maler.line_segment([ist + Vec2::new(-8.0, 8.0), ist + Vec2::new(8.0, -8.0)], stift);
        if (zp - ist).length() > 12.0 {
            maler.arrow(ist, (zp - ist) * 0.85, Stroke::new(2.5, farbe));
        }
        maler.text(ist + Vec2::new(10.0, 10.0), Align2::LEFT_TOP, "Ist", egui::FontId::proportional(13.0), farbe);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zoomstufe_passt_zum_versatz() {
        assert_eq!(zoom_stufe(0.003, 0.01), 0.02);
        assert_eq!(zoom_stufe(0.08, 0.01), 0.1);
        assert_eq!(zoom_stufe(3.2, 0.01), 5.0);
        assert_eq!(zoom_stufe(1e6, 0.01), 1000.0);
    }

    #[test]
    fn versatz_linear_im_kreis_gestaucht_ausserhalb() {
        assert!((versatz_px(0.05, 0.1, 300.0) - ZOOM_RADIUS / 2.0).abs() < 1e-3);
        assert!((versatz_px(-0.1, 0.1, 300.0) + ZOOM_RADIUS).abs() < 1e-3);
        let weit = versatz_px(10.0, 0.1, 300.0);
        assert!(weit > ZOOM_RADIUS && weit < 300.0);
    }

    #[test]
    fn ampel_nach_toleranz() {
        assert_eq!(ampel(8.0, 10.0), FARBE_OK);
        assert_eq!(ampel(15.0, 10.0), FARBE_WARNUNG);
        assert_eq!(ampel(25.0, 10.0), FARBE_FEHLER);
    }

    #[test]
    fn korrekturtexte_aus_sicht_des_instruments() {
        assert_eq!(korrektur_text(0.35, "ZURÜCK", "VOR"), "0.350 m  ZURÜCK");
        assert_eq!(korrektur_text(-0.012, "nach LINKS", "nach RECHTS"), "0.012 m  nach RECHTS");
    }
}
