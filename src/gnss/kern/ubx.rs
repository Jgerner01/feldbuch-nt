//! UBX configuration for the u-blox ZED-F9P (simpleRTK2B) via CFG-VALSET.
//! Key IDs from the u-blox "ZED-F9P Interface Description" (HPG 1.3x).
//! Verify against the document matching your firmware before relying on them.
//!
//! Hardware: simpleRTK2B (u-blox ZED-F9P)
//! Autor:    Johann Gerner
//!
//! Übernommen aus gnss-vermessung 0.4.0 (src/ubx.rs), eigenständig gepflegt in Feldbuch-NT.

#[derive(Debug, Clone, Copy)]
pub enum Port {
    Uart1,
    Uart2,
    Usb,
}

impl Port {
    fn idx(self) -> u32 {
        match self {
            Port::Uart1 => 0,
            Port::Uart2 => 1,
            Port::Usb => 2,
        }
    }
}

// CFG keys
pub const CFG_RATE_MEAS: u32 = 0x3021_0001; // U2, ms
pub const CFG_NMEA_HIGHPREC: u32 = 0x1093_0006; // L
pub const CFG_NAVHPG_DGNSSMODE: u32 = 0x2014_0011; // E1, 2=float only, 3=fixed
pub const CFG_NAVSPG_DYNMODEL: u32 = 0x2011_0021; // E1, 0=portable, 2=stationary
pub const CFG_USBINPROT_RTCM3X: u32 = 0x1077_0004; // L
pub const CFG_UART1INPROT_RTCM3X: u32 = 0x1073_0004; // L

// CFG-MSGOUT-NMEA_ID_xxx_UART1 base keys (UART2 = +1, USB = +2)
pub const NMEA_GGA: u32 = 0x2091_00bb;
pub const NMEA_GST: u32 = 0x2091_00d4;
pub const NMEA_GSA: u32 = 0x2091_00c0;
pub const NMEA_GSV: u32 = 0x2091_00c5;
pub const NMEA_RMC: u32 = 0x2091_00ac;
pub const NMEA_VTG: u32 = 0x2091_00b1;
pub const NMEA_GLL: u32 = 0x2091_00ca;

pub fn msgout(base: u32, port: Port) -> u32 {
    base + port.idx()
}

/// Layer bitmask for CFG-VALSET
pub const LAYER_RAM: u8 = 0x01;
pub const LAYER_BBR: u8 = 0x02;
pub const LAYER_FLASH: u8 = 0x04;

pub fn value_size(key: u32) -> Option<usize> {
    match (key >> 28) & 0x07 {
        1 | 2 => Some(1),
        3 => Some(2),
        4 => Some(4),
        5 => Some(8),
        _ => None,
    }
}

pub fn frame(class: u8, id: u8, payload: &[u8]) -> Vec<u8> {
    let mut f = vec![0xB5, 0x62, class, id];
    f.extend((payload.len() as u16).to_le_bytes());
    f.extend_from_slice(payload);
    let (mut a, mut b) = (0u8, 0u8);
    for &x in &f[2..] {
        a = a.wrapping_add(x);
        b = b.wrapping_add(a);
    }
    f.extend([a, b]);
    f
}

/// Build one CFG-VALSET message (max. 64 key/value pairs).
pub fn valset(layers: u8, items: &[(u32, u64)]) -> Vec<u8> {
    assert!(items.len() <= 64);
    let mut p = vec![0x00, layers, 0x00, 0x00];
    for &(key, val) in items {
        p.extend(key.to_le_bytes());
        let size = value_size(key).expect("ungültiger Konfigurationsschlüssel");
        p.extend(&val.to_le_bytes()[..size]);
    }
    frame(0x06, 0x8A, &p)
}

/// Standard configuration for RTK surveying with SAPOS HEPS on the given port:
/// GGA + GST only, high-precision NMEA, RTCM3 input, fixed ambiguity mode, given rate.
pub fn survey_config(port: Port, rate_ms: u16, layers: u8) -> Vec<u8> {
    let mut items = vec![
        (CFG_RATE_MEAS, rate_ms as u64),
        (CFG_NMEA_HIGHPREC, 1),
        (CFG_NAVHPG_DGNSSMODE, 3),
        (CFG_NAVSPG_DYNMODEL, 0),
        (msgout(NMEA_GGA, port), 1),
        (msgout(NMEA_GST, port), 1),
    ];
    for m in [NMEA_GSA, NMEA_GSV, NMEA_RMC, NMEA_VTG, NMEA_GLL] {
        items.push((msgout(m, port), 0));
    }
    items.push(match port {
        Port::Usb => (CFG_USBINPROT_RTCM3X, 1),
        _ => (CFG_UART1INPROT_RTCM3X, 1),
    });
    valset(layers, &items)
}

