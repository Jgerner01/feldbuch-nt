// Zweck:    Konvertierung von Punktdaten: Import aus GSI (Leica GSI-8/-16), DXF, KOR und DAT, Duplikaterkennung,
//           Nummernvergabe sowie Export als CSV, KOR und DAT
// Hardware: keine
// Autor:    Johann Gerner

use std::fs;
use std::path::Path;

use crate::dxf::leser::{self, RohEntity};
use crate::grundlagen::format::{f, parse_zahl};

/// Duplikat-Toleranz für Koordinaten [m].
pub const DUPLIKAT_EPS_M: f64 = 0.001;
/// Typ des Standpunkts für den DAT-Export.
const TYP_STANDPUNKT: &str = "Standpunkt";
/// Vorgaben des DAT-Exports ohne Standpunkt.
const STANDARD_STANDPUNKT: &str = "STP1";
/// Standardcode der KOR-/DAT-Exporte (Punkte ohne eigenen Code und Standpunktzeile).
pub const STANDARD_CODE: &str = "005";
/// Mindestlänge eines GSI-Blocks (GSI-8: WI 2 + Info 4 + Vorzeichen 1 + Daten 8).
const GSI_MIN_BLOCKLAENGE: usize = 15;
/// VERTEX-Flags (Gruppencode 70): Netzpunkt eines Polyface-Netzes bzw. Flächenverweis.
const VERTEX_NETZPUNKT: i32 = 64;
const VERTEX_FLAECHE: i32 = 128;
/// Blocklänge, ab der GSI-16 vorliegt (wird nur zur Unterscheidung benötigt).
const GSI_16_BLOCKLAENGE: usize = 23;

/// Ein Punkt der Konvertierungstabelle.
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
    /// Vorhandene Werte: R, H, Höhe, Hz, V, Schrägstrecke, Zielhöhe.
    pub vorhanden: [bool; 7],
}

// ── GSI ──────────────────────────────────────────────────────────────────────

/// Liest eine GSI-Datei (Latin-1).
pub fn gsi_lese(pfad: &Path) -> std::io::Result<Vec<KonvertierungPunkt>> {
    let text: String = fs::read(pfad)?.iter().map(|b| char::from(*b)).collect();
    Ok(gsi_parse(&text))
}

/// Parst den Text einer GSI-Datei.
pub fn gsi_parse(text: &str) -> Vec<KonvertierungPunkt> {
    text.lines().map(str::trim).filter(|z| !z.is_empty()).filter_map(gsi_zeile).collect()
}

fn dekodiere(roh: i64, einheit: char) -> f64 {
    let r = roh as f64;
    match einheit {
        '2' | '3' | '5' | '8' => r / 100_000.0,
        '4' => dekodiere_dms(roh),
        '6' | '7' => r / 10_000.0,
        _ => r / 1000.0,
    }
}

/// DMS-Format DDMMSS.SSSSS → Dezimalgrad.
fn dekodiere_dms(roh: i64) -> f64 {
    let v = roh as f64 / 100_000.0;
    let sekunden = v % 100.0;
    let rest = (v / 100.0) as i64;
    let minuten = (rest % 100) as f64;
    let grad = (rest / 100) as f64;
    grad + minuten / 60.0 + sekunden / 3600.0
}

fn gsi_zeile(zeile: &str) -> Option<KonvertierungPunkt> {
    let zeile = zeile.strip_prefix('*').unwrap_or(zeile);
    let tokens: Vec<Vec<char>> = zeile.split_whitespace().map(|t| t.chars().collect()).collect();
    if tokens.is_empty() {
        return None;
    }
    let _gsi16 = tokens.iter().any(|t| t.len() >= GSI_16_BLOCKLAENGE);
    let mut p = KonvertierungPunkt::default();
    let mut bemerkung: Vec<String> = Vec::new();
    let mut hat_horizontalstrecke = false;
    for token in &tokens {
        if token.len() < GSI_MIN_BLOCKLAENGE {
            continue;
        }
        let wi_text: String = token[..2].iter().collect();
        let Ok(wi) = wi_text.parse::<i32>() else {
            continue;
        };
        let einheit = token[5];
        let vorzeichen = token[6];
        let daten: String = token[7..].iter().collect();

        if wi == 11 {
            let roh = daten.trim_end();
            p.punkt_nr = match roh.parse::<i64>() {
                Ok(n) => n.to_string(),
                Err(_) => roh.trim_start_matches('0').to_string(),
            };
            continue;
        }
        let Ok(mut roh) = daten.trim().parse::<i64>() else {
            continue;
        };
        if vorzeichen == '-' {
            roh = -roh;
        }
        let wert = dekodiere(roh, einheit);
        if let Some(i) = match wi {
            81 => Some(0),
            82 => Some(1),
            83 => Some(2),
            21 => Some(3),
            22 => Some(4),
            31 => Some(5),
            87 => Some(6),
            _ => None,
        } {
            p.vorhanden[i] = true;
        }
        match wi {
            21 => p.hz = wert,
            22 => p.v = wert,
            31 => p.strecke = wert,
            // Horizontalstrecke: die Spalte „Strecke“ ist die Schrägstrecke, daher nur als Bemerkung
            32 => {
                hat_horizontalstrecke = true;
                bemerkung.push(format!("HD={}m", f(wert, 3)));
            }
            33 => bemerkung.push(format!("Δh={}m", f(wert, 3))),
            81 => p.r = wert,
            82 => p.h = wert,
            83 => p.hoehe = wert,
            84 => bemerkung.push(format!("STP-R={}m", f(wert, 3))),
            85 => bemerkung.push(format!("STP-H={}m", f(wert, 3))),
            86 => bemerkung.push(format!("STP-Hoehe={}m", f(wert, 3))),
            87 => p.zielhoehe = wert,
            88 => bemerkung.push(format!("Inst-h={}m", f(wert, 3))),
            41..=49 => {
                let code = daten.trim_end().trim_start_matches('0').to_string();
                if wi == 41 && p.punktcode.is_empty() {
                    p.punktcode = code;
                } else if !code.is_empty() {
                    bemerkung.push(format!("WI{wi}={code}"));
                }
            }
            71..=79 => {
                let attribut = daten.trim_end().trim_start_matches('0');
                if !attribut.is_empty() {
                    bemerkung.push(format!("Attr{wi}={attribut}"));
                }
            }
            _ => {}
        }
    }
    p.bemerkung = bemerkung.join(", ");
    p.typ = "Import-GSI".to_string();
    let hat_koordinaten = p.r != 0.0 || p.h != 0.0;
    let hat_messung = p.hz != 0.0 || p.v != 0.0 || p.strecke != 0.0 || hat_horizontalstrecke;
    if !hat_koordinaten && !hat_messung && p.punkt_nr.is_empty() {
        return None;
    }
    Some(p)
}

