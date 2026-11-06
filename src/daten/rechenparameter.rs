// Zweck:    Rechenparameter (Fehlergrenzen, Maßstab, Gewichtung) in Freie-Stationierung.xml
// Hardware: keine
// Autor:    Johann Gerner

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::grundlagen::format::{f, parse_zahl};
use crate::grundlagen::xml_einfach::{self, XmlDokument};
use crate::grundlagen::datei_speicher;
use crate::rechnen::polar::MassstabModus;

/// Wurzelelement der Rechenparameterdatei.
const WURZEL: &str = "RechenparameterFreieStationierung";
/// Kleinster Sigma-Faktor.
const SIGMA_MIN: i32 = 1;
/// Größter Sigma-Faktor.
const SIGMA_MAX: i32 = 3;

/// Standard-Zentrierfehler am Ziel [mm].
pub const ZENTRIERFEHLER_ZIEL_STANDARD_MM: f64 = 2.0;
/// Standard-Zentrierfehler am Standpunkt [mm].
pub const ZENTRIERFEHLER_STANDPUNKT_STANDARD_MM: f64 = 0.0;

/// Papierbezogene Vorgaben für den geplanten Vektor-PDF-Export des DXF-Viewers.
#[derive(Clone, Debug, PartialEq)]
pub struct PdfParameter {
    /// Nenner des Ausgabemaßstabs 1 : n.
    pub massstab: f64,
    pub querformat: bool,
    /// Unterer Seitenrand; links 20 mm, oben und rechts 10 mm sind fest vorgegeben.
    pub rand_mm: f64,
    pub linie_min_mm: f64,
    /// Mindestdurchmesser bzw. äußere Gesamtgröße eines Punktsymbols.
    pub symbol_min_mm: f64,
}

impl Default for PdfParameter {
    fn default() -> Self {
        Self { massstab: 1000.0, querformat: false, rand_mm: 10.0, linie_min_mm: 0.25, symbol_min_mm: 2.0 }
    }
}

impl PdfParameter {
    /// Ungültige Dateieinträge ersetzen; die geforderten Papier-Mindestmaße gelten immer.
    pub fn normalisiert(&self) -> Self {
        let standard = Self::default();
        let begrenzt = |wert: f64, ersatz: f64, min: f64, max: f64| {
            if wert.is_finite() { wert.clamp(min, max) } else { ersatz }
        };
        Self {
            massstab: begrenzt(self.massstab, standard.massstab, 1.0, 1_000_000.0),
            querformat: self.querformat,
            rand_mm: begrenzt(self.rand_mm, standard.rand_mm, 0.0, 50.0),
            linie_min_mm: begrenzt(self.linie_min_mm, standard.linie_min_mm, 0.25, 10.0),
            symbol_min_mm: begrenzt(self.symbol_min_mm, standard.symbol_min_mm, 2.0, 20.0),
        }
    }
}

/// Vorgaben für den DXF-Export des DXF-Viewers (Maßstab, Symbol- und Textgröße auf dem Papier).
#[derive(Clone, Debug, PartialEq)]
pub struct DxfParameter {
    /// Nenner des Zielmaßstabs 1 : n.
    pub massstab: f64,
    /// Symbolgröße auf dem Papier [mm].
    pub symbol_mm: f64,
    /// Textgröße auf dem Papier [mm].
    pub text_mm: f64,
}

/// Grenzen des Exportmaßstabs.
pub const DXF_MASSSTAB_BEREICH: (f64, f64) = (100.0, 50_000.0);
/// Grenzen der Symbol- und Textgröße [mm].
pub const DXF_GROESSE_BEREICH_MM: (f64, f64) = (0.5, 10.0);

impl Default for DxfParameter {
    fn default() -> Self {
        Self { massstab: 1000.0, symbol_mm: 1.5, text_mm: 2.0 }
    }
}

impl DxfParameter {
    /// Ungültige Werte durch Standardwerte ersetzen, gültige in die Grenzen zwingen.
    pub fn normalisiert(&self) -> Self {
        let standard = Self::default();
        let begrenzt = |wert: f64, ersatz: f64, grenzen: (f64, f64)| {
            if wert.is_finite() { wert.clamp(grenzen.0, grenzen.1) } else { ersatz }
        };
        Self {
            massstab: begrenzt(self.massstab, standard.massstab, DXF_MASSSTAB_BEREICH).round(),
            symbol_mm: begrenzt(self.symbol_mm, standard.symbol_mm, DXF_GROESSE_BEREICH_MM),
            text_mm: begrenzt(self.text_mm, standard.text_mm, DXF_GROESSE_BEREICH_MM),
        }
    }
}

