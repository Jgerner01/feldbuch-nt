// Zweck:    Extraktion aus dem Messprotokoll (Protokoll.md, Schritt 9): das Protokoll wird „abgespielt“ und daraus je
//           Aufstellung eine stp.csv (Anschlusspunkte mit Koordinaten, Stationierungsergebnis), eine gemeinsame
//           dat.csv (Neupunkt-, Kontroll- und Absteckmessungen aller Aufstellungen) und eine kor.csv (Standpunkte und
//           neu gerechnete Neupunkte) erzeugt. Korrekturen, Nummernänderungen, Ersetzungen und Löschungen werden
//           über „Bezug“ angewandt, Umbenennungen von Standpunkten nachgeführt. Die Ausgabe liegt im Unterordner
//           „Extraktion aus Messdaten“ des Projekts; vorhandene Projektdateien werden nie angefasst.
// Hardware: keine
// Autor:    Johann Gerner

use std::collections::{HashMap, HashSet};
use std::io;
use std::path::{Path, PathBuf};

use crate::daten::csv_datei::{CsvDatei, TYP_MESSDATEN, TYP_STANDPUNKT};
use crate::daten::kor_csv::{self, KorPunkt};
use crate::daten::messdaten::{TYP_ANSCHLUSSPUNKT, messzeit};
use crate::daten::messprotokoll::{Eintrag, Ereignis, zeit_schluessel};
use crate::daten::modelle::{NeupunktRohdaten, StationierungsErgebnis};
use crate::daten::rechenparameter::Rechenparameter;
use crate::grundlagen::datei_speicher;
use crate::grundlagen::format::{f, parse_zahl};
use crate::rechnen::neupunkt;

/// Unterordner des Projektverzeichnisses für alle extrahierten Dateien.
pub const ORDNER: &str = "Extraktion aus Messdaten";
/// Schlüssel für Messungen ohne AufstellungId (Altdaten): „(ohne Id) <Standpunkt>“.
const OHNE_ID: &str = "(ohne Id) ";

/// Kopfangaben der erzeugten Dateien.
#[derive(Clone, Debug, Default)]
pub struct Kopf {
    pub projekt: String,
    pub sensor: String,
    pub bearbeiter: String,
}

/// Übersicht über eine Aufstellung des Protokolls (für die Auswahl in der Oberfläche).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Aufstellung {
    /// AufstellungId (bei Altdaten „(ohne Id) <Standpunkt>“).
    pub id: String,
    /// Standpunktnummer nach allen Umbenennungen.
    pub standpunkt_nr: String,
    /// Datum der ersten Aktion (JJJJ-MM-TT).
    pub datum: String,
    pub anschlusspunkte: usize,
    pub neupunkte: usize,
    pub kontrollen: usize,
    pub absteckungen: usize,
    pub hat_stationierung: bool,
}

/// Ergebnis der Extraktion im Speicher.
#[derive(Clone, Debug, Default)]
pub struct Extraktion {
    /// Je Aufstellung mit Anschlusspunkten oder Stationierung: Dateiname-Teil und Datei.
    pub stp: Vec<(String, CsvDatei)>,
    pub dat: Option<CsvDatei>,
    pub kor: Vec<KorPunkt>,
    /// Auffälligkeiten (nicht berechnete Punkte, doppelte Nummern, …).
    pub hinweise: Vec<String>,
}

// ── Protokoll abspielen ──────────────────────────────────────────────────────

struct Messung {
    lfd: u64,
    e: Eintrag,
    aktiv: bool,
}

struct Station {
    lfd: u64,
    ergebnis: StationierungsErgebnis,
    ih: Option<f64>,
}

#[derive(Default)]
struct Stand {
    id: String,
    nr: String,
    /// LfdNr des Eintrags, der die Nummer gesetzt hat (spätere Umbenennungen gelten).
    nr_lfd: u64,
    code: String,
    ih: Option<f64>,
    erste_zeit: String,
    station: Option<Station>,
    messungen: Vec<usize>,
}

#[derive(Default)]
struct Modell {
    staende: Vec<Stand>,
    messungen: Vec<Messung>,
}

fn schluessel(e: &Eintrag) -> Option<String> {
    if !e.aufstellung_id.trim().is_empty() {
        Some(e.aufstellung_id.trim().to_string())
    } else if !e.standpunkt_nr.trim().is_empty() {
        Some(format!("{OHNE_ID}{}", e.standpunkt_nr.trim()))
    } else {
        None
    }
}

/// Zahl hinter einem Schlüssel im Text („ Ori=399.99 gon“ → 399.99).
fn wert_nach(text: &str, schluessel: &str) -> Option<f64> {
    let rest = &text[text.find(schluessel)? + schluessel.len()..];
    parse_zahl(rest.split_whitespace().next()?)
}

