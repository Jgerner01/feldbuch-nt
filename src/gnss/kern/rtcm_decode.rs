//! Zweck:    Inhaltliche Auswertung der RTCM-3-Nachrichten des SAPOS-Dienstes
//!           (Feldlayouts nach RTCM 10403).
//! Hardware: keine
//! Autor:    Johann Gerner
//!
//! Übernommen aus gnss-vermessung 0.4.0 (src/rtcm_decode.rs), eigenständig gepflegt in Feldbuch-NT.

use std::collections::BTreeMap;
use std::time::Instant;

const COORD_BITS: usize = 38;
const COORD_LSB_M: f64 = 0.0001;
/// Anzahl aufbewahrter Textmeldungen (1029)
const TEXT_LIMIT: usize = 20;

/// Bitweises Lesen, höchstwertiges Bit zuerst; liefert `None` am Datenende.
struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl Reader<'_> {
    fn u(&mut self, len: usize) -> Option<u64> {
        let mut v = 0;
        for i in self.pos..self.pos + len {
            let byte = *self.data.get(i / 8)?;
            v = (v << 1) | u64::from((byte >> (7 - i % 8)) & 1);
        }
        self.pos += len;
        Some(v)
    }

    fn skip(&mut self, len: usize) -> Option<()> {
        (self.pos + len <= self.data.len() * 8).then(|| self.pos += len)
    }

    fn coordinate(&mut self) -> Option<f64> {
        let raw = self.u(COORD_BITS)? as i64;
        Some(((raw << (64 - COORD_BITS)) >> (64 - COORD_BITS)) as f64 * COORD_LSB_M)
    }

    /// Zeichenkette mit vorangestellter Länge (8 Bit).
    fn text(&mut self) -> Option<String> {
        let n = self.u(8)? as usize;
        (0..n).map(|_| self.u(8).map(|c| c as u8 as char)).collect()
    }
}

/// Referenzstation, auf die sich die Korrekturen beziehen (1005/1006).
#[derive(Debug, Clone, PartialEq)]
pub struct Station {
    pub id: u16,
    pub ecef: [f64; 3],
    /// Antennenhöhe über dem Markenpunkt [m] (nur 1006)
    pub antenna_height: Option<f64>,
    /// `true`: berechnete (virtuelle) Station
    pub virtual_station: bool,
    pub itrf_year: u8,
}

/// Reale Station, auf der die virtuelle beruht (1032).
#[derive(Debug, Clone, PartialEq)]
pub struct Physical {
    pub id: u16,
    pub ecef: [f64; 3],
}

/// Inhalt der zuletzt empfangenen Beobachtungsnachricht eines Systems (MSM).
#[derive(Debug, Clone)]
pub struct Observations {
    pub message: u16,
    pub satellites: Vec<String>,
    pub signals: Vec<String>,
    pub at: Instant,
}

/// Gesammelte Aussagen über den Inhalt des Korrekturdatenstroms.
#[derive(Default)]
pub struct Content {
    pub station: Option<Station>,
    pub physical: Option<Physical>,
    /// Antennen- und Empfängerbeschreibung (Bezeichnung, Wert)
    pub equipment: BTreeMap<&'static str, String>,
    pub observations: BTreeMap<&'static str, Observations>,
    pub texts: Vec<String>,
    /// angekündigte Nachrichtenintervalle [s] aus 1013
    pub intervals: BTreeMap<u16, f64>,
    pub leap_seconds: Option<u8>,
    /// Transformationsnachrichten 1021–1027
    pub transformation: BTreeMap<u16, u64>,
    /// letzte GPS-L1C/A-Codemessungen der Station
    pub gps_ranges: Option<RangeEpoch>,
    /// Zähler neuer Codemessungs-Epochen
    pub range_epochs: u64,
    /// Transformationsnachrichten 1021, 1023, 1025 (ausgewertet)
    pub transform: crate::gnss::kern::transform::Transformation,
}

/// Codemessungen einer Epoche: GPS-Sekunde der Woche [ms] und (PRN, Pseudostrecke [m]).
#[derive(Debug, Clone)]
pub struct RangeEpoch {
    pub tow_ms: u32,
    pub ranges: Vec<(u8, f64)>,
}

