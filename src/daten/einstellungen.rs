// Zweck:    Projekt- und Geräteeinstellungen (Einstellungen.xml) inkl. aktives Projekt und DXF-Zoom
// Hardware: Tachymeter-Verbindungsparameter (COM-Port, Baudrate)
// Autor:    Johann Gerner

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::grundlagen::format::{f, parse_zahl};
use crate::grundlagen::xml_einfach::{self, XmlDokument};
use crate::grundlagen::{datei_speicher, fehler_log, pfade};
use crate::tachymeter::modell::TachymeterModell;

/// Name des Wurzelelements der Einstellungsdatei.
const WURZEL: &str = "FeldbuchEinstellungen";
/// Dateiname der Einstellungsdatei.
const DATEINAME: &str = "Einstellungen.xml";
/// Standard-Messmodus.
const STANDARD_MESSMODUS: &str = "Prisma";
/// Standard-Prisma.
const STANDARD_PRISMA: &str = "GPR1 Standard";
/// Standard-Baudrate der seriellen Verbindung.
const STANDARD_BAUDRATE: i32 = 9600;
/// Standard-Datenbits der seriellen Verbindung.
const STANDARD_DATENBITS: i32 = 8;

/// Gespeicherter DXF-Zoom (pro DXF-Datei).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DxfZoom {
    pub datei: String,
    pub scale: f64,
    pub pan_x: f64,
    pub pan_y: f64,
}

/// Alle persistenten Einstellungen.
#[derive(Clone, Debug)]
pub struct Einstellungen {
    pfad: PathBuf,
    pub projekt_name: String,
    pub projekt_verzeichnis: PathBuf,

    pub protokoll_aktiv: bool,
    pub auto_backup: bool,
    pub koordinaten_tooltip: bool,
    pub ton_bei_berechnung: bool,
    pub erweiterte_protokollierung: bool,

    pub transform_fester_massstab: bool,
    pub transform_typ_index: i32,

    pub neupunkt_zaehler: i32,
    pub instrumenten_hoehe: f64,
    pub neupunkte_visible: bool,
    pub residual_visible: bool,
    pub letzter_modus_neupunkt: bool,
    /// Laserpointer soll eingeschaltet sein (wird vor der ersten Messung einer Verbindung eingestellt).
    pub laser_an: bool,

    pub tachymeter_modell: TachymeterModell,
    pub tachymeter_port: String,
    pub tachymeter_baud_rate: i32,
    pub tachymeter_data_bits: i32,
    pub tachymeter_paritaet: String,
    pub tachymeter_stop_bits: String,
    /// Steuerleitung DTR beim Öffnen setzen (Standard aus).
    pub tachymeter_dtr: bool,
    /// Steuerleitung RTS beim Öffnen setzen (Standard aus).
    pub tachymeter_rts: bool,
    /// Zeilenende beim Senden: "CRLF", "CR" oder "LF".
    pub tachymeter_zeilenende: String,
    /// Gerätesprache der Leica-Geräte: "GeoCOM" oder "GSI Online" (leer = Standard des Modells).
    pub tachymeter_sprache: String,

    pub mess_modus_name: String,
    pub prisma_name: String,
    /// Prismenkonstante [mm] in Leica-Konvention (GPR1 = 0).
    pub prisma_konstante_mm: f64,

    pub dxf_zoom: DxfZoom,
}

impl Einstellungen {
    /// Standardwerte mit Speicherort im Benutzerverzeichnis.
    pub fn neu() -> Self {
        Self::mit_pfad(pfade::benutzer_get(DATEINAME))
    }

