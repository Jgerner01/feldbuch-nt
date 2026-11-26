// Zweck:    Einpassung der GNSS-Koordinaten (ETRS89/UTM32) in das Plansystem über identische Punkte (T-Punkte):
//           Methoden aus der Koordinatentransformation (2D-Helmert frei/fest, 3D-Helmert, 6 und 9 Parameter)
//           sowie reine Verschiebung; Höhe getrennt (Verschiebung, geneigte Ebene) oder mit den 3D-Methoden.
//           Speicherung aller Einpassungen eines Projekts in Einpassung.json.
// Hardware: keine (Messwerte vom GNSS-Empfänger u-blox ZED-F9P)
// Autor:    Johann Gerner

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::daten::einstellungen::Einstellungen;
use crate::grundlagen::datei_speicher;
use crate::rechnen::hoehen_einpassung::{self as he, HoehenErgebnis, HoehenModell, HoehenPunkt};
use crate::rechnen::koord_transformation::{self as kt, TransformPunkt, TransformationsTyp};

/// Dateiname der Einpassungen im Projektverzeichnis.
const DATEINAME: &str = "Einpassung.json";
/// Meter → Millimeter.
const MM_PRO_M: f64 = 1000.0;
/// Millionstel.
const PPM: f64 = 1e6;
/// Mittlerer Erdradius für die Höhenreduktion [m] (Gaußscher Krümmungsradius in Bayern).
pub const ERDRADIUS_M: f64 = 6_383_000.0;
/// Mindestanzahl T-Punkte der Verschiebung.
const MIN_PUNKTE_VERSCHIEBUNG: usize = 1;
/// Unter diesem Verhältnis kleinste/größte Ausdehnung gelten die T-Punkte als fast auf einer Linie.
const LINIEN_VERHAELTNIS: f64 = 0.05;
/// Ein Punkt gilt als möglicher grober Fehler, wenn seine Abweichung bei einer Berechnung ohne ihn dieses
/// Vielfache der Standardabweichung der übrigen Punkte übersteigt …
const GROBER_FEHLER_FAKTOR: f64 = 3.0;
/// … und zugleich mindestens so groß ist [mm] (verhindert Fehlalarme bei sehr kleinem s0).
const GROBER_FEHLER_MIN_MM: f64 = 20.0;

/// Woher kommen die Plankoordinaten der GNSS-Messungen?
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GnssBezug {
    /// über die aktive Einpassung (Regelfall)
    Einpassung,
    /// direkt ETRS89/UTM32, Ostwert ohne Zonenkennzahl
    UtmOhneZone,
    /// direkt ETRS89/UTM32, Ostwert mit Zonenkennzahl 32
    UtmMitZone,
}

impl GnssBezug {
    pub const ALLE: [Self; 3] = [Self::Einpassung, Self::UtmOhneZone, Self::UtmMitZone];

    pub fn name(self) -> &'static str {
        match self {
            Self::Einpassung => "Einpassung",
            Self::UtmOhneZone => "UTM32",
            Self::UtmMitZone => "UTM32Zone",
        }
    }

    pub fn anzeige(self) -> &'static str {
        match self {
            Self::Einpassung => "Einpassung über T-Punkte (Regelfall)",
            Self::UtmOhneZone => "ETRS89/UTM32, Ostwert ohne 32",
            Self::UtmMitZone => "ETRS89/UTM32, Ostwert mit 32",
        }
    }

    pub fn aus_name(name: &str) -> Option<Self> {
        Self::ALLE.into_iter().find(|b| b.name().eq_ignore_ascii_case(name.trim()))
    }
}

/// Lagemethode der Einpassung.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EinpassMethode {
    Verschiebung,
    Helmert2DFest,
    Helmert2D,
    Helmert3D,
    Parameter6,
    Parameter9,
}

impl EinpassMethode {
    pub const ALLE: [Self; 6] = [Self::Helmert2D, Self::Helmert2DFest, Self::Verschiebung, Self::Helmert3D, Self::Parameter6, Self::Parameter9];

