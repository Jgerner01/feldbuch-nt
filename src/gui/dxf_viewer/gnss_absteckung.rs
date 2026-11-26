// Zweck:    GNSS-Absteckung im DXF-Viewer: Zielpunkt im Plan wählen, Antenne und Ziel in Echtzeit, Ausrichtung der
//           Grafik nach der Gehrichtung (Bewegungsvektor), Vor/Rechts-Anzeige, Zoomstufen, Ampel nach der
//           Absteckgenauigkeit, Stabilitätsprüfung, Kontrollmessung, „Abgesteckt“ und Absteckprotokoll (GNSS.md Abschnitt 7)
// Hardware: GNSS-Empfänger u-blox ZED-F9P (über das GNSS-Modul)
// Autor:    Johann Gerner

use chrono::Local;
use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, RichText, Sense, Stroke, Vec2};

use super::DxfViewer;
use super::gnss::{FARBE_FIX, FARBE_FLOAT, FARBE_OHNE, MM_PRO_M, Messwert, PlanPos, Soll, TYP_GNSS_ABSTECKUNG, fix_farbe};
use crate::daten::messprotokoll::{self, Eintrag, Ereignis};
use crate::daten::projekt::Projekt;
use crate::gnss::bewegung::{Bewegung, vor_rechts};
use crate::grundlagen::format::f;
use crate::grundlagen::ton;
use crate::protokolle::{absteckung::{AbsteckProtokoll, punktabsteckung}, ausgabeverzeichnis};
use crate::rechnen::absteckung::AbsteckPunkt;

/// Fenstergröße.
const FENSTER_GROESSE: [f32; 2] = [440.0, 640.0];
/// Seitenlänge der Grafik [px].
const GRAFIK_PX: f32 = 380.0;
/// Anteil des Radius, auf dem die Antenne im Fernbereich liegt.
const FERN_ANTEIL: f64 = 0.8;
/// Ab diesem Abstand [m] zeigt die Grafik die Zielscheibe mit festen Ringen.
const NAH_M: f64 = 1.0;
/// Ringe der Zielscheibe [m].
const RINGE_M: [f64; 3] = [1.0, 0.1, 0.01];
/// Nahbereich, in dem die Gehrichtung festgehalten wird [m].
const RICHTUNG_FEST_M: f64 = 1.0;
/// Farben der Grafik.
const HINTERGRUND: Color32 = Color32::from_rgb(24, 28, 36);
const RING: Color32 = Color32::from_rgb(80, 90, 110);
const ZIEL: Color32 = Color32::from_rgb(255, 70, 70);
const NORD: Color32 = Color32::from_rgb(200, 200, 230);

/// Laufende GNSS-Absteckung.
#[derive(Default)]
pub(in crate::gui::dxf_viewer) struct GnssAbsteckung {
    pub ziel: Option<Soll>,
    pub bewegung: Bewegung,
    /// Grafik nach Norden statt nach der Gehrichtung ausrichten
    pub nord_oben: bool,
    /// Epochen in Folge innerhalb der Toleranz
    pub stabil: usize,
    pub letzte_messung: Option<Messwert>,
    pub abgesteckt: Vec<AbsteckPunkt>,
    pub meldung: String,
    /// Toleranz ist erreicht und wurde gemeldet (Ton nur einmal)
    gemeldet: bool,
}

/// Abweichung Ist → Soll und Bewertung.
struct Versatz {
    d_r: f64,
    d_h: f64,
    d_hoehe: Option<f64>,
}

impl Versatz {
    fn aus(soll: &Soll, r: f64, h: f64, hoehe: Option<f64>) -> Self {
        Self { d_r: soll.r - r, d_h: soll.h - h, d_hoehe: soll.hoehe.zip(hoehe).map(|(s, i)| s - i) }
    }

    fn strecke(&self) -> f64 {
        self.d_r.hypot(self.d_h)
    }

    fn in_toleranz(&self, lage_mm: f64, hoehe_mm: f64) -> bool {
        self.strecke() * MM_PRO_M <= lage_mm && self.d_hoehe.is_none_or(|d| d.abs() * MM_PRO_M <= hoehe_mm)
    }
}