/// Lichtweg in einer Millisekunde [m]
const RANGE_MS_M: f64 = crate::gnss::kern::spp::CLIGHT / 1000.0;
/// Signal GPS L1C/A in der MSM-Signalmaske
const GPS_L1CA: u32 = 2;
const ROUGH_INVALID: u64 = 255;

pub fn message_name(nr: u16) -> &'static str {
    match nr {
        1001..=1002 => "GPS L1 Beobachtungen",
        1003..=1004 => "GPS L1/L2 Beobachtungen",
        1005 => "Position der Referenzstation",
        1006 => "Position der Referenzstation mit Antennenhöhe",
        1007 => "Antennenbeschreibung",
        1008 => "Antennenbeschreibung mit Seriennummer",
        1009..=1012 => "GLONASS Beobachtungen",
        1013 => "Systemparameter",
        1014..=1017 => "Netz-RTK-Korrekturdifferenzen",
        1019 => "GPS Ephemeriden",
        1020 => "GLONASS Ephemeriden",
        1021 => "Transformation: Helmert/Molodenski",
        1022 => "Transformation: Molodenski-Badekas",
        1023 => "Transformation: Residuengitter (ellipsoidisch)",
        1024 => "Transformation: Residuengitter (eben)",
        1025..=1027 => "Transformation: Abbildungsparameter",
        1029 => "Textmeldung",
        1030 => "GPS Netz-Residuen",
        1031 => "GLONASS Netz-Residuen",
        1032 => "Position der physischen Referenzstation",
        1033 => "Empfänger- und Antennenbeschreibung",
        1034..=1039 => "Netz-RTK FKP-Gradienten",
        1042..=1046 => "Ephemeriden",
        1071..=1077 => "GPS Beobachtungen (MSM)",
        1081..=1087 => "GLONASS Beobachtungen (MSM)",
        1091..=1097 => "Galileo Beobachtungen (MSM)",
        1101..=1107 => "SBAS Beobachtungen (MSM)",
        1111..=1117 => "QZSS Beobachtungen (MSM)",
        1121..=1127 => "BeiDou Beobachtungen (MSM)",
        1230 => "GLONASS Code-Phasen-Biase",
        4001..=4095 => "herstellerspezifisch",
        _ => "unbekannt",
    }
}

/// Was die MSM-Stufe an Messgrößen enthält.
fn msm_content(level: u16) -> &'static str {
    match level {
        1 => "Code",
        2 => "Phase",
        3 => "Code, Phase",
        4 => "Code, Phase, Signalstärke",
        5 => "Code, Phase, Doppler, Signalstärke",
        6 => "Code, Phase, Signalstärke (hochauflösend)",
        _ => "Code, Phase, Doppler, Signalstärke (hochauflösend)",
    }
}

/// System, Kennbuchstabe und Nummer des ersten Satelliten einer MSM-Gruppe.
fn msm_system(nr: u16) -> Option<(&'static str, char, u32)> {
    if !(1..=7).contains(&(nr % 10)) {
        return None;
    }
    Some(match nr / 10 {
        107 => ("GPS", 'G', 1),
        108 => ("GLONASS", 'R', 1),
        109 => ("Galileo", 'E', 1),
        110 => ("SBAS", 'S', 20),
        111 => ("QZSS", 'J', 1),
        112 => ("BeiDou", 'C', 1),
        _ => return None,
    })
}