// ── KOR / DAT ────────────────────────────────────────────────────────────────

/// Kopfzeilen, an denen die eigenen KOR-/DAT-Exporte (feste Spalten) erkannt werden.
const KOPF_KOR: &str = "Koordinatenliste – Feldbuch Export";
const KOPF_DAT: &str = "Feldbuch Export – DAT Format";
/// Feste Spalten der eigenen Exporte (Zeichenpositionen): Nr, R, H, Höhe, Code, Rest (Bemerkung bzw. Typ).
const SPALTE_ENDE_NR: usize = 16;
const SPALTE_ENDE_R: usize = 32;
const SPALTE_ENDE_H: usize = 48;
const SPALTE_ENDE_HOEHE: usize = 62;
const SPALTE_ANFANG_CODE: usize = 64;
const SPALTE_ENDE_CODE: usize = 72;
const SPALTE_ANFANG_REST: usize = 74;
/// Mindestanzahl Felder einer Messzeile der Messdaten-DAT (Nr, Hz, V, Strecke, Zielhöhe).
const MIN_FELDER_DAT_MESSUNG: usize = 5;

/// Text einer älteren Textdatei: UTF-8 (mit oder ohne BOM), sonst Latin-1.
fn lese_text(pfad: &Path) -> std::io::Result<String> {
    let bytes = fs::read(pfad)?;
    let ohne_bom = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(&bytes);
    Ok(match std::str::from_utf8(ohne_bom) {
        Ok(t) => t.to_owned(),
        Err(_) => ohne_bom.iter().map(|b| char::from(*b)).collect(),
    })
}

fn ist_kommentar(zeile: &str) -> bool {
    zeile.is_empty() || zeile.starts_with(['%', '#', ';'])
}

/// Koordinatenzeile: (Nr, R, H, Höhe, Code, Rest). Mit `fest` zuerst nach den festen Spalten der eigenen Exporte
/// (leerer Code bleibt so erkennbar), sonst leerzeichengetrennt `Nr R H [Höhe] [Code] [Rest …]`.
fn koordinatenzeile(zeile: &str, fest: bool) -> Option<(String, f64, f64, Option<f64>, String, String)> {
    if fest {
        let z: Vec<char> = zeile.chars().collect();
        let teil = |a: usize, e: usize| z.get(a.min(z.len())..e.min(z.len())).map(|s| s.iter().collect::<String>().trim().to_string()).unwrap_or_default();
        let nr = teil(0, SPALTE_ENDE_NR);
        if let (false, Some(r), Some(h), Some(hoehe)) = (
            nr.is_empty() || nr.contains(' '),
            parse_zahl(&teil(SPALTE_ENDE_NR, SPALTE_ENDE_R)),
            parse_zahl(&teil(SPALTE_ENDE_R, SPALTE_ENDE_H)),
            parse_zahl(&teil(SPALTE_ENDE_H, SPALTE_ENDE_HOEHE)),
        ) {
            return Some((nr, r, h, Some(hoehe), teil(SPALTE_ANFANG_CODE, SPALTE_ENDE_CODE), teil(SPALTE_ANFANG_REST, usize::MAX)));
        }
    }
    let t: Vec<&str> = zeile.split_whitespace().collect();
    let (r, h) = (parse_zahl(t.get(1)?)?, parse_zahl(t.get(2)?)?);
    let hoehe = t.get(3).and_then(|s| parse_zahl(s));
    let weiter = if hoehe.is_some() { 4 } else { 3 };
    let code = t.get(weiter).map_or_else(String::new, |s| (*s).to_string());
    let rest = t.get(weiter + 1..).map_or_else(String::new, |s| s.join(" "));
    Some((t[0].to_string(), r, h, hoehe, code, rest))
}

fn koordinatenpunkt(nr: String, r: f64, h: f64, hoehe: Option<f64>, code: String, typ: String, bemerkung: String) -> KonvertierungPunkt {
    KonvertierungPunkt {
        punkt_nr: nr,
        typ,
        r,
        h,
        hoehe: hoehe.unwrap_or(0.0),
        punktcode: code,
        bemerkung,
        vorhanden: [true, true, hoehe.is_some(), false, false, false, false],
        ..KonvertierungPunkt::default()
    }
}

/// Liest eine KOR-Koordinatendatei.
pub fn kor_lese(pfad: &Path) -> std::io::Result<Vec<KonvertierungPunkt>> {
    Ok(kor_parse(&lese_text(pfad)?))
}

/// KOR: `Nr R H [Höhe] [Code] [Bemerkung]`; Kommentarzeilen (`%`, `#`, `;`) werden übersprungen.
pub fn kor_parse(text: &str) -> Vec<KonvertierungPunkt> {
    let fest = text.contains(KOPF_KOR);
    text.lines()
        .filter(|z| !ist_kommentar(z.trim()))
        .filter_map(|z| koordinatenzeile(z.trim_end(), fest))
        .map(|(nr, r, h, hoehe, code, rest)| koordinatenpunkt(nr, r, h, hoehe, code, "Import-KOR".into(), rest))
        .collect()
}

/// Liest eine DAT-Datei; erkennt die Koordinaten-Variante (Export der Konvertierung) und die Messdaten-Variante.
pub fn dat_lese(pfad: &Path) -> std::io::Result<Vec<KonvertierungPunkt>> {
    Ok(dat_parse(&lese_text(pfad)?))
}

/// Ist die DAT-Datei die Koordinaten-Variante (`STANDPUNKT …` und Koordinatenzeilen)?
pub fn ist_koordinaten_dat(text: &str) -> bool {
    text.contains(KOPF_DAT) || text.lines().any(|z| z.trim_start().starts_with("STANDPUNKT "))
}