impl DxfViewer {
    /// Startet oder beendet die GNSS-Absteckung (beim Beenden wird das Protokoll geschrieben).
    pub(super) fn gnss_absteckung_umschalten(&mut self, projekt: &Projekt) {
        if let Some(a) = self.gnss.absteckung.take() {
            self.schreibe_gnss_absteckprotokoll(projekt, &a);
            return;
        }
        self.gnss.fenster = None;
        self.gnss.absteckung = Some(GnssAbsteckung { meldung: "Zielpunkt im Plan antippen.".into(), ..Default::default() });
        self.status = "GNSS-Absteckung: Zielpunkt im Plan antippen".into();
    }

    pub(super) fn setze_gnss_absteckziel(&mut self, _projekt: &Projekt, soll: Soll) {
        let Some(a) = self.gnss.absteckung.as_mut() else {
            return;
        };
        a.meldung = format!("Ziel {}: R={}  H={}", soll.nr, f(soll.r, 3), f(soll.h, 3));
        a.ziel = Some(soll);
        a.bewegung.zuruecksetzen();
        a.stabil = 0;
        a.gemeldet = false;
        a.letzte_messung = None;
    }

    /// Neue Epoche: Gehrichtung und Stabilität nachführen, Ton bei Erreichen der Toleranz.
    pub(super) fn gnss_absteckung_epoche(&mut self, projekt: &Projekt, pos: PlanPos, sekunden: f64) {
        let p = &projekt.rechenparameter.params;
        let (tol_lage, tol_hoehe, stabil_n, ton_an, min_m) = (p.absteck_toleranz_lage_mm, p.absteck_toleranz_hoehe_mm, p.gnss.stabil_epochen, p.gnss.annaeherungston, p.gnss.ausrichtung_min_m);
        let fix_ok = self.gnss.live.as_ref().is_some_and(|l| l.fix == crate::gnss::kern::model::Fix::Fixed && !l.stale);
        let Some(a) = self.gnss.absteckung.as_mut() else {
            return;
        };
        let Some(ziel) = a.ziel.clone() else {
            return;
        };
        let v = Versatz::aus(&ziel, pos.r, pos.h, pos.hoehe);
        a.bewegung.neue_position(pos.r, pos.h, sekunden, min_m, v.strecke() < RICHTUNG_FEST_M);
        if fix_ok && v.in_toleranz(tol_lage, tol_hoehe) {
            a.stabil += 1;
        } else {
            a.stabil = 0;
            a.gemeldet = false;
        }
        if a.stabil >= stabil_n.max(1) && !a.gemeldet {
            a.gemeldet = true;
            if ton_an {
                ton::bestaetigung();
            }
        }
    }

    /// Ergebnis der Kontrollmessung.
    pub(super) fn gnss_absteckung_messung(&mut self, _projekt: &Projekt, m: Messwert) {
        if let Some(a) = self.gnss.absteckung.as_mut() {
            a.meldung = format!("Kontrollmessung: {} Epochen, sL {} mm", m.epochen, f(m.s_lage_mm(), 0));
            a.letzte_messung = Some(m);
            ton::bestaetigung();
        }
    }

    /// Speichert den abgesteckten Punkt (Messprotokoll und Liste für das Absteckprotokoll).
    fn gnss_abgesteckt(&mut self, projekt: &Projekt) {
        let p = &projekt.rechenparameter.params;
        let Some(a) = self.gnss.absteckung.as_mut() else {
            return;
        };
        let (Some(ziel), Some(m)) = (a.ziel.clone(), a.letzte_messung.take()) else {
            a.meldung = "Erst eine Kontrollmessung durchführen (Messen).".into();
            return;
        };
        let v = Versatz::aus(&ziel, m.pos.r, m.pos.h, m.pos.hoehe);
        let ok = v.in_toleranz(p.absteck_toleranz_lage_mm, p.absteck_toleranz_hoehe_mm);
        let text = format!(
            "GNSS dR={} dH={} mm{} · sL {} mm · n={}{}",
            f(-v.d_r * MM_PRO_M, 0),
            f(-v.d_h * MM_PRO_M, 0),
            v.d_hoehe.map_or(String::new(), |d| format!(" dHöhe={} mm", f(-d * MM_PRO_M, 0))),
            f(m.s_lage_mm(), 0),
            m.epochen,
            if ok { "" } else { " · AUSSERHALB TOLERANZ" }
        );
        let mut punkt = AbsteckPunkt::neu(&ziel.nr, ziel.r, ziel.h);
        punkt.status.clone_from(&text);
        a.abgesteckt.push(punkt);
        a.meldung = format!("{} abgesteckt: {text}", ziel.nr);
        a.ziel = None;
        a.stabil = 0;
        messprotokoll::protokolliere(
            &projekt.einstellungen,
            Eintrag::neu(Ereignis::Messung)
                .punkt(&ziel.nr, TYP_GNSS_ABSTECKUNG, "")
                .koordinaten(m.pos.r, m.pos.h, m.pos.hoehe.unwrap_or(f64::NAN))
                .mit_gnss(&m.werte)
                .text(format!("Absteckung Soll R={} H={}: {text}", f(ziel.r, 3), f(ziel.h, 3))),
        );
        ton::bestaetigung();
    }