/// RINEX-Kennung eines MSM-Signals (Nummer 1..32 der Signalmaske).
fn signal_name(system: &str, id: u32) -> String {
    let table: &[(u32, &str)] = match system {
        "GPS" | "QZSS" => &[
            (2, "1C"),
            (3, "1P"),
            (4, "1W"),
            (8, "2C"),
            (9, "2P"),
            (10, "2W"),
            (15, "2S"),
            (16, "2L"),
            (17, "2X"),
            (22, "5I"),
            (23, "5Q"),
            (24, "5X"),
            (30, "1S"),
            (31, "1L"),
            (32, "1X"),
        ],
        "GLONASS" => &[(2, "1C"), (3, "1P"), (8, "2C"), (9, "2P")],
        "Galileo" => &[
            (2, "1C"),
            (3, "1A"),
            (4, "1B"),
            (5, "1X"),
            (6, "1Z"),
            (8, "6C"),
            (9, "6A"),
            (10, "6B"),
            (11, "6X"),
            (12, "6Z"),
            (14, "7I"),
            (15, "7Q"),
            (16, "7X"),
            (18, "8I"),
            (19, "8Q"),
            (20, "8X"),
            (22, "5I"),
            (23, "5Q"),
            (24, "5X"),
        ],
        "BeiDou" => &[
            (2, "2I"),
            (3, "2Q"),
            (4, "2X"),
            (8, "6I"),
            (9, "6Q"),
            (10, "6X"),
            (14, "7I"),
            (15, "7Q"),
            (16, "7X"),
            (22, "5D"),
            (23, "5P"),
            (24, "5X"),
            (30, "1D"),
            (31, "1P"),
            (32, "1X"),
        ],
        "SBAS" => &[(2, "1C"), (22, "5I"), (23, "5Q"), (24, "5X")],
        _ => &[],
    };
    table
        .iter()
        .find(|t| t.0 == id)
        .map_or_else(|| format!("Sig{id}"), |t| t.1.to_string())
}

/// GPS-L1C/A-Pseudostrecken aus dem Daten­teil einer MSM4–7 (RTCM 10403, DF397/398/400/405).
fn msm_ranges(level: u16, sat_mask: u64, signal_mask: u64, r: &mut Reader) -> Option<Vec<(u8, f64)>> {
    if !(4..=7).contains(&level) {
        return None;
    }
    let sats: Vec<u32> = set_bits(sat_mask, 64).collect();
    let signals: Vec<u32> = set_bits(signal_mask, 32).collect();
    let cells = sats.len() * signals.len();
    if cells > 64 {
        return None;
    }
    let mut cell = Vec::with_capacity(cells);
    for _ in 0..cells {
        cell.push(r.u(1)? == 1);
    }
    let extended = matches!(level, 5 | 7);
    let mut rough_ms = Vec::new();
    for _ in &sats {
        rough_ms.push(r.u(8)?);
    }
    if extended {
        r.skip(4 * sats.len())?;
    }
    let mut rough_mod = Vec::new();
    for _ in &sats {
        rough_mod.push(r.u(10)? as f64 / 1024.0);
    }
    if extended {
        r.skip(14 * sats.len())?;
    }
    let (bits, scale) = if level >= 6 { (20, 2f64.powi(-29)) } else { (15, 2f64.powi(-24)) };
    let invalid = -(1i64 << (bits - 1));
    let mut out = Vec::new();
    for (i, prn) in sats.iter().enumerate() {
        for (j, sig) in signals.iter().enumerate() {
            if !cell[i * signals.len() + j] {
                continue;
            }
            let raw = r.u(bits)? as i64;
            let fine = (raw << (64 - bits)) >> (64 - bits);
            if *sig == GPS_L1CA && rough_ms[i] != ROUGH_INVALID && fine != invalid {
                let ms = rough_ms[i] as f64 + rough_mod[i] + fine as f64 * scale;
                out.push((*prn as u8, ms * RANGE_MS_M));
            }
        }
    }
    Some(out)
}

/// Nummern der gesetzten Bits einer Maske, beginnend bei 1.
fn set_bits(mask: u64, len: u32) -> impl Iterator<Item = u32> {
    (1..=len).filter(move |i| mask & (1 << (len - i)) != 0)
}

