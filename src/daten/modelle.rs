// Zweck:    Datenmodelle des Feldbuchs (Punkte, Stationierung, Rohdaten) – JSON-kompatibel zur C#-Version
// Hardware: keine
// Autor:    Johann Gerner

use chrono::{Local, NaiveDateTime};
use serde::{Deserialize, Serialize};

use crate::grundlagen::serde_hilfen::{datum_zeit, f64_text};

/// Typ-Kennung für berechnete Standpunkte.
pub const TYP_STANDPUNKT: &str = "Standpunkt";
/// Typ-Kennung für berechnete Neupunkte.
pub const TYP_NEUPUNKT: &str = "Neupunkt";

/// Aktuelle lokale Zeit ohne Zeitzone.
pub fn jetzt() -> NaiveDateTime {
    Local::now().naive_local()
}

/// Heutiges Datum als Text (yyyy-MM-dd).
pub fn heute_text() -> String {
    Local::now().format("%Y-%m-%d").to_string()
}

fn standard_wahr() -> bool {
    true
}

fn standard_nan() -> f64 {
    f64::NAN
}

fn standard_eins() -> f64 {
    1.0
}

fn standard_zeit() -> NaiveDateTime {
    jetzt()
}

// ── Feldbuchpunkte ────────────────────────────────────────────────────────────

/// Berechneter Punkt (Standpunkt oder Neupunkt) in Feldbuchpunkte.json.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct FeldbuchPunkt {
    #[serde(rename = "PunktNr")]
    pub punkt_nr: String,
    #[serde(rename = "Typ")]
    pub typ: String,
    #[serde(rename = "R")]
    pub r: f64,
    #[serde(rename = "H")]
    pub h: f64,
    #[serde(rename = "Hoehe")]
    pub hoehe: f64,
    #[serde(rename = "Orientierung_gon")]
    pub orientierung_gon: f64,
    #[serde(rename = "IstBerechnung3D")]
    pub ist_berechnung_3d: bool,
    #[serde(rename = "Datum")]
    pub datum: String,
    #[serde(rename = "Quelle")]
    pub quelle: String,
}

impl Default for FeldbuchPunkt {
    fn default() -> Self {
        Self {
            punkt_nr: String::new(),
            typ: TYP_NEUPUNKT.to_string(),
            r: 0.0,
            h: 0.0,
            hoehe: 0.0,
            orientierung_gon: 0.0,
            ist_berechnung_3d: true,
            datum: String::new(),
            quelle: String::new(),
        }
    }
}

/// Dateiinhalt von Feldbuchpunkte.json.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct FeldbuchDaten {
    #[serde(rename = "Punkte")]
    pub punkte: Vec<FeldbuchPunkt>,
}

// ── Importpunkte ──────────────────────────────────────────────────────────────

/// Importierter Punkt (KOR / CSV / JSON).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct ImportPunkt {
    #[serde(rename = "PunktNr")]
    pub punkt_nr: String,
    #[serde(rename = "R")]
    pub r: f64,
    #[serde(rename = "H")]
    pub h: f64,
    #[serde(rename = "Hoehe")]
    pub hoehe: f64,
    #[serde(rename = "Quelle")]
    pub quelle: String,
    #[serde(rename = "Datum")]
    pub datum: String,
}

/// Dateiinhalt von ImportPunkte.json.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct ImportPunkteDaten {
    #[serde(rename = "Punkte")]
    pub punkte: Vec<ImportPunkt>,
}

// ── Stationierung ─────────────────────────────────────────────────────────────

/// Instrumentengenauigkeiten für die Ausgleichung.
#[derive(Clone, Debug, PartialEq)]
pub struct InstrumentParameter {
    /// Richtungsgenauigkeit [cc].
    pub m_r_cc: f64,
    /// Streckenkonstante [mm].
    pub msk_mm: f64,
    /// Streckenmaßstabsanteil [ppm].
    pub msv_ppm: f64,
    /// Standpunktzentrierfehler [mm].
    pub m_spz_mm: f64,
    /// Zielpunktzentrierfehler [mm].
    pub m_zpz_mm: f64,
    /// Gewichtseinheit [mm].
    pub m0_mm: f64,
}

impl Default for InstrumentParameter {
    fn default() -> Self {
        Self {
            m_r_cc: 9.0,
            msk_mm: 3.0,
            msv_ppm: 2.0,
            m_spz_mm: 0.0,
            m_zpz_mm: 0.0,
            m0_mm: 1.0,
        }
    }
}