    fn schreibe_gnss_absteckprotokoll(&mut self, projekt: &Projekt, a: &GnssAbsteckung) {
        if a.abgesteckt.is_empty() {
            return;
        }
        let p: AbsteckProtokoll = punktabsteckung(None, &a.abgesteckt, &projekt.projektdaten.bearbeiter, Local::now().naive_local());
        self.status = match p.schreibe(&ausgabeverzeichnis(projekt)) {
            Ok((rtf, _)) => format!("Absteckprotokoll gespeichert: {}", rtf.display()),
            Err(e) => e,
        };
    }

    /// Beim Schließen des Viewers: Protokoll der GNSS-Absteckung schreiben.
    pub(super) fn gnss_beim_schliessen(&mut self, projekt: &Projekt) {
        if let Some(a) = self.gnss.absteckung.take() {
            self.schreibe_gnss_absteckprotokoll(projekt, &a);
        }
    }

    /// Absteckfenster.
    pub(super) fn zeige_gnss_absteckung(&mut self, ctx: &egui::Context, projekt: &mut Projekt) {
        let Some(mut a) = self.gnss.absteckung.take() else {
            return;
        };
        let mut offen = true;
        let mut messen = false;
        let mut abgesteckt = false;
        let params = projekt.rechenparameter.params.clone();
        let pos = self.gnss.pos;
        let live = self.gnss.live.clone();
        let misst = self.gnss.misst();
        let fortschritt = self.gnss.fortschritt.as_ref().map(|p| format!("{}/{} Epochen", p.accepted, p.epochs));
        egui::Window::new(format!("GNSS-Absteckung {}", a.ziel.as_ref().map_or("", |z| z.nr.as_str())))
            .open(&mut offen)
            .default_size(FENSTER_GROESSE)
            .default_pos(ctx.content_rect().right_top() + Vec2::new(-FENSTER_GROESSE[0] - 20.0, 90.0))
            .show(ctx, |ui| {
                ui.label(&a.meldung);
                let Some(ziel) = a.ziel.clone() else {
                    return;
                };
                let Some(pos) = pos else {
                    ui.colored_label(FARBE_OHNE, "Keine GNSS-Position");
                    return;
                };
                let v = Versatz::aus(&ziel, pos.r, pos.h, pos.hoehe);
                let richtung = if a.nord_oben { None } else { a.bewegung.richtung() };
                let fix = live.as_ref().map(|l| l.fix);
                let stale = live.as_ref().is_none_or(|l| l.stale);
                let gruen = a.stabil >= params.gnss.stabil_epochen.max(1);
                zeichne_grafik(ui, &a, &ziel, &v, richtung, fix_farbe(fix, stale), gruen, params.absteck_toleranz_lage_mm);
                ui.horizontal(|ui| {
                    ui.checkbox(&mut a.nord_oben, "Nord oben");
                    let zustand = match (a.nord_oben, a.bewegung.richtung(), a.bewegung.eingefroren()) {
                        (true, _, _) => "Grafik nach Norden",
                        (false, None, _) => "Gehrichtung noch unbekannt – einige Meter gehen",
                        (false, Some(_), true) => "Gehrichtung festgehalten 🔒",
                        (false, Some(_), false) => "Gehrichtung oben",
                    };
                    ui.label(zustand);
                });
                ui.separator();
                let gross = |ui: &mut egui::Ui, text: String, farbe: Color32| {
                    ui.label(RichText::new(text).size(20.0).strong().color(farbe));
                };
                let farbe = if gruen { FARBE_FIX } else if v.in_toleranz(params.absteck_toleranz_lage_mm, params.absteck_toleranz_hoehe_mm) { FARBE_FLOAT } else { ui.visuals().text_color() };
                match richtung {
                    Some(w) => {
                        let (vor, rechts) = vor_rechts(w, v.d_r, v.d_h);
                        gross(ui, format!("{}  {} m", if vor >= 0.0 { "VOR    " } else { "ZURÜCK " }, f(vor.abs(), 3)), farbe);
                        gross(ui, format!("{}  {} m", if rechts >= 0.0 { "RECHTS " } else { "LINKS  " }, f(rechts.abs(), 3)), farbe);
                    }
                    None => {
                        gross(ui, format!("{}  {} m", if v.d_h >= 0.0 { "NORD " } else { "SÜD  " }, f(v.d_h.abs(), 3)), farbe);
                        gross(ui, format!("{}  {} m", if v.d_r >= 0.0 { "OST  " } else { "WEST " }, f(v.d_r.abs(), 3)), farbe);
                    }
                }
                ui.label(format!("Abstand {} m   ·   Nord {}  Ost {}", f(v.strecke(), 3), f(v.d_h, 3), f(v.d_r, 3)));
                if let Some(dh) = v.d_hoehe {
                    ui.label(format!("Höhe {} m {}", f(dh, 3), if dh >= 0.0 { "Auftrag" } else { "Abtrag" }));
                }
                if let Some(l) = live.as_ref() {
                    let sl = l.sigma_e.zip(l.sigma_n).map(|(e, n)| e.hypot(n) * MM_PRO_M);
                    ui.colored_label(fix_farbe(Some(l.fix), l.stale), format!("{} · σL {} mm", l.fix.label(), sl.map_or("–".into(), |s| f(s, 0))));
                    if sl.is_some_and(|s| s > params.absteck_toleranz_lage_mm / 2.0) {
                        ui.colored_label(FARBE_OHNE, "⚠ Absteckgenauigkeit mit dieser Lösung nicht sicher erreichbar");
                    }
                }
                if let Some(m) = &a.letzte_messung {
                    let vm = Versatz::aus(&ziel, m.pos.r, m.pos.h, m.pos.hoehe);
                    let ok = vm.in_toleranz(params.absteck_toleranz_lage_mm, params.absteck_toleranz_hoehe_mm);
                    ui.colored_label(
                        if ok { FARBE_FIX } else { FARBE_OHNE },
                        format!("Messung: Abweichung {} mm{}", f(vm.strecke() * MM_PRO_M, 0), vm.d_hoehe.map_or(String::new(), |d| format!(", Höhe {} mm", f(d * MM_PRO_M, 0)))),
                    );
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    let text = match (&fortschritt, misst) {
                        (Some(p), true) => format!("■ {p}"),
                        (None, true) => "■ startet …".into(),
                        _ => "► Messen".into(),
                    };
                    if ui.add(egui::Button::new(RichText::new(text).strong()).min_size(Vec2::new(140.0, 32.0))).clicked() {
                        messen = true;
                    }
                    if ui.add_enabled(a.letzte_messung.is_some(), egui::Button::new(RichText::new("Abgesteckt").strong()).min_size(Vec2::new(120.0, 32.0))).clicked() {
                        abgesteckt = true;
                    }
                });
                ui.label(format!("Abgesteckt: {} Punkte", a.abgesteckt.len()));
            });
        self.gnss.absteckung = Some(a);
        if messen {
            self.gnss_messen_geklickt(projekt);
        }
        if abgesteckt {
            self.gnss_abgesteckt(projekt);
        }
        if !offen && let Some(a) = self.gnss.absteckung.take() {
            self.schreibe_gnss_absteckprotokoll(projekt, &a);
        }
    }
}