    /// Standardwerte mit frei wählbarem Speicherort (für Tests).
    pub fn mit_pfad(pfad: PathBuf) -> Self {
        Self {
            pfad,
            projekt_name: String::new(),
            projekt_verzeichnis: pfade::benutzer_basis(),
            protokoll_aktiv: true,
            auto_backup: false,
            koordinaten_tooltip: true,
            ton_bei_berechnung: false,
            erweiterte_protokollierung: false,
            transform_fester_massstab: false,
            transform_typ_index: 0,
            neupunkt_zaehler: 1,
            instrumenten_hoehe: 0.0,
            neupunkte_visible: true,
            residual_visible: false,
            letzter_modus_neupunkt: true,
            laser_an: false,
            tachymeter_modell: TachymeterModell::Manuell,
            tachymeter_port: String::new(),
            tachymeter_baud_rate: STANDARD_BAUDRATE,
            tachymeter_data_bits: STANDARD_DATENBITS,
            tachymeter_paritaet: "None".to_string(),
            tachymeter_stop_bits: "1".to_string(),
            tachymeter_dtr: false,
            tachymeter_rts: false,
            tachymeter_zeilenende: "CRLF".to_string(),
            tachymeter_sprache: String::new(),
            mess_modus_name: STANDARD_MESSMODUS.to_string(),
            prisma_name: STANDARD_PRISMA.to_string(),
            prisma_konstante_mm: 0.0,
            dxf_zoom: DxfZoom::default(),
        }
    }

    /// Ist ein Projekt gewählt?
    pub fn ist_geladen(&self) -> bool {
        !self.projekt_name.is_empty()
    }

    /// Wird ein GNSS-Empfänger verwendet?
    pub fn ist_gnss_geraet(&self) -> bool {
        self.tachymeter_modell == TachymeterModell::GnssNmea
    }

    /// Pfad zu einer Datei im Projektverzeichnis.
    pub fn get_pfad(&self, dateiname: &str) -> PathBuf {
        self.projekt_verzeichnis.join(dateiname)
    }

    /// Pfad zur DXF-Datei des Projekts (Projektname.dxf) oder leer.
    pub fn dxf_pfad(&self) -> Option<PathBuf> {
        self.ist_geladen()
            .then(|| self.projekt_verzeichnis.join(format!("{}.dxf", self.projekt_name)))
    }

    /// Existiert die DXF-Datei des Projekts?
    pub fn hat_dxf_datei(&self) -> bool {
        self.dxf_pfad().is_some_and(|p| p.exists())
    }

    /// Gespeicherter Zoom, sofern er zur DXF-Datei passt.
    pub fn lade_zoom(&self, dxf_pfad: &str) -> Option<(f64, f64, f64)> {
        let z = &self.dxf_zoom;
        (z.scale > 0.0 && z.datei.eq_ignore_ascii_case(dxf_pfad)).then_some((z.scale, z.pan_x, z.pan_y))
    }

    /// Speichert den Zoom einer DXF-Datei.
    pub fn speichere_zoom(&mut self, dxf_pfad: &str, scale: f64, pan_x: f64, pan_y: f64) -> io::Result<()> {
        self.dxf_zoom = DxfZoom {
            datei: dxf_pfad.to_string(),
            scale,
            pan_x,
            pan_y,
        };
        self.speichern()
    }

    /// Setzt das aktive Projekt (Name und Verzeichnis werden geprüft, das Verzeichnis angelegt).
    pub fn set_projekt(&mut self, name: &str, verzeichnis: &Path) -> Result<(), String> {
        let name = name.trim();
        const UNGUELTIG: [char; 9] = ['<', '>', ':', '"', '/', '\\', '|', '?', '*'];
        if name.is_empty() || name.contains(UNGUELTIG) || name.chars().any(char::is_control) {
            return Err("Bitte einen gültigen Projektnamen eingeben.".to_string());
        }
        fs::create_dir_all(verzeichnis).map_err(|e| e.to_string())?;
        let voll = std::path::absolute(verzeichnis).map_err(|e| e.to_string())?;
        let vorher = (self.projekt_name.clone(), self.projekt_verzeichnis.clone());
        self.projekt_name = name.to_string();
        self.projekt_verzeichnis = voll;
        if let Err(e) = self.speichern() {
            (self.projekt_name, self.projekt_verzeichnis) = vorher;
            return Err(e.to_string());
        }
        Ok(())
    }

    /// Legt ein leeres Projekt an und setzt alle projektbezogenen Zähler zurück.
    pub fn neues_leeres_projekt(&mut self, name: &str, verzeichnis: &Path) -> Result<(), String> {
        self.neupunkt_zaehler = 1;
        self.letzter_modus_neupunkt = true;
        self.instrumenten_hoehe = 0.0;
        self.dxf_zoom = DxfZoom::default();
        self.set_projekt(name, verzeichnis)
    }