// CFG-MSGOUT-Schlüssel der UBX-Meldungen, Basis UART1
pub const MSG_NAV_PVT: u32 = 0x2091_0007;
pub const MSG_NAV_HPPOSLLH: u32 = 0x2091_0034;
pub const MSG_NAV_COV: u32 = 0x2091_0084;
pub const MSG_NAV_DOP: u32 = 0x2091_0039;
pub const MSG_NAV_SAT: u32 = 0x2091_0016;
pub const MSG_NAV_SIG: u32 = 0x2091_0346;
pub const MSG_NAV_ORB: u32 = 0x2091_0011;
pub const MSG_NAV_TIMEGPS: u32 = 0x2091_0048;
pub const MSG_NAV_TIMEUTC: u32 = 0x2091_005c;
pub const MSG_NAV_TIMELS: u32 = 0x2091_0061;
pub const MSG_NAV_STATUS: u32 = 0x2091_001b;
pub const MSG_RXM_RTCM: u32 = 0x2091_0269;
pub const MSG_MON_RF: u32 = 0x2091_035a;
pub const MSG_RXM_SFRBX: u32 = 0x2091_0232;

pub const CFG_NAVSPG_INFIL_MINELEV: u32 = 0x2011_00a4; // I1, Grad
pub const CFG_USBOUTPROT_UBX: u32 = 0x1078_0001; // L
pub const CFG_USBOUTPROT_NMEA: u32 = 0x1078_0002; // L
pub const CFG_UART1OUTPROT_UBX: u32 = 0x1074_0001; // L

/// Empfängereinstellungen, die die Oberfläche setzt.
#[derive(Debug, Clone, Copy)]
pub struct ReceiverSetup {
    pub port: Port,
    pub rate_ms: u16,
    pub dyn_model: u8,
    pub elevation_mask: u8,
}

/// Ein einzeln bestätigter Konfigurationsschritt.
pub struct Step {
    pub label: &'static str,
    pub frame: Vec<u8>,
}

/// Konfiguration in Einzelschritten: CFG-VALSET wirkt ganz oder gar nicht,
/// so lässt sich ein abgelehnter Schlüssel eindeutig zuordnen.
pub fn gui_config(s: &ReceiverSetup, layers: u8) -> Vec<Step> {
    gui_config_items(s)
        .into_iter()
        .map(|(label, items)| Step {
            label,
            frame: valset(layers, &items),
        })
        .collect()
}