/// Gemessener Anschlusspunkt der Stationierung.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct StationierungsPunkt {
    #[serde(rename = "PunktNr")]
    pub punkt_nr: String,
    #[serde(rename = "R", with = "f64_text")]
    pub r: f64,
    #[serde(rename = "H", with = "f64_text")]
    pub h: f64,
    #[serde(rename = "Hoehe", with = "f64_text")]
    pub hoehe: f64,
    /// Horizontalrichtung, unorientiert [gon].
    #[serde(rename = "HZ", with = "f64_text")]
    pub hz: f64,
    /// Zenitwinkel [gon].
    #[serde(rename = "V", with = "f64_text")]
    pub v: f64,
    /// Schrägstrecke [m].
    #[serde(rename = "Strecke", with = "f64_text")]
    pub strecke: f64,
    /// Zielhöhe [m].
    #[serde(rename = "Zielhoehe", with = "f64_text")]
    pub zielhoehe: f64,
}

/// Residuen eines Anschlusspunktes.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct PunktResiduum {
    #[serde(rename = "PunktNr")]
    pub punkt_nr: String,
    /// Horizontalstrecke [m].
    #[serde(rename = "StreckeH", with = "f64_text")]
    pub strecke_h: f64,
    /// Winkelresiduum [cc].
    #[serde(rename = "vWinkel_cc", with = "f64_text")]
    pub v_winkel_cc: f64,
    /// Querabweichung [mm].
    #[serde(rename = "vQuer_mm", with = "f64_text")]
    pub v_quer_mm: f64,
    /// Längsabweichung [mm].
    #[serde(rename = "vStrecke_mm", with = "f64_text")]
    pub v_strecke_mm: f64,
    /// Höhenresiduum [mm].
    #[serde(rename = "vHoehe_mm", with = "f64_text")]
    pub v_hoehe_mm: f64,
    #[serde(rename = "RichtungAktiv")]
    pub richtung_aktiv: bool,
    #[serde(rename = "StreckeAktiv")]
    pub strecke_aktiv: bool,
    #[serde(rename = "HoeheAktiv")]
    pub hoehe_aktiv: bool,
    /// Relatives Richtungsgewicht (0..1); NaN = nicht bestimmt.
    #[serde(rename = "GewichtRichtung", with = "f64_text", default = "standard_nan")]
    pub gewicht_richtung: f64,
}

impl Default for PunktResiduum {
    fn default() -> Self {
        Self {
            punkt_nr: String::new(),
            strecke_h: 0.0,
            v_winkel_cc: 0.0,
            v_quer_mm: 0.0,
            v_strecke_mm: 0.0,
            v_hoehe_mm: 0.0,
            richtung_aktiv: standard_wahr(),
            strecke_aktiv: standard_wahr(),
            hoehe_aktiv: standard_wahr(),
            gewicht_richtung: f64::NAN,
        }
    }
}

/// Ergebnis einer (freien) Stationierung.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct StationierungsErgebnis {
    #[serde(rename = "R", with = "f64_text")]
    pub r: f64,
    #[serde(rename = "H", with = "f64_text")]
    pub h: f64,
    #[serde(rename = "Hoehe", with = "f64_text")]
    pub hoehe: f64,
    #[serde(rename = "Orientierung_gon", with = "f64_text")]
    pub orientierung_gon: f64,
    #[serde(rename = "Massstab", with = "f64_text")]
    pub massstab: f64,
    #[serde(rename = "s0_mm", with = "f64_text")]
    pub s0_mm: f64,
    /// Redundanz: n_aktiv_dir + n_aktiv_dist − 3.
    #[serde(rename = "Redundanz")]
    pub redundanz: i32,
    #[serde(rename = "Iterationen")]
    pub iterationen: i32,
    #[serde(rename = "Konvergiert")]
    pub konvergiert: bool,
    #[serde(rename = "Berechnung3D")]
    pub berechnung_3d: bool,
    /// Lokales System ohne DXF: Standpunkt 0/0/0, Orientierung 0 gon.
    #[serde(rename = "IstLokalesSystem")]
    pub ist_lokales_system: bool,
    #[serde(rename = "WarnungHoehe")]
    pub warnung_hoehe: String,
    #[serde(rename = "Residuen")]
    pub residuen: Vec<PunktResiduum>,
}

impl Default for StationierungsErgebnis {
    fn default() -> Self {
        Self {
            r: 0.0,
            h: 0.0,
            hoehe: 0.0,
            orientierung_gon: 0.0,
            massstab: 0.0,
            s0_mm: 0.0,
            redundanz: 0,
            iterationen: 0,
            konvergiert: false,
            berechnung_3d: true,
            ist_lokales_system: false,
            warnung_hoehe: String::new(),
            residuen: Vec::new(),
        }
    }
}

// ── Neupunkte ─────────────────────────────────────────────────────────────────