    pub fn name(self) -> &'static str {
        match self {
            Self::Verschiebung => "Verschiebung",
            Self::Helmert2DFest => "Helmert2DFest",
            Self::Helmert2D => "Helmert2D",
            Self::Helmert3D => "Helmert3D",
            Self::Parameter6 => "Parameter6",
            Self::Parameter9 => "Parameter9",
        }
    }

    pub fn anzeige(self) -> &'static str {
        match self {
            Self::Verschiebung => "Verschiebung (2 Parameter)",
            Self::Helmert2DFest => "2D-Helmert, fester Maßstab (3 Parameter)",
            Self::Helmert2D => "2D-Helmert (4 Parameter)",
            Self::Helmert3D => "3D-Helmert / Bursa-Wolf (7 Parameter)",
            Self::Parameter6 => "3D, fester Maßstab (6 Parameter)",
            Self::Parameter9 => "3D, getrennte Maßstäbe (9 Parameter)",
        }
    }

    pub fn aus_name(name: &str) -> Option<Self> {
        Self::ALLE.into_iter().find(|m| m.name().eq_ignore_ascii_case(name.trim()))
    }

    /// Lage und Höhe gemeinsam (3D-Methoden).
    pub fn ist_3d(self) -> bool {
        matches!(self, Self::Helmert3D | Self::Parameter6 | Self::Parameter9)
    }

    /// Maßstab fest: die Quellkoordinaten werden vorab mit dem UTM-Maßstab am Ort auf Geländemaß gebracht.
    pub fn fester_massstab(self) -> bool {
        matches!(self, Self::Helmert2DFest | Self::Parameter6)
    }

    /// Transformationstyp der Koordinatentransformation (nicht für die Verschiebung).
    fn typ(self) -> TransformationsTyp {
        match self {
            Self::Verschiebung | Self::Helmert2DFest | Self::Helmert2D => TransformationsTyp::Helmert2D,
            Self::Helmert3D | Self::Parameter6 => TransformationsTyp::Parameter7,
            Self::Parameter9 => TransformationsTyp::Parameter9,
        }
    }

    pub fn min_punkte(self) -> usize {
        match self {
            Self::Verschiebung => MIN_PUNKTE_VERSCHIEBUNG,
            m => m.typ().min_punkte(m.fester_massstab()),
        }
    }
}

/// Höheneinpassung bei den 2D-Methoden.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum HoehenEinpassung {
    Keine,
    Verschiebung,
    Ebene,
}

impl HoehenEinpassung {
    pub const ALLE: [Self; 3] = [Self::Verschiebung, Self::Ebene, Self::Keine];

    pub fn name(self) -> &'static str {
        match self {
            Self::Keine => "Keine",
            Self::Verschiebung => "Verschiebung",
            Self::Ebene => "Ebene",
        }
    }

    pub fn anzeige(self) -> &'static str {
        match self {
            Self::Keine => "keine (Höhe wie GNSS-Höhenbezug)",
            Self::Verschiebung => "Höhenverschiebung (1 Parameter)",
            Self::Ebene => "geneigte Ebene (3 Parameter)",
        }
    }

    pub fn aus_name(name: &str) -> Option<Self> {
        Self::ALLE.into_iter().find(|m| m.name().eq_ignore_ascii_case(name.trim()))
    }

    fn modell(self) -> Option<HoehenModell> {
        match self {
            Self::Keine => None,
            Self::Verschiebung => Some(HoehenModell::Verschiebung),
            Self::Ebene => Some(HoehenModell::Ebene),
        }
    }
}

/// UTM-Maßstab am Ort einschließlich Höhenreduktion: Gitterstrecke · m = Geländestrecke.
pub fn gelaende_massstab(k_utm: f64, hoehe_m: f64) -> f64 {
    (1.0 + hoehe_m / ERDRADIUS_M) / k_utm
}

/// Ein identischer Punkt: Soll im Plansystem, Ist aus der GNSS-Messung.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TPunkt {
    pub nr: String,
    pub soll_r: f64,
    pub soll_h: f64,
    pub soll_hoehe: Option<f64>,
    /// UTM32 ohne Zonenkennzahl
    pub ist_e: f64,
    pub ist_n: f64,
    /// Höhe im GNSS-Höhenbezug (Bodenpunkt)
    pub ist_hoehe: Option<f64>,
    /// Streuung der Mittelung [mm]
    pub s_lage_mm: f64,
    pub s_hoehe_mm: f64,
    pub epochen: usize,
    /// Anzahl gemittelter Belegungen
    #[serde(default = "eins")]
    pub belegungen: usize,
    pub lage_aktiv: bool,
    pub hoehe_aktiv: bool,
    pub zeit: String,
}

fn eins() -> usize {
    1
}

