// Zweck:    Bestätigungston (0,5 s: zwei aufsteigende Töne) nach erfolgreicher Messung und Fehlerton (0,5 s: dreimal 800 Hz)
//           nach fehlgeschlagener Messung, ohne Zusatzbibliothek
// Hardware: Audioausgabe des Rechners (Windows: winmm PlaySound aus dem Speicher)
// Autor:    Johann Gerner

use std::sync::OnceLock;

/// Abtastrate und Amplitude des erzeugten Tons.
const ABTASTRATE: u32 = 44_100;
const AMPLITUDE: f64 = 0.6;
/// Frequenzen der beiden Teiltöne [Hz] und Dauer je Teilton [s].
const FREQUENZ_TON_1_HZ: f64 = 400.0;
const FREQUENZ_TON_2_HZ: f64 = 600.0;
const DAUER_TEILTON_S: f64 = 0.25;
/// Ein- und Ausblendzeit je Teilton [s] gegen Knacken.
const EINBLENDEN_S: f64 = 0.01;
const AUSBLENDEN_S: f64 = 0.05;
/// Fehlerton: dreimal 800 Hz in kurzer Folge (Piep und Pause je 0,1 s → zusammen 0,5 s).
const FEHLER_FREQUENZ_HZ: f64 = 800.0;
const FEHLER_PIEP_S: f64 = 0.1;
const FEHLER_PAUSE_S: f64 = 0.1;
const FEHLER_ANZAHL_PIEPS: usize = 3;
const FEHLER_EINBLENDEN_S: f64 = 0.005;
const FEHLER_AUSBLENDEN_S: f64 = 0.02;
/// Größe des WAV-Kopfes [Byte] und Format-Konstanten (PCM, mono, 16 Bit).
const WAV_KOPF_BYTES: u32 = 44;
const WAV_FORMAT_PCM: u16 = 1;
const WAV_KANAELE: u16 = 1;
const WAV_BITS: u16 = 16;
const WAV_FMT_BLOCKGROESSE: u32 = 16;
/// Skalierung auf 16-Bit-Ganzzahlen.
const PCM_MAX: f64 = 32767.0;

/// Ein Ton mit Ein- und Ausblendung als 16-Bit-Werte.
fn ton(frequenz_hz: f64, dauer_s: f64, einblenden_s: f64, ausblenden_s: f64) -> Vec<i16> {
    let anzahl = (f64::from(ABTASTRATE) * dauer_s) as usize;
    (0..anzahl)
        .map(|i| {
            let t = i as f64 / f64::from(ABTASTRATE);
            let huellkurve = (t / einblenden_s).min((dauer_s - t) / ausblenden_s).min(1.0);
            (AMPLITUDE * huellkurve * (2.0 * std::f64::consts::PI * frequenz_hz * t).sin() * PCM_MAX) as i16
        })
        .collect()
}

/// Bestätigungston: zwei aufsteigende Töne.
fn werte_bestaetigung() -> Vec<i16> {
    let mut werte = ton(FREQUENZ_TON_1_HZ, DAUER_TEILTON_S, EINBLENDEN_S, AUSBLENDEN_S);
    werte.extend(ton(FREQUENZ_TON_2_HZ, DAUER_TEILTON_S, EINBLENDEN_S, AUSBLENDEN_S));
    werte
}

/// Fehlerton: dreimal kurzer Piep mit Pausen dazwischen.
fn werte_fehler() -> Vec<i16> {
    let pause = vec![0i16; (f64::from(ABTASTRATE) * FEHLER_PAUSE_S) as usize];
    let mut werte = Vec::new();
    for nr in 0..FEHLER_ANZAHL_PIEPS {
        if nr > 0 {
            werte.extend_from_slice(&pause);
        }
        werte.extend(ton(FEHLER_FREQUENZ_HZ, FEHLER_PIEP_S, FEHLER_EINBLENDEN_S, FEHLER_AUSBLENDEN_S));
    }
    werte
}

/// Die Tonwerte als WAV-Datei im Speicher.
fn erzeuge_wav(werte: Vec<i16>) -> Vec<u8> {
    let datenbytes = (werte.len() * 2) as u32;
    let bytes_je_sekunde = ABTASTRATE * u32::from(WAV_KANAELE) * u32::from(WAV_BITS) / 8;
    let blockgroesse = WAV_KANAELE * WAV_BITS / 8;
    let mut wav = Vec::with_capacity((WAV_KOPF_BYTES + datenbytes) as usize);
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(WAV_KOPF_BYTES - 8 + datenbytes).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&WAV_FMT_BLOCKGROESSE.to_le_bytes());
    wav.extend_from_slice(&WAV_FORMAT_PCM.to_le_bytes());
    wav.extend_from_slice(&WAV_KANAELE.to_le_bytes());
    wav.extend_from_slice(&ABTASTRATE.to_le_bytes());
    wav.extend_from_slice(&bytes_je_sekunde.to_le_bytes());
    wav.extend_from_slice(&blockgroesse.to_le_bytes());
    wav.extend_from_slice(&WAV_BITS.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&datenbytes.to_le_bytes());
    for w in werte {
        wav.extend_from_slice(&w.to_le_bytes());
    }
    wav
}

#[cfg(windows)]
mod winmm {
    /// Flags von PlaySound: asynchron, kein Systemstandardton, Quelle ist ein Speicherpuffer.
    pub const SND_ASYNC: u32 = 0x0001;
    pub const SND_NODEFAULT: u32 = 0x0002;
    pub const SND_MEMORY: u32 = 0x0004;

    #[link(name = "winmm")]
    unsafe extern "system" {
        pub fn PlaySoundW(ton: *const u8, modul: *mut std::ffi::c_void, flags: u32) -> i32;
    }
}

/// Spielt den Puffer ab (nicht blockierend). Ohne Audiogerät oder auf anderen Systemen geschieht nichts.
fn spiele(wav: &'static [u8]) {
    #[cfg(windows)]
    // SAFETY: `wav` ist ein gültiger, unveränderlicher WAV-Puffer mit 'static-Lebensdauer (bleibt während der asynchronen Wiedergabe gültig).
    unsafe {
        winmm::PlaySoundW(wav.as_ptr(), std::ptr::null_mut(), winmm::SND_ASYNC | winmm::SND_NODEFAULT | winmm::SND_MEMORY);
    }
    #[cfg(not(windows))]
    let _ = wav;
}

/// Bestätigungston nach erfolgreicher Messung.
pub fn bestaetigung() {
    static WAV: OnceLock<Vec<u8>> = OnceLock::new();
    spiele(WAV.get_or_init(|| erzeuge_wav(werte_bestaetigung())));
}

/// Fehlerton nach fehlgeschlagener Messung.
pub fn fehler() {
    static WAV: OnceLock<Vec<u8>> = OnceLock::new();
    spiele(WAV.get_or_init(|| erzeuge_wav(werte_fehler())));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toene_haben_halbe_sekunde_und_gueltigen_kopf() {
        for werte in [werte_bestaetigung(), werte_fehler()] {
            let wav = erzeuge_wav(werte);
            assert_eq!(&wav[..4], b"RIFF");
            assert_eq!(&wav[8..12], b"WAVE");
            let daten = wav.len() - WAV_KOPF_BYTES as usize;
            let dauer_s = daten as f64 / 2.0 / f64::from(ABTASTRATE);
            assert!((dauer_s - 0.5).abs() < 1e-3, "Dauer {dauer_s}");
        }
    }
}
