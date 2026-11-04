// Zweck:    Tachymeter-Modelle mit Voreinstellungen, Anzeigetexten und der wählbaren Gerätesprache
//           (Leica GeoCOM oder GSI Online)
// Hardware: Leica TS06, TPS300-Serie, TPS1200 (GeoCOM), Topcon GTS, GNSS NMEA 0183
// Autor:    Johann Gerner

/// Unterstützte Messgeräte.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TachymeterModell {
    /// Leica TS06 (GeoCOM, optional GSI Online).
    LeicaTs06,
    /// Leica TPS300-Serie (GSI Online, optional GeoCOM).
    LeicaTps300,
    /// Leica TPS1200 über GeoCOM mit 38400 Baud.
    LeicaGeoCom,
    /// Leica TPS1200 über GeoCOM mit 9600 Baud (sicherer Rückfall).
    LeicaGeoComStandard,
    LeicaTs11,
    TrimbleS3,
    TopconGts,
    GnssNmea,
    Manuell,
}

/// Alle Modelle in der Reihenfolge der Auswahlliste.
pub const ALLE_MODELLE: [TachymeterModell; 9] = [
    TachymeterModell::LeicaTs06,
    TachymeterModell::LeicaTps300,
    TachymeterModell::LeicaGeoCom,
    TachymeterModell::LeicaGeoComStandard,
    TachymeterModell::LeicaTs11,
    TachymeterModell::TrimbleS3,
    TachymeterModell::TopconGts,
    TachymeterModell::GnssNmea,
    TachymeterModell::Manuell,
];

/// Gerätesprache der Leica-Geräte.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sprache {
    GeoCom,
    GsiOnline,
}

impl Sprache {
    /// Name wie in Einstellungen.xml.
    pub fn name(self) -> &'static str {
        match self {
            Self::GeoCom => "GeoCOM",
            Self::GsiOnline => "GSI Online",
        }
    }

    /// Findet die Sprache anhand des gespeicherten Namens.
    pub fn aus_name(name: &str) -> Option<Self> {
        [Self::GeoCom, Self::GsiOnline].into_iter().find(|s| s.name().eq_ignore_ascii_case(name.trim()))
    }
}

/// Baudrate für GeoCOM mit hoher Geschwindigkeit.
const BAUD_GEOCOM_SCHNELL: u32 = 38_400;
/// Standard-Baudrate der meisten Geräte.
const BAUD_STANDARD: u32 = 9_600;
/// Baudrate für NMEA-0183-Empfänger.
const BAUD_NMEA: u32 = 4_800;

/// Voreinstellung der seriellen Schnittstelle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SeriellPreset {
    pub baud: u32,
    pub datenbits: u8,
}

impl TachymeterModell {
    /// Name wie in Einstellungen.xml (identisch zur C#-Version).
    pub fn name(self) -> &'static str {
        match self {
            Self::LeicaTs06 => "LeicaTS06",
            Self::LeicaTps300 => "LeicaTPS300",
            Self::LeicaGeoCom => "LeicaGeoCOM",
            Self::LeicaGeoComStandard => "LeicaGeoCOMStandard",
            Self::LeicaTs11 => "LeicaTS11",
            Self::TrimbleS3 => "TrimbleS3",
            Self::TopconGts => "TopconGTS",
            Self::GnssNmea => "GnssNmea",
            Self::Manuell => "Manuell",
        }
    }

    /// Findet ein Modell anhand des gespeicherten Namens.
    pub fn aus_name(name: &str) -> Option<Self> {
        ALLE_MODELLE.into_iter().find(|m| m.name().eq_ignore_ascii_case(name))
    }

    /// Kann bei diesem Modell zwischen GeoCOM und GSI Online gewählt werden?
    pub fn sprache_waehlbar(self) -> bool {
        matches!(
            self,
            Self::LeicaTs06 | Self::LeicaTps300 | Self::LeicaGeoCom | Self::LeicaGeoComStandard | Self::LeicaTs11
        )
    }

    /// Standardsprache des Modells.
    pub fn standard_sprache(self) -> Sprache {
        if self == Self::LeicaTps300 { Sprache::GsiOnline } else { Sprache::GeoCom }
    }

    /// Voreinstellung für die serielle Schnittstelle.
    pub fn preset(self) -> SeriellPreset {
        let baud = match self {
            Self::LeicaGeoCom => BAUD_GEOCOM_SCHNELL,
            Self::GnssNmea => BAUD_NMEA,
            _ => BAUD_STANDARD,
        };
        SeriellPreset { baud, datenbits: 8 }
    }

    /// Anzeigetext für Auswahllisten.
    pub fn anzeige(self) -> &'static str {
        match self {
            Self::LeicaTs06 => "Leica TS06  (GeoCOM / GSI Online)",
            Self::LeicaTps300 => "Leica TPS300-Serie  (GSI Online / GeoCOM)  –  9600 Baud",
            Self::LeicaGeoCom => "Leica GeoCOM (TPS1200)  –  38400 Baud",
            Self::LeicaGeoComStandard => "Leica GeoCOM Standard (TPS1200)  –  9600 Baud",
            Self::LeicaTs11 => "Leica TS11",
            Self::TrimbleS3 => "Trimble S3",
            Self::TopconGts => "Topcon GTS/GPT",
            Self::GnssNmea => "GNSS  (NMEA 0183)",
            Self::Manuell => "Manuell",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn namen_sind_rundlauffaehig() {
        for m in ALLE_MODELLE {
            assert_eq!(TachymeterModell::aus_name(m.name()), Some(m));
        }
        assert_eq!(TachymeterModell::aus_name("gibtsnicht"), None);
        // Sokkia wird nicht mehr unterstützt
        assert_eq!(TachymeterModell::aus_name("SokkiaSDR"), None);
    }

    #[test]
    fn sprache_und_voreinstellungen() {
        assert_eq!(TachymeterModell::LeicaTps300.standard_sprache(), Sprache::GsiOnline);
        assert_eq!(TachymeterModell::LeicaTs06.standard_sprache(), Sprache::GeoCom);
        assert!(TachymeterModell::LeicaTs06.sprache_waehlbar());
        assert!(TachymeterModell::LeicaTps300.sprache_waehlbar());
        assert!(!TachymeterModell::TrimbleS3.sprache_waehlbar());
        assert!(!TachymeterModell::Manuell.sprache_waehlbar());
        assert_eq!(TachymeterModell::GnssNmea.preset().baud, 4800);
        assert_eq!(TachymeterModell::LeicaGeoCom.preset().baud, 38_400);
        assert_eq!(TachymeterModell::LeicaTps300.preset().baud, 9_600);
        // Die TS06 arbeitet im Praxistest über Bluetooth mit 9600 Baud (Protokoll vom 4.11.2026)
        assert_eq!(TachymeterModell::LeicaTs06.preset().baud, 9_600);
        assert_eq!(Sprache::aus_name(" gsi online "), Some(Sprache::GsiOnline));
        assert_eq!(Sprache::aus_name("x"), None);
    }
}
