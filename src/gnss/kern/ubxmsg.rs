//! Zweck:    Dekodierung der UBX-Nachrichten (Feldlayouts nach ZED-F9P Interface Description, HPG 1.3x).
//! Hardware: simpleRTK2B (u-blox ZED-F9P)
//! Autor:    Johann Gerner
//!
//! Übernommen aus gnss-vermessung 0.4.0 (src/ubxmsg.rs), eigenständig gepflegt in Feldbuch-NT.

fn i1(p: &[u8], o: usize) -> i8 {
    p[o] as i8
}
fn u2(p: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([p[o], p[o + 1]])
}
fn i2(p: &[u8], o: usize) -> i16 {
    u2(p, o) as i16
}
fn u4(p: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([p[o], p[o + 1], p[o + 2], p[o + 3]])
}
fn i4(p: &[u8], o: usize) -> i32 {
    u4(p, o) as i32
}
fn r4(p: &[u8], o: usize) -> f64 {
    f32::from_bits(u4(p, o)) as f64
}
fn text(p: &[u8]) -> String {
    String::from_utf8_lossy(p)
        .trim_end_matches('\0')
        .to_string()
}

#[derive(Debug, Clone, Default)]
pub struct NavPvt {
    pub itow: u32,
    pub fix_type: u8,
    pub fix_ok: bool,
    pub diff: bool,
    /// 0 = keine Trägerphasenlösung, 1 = float, 2 = fixed
    pub carrier: u8,
    pub num_sv: u8,
    pub lat: f64,
    pub lon: f64,
    pub height: f64,
    pub h_acc: f64,
    pub v_acc: f64,
    pub speed: f64,
    pub pdop: f64,
}

#[derive(Debug, Clone, Default)]
pub struct HpPos {
    pub itow: u32,
    pub valid: bool,
    pub lat: f64,
    pub lon: f64,
    pub height: f64,
}