/// DAT, Koordinaten-Variante: `Nr R H Höhe Code Typ`; Messdaten-Variante: Standpunktzeile `-Nr IH [Code]`, danach
/// `Nr Hz V Strecke Zielhöhe [Code]`.
pub fn dat_parse(text: &str) -> Vec<KonvertierungPunkt> {
    if ist_koordinaten_dat(text) {
        let fest = text.contains(KOPF_DAT);
        // Der in der Standpunktzeile genannte Punkt wird als Standpunkt übernommen
        let standpunkt_nr = text
            .lines()
            .find_map(|z| z.trim_start().strip_prefix("STANDPUNKT "))
            .and_then(|rest| rest.split_whitespace().next())
            .unwrap_or("")
            .to_string();
        return text
            .lines()
            .map(str::trim_end)
            .filter(|z| !ist_kommentar(z.trim()) && !z.trim_start().starts_with("STANDPUNKT "))
            .filter_map(|z| koordinatenzeile(z, fest))
            .map(|(nr, r, h, hoehe, code, typ)| {
                let typ = if !standpunkt_nr.is_empty() && nr == standpunkt_nr {
                    TYP_STANDPUNKT.to_string()
                } else if typ.is_empty() {
                    "Import-DAT".to_string()
                } else {
                    typ
                };
                koordinatenpunkt(nr, r, h, hoehe, code, typ, String::new())
            })
            .collect();
    }
    let mut standpunkt = String::new();
    let mut ergebnis = Vec::new();
    for zeile in text.lines().map(str::trim).filter(|z| !ist_kommentar(z)) {
        if let Some(sp) = zeile.strip_prefix('-') {
            // Standpunktzeile (auch mehrere Aufstellungen in einer Datei)
            let t: Vec<&str> = sp.split_whitespace().collect();
            standpunkt = match (t.first(), t.get(1)) {
                (Some(nr), Some(ih)) => format!("STP={nr}, IH={ih}m"),
                (Some(nr), None) => format!("STP={nr}"),
                _ => String::new(),
            };
            continue;
        }
        let t: Vec<&str> = zeile.split_whitespace().collect();
        if t.len() < MIN_FELDER_DAT_MESSUNG {
            continue;
        }
        let (Some(hz), Some(v), Some(strecke), Some(zielhoehe)) = (parse_zahl(t[1]), parse_zahl(t[2]), parse_zahl(t[3]), parse_zahl(t[4])) else {
            continue;
        };
        ergebnis.push(KonvertierungPunkt {
            punkt_nr: t[0].to_string(),
            typ: "Import-DAT".into(),
            hz,
            v,
            strecke,
            zielhoehe,
            punktcode: t.get(5).map_or_else(String::new, |s| (*s).to_string()),
            bemerkung: standpunkt.clone(),
            vorhanden: [false, false, false, true, true, true, true],
            ..KonvertierungPunkt::default()
        });
    }
    ergebnis
}

// ── DXF ──────────────────────────────────────────────────────────────────────

/// Extrahiert Koordinaten aus einer DXF-Datei: Linienendpunkte, Eckpunkte von Polylinien (LWPOLYLINE und
/// 3D-POLYLINE/VERTEX), Symbole, Punkte und Kreismittelpunkte. Höhen (Gruppencode 30/31, bei LWPOLYLINE die
/// Erhebung 38) werden übernommen, wenn sie von 0 verschieden sind – 2D-Zeichnungen schreiben dort meist 0.
pub fn dxf_import(pfad: &Path) -> Result<Vec<KonvertierungPunkt>, String> {
    Ok(dxf_punkte(&leser::lese_roh(pfad)?))
}

fn dxf_punkte(entities: &[RohEntity]) -> Vec<KonvertierungPunkt> {
    let mut ergebnis = Vec::new();
    let punkt = |typ: &str, r: f64, h: f64, z: Option<f64>, code: &str, bem: String| KonvertierungPunkt {
        vorhanden: [true, true, z.is_some(), false, false, false, false],
        typ: typ.to_string(),
        r,
        h,
        hoehe: z.unwrap_or(0.0),
        punktcode: code.to_string(),
        bemerkung: bem,
        ..KonvertierungPunkt::default()
    };
    // Höhe nur, wenn vorhanden und nicht 0
    let hoehe = |e: &RohEntity, code: i32| e.zahl(code).filter(|z| *z != 0.0 && z.is_finite());
    let xy = |e: &RohEntity, cx: i32, cy: i32| Some((e.zahl(cx)?, e.zahl(cy)?));
    for e in entities {
        let layer = e.layer();
        match e.typ.as_str() {
            "LINE" => {
                for (n, (cx, cy, cz)) in [(10, 20, 30), (11, 21, 31)].into_iter().enumerate() {
                    if let Some((x, y)) = xy(e, cx, cy) {
                        ergebnis.push(punkt("DXF-Linie", x, y, hoehe(e, cz), "", format!("Layer={layer}, Linienendpunkt {}", n + 1)));
                    }
                }
            }
            "LWPOLYLINE" => {
                let z = hoehe(e, 38);
                let mut vx: Option<f64> = None;
                let mut nr = 0;
                for (c, v) in &e.gruppen {
                    match *c {
                        10 => vx = v.trim().parse().ok(),
                        20 => {
                            if let (Some(x), Ok(y)) = (vx.take(), v.trim().parse::<f64>()) {
                                nr += 1;
                                ergebnis.push(punkt("DXF-Polylinie", x, y, z, "", format!("Layer={layer}, Eckpunkt {nr}")));
                            }
                        }
                        _ => {}
                    }
                }
            }
            "VERTEX" => {
                // Flächenverweise eines Polyface-Netzes (Flag 128 ohne 64) sind keine Punkte
                let flags = e.text(70).and_then(|t| t.trim().parse::<i32>().ok()).unwrap_or(0);
                if flags & VERTEX_FLAECHE != 0 && flags & VERTEX_NETZPUNKT == 0 {
                    continue;
                }
                if let Some((x, y)) = xy(e, 10, 20) {
                    ergebnis.push(punkt("DXF-Polylinie", x, y, hoehe(e, 30), "", format!("Layer={layer}, Eckpunkt")));
                }
            }
            "INSERT" => {
                if let Some((x, y)) = xy(e, 10, 20) {
                    let block = e.text(2).unwrap_or("");
                    ergebnis.push(punkt("DXF-Symbol", x, y, hoehe(e, 30), block, format!("Layer={layer}, Block={block}")));
                }
            }
            "POINT" => {
                if let Some((x, y)) = xy(e, 10, 20) {
                    ergebnis.push(punkt("DXF-Punkt", x, y, hoehe(e, 30), "", format!("Layer={layer}")));
                }
            }
            "CIRCLE" => {
                if let Some((x, y)) = xy(e, 10, 20) {
                    let radius = e.zahl(40).unwrap_or(0.0);
                    ergebnis.push(punkt("DXF-Kreis", x, y, hoehe(e, 30), "", format!("Layer={layer}, R={}m", f(radius, 3))));
                }
            }
            _ => {}
        }
    }
    ergebnis
}