/// Zeichnet Ziel, Antenne, Spur, Ringe und Nordpfeil; die Grafik ist um die Gehrichtung gedreht.
#[allow(clippy::too_many_arguments)]
fn zeichne_grafik(ui: &mut egui::Ui, a: &GnssAbsteckung, ziel: &Soll, v: &Versatz, richtung: Option<f64>, farbe: Color32, gruen: bool, tol_mm: f64) {
    let (antwort, maler) = ui.allocate_painter(Vec2::splat(GRAFIK_PX), Sense::hover());
    let rect: Rect = antwort.rect;
    maler.rect_filled(rect, 8.0, HINTERGRUND);
    let mitte = rect.center();
    let radius = f64::from(GRAFIK_PX) / 2.0 - 16.0;
    let d = v.strecke();
    // Maßstab: im Fernbereich so, dass die Antenne auf dem äußeren Kreis liegt; im Nahbereich feste Ringe
    let px_pro_m = if d > NAH_M {
        FERN_ANTEIL * radius / d
    } else {
        let ring = RINGE_M.iter().copied().filter(|r| *r >= d.max(tol_mm / MM_PRO_M)).fold(NAH_M, f64::min);
        FERN_ANTEIL * radius / ring
    };
    let w = richtung.unwrap_or(0.0);
    let (s, c) = w.sin_cos();
    // Plan (dR Ost, dH Nord) → Bildschirm, gedreht, damit die Gehrichtung nach oben zeigt
    let bild = |de: f64, dn: f64| -> Pos2 {
        let x = de * c - dn * s;
        let y = de * s + dn * c;
        mitte + Vec2::new((x * px_pro_m) as f32, -(y * px_pro_m) as f32)
    };
    for ring in RINGE_M {
        let r_px = (ring * px_pro_m) as f32;
        if r_px > 4.0 && r_px < (radius * 1.2) as f32 {
            maler.circle_stroke(mitte, r_px, Stroke::new(1.0, RING));
            maler.text(mitte + Vec2::new(r_px * 0.7, -r_px * 0.7), Align2::LEFT_BOTTOM, format!("{} m", f(ring, 2)), FontId::proportional(10.0), RING);
        }
    }
    let tol_px = (tol_mm / MM_PRO_M * px_pro_m) as f32;
    if tol_px > 2.0 {
        maler.circle_filled(mitte, tol_px, Color32::from_rgba_unmultiplied(40, 200, 70, if gruen { 120 } else { 40 }));
    }
    // Ziel im Zentrum
    maler.circle_stroke(mitte, 7.0, Stroke::new(2.0, ZIEL));
    maler.line_segment([mitte - Vec2::new(12.0, 0.0), mitte + Vec2::new(12.0, 0.0)], Stroke::new(1.5, ZIEL));
    maler.line_segment([mitte - Vec2::new(0.0, 12.0), mitte + Vec2::new(0.0, 12.0)], Stroke::new(1.5, ZIEL));
    maler.text(mitte + Vec2::new(10.0, 10.0), Align2::LEFT_TOP, &ziel.nr, FontId::proportional(12.0), ZIEL);
    // Antenne relativ zum Ziel
    let antenne = bild(-v.d_r, -v.d_h);
    let innen = rect.shrink(6.0);
    let antenne_sichtbar = innen.contains(antenne);
    let antenne_bild = if antenne_sichtbar { antenne } else { mitte + (antenne - mitte).normalized() * (radius as f32) };
    maler.line_segment([antenne_bild, mitte], Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 80)));
    maler.circle_stroke(antenne_bild, 10.0, Stroke::new(3.0, farbe));
    maler.circle_filled(antenne_bild, 3.0, farbe);
    // Gehrichtung als Pfeil an der Antenne (zeigt nach oben, wenn ausgerichtet)
    if let Some(r) = a.bewegung.richtung() {
        let rel = r - w;
        let pfeil = Vec2::new(rel.sin() as f32, -(rel.cos() as f32)) * 30.0;
        maler.arrow(antenne_bild, pfeil, Stroke::new(2.5, farbe));
    }
    // Nordpfeil
    let nord = Vec2::new(-(s as f32), -(c as f32)) * 22.0;
    let nord_fuss = rect.right_top() + Vec2::new(-30.0, 34.0);
    maler.arrow(nord_fuss - nord * 0.5, nord, Stroke::new(2.0, NORD));
    maler.text(nord_fuss + nord * 0.6, Align2::CENTER_CENTER, "N", FontId::proportional(12.0), NORD);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versatz_und_toleranz() {
        let soll = Soll { nr: "1".into(), r: 10.0, h: 20.0, hoehe: Some(100.0) };
        let v = Versatz::aus(&soll, 10.003, 19.996, Some(100.004));
        assert!((v.strecke() - 0.005).abs() < 1e-9);
        assert!(v.in_toleranz(10.0, 10.0));
        assert!(!v.in_toleranz(4.0, 10.0));
        assert!(!v.in_toleranz(10.0, 3.0));
    }
}