impl TPunkt {
    /// Mittelt eine weitere Belegung desselben Punkts; liefert die Lage- und Höhendifferenz zur bisherigen [m].
    pub fn mittle(&mut self, neu: &TPunkt) -> (f64, Option<f64>) {
        let n = self.belegungen as f64;
        let d_lage = (neu.ist_e - self.ist_e).hypot(neu.ist_n - self.ist_n);
        let d_hoehe = neu.ist_hoehe.zip(self.ist_hoehe).map(|(a, b)| a - b);
        self.ist_e = (self.ist_e * n + neu.ist_e) / (n + 1.0);
        self.ist_n = (self.ist_n * n + neu.ist_n) / (n + 1.0);
        self.ist_hoehe = match (self.ist_hoehe, neu.ist_hoehe) {
            (Some(a), Some(b)) => Some((a * n + b) / (n + 1.0)),
            (a, b) => a.or(b),
        };
        self.s_lage_mm = self.s_lage_mm.max(neu.s_lage_mm);
        self.s_hoehe_mm = self.s_hoehe_mm.max(neu.s_hoehe_mm);
        self.epochen += neu.epochen;
        self.belegungen += 1;
        self.zeit.clone_from(&neu.zeit);
        (d_lage, d_hoehe)
    }
}

/// Residuen eines T-Punkts [mm] (v = berechnet − Soll).
#[derive(Clone, Debug, PartialEq)]
pub struct Residuum {
    pub nr: String,
    pub v_r: Option<f64>,
    pub v_h: Option<f64>,
    pub v_hoehe: Option<f64>,
}

impl Residuum {
    pub fn v_lage(&self) -> Option<f64> {
        self.v_r.zip(self.v_h).map(|(a, b)| a.hypot(b))
    }
}

/// Ergebnis der Lageeinpassung.
#[derive(Clone, Debug)]
pub struct LageErgebnis {
    /// `None` bei der Verschiebung
    pub kt: Option<kt::Ergebnis>,
    pub typ: TransformationsTyp,
    pub fest: bool,
    /// Schwerpunkte, auf die reduziert wurde (Quelle: UTM, Ziel: Plan)
    pub quelle_sp: (f64, f64, f64),
    pub ziel_sp: (f64, f64, f64),
    /// Vorab-Maßstab der Quellkoordinaten (1 bei freiem Maßstab)
    pub massstab_vorgabe: f64,
    /// Verschiebung (nur bei der Methode Verschiebung)
    pub verschiebung: (f64, f64),
    pub s0_mm: f64,
}

impl LageErgebnis {
    /// Wirksamer Maßstab UTM → Plan.
    pub fn massstab(&self) -> f64 {
        let m = match &self.kt {
            Some(e) if self.typ == TransformationsTyp::Helmert2D => e.parameter.massstab_2d,
            Some(e) if self.typ == TransformationsTyp::Parameter9 => 1.0 + (e.parameter.mx + e.parameter.my) / 2.0,
            Some(e) => 1.0 + e.parameter.m,
            None => 1.0,
        };
        m * self.massstab_vorgabe
    }

    /// Drehung [gon] (nur 2D).
    pub fn drehung_gon(&self) -> Option<f64> {
        self.kt.as_ref().filter(|_| self.typ == TransformationsTyp::Helmert2D).map(|e| e.parameter.alpha_gon)
    }
}

/// Vollständiges Ergebnis einer Einpassung.
#[derive(Clone, Debug)]
pub struct EinpassErgebnis {
    pub lage: LageErgebnis,
    pub hoehe: Option<HoehenErgebnis>,
    pub residuen: Vec<Residuum>,
    /// Warnungen (Geometrie, grobe Fehler, Maßstab)
    pub hinweise: Vec<String>,
    /// Rahmen der aktiven T-Punkte (UTM): min_e, min_n, max_e, max_n
    pub rahmen: (f64, f64, f64, f64),
}

impl EinpassErgebnis {
    /// Wendet die Einpassung an: UTM (ohne Zone) und Höhe im GNSS-Höhenbezug → R, H, Höhe.
    pub fn transformiere(&self, e: f64, n: f64, hoehe: Option<f64>) -> (f64, f64, Option<f64>) {
        let l = &self.lage;
        let m = l.massstab_vorgabe;
        let q = TransformPunkt::neu("", (e - l.quelle_sp.0) * m, (n - l.quelle_sp.1) * m, hoehe.map_or(0.0, |h| h - l.quelle_sp.2));
        let (r, h, z) = match &l.kt {
            Some(erg) => {
                let t = kt::transformiere(l.typ, &erg.parameter, &q);
                (t.x + l.ziel_sp.0, t.y + l.ziel_sp.1, t.z + l.ziel_sp.2)
            }
            None => (q.x + l.verschiebung.0 + l.ziel_sp.0, q.y + l.verschiebung.1 + l.ziel_sp.1, q.z),
        };
        let hoehe_plan = match (&self.hoehe, hoehe) {
            (_, None) => None,
            (Some(he), Some(ist)) => Some(he.anwenden(e, n, ist)),
            // 3D-Methoden: Höhe aus der räumlichen Transformation
            (None, Some(_)) if l.typ != TransformationsTyp::Helmert2D => Some(z),
            (None, Some(ist)) => Some(ist),
        };
        (r, h, hoehe_plan)
    }

