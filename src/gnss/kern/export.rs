//! Zweck:    Speichern der Punkte als <Name>-kor.csv und parallel als <Name>.kor (Format Feldbuch-NT),
//!           dazu ein Messprotokoll <Name>-protokoll.csv mit allen Qualitätsangaben.
//! Hardware: keine
//! Autor:    Johann Gerner
//!
//! Übernommen aus gnss-vermessung 0.4.0 (src/export.rs), eigenständig gepflegt in Feldbuch-NT.

use anyhow::{Context, Result, bail};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Dezimalstellen der Koordinaten und Höhen in beiden Koordinatendateien.
const DECIMALS: usize = 3;
const KERN: [&str; 5] = ["PunktNr", "X", "Y", "Z", "Code"];
const ZUSATZ: [&str; 3] = ["Bemerkung", "Quelle", "Zeitstempel"];
/// Rechtswerte darüber enthalten die Zonenkennzahl (32xxxxxx).
const PREFIX_LIMIT: f64 = 10_000_000.0;

/// Kopfangaben der Koordinatendatei.
#[derive(Debug, Clone)]
pub struct Meta {
    pub project: String,
    pub operator: String,
    pub date: String,
    pub coord_system: String,
    pub height_system: String,
}

/// Ein zu speichernder Punkt. `x` mit oder ohne Zonenkennzahl, wie eingestellt.
#[derive(Debug, Clone)]
pub struct Point {
    pub nr: String,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub code: String,
    pub remark: String,
    pub source: String,
    pub timestamp: String,
    /// weitere Angaben für das Messprotokoll (Spalte, Wert)
    pub protocol: Vec<(&'static str, String)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Stored {
    pub nr: String,
    pub x: f64,
    pub y: f64,
    pub z: Option<f64>,
    pub code: String,
}

#[derive(Debug, Clone)]
pub struct Files {
    pub csv: PathBuf,
    pub kor: PathBuf,
    pub protocol: PathBuf,
}

/// Leitet die drei Dateinamen aus einem beliebigen davon oder einem Basisnamen ab.
pub fn files(path: &Path) -> Files {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let lower = name.to_lowercase();
    let mut base = name.as_str();
    for suffix in ["-protokoll.csv", "-kor.csv", ".csv", ".kor"] {
        if lower.ends_with(suffix) {
            base = &name[..name.len() - suffix.len()];
            break;
        }
    }
    Files {
        csv: path.with_file_name(format!("{base}-kor.csv")),
        kor: path.with_file_name(format!("{base}.kor")),
        protocol: path.with_file_name(format!("{base}-protokoll.csv")),
    }
}

/// Punktnummer und Code ohne Leer- und Trennzeichen (das .kor-Format trennt mit Leerzeichen).
pub fn clean(s: &str) -> String {
    s.trim()
        .chars()
        .map(|c| {
            if c.is_whitespace() || c == ';' || c == '"' {
                '_'
            } else {
                c
            }
        })
        .collect()
}

fn read_text(path: &Path) -> Result<Option<String>> {
    match fs::read_to_string(path) {
        Ok(s) => Ok(Some(s.trim_start_matches('\u{feff}').to_string())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e).with_context(|| format!("{} nicht lesbar", path.display())),
    }
}

/// Zerlegt eine CSV-Zeile; Felder dürfen in Anführungszeichen stehen.
fn split(line: &str) -> Vec<String> {
    let mut out = vec![String::new()];
    let mut quoted = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if quoted && chars.peek() == Some(&'"') => {
                chars.next();
                out.last_mut().into_iter().for_each(|f| f.push('"'));
            }
            '"' => quoted = !quoted,
            ';' if !quoted => out.push(String::new()),
            c => out.last_mut().into_iter().for_each(|f| f.push(c)),
        }
    }
    out.iter().map(|f| f.trim().to_string()).collect()
}

