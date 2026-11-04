// Zweck:    Universelles Messdatenmodell für alle Tachymeterformate (Winkel gon, Strecken m, Prismenkonstante mm)
// Hardware: Leica GeoCOM / GSI, Sokkia SDR, Topcon, GNSS
// Autor:    Johann Gerner

use chrono::NaiveDateTime;

use crate::daten::modelle::{KonvertierungPunkt, jetzt};
use crate::grundlagen::format::f;

/// Art der Messung bzw. des Datensatzes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum MessungsTyp {
    #[default]
    Unbekannt,
    /// Nur Hz und V.
    Winkel,
    /// Hz, V und Schrägstrecke.
    Vollmessung,
    /// Direkte Koordinaten.
    Koordinate,
    /// Gerätestatus, Bestätigung, Ping-Antwort.
    Status,
    /// EDM-Modus-Rückmeldung.
    EdmModusInfo,
    /// Atmosphärische Korrekturdaten.
    AtmKorrektur,
    /// Fehlermeldung.
    Fehler,
}

/// EDM-Messmodus (Reflektortyp).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum MessungsEdmModus {
    #[default]
    Unbekannt,
    Prisma,
    Folie,
    Reflektorlos,
}

/// Vollständiger Messdatensatz; nicht belegte Felder bleiben `None`.
#[derive(Clone, Debug)]
pub struct TachymeterMessung {
    pub zeitstempel: NaiveDateTime,
    pub quelle: String,
    pub rohdaten: String,
    pub typ: MessungsTyp,
    pub punkt_nr: String,
    pub punktcode: String,
    pub hz_gon: Option<f64>,
    pub v_gon: Option<f64>,
    pub schraegstrecke_m: Option<f64>,
    pub horizontalstrecke_m: Option<f64>,
    pub hoehenunterschied_m: Option<f64>,
    pub e_m: Option<f64>,
    pub n_m: Option<f64>,
    pub h_m: Option<f64>,
    pub standpunkt_e_m: Option<f64>,
    pub standpunkt_n_m: Option<f64>,
    pub standpunkt_h_m: Option<f64>,
    pub zielhoehe_m: Option<f64>,
    pub instrumenthoehe_m: Option<f64>,
    pub prismenkonstante_mm: Option<f64>,
    pub edm_modus: MessungsEdmModus,
    pub return_code: Option<i32>,
    pub edm_modus_roh: Option<i32>,
    pub winkel_genauigkeit_cc: Option<f64>,
    pub strecken_genauigkeit_mm: Option<f64>,
    pub kreuz_neigung_rad: Option<f64>,
    pub laengs_neigung_rad: Option<f64>,
    pub atm_lambda_m: Option<f64>,
    pub atm_druck_mbar: Option<f64>,
    pub atm_temp_trock_c: Option<f64>,
    pub atm_temp_feucht_c: Option<f64>,
    pub atm_ppm: Option<f64>,
    pub bemerkung: String,
}

impl Default for TachymeterMessung {
    fn default() -> Self {
        Self {
            zeitstempel: jetzt(),
            quelle: String::new(),
            rohdaten: String::new(),
            typ: MessungsTyp::Unbekannt,
            punkt_nr: String::new(),
            punktcode: String::new(),
            hz_gon: None,
            v_gon: None,
            schraegstrecke_m: None,
            horizontalstrecke_m: None,
            hoehenunterschied_m: None,
            e_m: None,
            n_m: None,
            h_m: None,
            standpunkt_e_m: None,
            standpunkt_n_m: None,
            standpunkt_h_m: None,
            zielhoehe_m: None,
            instrumenthoehe_m: None,
            prismenkonstante_mm: None,
            edm_modus: MessungsEdmModus::Unbekannt,
            return_code: None,
            edm_modus_roh: None,
            winkel_genauigkeit_cc: None,
            strecken_genauigkeit_mm: None,
            kreuz_neigung_rad: None,
            laengs_neigung_rad: None,
            atm_lambda_m: None,
            atm_druck_mbar: None,
            atm_temp_trock_c: None,
            atm_temp_feucht_c: None,
            atm_ppm: None,
            bemerkung: String::new(),
        }
    }
}

impl TachymeterMessung {
    /// Neue Messung mit Quelle und Rohdaten.
    pub fn neu(quelle: &str, rohdaten: &str) -> Self {
        Self { quelle: quelle.to_string(), rohdaten: rohdaten.to_string(), ..Self::default() }
    }

    /// Ist ein Returncode vorhanden und ungleich 0?
    pub fn ist_fehler(&self) -> bool {
        self.return_code.is_some_and(|rc| rc != 0)
    }

    /// Sind Winkeldaten vorhanden?
    pub fn hat_winkel(&self) -> bool {
        self.hz_gon.is_some() || self.v_gon.is_some()
    }

    /// Liegt eine vollständige Polarmessung vor (Hz, V, Schrägstrecke)?
    pub fn ist_vollmessung(&self) -> bool {
        self.hz_gon.is_some() && self.v_gon.is_some() && self.schraegstrecke_m.is_some()
    }

    /// Wandelt die Messung in einen Datensatz der Konvertierungstabelle um.
    pub fn zu_konvertierung_punkt(&self) -> KonvertierungPunkt {
        KonvertierungPunkt {
            punkt_nr: self.punkt_nr.clone(),
            typ: self.quelle.clone(),
            r: self.e_m.unwrap_or(0.0),
            h: self.n_m.unwrap_or(0.0),
            hoehe: self.h_m.unwrap_or(0.0),
            hz: self.hz_gon.unwrap_or(0.0),
            v: self.v_gon.unwrap_or(0.0),
            strecke: self.schraegstrecke_m.unwrap_or(0.0),
            zielhoehe: self.zielhoehe_m.unwrap_or(0.0),
            punktcode: self.punktcode.clone(),
            bemerkung: self.bemerkung.clone(),
        }
    }

    /// Lesbare Darstellung für Protokollfenster.
    pub fn text(&self) -> String {
        let mut s = format!("[{}] {:?}", self.quelle, self.typ);
        let mut teil = |bedingung: Option<String>| {
            if let Some(t) = bedingung {
                s.push_str("  ");
                s.push_str(&t);
            }
        };
        teil((!self.punkt_nr.is_empty()).then(|| format!("Pkt:{}", self.punkt_nr)));
        teil(self.hz_gon.map(|v| format!("Hz:{} gon", f(v, 4))));
        teil(self.v_gon.map(|v| format!("V:{} gon", f(v, 4))));
        teil(self.schraegstrecke_m.map(|v| format!("D:{} m", f(v, 3))));
        teil(self.horizontalstrecke_m.map(|v| format!("Dh:{} m", f(v, 3))));
        teil(self.hoehenunterschied_m.map(|v| format!("Δh:{} m", f(v, 3))));
        teil(self.e_m.map(|v| format!("E:{}", f(v, 3))));
        teil(self.n_m.map(|v| format!("N:{}", f(v, 3))));
        teil(self.h_m.map(|v| format!("H:{}", f(v, 3))));
        teil(self.prismenkonstante_mm.map(|v| format!("PC:{} mm", f(v, 1))));
        teil((self.edm_modus != MessungsEdmModus::Unbekannt).then(|| format!("EDM:{:?}", self.edm_modus)));
        // Nur echte Fehler markieren; informative Codes (z. B. 3081) stehen in der Bemerkung
        teil((self.typ == MessungsTyp::Fehler).then(|| format!("!!GRC={}", self.return_code.unwrap_or(0))));
        teil((!self.bemerkung.is_empty()).then(|| format!("({})", self.bemerkung)));
        s
    }
}