    /// Liegt die Position außerhalb des T-Punkt-Rahmens zuzüglich `puffer_prozent` der Ausdehnung?
    pub fn ausserhalb(&self, e: f64, n: f64, puffer_prozent: f64) -> bool {
        let (min_e, min_n, max_e, max_n) = self.rahmen;
        let puffer = (max_e - min_e).max(max_n - min_n) * puffer_prozent / 100.0;
        e < min_e - puffer || e > max_e + puffer || n < min_n - puffer || n > max_n + puffer
    }
}

/// Eine Einpassung mit ihren T-Punkten.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Einpassung {
    pub id: String,
    pub methode: EinpassMethode,
    pub hoehe_modus: HoehenEinpassung,
    /// Höhenbezug der GNSS-Höhen bei der Messung der T-Punkte
    pub hoehenbezug: String,
    pub punkte: Vec<TPunkt>,
    /// Zeitpunkt der letzten Übernahme
    #[serde(default)]
    pub uebernommen: String,
    #[serde(skip)]
    pub ergebnis: Option<EinpassErgebnis>,
    #[serde(skip)]
    pub fehler: Option<String>,
}

impl Einpassung {
    pub fn neu(id: String, methode: EinpassMethode, hoehe_modus: HoehenEinpassung, hoehenbezug: &str) -> Self {
        Self { id, methode, hoehe_modus, hoehenbezug: hoehenbezug.to_string(), punkte: Vec::new(), uebernommen: String::new(), ergebnis: None, fehler: None }
    }

    /// Fügt einen T-Punkt hinzu oder mittelt ihn mit einer früheren Belegung derselben Nummer.
    /// Liefert bei einer weiteren Belegung die Differenzen (Lage, Höhe) [m].
    pub fn setze_punkt(&mut self, p: TPunkt) -> Option<(f64, Option<f64>)> {
        match self.punkte.iter_mut().find(|q| q.nr.eq_ignore_ascii_case(&p.nr)) {
            Some(alt) => {
                alt.soll_r = p.soll_r;
                alt.soll_h = p.soll_h;
                alt.soll_hoehe = p.soll_hoehe;
                Some(alt.mittle(&p))
            }
            None => {
                self.punkte.push(p);
                None
            }
        }
    }

    /// Rechnet die Einpassung neu; Fehler stehen in `fehler`.
    pub fn berechne(&mut self, utm_k: Option<f64>) {
        match berechne(self, utm_k) {
            Ok(e) => {
                self.ergebnis = Some(e);
                self.fehler = None;
            }
            Err(f) => {
                self.ergebnis = None;
                self.fehler = Some(f);
            }
        }
    }

    pub fn aktive_lage(&self) -> usize {
        self.punkte.iter().filter(|p| p.lage_aktiv).count()
    }
}

fn berechne(ep: &Einpassung, utm_k: Option<f64>) -> Result<EinpassErgebnis, String> {
    let mut erg = berechne_ohne_probe(ep, utm_k)?;
    if let Some(text) = grober_fehler(ep, utm_k) {
        erg.hinweise.insert(0, text);
    }
    Ok(erg)
}