/// Rechenparameter der Stationierung und der Polarpunkte.
#[derive(Clone, Debug, PartialEq)]
pub struct Rechenparameter {
    /// Fehlergrenze für Winkelresiduen [cc].
    pub fehlergrenze_cc_winkel: f64,
    /// Fehlergrenze für Streckenresiduen [mm].
    pub fehlergrenze_mm_strecke: f64,
    /// Fehlergrenze für Höhenresiduen [mm].
    pub fehlergrenze_mm_hoehe: f64,
    /// s0-Schwellwert für die Auto-Match-Freigabe [mm].
    pub fehlergrenze_mm_s0_auto_match: f64,
    /// Freier Maßstab (true) oder Maßstab fest m=1 (false).
    pub freier_massstab: bool,
    /// 3D-Berechnung inkl. Höhen.
    pub berechnung_3d: bool,
    /// Ausgabe der Standardabweichung: 1-, 2- oder 3-fach.
    pub sigma_faktor: i32,
    pub polar_massstab_modus: MassstabModus,
    pub polar_massstab_wert: f64,
    /// Herkunft des vorgegebenen Maßstabs: "manuell" oder "UTM".
    pub polar_massstab_quelle: String,
    pub gewichtung_nahe_ziele: bool,
    pub zentrierfehler_ziel_mm: f64,
    pub zentrierfehler_standpunkt_mm: f64,
    pub pdf: PdfParameter,
    pub dxf: DxfParameter,
}

impl Default for Rechenparameter {
    fn default() -> Self {
        Self {
            fehlergrenze_cc_winkel: 20.0,
            fehlergrenze_mm_strecke: 10.0,
            fehlergrenze_mm_hoehe: 10.0,
            fehlergrenze_mm_s0_auto_match: 20.0,
            freier_massstab: true,
            berechnung_3d: true,
            sigma_faktor: 1,
            polar_massstab_modus: MassstabModus::Eins,
            polar_massstab_wert: 1.0,
            polar_massstab_quelle: "manuell".to_string(),
            gewichtung_nahe_ziele: true,
            zentrierfehler_ziel_mm: ZENTRIERFEHLER_ZIEL_STANDARD_MM,
            zentrierfehler_standpunkt_mm: ZENTRIERFEHLER_STANDPUNKT_STANDARD_MM,
            pdf: PdfParameter::default(),
            dxf: DxfParameter::default(),
        }
    }
}

impl Rechenparameter {
    /// Wirksamer Sigma-Faktor (1..3).
    pub fn sigma_faktor_wirksam(&self) -> i32 {
        self.sigma_faktor.clamp(SIGMA_MIN, SIGMA_MAX)
    }

    /// Bezeichnung: "s₀", "2·s₀" oder "3·s₀".
    pub fn sigma_bez(&self) -> String {
        match self.sigma_faktor_wirksam() {
            1 => "s₀".to_string(),
            n => format!("{n}·s₀"),
        }
    }

    /// s0 [mm] mit dem eingestellten Faktor multipliziert.
    pub fn sigma_wert(&self, s0_mm: f64) -> f64 {
        s0_mm * f64::from(self.sigma_faktor_wirksam())
    }

    /// Anzeige, z. B. "2·s₀=31,2 mm".
    pub fn sigma_text(&self, s0_mm: f64, stellen: usize) -> String {
        format!(
            "{}={} mm",
            self.sigma_bez(),
            crate::grundlagen::format::de(self.sigma_wert(s0_mm), stellen)
        )
    }
}

/// Verwaltet Laden und Speichern der Rechenparameter.
pub struct RechenparameterManager {
    pfad: PathBuf,
    pub params: Rechenparameter,
}

impl RechenparameterManager {
    /// Lädt die Datei oder legt sie mit Standardwerten an.
    pub fn initialisiere(pfad: &Path) -> Self {
        let mut m = Self {
            pfad: pfad.to_path_buf(),
            params: Rechenparameter::default(),
        };
        if pfad.exists() {
            m.laden();
        } else {
            datei_speicher::laden_erfolgreich(pfad);
            let _ = m.speichern();
        }
        m
    }

    fn laden(&mut self) {
        let inhalt = match fs::read_to_string(&self.pfad) {
            Ok(t) => t,
            Err(e) => {
                datei_speicher::melde_ladefehler(&self.pfad, &e);
                self.params = Rechenparameter::default();
                return;
            }
        };
        match xml_einfach::lese(&inhalt) {
            Ok(doc) if doc.wurzel == WURZEL => {
                self.params = parse_parameter(&doc);
                datei_speicher::laden_erfolgreich(&self.pfad);
            }
            Ok(_) => self.defekt("Ungültige Rechenparameterdatei."),
            Err(fehler) => self.defekt(&fehler),
        }
    }