impl Content {
    /// Wertet die Nutzdaten einer Nachricht aus und liefert eine Zeile Klartext.
    pub fn apply(&mut self, payload: &[u8]) -> String {
        let mut r = Reader {
            data: payload,
            pos: 0,
        };
        let Some(nr) = r.u(12).map(|v| v as u16) else {
            return "Nachricht zu kurz".into();
        };
        let name = message_name(nr);
        if matches!(nr, 1021 | 1023 | 1025) {
            *self.transformation.entry(nr).or_default() += 1;
            return match self.transform.apply(nr, payload) {
                Some(text) => format!("{nr} {name}: {text}"),
                None => format!("{nr} {name}: unvollständig ({} Byte)", payload.len()),
            };
        }
        match self.decode(nr, &mut r) {
            Some(detail) if !detail.is_empty() => format!("{nr} {name}: {detail}"),
            Some(_) => format!("{nr} {name} ({} Byte)", payload.len()),
            None => format!("{nr} {name}: unvollständig ({} Byte)", payload.len()),
        }
    }

    fn decode(&mut self, nr: u16, r: &mut Reader) -> Option<String> {
        if let Some((system, letter, first)) = msm_system(nr) {
            return self.msm(nr, system, letter, first, r);
        }
        Some(match nr {
            1005 | 1006 => {
                let id = r.u(12)? as u16;
                let itrf_year = r.u(6)? as u8;
                r.u(3)?; // Systemkennzeichen GPS, GLONASS, Galileo
                let virtual_station = r.u(1)? == 1;
                let x = r.coordinate()?;
                r.u(2)?;
                let y = r.coordinate()?;
                r.u(2)?;
                let z = r.coordinate()?;
                let antenna_height = if nr == 1006 {
                    Some(r.u(16)? as f64 * COORD_LSB_M)
                } else {
                    None
                };
                self.station = Some(Station {
                    id,
                    ecef: [x, y, z],
                    antenna_height,
                    virtual_station,
                    itrf_year,
                });
                let kind = if virtual_station {
                    "virtuell"
                } else {
                    "physisch"
                };
                let height = antenna_height
                    .map(|h| format!(", Antennenhöhe {h:.4} m"))
                    .unwrap_or_default();
                format!("Station {id} ({kind}) X {x:.4} Y {y:.4} Z {z:.4}{height}")
            }
            1032 => {
                r.u(12)?; // Kennung der virtuellen Station
                let id = r.u(12)? as u16;
                r.u(6)?;
                let ecef = [r.coordinate()?, r.coordinate()?, r.coordinate()?];
                self.physical = Some(Physical { id, ecef });
                format!(
                    "Station {id} X {:.4} Y {:.4} Z {:.4}",
                    ecef[0], ecef[1], ecef[2]
                )
            }
            1007 | 1008 | 1033 => {
                r.u(12)?;
                let mut parts = vec![("Antenne", r.text()?)];
                let setup = r.u(8)?;
                parts.push(("Antennen-Setup", setup.to_string()));
                if nr != 1007 {
                    parts.push(("Antennen-Seriennummer", r.text()?));
                }
                if nr == 1033 {
                    parts.push(("Empfänger", r.text()?));
                    parts.push(("Empfänger-Firmware", r.text()?));
                    parts.push(("Empfänger-Seriennummer", r.text()?));
                }
                parts.retain(|p| !p.1.trim().is_empty());
                let line = parts
                    .iter()
                    .map(|p| format!("{} {}", p.0, p.1))
                    .collect::<Vec<_>>()
                    .join(", ");
                self.equipment.extend(parts);
                line
            }
            1029 => {
                let id = r.u(12)?;
                r.u(16 + 17 + 7)?; // Datum, Uhrzeit, Zeichenzahl
                let n = r.u(8)? as usize;
                let bytes: Vec<u8> = (0..n)
                    .map(|_| r.u(8).map(|c| c as u8))
                    .collect::<Option<_>>()?;
                let text = String::from_utf8_lossy(&bytes).into_owned();
                if !self.texts.contains(&text) {
                    self.texts.push(text.clone());
                    if self.texts.len() > TEXT_LIMIT {
                        self.texts.remove(0);
                    }
                }
                format!("Station {id}: „{text}“")
            }
            1013 => {
                r.u(12 + 16 + 17)?;
                let n = r.u(5)?;
                let leap = r.u(8)? as u8;
                self.leap_seconds = Some(leap);
                for _ in 0..n {
                    let id = r.u(12)? as u16;
                    r.u(1)?;
                    self.intervals.insert(id, r.u(16)? as f64 * 0.1);
                }
                format!("Schaltsekunden {leap}, Intervalle für {n} Nachrichten angekündigt")
            }
            1004 | 1012 => {
                let id = r.u(12)?;
                r.u(if nr == 1004 { 30 } else { 27 })?;
                r.u(1)?;
                format!("Station {id}, {} Satelliten", r.u(5)?)
            }
            1021..=1027 => {
                *self.transformation.entry(nr).or_default() += 1;
                "Koordinatentransformation (noch nicht angewendet)".into()
            }
            _ => String::new(),
        })
    }