    /// Lädt die Einstellungen aus der Datei; defekte Dateien werden beiseitegelegt.
    pub fn laden(&mut self) {
        let pfad = self.pfad.clone();
        if !pfad.exists() {
            return;
        }
        let inhalt = match fs::read_to_string(&pfad) {
            Ok(text) => text,
            Err(e) => {
                datei_speicher::melde_ladefehler(&pfad, &e);
                return;
            }
        };
        match xml_einfach::lese(&inhalt) {
            Ok(doc) if doc.wurzel == WURZEL => {
                self.uebernimm(&doc);
                datei_speicher::laden_erfolgreich(&pfad);
            }
            Ok(doc) => self.behandle_defekt(&pfad, &format!("Ungültige Einstellungsdatei (Wurzel <{}>).", doc.wurzel)),
            Err(fehler) => self.behandle_defekt(&pfad, &fehler),
        }
    }

    fn behandle_defekt(&mut self, pfad: &Path, fehler: &str) {
        const HINWEIS: &str = "Es gelten die Standardeinstellungen. Bitte vor dem Messen Projekt, Tachymeter, \
                               Messmodus und Prismenkonstante prüfen. Die Punktzähler beginnen wieder bei 1.";
        if let Err(e) = datei_speicher::lege_defekte_datei_beiseite(pfad, &fehler, HINWEIS) {
            datei_speicher::melde_ladefehler(pfad, &e);
        }
    }

    fn uebernimm(&mut self, doc: &XmlDokument) {
        let text = |tag: &str| doc.text(tag).map(str::trim);
        let wahrheit = |tag: &str, standard: bool| match text(tag) {
            Some(t) => matches!(t.to_lowercase().as_str(), "true" | "1" | "yes"),
            None => standard,
        };
        let ganzzahl = |tag: &str, standard: i32| text(tag).and_then(|t| t.parse().ok()).unwrap_or(standard);
        let zahl = |tag: &str| text(tag).and_then(parse_zahl).unwrap_or(0.0);

        let name = text("LetztesProjektName").unwrap_or("");
        let verzeichnis = text("LetztesProjektVerzeichnis").unwrap_or("");
        if !name.is_empty() && Path::new(verzeichnis).is_dir() {
            self.projekt_name = name.to_string();
            self.projekt_verzeichnis = PathBuf::from(verzeichnis);
        }

        self.transform_fester_massstab = wahrheit("TransformFesterMassstab", false);
        if let Some(i) = text("TransformTypIndex").and_then(|t| t.parse().ok()) {
            self.transform_typ_index = i;
        }
        self.protokoll_aktiv = wahrheit("ProtokollAktiv", true);
        self.auto_backup = wahrheit("AutoBackup", false);
        self.koordinaten_tooltip = wahrheit("KoordinatenTooltip", true);
        self.ton_bei_berechnung = wahrheit("TonBeiBerechnung", false);
        self.erweiterte_protokollierung = wahrheit("ErweiterteProtokollierung", false);

        if let Some(n) = text("NeupunktZaehler").and_then(|t| t.parse().ok()) {
            self.neupunkt_zaehler = n;
        }
        self.instrumenten_hoehe = zahl("InstrumentenHoehe");
        self.neupunkte_visible = wahrheit("NeupunkteVisible", true);
        self.residual_visible = wahrheit("ResidualVisible", false);
        self.letzter_modus_neupunkt = wahrheit("LetzterModusNeupunkt", true);
        self.laser_an = wahrheit("LaserAn", false);

        // Modell mit Migration alter Namen
        let modell_text = match text("TachymeterModell").unwrap_or("") {
            "SokkiaSET" => "SokkiaSDR",
            "TopconGPT3000" => "TopconGTS",
            anderes => anderes,
        };
        if let Some(m) = TachymeterModell::aus_name(modell_text) {
            self.tachymeter_modell = m;
        }
        if self.tachymeter_modell != TachymeterModell::GnssNmea
            && text("IstGnssGeraet").is_some_and(|t| t.eq_ignore_ascii_case("true"))
        {
            self.tachymeter_modell = TachymeterModell::GnssNmea;
        }
        self.tachymeter_port = text("TachymeterPort").unwrap_or("").to_string();
        self.tachymeter_baud_rate = ganzzahl("TachymeterBaudRate", self.tachymeter_baud_rate);
        self.tachymeter_data_bits = ganzzahl("TachymeterDataBits", self.tachymeter_data_bits);
        self.tachymeter_paritaet = text("TachymeterParitaet").unwrap_or("None").to_string();
        self.tachymeter_stop_bits = text("TachymeterStopBits").unwrap_or("1").to_string();
        self.tachymeter_dtr = wahrheit("TachymeterDtr", false);
        self.tachymeter_rts = wahrheit("TachymeterRts", false);
        self.tachymeter_zeilenende = text("TachymeterZeilenende").filter(|t| !t.is_empty()).unwrap_or("CRLF").to_string();
        self.tachymeter_sprache = text("TachymeterSprache").unwrap_or("").to_string();

        if let Some(m) = text("MessModus").filter(|t| !t.is_empty()) {
            self.mess_modus_name = m.to_string();
        }
        if let Some(p) = text("PrismaName").filter(|t| !t.is_empty()) {
            self.prisma_name = p.to_string();
        }
        if let Some(k) = text("PrismaKonstanteMm").and_then(parse_zahl) {
            self.prisma_konstante_mm = k;
        }

        self.dxf_zoom = DxfZoom {
            datei: text("DxfZoomDatei").unwrap_or("").to_string(),
            scale: zahl("DxfZoomScale"),
            pan_x: zahl("DxfZoomPanX"),
            pan_y: zahl("DxfZoomPanY"),
        };
    }