fn join(fields: &[String]) -> String {
    fields
        .iter()
        .map(|f| {
            if f.contains([';', '"', '\r', '\n']) {
                format!("\"{}\"", f.replace('"', "\"\"").replace(['\r', '\n'], " "))
            } else {
                f.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(";")
}

fn number(s: &str) -> Option<f64> {
    s.trim().replace(',', ".").parse().ok()
}

/// Kopfzeilenindex, Spaltennamen und Indizes der Datenzeilen.
struct Table {
    lines: Vec<String>,
    header: usize,
    columns: Vec<String>,
    data: Vec<usize>,
}

impl Table {
    fn parse(text: &str) -> Result<Table> {
        let lines: Vec<String> = text.lines().map(str::to_string).collect();
        let is_header = |l: &String| split(l).iter().any(|f| f.eq_ignore_ascii_case("PunktNr"));
        let header = lines
            .iter()
            .position(is_header)
            .context("keine Kopfzeile mit PunktNr gefunden")?;
        let columns = split(&lines[header]);
        for k in &KERN[..3] {
            if !columns.iter().any(|c| c.eq_ignore_ascii_case(k)) {
                bail!("Spalte {k} fehlt – keine Koordinatendatei");
            }
        }
        let mut data = Vec::new();
        for (i, l) in lines.iter().enumerate().skip(header + 1) {
            let t = l.trim();
            if t.eq_ignore_ascii_case("# STATIONIERUNG") {
                break;
            }
            if !t.is_empty() && !t.starts_with(['#', '%']) && t != "---" {
                data.push(i);
            }
        }
        Ok(Table {
            lines,
            header,
            columns,
            data,
        })
    }

    fn column(&self, name: &str) -> Option<usize> {
        self.columns
            .iter()
            .position(|c| c.eq_ignore_ascii_case(name))
    }

    fn cell(&self, fields: &[String], name: &str) -> String {
        self.column(name)
            .and_then(|i| fields.get(i))
            .cloned()
            .unwrap_or_default()
    }

    fn meta(&self, key: &str) -> Option<String> {
        self.lines[..self.header].iter().find_map(|l| {
            let (k, v) = l.split_once(':')?;
            k.trim()
                .eq_ignore_ascii_case(key)
                .then(|| v.trim().trim_end_matches([';', ',']).to_string())
        })
    }

    fn points(&self) -> Vec<Stored> {
        self.data
            .iter()
            .filter_map(|&i| {
                let f = split(&self.lines[i]);
                Some(Stored {
                    nr: self.cell(&f, "PunktNr"),
                    x: number(&self.cell(&f, "X"))?,
                    y: number(&self.cell(&f, "Y"))?,
                    z: number(&self.cell(&f, "Z")),
                    code: self.cell(&f, "Code"),
                })
            })
            .collect()
    }
}

/// Punkte aus einem .kor-Text (Nr X Y Z [Code]).
fn kor_points(text: &str) -> Vec<Stored> {
    text.lines()
        .filter(|l| !l.trim_start().starts_with(['%', '#', ';']))
        .filter_map(|l| {
            let p: Vec<&str> = l.split_whitespace().collect();
            Some(Stored {
                nr: p.first()?.to_string(),
                x: number(p.get(1)?)?,
                y: number(p.get(2)?)?,
                z: p.get(3).and_then(|v| number(v)),
                code: p.get(4).map(|c| c.to_string()).unwrap_or_default(),
            })
        })
        .collect()
}

/// Vorhandene Punkte; die CSV-Datei hat Vorrang vor der .kor-Datei.
pub fn load(path: &Path) -> Result<Vec<Stored>> {
    let f = files(path);
    if let Some(t) = read_text(&f.csv)? {
        return Ok(Table::parse(&t)
            .with_context(|| f.csv.display().to_string())?
            .points());
    }
    Ok(read_text(&f.kor)?
        .map(|t| kor_points(&t))
        .unwrap_or_default())
}

fn fixed(v: f64) -> String {
    format!("{v:.DECIMALS$}")
}

fn kor_line(nr: &str, x: &str, y: &str, z: &str, code: &str) -> String {
    format!("{nr:<7} {x:>15} {y:>15} {z:>10}    {code}")
        .trim_end()
        .to_string()
}

fn crlf(lines: &[String]) -> String {
    let mut s = lines.join("\r\n");
    s.push_str("\r\n");
    s
}

fn new_csv(meta: &Meta, existing: &[Stored]) -> Vec<String> {
    let mut lines = vec![
        "# METADATA".to_string(),
        "Formatversion: 1.0".to_string(),
        format!("Projekt: {}", meta.project),
        "Sensor: GPS/GNSS RTK".to_string(),
        format!("Bearbeiter: {}", meta.operator),
        format!("Datum: {}", meta.date),
        format!("Koordinatensystem: {}", meta.coord_system),
        format!("Hoehensystem: {}", meta.height_system),
        "---".to_string(),
        "# DATENTYP".to_string(),
        "Koordinaten".to_string(),
        "# DATEN".to_string(),
        KERN.iter()
            .chain(&ZUSATZ)
            .copied()
            .collect::<Vec<_>>()
            .join(";"),
    ];
    // Punkte einer allein vorhandenen .kor-Datei übernehmen
    for p in existing {
        let z = p.z.map(fixed).unwrap_or_default();
        lines.push(join(&[
            p.nr.clone(),
            fixed(p.x),
            fixed(p.y),
            z,
            p.code.clone(),
        ]));
    }
    lines
}

/// Schreibt über eine Zwischendatei, damit nie eine halbe Datei zurückbleibt.
fn write_atomic(path: &Path, text: &str) -> Result<()> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    fs::write(&tmp, text).with_context(|| format!("{} nicht schreibbar", tmp.display()))?;
    fs::rename(&tmp, path).with_context(|| {
        format!(
            "{} nicht ersetzbar (in anderem Programm geöffnet?)",
            path.display()
        )
    })
}

/// Speichert den Punkt in beide Koordinatendateien und das Protokoll.
/// Ein vorhandener Punkt gleicher Nummer wird nur mit `replace` überschrieben.
pub fn save(path: &Path, meta: &Meta, point: &Point, replace: bool) -> Result<()> {
    let f = files(path);
    if let Some(dir) = f.csv.parent().filter(|d| !d.as_os_str().is_empty()) {
        fs::create_dir_all(dir)?;
    }
    let nr = clean(&point.nr);
    if nr.is_empty() {
        bail!("Punktnummer fehlt");
    }
    let code = clean(&point.code);
    let kor_old = read_text(&f.kor)?;
    let mut lines = match read_text(&f.csv)? {
        Some(t) => t.lines().map(str::to_string).collect(),
        None => new_csv(
            meta,
            &kor_old.as_deref().map(kor_points).unwrap_or_default(),
        ),
    };
    let table = Table::parse(&lines.join("\n")).with_context(|| f.csv.display().to_string())?;

    // Bezugssysteme dürfen sich innerhalb einer Datei nicht mischen
    for (key, new) in [
        ("Koordinatensystem", &meta.coord_system),
        ("Hoehensystem", &meta.height_system),
    ] {
        if let Some(old) = table.meta(key).filter(|v| !v.is_empty()) {
            if !old.eq_ignore_ascii_case(new) {
                bail!(
                    "{key} der Datei ist „{old}“, die Messung liefert „{new}“. Bitte andere Datei wählen."
                );
            }
        }
    }
    if let Some(p) = table.points().first() {
        if (p.x >= PREFIX_LIMIT) != (point.x >= PREFIX_LIMIT) {
            bail!("Rechtswerte der Datei und der Messung unterscheiden sich in der Zonenkennzahl.");
        }
    }

    let existing = table.data.iter().copied().find(|&i| {
        table
            .cell(&split(&lines[i]), "PunktNr")
            .eq_ignore_ascii_case(&nr)
    });
    if existing.is_some() && !replace {
        bail!("Punktnummer {nr} ist bereits vorhanden");
    }
    let values = [
        ("PunktNr", nr.clone()),
        ("X", fixed(point.x)),
        ("Y", fixed(point.y)),
        ("Z", fixed(point.z)),
        ("Code", code.clone()),
        ("Bemerkung", point.remark.clone()),
        ("Quelle", point.source.clone()),
        ("Zeitstempel", point.timestamp.clone()),
    ];
    let mut fields = existing.map(|i| split(&lines[i])).unwrap_or_default();
    fields.resize(table.columns.len(), String::new());
    for (name, value) in values {
        if let Some(i) = table.column(name) {
            fields[i] = value;
        }
    }
    let row = join(&fields);
    match existing {
        Some(i) => lines[i] = row,
        None => {
            let at = table.data.last().copied().unwrap_or(table.header) + 1;
            lines.insert(at, row);
        }
    }

    let new_line = kor_line(
        &nr,
        &fixed(point.x),
        &fixed(point.y),
        &fixed(point.z),
        &code,
    );
    let kor: Vec<String> = match kor_old {
        Some(t) => {
            let mut kor: Vec<String> = t.lines().map(str::to_string).collect();
            let found = kor.iter().position(|l| {
                !l.trim_start().starts_with(['%', '#', ';'])
                    && l.split_whitespace()
                        .next()
                        .is_some_and(|n| n.eq_ignore_ascii_case(&nr))
            });
            match found {
                Some(_) if !replace => {
                    bail!("Punktnummer {nr} ist bereits in der .kor-Datei vorhanden")
                }
                Some(i) => kor[i] = new_line,
                None => kor.push(new_line),
            }
            kor
        }
        // .kor fehlt: vollständig aus der CSV-Datei erzeugen
        None => Table::parse(&lines.join("\n"))?
            .points()
            .iter()
            .map(|p| {
                kor_line(
                    &p.nr,
                    &fixed(p.x),
                    &fixed(p.y),
                    &fixed(p.z.unwrap_or(0.0)),
                    &p.code,
                )
            })
            .collect(),
    };

    // Protokoll zuerst: es wird nur ergänzt und hält jede Messung fest
    append_protocol(&f.protocol, point, &nr, replace && existing.is_some())?;
    write_atomic(&f.csv, &crlf(&lines))?;
    write_atomic(&f.kor, &crlf(&kor))
}

fn append_protocol(path: &Path, point: &Point, nr: &str, replaced: bool) -> Result<()> {
    let mut head = vec![
        "PunktNr".to_string(),
        "X".into(),
        "Y".into(),
        "Z".into(),
        "Code".into(),
        "Bemerkung".into(),
        "Quelle".into(),
        "Zeitstempel".into(),
        "Ersetzt".into(),
    ];
    let mut row = vec![
        nr.to_string(),
        fixed(point.x),
        fixed(point.y),
        fixed(point.z),
        clean(&point.code),
        point.remark.clone(),
        point.source.clone(),
        point.timestamp.clone(),
        if replaced { "ja".into() } else { "nein".into() },
    ];
    for (k, v) in &point.protocol {
        head.push(k.to_string());
        row.push(v.clone());
    }
    let new = !path.exists();
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .with_context(|| {
            format!(
                "{} nicht schreibbar (in anderem Programm geöffnet?)",
                path.display()
            )
        })?;
    if new {
        write!(file, "{}\r\n", join(&head))?;
    }
    write!(file, "{}\r\n", join(&row))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("gnss-export-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn meta() -> Meta {
        Meta {
            project: "Test".into(),
            operator: String::new(),
            date: "2026-11-26".into(),
            coord_system: "ETRS89/UTM32".into(),
            height_system: "DHHN2016".into(),
        }
    }

    fn point(nr: &str, x: f64) -> Point {
        Point {
            nr: nr.into(),
            x,
            y: 5_416_086.479,
            z: 323.439,
            code: "030".into(),
            remark: String::new(),
            source: "GNSS".into(),
            timestamp: "2026-11-26T12:00:00+01:00".into(),
            protocol: vec![("Epochen", "30".into())],
        }
    }

    #[test]
    fn names() {
        for n in ["a/Bau", "a/Bau.kor", "a/Bau-kor.csv", "a/Bau.csv"] {
            assert_eq!(files(Path::new(n)).csv, Path::new("a/Bau-kor.csv"), "{n}");
            assert_eq!(files(Path::new(n)).kor, Path::new("a/Bau.kor"), "{n}");
        }
    }

    #[test]
    fn writes_both_formats_and_protects_points() {
        let d = dir("write");
        let base = d.join("Bau");
        save(&base, &meta(), &point("1", 790_071.472), false).unwrap();
        save(&base, &meta(), &point("2", 790_084.634), false).unwrap();
        assert!(save(&base, &meta(), &point("2", 1.0), false).is_err());
        save(&base, &meta(), &point("2", 790_085.000), true).unwrap();
        let f = files(&base);
        let kor = fs::read_to_string(&f.kor).unwrap();
        assert_eq!(kor.lines().count(), 2);
        assert_eq!(
            kor.lines()
                .nth(1)
                .unwrap()
                .split_whitespace()
                .collect::<Vec<_>>(),
            ["2", "790085.000", "5416086.479", "323.439", "030"]
        );
        let p = load(&base).unwrap();
        assert_eq!(p.len(), 2);
        assert_eq!(p[1].x, 790_085.0);
        assert_eq!(fs::read_to_string(&f.protocol).unwrap().lines().count(), 4);
        // anderes Höhensystem wird abgewiesen
        let other = Meta {
            height_system: "ellipsoidisch GRS80".into(),
            ..meta()
        };
        assert!(save(&base, &other, &point("3", 790_000.0), false).is_err());
        // Zonenkennzahl darf nicht gemischt werden
        assert!(save(&base, &meta(), &point("3", 32_790_000.0), false).is_err());
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn appends_to_existing_feldbuch_file() {
        let d = dir("append");
        let csv = d.join("Alt-kor.csv");
        fs::write(&csv, "\u{feff}# METADATA\r\nProjekt: Alt\r\n---\r\n# DATENTYP\r\nKoordinaten\r\n# DATEN\r\nPunktNr; X; Y; Z; Code\r\n1; 790071.472; 5416086.479; 323.439; 030\r\n# STATIONIERUNG\r\nx\r\n").unwrap();
        save(&csv, &meta(), &point("2", 790_084.634), false).unwrap();
        let text = fs::read_to_string(&csv).unwrap();
        let l: Vec<&str> = text.lines().collect();
        assert_eq!(l[1], "Projekt: Alt");
        assert_eq!(l[7], "1; 790071.472; 5416086.479; 323.439; 030");
        assert_eq!(l[8], "2;790084.634;5416086.479;323.439;030");
        assert_eq!(l[9], "# STATIONIERUNG");
        assert_eq!(
            fs::read_to_string(d.join("Alt.kor"))
                .unwrap()
                .lines()
                .count(),
            2
        );
        let _ = fs::remove_dir_all(d);
    }
}