/// Inhalt der Konfigurationsschritte: Bezeichnung und Schlüssel/Wert-Paare.
pub fn gui_config_items(s: &ReceiverSetup) -> Vec<(&'static str, Vec<(u32, u64)>)> {
    const MS_PER_S: u16 = 1000;
    // Anzahl Epochen je Sekunde für Meldungen, die nur sekündlich nötig sind
    let per_second = (MS_PER_S / s.rate_ms.max(1)).max(1) as u64;
    let step = |label, items: &[(u32, u64)]| (label, items.to_vec());
    let protocols: &[(u32, u64)] = match s.port {
        Port::Usb => &[
            (CFG_USBOUTPROT_UBX, 1),
            (CFG_USBOUTPROT_NMEA, 1),
            (CFG_USBINPROT_RTCM3X, 1),
        ],
        _ => &[(CFG_UART1OUTPROT_UBX, 1), (CFG_UART1INPROT_RTCM3X, 1)],
    };
    let mut steps = vec![
        step("Messrate", &[(CFG_RATE_MEAS, s.rate_ms as u64)]),
        step("Protokolle", protocols),
        step("Ambiguitäten fixieren", &[(CFG_NAVHPG_DGNSSMODE, 3)]),
        step(
            "Dynamikmodell",
            &[(CFG_NAVSPG_DYNMODEL, s.dyn_model as u64)],
        ),
        step(
            "Elevationsmaske",
            &[(CFG_NAVSPG_INFIL_MINELEV, s.elevation_mask as u64)],
        ),
        step("NMEA hochauflösend", &[(CFG_NMEA_HIGHPREC, 1)]),
    ];
    let messages: [(&'static str, u32, u64); 21] = [
        ("NMEA GGA", NMEA_GGA, 1),
        ("NMEA GST", NMEA_GST, 1),
        ("NMEA GSA aus", NMEA_GSA, 0),
        ("NMEA GSV aus", NMEA_GSV, 0),
        ("NMEA RMC aus", NMEA_RMC, 0),
        ("NMEA VTG aus", NMEA_VTG, 0),
        ("NMEA GLL aus", NMEA_GLL, 0),
        ("NAV-PVT", MSG_NAV_PVT, 1),
        ("NAV-HPPOSLLH", MSG_NAV_HPPOSLLH, 1),
        ("NAV-COV", MSG_NAV_COV, 1),
        ("NAV-DOP", MSG_NAV_DOP, 1),
        ("NAV-SAT", MSG_NAV_SAT, per_second),
        ("NAV-SIG", MSG_NAV_SIG, per_second),
        ("NAV-ORB", MSG_NAV_ORB, 5 * per_second),
        ("NAV-TIMEGPS", MSG_NAV_TIMEGPS, per_second),
        ("NAV-TIMEUTC", MSG_NAV_TIMEUTC, per_second),
        ("NAV-TIMELS", MSG_NAV_TIMELS, 60 * per_second),
        ("NAV-STATUS", MSG_NAV_STATUS, per_second),
        ("RXM-RTCM", MSG_RXM_RTCM, 1),
        ("MON-RF", MSG_MON_RF, per_second),
        // Bahndaten für die Auswertung der SAPOS-Messungen
        ("RXM-SFRBX", MSG_RXM_SFRBX, 1),
    ];
    for (label, key, rate) in messages {
        // Raten sind im Empfänger ein Byte groß
        steps.push(step(
            label,
            &[(msgout(key, s.port), rate.min(u8::MAX as u64))],
        ));
    }
    steps
}

/// Schlüssel, die beim Rücklesen angezeigt werden.
pub const READBACK_KEYS: [(u32, &str); 5] = [
    (CFG_RATE_MEAS, "Messrate [ms]"),
    (CFG_NAVSPG_DYNMODEL, "Dynamikmodell"),
    (CFG_NAVSPG_INFIL_MINELEV, "Elevationsmaske [°]"),
    (CFG_NAVHPG_DGNSSMODE, "Ambiguitätsmodus (3 = fix)"),
    (CFG_NMEA_HIGHPREC, "NMEA hochauflösend"),
];

/// CFG-VALGET-Anfrage für die Werte im RAM.
pub fn valget(keys: &[u32]) -> Vec<u8> {
    valget_layer(VALGET_RAM, keys)
}

/// Ebenen (Layer) für CFG-VALGET; anders kodiert als die Bitmaske von CFG-VALSET.
pub const VALGET_RAM: u8 = 0;
pub const VALGET_FLASH: u8 = 2;

/// CFG-VALGET-Anfrage für die Werte einer Ebene (höchstens 64 Schlüssel).
pub fn valget_layer(layer: u8, keys: &[u32]) -> Vec<u8> {
    let mut p = vec![0x00, layer, 0x00, 0x00];
    for k in keys {
        p.extend(k.to_le_bytes());
    }
    frame(0x06, 0x8B, &p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checksum_known() {
        // UBX-MON-VER poll: B5 62 0A 04 00 00 0E 34
        assert_eq!(
            frame(0x0A, 0x04, &[]),
            vec![0xB5, 0x62, 0x0A, 0x04, 0, 0, 0x0E, 0x34]
        );
    }

    #[test]
    fn valset_layout() {
        let m = valset(LAYER_RAM, &[(CFG_RATE_MEAS, 200)]);
        assert_eq!(&m[6..10], &[0, 1, 0, 0]);
        assert_eq!(&m[10..14], &CFG_RATE_MEAS.to_le_bytes());
        assert_eq!(&m[14..16], &200u16.to_le_bytes());
    }
}
