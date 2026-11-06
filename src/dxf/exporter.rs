// Zweck:    Schreibt alle Feldbuchpunkte als eigenständige DXF-Datei (AC1009) mit Layern je Kategorie und Quelle;
//           Symbol- und Textgrößen werden auf den Zielmaßstab skaliert
// Hardware: keine
// Autor:    Johann Gerner

use crate::daten::modelle::{FeldbuchPunkt, ImportPunkt, NeupunktErgebnis, TYP_STANDPUNKT};
use crate::grundlagen::format::f;

/// ACI-Farben passend zur Grafik des Viewers.
const ACI_ROT: i32 = 1;
const ACI_GRUEN: i32 = 3;
const ACI_BLAU: i32 = 5;
const ACI_BRAUN: i32 = 30;
/// Millimeter je Meter (Maßstabsumrechnung).
const MM_PRO_M: f64 = 1000.0;
/// Stützpunkte der als Polylinie geschriebenen Ellipse.
const ELLIPSEN_PUNKTE: usize = 36;
/// Faktor für Länge und Innenabstand der Standpunktkreuze.
const KREUZ_ARM: f64 = 1.6;
const KREUZ_INNEN: f64 = 0.6;
/// Textversatz relativ zur Texthöhe und Symbolgröße.
const TEXT_VERSATZ: f64 = 0.3;
/// Zeilenende der DXF-Datei.
const NEUE_ZEILE: &str = "\r\n";
/// Name für unbekannte Quellen und Standpunkte.
const UNBEKANNT: &str = "Unbekannt";

/// Ellipse: Mittelpunkt, Halbachsen [mm], Richtung [rad] und Skalierung.
/// `konfidenz = false`: Residuenellipse (Halbachsen = Längs- und Querresiduum, keine statistische Aussage);
/// `konfidenz = true`: Konfidenzellipse des Standpunkts aus der Kovarianzmatrix (a, b in `v_strecke_mm`, `v_quer_mm`).
#[derive(Clone, Copy, Debug)]
pub struct Ellipse {
    pub r: f64,
    pub h: f64,
    pub v_strecke_mm: f64,
    pub v_quer_mm: f64,
    pub phi_rad: f64,
    pub skalierung: f64,
    pub konfidenz: bool,
}

/// Layer der Residuenellipsen und der Konfidenzellipse.
const LAYER_RESIDUENELLIPSEN: &str = "Residuenellipsen";
const LAYER_KONFIDENZELLIPSE: &str = "Konfidenzellipse";

/// Größenangaben des Exports.
#[derive(Clone, Copy, Debug)]
pub struct Einstellung {
    pub massstab: f64,
    pub symbol_mm: f64,
    pub text_mm: f64,
}

impl Default for Einstellung {
    fn default() -> Self {
        Self { massstab: 1000.0, symbol_mm: 1.5, text_mm: 2.0 }
    }
}

impl Einstellung {
    /// Symbolradius in Weltmetern.
    pub fn symbol_radius(&self) -> f64 {
        self.symbol_mm * self.massstab / (2.0 * MM_PRO_M)
    }

    /// Texthöhe in Weltmetern.
    pub fn text_hoehe(&self) -> f64 {
        self.text_mm * self.massstab / MM_PRO_M
    }
}

/// Bereinigt einen Dateinamen zu einem gültigen Layernamen.
pub fn layer_name(name: &str) -> String {
    if name.trim().is_empty() {
        return UNBEKANNT.to_string();
    }
    let stamm = std::path::Path::new(name).file_stem().map_or_else(|| name.to_string(), |s| s.to_string_lossy().into_owned());
    let ersetzt: String = stamm.chars().map(|c| if c.is_alphanumeric() || c == '_' || c == '-' { c } else { '_' }).collect();
    let bereinigt = ersetzt.trim_matches('_');
    if bereinigt.is_empty() { UNBEKANNT.to_string() } else { bereinigt.to_string() }
}

/// Nur ASCII ausgeben (wie Encoding.ASCII des Originals: alles andere wird zu „?").
fn ascii(text: &str) -> String {
    text.chars().map(|c| if c.is_ascii() { c } else { '?' }).collect()
}

struct Schreiber {
    text: String,
}

impl Schreiber {
    fn gruppe(&mut self, code: i32, wert: &str) {
        self.text.push_str(&format!("{code:>3}{NEUE_ZEILE}{wert}{NEUE_ZEILE}"));
    }

    fn ganz(&mut self, code: i32, wert: i32) {
        self.gruppe(code, &wert.to_string());
    }