    fn defekt(&mut self, fehler: &str) {
        self.params = Rechenparameter::default();
        const HINWEIS: &str = "Es gelten die Standardwerte. Bitte vor dem Berechnen Fehlergrenzen, Maßstab, \
                               Gewichtung und Zentrierfehler in den Rechenparametern prüfen.";
        if let Err(e) = datei_speicher::lege_defekte_datei_beiseite(&self.pfad, &fehler, HINWEIS) {
            datei_speicher::melde_ladefehler(&self.pfad, &e);
        }
    }

    /// Speichert die Parameter.
    pub fn speichern(&self) -> io::Result<()> {
        if self.pfad.as_os_str().is_empty() {
            return Ok(());
        }
        let p = &self.params;
        let pdf = p.pdf.normalisiert();
        let dxf = p.dxf.normalisiert();
        let wahr = |b: bool| if b { "true" } else { "false" }.to_string();
        let eintraege: Vec<(&str, String)> = vec![
            ("FehlergrenzCC_Winkel", f(p.fehlergrenze_cc_winkel, 1)),
            ("FehlergrenzeMM_Strecke", f(p.fehlergrenze_mm_strecke, 1)),
            ("FehlergrenzeMM_Hoehe", f(p.fehlergrenze_mm_hoehe, 1)),
            ("FehlergrenzeMM_s0AutoMatch", f(p.fehlergrenze_mm_s0_auto_match, 1)),
            ("FreierMassstab", wahr(p.freier_massstab)),
            ("Berechnung3D", wahr(p.berechnung_3d)),
            ("SigmaFaktor", p.sigma_faktor_wirksam().to_string()),
            ("PolarMassstabModus", p.polar_massstab_modus.name().to_string()),
            ("PolarMassstabWert", f(p.polar_massstab_wert, 6)),
            ("PolarMassstabQuelle", p.polar_massstab_quelle.clone()),
            ("GewichtungNaheZiele", wahr(p.gewichtung_nahe_ziele)),
            ("ZentrierfehlerZiel_mm", f(p.zentrierfehler_ziel_mm, 1)),
            ("ZentrierfehlerStandpunkt_mm", f(p.zentrierfehler_standpunkt_mm, 1)),
            ("PdfMassstab", f(pdf.massstab, 0)),
            ("PdfQuerformat", wahr(pdf.querformat)),
            ("PdfRand_mm", f(pdf.rand_mm, 1)),
            ("PdfLinieMin_mm", f(pdf.linie_min_mm, 2)),
            ("PdfSymbolMin_mm", f(pdf.symbol_min_mm, 1)),
            ("DxfMassstab", f(dxf.massstab, 0)),
            ("DxfSymbol_mm", f(dxf.symbol_mm, 1)),
            ("DxfText_mm", f(dxf.text_mm, 1)),
        ];
        datei_speicher::schreibe_text(&self.pfad, &xml_einfach::schreibe(WURZEL, &eintraege))
    }
}