// ── Tabellenlogik ────────────────────────────────────────────────────────────

/// Ist der neue Punkt schon vorhanden (gleiche Nummer oder gleiche Koordinate innerhalb der Toleranz)?
pub fn ist_doppelt(neu: &KonvertierungPunkt, vorhandene: &[KonvertierungPunkt]) -> bool {
    vorhandene.iter().any(|v| {
        let gleiche_nr = !neu.punkt_nr.is_empty() && !v.punkt_nr.is_empty() && v.punkt_nr == neu.punkt_nr;
        let gleiche_lage = (v.r - neu.r).abs() < DUPLIKAT_EPS_M && (v.h - neu.h).abs() < DUPLIKAT_EPS_M && (neu.r != 0.0 || neu.h != 0.0);
        gleiche_nr || gleiche_lage
    })
}

/// Behält die Nummer, wenn sie frei ist; sonst die nächste freie fortlaufende Ganzzahl.
pub fn naechste_punkt_nr(vorschlag: &str, vorhandene: &[KonvertierungPunkt]) -> String {
    let hoechste = vorhandene.iter().filter_map(|p| p.punkt_nr.parse::<i32>().ok()).max().unwrap_or(0).max(0);
    if !vorschlag.is_empty() && !vorhandene.iter().any(|p| p.punkt_nr == vorschlag) {
        return vorschlag.to_string();
    }
    (hoechste + 1).to_string()
}

/// Fügt importierte Punkte hinzu; liefert (hinzugefügt, doppelt).
pub fn importiere_in(tabelle: &mut Vec<KonvertierungPunkt>, neue: Vec<KonvertierungPunkt>) -> (usize, usize) {
    let (mut hinzugefuegt, mut doppelt) = (0, 0);
    for mut p in neue {
        if ist_doppelt(&p, tabelle) {
            doppelt += 1;
            continue;
        }
        p.punkt_nr = naechste_punkt_nr(&p.punkt_nr, tabelle);
        tabelle.push(p);
        hinzugefuegt += 1;
    }
    (hinzugefuegt, doppelt)
}

// ── Auswahl im DXF-Viewer ────────────────────────────────────────────────────

/// Gon → Radiant.
const GON_ZU_RAD: f64 = std::f64::consts::PI / 200.0;

/// Ein Tabellenpunkt, der im DXF-Viewer gewählt werden kann.
#[derive(Clone, Debug, PartialEq)]
pub struct AuswahlKandidat {
    /// Index der Tabellenzeile.
    pub zeile: usize,
    pub nr: String,
    pub r: f64,
    pub h: f64,
    /// Lage nur hilfsweise aus Messdaten (Standpunkt 0/0, Orientierung t = 0).
    pub hilfskoordinate: bool,
}

/// Auftrag an den DXF-Viewer: Kandidaten mit der aktuellen Markierung der Tabelle.
#[derive(Clone, Debug, PartialEq)]
pub struct AuswahlAnfrage {
    pub kandidaten: Vec<AuswahlKandidat>,
    /// Je Kandidat: in der Tabelle bereits markiert.
    pub vorauswahl: Vec<bool>,
    /// Zeilen ohne Lage (weder Koordinaten noch Richtung mit Strecke) – im Viewer nicht wählbar.
    pub ohne_lage: usize,
    /// Stand der Tabelle beim Öffnen; ändert sich die Tabelle inzwischen, wird die Auswahl verworfen.
    pub tabellen_version: u64,
}

/// Rückgabe des DXF-Viewers beim Schließen.
#[derive(Clone, Debug, PartialEq)]
pub struct AuswahlErgebnis {
    pub tabellen_version: u64,
    /// `false`: Auswahl verworfen.
    pub uebernehmen: bool,
    /// Je Kandidat: (Tabellenzeile, gewählt).
    pub zeilen: Vec<(usize, bool)>,
}

/// Lage einer Tabellenzeile für die Auswahl: Koordinaten, sonst hilfsweise aus Hz, V und Schrägstrecke mit dem
/// Standpunkt in 0/0 und t = 0 (R = e·sin Hz, H = e·cos Hz, e = s·sin V; ohne V gilt die Strecke als horizontal).
pub fn auswahl_lage(p: &KonvertierungPunkt) -> Option<(f64, f64, bool)> {
    if p.vorhanden[0] && p.vorhanden[1] {
        return Some((p.r, p.h, false));
    }
    if !(p.vorhanden[3] && p.vorhanden[5]) || p.strecke <= 0.0 {
        return None;
    }
    let e = if p.vorhanden[4] { p.strecke * (p.v * GON_ZU_RAD).sin() } else { p.strecke };
    let t = p.hz * GON_ZU_RAD;
    Some((e * t.sin(), e * t.cos(), true))
}

/// Baut den Auftrag an den DXF-Viewer aus Tabelleninhalt und Markierung.
pub fn auswahl_anfrage(punkte: &[KonvertierungPunkt], markiert: &[bool], tabellen_version: u64) -> AuswahlAnfrage {
    let mut kandidaten = Vec::new();
    let mut vorauswahl = Vec::new();
    for (zeile, p) in punkte.iter().enumerate() {
        if let Some((r, h, hilfskoordinate)) = auswahl_lage(p) {
            kandidaten.push(AuswahlKandidat { zeile, nr: p.punkt_nr.trim().to_string(), r, h, hilfskoordinate });
            vorauswahl.push(markiert.get(zeile).copied().unwrap_or(false));
        }
    }
    let ohne_lage = punkte.len() - kandidaten.len();
    AuswahlAnfrage { kandidaten, vorauswahl, ohne_lage, tabellen_version }
}

/// Liegt der Punkt im (automatisch geschlossenen) Polygon? Strahlverfahren; Punkte auf dem Rand zählen nicht sicher.
pub fn punkt_im_polygon(p: (f64, f64), polygon: &[(f64, f64)]) -> bool {
    if polygon.len() < 3 {
        return false;
    }
    let (x, y) = p;
    let mut innen = false;
    let mut j = polygon.len() - 1;
    for i in 0..polygon.len() {
        let (xi, yi) = polygon[i];
        let (xj, yj) = polygon[j];
        if (yi > y) != (yj > y) && x < (xj - xi) * (y - yi) / (yj - yi) + xi {
            innen = !innen;
        }
        j = i;
    }
    innen
}