    fn zahl(&mut self, code: i32, wert: f64) {
        self.gruppe(code, &f(wert, 6));
    }

    fn kreis(&mut self, cx: f64, cy: f64, r: f64, layer: &str) {
        self.gruppe(0, "CIRCLE");
        self.gruppe(8, layer);
        self.zahl(10, cx);
        self.zahl(20, cy);
        self.zahl(40, r);
    }

    fn linie(&mut self, a: (f64, f64), b: (f64, f64), layer: &str) {
        self.gruppe(0, "LINE");
        self.gruppe(8, layer);
        self.zahl(10, a.0);
        self.zahl(20, a.1);
        self.zahl(11, b.0);
        self.zahl(21, b.1);
    }

    fn text(&mut self, x: f64, y: f64, hoehe: f64, text: &str, layer: &str) {
        self.gruppe(0, "TEXT");
        self.gruppe(8, layer);
        self.zahl(10, x);
        self.zahl(20, y);
        self.zahl(40, hoehe);
        self.gruppe(1, text);
    }

    /// Ellipse als geschlossene Polylinie (AC1009 kennt kein ELLIPSE-Element).
    fn ellipse(&mut self, e: &Ellipse, layer: &str) {
        let a = e.v_strecke_mm.abs() * 0.001 * e.skalierung;
        let b = e.v_quer_mm.abs() * 0.001 * e.skalierung;
        let (cos_phi, sin_phi) = (e.phi_rad.cos(), e.phi_rad.sin());
        self.gruppe(0, "POLYLINE");
        self.gruppe(8, layer);
        self.ganz(66, 1);
        self.ganz(70, 1);
        self.zahl(10, 0.0);
        self.zahl(20, 0.0);
        for i in 0..ELLIPSEN_PUNKTE {
            let t = 2.0 * std::f64::consts::PI * i as f64 / ELLIPSEN_PUNKTE as f64;
            let (lx, ly) = (a * t.cos(), b * t.sin());
            self.gruppe(0, "VERTEX");
            self.gruppe(8, layer);
            self.zahl(10, e.r + lx * cos_phi - ly * sin_phi);
            self.zahl(20, e.h + lx * sin_phi + ly * cos_phi);
        }
        self.gruppe(0, "SEQEND");
        self.gruppe(8, layer);
    }

    /// Symbol, Nummer und (falls vorhanden) Höhe eines Punkts.
    fn punkt(&mut self, e: &Einstellung, (x, y): (f64, f64), nr: &str, hoehe: Option<f64>, layer: (&str, &str, &str)) {
        let (sym_r, text_h) = (e.symbol_radius(), e.text_hoehe());
        let versatz = text_h * TEXT_VERSATZ;
        self.kreis(x, y, sym_r, layer.0);
        self.text(x + sym_r + versatz, y + sym_r * TEXT_VERSATZ + versatz, text_h, nr, layer.1);
        if let Some(h) = hoehe {
            self.text(x + sym_r + versatz, y - sym_r * TEXT_VERSATZ - versatz - text_h, text_h, &f(h, 3), layer.2);
        }
    }
}

fn eindeutig(namen: impl Iterator<Item = String>) -> Vec<String> {
    let mut liste: Vec<String> = Vec::new();
    for n in namen {
        if !liste.iter().any(|v| v.eq_ignore_ascii_case(&n)) {
            liste.push(n);
        }
    }
    liste
}