/// Stationierungsergebnis aus einem STATIONIERUNG-Eintrag (R/H/Höhe in den Spalten, Rest im Text).
fn lese_station(e: &Eintrag) -> Option<StationierungsErgebnis> {
    let (r, h, hoehe) = (e.r?, e.h?, e.hoehe?);
    let t = format!(" {}", e.text);
    let ori = wert_nach(&t, " Ori=")?;
    Some(StationierungsErgebnis {
        r,
        h,
        hoehe,
        orientierung_gon: ori,
        massstab: wert_nach(&t, " Massstab=").filter(|m| *m > 0.0).unwrap_or(1.0),
        s0_mm: wert_nach(&t, " s0=").unwrap_or(0.0),
        redundanz: wert_nach(&t, " n=").map_or(0, |n| n as i32),
        konvergiert: true,
        berechnung_3d: !t.contains(" 3D=nein"),
        ..StationierungsErgebnis::default()
    })
}

impl Modell {
    fn stand(&mut self, e: &Eintrag) -> Option<usize> {
        let id = schluessel(e)?;
        if let Some(i) = self.staende.iter().position(|s| s.id == id) {
            return Some(i);
        }
        self.staende.push(Stand { id, nr: e.standpunkt_nr.trim().to_string(), nr_lfd: e.lfd_nr, ..Stand::default() });
        Some(self.staende.len() - 1)
    }
}

/// Spielt das Protokoll von vorne nach hinten ab.
fn spiele_ab(eintraege: &[Eintrag]) -> Modell {
    let mut m = Modell::default();
    let mut nach_lfd: HashMap<u64, usize> = HashMap::new();
    let mut umbenennungen: Vec<(u64, String, String)> = Vec::new();
    for e in eintraege {
        let Some(art) = e.art() else { continue };
        match art {
            Ereignis::StpNeu | Ereignis::StpFort | Ereignis::Ih | Ereignis::Stationierung | Ereignis::Messung => {
                if art == Ereignis::Messung && !matches!(e.typ.trim().to_ascii_uppercase().as_str(), "AP" | "NP" | "KP" | "AB") {
                    continue;
                }
                let Some(i) = m.stand(e) else { continue };
                if m.staende[i].erste_zeit.is_empty() {
                    m.staende[i].erste_zeit.clone_from(&e.zeit);
                }
                match art {
                    Ereignis::StpNeu | Ereignis::StpFort => {
                        let s = &mut m.staende[i];
                        if !e.standpunkt_nr.trim().is_empty() {
                            s.nr = e.standpunkt_nr.trim().to_string();
                            s.nr_lfd = e.lfd_nr;
                        }
                        if !e.code.trim().is_empty() {
                            s.code = e.code.trim().to_string();
                        }
                        if e.ih.is_some() {
                            s.ih = e.ih;
                        }
                    }
                    Ereignis::Ih => {
                        // Alle Messungen der Aufstellung erhalten die korrigierte Höhe (wie im Programm)
                        if let Some(neu) = parse_zahl(&e.neu) {
                            m.staende[i].ih = Some(neu);
                            for &k in &m.staende[i].messungen {
                                m.messungen[k].e.ih = Some(neu);
                            }
                        }
                    }
                    Ereignis::Stationierung => {
                        if let Some(ergebnis) = lese_station(e) {
                            m.staende[i].station = Some(Station { lfd: e.lfd_nr, ergebnis, ih: e.ih });
                        }
                    }
                    _ => {
                        let k = m.messungen.len();
                        let mut kopie = e.clone();
                        kopie.typ = kopie.typ.trim().to_ascii_uppercase();
                        m.messungen.push(Messung { lfd: e.lfd_nr, e: kopie, aktiv: true });
                        m.staende[i].messungen.push(k);
                        nach_lfd.insert(e.lfd_nr, k);
                    }
                }
            }
            Ereignis::Korrektur | Ereignis::Nummer | Ereignis::Exz | Ereignis::Ersetzt | Ereignis::Geloescht => {
                let ziel = e.bezug.and_then(|b| nach_lfd.get(&b).copied()).or_else(|| {
                    // Ohne Bezug: letzte gültige Messung derselben Aufstellung, Nummer und Art
                    if art != Ereignis::Exz {
                        return None;
                    }
                    m.messungen.iter().rposition(|x| x.aktiv && x.e.aufstellung_id == e.aufstellung_id && x.e.punkt_nr == e.punkt_nr && x.e.typ == "NP")
                });
                let Some(k) = ziel else { continue };
                let x = &mut m.messungen[k];
                match art {
                    Ereignis::Ersetzt | Ereignis::Geloescht => x.aktiv = false,
                    Ereignis::Nummer if !e.neu.trim().is_empty() => x.e.punkt_nr = e.neu.trim().to_string(),
                    Ereignis::Exz => (x.e.exz_l, x.e.exz_q) = (e.exz_l, e.exz_q),
                    Ereignis::Korrektur => {
                        let neu = parse_zahl(&e.neu);
                        match e.feld.as_str() {
                            "Hz_gon" => x.e.hz = neu,
                            "V_gon" => x.e.v = neu,
                            "S_m" => x.e.s = neu,
                            "ZH_m" => x.e.zh = neu,
                            _ => {}
                        }
                    }
                    _ => {}
                }
            }
            Ereignis::StpUmbenannt => umbenennungen.push((e.lfd_nr, e.alt.trim().to_string(), e.neu.trim().to_string())),
            _ => {}
        }
    }
    // Umbenennungen, die nach der letzten Nennung des Namens erfolgten, der Reihe nach anwenden
    for s in &mut m.staende {
        for (lfd, alt, neu) in &umbenennungen {
            if *lfd > s.nr_lfd && s.nr == *alt && !neu.is_empty() {
                s.nr.clone_from(neu);
            }
        }
    }
    m
}