/// Unveränderliche Rohdaten einer Neupunkt-Messung.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct NeupunktRohdaten {
    #[serde(rename = "PunktNr")]
    pub punkt_nr: String,
    #[serde(rename = "Code")]
    pub code: String,
    #[serde(rename = "Hz_gon", with = "f64_text")]
    pub hz_gon: f64,
    #[serde(rename = "V_gon", with = "f64_text")]
    pub v_gon: f64,
    #[serde(rename = "Strecke_m", with = "f64_text")]
    pub strecke_m: f64,
    #[serde(rename = "Zielhoehe_m", with = "f64_text")]
    pub zielhoehe_m: f64,
    #[serde(rename = "Zeitstempel", with = "datum_zeit")]
    pub zeitstempel: NaiveDateTime,
    #[serde(rename = "StandpunktNr")]
    pub standpunkt_nr: String,
    /// Längsexzentrizität [m]: positiv = weiter vom Standpunkt weg.
    #[serde(rename = "ExzLaengs_m", with = "f64_text")]
    pub exz_laengs_m: f64,
    /// Querexzentrizität [m]: positiv = rechts der Messrichtung.
    #[serde(rename = "ExzQuer_m", with = "f64_text")]
    pub exz_quer_m: f64,
}

impl Default for NeupunktRohdaten {
    fn default() -> Self {
        Self {
            punkt_nr: String::new(),
            code: String::new(),
            hz_gon: 0.0,
            v_gon: 0.0,
            strecke_m: 0.0,
            zielhoehe_m: 0.0,
            zeitstempel: standard_zeit(),
            standpunkt_nr: String::new(),
            exz_laengs_m: 0.0,
            exz_quer_m: 0.0,
        }
    }
}

/// Berechnetes Ergebnis einer Neupunktmessung.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct NeupunktErgebnis {
    #[serde(rename = "PunktNr")]
    pub punkt_nr: String,
    #[serde(rename = "Code")]
    pub code: String,
    #[serde(rename = "R", with = "f64_text")]
    pub r: f64,
    #[serde(rename = "H", with = "f64_text")]
    pub h: f64,
    #[serde(rename = "Hoehe", with = "f64_text")]
    pub hoehe: f64,
    #[serde(rename = "Ist3D")]
    pub ist_3d: bool,
    /// Orientierte Horizontalrichtung [gon].
    #[serde(rename = "HzOrientiert", with = "f64_text")]
    pub hz_orientiert: f64,
    #[serde(rename = "HzRoh", with = "f64_text")]
    pub hz_roh: f64,
    #[serde(rename = "V_gon", with = "f64_text")]
    pub v_gon: f64,
    #[serde(rename = "Strecke_m", with = "f64_text")]
    pub strecke_m: f64,
    #[serde(rename = "Zielhoehe_m", with = "f64_text")]
    pub zielhoehe_m: f64,
    #[serde(rename = "Zeitstempel", with = "datum_zeit")]
    pub zeitstempel: NaiveDateTime,
    #[serde(rename = "StandpunktNr")]
    pub standpunkt_nr: String,
    /// Bei der Berechnung verwendete Orientierung [gon].
    #[serde(rename = "VerwendeteOrientierung", with = "f64_text")]
    pub verwendete_orientierung: f64,
    #[serde(rename = "ExzLaengs_m", with = "f64_text")]
    pub exz_laengs_m: f64,
    #[serde(rename = "ExzQuer_m", with = "f64_text")]
    pub exz_quer_m: f64,
    /// Angewandter Maßstab der Horizontalstrecke (Standard 1).
    #[serde(rename = "VerwendeterMassstab", with = "f64_text", default = "standard_eins")]
    pub verwendeter_massstab: f64,
    /// Herkunft: leer = DXF-Viewer, "Polar" = Polarmodul.
    #[serde(rename = "Quelle")]
    pub quelle: String,
}

impl Default for NeupunktErgebnis {
    fn default() -> Self {
        Self {
            punkt_nr: String::new(),
            code: String::new(),
            r: 0.0,
            h: 0.0,
            hoehe: 0.0,
            ist_3d: false,
            hz_orientiert: 0.0,
            hz_roh: 0.0,
            v_gon: 0.0,
            strecke_m: 0.0,
            zielhoehe_m: 0.0,
            zeitstempel: standard_zeit(),
            standpunkt_nr: String::new(),
            verwendete_orientierung: 0.0,
            exz_laengs_m: 0.0,
            exz_quer_m: 0.0,
            verwendeter_massstab: 1.0,
            quelle: String::new(),
        }
    }
}

// ── Konvertierung ─────────────────────────────────────────────────────────────

/// Datensatz der Konvertierungstabelle (Muster.csv + Punktcode + Bemerkung).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct KonvertierungPunkt {
    pub punkt_nr: String,
    pub typ: String,
    pub r: f64,
    pub h: f64,
    pub hoehe: f64,
    pub hz: f64,
    pub v: f64,
    pub strecke: f64,
    pub zielhoehe: f64,
    pub punktcode: String,
    pub bemerkung: String,
}