/// Kovarianzmatrix der Position im System Nord, Ost, Tief [m²].
#[derive(Debug, Clone, Default)]
pub struct Cov {
    pub itow: u32,
    pub valid: bool,
    pub nn: f64,
    pub ne: f64,
    pub ee: f64,
    pub dd: f64,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Dop {
    pub g: f64,
    pub p: f64,
    pub t: f64,
    pub v: f64,
    pub h: f64,
}

#[derive(Debug, Clone, Default)]
pub struct Sat {
    pub gnss: u8,
    pub sv: u8,
    pub cno: u8,
    /// `None`, wenn der Empfänger die Richtung nicht kennt
    pub elev: Option<f64>,
    pub azim: f64,
    pub quality: u8,
    pub used: bool,
    pub health: u8,
    pub diff_corr: bool,
    pub orbit_source: u8,
    pub rtcm_used: bool,
}

#[derive(Debug, Clone, Default)]
pub struct Signal {
    pub gnss: u8,
    pub sv: u8,
    pub sig: u8,
    pub cno: u8,
    pub used: bool,
}

#[derive(Debug, Clone, Default)]
pub struct Orbit {
    pub gnss: u8,
    pub sv: u8,
    /// 0 unbekannt, 1 unter Horizont, 2 sichtbar, 3 über der Elevationsmaske
    pub visibility: u8,
}

#[derive(Debug, Clone, Default)]
pub struct TimeGps {
    pub itow: u32,
    pub ftow_ns: i32,
    pub week: i16,
    pub leap_s: i8,
    pub tow_valid: bool,
    pub week_valid: bool,
    pub leap_valid: bool,
    pub acc_ns: u32,
}

#[derive(Debug, Clone, Default)]
pub struct TimeUtc {
    pub acc_ns: u32,
    pub nano: i32,
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub min: u8,
    pub sec: u8,
    pub valid: bool,
}

#[derive(Debug, Clone, Default)]
pub struct TimeLs {
    pub current: i8,
    pub current_valid: bool,
    pub change: i8,
    pub seconds_to_event: i32,
    pub event_valid: bool,
}

#[derive(Debug, Clone, Default)]
pub struct RtcmUse {
    pub crc_failed: bool,
    /// 0 unbekannt, 1 nicht verwendet, 2 verwendet
    pub used: u8,
    pub msg_type: u16,
}

#[derive(Debug, Clone, Default)]
pub struct RfBlock {
    pub block: u8,
    /// 0 unbekannt, 1 ok, 2 Warnung, 3 kritisch
    pub jamming: u8,
    /// 0 Start, 1 unbekannt, 2 ok, 3 Kurzschluss, 4 offen
    pub antenna: u8,
    pub cw_indicator: u8,
}

#[derive(Debug, Clone)]
pub enum Msg {
    Pvt(NavPvt),
    HpPos(HpPos),
    Cov(Cov),
    Dop(Dop),
    Sat {
        itow: u32,
        sats: Vec<Sat>,
    },
    Sig(Vec<Signal>),
    Orb(Vec<Orbit>),
    TimeGps(TimeGps),
    TimeUtc(TimeUtc),
    TimeLs(TimeLs),
    /// Zeit bis zum ersten Fix [s] und Laufzeit [s]
    Status {
        ttff_s: f64,
        uptime_s: f64,
    },
    RtcmUse(RtcmUse),
    Rf(Vec<RfBlock>),
    /// Navigationsdaten-Unterrahmen: GNSS, Satellit, Datenworte
    Sfrbx { gnss: u8, sv: u8, words: Vec<u32> },
    Version {
        software: String,
        hardware: String,
        extensions: Vec<String>,
    },
    Ack {
        class: u8,
        id: u8,
        ok: bool,
    },
    ValGet(Vec<(u32, u64)>),
}

pub fn name(class: u8, id: u8) -> String {
    let n = match (class, id) {
        (0x01, 0x03) => "NAV-STATUS",
        (0x01, 0x04) => "NAV-DOP",
        (0x01, 0x07) => "NAV-PVT",
        (0x01, 0x14) => "NAV-HPPOSLLH",
        (0x01, 0x20) => "NAV-TIMEGPS",
        (0x01, 0x21) => "NAV-TIMEUTC",
        (0x01, 0x26) => "NAV-TIMELS",
        (0x01, 0x34) => "NAV-ORB",
        (0x01, 0x35) => "NAV-SAT",
        (0x01, 0x36) => "NAV-COV",
        (0x01, 0x43) => "NAV-SIG",
        (0x02, 0x32) => "RXM-RTCM",
        (0x05, 0x00) => "ACK-NAK",
        (0x05, 0x01) => "ACK-ACK",
        (0x06, 0x8A) => "CFG-VALSET",
        (0x06, 0x8B) => "CFG-VALGET",
        (0x0A, 0x04) => "MON-VER",
        (0x0A, 0x38) => "MON-RF",
        (0x02, 0x13) => "RXM-SFRBX",
        _ => return format!("UBX {class:02X}-{id:02X}"),
    };
    n.to_string()
}

/// Wiederholblöcke nach einem Kopf fester Länge.
fn blocks(p: &[u8], head: usize, size: usize, count: usize) -> impl Iterator<Item = &[u8]> {
    p.get(head..).unwrap_or(&[]).chunks_exact(size).take(count)
}

/// Dekodiert die Nutzdaten; `None` bei unbekannter oder zu kurzer Nachricht.
pub fn decode(class: u8, id: u8, p: &[u8]) -> Option<Msg> {
    let need = |n: usize| (p.len() >= n).then_some(());
    Some(match (class, id) {
        (0x01, 0x07) => {
            need(92)?;
            Msg::Pvt(NavPvt {
                itow: u4(p, 0),
                fix_type: p[20],
                fix_ok: p[21] & 0x01 != 0,
                diff: p[21] & 0x02 != 0,
                carrier: p[21] >> 6,
                num_sv: p[23],
                lon: i4(p, 24) as f64 * 1e-7,
                lat: i4(p, 28) as f64 * 1e-7,
                height: i4(p, 32) as f64 * 1e-3,
                h_acc: u4(p, 40) as f64 * 1e-3,
                v_acc: u4(p, 44) as f64 * 1e-3,
                speed: i4(p, 60) as f64 * 1e-3,
                pdop: u2(p, 76) as f64 * 0.01,
            })
        }
        (0x01, 0x14) => {
            need(36)?;
            Msg::HpPos(HpPos {
                itow: u4(p, 4),
                valid: p[3] & 0x01 == 0,
                lon: i4(p, 8) as f64 * 1e-7 + i1(p, 24) as f64 * 1e-9,
                lat: i4(p, 12) as f64 * 1e-7 + i1(p, 25) as f64 * 1e-9,
                height: i4(p, 16) as f64 * 1e-3 + i1(p, 26) as f64 * 1e-4,
            })
        }
        (0x01, 0x36) => {
            need(64)?;
            Msg::Cov(Cov {
                itow: u4(p, 0),
                valid: p[5] != 0,
                nn: r4(p, 16),
                ne: r4(p, 20),
                ee: r4(p, 28),
                dd: r4(p, 36),
            })
        }
        (0x01, 0x04) => {
            need(18)?;
            let d = |o| u2(p, o) as f64 * 0.01;
            Msg::Dop(Dop {
                g: d(4),
                p: d(6),
                t: d(8),
                v: d(10),
                h: d(12),
            })
        }
        (0x01, 0x35) => {
            need(8)?;
            let sats = blocks(p, 8, 12, p[5] as usize)
                .map(|b| {
                    let flags = u4(b, 8);
                    let elev = i1(b, 3);
                    Sat {
                        gnss: b[0],
                        sv: b[1],
                        cno: b[2],
                        elev: (-90..=90).contains(&elev).then_some(elev as f64),
                        azim: i2(b, 4) as f64,
                        quality: (flags & 0x07) as u8,
                        used: flags & (1 << 3) != 0,
                        health: ((flags >> 4) & 0x03) as u8,
                        diff_corr: flags & (1 << 6) != 0,
                        orbit_source: ((flags >> 8) & 0x07) as u8,
                        rtcm_used: flags & (1 << 17) != 0,
                    }
                })
                .collect();
            Msg::Sat {
                itow: u4(p, 0),
                sats,
            }
        }
        (0x01, 0x43) => {
            need(8)?;
            Msg::Sig(
                blocks(p, 8, 16, p[5] as usize)
                    .map(|b| Signal {
                        gnss: b[0],
                        sv: b[1],
                        sig: b[2],
                        cno: b[6],
                        used: u2(b, 10) & 0x38 != 0,
                    })
                    .collect(),
            )
        }
        (0x01, 0x34) => {
            need(8)?;
            Msg::Orb(
                blocks(p, 8, 6, p[5] as usize)
                    .map(|b| Orbit {
                        gnss: b[0],
                        sv: b[1],
                        visibility: (b[2] >> 2) & 0x03,
                    })
                    .collect(),
            )
        }
        (0x01, 0x20) => {
            need(16)?;
            Msg::TimeGps(TimeGps {
                itow: u4(p, 0),
                ftow_ns: i4(p, 4),
                week: i2(p, 8),
                leap_s: i1(p, 10),
                tow_valid: p[11] & 0x01 != 0,
                week_valid: p[11] & 0x02 != 0,
                leap_valid: p[11] & 0x04 != 0,
                acc_ns: u4(p, 12),
            })
        }
        (0x01, 0x21) => {
            need(20)?;
            Msg::TimeUtc(TimeUtc {
                acc_ns: u4(p, 4),
                nano: i4(p, 8),
                year: u2(p, 12),
                month: p[14],
                day: p[15],
                hour: p[16],
                min: p[17],
                sec: p[18],
                valid: p[19] & 0x04 != 0,
            })
        }
        (0x01, 0x26) => {
            need(24)?;
            Msg::TimeLs(TimeLs {
                current: i1(p, 9),
                change: i1(p, 11),
                seconds_to_event: i4(p, 12),
                current_valid: p[23] & 0x01 != 0,
                event_valid: p[23] & 0x02 != 0,
            })
        }
        (0x01, 0x03) => {
            need(16)?;
            Msg::Status {
                ttff_s: u4(p, 8) as f64 * 1e-3,
                uptime_s: u4(p, 12) as f64 * 1e-3,
            }
        }
        (0x02, 0x32) => {
            need(8)?;
            Msg::RtcmUse(RtcmUse {
                crc_failed: p[1] & 0x01 != 0,
                used: (p[1] >> 1) & 0x03,
                msg_type: u2(p, 6),
            })
        }
        (0x02, 0x13) => {
            need(8)?;
            let words = p[8..].chunks_exact(4).take(usize::from(p[4])).map(|w| u4(w, 0)).collect();
            Msg::Sfrbx { gnss: p[0], sv: p[1], words }
        }
        (0x0A, 0x38) => {
            need(4)?;
            Msg::Rf(
                blocks(p, 4, 24, p[1] as usize)
                    .map(|b| RfBlock {
                        block: b[0],
                        jamming: b[1] & 0x03,
                        antenna: b[2],
                        cw_indicator: b[16],
                    })
                    .collect(),
            )
        }
        (0x0A, 0x04) => {
            need(40)?;
            Msg::Version {
                software: text(&p[..30]),
                hardware: text(&p[30..40]),
                extensions: p[40..].chunks_exact(30).map(text).collect(),
            }
        }
        (0x05, 0x00) | (0x05, 0x01) => {
            need(2)?;
            Msg::Ack {
                class: p[0],
                id: p[1],
                ok: id == 0x01,
            }
        }
        // Antwort auf CFG-VALGET (Version 1)
        (0x06, 0x8B) => {
            need(4)?;
            if p[0] != 1 {
                return None;
            }
            let mut items = Vec::new();
            let mut o = 4;
            while o + 4 <= p.len() {
                let key = u4(p, o);
                let size = crate::gnss::kern::ubx::value_size(key)?;
                o += 4;
                let bytes = p.get(o..o + size)?;
                let mut v = [0u8; 8];
                v[..size].copy_from_slice(bytes);
                items.push((key, u64::from_le_bytes(v)));
                o += size;
            }
            Msg::ValGet(items)
        }
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hppos_high_precision() {
        let mut p = vec![0u8; 36];
        p[8..12].copy_from_slice(&129_612_345i32.to_le_bytes());
        p[12..16].copy_from_slice(&488_312_345i32.to_le_bytes());
        p[16..20].copy_from_slice(&361_438i32.to_le_bytes());
        p[24] = 67;
        p[25] = (-5i8) as u8;
        p[26] = 3;
        let Some(Msg::HpPos(h)) = decode(0x01, 0x14, &p) else {
            panic!()
        };
        assert!((h.lon - 12.961_234_567).abs() < 1e-12);
        assert!((h.lat - 48.831_234_495).abs() < 1e-12);
        assert!((h.height - 361.4383).abs() < 1e-9);
    }

    #[test]
    fn sat_block_and_short_payload() {
        let mut p = vec![0, 0, 0, 0, 1, 1, 0, 0];
        p.extend([2, 11, 44, 35, 0x2C, 0x01, 0, 0]);
        p.extend((0x0002_000Fu32).to_le_bytes());
        let Some(Msg::Sat { sats, .. }) = decode(0x01, 0x35, &p) else {
            panic!()
        };
        assert_eq!(sats[0].elev, Some(35.0));
        assert_eq!(sats[0].azim, 300.0);
        assert!(sats[0].used && sats[0].rtcm_used);
        assert!(decode(0x01, 0x07, &p).is_none());
    }
}
