//! RTCM 3 frame scanner (CRC-24Q). Used to monitor the correction stream:
//! message types, data rate, presence of transformation messages 1021/1023.
//!
//! Hardware: simpleRTK2B (u-blox ZED-F9P)
//! Autor:    Johann Gerner
//!
//! Übernommen aus gnss-vermessung 0.4.0 (src/rtcm.rs), eigenständig gepflegt in Feldbuch-NT.

use std::collections::BTreeMap;

const CRC24Q_POLY: u32 = 0x186_4CFB;

pub fn crc24q(data: &[u8]) -> u32 {
    let mut crc: u32 = 0;
    for &b in data {
        crc ^= (b as u32) << 16;
        for _ in 0..8 {
            crc <<= 1;
            if crc & 0x100_0000 != 0 {
                crc ^= CRC24Q_POLY;
            }
        }
    }
    crc & 0xFF_FFFF
}

#[derive(Default)]
pub struct RtcmScanner {
    buf: Vec<u8>,
    pub counts: BTreeMap<u16, u64>,
    pub bytes: u64,
    pub crc_errors: u64,
    pub last_seen: BTreeMap<u16, std::time::Instant>,
}

impl RtcmScanner {
    /// Feed raw bytes; returns message types of complete, CRC-valid frames.
    pub fn feed(&mut self, data: &[u8]) -> Vec<u16> {
        self.feed_frames(data)
            .iter()
            .map(|p| (u16::from(p[0]) << 4) | u16::from(p[1] >> 4))
            .collect()
    }

    /// Feed raw bytes; returns the payloads of complete, CRC-valid frames.
    pub fn feed_frames(&mut self, data: &[u8]) -> Vec<Vec<u8>> {
        self.bytes += data.len() as u64;
        self.buf.extend_from_slice(data);
        let mut out = Vec::new();
        loop {
            let Some(start) = self.buf.iter().position(|&b| b == 0xD3) else {
                self.buf.clear();
                break;
            };
            self.buf.drain(..start);
            if self.buf.len() < 6 {
                break;
            }
            if self.buf[1] & 0xFC != 0 {
                // reserved bits set -> false preamble
                self.buf.drain(..1);
                continue;
            }
            let len = (((self.buf[1] & 0x03) as usize) << 8) | self.buf[2] as usize;
            let total = 3 + len + 3;
            if self.buf.len() < total {
                break;
            }
            let crc = ((self.buf[3 + len] as u32) << 16)
                | ((self.buf[4 + len] as u32) << 8)
                | self.buf[5 + len] as u32;
            if crc24q(&self.buf[..3 + len]) == crc && len >= 2 {
                let mt = ((self.buf[3] as u16) << 4) | (self.buf[4] as u16 >> 4);
                *self.counts.entry(mt).or_default() += 1;
                out.push(self.buf[3..3 + len].to_vec());
                self.last_seen.insert(mt, std::time::Instant::now());
                self.buf.drain(..total);
            } else {
                self.crc_errors += 1;
                self.buf.drain(..1); // resync
            }
        }
        out
    }

    pub fn summary(&self) -> String {
        self.counts
            .iter()
            .map(|(k, v)| format!("{k}:{v}"))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_roundtrip() {
        // synthetic message type 1005, 19 byte payload
        let mut payload = vec![0u8; 19];
        payload[0] = (1005u16 >> 4) as u8;
        payload[1] = ((1005u16 & 0x0F) << 4) as u8;
        let mut frame = vec![0xD3, 0x00, payload.len() as u8];
        frame.extend(&payload);
        let crc = crc24q(&frame);
        frame.extend([(crc >> 16) as u8, (crc >> 8) as u8, crc as u8]);
        let mut s = RtcmScanner::default();
        let mut junk = vec![0x00, 0xD3, 0x55];
        junk.extend(&frame);
        assert_eq!(s.feed(&junk), vec![1005]);
    }
}