/// Aufstellungen des Protokolls mit ihren Zahlen (in der Reihenfolge des ersten Auftretens).
pub fn aufstellungen(eintraege: &[Eintrag]) -> Vec<Aufstellung> {
    let m = spiele_ab(eintraege);
    m.staende
        .iter()
        .map(|s| {
            let zaehle = |typ: &str| s.messungen.iter().filter(|&&k| m.messungen[k].aktiv && m.messungen[k].e.typ == typ).count();
            Aufstellung {
                id: s.id.clone(),
                standpunkt_nr: s.nr.clone(),
                datum: s.erste_zeit.get(..10).unwrap_or("").to_string(),
                anschlusspunkte: zaehle("AP"),
                neupunkte: zaehle("NP"),
                kontrollen: zaehle("KP"),
                absteckungen: zaehle("AB"),
                hat_stationierung: s.station.is_some(),
            }
        })
        .collect()
}

// ── Dateien aufbauen ─────────────────────────────────────────────────────────

fn zahl(w: Option<f64>, stellen: usize) -> String {
    w.filter(|x| x.is_finite()).map_or_else(String::new, |x| f(x, stellen))
}

fn zeitstempel(e: &Eintrag) -> String {
    zeit_schluessel(&e.zeit).map_or_else(|| e.zeit.clone(), messzeit)
}