/// Berechnung ohne die Probe auf grobe Fehler.
fn berechne_ohne_probe(ep: &Einpassung, utm_k: Option<f64>) -> Result<EinpassErgebnis, String> {
    let lage: Vec<&TPunkt> = ep.punkte.iter().filter(|p| p.lage_aktiv).collect();
    let min = ep.methode.min_punkte();
    if lage.len() < min {
        return Err(format!("{}: mindestens {min} aktive T-Punkte erforderlich ({} vorhanden).", ep.methode.anzeige(), lage.len()));
    }
    let ist_3d = ep.methode.ist_3d();
    if ist_3d && lage.iter().any(|p| p.soll_hoehe.is_none() || p.ist_hoehe.is_none()) {
        return Err("3D-Methoden brauchen Soll- und GNSS-Höhe an allen aktiven T-Punkten.".into());
    }
    let anzahl = lage.len() as f64;
    let mittel = |f: &dyn Fn(&TPunkt) -> f64| lage.iter().map(|p| f(p)).sum::<f64>() / anzahl;
    let hoehe_ist = |p: &TPunkt| p.ist_hoehe.unwrap_or(0.0);
    let hoehe_soll = |p: &TPunkt| p.soll_hoehe.unwrap_or(0.0);
    let quelle_sp = (mittel(&|p| p.ist_e), mittel(&|p| p.ist_n), if ist_3d { mittel(&hoehe_ist) } else { 0.0 });
    let ziel_sp = (mittel(&|p| p.soll_r), mittel(&|p| p.soll_h), if ist_3d { mittel(&hoehe_soll) } else { 0.0 });
    let massstab_vorgabe = match (ep.methode.fester_massstab(), utm_k) {
        (true, Some(k)) => gelaende_massstab(k, if ist_3d { quelle_sp.2 } else { mittel(&hoehe_ist) }),
        _ => 1.0,
    };
    let quelle: Vec<TransformPunkt> = lage
        .iter()
        .map(|p| {
            let mut t = TransformPunkt::neu(&p.nr, (p.ist_e - quelle_sp.0) * massstab_vorgabe, (p.ist_n - quelle_sp.1) * massstab_vorgabe, hoehe_ist(p) - quelle_sp.2);
            t.use_z = p.hoehe_aktiv;
            t
        })
        .collect();
    let ziel: Vec<TransformPunkt> = lage
        .iter()
        .map(|p| {
            let mut t = TransformPunkt::neu(&p.nr, p.soll_r - ziel_sp.0, p.soll_h - ziel_sp.1, hoehe_soll(p) - ziel_sp.2);
            t.use_z = p.hoehe_aktiv;
            t
        })
        .collect();
    let typ = ep.methode.typ();
    let (kt_erg, verschiebung, s0_lage) = if ep.methode == EinpassMethode::Verschiebung {
        let dx = ziel.iter().zip(&quelle).map(|(z, q)| z.x - q.x).sum::<f64>() / anzahl;
        let dy = ziel.iter().zip(&quelle).map(|(z, q)| z.y - q.y).sum::<f64>() / anzahl;
        let vtv: f64 = ziel.iter().zip(&quelle).map(|(z, q)| ((q.x + dx - z.x) * MM_PRO_M).powi(2) + ((q.y + dy - z.y) * MM_PRO_M).powi(2)).sum();
        let redundanz = 2 * lage.len() - 2;
        let s0 = if redundanz > 0 { (vtv / redundanz as f64).sqrt() } else { 0.0 };
        (None, (dx, dy), s0)
    } else {
        let e = kt::berechnen(typ, &quelle, &ziel, ep.methode.fester_massstab())?;
        let s0 = e.s0_mm;
        (Some(e), (0.0, 0.0), s0)
    };
    let lage_erg = LageErgebnis { kt: kt_erg, typ, fest: ep.methode.fester_massstab(), quelle_sp, ziel_sp, massstab_vorgabe, verschiebung, s0_mm: s0_lage };

    // Höhe getrennt (2D-Methoden)
    let hoehe = match (ist_3d, ep.hoehe_modus.modell()) {
        (false, Some(modell)) => {
            let punkte: Vec<HoehenPunkt> = ep
                .punkte
                .iter()
                .filter(|p| p.hoehe_aktiv)
                .filter_map(|p| Some(HoehenPunkt { nr: p.nr.clone(), e: p.ist_e, n: p.ist_n, ist: p.ist_hoehe?, soll: p.soll_hoehe? }))
                .collect();
            if punkte.len() >= modell.min_punkte() { Some(he::berechne(modell, &punkte)?) } else { None }
        }
        _ => None,
    };
    let mut erg = EinpassErgebnis { lage: lage_erg, hoehe, residuen: Vec::new(), hinweise: Vec::new(), rahmen: (0.0, 0.0, 0.0, 0.0) };
    let (mut min_e, mut min_n, mut max_e, mut max_n) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for p in &lage {
        min_e = min_e.min(p.ist_e);
        min_n = min_n.min(p.ist_n);
        max_e = max_e.max(p.ist_e);
        max_n = max_n.max(p.ist_n);
    }
    erg.rahmen = (min_e, min_n, max_e, max_n);
    // Residuen aller T-Punkte (auch inaktiver, zur Kontrolle)
    erg.residuen = ep
        .punkte
        .iter()
        .map(|p| {
            let (r, h, hoehe) = erg.transformiere(p.ist_e, p.ist_n, p.ist_hoehe);
            let hoehen_bestimmt = ist_3d || erg.hoehe.is_some();
            Residuum {
                nr: p.nr.clone(),
                v_r: Some((r - p.soll_r) * MM_PRO_M),
                v_h: Some((h - p.soll_h) * MM_PRO_M),
                v_hoehe: if hoehen_bestimmt { hoehe.zip(p.soll_hoehe).map(|(a, b)| (a - b) * MM_PRO_M) } else { None },
            }
        })
        .collect();
    erg.hinweise = hinweise(&erg, &lage);
    Ok(erg)
}

