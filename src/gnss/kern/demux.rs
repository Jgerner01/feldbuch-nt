//! Zweck:    Trennt NMEA-Zeilen und UBX-Nachrichten im gemeinsamen Bytestrom des Empfängers.
//! Hardware: simpleRTK2B (u-blox ZED-F9P)
//! Autor:    Johann Gerner
//!
//! Übernommen aus gnss-vermessung 0.4.0 (src/demux.rs), eigenständig gepflegt in Feldbuch-NT.

const UBX_SYNC: [u8; 2] = [0xB5, 0x62];
const UBX_HEADER: usize = 6;
const UBX_CHECKSUM: usize = 2;
const UBX_MAX_PAYLOAD: usize = 8192;
const NMEA_MAX: usize = 512;

#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    /// vollständige NMEA-Zeile ohne Zeilenende
    Nmea(String),
    /// gültige UBX-Nachricht; `raw` enthält den ganzen Rahmen
    Ubx { class: u8, id: u8, raw: Vec<u8> },
    /// nicht zuordenbare oder beschädigte Bytes
    Junk(Vec<u8>),
}

#[derive(Default)]
pub struct Demux {
    buf: Vec<u8>,
}

enum Step {
    Take(usize, Item),
    Wait,
    Skip,
}

impl Demux {
    pub fn feed(&mut self, data: &[u8]) -> Vec<Item> {
        self.buf.extend_from_slice(data);
        let mut out = Vec::new();
        let mut junk = Vec::new();
        let mut pos = 0;
        while pos < self.buf.len() {
            match Self::probe(&self.buf[pos..]) {
                Step::Take(n, item) => {
                    flush_junk(&mut junk, &mut out);
                    out.push(item);
                    pos += n;
                }
                Step::Wait => break,
                Step::Skip => {
                    junk.push(self.buf[pos]);
                    pos += 1;
                }
            }
        }
        self.buf.drain(..pos);
        flush_junk(&mut junk, &mut out);
        out
    }

    fn probe(b: &[u8]) -> Step {
        if b[0] == b'$' {
            for (i, &c) in b.iter().enumerate().take(NMEA_MAX).skip(1) {
                if c == b'\n' {
                    let line = String::from_utf8_lossy(&b[..i]).trim_end().to_string();
                    // ohne Prüfsumme ist es kein NMEA-Satz (z. B. '$' in Binärdaten)
                    if !line.contains('*') {
                        return Step::Skip;
                    }
                    return Step::Take(i + 1, Item::Nmea(line));
                }
                if c != b'\r' && !(0x20..0x7F).contains(&c) {
                    return Step::Skip;
                }
            }
            return if b.len() < NMEA_MAX {
                Step::Wait
            } else {
                Step::Skip
            };
        }
        if b[0] != UBX_SYNC[0] {
            return Step::Skip;
        }
        if b.len() < 2 {
            return Step::Wait;
        }
        if b[1] != UBX_SYNC[1] {
            return Step::Skip;
        }
        if b.len() < UBX_HEADER {
            return Step::Wait;
        }
        let len = u16::from_le_bytes([b[4], b[5]]) as usize;
        if len > UBX_MAX_PAYLOAD {
            return Step::Skip;
        }
        let total = UBX_HEADER + len + UBX_CHECKSUM;
        if b.len() < total {
            return Step::Wait;
        }
        let (mut a, mut c) = (0u8, 0u8);
        for &x in &b[2..total - UBX_CHECKSUM] {
            a = a.wrapping_add(x);
            c = c.wrapping_add(a);
        }
        if [a, c] != b[total - UBX_CHECKSUM..total] {
            return Step::Skip;
        }
        Step::Take(
            total,
            Item::Ubx {
                class: b[2],
                id: b[3],
                raw: b[..total].to_vec(),
            },
        )
    }
}

/// Gesammelte Störbytes ausgeben; reine Zeilenenden sind kein Fehler.
fn flush_junk(junk: &mut Vec<u8>, out: &mut Vec<Item>) {
    let bytes = std::mem::take(junk);
    if bytes.iter().any(|&b| b != b'\r' && b != b'\n') {
        out.push(Item::Junk(bytes));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gnss::kern::ubx::frame;

    #[test]
    fn mixed_stream_bytewise() {
        let ubx = frame(0x01, 0x07, &[b'$', b'\n', 0xB5]);
        let mut data = b"\x00\x11$GNGGA,1*00\r\n".to_vec();
        data.extend(&ubx);
        let mut bad = ubx.clone();
        *bad.last_mut().unwrap() ^= 1;
        data.extend(&bad);
        data.extend(&ubx);
        let mut d = Demux::default();
        let items: Vec<Item> = data.iter().flat_map(|&b| d.feed(&[b])).collect();
        let good: Vec<&Item> = items
            .iter()
            .filter(|i| !matches!(i, Item::Junk(_)))
            .collect();
        assert_eq!(good.len(), 3);
        assert_eq!(good[0], &Item::Nmea("$GNGGA,1*00".into()));
        assert!(matches!(good[2], Item::Ubx { class: 1, id: 7, raw } if *raw == ubx));
    }
}