fn sicherer_name(text: &str) -> String {
    text.chars().map(|c| if r#"\/:*?"<>|"#.contains(c) || c.is_control() { '_' } else { c }).collect()
}

fn id_spalte(id: &str) -> String {
    if id.starts_with(OHNE_ID) { String::new() } else { id.to_string() }
}

/// Erzeugt die Dateien im Speicher. `auswahl`: nur diese Aufstellungen (Schlüssel aus `aufstellungen`), sonst alle.
pub fn extrahiere(eintraege: &[Eintrag], auswahl: Option<&HashSet<String>>, kopf: &Kopf, params: &Rechenparameter) -> Extraktion {
    let m = spiele_ab(eintraege);
    let mut aus = Extraktion::default();
    let gewaehlt = |s: &Stand| auswahl.is_none_or(|a| a.contains(&s.id));
    let einzeln = m.staende.iter().filter(|s| gewaehlt(s)).count() == 1;

    // Messdatei: Zeilen und Kommentare nach LfdNr geordnet
    let mut dat = CsvDatei::neu(TYP_MESSDATEN);
    dat.projekt.clone_from(&kopf.projekt);
    dat.sensor.clone_from(&kopf.sensor);
    dat.bearbeiter.clone_from(&kopf.bearbeiter);
    enum Zeile {
        Kommentar(String),
        Messung(Vec<String>),
    }
    let mut ereignisse: Vec<(u64, u8, Zeile)> = Vec::new();

    let mut nummern: HashMap<String, Vec<String>> = HashMap::new();
    for s in m.staende.iter().filter(|s| gewaehlt(s)) {
        let aktive: Vec<&Messung> = s.messungen.iter().map(|&k| &m.messungen[k]).filter(|x| x.aktiv).collect();
        let ih_final = s.station.as_ref().and_then(|st| st.ih).or(s.ih).or_else(|| aktive.iter().rev().find_map(|x| x.e.ih)).unwrap_or(0.0);

        // Messdatei: alles außer Anschlusspunkten
        let dat_zeilen: Vec<&&Messung> = aktive.iter().filter(|x| x.e.typ != TYP_ANSCHLUSSPUNKT).collect();
        if let Some(erste) = dat_zeilen.first() {
            ereignisse.push((erste.lfd, 0, Zeile::Kommentar(format!("# NEUER STANDPUNKT: SP={}  IH={} m  Id={}", s.nr, f(ih_final, 3), id_spalte(&s.id)))));
        }
        for x in &dat_zeilen {
            let e = &x.e;
            ereignisse.push((
                x.lfd,
                1,
                Zeile::Messung(vec![
                    e.punkt_nr.clone(),
                    zahl(e.hz, 4),
                    zahl(e.v, 4),
                    zahl(e.s, 3),
                    zahl(e.zh, 3),
                    e.code.clone(),
                    zeitstempel(e),
                    zahl(e.exz_l.or(Some(0.0)), 3),
                    zahl(e.exz_q.or(Some(0.0)), 3),
                    s.nr.clone(),
                    e.typ.clone(),
                    id_spalte(&s.id),
                    zahl(e.ih.or(Some(ih_final)), 3),
                ]),
            ));
        }
        if let (Some(st), false) = (&s.station, dat_zeilen.is_empty()) {
            let g = &st.ergebnis;
            ereignisse.push((
                st.lfd,
                1,
                Zeile::Kommentar(format!(
                    "# STATIONIERUNG: SP={}  IH={} m  R={}  H={}  Hoehe={}  Ori={} gon  s0={} mm  n={}",
                    s.nr,
                    f(ih_final, 3),
                    f(g.r, 4),
                    f(g.h, 4),
                    f(g.hoehe, 4),
                    f(g.orientierung_gon, 4),
                    f(g.s0_mm, 2),
                    g.redundanz
                )),
            ));
        }

        // Standpunktdatei: Anschlusspunkte und Stationierungsergebnis
        let ap: Vec<&&Messung> = aktive.iter().filter(|x| x.e.typ == TYP_ANSCHLUSSPUNKT).collect();
        if !ap.is_empty() || s.station.is_some() {
            let mut d = CsvDatei::neu(TYP_STANDPUNKT);
            d.projekt.clone_from(&kopf.projekt);
            d.sensor.clone_from(&kopf.sensor);
            d.bearbeiter.clone_from(&kopf.bearbeiter);
            if let Some(datum) = s.erste_zeit.get(..10) {
                d.datum = datum.to_string();
            }
            d.standpunkt_nr.clone_from(&s.nr);
            d.instr_hoehe = f(ih_final, 3);
            d.standpunkt_code.clone_from(&s.code);
            if !s.id.starts_with(OHNE_ID) {
                d.metadaten.push(("AufstellungId".into(), s.id.clone()));
            }
            d.zeilen = ap
                .iter()
                .map(|x| {
                    let e = &x.e;
                    vec![
                        e.punkt_nr.clone(),
                        zahl(e.r, 4),
                        zahl(e.h, 4),
                        zahl(e.hoehe, 4),
                        zahl(e.hz, 4),
                        zahl(e.v, 4),
                        zahl(e.s, 3),
                        zahl(e.zh, 3),
                        e.code.clone(),
                        zeitstempel(e),
                        zahl(e.exz_l.or(Some(0.0)), 3),
                        zahl(e.exz_q.or(Some(0.0)), 3),
                        TYP_ANSCHLUSSPUNKT.to_string(),
                    ]
                })
                .collect();
            if let Some(st) = &s.station {
                let g = &st.ergebnis;
                d.stationierung = vec![
                    ["X", "Y", "Z", "Orientierung_gon", "Massstab", "s0_mm", "Redundanz"].iter().map(|t| (*t).to_string()).collect(),
                    vec![f(g.r, 4), f(g.h, 4), f(g.hoehe, 4), f(g.orientierung_gon, 4), f(g.massstab, 8), f(g.s0_mm, 2), g.redundanz.to_string()],
                ];
            }
            aus.stp.push((sicherer_name(&s.id), d));
        }

        // Koordinaten: Standpunkt und neu gerechnete Neupunkte
        let np: Vec<&&Messung> = aktive.iter().filter(|x| x.e.typ == "NP").collect();
        match &s.station {
            Some(st) => {
                let g = &st.ergebnis;
                aus.kor.push(KorPunkt { punkt_nr: s.nr.clone(), r: g.r, h: g.h, hoehe: Some(g.hoehe), code: s.code.clone() });
                nummern.entry(s.nr.clone()).or_default().push(s.nr.clone());
                let mut nicht_berechnet = Vec::new();
                for x in &np {
                    let e = &x.e;
                    let roh = NeupunktRohdaten {
                        aufstellung_id: id_spalte(&s.id),
                        punkt_nr: e.punkt_nr.clone(),
                        code: e.code.clone(),
                        hz_gon: e.hz.unwrap_or(f64::NAN),
                        v_gon: e.v.unwrap_or(f64::NAN),
                        strecke_m: e.s.unwrap_or(0.0),
                        zielhoehe_m: e.zh.unwrap_or(0.0),
                        standpunkt_nr: s.nr.clone(),
                        exz_laengs_m: e.exz_l.unwrap_or(0.0),
                        exz_quer_m: e.exz_q.unwrap_or(0.0),
                        ..NeupunktRohdaten::default()
                    };
                    let ergebnis = if roh.hz_gon.is_finite() && roh.v_gon.is_finite() { neupunkt::berechnen(&roh, g, params, None, ih_final) } else { None };
                    match ergebnis {
                        Some(n) => {
                            aus.kor.push(KorPunkt { punkt_nr: n.punkt_nr.clone(), r: n.r, h: n.h, hoehe: n.ist_3d.then_some(n.hoehe), code: n.code.clone() });
                            nummern.entry(n.punkt_nr.clone()).or_default().push(s.nr.clone());
                        }
                        None => nicht_berechnet.push(e.punkt_nr.clone()),
                    }
                }
                if !nicht_berechnet.is_empty() {
                    aus.hinweise.push(format!("Aufstellung {}: ohne Strecke oder Winkel, nicht berechnet: {}", s.nr, nicht_berechnet.join(", ")));
                }
            }
            None if !np.is_empty() => {
                aus.hinweise.push(format!("Aufstellung {}: keine Stationierung im Protokoll – {} Neupunkt(e) nicht berechnet, nur in der dat.csv.", s.nr, np.len()));
            }
            None => {}
        }
    }

    ereignisse.sort_by_key(|(lfd, art, _)| (*lfd, *art));
    for (_, _, z) in ereignisse {
        match z {
            Zeile::Kommentar(t) => dat.kommentare.push((dat.zeilen.len(), t)),
            Zeile::Messung(w) => dat.zeilen.push(w),
        }
    }
    if einzeln {
        if let Some(s) = m.staende.iter().find(|s| gewaehlt(s)) {
            dat.standpunkt_nr.clone_from(&s.nr);
            dat.instr_hoehe = f(s.station.as_ref().and_then(|st| st.ih).or(s.ih).unwrap_or(0.0), 3);
        }
    }
    if !dat.zeilen.is_empty() {
        aus.dat = Some(dat);
    }
    let mut doppelt: Vec<String> = nummern.iter().filter(|(_, l)| l.len() > 1).map(|(n, l)| format!("{n} ({})", l.join(", "))).collect();
    doppelt.sort();
    if !doppelt.is_empty() {
        aus.hinweise.push(format!("Punktnummern mehrfach vergeben (Standpunkte in Klammern): {}", doppelt.join("; ")));
    }
    aus
}

// ── Schreiben ────────────────────────────────────────────────────────────────

/// Zielordner der Extraktion unterhalb des Ausgabeverzeichnisses.
pub fn zielordner(basis: &Path) -> PathBuf {
    basis.join(ORDNER)
}

/// Schreibt die Dateien in `ordner` (wird angelegt). Alle Namen tragen den Zeitstempel, ältere Extraktionen
/// bleiben erhalten. Liefert die geschriebenen Dateien.
pub fn schreibe(ex: &Extraktion, ordner: &Path, kopf: &Kopf, zeitmarke: &str) -> io::Result<Vec<PathBuf>> {
    std::fs::create_dir_all(ordner)?;
    let basis = format!("{}-Extraktion-{zeitmarke}", sicherer_name(if kopf.projekt.is_empty() { "Projekt" } else { &kopf.projekt }));
    let mut dateien = Vec::new();
    for (id, d) in &ex.stp {
        let mut d = d.clone();
        d.pfad = ordner.join(format!("{basis}-{id}-stp.csv"));
        d.speichern()?;
        dateien.push(d.pfad);
    }
    if let Some(d) = &ex.dat {
        let mut d = d.clone();
        d.pfad = ordner.join(format!("{basis}-dat.csv"));
        d.speichern()?;
        dateien.push(d.pfad);
    }
    if !ex.kor.is_empty() {
        let datum = chrono::Local::now().format("%Y-%m-%d").to_string();
        let text = kor_csv::erzeuge(&kopf.projekt, &kopf.sensor, &datum, &ex.kor)?;
        let pfad = ordner.join(format!("{basis}-kor.csv"));
        datei_speicher::schreibe_text(&pfad, &text)?;
        dateien.push(pfad);
    }
    Ok(dateien)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daten::messprotokoll as mp;

    fn eintrag(lfd: u64, art: Ereignis) -> Eintrag {
        let mut e = Eintrag::neu(art);
        e.lfd_nr = lfd;
        e.zeit = format!("2026-11-23T23:{:02}:00", lfd % 60);
        e
    }

    fn stp(lfd: u64, id: &str, nr: &str, ih: f64) -> Eintrag {
        let mut e = eintrag(lfd, Ereignis::StpNeu);
        (e.aufstellung_id, e.standpunkt_nr, e.ih) = (id.into(), nr.into(), Some(ih));
        e
    }

    fn messung(lfd: u64, id: &str, sp: &str, nr: &str, typ: &str, hz: f64, v: f64, s: f64, ih: f64) -> Eintrag {
        let mut e = eintrag(lfd, Ereignis::Messung);
        (e.aufstellung_id, e.standpunkt_nr, e.ih, e.punkt_nr, e.typ) = (id.into(), sp.into(), Some(ih), nr.into(), typ.into());
        (e.hz, e.v, e.s, e.zh) = (Some(hz), Some(v), Some(s), Some(0.0));
        e
    }

    fn ap(lfd: u64, id: &str, sp: &str, nr: &str, r: f64, h: f64, hoehe: f64, hz: f64, v: f64, s: f64) -> Eintrag {
        let mut e = messung(lfd, id, sp, nr, "AP", hz, v, s, 1.5);
        (e.r, e.h, e.hoehe) = (Some(r), Some(h), Some(hoehe));
        e
    }

    fn stationierung(lfd: u64, id: &str, sp: &str, r: f64, h: f64, hoehe: f64, ori: f64, ih: f64) -> Eintrag {
        let mut e = eintrag(lfd, Ereignis::Stationierung);
        (e.aufstellung_id, e.standpunkt_nr, e.ih) = (id.into(), sp.into(), Some(ih));
        (e.r, e.h, e.hoehe) = (Some(r), Some(h), Some(hoehe));
        e.text = format!("freie Stationierung Ori={} gon Massstab=0.999986 s0=4.31 mm n=4 3D=ja Anschluss: 6, 5", f(ori, 5));
        e
    }

    fn kopf() -> Kopf {
        Kopf { projekt: "Test5".into(), sensor: "LeicaTPS300".into(), bearbeiter: "hans".into() }
    }

    /// Zwei Aufstellungen: STP1 (Stationierung, 2 AP, 2 NP) und STP2 (ohne Stationierung, 1 NP).
    fn beispiel() -> Vec<Eintrag> {
        let mut v = vec![
            stp(1, "STP1-A", "STP1", 0.0),
            messung(2, "STP1-A", "STP1", "1", "NP", 321.2390, 93.3045, 1.472, 0.0),
            messung(3, "STP1-A", "STP1", "2", "NP", 321.2390, 93.3045, 1.471, 0.0),
            ap(4, "STP1-A", "STP1", "6", 2.9446, 1.4694, 1.355, 70.5340, 75.1300, 3.559),
            ap(5, "STP1-A", "STP1", "5", 0.3021, 2.3929, 1.509, 7.9995, 66.2145, 2.982),
            stationierung(6, "STP1-A", "STP1", 0.0001, 0.0002, -1.5002, 399.9986, 1.5),
            stp(7, "STP2-B", "STP2", 1.6),
            messung(8, "STP2-B", "STP2", "20", "NP", 10.0, 100.0, 5.0, 1.6),
        ];
        v.push(eintrag(9, Ereignis::Bemerkung));
        v
    }

    #[test]
    fn mehrere_standpunkte_werden_getrennt_ausgegeben() {
        let ex = extrahiere(&beispiel(), None, &kopf(), &Rechenparameter::default());
        // Standpunktdatei nur für STP1 (STP2 hat weder Anschlusspunkte noch Stationierung)
        assert_eq!(ex.stp.len(), 1);
        let (_, d) = &ex.stp[0];
        assert_eq!(d.datentyp, TYP_STANDPUNKT);
        assert_eq!(d.standpunkt_nr, "STP1");
        assert_eq!(d.instr_hoehe, "1.500");
        assert_eq!(d.zeilen.len(), 2);
        assert_eq!(d.zeilen[0][..4], ["6", "2.9446", "1.4694", "1.3550"]);
        assert_eq!(d.zeilen[0][12], "AP");
        assert_eq!(d.stationierung[1][3], "399.9986");
        assert_eq!(d.stationierung[1][6], "4");
        // Messdatei: alle NP beider Standpunkte, jeweils mit Aufstellung
        let dat = ex.dat.as_ref().unwrap();
        assert_eq!(dat.zeilen.len(), 3);
        assert_eq!(dat.zelle(&dat.zeilen[2], "StandpunktNr"), "STP2");
        assert_eq!(dat.zelle(&dat.zeilen[2], "AufstellungId"), "STP2-B");
        assert_eq!(dat.zelle(&dat.zeilen[0], "Typ"), "NP");
        assert!(dat.standpunkt_nr.is_empty(), "bei mehreren Aufstellungen kein gemeinsamer Kopf");
        assert_eq!(dat.kommentare.iter().filter(|(_, t)| t.starts_with("# NEUER STANDPUNKT")).count(), 2);
        assert!(dat.kommentare.iter().any(|(pos, t)| *pos == 2 && t.starts_with("# STATIONIERUNG: SP=STP1")), "{:?}", dat.kommentare);
        // Koordinaten: Standpunkt + 2 Neupunkte von STP1; STP2 nicht berechenbar
        assert_eq!(ex.kor.len(), 3);
        assert_eq!(ex.kor[0].punkt_nr, "STP1");
        assert!(ex.hinweise.iter().any(|h| h.contains("STP2") && h.contains("keine Stationierung")), "{:?}", ex.hinweise);
        // Text der Dateien ist gültig (Spaltenprüfung im Schreiber)
        assert!(d.text().unwrap().contains("# STATIONIERUNG"));
        assert!(dat.text().unwrap().contains("# NEUER STANDPUNKT: SP=STP2"));
    }

    #[test]
    fn neupunkte_werden_neu_gerechnet() {
        let ex = extrahiere(&beispiel(), None, &kopf(), &Rechenparameter::default());
        let p1 = ex.kor.iter().find(|p| p.punkt_nr == "1").unwrap();
        // gleiche Rechnung wie im Programm (Beispiel testdaten/polar-test5: R −1.383, H 0.479, Höhe 0.1545)
        assert!((p1.r + 1.3832).abs() < 0.002, "{}", p1.r);
        assert!((p1.h - 0.4793).abs() < 0.002, "{}", p1.h);
        assert!((p1.hoehe.unwrap() - 0.1545).abs() < 0.002, "{:?}", p1.hoehe);
    }

    #[test]
    fn auswahl_beschraenkt_auf_gewaehlte_aufstellungen() {
        let nur: HashSet<String> = ["STP2-B".to_string()].into();
        let ex = extrahiere(&beispiel(), Some(&nur), &kopf(), &Rechenparameter::default());
        assert!(ex.stp.is_empty());
        let dat = ex.dat.as_ref().unwrap();
        assert_eq!(dat.zeilen.len(), 1);
        assert_eq!(dat.standpunkt_nr, "STP2", "bei einer Aufstellung steht sie im Kopf");
        assert_eq!(dat.instr_hoehe, "1.600");
        let liste = aufstellungen(&beispiel());
        assert_eq!(liste.len(), 2);
        assert_eq!((liste[0].anschlusspunkte, liste[0].neupunkte, liste[0].hat_stationierung), (2, 2, true));
        assert_eq!((liste[1].standpunkt_nr.as_str(), liste[1].hat_stationierung, liste[1].datum.as_str()), ("STP2", false, "2026-11-23"));
    }

    #[test]
    fn korrekturen_nummern_ersetzen_loeschen_und_umbenennen() {
        let mut v = beispiel();
        let mut k = eintrag(10, Ereignis::Korrektur);
        (k.bezug, k.feld, k.neu) = (Some(4), "Hz_gon".into(), "70.6000".into());
        v.push(k);
        let mut n = eintrag(11, Ereignis::Nummer);
        (n.bezug, n.neu) = (Some(2), "1a".into());
        v.push(n);
        let mut e = eintrag(12, Ereignis::Ersetzt);
        e.bezug = Some(3);
        v.push(e);
        let mut g = eintrag(13, Ereignis::Geloescht);
        g.bezug = Some(5);
        v.push(g);
        let mut x = eintrag(14, Ereignis::Exz);
        (x.bezug, x.exz_l, x.exz_q) = (Some(2), Some(0.1), Some(-0.05));
        v.push(x);
        let mut u = eintrag(15, Ereignis::StpUmbenannt);
        (u.alt, u.neu) = ("STP1".into(), "A1".into());
        v.push(u);
        let ex = extrahiere(&v, None, &kopf(), &Rechenparameter::default());
        let (_, d) = &ex.stp[0];
        assert_eq!(d.standpunkt_nr, "A1");
        assert_eq!(d.zeilen.len(), 1, "AP 5 gelöscht");
        assert_eq!(d.zeilen[0][4], "70.6000", "Hz korrigiert");
        let dat = ex.dat.as_ref().unwrap();
        assert_eq!(dat.zeilen.len(), 2, "NP 2 ersetzt");
        assert_eq!(dat.zelle(&dat.zeilen[0], "PunktNr"), "1a");
        assert_eq!(dat.zelle(&dat.zeilen[0], "ExzLaengs_m"), "0.100");
        assert_eq!(dat.zelle(&dat.zeilen[0], "ExzQuer_m"), "-0.050");
        assert_eq!(dat.zelle(&dat.zeilen[0], "StandpunktNr"), "A1");
        assert_eq!(aufstellungen(&v)[0].standpunkt_nr, "A1");
    }

    #[test]
    fn ih_korrektur_wirkt_auf_alle_messungen_der_aufstellung() {
        let mut v = beispiel();
        let mut i = eintrag(10, Ereignis::Ih);
        (i.aufstellung_id, i.standpunkt_nr, i.neu) = ("STP1-A".into(), "STP1".into(), "1.700".into());
        v.push(i);
        let ex = extrahiere(&v, None, &kopf(), &Rechenparameter::default());
        let dat = ex.dat.as_ref().unwrap();
        assert_eq!(dat.zelle(&dat.zeilen[0], "InstrumentenHoehe_m"), "1.700");
        assert_eq!(dat.zelle(&dat.zeilen[2], "InstrumentenHoehe_m"), "1.600", "andere Aufstellung unverändert");
    }

    #[test]
    fn altdaten_ohne_aufstellungsid_und_doppelte_nummern() {
        let mut a = messung(1, "", "ALT", "7", "NP", 10.0, 100.0, 5.0, 1.5);
        a.quelle = "Import".into();
        let mut b = messung(2, "", "", "8", "NP", 10.0, 100.0, 5.0, 1.5);
        b.quelle = "Import".into();
        let v = vec![a, b, messung(3, "X", "X", "9", "XX", 1.0, 1.0, 1.0, 1.0), stationierung(4, "S1", "S1", 0.0, 0.0, 0.0, 0.0, 1.5), stationierung(5, "S2", "S1", 5.0, 5.0, 0.0, 0.0, 1.5)];
        let ex = extrahiere(&v, None, &kopf(), &Rechenparameter::default());
        let dat = ex.dat.as_ref().unwrap();
        assert_eq!(dat.zeilen.len(), 1, "nur Messungen mit Standpunkt, unbekannte Typen übersprungen");
        assert_eq!(dat.zelle(&dat.zeilen[0], "AufstellungId"), "");
        assert!(ex.hinweise.iter().any(|h| h.contains("mehrfach") && h.contains("S1")), "{:?}", ex.hinweise);
    }

    #[test]
    fn dateien_werden_in_den_unterordner_geschrieben() {
        let dir = std::env::temp_dir().join(format!("fb-extr-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let protokoll = dir.join("Test5-Messprotokoll.csv");
        for e in beispiel() {
            mp::schreibe_in(&protokoll, "Test5", e).unwrap();
        }
        // Aus der Datei gelesen (Rundlauf Schreiben → Lesen → Extrahieren)
        let gelesen = mp::lese(&protokoll).unwrap();
        let ex = extrahiere(&gelesen, None, &kopf(), &Rechenparameter::default());
        let ordner = zielordner(&dir);
        let dateien = schreibe(&ex, &ordner, &kopf(), "20261124-120000").unwrap();
        assert_eq!(dateien.len(), 3, "stp, dat, kor");
        assert!(dateien.iter().all(|p| p.starts_with(&ordner) && p.exists()));
        let namen: Vec<String> = dateien.iter().map(|p| p.file_name().unwrap().to_string_lossy().into_owned()).collect();
        assert!(namen.contains(&"Test5-Extraktion-20261124-120000-STP1-A-stp.csv".to_string()), "{namen:?}");
        assert!(namen.contains(&"Test5-Extraktion-20261124-120000-dat.csv".to_string()));
        assert!(namen.contains(&"Test5-Extraktion-20261124-120000-kor.csv".to_string()));
        // Die Dateien lassen sich mit den vorhandenen Lesern wieder laden
        let dat = CsvDatei::laden(dateien.iter().find(|p| p.to_string_lossy().ends_with("-dat.csv")).unwrap()).unwrap();
        assert_eq!(dat.zeilen.len(), 3);
        let kor = kor_csv::lese_punkte(dateien.iter().find(|p| p.to_string_lossy().ends_with("-kor.csv")).unwrap()).unwrap();
        assert_eq!(kor.len(), 3);
        let stp = CsvDatei::laden(dateien.iter().find(|p| p.to_string_lossy().ends_with("-stp.csv")).unwrap()).unwrap();
        assert_eq!(stp.zeilen.len(), 2);
        assert_eq!(stp.stationierung.len(), 2);
        let _ = std::fs::remove_dir_all(dir);
    }
}