    /// Speichert alle Einstellungen atomar.
    pub fn speichern(&self) -> io::Result<()> {
        datei_speicher::setze_auto_backup(self.auto_backup);
        let wahr = |b: bool| if b { "true" } else { "false" }.to_string();
        let eintraege: Vec<(&str, String)> = vec![
            ("LetztesProjektName", self.projekt_name.clone()),
            ("LetztesProjektVerzeichnis", self.projekt_verzeichnis.to_string_lossy().into_owned()),
            ("TransformFesterMassstab", wahr(self.transform_fester_massstab)),
            ("TransformTypIndex", self.transform_typ_index.to_string()),
            ("ProtokollAktiv", wahr(self.protokoll_aktiv)),
            ("AutoBackup", wahr(self.auto_backup)),
            ("KoordinatenTooltip", wahr(self.koordinaten_tooltip)),
            ("TonBeiBerechnung", wahr(self.ton_bei_berechnung)),
            ("ErweiterteProtokollierung", wahr(self.erweiterte_protokollierung)),
            ("NeupunktZaehler", self.neupunkt_zaehler.to_string()),
            ("InstrumentenHoehe", f(self.instrumenten_hoehe, 3)),
            ("NeupunkteVisible", wahr(self.neupunkte_visible)),
            ("ResidualVisible", wahr(self.residual_visible)),
            ("LetzterModusNeupunkt", wahr(self.letzter_modus_neupunkt)),
            ("LaserAn", wahr(self.laser_an)),
            ("TachymeterModell", self.tachymeter_modell.name().to_string()),
            ("TachymeterPort", self.tachymeter_port.clone()),
            ("TachymeterBaudRate", self.tachymeter_baud_rate.to_string()),
            ("TachymeterDataBits", self.tachymeter_data_bits.to_string()),
            ("TachymeterParitaet", self.tachymeter_paritaet.clone()),
            ("TachymeterStopBits", self.tachymeter_stop_bits.clone()),
            ("TachymeterDtr", wahr(self.tachymeter_dtr)),
            ("TachymeterRts", wahr(self.tachymeter_rts)),
            ("TachymeterZeilenende", self.tachymeter_zeilenende.clone()),
            ("TachymeterSprache", self.tachymeter_sprache.clone()),
            ("MessModus", self.mess_modus_name.clone()),
            ("PrismaName", self.prisma_name.clone()),
            ("PrismaKonstanteMm", f(self.prisma_konstante_mm, 1)),
            ("DxfZoomDatei", self.dxf_zoom.datei.clone()),
            ("DxfZoomScale", self.dxf_zoom.scale.to_string()),
            ("DxfZoomPanX", self.dxf_zoom.pan_x.to_string()),
            ("DxfZoomPanY", self.dxf_zoom.pan_y.to_string()),
        ];
        let xml = xml_einfach::schreibe(WURZEL, &eintraege);
        datei_speicher::schreibe_text(&self.pfad, &xml).inspect_err(|e| {
            fehler_log::log("Einstellungen.speichern", &e.to_string());
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_pfad(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("fb-einst-{name}-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        dir.join(DATEINAME)
    }

    #[test]
    fn speichern_und_laden_ist_verlustfrei() {
        let pfad = test_pfad("roundtrip");
        let projekt_dir = pfad.parent().unwrap().to_path_buf();
        let mut e = Einstellungen::mit_pfad(pfad.clone());
        e.set_projekt("Demo", &projekt_dir).unwrap();
        e.neupunkt_zaehler = 17;
        e.instrumenten_hoehe = 1.556;
        e.tachymeter_modell = TachymeterModell::LeicaTs06;
        e.tachymeter_sprache = "GSI Online".into();
        e.tachymeter_dtr = true;
        e.tachymeter_zeilenende = "LF".into();
        e.tachymeter_port = "COM7".into();
        e.prisma_konstante_mm = -17.5;
        e.dxf_zoom = DxfZoom { datei: "a.dxf".into(), scale: 0.125, pan_x: 1.5, pan_y: -2.0 };
        e.speichern().unwrap();

        let mut neu = Einstellungen::mit_pfad(pfad.clone());
        neu.laden();
        assert_eq!(neu.projekt_name, "Demo");
        assert_eq!(neu.neupunkt_zaehler, 17);
        assert!((neu.instrumenten_hoehe - 1.556).abs() < 1e-9);
        assert_eq!(neu.tachymeter_modell, TachymeterModell::LeicaTs06);
        assert_eq!(neu.tachymeter_sprache, "GSI Online");
        assert!(neu.tachymeter_dtr && !neu.tachymeter_rts);
        assert_eq!(neu.tachymeter_zeilenende, "LF");
        assert_eq!(neu.tachymeter_port, "COM7");
        assert!((neu.prisma_konstante_mm + 17.5).abs() < 1e-9);
        assert_eq!(neu.lade_zoom("A.DXF"), Some((0.125, 1.5, -2.0)));
        let _ = fs::remove_dir_all(projekt_dir);
    }

    #[test]
    fn ungueltiger_projektname_wird_abgelehnt() {
        let mut e = Einstellungen::mit_pfad(test_pfad("name"));
        assert!(e.set_projekt("  ", Path::new(".")).is_err());
        assert!(e.set_projekt("a/b", Path::new(".")).is_err());
    }

    #[test]
    fn sokkia_wird_nicht_mehr_unterstuetzt_und_faellt_auf_manuell() {
        let pfad = test_pfad("sokkia");
        fs::write(&pfad, "<FeldbuchEinstellungen><TachymeterModell>SokkiaSET</TachymeterModell></FeldbuchEinstellungen>").unwrap();
        let mut e = Einstellungen::mit_pfad(pfad.clone());
        e.laden();
        assert_eq!(e.tachymeter_modell, TachymeterModell::Manuell);
        let _ = fs::remove_dir_all(pfad.parent().unwrap());
    }

    #[test]
    fn alte_modellnamen_werden_migriert() {
        let pfad = test_pfad("migration");
        fs::write(
            &pfad,
            "<FeldbuchEinstellungen><TachymeterModell>TopconGPT3000</TachymeterModell></FeldbuchEinstellungen>",
        )
        .unwrap();
        let mut e = Einstellungen::mit_pfad(pfad.clone());
        e.laden();
        assert_eq!(e.tachymeter_modell, TachymeterModell::TopconGts);
        let _ = fs::remove_dir_all(pfad.parent().unwrap());
    }
}