/// Probe auf einen groben Fehler: jeden aktiven T-Punkt einmal weglassen und seine Abweichung gegen die
/// Einpassung der übrigen prüfen; gemeldet wird der auffälligste Punkt.
fn grober_fehler(ep: &Einpassung, utm_k: Option<f64>) -> Option<String> {
    let aktive: Vec<usize> = ep.punkte.iter().enumerate().filter(|(_, p)| p.lage_aktiv).map(|(i, _)| i).collect();
    // ohne den Punkt muss noch eine Überbestimmung bleiben
    if aktive.len() < ep.methode.min_punkte() + 2 {
        return None;
    }
    let mut schlechtester: Option<(f64, String, f64)> = None;
    for &i in &aktive {
        let mut probe = ep.clone();
        probe.punkte[i].lage_aktiv = false;
        probe.hoehe_modus = HoehenEinpassung::Keine;
        let Ok(erg) = berechne_ohne_probe(&probe, utm_k) else { continue };
        let p = &ep.punkte[i];
        let (r, h, _) = erg.transformiere(p.ist_e, p.ist_n, p.ist_hoehe);
        let abweichung = (r - p.soll_r).hypot(h - p.soll_h) * MM_PRO_M;
        let grenze = (GROBER_FEHLER_FAKTOR * erg.lage.s0_mm).max(GROBER_FEHLER_MIN_MM);
        let verhaeltnis = abweichung / grenze;
        if verhaeltnis > 1.0 && schlechtester.as_ref().is_none_or(|s| verhaeltnis > s.0) {
            schlechtester = Some((verhaeltnis, p.nr.clone(), abweichung));
        }
    }
    schlechtester.map(|(_, nr, v)| {
        format!("T-Punkt {nr}: weicht ohne ihn gerechnet um {v:.0} mm ab – möglicher grober Fehler, Punkt prüfen oder deaktivieren.")
    })
}

/// Prüft Geometrie und Maßstab.
fn hinweise(erg: &EinpassErgebnis, lage: &[&TPunkt]) -> Vec<String> {
    let mut h = Vec::new();
    if lage.len() >= 2 {
        // Hauptachsen der Punktverteilung
        let (me, mn) = (erg.lage.quelle_sp.0, erg.lage.quelle_sp.1);
        let (mut see, mut snn, mut sen) = (0.0, 0.0, 0.0);
        for p in lage {
            let (de, dn) = (p.ist_e - me, p.ist_n - mn);
            see += de * de;
            snn += dn * dn;
            sen += de * dn;
        }
        let spur = see + snn;
        let det = see * snn - sen * sen;
        let wurzel = (spur * spur / 4.0 - det).max(0.0).sqrt();
        let (gross, klein) = (spur / 2.0 + wurzel, (spur / 2.0 - wurzel).max(0.0));
        if lage.len() >= 3 && gross > 0.0 && (klein / gross).sqrt() < LINIEN_VERHAELTNIS {
            h.push("T-Punkte liegen fast auf einer Linie – quer dazu ist die Einpassung schwach bestimmt.".into());
        }
    }
    if !erg.lage.fest && erg.lage.kt.is_some() {
        let ppm = (erg.lage.massstab() - 1.0) * PPM;
        h.push(format!("Maßstab UTM → Plan: {ppm:+.0} ppm"));
    }
    h
}

/// Alle Einpassungen eines Projekts und die aktive.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct EinpassungsDatei {
    pub aktiv: Option<String>,
    pub einpassungen: Vec<Einpassung>,
}

impl EinpassungsDatei {
    pub fn pfad(e: &Einstellungen) -> PathBuf {
        e.get_pfad(DATEINAME)
    }