// ── Export ───────────────────────────────────────────────────────────────────

/// Fachliche CSV-Exporte. Fehlende Werte bleiben leer; Herkunft ist kein AP-/NP-Typ.
pub fn csv_datei(punkte: &[KonvertierungPunkt], typ: &str, projekt: &str) -> std::io::Result<crate::daten::csv_datei::CsvDatei> {
    use crate::daten::csv_datei::{CsvDatei, TYP_KOORDINATEN, TYP_MESSDATEN, TYP_STANDPUNKT};
    let mut d = CsvDatei::neu(typ);
    d.projekt = projekt.into();
    d.spalten.extend(["Quelle".into(), "Bemerkung".into()]);
    for p in punkte {
        let koordinaten = p.vorhanden[0] && p.vorhanden[1];
        let messung = p.vorhanden[3] || p.vorhanden[4] || p.vorhanden[5];
        if (typ == TYP_KOORDINATEN && !koordinaten) || (typ == TYP_MESSDATEN && !messung) || (typ == TYP_STANDPUNKT && !messung) {
            continue;
        }
        if p.punkt_nr.trim().is_empty() {
            return Err(crate::daten::csv_basis::fehler("PunktNr fehlt in der Konvertierungstabelle."));
        }
        let mut z = vec![String::new(); d.spalten.len()];
        let werte = [p.r, p.h, p.hoehe, p.hz, p.v, p.strecke, p.zielhoehe];
        let namen = ["X", "Y", "Z", "HZ_gon", "V_gon", "Strecke_m", "Zielhoehe_m"];
        for j in 0..7 {
            if p.vorhanden[j] {
                if !werte[j].is_finite() {
                    return Err(crate::daten::csv_basis::fehler(format!("Punkt {}, Spalte {}: ungültige Zahl.", p.punkt_nr, namen[j])));
                }
                if let Some(i) = d.spalten_index(namen[j]) {
                    z[i] = f(werte[j], if j <= 4 { 4 } else { 3 });
                }
            }
        }
        for (k, v) in [("PunktNr", p.punkt_nr.as_str()), ("Code", &p.punktcode), ("Quelle", &p.typ), ("Bemerkung", &p.bemerkung)] {
            if let Some(i) = d.spalten_index(k) {
                z[i] = v.into();
            }
        }
        // Nur ausdrücklich bekannte Beobachtungstypen übernehmen. Ein Entwurf
        // mit leerem Typ erfordert beim Import eine bewusste Zuordnung.
        if let Some(i) = d.spalten_index("Typ") {
            z[i] = if p.typ == "AP" || p.typ == "NP" { p.typ.clone() } else { String::new() };
        }
        d.zeilen.push(z);
    }
    if d.zeilen.is_empty() {
        return Err(crate::daten::csv_basis::fehler("Keine passenden Daten für diesen CSV-Datentyp."));
    }
    Ok(d)
}

/// Kann der Punkt in KOR/DAT geschrieben werden (Punktnummer sowie Rechts- und Hochwert vorhanden)?
/// Reine Messzeilen und leere Zeilen erschienen dort sonst mit R = H = 0.
pub fn hat_koordinaten(p: &KonvertierungPunkt) -> bool {
    !p.punkt_nr.trim().is_empty() && p.vorhanden[0] && p.vorhanden[1]
}

/// Ergebnis eines Textexports.
#[derive(Debug)]
pub struct TextExport {
    pub text: String,
    /// Geschriebene Punkte.
    pub geschrieben: usize,
    /// Übersprungene Zeilen ohne Punktnummer oder ohne Koordinaten.
    pub uebersprungen: usize,
}

/// Koordinatenzeile der KOR-/DAT-Exporte: nur `Nr R H Höhe Code`; ein leerer Code wird zum Standardcode.
fn export_zeile(p: &KonvertierungPunkt) -> String {
    let code = if p.punktcode.trim().is_empty() { STANDARD_CODE } else { p.punktcode.trim() };
    format!("{:<16}{:>16}{:>16}{:>14}  {code}\r\n", p.punkt_nr.trim(), f(p.r, 3), f(p.h, 3), f(p.hoehe, 3))
}

/// KOR-Koordinatenliste: nur Punkte mit Koordinaten, ohne Kopf und ohne weitere Angaben.
pub fn kor_text(punkte: &[KonvertierungPunkt]) -> TextExport {
    let mut text = String::new();
    let mut geschrieben = 0;
    for z in punkte.iter().filter(|p| hat_koordinaten(p)) {
        text.push_str(&export_zeile(z));
        geschrieben += 1;
    }
    TextExport { text, geschrieben, uebersprungen: punkte.len() - geschrieben }
}