    fn msm(
        &mut self,
        nr: u16,
        system: &'static str,
        letter: char,
        first: u32,
        r: &mut Reader,
    ) -> Option<String> {
        let id = r.u(12)?;
        let tow_ms = r.u(30)? as u32;
        // Mehrfachbit, IODS, Reserve, Uhrenkennzeichen, Glättung
        r.u(1 + 3 + 7 + 2 + 2 + 1 + 3)?;
        let sat_mask = r.u(64)?;
        let signal_mask = r.u(32)?;
        if system == "GPS" {
            if let Some(ranges) = msm_ranges(nr % 10, sat_mask, signal_mask, r) {
                self.gps_ranges = Some(RangeEpoch { tow_ms, ranges });
                self.range_epochs += 1;
            }
        }
        let satellites: Vec<String> = set_bits(sat_mask, 64)
            .map(|i| format!("{letter}{:02}", i + first - 1))
            .collect();
        let signals: Vec<String> = set_bits(signal_mask, 32)
            .map(|i| signal_name(system, i))
            .collect();
        let line = format!(
            "Station {id}, MSM{} ({}), {} Satelliten, Signale {}",
            nr % 10,
            msm_content(nr % 10),
            satellites.len(),
            signals.join(" ")
        );
        self.observations.insert(
            system,
            Observations {
                message: nr,
                satellites,
                signals,
                at: Instant::now(),
            },
        );
        Some(line)
    }