    /// Lädt die Datei und rechnet alle Einpassungen; fehlt sie, ist die Liste leer.
    pub fn lade(pfad: &Path, utm_k: Option<f64>) -> Result<Self, String> {
        let text = match std::fs::read_to_string(pfad) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(e) => return Err(format!("{} nicht lesbar: {e}", pfad.display())),
        };
        let mut d: Self = serde_json::from_str(&text).map_err(|e| format!("{} fehlerhaft: {e}", pfad.display()))?;
        for ep in &mut d.einpassungen {
            ep.berechne(utm_k);
        }
        Ok(d)
    }

    pub fn speichere(&self, pfad: &Path) -> Result<(), String> {
        let text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        datei_speicher::schreibe_text(pfad, &text).map_err(|e| format!("{} nicht schreibbar: {e}", pfad.display()))
    }

    pub fn aktive(&self) -> Option<&Einpassung> {
        let id = self.aktiv.as_ref()?;
        self.einpassungen.iter().find(|e| &e.id == id)
    }

    /// Nächste freie Id „E-JJJJ-MM-TT-n".
    pub fn neue_id(&self, datum: &str) -> String {
        (1..)
            .map(|n| format!("E-{datum}-{n}"))
            .find(|id| !self.einpassungen.iter().any(|e| &e.id == id))
            .unwrap_or_default()
    }

    /// Ersetzt oder ergänzt eine Einpassung.
    pub fn setze(&mut self, ep: Einpassung) {
        match self.einpassungen.iter_mut().find(|e| e.id == ep.id) {
            Some(alt) => *alt = ep,
            None => self.einpassungen.push(ep),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Erzeugt T-Punkte aus einer bekannten Ähnlichkeitstransformation UTM → Plan.
    fn punkte(m: f64, alpha: f64, t: (f64, f64), dh: f64) -> Vec<TPunkt> {
        let (c, s) = (alpha.cos(), alpha.sin());
        [(0.0, 0.0), (120.0, 10.0), (60.0, 140.0), (-30.0, 80.0)]
            .iter()
            .enumerate()
            .map(|(i, &(de, dn))| {
                let (e, n) = (690_000.0 + de, 5_430_000.0 + dn);
                let hoehe = 400.0 + de * 0.01;
                TPunkt {
                    nr: format!("T{i}"),
                    soll_r: m * (c * e - s * n) + t.0,
                    soll_h: m * (s * e + c * n) + t.1,
                    soll_hoehe: Some(hoehe + dh),
                    ist_e: e,
                    ist_n: n,
                    ist_hoehe: Some(hoehe),
                    s_lage_mm: 5.0,
                    s_hoehe_mm: 8.0,
                    epochen: 30,
                    belegungen: 1,
                    lage_aktiv: true,
                    hoehe_aktiv: true,
                    zeit: String::new(),
                }
            })
            .collect()
    }

    #[test]
    fn helmert_2d_mit_hoehenverschiebung_ist_exakt() {
        let mut ep = Einpassung::neu("E".into(), EinpassMethode::Helmert2D, HoehenEinpassung::Verschiebung, "DHHN2016");
        ep.punkte = punkte(1.000_35, 0.0123, (-5_000_000.0, 100_000.0), -0.25);
        ep.berechne(None);
        let erg = ep.ergebnis.as_ref().expect("Ergebnis");
        assert!(erg.lage.s0_mm < 0.01, "s0 {}", erg.lage.s0_mm);
        for r in &erg.residuen {
            assert!(r.v_lage().unwrap() < 0.01 && r.v_hoehe.unwrap().abs() < 0.01);
        }
        // ein Punkt außerhalb der T-Punkte wird genauso abgebildet
        let (e, n) = (690_050.0, 5_430_050.0);
        let (r, h, hoehe) = erg.transformiere(e, n, Some(401.0));
        let (c, s) = (0.0123f64.cos(), 0.0123f64.sin());
        assert!((r - (1.000_35 * (c * e - s * n) - 5_000_000.0)).abs() < 1e-5);
        assert!((h - (1.000_35 * (s * e + c * n) + 100_000.0)).abs() < 1e-5);
        assert!((hoehe.unwrap() - 400.75).abs() < 1e-6);
        assert!((erg.lage.massstab() - 1.000_35).abs() < 1e-9);
    }

    #[test]
    fn verschiebung_mit_einem_punkt() {
        let mut ep = Einpassung::neu("E".into(), EinpassMethode::Verschiebung, HoehenEinpassung::Verschiebung, "");
        ep.punkte = punkte(1.0, 0.0, (1.5, -2.0), 0.1).into_iter().take(1).collect();
        ep.berechne(None);
        let erg = ep.ergebnis.as_ref().expect("Ergebnis");
        let (r, h, hoehe) = erg.transformiere(690_010.0, 5_430_010.0, Some(400.0));
        assert!((r - 690_011.5).abs() < 1e-9 && (h - 5_430_008.0).abs() < 1e-9);
        assert!((hoehe.unwrap() - 400.1).abs() < 1e-9);
    }

    #[test]
    fn fester_massstab_nutzt_utm_verzerrung() {
        // Plan in Geländemaß: Soll = UTM · m mit m aus UTM-Maßstab und Höhe
        let k = 0.9998;
        let m = gelaende_massstab(k, 400.0);
        let mut ep = Einpassung::neu("E".into(), EinpassMethode::Helmert2DFest, HoehenEinpassung::Keine, "");
        ep.punkte = punkte(m, 0.0, (0.0, 0.0), 0.0);
        for p in &mut ep.punkte {
            p.ist_hoehe = Some(400.0);
        }
        ep.berechne(Some(k));
        let erg = ep.ergebnis.as_ref().expect("Ergebnis");
        assert!(erg.lage.s0_mm < 0.05, "s0 {}", erg.lage.s0_mm);
        // ohne die Vorgabe entstünden Restfehler von mehreren Zentimetern
        ep.berechne(None);
        assert!(ep.ergebnis.as_ref().unwrap().lage.s0_mm > 10.0);
    }

    #[test]
    fn zu_wenige_punkte_und_grober_fehler() {
        let mut ep = Einpassung::neu("E".into(), EinpassMethode::Helmert2D, HoehenEinpassung::Keine, "");
        ep.punkte = punkte(1.0, 0.0, (0.0, 0.0), 0.0).into_iter().take(1).collect();
        ep.berechne(None);
        assert!(ep.ergebnis.is_none() && ep.fehler.is_some());
        let mut ep = Einpassung::neu("E".into(), EinpassMethode::Helmert2D, HoehenEinpassung::Keine, "");
        ep.punkte = punkte(1.0, 0.0, (0.0, 0.0), 0.0);
        ep.punkte.push(TPunkt { nr: "T9".into(), ist_e: 690_040.0, ist_n: 5_430_040.0, soll_r: 690_040.0, soll_h: 5_430_040.0, ..ep.punkte[0].clone() });
        ep.punkte[1].soll_r += 0.30;
        ep.berechne(None);
        let erg = ep.ergebnis.as_ref().unwrap();
        assert!(erg.hinweise.iter().any(|h| h.contains("T1")), "{:?}", erg.hinweise);
        assert!(!erg.hinweise.iter().any(|h| h.contains("T0") || h.contains("T2")));
    }

    #[test]
    fn mehrfachbelegung_wird_gemittelt() {
        let mut ep = Einpassung::neu("E".into(), EinpassMethode::Helmert2D, HoehenEinpassung::Keine, "");
        let p = punkte(1.0, 0.0, (0.0, 0.0), 0.0).remove(0);
        assert!(ep.setze_punkt(p.clone()).is_none());
        let zweite = TPunkt { ist_e: p.ist_e + 0.02, ..p };
        let (d, _) = ep.setze_punkt(zweite).unwrap();
        assert!((d - 0.02).abs() < 1e-9);
        assert_eq!(ep.punkte.len(), 1);
        assert_eq!(ep.punkte[0].belegungen, 2);
        assert!((ep.punkte[0].ist_e - 690_000.01).abs() < 1e-9);
    }

    #[test]
    fn rahmen_und_extrapolation() {
        let mut ep = Einpassung::neu("E".into(), EinpassMethode::Helmert2D, HoehenEinpassung::Keine, "");
        ep.punkte = punkte(1.0, 0.0, (0.0, 0.0), 0.0);
        ep.berechne(None);
        let erg = ep.ergebnis.as_ref().unwrap();
        assert!(!erg.ausserhalb(690_050.0, 5_430_050.0, 20.0));
        assert!(erg.ausserhalb(690_500.0, 5_430_050.0, 20.0));
    }

    #[test]
    fn namen_rundlauf() {
        for m in EinpassMethode::ALLE {
            assert_eq!(EinpassMethode::aus_name(m.name()), Some(m));
        }
        for h in HoehenEinpassung::ALLE {
            assert_eq!(HoehenEinpassung::aus_name(h.name()), Some(h));
        }
        for b in GnssBezug::ALLE {
            assert_eq!(GnssBezug::aus_name(b.name()), Some(b));
        }
    }
}