/// Erzeugt den DXF-Text aller Punktkategorien.
pub fn erzeuge(
    feldbuch: &[FeldbuchPunkt],
    import: &[ImportPunkt],
    gemessen: &[NeupunktErgebnis],
    einstellung: &Einstellung,
    ellipsen: &[Ellipse],
) -> String {
    let mut layer: Vec<(String, i32)> = Vec::new();
    if feldbuch.iter().any(|p| p.typ == TYP_STANDPUNKT) {
        layer.extend([("Feldbuch_Standpunkt_Symbol".into(), ACI_ROT), ("Feldbuch_Standpunkt_Nummer".into(), ACI_ROT), ("Feldbuch_Standpunkt_Hoehe".into(), ACI_BRAUN)]);
    }
    if feldbuch.iter().any(|p| p.typ == "Neupunkt") {
        layer.extend([("Feldbuch_Neupunkt_Symbol".into(), ACI_GRUEN), ("Feldbuch_Neupunkt_Nummer".into(), ACI_GRUEN), ("Feldbuch_Neupunkt_Hoehe".into(), ACI_BRAUN)]);
    }
    let quellen = eindeutig(import.iter().map(|p| p.quelle.clone()));
    for q in &quellen {
        let pfx = format!("Koordinaten_{}", layer_name(q));
        layer.extend([(format!("{pfx}_Symbol"), ACI_ROT), (format!("{pfx}_Nummer"), ACI_GRUEN), (format!("{pfx}_Hoehe"), ACI_BRAUN)]);
    }
    let stationen = eindeutig(gemessen.iter().map(|p| if p.standpunkt_nr.is_empty() { UNBEKANNT.to_string() } else { p.standpunkt_nr.clone() }));
    for sp in &stationen {
        let pfx = format!("Gemessen_{sp}");
        layer.extend([(format!("{pfx}_Symbol"), ACI_BLAU), (format!("{pfx}_Nummer"), ACI_BLAU), (format!("{pfx}_Hoehe"), ACI_BRAUN)]);
    }
    if ellipsen.iter().any(|e| !e.konfidenz) {
        layer.push((LAYER_RESIDUENELLIPSEN.into(), ACI_BLAU));
    }
    if ellipsen.iter().any(|e| e.konfidenz) {
        layer.push((LAYER_KONFIDENZELLIPSE.into(), ACI_ROT));
    }

    let mut s = Schreiber { text: String::with_capacity(16 * 1024) };
    // Kopf
    s.gruppe(0, "SECTION");
    s.gruppe(2, "HEADER");
    s.gruppe(9, "$ACADVER");
    s.gruppe(1, "AC1009");
    s.gruppe(0, "ENDSEC");
    // Tabellen
    s.gruppe(0, "SECTION");
    s.gruppe(2, "TABLES");
    s.gruppe(0, "TABLE");
    s.gruppe(2, "LTYPE");
    s.ganz(70, 1);
    s.gruppe(0, "LTYPE");
    s.gruppe(2, "CONTINUOUS");
    s.ganz(70, 0);
    s.gruppe(3, "Solid line");
    s.ganz(72, 65);
    s.ganz(73, 0);
    s.zahl(40, 0.0);
    s.gruppe(0, "ENDTAB");
    s.gruppe(0, "TABLE");
    s.gruppe(2, "LAYER");
    s.ganz(70, layer.len() as i32 + 1);
    s.gruppe(0, "LAYER");
    s.gruppe(2, "0");
    s.ganz(70, 0);
    s.ganz(62, 7);
    s.gruppe(6, "CONTINUOUS");
    for (name, aci) in &layer {
        s.gruppe(0, "LAYER");
        s.gruppe(2, name);
        s.ganz(70, 0);
        s.ganz(62, *aci);
        s.gruppe(6, "CONTINUOUS");
    }
    s.gruppe(0, "ENDTAB");
    s.gruppe(0, "ENDSEC");
    // Elemente
    s.gruppe(0, "SECTION");
    s.gruppe(2, "ENTITIES");

    let sym_r = einstellung.symbol_radius();
    for p in feldbuch {
        let ist_sp = p.typ == TYP_STANDPUNKT;
        let art = if ist_sp { "Standpunkt" } else { "Neupunkt" };
        let (l_sym, l_nr, l_h) = (format!("Feldbuch_{art}_Symbol"), format!("Feldbuch_{art}_Nummer"), format!("Feldbuch_{art}_Hoehe"));
        s.punkt(einstellung, (p.r, p.h), &p.punkt_nr, p.ist_berechnung_3d.then_some(p.hoehe), (&l_sym, &l_nr, &l_h));
        if ist_sp {
            let (arm, innen) = (sym_r * KREUZ_ARM, sym_r * KREUZ_INNEN);
            s.linie((p.r - arm, p.h - arm), (p.r - innen, p.h - innen), &l_sym);
            s.linie((p.r + innen, p.h + innen), (p.r + arm, p.h + arm), &l_sym);
            s.linie((p.r + arm, p.h - arm), (p.r + innen, p.h - innen), &l_sym);
            s.linie((p.r - innen, p.h + innen), (p.r - arm, p.h + arm), &l_sym);
        }
    }
    for p in import {
        let pfx = format!("Koordinaten_{}", layer_name(&p.quelle));
        s.punkt(einstellung, (p.r, p.h), &p.punkt_nr, (p.hoehe != 0.0).then_some(p.hoehe), (&format!("{pfx}_Symbol"), &format!("{pfx}_Nummer"), &format!("{pfx}_Hoehe")));
    }
    for p in gemessen {
        let sp = if p.standpunkt_nr.is_empty() { UNBEKANNT } else { p.standpunkt_nr.as_str() };
        let pfx = format!("Gemessen_{sp}");
        s.punkt(einstellung, (p.r, p.h), &p.punkt_nr, p.ist_3d.then_some(p.hoehe), (&format!("{pfx}_Symbol"), &format!("{pfx}_Nummer"), &format!("{pfx}_Hoehe")));
    }
    for e in ellipsen {
        s.ellipse(e, if e.konfidenz { LAYER_KONFIDENZELLIPSE } else { LAYER_RESIDUENELLIPSEN });
    }
    s.gruppe(0, "ENDSEC");
    s.gruppe(0, "EOF");
    ascii(&s.text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fb(nr: &str, typ: &str, hoehe: f64, drei_d: bool) -> FeldbuchPunkt {
        FeldbuchPunkt { punkt_nr: nr.into(), typ: typ.into(), r: 100.0, h: 200.0, hoehe, ist_berechnung_3d: drei_d, ..FeldbuchPunkt::default() }
    }

    fn zaehle(text: &str, wert: &str) -> usize {
        text.lines().filter(|z| z.trim() == wert).count()
    }

    #[test]
    fn layernamen_werden_bereinigt() {
        assert_eq!(layer_name("Punkte.kor"), "Punkte");
        assert_eq!(layer_name("Mein Plan (1).csv"), "Mein_Plan__1");
        assert_eq!(layer_name("  "), "Unbekannt");
        assert_eq!(layer_name("___.x"), "Unbekannt");
    }

    #[test]
    fn groessen_nach_massstab() {
        let e = Einstellung { massstab: 500.0, symbol_mm: 2.0, text_mm: 3.0 };
        assert!((e.symbol_radius() - 0.5).abs() < 1e-12);
        assert!((e.text_hoehe() - 1.5).abs() < 1e-12);
    }

    #[test]
    fn export_mit_allen_kategorien() {
        let feldbuch = [fb("S1", "Standpunkt", 0.0, false), fb("N1", "Neupunkt", 12.5, true)];
        let import = [ImportPunkt { punkt_nr: "I1".into(), r: 1.0, h: 2.0, hoehe: 3.0, quelle: "Plan Nord.kor".into(), ..ImportPunkt::default() }];
        let gemessen = [NeupunktErgebnis { punkt_nr: "G1".into(), r: 5.0, h: 6.0, standpunkt_nr: String::new(), ..NeupunktErgebnis::default() }];
        let ellipsen = [Ellipse { r: 0.0, h: 0.0, v_strecke_mm: 5.0, v_quer_mm: 3.0, phi_rad: 0.5, skalierung: 100.0, konfidenz: false }];
        let dxf = erzeuge(&feldbuch, &import, &gemessen, &Einstellung::default(), &ellipsen);
        assert!(dxf.starts_with("  0\r\nSECTION\r\n  2\r\nHEADER\r\n"));
        assert!(dxf.trim_end().ends_with("EOF"));
        assert_eq!(zaehle(&dxf, "AC1009"), 1);
        // Layer: 3 + 3 + 3 + 3 + 1 Ellipsen + Layer "0" = 14
        assert!(dxf.contains(" 70\r\n14\r\n"));
        assert!(dxf.contains("Koordinaten_Plan_Nord_Symbol") && dxf.contains("Gemessen_Unbekannt_Nummer") && dxf.contains("Residuenellipsen"));
        // 4 Kreise (S1, N1, I1, G1), Standpunktkreuz = 4 Linien, Ellipse = 36 Stützpunkte
        assert_eq!(zaehle(&dxf, "CIRCLE"), 4);
        assert_eq!(zaehle(&dxf, "LINE"), 4);
        assert_eq!(zaehle(&dxf, "VERTEX"), ELLIPSEN_PUNKTE);
        assert_eq!(zaehle(&dxf, "POLYLINE"), 1);
        // Höhentexte: N1 (3D) und I1 (Höhe ≠ 0), nicht S1 und G1
        assert_eq!(zaehle(&dxf, "12.500"), 1);
        assert_eq!(zaehle(&dxf, "3.000"), 1);
    }

    #[test]
    fn nur_ascii_und_leere_datenmenge() {
        let feldbuch = [fb("Ä1", "Neupunkt", 0.0, false)];
        let dxf = erzeuge(&feldbuch, &[], &[], &Einstellung::default(), &[]);
        assert!(dxf.is_ascii());
        assert!(dxf.contains("?1"));
        let leer = erzeuge(&[], &[], &[], &Einstellung::default(), &[]);
        assert!(leer.contains("ENTITIES") && zaehle(&leer, "CIRCLE") == 0);
    }
}