/// DAT-Felddatenformat: Standpunktzeile, danach die Punkte mit Koordinaten wie bei KOR (ohne Kopf und weitere
/// Angaben); liefert zusätzlich, ob ein Standpunkt gefunden wurde.
pub fn dat_text(punkte: &[KonvertierungPunkt]) -> (TextExport, bool) {
    let standpunkt = punkte.iter().find(|p| p.typ.eq_ignore_ascii_case(TYP_STANDPUNKT) && !p.punkt_nr.trim().is_empty());
    let nr = standpunkt.map_or(STANDARD_STANDPUNKT, |s| s.punkt_nr.trim());
    let hoehe = standpunkt.map_or_else(|| f(0.0, 3), |s| f(s.hoehe, 3));
    let mut text = format!("STANDPUNKT {nr} {hoehe} {STANDARD_CODE}\r\n");
    let mut geschrieben = 0;
    for z in punkte.iter().filter(|p| hat_koordinaten(p)) {
        text.push_str(&export_zeile(z));
        geschrieben += 1;
    }
    (TextExport { text, geschrieben, uebersprungen: punkte.len() - geschrieben }, standpunkt.is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Baut einen GSI-8-Block: WI(2) + Info(4) + Vorzeichen + 8 Datenstellen.
    fn block(wi: &str, einheit: char, wert: i64) -> String {
        format!("{wi}000{einheit}{}{:08}", if wert < 0 { '-' } else { '+' }, wert.abs())
    }

    #[test]
    fn gsi_messzeile() {
        // Punkt 12, Hz 123.4567 gon, V 98.7654 gon, Strecke 25.123 m, Zielhöhe 1.500 m
        let zeile =
            format!("*{} {} {} {} {}", format!("110001+{:08}", 12), block("21", '2', 12_345_670), block("22", '2', 9_876_540), block("31", '0', 25_123), block("87", '0', 1_500));
        let p = gsi_parse(&zeile);
        assert_eq!(p.len(), 1);
        let p = &p[0];
        assert_eq!(p.punkt_nr, "12");
        assert!((p.hz - 123.4567).abs() < 1e-9 && (p.v - 98.7654).abs() < 1e-9);
        assert!((p.strecke - 25.123).abs() < 1e-9 && (p.zielhoehe - 1.5).abs() < 1e-9);
        assert_eq!(p.typ, "Import-GSI");
    }

    #[test]
    fn gsi_koordinaten_negativ_und_bemerkung() {
        let zeile =
            format!("{} {} {} {} {}", "110001+0000A001", block("81", '0', 4_500_123_456), block("82", '6', -53_000_001_234), block("83", '0', 450_500), block("33", '0', -1_250));
        let p = gsi_parse(&zeile);
        assert_eq!(p.len(), 1, "{zeile}");
        assert_eq!(p[0].punkt_nr, "A001");
        assert!((p[0].h + 5_300_000.1234).abs() < 1e-6, "{}", p[0].h);
        assert!(p[0].bemerkung.contains("Δh=-1.250m"));
    }

    #[test]
    fn gsi_ueberspringt_leere_und_ungueltige_zeilen() {
        assert!(gsi_parse("\n   \nkurz abc\n").is_empty());
        // Nur Nummer: wird übernommen
        assert_eq!(gsi_parse("110001+00000007").len(), 1);
    }

    #[test]
    fn gsi_dms_und_einheiten() {
        assert!((dekodiere(12_345_670, '2') - 123.4567).abs() < 1e-9);
        assert!((dekodiere(25_123, '0') - 25.123).abs() < 1e-12);
        assert!((dekodiere(251_234, '6') - 25.1234).abs() < 1e-12);
        // 12°34'56.5" als 123456.50000
        assert!((dekodiere(12_345_650_000, '4') - (12.0 + 34.0 / 60.0 + 56.5 / 3600.0)).abs() < 1e-9);
    }

    fn pkt(nr: &str, r: f64, h: f64) -> KonvertierungPunkt {
        KonvertierungPunkt { punkt_nr: nr.into(), typ: "Test".into(), r, h, ..KonvertierungPunkt::default() }
    }

    #[test]
    fn duplikate_und_nummernvergabe() {
        let mut tabelle = vec![pkt("1", 10.0, 20.0), pkt("A", 30.0, 40.0)];
        assert!(ist_doppelt(&pkt("1", 99.0, 99.0), &tabelle), "gleiche Nummer");
        assert!(ist_doppelt(&pkt("", 10.0005, 20.0005), &tabelle), "gleiche Lage");
        assert!(!ist_doppelt(&pkt("", 0.0, 0.0), &[pkt("", 0.0, 0.0)]), "(0,0) zählt nicht");
        assert_eq!(naechste_punkt_nr("7", &tabelle), "7");
        assert_eq!(naechste_punkt_nr("1", &tabelle), "2");
        assert_eq!(naechste_punkt_nr("", &tabelle), "2");
        // Nummer 1 ist vergeben (Duplikat), (10/20) liegt auf Punkt 1 (Duplikat), nur der dritte Punkt kommt hinzu
        let (neu, doppelt) = importiere_in(&mut tabelle, vec![pkt("1", 50.0, 60.0), pkt("", 10.0, 20.0), pkt("", 70.0, 80.0)]);
        assert_eq!((neu, doppelt), (1, 2));
        assert_eq!(tabelle[2].punkt_nr, "2");
        let (neu, _) = importiere_in(&mut tabelle, vec![pkt("", 90.0, 91.0)]);
        assert_eq!(neu, 1);
        assert_eq!(tabelle[3].punkt_nr, "3");
    }

    #[test]
    fn exporte() {
        let mut p = pkt("1", 100.5, 200.25);
        p.hoehe = 12.0;
        p.punktcode = "Baum".into();
        p.bemerkung = "x".into();
        p.vorhanden = [true, true, true, false, false, false, false];
        let csv = csv_datei(&[p.clone()], crate::daten::csv_datei::TYP_KOORDINATEN, "Demo").unwrap();
        assert_eq!(csv.zelle(&csv.zeilen[0], "Code"), "Baum");
        assert_eq!(csv.zelle(&csv.zeilen[0], "X"), "100.5000");
        assert!(csv_datei(&[p.clone()], crate::daten::csv_datei::TYP_MESSDATEN, "Demo").is_err());
        // KOR: nur Nr R H Höhe Code – kein Kopf, keine Bemerkung
        let kor = kor_text(&[p.clone()]);
        assert_eq!(kor.text, format!("{:<16}{:>16}{:>16}{:>14}  Baum\r\n", "1", "100.500", "200.250", "12.000"));
        let (dat, mit) = dat_text(&[p.clone()]);
        assert!(!mit);
        assert_eq!(dat.text.lines().collect::<Vec<_>>(), ["STANDPUNKT STP1 0.000 005", kor.text.trim_end()]);
        let mut stp = pkt("S9", 1.0, 2.0);
        stp.typ = "standpunkt".into();
        stp.hoehe = 3.5;
        let (dat, mit) = dat_text(&[stp, p]);
        assert!(mit && dat.text.starts_with("STANDPUNKT S9 3.500 005\r\n"));
        assert!(!dat.text.contains('%') && !dat.text.contains("standpunkt"), "kein Kopf, kein Typ");
    }

    #[test]
    fn leerer_code_wird_005() {
        let mut p = pkt("7", 1.0, 2.0);
        p.vorhanden = [true, true, false, false, false, false, false];
        p.bemerkung = "wird weggelassen".into();
        let kor = kor_text(&[p]);
        assert!(kor.text.trim_end().ends_with(" 005"), "{}", kor.text);
        assert!(!kor.text.contains("weggelassen"));
    }

    #[test]
    fn hilfskoordinaten_aus_messdaten() {
        // Koordinaten haben Vorrang
        let mut k = pkt("1", 100.0, 200.0);
        k.vorhanden = [true, true, false, true, true, true, false];
        assert_eq!(auswahl_lage(&k), Some((100.0, 200.0, false)));
        // Hz 100 gon, V 100 gon, s 10 m → R = 10, H = 0 (Standpunkt 0/0, t = 0)
        let mut m = pkt("2", 0.0, 0.0);
        m.hz = 100.0;
        m.v = 100.0;
        m.strecke = 10.0;
        m.vorhanden = [false, false, false, true, true, true, true];
        let (r, h, hilf) = auswahl_lage(&m).unwrap();
        assert!(hilf && (r - 10.0).abs() < 1e-9 && h.abs() < 1e-9);
        // V 50 gon: Horizontalstrecke s·sin V; Hz 0 → nach Norden
        m.hz = 0.0;
        m.v = 50.0;
        let (r, h, _) = auswahl_lage(&m).unwrap();
        assert!(r.abs() < 1e-9 && (h - 10.0 * (std::f64::consts::PI / 4.0).sin()).abs() < 1e-9);
        // Ohne V gilt die Strecke als horizontal
        m.vorhanden[4] = false;
        assert!((auswahl_lage(&m).unwrap().1 - 10.0).abs() < 1e-9);
        // Nur Richtung ohne Strecke: keine Lage
        m.vorhanden[5] = false;
        assert_eq!(auswahl_lage(&m), None);
    }

    #[test]
    fn auswahl_anfrage_mit_vorauswahl() {
        let mut a = pkt("1", 1.0, 2.0);
        a.vorhanden = [true, true, false, false, false, false, false];
        let leer = KonvertierungPunkt::default();
        let mut b = pkt("3", 5.0, 6.0);
        b.vorhanden = [true, true, false, false, false, false, false];
        let anfrage = auswahl_anfrage(&[a, leer, b], &[false, true, true], 7);
        assert_eq!(anfrage.kandidaten.iter().map(|k| k.zeile).collect::<Vec<_>>(), [0, 2]);
        assert_eq!(anfrage.vorauswahl, [false, true]);
        assert_eq!((anfrage.ohne_lage, anfrage.tabellen_version), (1, 7));
    }

    #[test]
    fn polygon_test() {
        let quadrat = [(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)];
        assert!(punkt_im_polygon((5.0, 5.0), &quadrat));
        assert!(!punkt_im_polygon((15.0, 5.0), &quadrat));
        // Konkav (U-Form): die Einbuchtung liegt außerhalb
        let u = [(0.0, 0.0), (9.0, 0.0), (9.0, 9.0), (6.0, 9.0), (6.0, 3.0), (3.0, 3.0), (3.0, 9.0), (0.0, 9.0)];
        assert!(punkt_im_polygon((1.5, 6.0), &u));
        assert!(!punkt_im_polygon((4.5, 6.0), &u));
        assert!(!punkt_im_polygon((1.0, 1.0), &quadrat[..2]), "weniger als drei Ecken");
    }

    #[test]
    fn kor_und_dat_ueberspringen_zeilen_ohne_koordinaten() {
        let mut mit_koord = pkt("1", 100.0, 200.0);
        mit_koord.vorhanden = [true, true, false, false, false, false, false];
        // reine Messzeile (GSI), Zeile ohne Nummer, leere Zeile
        let mut messung = pkt("2", 0.0, 0.0);
        messung.vorhanden = [false, false, false, true, true, true, false];
        let mut ohne_nr = pkt("", 5.0, 6.0);
        ohne_nr.vorhanden = [true, true, false, false, false, false, false];
        let leer = KonvertierungPunkt::default();
        let punkte = [mit_koord, messung, ohne_nr, leer];
        let kor = kor_text(&punkte);
        assert_eq!((kor.geschrieben, kor.uebersprungen), (1, 3));
        assert!(!kor.text.lines().any(|z| z.starts_with('2')), "Messzeile darf nicht als Koordinate erscheinen");
        let (dat, _) = dat_text(&punkte);
        assert_eq!((dat.geschrieben, dat.uebersprungen), (1, 3));
    }

    #[test]
    fn kor_rundreise() {
        let mut a = pkt("12", 4_500_123.456, 5_300_001.234);
        a.hoehe = 412.5;
        a.vorhanden = [true, true, true, false, false, false, false];
        let mut b = pkt("A7", 1.0, 2.0);
        b.punktcode = "Baum".into();
        b.vorhanden = [true, true, true, false, false, false, false];
        let text = kor_text(&[a, b]).text;
        let p = kor_parse(&text);
        assert_eq!(p.len(), 2, "{text}");
        assert_eq!((p[0].punkt_nr.as_str(), p[0].r, p[0].h, p[0].hoehe), ("12", 4_500_123.456, 5_300_001.234, 412.5));
        assert_eq!(p[0].punktcode, STANDARD_CODE);
        assert_eq!((p[1].punktcode.as_str(), p[1].typ.as_str()), ("Baum", "Import-KOR"));
        assert!(p[1].vorhanden[0] && p[1].vorhanden[1] && p[1].vorhanden[2]);
    }

    #[test]
    fn alte_kor_mit_kopf_und_leerem_code() {
        // Frühere Exporte der Konvertierung: Kopf, feste Spalten, leerer Code, Bemerkung
        let text = format!(
            "% Koordinatenliste – Feldbuch Export\r\n% Datum:   2026-11-16\r\n%\r\n{:<16}{:>16}{:>16}{:>14}  {:<8}  {}\r\n",
            "12", "4500123.456", "5300001.234", "412.500", "", "Quelle=GSI  Datum=2026-11-16"
        );
        let p = kor_parse(&text);
        assert_eq!(p.len(), 1);
        assert_eq!((p[0].punktcode.as_str(), p[0].bemerkung.as_str()), ("", "Quelle=GSI  Datum=2026-11-16"));
    }

    #[test]
    fn kor_ohne_kopf_leerzeichengetrennt() {
        // Schreibweise des Dateneditors: Nr X Y Z Code, ohne Kopf; Dezimalkomma wird toleriert
        let text = "1001        4500000.1234    5300000.5678   401.250    GP\r\n1002  4500010,5  5300010,25\r\n% Kommentar\r\nkaputt x y\r\n";
        let p = kor_parse(text);
        assert_eq!(p.len(), 2);
        assert_eq!((p[0].punktcode.as_str(), p[0].hoehe), ("GP", 401.25));
        assert_eq!((p[1].r, p[1].h), (4_500_010.5, 5_300_010.25));
        assert!(!p[1].vorhanden[2], "fehlende Höhe bleibt unbekannt");
    }

    #[test]
    fn dat_koordinaten_rundreise() {
        let mut stp = pkt("S9", 10.0, 20.0);
        stp.typ = "Standpunkt".into();
        stp.hoehe = 3.5;
        stp.vorhanden = [true, true, true, false, false, false, false];
        let mut np = pkt("101", 11.0, 21.0);
        np.typ = "Import-GSI".into();
        np.vorhanden = [true, true, false, false, false, false, false];
        let (export, _) = dat_text(&[stp, np]);
        assert!(ist_koordinaten_dat(&export.text));
        let p = dat_parse(&export.text);
        assert_eq!(p.len(), 2, "{}", export.text);
        // Der Standpunkt wird über die STANDPUNKT-Zeile erkannt, andere Punkte sind Import-DAT
        assert_eq!((p[0].punkt_nr.as_str(), p[0].typ.as_str(), p[0].hoehe), ("S9", "Standpunkt", 3.5));
        assert_eq!((p[1].typ.as_str(), p[1].r, p[1].punktcode.as_str()), ("Import-DAT", 11.0, "005"));
    }

    #[test]
    fn dat_messdaten_mit_standpunkten() {
        let text = "-STP1 1.523 005\r\n 101   123.4567   98.7654   25.123   1.500  Baum\r\n 102   200.0000  100.0000   10.000   1.300\r\n\
                    -STP2 1.600\r\n 103   10.0000  99.0000  5.000  1.500\r\n 104 kurz\r\n";
        assert!(!ist_koordinaten_dat(text));
        let p = dat_parse(text);
        assert_eq!(p.len(), 3);
        assert_eq!((p[0].punkt_nr.as_str(), p[0].hz, p[0].v, p[0].strecke, p[0].zielhoehe), ("101", 123.4567, 98.7654, 25.123, 1.5));
        assert_eq!((p[0].punktcode.as_str(), p[0].bemerkung.as_str()), ("Baum", "STP=STP1, IH=1.523m"));
        assert_eq!(p[2].bemerkung, "STP=STP2, IH=1.600m");
        assert_eq!(p[0].vorhanden, [false, false, false, true, true, true, true]);
    }

    #[test]
    fn gsi_horizontalstrecke_ist_keine_schraegstrecke() {
        let zeile = format!("{} {} {}", format!("110001+{:08}", 3), block("21", '2', 10_000_000), block("32", '0', 12_345));
        let p = &gsi_parse(&zeile)[0];
        assert_eq!(p.strecke, 0.0);
        assert!(!p.vorhanden[5], "Schrägstrecke nicht vorhanden");
        assert!(p.bemerkung.contains("HD=12.345m"), "{}", p.bemerkung);
        // Schrägstrecke und Horizontalstrecke: Schrägstrecke bleibt
        let zeile = format!("{} {} {}", format!("110001+{:08}", 4), block("31", '0', 20_000), block("32", '0', 19_000));
        let p = &gsi_parse(&zeile)[0];
        assert!((p.strecke - 20.0).abs() < 1e-12 && p.vorhanden[5]);
        // Nur Horizontalstrecke ohne Nummer wird trotzdem übernommen
        assert_eq!(gsi_parse(&block("32", '0', 5_000)).len(), 1);
    }

    #[test]
    fn dxf_polylinien_und_hoehen() {
        let dxf = "0\nSECTION\n2\nENTITIES\n\
            0\nLWPOLYLINE\n8\nFlst\n38\n412.5\n70\n1\n10\n1\n20\n2\n10\n3\n20\n4\n10\n5\n20\n6\n\
            0\nPOLYLINE\n8\nBruch\n10\n0\n20\n0\n30\n0\n70\n8\n\
            0\nVERTEX\n8\nBruch\n10\n10\n20\n11\n30\n401.25\n70\n32\n\
            0\nVERTEX\n8\nBruch\n10\n0\n20\n0\n30\n0\n70\n128\n\
            0\nSEQEND\n8\nBruch\n\
            0\nPOINT\n8\nP\n10\n7\n20\n8\n30\n0.0\n\
            0\nLINE\n8\nL\n10\n1\n20\n1\n30\n399.5\n11\n2\n21\n2\n31\n0\n\
            0\nENDSEC\n0\nEOF\n";
        let dir = std::env::temp_dir().join(format!("fb-konv-pl-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let pfad = dir.join("pl.dxf");
        fs::write(&pfad, dxf).unwrap();
        let p = dxf_import(&pfad).unwrap();
        let _ = fs::remove_dir_all(dir);
        // 3 Polylinien-Eckpunkte, 1 Vertex (Flächenverweis übersprungen), 1 Punkt, 2 Linienendpunkte
        assert_eq!(p.len(), 7, "{p:?}");
        assert!(p[..3].iter().all(|q| q.typ == "DXF-Polylinie" && q.vorhanden[2] && q.hoehe == 412.5));
        assert_eq!((p[2].r, p[2].h), (5.0, 6.0));
        assert_eq!(p[2].bemerkung, "Layer=Flst, Eckpunkt 3");
        assert_eq!((p[3].r, p[3].h, p[3].hoehe), (10.0, 11.0, 401.25));
        assert_eq!(p[4].typ, "DXF-Punkt");
        assert!(!p[4].vorhanden[2], "Höhe 0 gilt als nicht vorhanden");
        assert!(p[5].vorhanden[2] && p[5].hoehe == 399.5);
        assert!(!p[6].vorhanden[2]);
    }

    #[test]
    fn dxf_import_aus_datei() {
        let dir = std::env::temp_dir().join(format!("fb-konv-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let pfad = dir.join("t.dxf");
        let dxf = "0\nSECTION\n2\nENTITIES\n0\nLINE\n8\nGrenze\n10\n1.0\n20\n2.0\n11\n3.0\n21\n4.0\n0\nPOINT\n8\nP\n10\n5.0\n20\n6.0\n0\nCIRCLE\n8\nK\n10\n7.0\n20\n8.0\n40\n1.5\n0\nENDSEC\n0\nEOF\n";
        fs::write(&pfad, dxf).unwrap();
        let p = dxf_import(&pfad).unwrap();
        assert_eq!(p.len(), 4, "{p:?}");
        assert_eq!((p[0].typ.as_str(), p[0].r, p[1].h), ("DXF-Linie", 1.0, 4.0));
        assert_eq!(p[0].bemerkung, "Layer=Grenze, Linienendpunkt 1");
        assert_eq!(p[2].typ, "DXF-Punkt");
        assert_eq!(p[3].bemerkung, "Layer=K, R=1.500m");
        let _ = fs::remove_dir_all(dir);
    }
}