fn parse_parameter(doc: &XmlDokument) -> Rechenparameter {
    let standard = Rechenparameter::default();
    let zahl = |tag: &str, ersatz: f64| doc.text(tag).and_then(parse_zahl).unwrap_or(ersatz);
    let wahrheit = |tag: &str, ersatz: bool| match doc.text(tag) {
        Some(t) => t.trim().eq_ignore_ascii_case("true"),
        None => ersatz,
    };
    Rechenparameter {
        fehlergrenze_cc_winkel: zahl("FehlergrenzCC_Winkel", standard.fehlergrenze_cc_winkel),
        fehlergrenze_mm_strecke: zahl("FehlergrenzeMM_Strecke", standard.fehlergrenze_mm_strecke),
        fehlergrenze_mm_hoehe: zahl("FehlergrenzeMM_Hoehe", standard.fehlergrenze_mm_hoehe),
        fehlergrenze_mm_s0_auto_match: zahl("FehlergrenzeMM_s0AutoMatch", standard.fehlergrenze_mm_s0_auto_match),
        freier_massstab: wahrheit("FreierMassstab", standard.freier_massstab),
        berechnung_3d: wahrheit("Berechnung3D", standard.berechnung_3d),
        sigma_faktor: zahl("SigmaFaktor", f64::from(standard.sigma_faktor)) as i32,
        polar_massstab_modus: doc
            .text("PolarMassstabModus")
            .and_then(|t| MassstabModus::aus_name(t.trim()))
            .unwrap_or(standard.polar_massstab_modus),
        polar_massstab_wert: zahl("PolarMassstabWert", standard.polar_massstab_wert),
        polar_massstab_quelle: doc
            .text("PolarMassstabQuelle")
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .map_or(standard.polar_massstab_quelle, str::to_string),
        gewichtung_nahe_ziele: wahrheit("GewichtungNaheZiele", standard.gewichtung_nahe_ziele),
        zentrierfehler_ziel_mm: zahl("ZentrierfehlerZiel_mm", ZENTRIERFEHLER_ZIEL_STANDARD_MM),
        zentrierfehler_standpunkt_mm: zahl("ZentrierfehlerStandpunkt_mm", ZENTRIERFEHLER_STANDPUNKT_STANDARD_MM),
        pdf: PdfParameter {
            massstab: zahl("PdfMassstab", standard.pdf.massstab),
            querformat: wahrheit("PdfQuerformat", standard.pdf.querformat),
            rand_mm: zahl("PdfRand_mm", standard.pdf.rand_mm),
            linie_min_mm: zahl("PdfLinieMin_mm", standard.pdf.linie_min_mm),
            symbol_min_mm: zahl("PdfSymbolMin_mm", standard.pdf.symbol_min_mm),
        }.normalisiert(),
        dxf: DxfParameter {
            massstab: zahl("DxfMassstab", standard.dxf.massstab),
            symbol_mm: zahl("DxfSymbol_mm", standard.dxf.symbol_mm),
            text_mm: zahl("DxfText_mm", standard.dxf.text_mm),
        }.normalisiert(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_und_sigma() {
        let dir = std::env::temp_dir().join(format!("fb-rp-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let pfad = dir.join("Freie-Stationierung.xml");
        let mut m = RechenparameterManager::initialisiere(&pfad);
        m.params.sigma_faktor = 2;
        m.params.freier_massstab = false;
        m.params.polar_massstab_modus = MassstabModus::Vorgegeben;
        m.params.polar_massstab_wert = 1.000123;
        m.params.pdf = PdfParameter { massstab: 500.0, querformat: true, rand_mm: 12.0, linie_min_mm: 0.35, symbol_min_mm: 3.0 };
        m.params.dxf = DxfParameter { massstab: 2500.0, symbol_mm: 2.5, text_mm: 3.0 };
        m.speichern().unwrap();

        let neu = RechenparameterManager::initialisiere(&pfad);
        assert_eq!(neu.params.sigma_faktor, 2);
        assert!(!neu.params.freier_massstab);
        assert_eq!(neu.params.polar_massstab_modus, MassstabModus::Vorgegeben);
        assert!((neu.params.polar_massstab_wert - 1.000123).abs() < 1e-9);
        assert_eq!(neu.params.sigma_bez(), "2·s₀");
        assert_eq!(neu.params.pdf, m.params.pdf);
        assert_eq!(neu.params.dxf, m.params.dxf);
        assert!((neu.params.sigma_wert(1.5) - 3.0).abs() < 1e-12);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn sigma_faktor_wird_begrenzt() {
        let p = Rechenparameter { sigma_faktor: 9, ..Rechenparameter::default() };
        assert_eq!(p.sigma_faktor_wirksam(), 3);
        let p = Rechenparameter { sigma_faktor: -4, ..Rechenparameter::default() };
        assert_eq!(p.sigma_faktor_wirksam(), 1);
    }

    #[test]
    fn alte_parameterdateien_erhalten_pdf_standardwerte() {
        let doc = xml_einfach::lese(&format!("<{WURZEL}><Berechnung3D>false</Berechnung3D></{WURZEL}>")).unwrap();
        let p = parse_parameter(&doc);
        assert_eq!(p.pdf, PdfParameter::default());
        assert_eq!(p.dxf, DxfParameter::default());
        assert!(!p.berechnung_3d);
    }

    #[test]
    fn pdf_parameter_halten_papier_mindestmasse_ein() {
        let doc = xml_einfach::lese(&format!("<{WURZEL}><PdfMassstab>-1</PdfMassstab><PdfLinieMin_mm>0.01</PdfLinieMin_mm><PdfSymbolMin_mm>0.5</PdfSymbolMin_mm><PdfRand_mm>200</PdfRand_mm></{WURZEL}>")).unwrap();
        let p = parse_parameter(&doc);
        assert_eq!(p.pdf.massstab, 1.0);
        assert_eq!(p.pdf.linie_min_mm, 0.25);
        assert_eq!(p.pdf.symbol_min_mm, 2.0);
        assert_eq!(p.pdf.rand_mm, 50.0);
        let p = PdfParameter { massstab: f64::NAN, linie_min_mm: f64::INFINITY, ..PdfParameter::default() }.normalisiert();
        assert_eq!(p, PdfParameter::default());
    }
}