    /// Kurze Zusammenfassung dessen, was übertragen wird.
    pub fn summary(&self) -> Vec<String> {
        let mut out = Vec::new();
        match &self.station {
            Some(s) if s.virtual_station => out.push(format!(
                "Virtuelle Referenzstation {} (vom Netz berechnet)",
                s.id
            )),
            Some(s) => out.push(format!(
                "Referenzstation {} (als physisch gekennzeichnet)",
                s.id
            )),
            None => out.push("Noch keine Stationsposition (1005/1006) empfangen".into()),
        }
        if let Some(p) = &self.physical {
            out.push(format!("Zugehörige physische Station {} (1032)", p.id));
        }
        if !self.observations.is_empty() {
            let parts: Vec<String> = self
                .observations
                .iter()
                .map(|(n, o)| format!("{n} {}", o.satellites.len()))
                .collect();
            let total: usize = self.observations.values().map(|o| o.satellites.len()).sum();
            out.push(format!(
                "Beobachtungsdaten für {total} Satelliten: {}",
                parts.join(", ")
            ));
        }
        out.push(if self.transformation.is_empty() {
            "Keine Transformationsnachrichten (1021–1027) im Strom".into()
        } else {
            let numbers: Vec<String> = self.transformation.keys().map(u16::to_string).collect();
            format!(
                "Transformationsnachrichten vorhanden: {}",
                numbers.join(", ")
            )
        });
        out
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Baut Nutzdaten aus (Wert, Bitanzahl).
    pub(crate) fn build(fields: &[(u64, usize)]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut pos = 0;
        for &(value, len) in fields {
            for i in (0..len).rev() {
                if pos % 8 == 0 {
                    out.push(0);
                }
                if let Some(b) = out.last_mut() {
                    *b |= (((value >> i) & 1) as u8) << (7 - pos % 8);
                }
                pos += 1;
            }
        }
        out
    }

    fn coord(v: f64) -> (u64, usize) {
        (
            ((v / COORD_LSB_M).round() as i64 as u64) & ((1 << COORD_BITS) - 1),
            COORD_BITS,
        )
    }

    #[test]
    fn station_virtual_and_physical() {
        let mut c = Content::default();
        let p = build(&[
            (1006, 12),
            (7, 12),
            (0, 6),
            (7, 3),
            (1, 1),
            coord(4_100_000.1234),
            (0, 2),
            coord(-950_000.5),
            (0, 2),
            coord(4_780_000.0),
            (15_000, 16),
        ]);
        let line = c.apply(&p);
        let s = c.station.clone().unwrap();
        assert!(s.virtual_station && s.id == 7);
        assert!((s.ecef[0] - 4_100_000.1234).abs() < 1e-6 && (s.ecef[1] + 950_000.5).abs() < 1e-6);
        assert_eq!(s.antenna_height, Some(1.5));
        assert!(line.contains("virtuell"));
        let p = build(&[
            (1032, 12),
            (7, 12),
            (253, 12),
            (0, 6),
            coord(4_090_000.0),
            coord(960_000.0),
            coord(4_790_000.0),
        ]);
        c.apply(&p);
        assert_eq!(c.physical.as_ref().unwrap().id, 253);
        assert!((c.physical.unwrap().ecef[2] - 4_790_000.0).abs() < 1e-6);
    }

    #[test]
    fn msm_masks() {
        let mut c = Content::default();
        // Galileo MSM4 mit Satelliten 1 und 36, Signalen 1C und 7Q
        let sats = (1u64 << 63) | (1 << (64 - 36));
        let sigs = (1u64 << (32 - 2)) | (1 << (32 - 15));
        let p = build(&[(1094, 12), (7, 12), (0, 49), (sats, 64), (sigs, 32)]);
        let line = c.apply(&p);
        let o = &c.observations["Galileo"];
        assert_eq!(o.satellites, ["E01", "E36"]);
        assert_eq!(o.signals, ["1C", "7Q"]);
        assert!(
            line.contains("MSM4") && line.contains("2 Satelliten"),
            "{line}"
        );
    }

    #[test]
    fn msm_pseudoranges() {
        let mut c = Content::default();
        // GPS MSM4: Satelliten 3 und 7, Signale 1C und 2W, alle vier Zellen belegt
        let sats = (1u64 << (64 - 3)) | (1 << (64 - 7));
        let sigs = (1u64 << (32 - 2)) | (1 << (32 - 10));
        let fine = |v: i64| ((v as u64) & 0x7FFF, 15);
        let p = build(&[
            (1074, 12), (7, 12), (388_800_000, 30), (0, 19), (sats, 64), (sigs, 32), (0b1111, 4),
            (70, 8), (72, 8), (512, 10), (0, 10),
            fine(1 << 12), fine(0), fine(-(1 << 12)), fine(-16384),
        ]);
        c.apply(&p);
        let e = c.gps_ranges.as_ref().unwrap();
        assert_eq!(e.tow_ms, 388_800_000);
        assert_eq!(e.ranges.len(), 2);
        let ms = |v: f64| v * crate::gnss::kern::spp::CLIGHT / 1000.0;
        assert_eq!(e.ranges[0].0, 3);
        assert!((e.ranges[0].1 - ms(70.5 + 4096.0 / 16_777_216.0)).abs() < 1e-6);
        assert_eq!(e.ranges[1].0, 7);
        assert!((e.ranges[1].1 - ms(72.0 - 4096.0 / 16_777_216.0)).abs() < 1e-6);
    }

    #[test]
    fn text_and_short_message() {
        let mut c = Content::default();
        let mut f = vec![
            (1029, 12),
            (0, 12),
            (0, 40),
            (2, 8),
            (u64::from(b'H'), 8),
            (u64::from(b'i'), 8),
        ];
        c.apply(&build(&f));
        assert_eq!(c.texts, ["Hi"]);
        f.truncate(3);
        assert!(c.apply(&build(&f)).contains("unvollständig"));
        assert!(c.apply(&[]).contains("zu kurz"));
    }
}
