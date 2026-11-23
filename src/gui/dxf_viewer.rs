// Zweck:    DXF-Viewer als eigenes Fenster, Nachbau von FormDxfViewer/DxfCanvas der Originalversion:
//           Toolbar, Seitenleiste, Statusleiste, Zeichenfläche mit Zoom/Pan/Snap, Overlays,
//           Import (KOR/CSV/JSON), Digitalisierung, Löschen per Fenster, Punkt-Index, Festpunkte
// Hardware: Tachymeter-Funktionen (Messen, EDM, Laser) sind noch nicht angebunden
// Autor:    Johann Gerner

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

use eframe::egui::{self, Align, Align2, Color32, FontId, Layout, Pos2, Rect, RichText, Sense, Stroke, Vec2};

use crate::daten::modelle::{NeupunktErgebnis, StationierungsErgebnis, TYP_STANDPUNKT};
use crate::daten::projekt::Projekt;
use crate::gui::bildschirmtastatur;
use crate::daten::punktlisten::{lese_csv, lese_json};
use crate::dxf::entity::{Ansicht, Art, DxfEntity, Rahmen};
use crate::dxf::{leser, punktindex};
use crate::grundlagen::datei_speicher;
use crate::grundlagen::format::{de, f, parse_zahl};
use crate::grundlagen::protokoll;
use crate::gui::geraet::Geraet;
use crate::rechnen::polar::EXZ_INFO_GRENZE_M;

// ── Farben des Originals ──────────────────────────────────────────────────────
const FENSTER_FARBE: Color32 = Color32::from_rgb(228, 230, 235);
const TOOLBAR_FARBE: Color32 = Color32::from_rgb(42, 72, 130);
const FELD_FARBE: Color32 = Color32::from_rgb(55, 88, 150);
const LABEL_FARBE: Color32 = Color32::from_rgb(185, 205, 240);
const TRENNER_FARBE: Color32 = Color32::from_rgb(80, 110, 170);
const TOOLBAR_KNOPF: Color32 = Color32::from_rgb(60, 95, 160);
const TOOLBAR_KNOPF_RAND: Color32 = Color32::from_rgb(80, 115, 185);
const SEITE_FARBE: Color32 = Color32::from_rgb(52, 56, 70);
const STATUS_FARBE: Color32 = Color32::from_rgb(210, 213, 222);
const STATUS_TEXT: Color32 = Color32::from_rgb(40, 45, 70);
const HINTERGRUND: Color32 = Color32::WHITE;
const STANDARD_LINIE: Color32 = Color32::from_rgb(40, 40, 100);
const AUSWAHL_FARBE: Color32 = Color32::RED;
const FADENKREUZ_FARBE: Color32 = Color32::from_rgb(200, 200, 200);
const SNAP_FARBE: Color32 = Color32::from_rgb(220, 100, 0);
const EXZ_GROSS_HINTERGRUND: Color32 = Color32::from_rgb(255, 230, 100);

// ── Maße des Originals ────────────────────────────────────────────────────────
const TOOLBAR_HOEHE: f32 = 46.0;
const STATUS_HOEHE: f32 = 40.0;
const SEITE_BREITE: f32 = 46.0;
const KNOPF_GROESSE: f32 = 36.0;
/// Breite des Modus-Knopfs (in beiden Zuständen gleich) und Abstand zum DXF-Knopf.
const MODUS_KNOPF_BREITE: f32 = 100.0;
const MODUS_KNOPF_ABSTAND: f32 = 12.0;
/// Breite des Messen-Knopfs (bzw. „Übernehmen“).
const MESSEN_KNOPF_BREITE: f32 = 80.0;
/// Breite der Knöpfe „Neuer Stp" und „Kontrolle".
const NEUER_STP_BREITE: f32 = 52.0;
/// Breite des Umschalters für die getrennte Messung.
const GETRENNT_KNOPF_BREITE: f32 = 54.0;
const STATUS_LABEL_BREITE: f32 = 480.0;
/// Fensterstartgröße des Viewers.
const FENSTER_GROESSE: [f32; 2] = [1200.0, 780.0];
/// Zoomfaktor der Zoom-Knöpfe.
const ZOOM_KNOPF_FAKTOR: f64 = 1.5;
/// Zoomfaktor pro Mausradraste.
const ZOOM_RAD_FAKTOR: f64 = 1.25;
/// Kleinster erlaubter Maßstab.
const MIN_MASSSTAB: f64 = 1e-9;
/// Größter erlaubter Maßstab ohne Begrenzung.
const KEIN_MAX_MASSSTAB: f64 = 1e9;
/// Pixel pro Zoll des Originals; Maßstab 1:1 entspricht 96 dpi.
const DPI: f64 = 96.0;
/// Umrechnung des Konfidenzniveaus (Anteil) in Prozent für die Beschriftung.
const KONFIDENZ_PROZENT: f64 = 100.0;
/// Meter pro Zoll.
const M_PRO_ZOLL: f64 = 0.0254;
/// Rand beim Einpassen.
const FIT_RAND: f64 = 0.9;
/// Pick-Radius in Pixeln.
const PICK_RADIUS_PX: f64 = 14.0;
/// Pick-Radius der Punktmarker in Pixeln.
const MARKER_PICK_PX: f64 = 20.0;
/// Zeichengrenze (Koordinaten außerhalb werden nicht gezeichnet).
const ZEICHEN_GRENZE: f32 = 100_000.0;
/// Mindestgröße des Auswahlrechtecks in Pixeln.
const MIN_AUSWAHL_PX: f32 = 4.0;
/// Kleinster Radius der lokalen Ansicht [m].
const LOKAL_MIN_RADIUS_M: f64 = 20.0;
/// Rand um die äußersten Punkte der lokalen Ansicht.
const LOKAL_RAND_FAKTOR: f64 = 1.15;
/// Anzahl der Punktnummern, die im Löschdialog aufgelistet werden.
const LOESCH_LISTE_MAX: usize = 8;
/// Schwelle s0 [mm] für „bedingt brauchbar".
const S0_BEDINGT_MM: f64 = 5.0;
/// Schwelle s0 [mm] für „schwach".
const S0_SCHWACH_MM: f64 = 15.0;

mod absteckung;
mod auswahl;
mod export;
mod pdf_export;
mod messdaten_eingabe;
mod messen;
mod standpunkt;
mod zeichnen;

/// A-Taste des Panasonic Toughpad (Panasonic-Einstellungen → Tablet-Tasten): Strg+Alt+E, löst „Messen" aus.
/// Auf deutscher Tastatur ist das AltGr+E und erzeugt das Zeichen „€".
const MESS_TASTE: egui::Key = egui::Key::E;
const MESS_TASTE_ZEICHEN: &str = "€";

/// Auffrischintervall des Viewers (Zielrichtung, Messablauf), solange ein Tachymeter verbunden ist oder misst.
const REPAINT_INTERVALL: std::time::Duration = std::time::Duration::from_millis(100);
/// Auffrischintervall ohne Tachymeter (nur noch für Statusmeldungen und Dialoge).
const REPAINT_INTERVALL_RUHE: std::time::Duration = std::time::Duration::from_millis(1000);
/// Nach einer Änderung der Ansicht (Zoom, Verschieben) wird so lange vereinfacht gezeichnet, danach folgt das volle Bild.
const LOD_NACHLAUF: std::time::Duration = std::time::Duration::from_millis(180);
/// Zuschlag zum Zeichenbereich, damit Symbole und Texte am Rand nicht abgeschnitten werden [px].
const SICHT_RAND_PX: f64 = 40.0;
/// Linien, Bögen und Polylinien, die kleiner sind (größte Ausdehnung in px), werden nicht gezeichnet.
const KLEIN_PX: f64 = 0.8;
/// Dasselbe während der Bewegung.
const KLEIN_PX_BEWEGUNG: f64 = 2.5;
/// Texte unter dieser Höhe [px] werden nicht gezeichnet.
const TEXT_MIN_PX: f64 = 4.0;
/// Geschätzte Zeichenbreite im Verhältnis zur Texthöhe (für die Sichtbarkeitsprüfung von Texten).
const TEXT_BREITE_FAKTOR: f64 = 0.7;
/// Kleinste Schrifthöhe beim Zeichnen [px] (wie in `DxfEntity::zeichne`).
const TEXT_MIN_ZEICHEN_PX: f64 = 6.0;
/// Während der Bewegung werden Polylinienpunkte, die näher als dieser Abstand [px] beieinander liegen, ausgelassen.
const DUENN_PX: f32 = 2.0;
/// Weiches Zoomen: Zeitkonstante [s], längster und kürzester Zeitschritt [s] und Abbruchschwelle (ln des Restfaktors).
const ZOOM_ZEITKONSTANTE_S: f64 = 0.07;
const ZOOM_DT_MAX_S: f32 = 0.05;
const ZOOM_DT_MIN_S: f32 = 1.0 / 120.0;
const ZOOM_ENDE_LOG: f64 = 0.003;
/// Zulässige relative Abweichung des Maßstabs, bevor eine laufende Zoom-Animation verworfen wird (Maßstab wurde anderswo gesetzt).
const ZOOM_FREMD_TOLERANZ: f64 = 1e-9;

use zeichnen::{FeldArt, trenner, zeichne_lampe};

/// Bezeichnung des Import-Layers der gespeicherten Importpunkte.
const IMPORT_LAYER_NAME: &str = "ImportPunkte.json";

/// Ausgewählte Entity.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Auswahl {
    Zeichnung(usize),
    Overlay(usize),
    Neupunkt(usize),
    Import(usize, usize),
    Marker(usize),
}

/// Wählt aus (Abstand [px], Treffer, Fangradius [px]) den nächsten Treffer innerhalb seines Fangradius.
fn naechster_treffer(kandidaten: impl Iterator<Item = (f64, Auswahl, f64)>) -> Option<Auswahl> {
    kandidaten.filter(|(d, _, r)| d.is_finite() && d < r).min_by(|a, b| a.0.total_cmp(&b.0)).map(|(_, a, _)| a)
}

#[cfg(test)]
mod pick_tests {
    use super::*;

    #[test]
    fn naechster_treffer_gewinnt_unabhaengig_von_der_reihenfolge() {
        // Audit D2: Marker A 1 px neben B, Klick genau auf B → B
        let a = (1.0, Auswahl::Marker(0), MARKER_PICK_PX);
        let b = (0.0, Auswahl::Marker(1), MARKER_PICK_PX);
        assert_eq!(naechster_treffer([a, b].into_iter()), Some(Auswahl::Marker(1)));
        assert_eq!(naechster_treffer([b, a].into_iter()), Some(Auswahl::Marker(1)));
        // Ein näherer Importpunkt wird nicht von einem weiter entfernten Marker verdrängt
        let marker = (12.0, Auswahl::Marker(0), MARKER_PICK_PX);
        let import = (3.0, Auswahl::Import(0, 4), PICK_RADIUS_PX);
        assert_eq!(naechster_treffer([marker, import].into_iter()), Some(Auswahl::Import(0, 4)));
        // Außerhalb des eigenen Radius kein Treffer, der größere Markerradius gilt nur für Marker
        assert_eq!(naechster_treffer([(16.0, Auswahl::Zeichnung(0), PICK_RADIUS_PX)].into_iter()), None);
        assert_eq!(naechster_treffer([(16.0, Auswahl::Marker(2), MARKER_PICK_PX)].into_iter()), Some(Auswahl::Marker(2)));
    }
}

/// Import-Layer: je Datei ein Satz Punkte mit Sichtbarkeit.
struct ImportLayer {
    name: String,
    entities: Vec<DxfEntity>,
    sichtbar: bool,
}

/// Kandidat der Festpunktübernahme im Dialog.
struct FestpunktZeile {
    ergebnis: NeupunktErgebnis,
    gewaehlt: bool,
    vorhanden: bool,
}

/// Offener Dialog.
enum Dialog {
    Hinweis { titel: String, text: String },
    Fehler { titel: String, text: String },
    DxfLeeren,
    IndexReset,
    Loeschen { text: String, anzahl: usize, ziele: LoeschZiele },
    Festpunkte { zeilen: Vec<FestpunktZeile>, rueckfrage: Option<String>, fehler: String },
    Prisma { auswahl: Option<usize>, konstante: String },
    DxfExport { massstab: String, symbol: String, text: String },
    Messdaten(Box<messdaten_eingabe::MessdatenEingabe>),
    /// Dialog „Neuer Stp" (neu, fortsetzen, umbenennen).
    Standpunkt(Box<crate::gui::standpunkt_dialog::StandpunktDialog>),
    /// Rückfrage zur Korrektur der Instrumentenhöhe der aktiven Aufstellung.
    IhKorrektur { alt: f64, neu: f64 },
    /// Rückfrage beim Öffnen: gilt eine ältere Aufstellung noch?
    Aufstellungspruefung { text: String },
    /// Orientierungskontrolle auf einen Anschlusspunkt.
    Kontrolle(Box<standpunkt::KontrollDialog>),
}

/// Zu löschende Punkte.
struct LoeschZiele {
    feldbuch: Vec<String>,
    neupunkte: Vec<crate::daten::punktlisten::NeupunktSchluessel>,
    importpunkte: Vec<String>,
}

/// Zwischengespeicherte Daten der DXF-Zeichnung (einmal je Datei statt je Bild berechnet).
#[derive(Default)]
struct ZeichenCache {
    /// Gültig für den Vec mit dieser Adresse und Länge.
    schluessel: (usize, usize),
    bounds: Vec<Rahmen>,
    farben: Vec<Color32>,
    /// Indizes der Linien, Bögen, Texte … und der Katasterpunkte (INSERT), in Zeichenreihenfolge.
    linien: Vec<usize>,
    inserts: Vec<usize>,
}

/// Ziel einer weichen Zoom-Animation.
#[derive(Clone, Copy, Debug)]
struct ZoomZiel {
    scale: f64,
    anker: Pos2,
    ursprung: Pos2,
    /// Maßstab nach dem letzten Animationsschritt; weicht er ab, wurde der Zoom anderswo geändert.
    erwartet: f64,
}

/// Wunsch des Viewers an das Hauptfenster.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Wunsch {
    FreieStationierung,
    /// Polarpunkte öffnen.
    Polarpunkte,
    /// Aufnahmetest öffnen.
    Aufnahmetest,
    /// Aufnahmetest öffnen und den Punkt mit dieser Nummer anfahren.
    Anfahren(String),
}

/// Der DXF-Viewer.
pub struct DxfViewer {
    initialisiert: bool,
    // Zeichenfläche
    entities: Vec<DxfEntity>,
    overlay: Vec<DxfEntity>,
    neupunkte: Vec<DxfEntity>,
    residuen: Vec<DxfEntity>,
    ellipsen: Vec<DxfEntity>,
    import_layer: Vec<ImportLayer>,
    marker: Vec<DxfEntity>,
    punkt_index: Option<punktindex::PunktIndex>,
    snap_aktiv: bool,
    punkte_sichtbar: bool,
    dxf_sichtbar: bool,
    neupunkte_sichtbar: bool,
    residuen_sichtbar: bool,
    marker_sichtbar: bool,
    loesch_modus: bool,
    scale: f64,
    mitte: [f64; 2],
    max_scale: f64,
    ellipsen_skalierung: f64,
    auswahl: Option<Auswahl>,
    snap_punkt: Option<(f64, f64)>,
    auswahl_start: Option<Pos2>,
    auswahl_ende: Option<Pos2>,
    kontext_welt: (f64, f64),
    canvas_groesse: Vec2,
    canvas_rect: Rect,
    fit_ausstehend: bool,
    lokal_zeigen_ausstehend: bool,
    pdf_mitte: Option<[f64; 2]>,
    pdf_ziehen: bool,
    pdf_vorschau: std::cell::RefCell<pdf_export::Vorschau>,
    // Formular
    dxf_pfad: String,
    status: String,
    lokales_system: bool,
    neupunkt_modus: bool,
    /// Getrennte Messung im Neupunktmodus: erst Strecke messen, dann Ziel anpeilen und Hz/V übernehmen.
    getrennt_modus: bool,
    /// Stand von `Projekt::station_version`, für den die Anzeigen zuletzt gebaut wurden.
    gesehene_station_version: u64,
    /// Strecke der getrennten Messung, bis Hz/V übernommen werden.
    getrennt_messung: Option<crate::tachymeter::messung::TachymeterMessung>,
    neupunkt_zaehler: i32,
    /// Zielstrahlen vom Standpunkt zu den Anschlusspunkten anzeigen.
    zielstrahlen_sichtbar: bool,
    /// Abstecken-Modus mit Zielpunkt, Messungen und Absteckfenster (Absteckung.md).
    absteckung: Option<Box<absteckung::Absteckung>>,
    txt_instr_hoehe: String,
    txt_zielhoehe: String,
    txt_exz_laengs: String,
    txt_exz_quer: String,
    txt_code: String,
    txt_punkt_nr: String,
    /// Die A-Taste wurde in diesem Bildaufbau gedrückt.
    mess_taste: bool,
    /// Zwischenspeicher für Umschließungsrechtecke und Farben der DXF-Zeichnung.
    zeichen_cache: std::cell::RefCell<ZeichenCache>,
    /// Bis dahin wird vereinfacht gezeichnet (Ansicht wurde gerade geändert).
    bewegung_bis: Option<std::time::Instant>,
    bewegung_aktiv: bool,
    letzte_ansicht: (f64, [f64; 2]),
    /// Laufende weiche Zoom-Animation (Mausrad).
    zoom_ziel: Option<ZoomZiel>,
    /// Zuletzt gemessene Horizontalrichtung der Zielrichtung [gon].
    ziel_hz: Option<f64>,
    /// Zuletzt im Dialog „Messdaten eingeben" verwendeter Code und Zielhöhe.
    letzter_code: String,
    letzte_zielhoehe: String,
    // Digitalisierung
    digital_modus: bool,
    digital_zaehler: i32,
    digital_kor_pfad: Option<PathBuf>,
    digital_layer: Option<usize>,
    // Dialog und Wünsche
    dialog: Option<Dialog>,
    wunsch: Option<Wunsch>,
    war_fokussiert: bool,
    /// Die Startgröße (Full HD oder maximiert) des Viewer-Fensters wurde bereits festgelegt.
    startgroesse_gesetzt: bool,
    offen: bool,
    /// Auswahlmodus für die Konvertierung (eigene Instanz, nur grafische Funktionen).
    auswahl_modus: Option<Box<auswahl::AuswahlModus>>,
}

impl Default for DxfViewer {
    fn default() -> Self {
        Self {
            initialisiert: false,
            entities: Vec::new(),
            overlay: Vec::new(),
            neupunkte: Vec::new(),
            residuen: Vec::new(),
            ellipsen: Vec::new(),
            import_layer: Vec::new(),
            marker: Vec::new(),
            punkt_index: None,
            snap_aktiv: true,
            punkte_sichtbar: true,
            dxf_sichtbar: true,
            neupunkte_sichtbar: true,
            residuen_sichtbar: true,
            marker_sichtbar: true,
            loesch_modus: false,
            scale: 1.0,
            mitte: [0.0, 0.0],
            max_scale: DPI / M_PRO_ZOLL,
            ellipsen_skalierung: 1.0,
            auswahl: None,
            snap_punkt: None,
            auswahl_start: None,
            auswahl_ende: None,
            kontext_welt: (0.0, 0.0),
            canvas_groesse: Vec2::new(800.0, 600.0),
            canvas_rect: Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0)),
            fit_ausstehend: false,
            lokal_zeigen_ausstehend: false,
            pdf_mitte: None,
            pdf_ziehen: false,
            pdf_vorschau: std::cell::RefCell::new(pdf_export::Vorschau::default()),
            dxf_pfad: String::new(),
            status: "DXF-Datei öffnen …".to_string(),
            lokales_system: false,
            neupunkt_modus: false,
            getrennt_modus: false,
            gesehene_station_version: 0,
            getrennt_messung: None,
            neupunkt_zaehler: 1,
            zielstrahlen_sichtbar: true,
            absteckung: None,
            txt_instr_hoehe: "0.000".to_string(),
            txt_zielhoehe: "0.000".to_string(),
            txt_exz_laengs: "0.000".to_string(),
            txt_exz_quer: "0.000".to_string(),
            txt_code: String::new(),
            txt_punkt_nr: String::new(),
            mess_taste: false,
            zeichen_cache: std::cell::RefCell::new(ZeichenCache::default()),
            bewegung_bis: None,
            bewegung_aktiv: false,
            letzte_ansicht: (0.0, [0.0, 0.0]),
            zoom_ziel: None,
            ziel_hz: None,
            letzter_code: String::new(),
            letzte_zielhoehe: messdaten_eingabe::STANDARD_ZIELHOEHE.to_string(),
            digital_modus: false,
            digital_zaehler: 1,
            digital_kor_pfad: None,
            digital_layer: None,
            dialog: None,
            wunsch: None,
            war_fokussiert: true,
            startgroesse_gesetzt: false,
            offen: true,
            auswahl_modus: None,
        }
    }
}

/// Layerabhängige Farbe (Flurstücke grün, Gebäude schwarz).
fn layer_farbe(layer: &str) -> Option<Color32> {
    let l = layer.to_lowercase();
    if l.contains("flurstueck") || (l.contains("grenze") && !l.contains("grenzpunkt")) {
        return Some(Color32::from_rgb(0, 140, 0));
    }
    if l.contains("gebaeude") || l.contains("bauwerk") {
        return Some(Color32::BLACK);
    }
    None
}

/// Rundet einen Maßstabsnenner wie im Original.
pub fn schoen_runden(v: f64) -> i64 {
    let (teiler, _) = if v < 10.0 {
        (1.0, 0)
    } else if v < 100.0 {
        (5.0, 0)
    } else if v < 1000.0 {
        (10.0, 0)
    } else if v < 10_000.0 {
        (50.0, 0)
    } else {
        (100.0, 0)
    };
    ((v / teiler).round() * teiler) as i64
}

impl DxfViewer {
    /// Wunsch an das Hauptfenster abholen.
    pub fn nimm_wunsch(&mut self) -> Option<Wunsch> {
        self.wunsch.take()
    }

    pub fn ist_offen(&self) -> bool {
        self.offen
    }

    // ── Initialisierung ───────────────────────────────────────────────────────

    fn initialisiere(&mut self, projekt: &mut Projekt) {
        self.initialisiert = true;
        self.neupunkt_zaehler = projekt.einstellungen.neupunkt_zaehler;

        projekt.lade_aktuellen_standpunkt();
        let ih = if projekt.hat_aktive_aufstellung() { projekt.station.instrumenten_hoehe } else { projekt.einstellungen.instrumenten_hoehe };
        self.txt_instr_hoehe = f(ih, 3);
        self.gesehene_station_version = projekt.station_version;

        self.snap_aktiv = projekt.projektdaten.get_value("Einstellungen", "Snap") != Some("Inaktiv");
        self.punkte_sichtbar = projekt.projektdaten.get_value("Einstellungen", "Katasterpunkte") != Some("Inaktiv");
        self.neupunkte_sichtbar = projekt.einstellungen.neupunkte_visible;
        self.residuen_sichtbar = projekt.einstellungen.residual_visible;

        self.baue_overlay(projekt);
        self.baue_neupunkte_overlay(projekt);
        self.baue_import_overlay(projekt);
        if let Some(erg) = projekt.station.ergebnis.clone().filter(|e| !e.residuen.is_empty()) {
            self.baue_residual_overlay(projekt, &erg);
        }

        if projekt.einstellungen.hat_dxf_datei() {
            if let Some(pfad) = projekt.einstellungen.dxf_pfad() {
                self.lade_dxf(projekt, &pfad);
            }
        }

        self.neupunkt_modus = projekt.hat_gueltige_station() && projekt.einstellungen.letzter_modus_neupunkt;
        self.aktualisiere_lokales_system(projekt);
        self.pruefe_aufstellung_beim_start(projekt);
        if !self.lokales_system
            && !projekt.einstellungen.hat_dxf_datei()
            && (!self.overlay.is_empty() || !self.import_layer.is_empty() || !self.neupunkte.is_empty())
        {
            self.fit_ausstehend = true;
        }
    }

    // ── Overlays ──────────────────────────────────────────────────────────────

    fn baue_overlay(&mut self, projekt: &Projekt) {
        self.overlay.clear();
        for p in projekt.feldbuchpunkte.punkte() {
            let art = if p.typ == TYP_STANDPUNKT { Art::Standpunkt(p.clone()) } else { Art::Neupunkt(p.clone()) };
            self.overlay.push(DxfEntity::overlay(&p.typ, art));
        }
        if self.lokales_system {
            self.overlay.push(DxfEntity::overlay(
                "Standpunkt",
                Art::LokalerStandpunkt { name: projekt.station.standpunkt_nr.clone() },
            ));
        }
    }

    fn baue_neupunkte_overlay(&mut self, projekt: &Projekt) {
        self.neupunkte = projekt
            .neupunkte
            .koordinaten()
            .iter()
            .filter(|e| e.r != 0.0 || e.h != 0.0)
            .map(|e| DxfEntity::overlay("Neupunkt", Art::Gemessen(e.clone())))
            .collect();
    }

    fn baue_import_overlay(&mut self, projekt: &Projekt) {
        let punkte = projekt.importpunkte.punkte();
        if punkte.is_empty() {
            return;
        }
        let entities: Vec<DxfEntity> = punkte
            .iter()
            .map(|p| DxfEntity::overlay("Import", Art::Import { nr: p.punkt_nr.clone(), wx: p.r, wy: p.h, hoehe: p.hoehe }))
            .collect();
        match self.import_layer.iter_mut().find(|l| l.name.eq_ignore_ascii_case(IMPORT_LAYER_NAME)) {
            Some(layer) => layer.entities = entities,
            None => self.import_layer.push(ImportLayer { name: IMPORT_LAYER_NAME.to_string(), entities, sichtbar: true }),
        }
    }

    fn baue_residual_overlay(&mut self, projekt: &Projekt, ergebnis: &StationierungsErgebnis) {
        self.residuen.clear();
        self.ellipsen.clear();
        for res in &ergebnis.residuen {
            let Some((r, h, _)) = projekt.finde_anschlusspunkt(&res.punkt_nr) else {
                continue;
            };
            if r == 0.0 && h == 0.0 {
                continue;
            }
            self.residuen.push(DxfEntity::overlay("Residual", Art::Residual { res: res.clone(), wx: r, wy: h }));
            let phi = (h - ergebnis.h).atan2(r - ergebnis.r);
            self.ellipsen.push(DxfEntity::overlay(
                "ResiduenEllipse",
                Art::Ellipse {
                    nr: res.punkt_nr.clone(),
                    r,
                    h,
                    v_strecke_mm: res.v_strecke_mm,
                    v_quer_mm: res.v_quer_mm,
                    phi,
                    skalierung: self.ellipsen_skalierung,
                    konfidenz: false,
                },
            ));
        }
        // Konfidenzellipse des Standpunkts aus der Kovarianzmatrix der Lageausgleichung
        if let Some(k) = &ergebnis.konfidenz {
            self.ellipsen.push(DxfEntity::overlay(
                "KonfidenzEllipse",
                Art::Ellipse {
                    nr: format!("{} ({} %)", projekt.station.standpunkt_nr, f(k.niveau * KONFIDENZ_PROZENT, 0)),
                    r: ergebnis.r,
                    h: ergebnis.h,
                    v_strecke_mm: k.a_mm,
                    v_quer_mm: k.b_mm,
                    phi: k.theta_rad,
                    skalierung: self.ellipsen_skalierung,
                    konfidenz: true,
                },
            ));
        }
        if self.residuen_sichtbar && !self.ellipsen.is_empty() {
            self.auto_skaliere_ellipsen();
        }
    }

    /// Auto-Skalierung: mittlere Ellipse ≈ 1 cm auf dem Bildschirm.
    fn auto_skaliere_ellipsen(&mut self) {
        if self.ellipsen.is_empty() || self.scale <= 0.0 {
            return;
        }
        let werte: Vec<f64> = self
            .ellipsen
            .iter()
            .filter_map(|e| match &e.art {
                Art::Ellipse { v_strecke_mm, v_quer_mm, konfidenz: false, .. } => Some(v_strecke_mm.hypot(*v_quer_mm)),
                _ => None,
            })
            .filter(|v| *v > 0.0)
            .collect();
        let mittel = if werte.is_empty() { 1.0 } else { werte.iter().sum::<f64>() / werte.len() as f64 };
        let ziel_px = DPI / 2.54;
        let neu = (ziel_px / (mittel * 0.001 * self.scale)).clamp(1e-4, 1e9);
        self.setze_ellipsen_skalierung(neu);
    }

    fn setze_ellipsen_skalierung(&mut self, neu: f64) {
        self.ellipsen_skalierung = neu;
        for e in &mut self.ellipsen {
            if let Art::Ellipse { skalierung, .. } = &mut e.art {
                *skalierung = neu;
            }
        }
    }

    // ── Lokales System / Signallampe ──────────────────────────────────────────

    fn ist_lokales_system_aktiv(&self, projekt: &Projekt) -> bool {
        self.entities.is_empty() && self.import_layer.is_empty() && !projekt.hat_gueltige_station()
    }

    fn aktualisiere_lokales_system(&mut self, projekt: &Projekt) {
        let neu = self.ist_lokales_system_aktiv(projekt);
        let wechsel = neu != self.lokales_system;
        self.lokales_system = neu;
        if neu {
            if wechsel {
                self.neupunkt_modus = true;
                self.aktualisiere_modus();
            }
            self.baue_overlay(projekt);
            if wechsel {
                self.lokal_zeigen_ausstehend = true;
            }
        } else if wechsel {
            self.baue_overlay(projekt);
        }
    }

    /// Zentriert die Ansicht auf den Standpunkt und passt sie an alle Neupunkte an.
    fn zeige_lokales_system(&mut self) {
        let mut radius = LOKAL_MIN_RADIUS_M;
        for e in &self.neupunkte {
            if let Art::Gemessen(n) = &e.art {
                radius = radius.max(n.r.abs().max(n.h.abs()) * LOKAL_RAND_FAKTOR);
            }
        }
        let (b, h) = (f64::from(self.canvas_groesse.x), f64::from(self.canvas_groesse.y));
        self.mitte = [0.0, 0.0];
        self.scale = (b.min(h) / (2.0 * radius)).min(self.max_scale);
    }

    /// Farbe und Tooltip der Signallampe aus dem aktuellen Stationierungsergebnis.
    fn signal(&self, projekt: &Projekt) -> (Option<Color32>, String, String) {
        let Some(erg) = projekt.station.ergebnis.as_ref().filter(|_| projekt.hat_gueltige_station()) else {
            return (None, "Keine gültige Stationierung".to_string(), "–".to_string());
        };
        let p = &projekt.rechenparameter.params;
        let s0 = erg.s0_mm;
        let info = p.sigma_text(s0, 1);
        if s0 > S0_SCHWACH_MM || erg.redundanz <= 0 {
            (Some(Color32::from_rgb(220, 30, 30)), format!("Schwache Stationierung: {info}  r={}", erg.redundanz), info)
        } else if s0 > S0_BEDINGT_MM {
            (Some(Color32::from_rgb(230, 160, 0)), format!("Bedingt brauchbar: {info}  r={}", erg.redundanz), info)
        } else {
            (
                Some(Color32::from_rgb(30, 190, 50)),
                format!("Gute Stationierung: {}  r={}", p.sigma_text(s0, 2), erg.redundanz),
                info,
            )
        }
    }

    fn aktualisiere_modus(&mut self) {
        self.getrennt_messung = None;
        if self.neupunkt_modus && (self.txt_punkt_nr.trim().is_empty() || self.txt_punkt_nr.trim().parse::<i32>().is_ok()) {
            self.txt_punkt_nr = self.neupunkt_zaehler.to_string();
        }
    }

    // ── Laden ─────────────────────────────────────────────────────────────────

    fn punkt_index_pfad(projekt: &Projekt) -> PathBuf {
        let name = if projekt.einstellungen.projekt_name.is_empty() {
            "OhneProjekt"
        } else {
            projekt.einstellungen.projekt_name.as_str()
        };
        projekt.einstellungen.get_pfad(&format!("{name}-PunktIndex.json"))
    }

    fn baue_marker(&mut self) {
        self.marker.clear();
        let Some(index) = &self.punkt_index else {
            return;
        };
        for e in &self.entities {
            for (r, h) in punktindex::entity_punkte(e) {
                if let Some(nr) = index.get_punkt_nr(r, h) {
                    self.marker.push(DxfEntity::overlay("DXF-Punkte", Art::Marker { nr: nr.to_string(), wx: r, wy: h }));
                }
            }
        }
    }

    fn lade_dxf(&mut self, projekt: &mut Projekt, pfad: &Path) {
        let entities = match leser::lese(pfad) {
            Ok(e) => e,
            Err(fehler) => {
                self.dialog = Some(Dialog::Hinweis { titel: "Fehler".into(), text: format!("Fehler beim Lesen der DXF-Datei:\n{fehler}") });
                return;
            }
        };
        self.entities = entities;
        *self.zeichen_cache.borrow_mut() = ZeichenCache::default();
        self.auswahl = None;

        let index_pfad = Self::punkt_index_pfad(projekt);
        let bestehend = punktindex::lade(&index_pfad);
        let ist_neu_ladung = bestehend.is_some();
        let index = punktindex::PunktIndex::aufbauen_mit_merge(&self.entities, bestehend.as_deref());
        if let Err(e) = punktindex::speichere(&index.eintraege, &index_pfad) {
            self.dialog = Some(Dialog::Hinweis { titel: "Fehler".into(), text: format!("Punkt-Index konnte nicht gespeichert werden:\n{e}") });
        }
        let anzahl = index.anzahl();
        let alt = bestehend.as_ref().map_or(0, Vec::len);
        self.punkt_index = Some(index);
        self.baue_marker();

        let pfad_text = pfad.to_string_lossy().into_owned();
        if let Some((scale, px, py)) = projekt.einstellungen.lade_zoom(&pfad_text) {
            self.scale = scale.min(self.max_scale);
            self.mitte = [px, py];
        } else {
            self.fit_ausstehend = true;
        }
        self.dxf_pfad = pfad_text;
        let name = pfad.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let neu = anzahl.saturating_sub(alt);
        self.status = format!(
            "Geladen: {name}  |  {} Objekte, {anzahl} Punkte{}",
            self.entities.len(),
            if ist_neu_ladung && neu > 0 {
                format!("  ({neu} neu, {} übernommen)", anzahl - neu)
            } else if ist_neu_ladung {
                "  (Nummern übernommen)".to_string()
            } else {
                String::new()
            }
        );
    }

    fn punkt_index_zuruecksetzen(&mut self, projekt: &Projekt) {
        let pfad = Self::punkt_index_pfad(projekt);
        punktindex::loesche(&pfad);
        if !self.dxf_pfad.is_empty() && !self.entities.is_empty() {
            let index = punktindex::PunktIndex::aufbauen(&self.entities);
            let _ = punktindex::speichere(&index.eintraege, &pfad);
            self.status = format!("Punkt-Index zurückgesetzt: {} Punkte neu nummeriert.", index.anzahl());
            self.punkt_index = Some(index);
            self.baue_marker();
        } else {
            self.status = "Keine DXF-Datei geladen – Punkt-Index gelöscht.".to_string();
        }
    }

    // ── Ansicht ───────────────────────────────────────────────────────────────

    fn ansicht(&self, ursprung: Pos2) -> Ansicht {
        Ansicht { mitte: self.mitte, scale: self.scale, ursprung }
    }

    fn alle_sichtbaren_fuer_fit(&self) -> Vec<&DxfEntity> {
        let mut liste: Vec<&DxfEntity> = Vec::new();
        if self.dxf_sichtbar {
            liste.extend(self.entities.iter());
        }
        liste.extend(self.overlay.iter());
        liste.extend(self.import_layer.iter().filter(|l| l.sichtbar).flat_map(|l| l.entities.iter()));
        liste.extend(self.neupunkte.iter());
        liste
    }

    fn fit_to_view(&mut self) {
        // Auswahlmodus: auf die wählbaren Punkte einpassen (ohne Punkte auf die Zeichnung)
        if self.ist_auswahl() && self.auswahl_einpassen(None) {
            return;
        }
        let liste = self.alle_sichtbaren_fuer_fit();
        let Some(bb) = liste.iter().map(|e| e.bounds()).reduce(|a, b| a.erweitert(&b)) else {
            return;
        };
        if bb.breite() < 1e-10 && bb.hoehe() < 1e-10 {
            return;
        }
        let (sb, sh) = (f64::from(self.canvas_groesse.x).max(1.0), f64::from(self.canvas_groesse.y).max(1.0));
        self.scale = (sb / bb.breite().max(1e-6)).min(sh / bb.hoehe().max(1e-6)) * FIT_RAND;
        self.mitte = [bb.mitte_x(), bb.mitte_y()];
    }

    /// Sofortiger Zoom um einen Bildschirmpunkt (Knöpfe, Zwei-Finger-Geste).
    fn zoom(&mut self, faktor: f64, bildschirm: Pos2, ursprung: Pos2) {
        self.zoom_ziel = None;
        self.zoom_sofort(faktor, bildschirm, ursprung);
    }

    /// Mausrad: setzt nur ein Ziel, der Maßstab gleitet in wenigen Bildern dorthin.
    fn zoom_animiert(&mut self, faktor: f64, bildschirm: Pos2, ursprung: Pos2) {
        let max = if self.max_scale > 0.0 { self.max_scale } else { KEIN_MAX_MASSSTAB };
        let basis = self.zoom_ziel.map_or(self.scale, |z| z.scale);
        let ziel = (basis * faktor).clamp(MIN_MASSSTAB, max);
        self.zoom_ziel = Some(ZoomZiel { scale: ziel, anker: bildschirm, ursprung, erwartet: self.scale });
    }

    /// Ein Schritt der weichen Zoom-Animation; wird in jedem Bild vor dem Zeichnen aufgerufen.
    fn treibe_zoom_animation(&mut self, ctx: &egui::Context) {
        let Some(ziel) = self.zoom_ziel else {
            return;
        };
        if ((self.scale - ziel.erwartet) / ziel.erwartet).abs() > ZOOM_FREMD_TOLERANZ {
            self.zoom_ziel = None;
            return;
        }
        let dt = f64::from(ctx.input(|i| i.unstable_dt).clamp(ZOOM_DT_MIN_S, ZOOM_DT_MAX_S));
        let anteil = 1.0 - (-dt / ZOOM_ZEITKONSTANTE_S).exp();
        let rest = ziel.scale / self.scale;
        if rest.ln().abs() < ZOOM_ENDE_LOG {
            self.zoom_sofort(rest, ziel.anker, ziel.ursprung);
            self.zoom_ziel = None;
        } else {
            self.zoom_sofort(rest.powf(anteil), ziel.anker, ziel.ursprung);
            self.zoom_ziel = Some(ZoomZiel { erwartet: self.scale, ..ziel });
            ctx.request_repaint();
        }
    }

    /// Prüft den Zwischenspeicher der DXF-Zeichnung und baut ihn bei Bedarf neu auf.
    fn pruefe_zeichen_cache(&self) {
        let schluessel = (self.entities.as_ptr() as usize, self.entities.len());
        let mut cache = self.zeichen_cache.borrow_mut();
        if cache.schluessel == schluessel && cache.bounds.len() == self.entities.len() {
            return;
        }
        cache.schluessel = schluessel;
        cache.bounds = self.entities.iter().map(DxfEntity::bounds).collect();
        cache.farben = self.entities.iter().map(|e| layer_farbe(&e.layer).or(e.farbe).unwrap_or(STANDARD_LINIE)).collect();
        cache.linien = (0..self.entities.len()).filter(|&i| !self.entities[i].ist_insert()).collect();
        cache.inserts = (0..self.entities.len()).filter(|&i| self.entities[i].ist_insert()).collect();
    }

    fn zoom_sofort(&mut self, faktor: f64, bildschirm: Pos2, ursprung: Pos2) {
        let anker = self.ansicht(ursprung).zu_welt(bildschirm);
        let max = if self.max_scale > 0.0 { self.max_scale } else { KEIN_MAX_MASSSTAB };
        self.scale = (self.scale * faktor).clamp(MIN_MASSSTAB, max);
        self.mitte = [
            anker.0 - f64::from(bildschirm.x - ursprung.x) / self.scale,
            anker.1 + f64::from(bildschirm.y - ursprung.y) / self.scale,
        ];
    }

    /// Maßstab als Text, z. B. "1:500".
    #[allow(dead_code)]
    pub fn massstab_text(&self) -> String {
        let pixel_m = M_PRO_ZOLL / DPI;
        let m = 1.0 / (self.scale * pixel_m);
        if m >= 1.0 { format!("1:{}", schoen_runden(m)) } else { format!("{}:1", de(1.0 / m, 1)) }
    }

    // ── Picking ───────────────────────────────────────────────────────────────

    fn entity(&self, a: Auswahl) -> Option<&DxfEntity> {
        match a {
            Auswahl::Zeichnung(i) => self.entities.get(i),
            Auswahl::Overlay(i) => self.overlay.get(i),
            Auswahl::Neupunkt(i) => self.neupunkte.get(i),
            Auswahl::Import(l, i) => self.import_layer.get(l).and_then(|l| l.entities.get(i)),
            Auswahl::Marker(i) => self.marker.get(i),
        }
    }

    /// Nächster Treffer zum Klickpunkt (audit.md D2): alle Kandidaten werden nach Bildschirmabstand verglichen,
    /// unabhängig von der Reihenfolge in der Datei. Marker haben einen größeren Fangradius, gewinnen aber nur,
    /// wenn sie tatsächlich am nächsten liegen.
    fn pick(&self, welt: (f64, f64)) -> Option<Auswahl> {
        let (wx, wy) = welt;
        // (Abstand [px], Treffer, Fangradius [px])
        let mut kandidaten: Vec<(f64, Auswahl, f64)> = Vec::new();
        for (i, e) in self.overlay.iter().enumerate() {
            kandidaten.push((e.distance_to(wx, wy), Auswahl::Overlay(i), PICK_RADIUS_PX));
        }
        if self.neupunkte_sichtbar {
            for (i, e) in self.neupunkte.iter().enumerate() {
                kandidaten.push((e.distance_to(wx, wy), Auswahl::Neupunkt(i), PICK_RADIUS_PX));
            }
        }
        for (l, layer) in self.import_layer.iter().enumerate().filter(|(_, l)| l.sichtbar) {
            for (i, e) in layer.entities.iter().enumerate() {
                kandidaten.push((e.distance_to(wx, wy), Auswahl::Import(l, i), PICK_RADIUS_PX));
            }
        }
        if self.dxf_sichtbar {
            for (i, e) in self.entities.iter().enumerate() {
                if e.ist_insert() && !self.punkte_sichtbar {
                    continue;
                }
                kandidaten.push((e.distance_to(wx, wy), Auswahl::Zeichnung(i), PICK_RADIUS_PX));
            }
        }
        if self.marker_sichtbar {
            for (i, m) in self.marker.iter().enumerate() {
                kandidaten.push((m.distance_to(wx, wy), Auswahl::Marker(i), MARKER_PICK_PX));
            }
        }
        naechster_treffer(kandidaten.into_iter().map(|(d, a, r)| (d * self.scale, a, r)))
    }

    fn ist_zeichenbar(&self, e: &DxfEntity, ursprung: Pos2, groesse: Vec2) -> bool {
        let b = e.bounds();
        let a = self.ansicht(ursprung);
        let p1 = a.zu_bildschirm(b.min_x, b.min_y);
        let p2 = a.zu_bildschirm(b.max_x, b.max_y);
        if ![p1.x, p1.y, p2.x, p2.y].iter().all(|v| v.is_finite()) {
            return false;
        }
        let (x_min, x_max) = (p1.x.min(p2.x), p1.x.max(p2.x));
        let (y_min, y_max) = (p1.y.min(p2.y), p1.y.max(p2.y));
        x_max > -ZEICHEN_GRENZE
            && x_min < groesse.x + ZEICHEN_GRENZE
            && y_max > -ZEICHEN_GRENZE
            && y_min < groesse.y + ZEICHEN_GRENZE
    }

    // ── Hauptfenster des Viewers ──────────────────────────────────────────────

    /// Zeigt den Viewer als eigenes Fenster. Der Rückgabewert ist `false`, wenn er geschlossen wurde.
    pub fn zeige(&mut self, ctx: &egui::Context, projekt: &mut Projekt, geraet: &mut Geraet) -> bool {
        if !self.initialisiert {
            self.initialisiere(projekt);
        }
        // Der Auswahlmodus ist eine eigene Instanz in eigenem Fenster
        let id = egui::ViewportId::from_hash_of(if self.ist_auswahl() { "dxf_viewer_auswahl" } else { "dxf_viewer" });
        let builder = egui::ViewportBuilder::default()
            .with_title(self.fenstertitel())
            .with_inner_size(FENSTER_GROESSE)
            .with_min_inner_size([760.0, 420.0]);
        ctx.show_viewport_immediate(id, builder, |ui, _klasse| {
            if !self.startgroesse_gesetzt {
                self.startgroesse_gesetzt = super::setze_startgroesse_fenster(ui.ctx());
            }
            if !self.ist_auswahl() {
                // Rückkehr zum Fenster: Stationierung kann sich geändert haben (OnFormActivated)
                let fokus = ui.ctx().input(|i| i.viewport().focused.unwrap_or(true));
                if fokus && !self.war_fokussiert {
                    self.bei_aktivierung(projekt);
                }
                self.war_fokussiert = fokus;
                self.bediene_geraet(projekt, geraet);
            }
            self.zeichne(ui, projekt, geraet);
            if !self.ist_auswahl() {
                self.zeige_absteckfenster(&ui.ctx().clone(), projekt, geraet);
            }
            let ruhe = !(geraet.ist_verbunden() || geraet.verbindet() || geraet.beschaeftigt());
            ui.ctx().request_repaint_after(if ruhe { REPAINT_INTERVALL_RUHE } else { REPAINT_INTERVALL });

            if ui.ctx().input(|i| i.viewport().close_requested()) {
                if self.ist_auswahl() {
                    self.auswahl_schliessen_angefragt(ui.ctx());
                } else {
                    self.beim_schliessen(projekt);
                    self.offen = false;
                }
            }
            if self.auswahl_entschieden() {
                self.offen = false;
            }
        });
        self.offen
    }

    fn fenstertitel(&self) -> String {
        if self.ist_auswahl() {
            "DXF-Viewer – Auswahl für die Konvertierung".to_string()
        } else if self.dxf_pfad.is_empty() {
            "DXF-Viewer".to_string()
        } else {
            let name = Path::new(&self.dxf_pfad).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            format!("DXF-Viewer – {name}")
        }
    }

    fn bei_aktivierung(&mut self, projekt: &mut Projekt) {
        projekt.lade_aktuellen_standpunkt();
        self.nach_standpunktwechsel(projekt);
    }

    /// Baut Residuen und Ellipsen aus der aktuellen Stationierung neu auf (oder leert sie, wenn es keine gibt).
    /// Ein vom Benutzer eingestellter Ellipsenmaßstab bleibt erhalten.
    fn baue_qualitaets_overlay(&mut self, projekt: &Projekt) {
        let skalierung = (!self.ellipsen.is_empty()).then_some(self.ellipsen_skalierung);
        match projekt.station.ergebnis.clone().filter(|e| !e.residuen.is_empty()) {
            Some(erg) => self.baue_residual_overlay(projekt, &erg),
            None => {
                self.residuen.clear();
                self.ellipsen.clear();
            }
        }
        if let Some(skalierung) = skalierung {
            self.setze_ellipsen_skalierung(skalierung);
        }
    }

    fn beim_schliessen(&mut self, projekt: &mut Projekt) {
        // Absteckung: Protokoll der abgesteckten Punkte schreiben (offene Messung nur innerhalb der Toleranz)
        if let Some(mut a) = self.absteckung.take() {
            a.beim_schliessen_des_viewers(projekt);
        }
        if let Err(e)=projekt.rechenparameter.speichern() {self.status=format!("PDF-Vorgaben konnten nicht gespeichert werden: {e}");}
        projekt.einstellungen.letzter_modus_neupunkt = self.neupunkt_modus;
        let _ = projekt.einstellungen.speichern();
        if !self.dxf_pfad.is_empty() {
            let _ = projekt.einstellungen.speichere_zoom(&self.dxf_pfad.clone(), self.scale, self.mitte[0], self.mitte[1]);
        }
    }

    /// Prüft, ob die A-Taste (Strg+Alt+E = AltGr+E) gedrückt wurde, und entfernt deren Ereignisse,
    /// damit das erzeugte „€" nicht in ein Eingabefeld getippt wird.
    fn nimm_mess_taste(ctx: &egui::Context) -> bool {
        ctx.input_mut(|i| {
            let mut gedrueckt = false;
            i.events.retain(|e| match e {
                egui::Event::Text(t) if t == MESS_TASTE_ZEICHEN => {
                    gedrueckt = true;
                    false
                }
                egui::Event::Key { key: MESS_TASTE, modifiers, pressed, .. } if modifiers.ctrl && modifiers.alt => {
                    gedrueckt |= *pressed;
                    false
                }
                _ => true,
            });
            gedrueckt
        })
    }

    fn zeichne(&mut self, ui: &mut egui::Ui, projekt: &mut Projekt, geraet: &mut Geraet) {
        self.mess_taste = Self::nimm_mess_taste(ui.ctx());
        // Fensterhintergrund
        let voll = ui.max_rect();
        ui.painter().rect_filled(voll, 0.0, FENSTER_FARBE);

        // Im Auswahlmodus sind Messen, Berechnen und Import gesperrt (ausgegraut)
        let frei = !self.ist_auswahl();
        egui::Panel::top("dxf_toolbar")
            .exact_size(TOOLBAR_HOEHE)
            .frame(egui::Frame::new().fill(TOOLBAR_FARBE))
            .show(ui, |ui| {
                ui.add_enabled_ui(frei, |ui| self.zeichne_toolbar(ui, projekt, geraet));
            });
        if !frei {
            egui::Panel::top("dxf_auswahl_leiste")
                .frame(egui::Frame::new().fill(auswahl::LEISTE_FARBE).inner_margin(egui::Margin::symmetric(8, 6)))
                .show(ui, |ui| self.zeichne_auswahl_leiste(ui));
        }

        egui::Panel::bottom("dxf_status")
            .exact_size(STATUS_HOEHE)
            .frame(egui::Frame::new().fill(STATUS_FARBE))
            .show(ui, |ui| if frei { self.zeichne_statusleiste(ui, projekt) } else { self.zeichne_auswahl_status(ui) });

        egui::Panel::right("dxf_seite")
            .exact_size(SEITE_BREITE)
            .frame(egui::Frame::new().fill(SEITE_FARBE))
            .show(ui, |ui| self.zeichne_seitenleiste(ui, projekt));

        if self.pdf_mitte.is_some() {
            egui::Panel::top("dxf_pdf").show(ui, |ui| self.pdf_leiste(ui, projekt));
        }

        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(HINTERGRUND))
            .show(ui, |ui| self.zeichne_canvas(ui, projekt));

        // A-Taste des Toughpad (als MESS_TASTE konfiguriert) löst die Messung aus, solange kein Dialog offen ist
        let absteck_ziel = self.absteckung.as_ref().is_some_and(|a| a.ziel.is_some());
        if self.dialog.is_none() && absteck_ziel && self.mess_taste {
            self.absteck_messen(projekt, geraet);
        } else if self.dialog.is_none() && self.neupunkt_modus && self.mess_taste {
            if self.getrennt_messung.is_some() {
                self.uebernimm_getrennte_messung(projekt, geraet);
            } else {
                self.messen_geklickt(projekt, geraet);
            }
        }

        self.zeichne_dialog(ui.ctx().clone(), projekt, geraet);
        self.zeichne_auswahl_frage(&ui.ctx().clone());
    }

    // ── Toolbar ───────────────────────────────────────────────────────────────

    fn zeichne_toolbar(&mut self, ui: &mut egui::Ui, projekt: &mut Projekt, geraet: &mut Geraet) {
        ui.spacing_mut().item_spacing = Vec2::new(4.0, 0.0);
        let reflektorlos = geraet.reflektorlos;
        let laser_an = geraet.laser_an;

        // Rechts nach links: Code, Nr, ZH, [eQ, eL], Messen, Trenner, SP, IH, Modus, Lampe, Trenner
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.add_space(8.0);
            self.toolbar_feld(ui, "Code", 40.0, 90.0, FeldArt::Code);
            ui.add_space(8.0);
            self.toolbar_feld(ui, "Nr", 22.0, 80.0, FeldArt::PunktNr);
            ui.add_space(8.0);
            self.toolbar_feld(ui, "ZH", 24.0, 58.0, FeldArt::Zielhoehe);
            if self.neupunkt_modus {
                ui.add_space(8.0);
                self.toolbar_feld(ui, "eQ", 24.0, 54.0, FeldArt::ExzQuer);
                ui.add_space(8.0);
                self.toolbar_feld(ui, "eL", 24.0, 54.0, FeldArt::ExzLaengs);
            }
            ui.add_space(6.0);
            if self.neupunkt_modus {
                let uebernehmen = self.getrennt_messung.is_some();
                let (text, tip) = if uebernehmen {
                    ("Übernehmen", "Strecke ist gemessen: Ziel anpeilen, dann Hz/V vom Gerät übernehmen")
                } else {
                    ("► Messen", "Messung mit Tachymeter auslösen")
                };
                let messen = egui::Button::new(egui::RichText::new(text).strong().size(12.0).color(Color32::WHITE))
                    .fill(Color32::from_rgb(160, 60, 20))
                    .stroke(Stroke::new(1.0, Color32::from_rgb(130, 40, 10)))
                    .min_size(Vec2::new(MESSEN_KNOPF_BREITE, KNOPF_GROESSE));
                if ui.add(messen).on_hover_text(tip).clicked() {
                    if uebernehmen {
                        self.uebernimm_getrennte_messung(projekt, geraet);
                    } else {
                        self.messen_geklickt(projekt, geraet);
                    }
                }
                ui.add_space(4.0);
                self.zeichne_getrennt_knopf(ui);
            } else {
                // Stationierung: Messen ist nicht möglich, der identische Punkt wird im Plan gewählt
                ui.add_sized(
                    [MESSEN_KNOPF_BREITE + 40.0, KNOPF_GROESSE],
                    egui::Label::new(egui::RichText::new("Identischen Punkt
auswählen").strong().size(11.0).color(Color32::from_rgb(255, 190, 100))),
                );
            }
            ui.add_space(6.0);
            trenner(ui);
            ui.add_space(4.0);
            // Instrumentenhöhe der aktiven Aufstellung (Korrektur mit Rückfrage); Standpunkt und Qualität stehen im Infofeld
            self.toolbar_feld_mit_projekt(ui, projekt, "IH", 22.0, 58.0);
            ui.add_space(6.0);
            trenner(ui);

            // Links (feste Positionen): Neuer Stp, Kontrolle, Prisma, EDM, Laser, PDF, DXF, Modus
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                ui.add_space(4.0);
                let neuer_stp = egui::Button::new(RichText::new("Neuer\nStp").strong().size(11.0).color(Color32::WHITE))
                    .fill(standpunkt::NEUER_STP_FARBE)
                    .stroke(Stroke::new(1.0, standpunkt::NEUER_STP_RAND))
                    .min_size(Vec2::new(NEUER_STP_BREITE, KNOPF_GROESSE));
                if ui.add(neuer_stp).on_hover_text("Neuen Standpunkt anlegen, vorhandenen fortsetzen oder umbenennen").clicked() {
                    self.oeffne_standpunkt_dialog(projekt, geraet);
                }
                let kontrolle_moeglich = projekt.hat_gueltige_station() && !self.lokales_system;
                let kontrolle = egui::Button::new(RichText::new("Kon-\ntrolle").strong().size(11.0).color(Color32::WHITE))
                    .fill(TOOLBAR_KNOPF)
                    .stroke(Stroke::new(1.0, TOOLBAR_KNOPF_RAND))
                    .min_size(Vec2::new(NEUER_STP_BREITE, KNOPF_GROESSE));
                if ui
                    .add_enabled(kontrolle_moeglich, kontrolle)
                    .on_hover_text("Orientierungskontrolle: einen Anschlusspunkt anzielen und messen")
                    .on_disabled_hover_text("Erst nach einer gültigen Stationierung möglich")
                    .clicked()
                {
                    self.oeffne_kontrolle(projekt);
                }
                let abstecken_moeglich = projekt.hat_aktive_aufstellung() && (projekt.hat_gueltige_station() || self.lokales_system);
                let abstecken_aktiv = self.absteckung.is_some();
                let (fill, rand) = if abstecken_aktiv { (Color32::from_rgb(235, 120, 0), Color32::from_rgb(255, 170, 60)) } else { (TOOLBAR_KNOPF, TOOLBAR_KNOPF_RAND) };
                let abstecken = egui::Button::new(RichText::new("Ab-\nstecken").strong().size(11.0).color(Color32::WHITE))
                    .fill(fill)
                    .stroke(Stroke::new(1.0, rand))
                    .min_size(Vec2::new(NEUER_STP_BREITE, KNOPF_GROESSE));
                if ui
                    .add_enabled(abstecken_moeglich || abstecken_aktiv, abstecken)
                    .on_hover_text(if abstecken_aktiv { "Absteckung beenden (Protokoll wird geschrieben)" } else { "Punktabsteckung: Zielpunkt im Plan antippen" })
                    .on_disabled_hover_text("Erst nach einer gültigen Stationierung möglich")
                    .clicked()
                {
                    self.absteckung_umschalten(projekt);
                }
                ui.add_space(6.0);
                let prisma = egui::Button::image(egui::Image::new(egui::include_image!("../../icons/toolbar_prisma.png")).fit_to_exact_size(Vec2::splat(24.0)))
                    .fill(TOOLBAR_KNOPF)
                    .stroke(Stroke::new(1.0, TOOLBAR_KNOPF_RAND))
                    .min_size(Vec2::splat(KNOPF_GROESSE));
                if ui.add(prisma).on_hover_text("Prismenkonstante").clicked() {
                    self.oeffne_prisma_dialog(projekt);
                }
                let (edm_fill, edm_rand, edm_text, edm_tip) = if reflektorlos {
                    (Color32::from_rgb(150, 70, 20), Color32::from_rgb(200, 100, 40), "RL", "EDM: Reflektorlos – Klick → Prisma")
                } else {
                    (TOOLBAR_KNOPF, TOOLBAR_KNOPF_RAND, "P", "EDM: Prisma – Klick → Reflektorlos")
                };
                let edm = egui::Button::new(egui::RichText::new(edm_text).strong().color(Color32::WHITE))
                    .fill(edm_fill)
                    .stroke(Stroke::new(1.0, edm_rand))
                    .min_size(Vec2::splat(KNOPF_GROESSE));
                if ui.add(edm).on_hover_text(edm_tip).clicked() {
                    if let Err(fehler) = geraet.starte_ziel_umschaltung() {
                        self.status = fehler;
                    }
                }
                let (laser_fill, laser_rand, laser_tip) = if laser_an {
                    (Color32::from_rgb(180, 55, 15), Color32::from_rgb(240, 90, 30), "Laserpointer AN – Klick zum Ausschalten")
                } else {
                    (TOOLBAR_KNOPF, TOOLBAR_KNOPF_RAND, "Laserpointer AUS – Klick zum Einschalten")
                };
                let laser = egui::Button::new(egui::RichText::new("●").color(Color32::WHITE))
                    .fill(laser_fill)
                    .stroke(Stroke::new(1.0, laser_rand))
                    .min_size(Vec2::splat(KNOPF_GROESSE));
                if ui.add(laser).on_hover_text(laser_tip).clicked() {
                    if let Err(fehler) = geraet.starte_laser_umschaltung() {
                        self.status = fehler;
                    }
                }
                let pdf_aktiv = self.pdf_mitte.is_some();
                let (pdf_fill, pdf_rand, pdf_tip) = if pdf_aktiv {
                    (Color32::from_rgb(180, 85, 20), Color32::from_rgb(240, 140, 50), "PDF-Ausschnitt AN – Klick zum Ausschalten")
                } else {
                    (TOOLBAR_KNOPF, TOOLBAR_KNOPF_RAND, "PDF-Ausschnitt AUS – Klick zum Einschalten")
                };
                let pdf = egui::Button::new(RichText::new("PDF").strong().size(11.0).color(Color32::WHITE))
                    .fill(pdf_fill)
                    .stroke(Stroke::new(1.0, pdf_rand))
                    .min_size(Vec2::splat(KNOPF_GROESSE));
                if ui.add(pdf).on_hover_text(pdf_tip).clicked() {
                    self.pdf_mitte = if pdf_aktiv { None } else { Some(self.mitte) };
                    self.pdf_ziehen = false;
                }
                let dxf_export = egui::Button::new(RichText::new("DXF").strong().size(11.0).color(Color32::WHITE))
                    .fill(TOOLBAR_KNOPF)
                    .stroke(Stroke::new(1.0, TOOLBAR_KNOPF_RAND))
                    .min_size(Vec2::splat(KNOPF_GROESSE));
                if ui.add(dxf_export).on_hover_text("Als DXF exportieren").clicked() {
                    self.oeffne_export_dialog(projekt);
                }
                ui.add_space(MODUS_KNOPF_ABSTAND);
                self.zeichne_modus_knopf(ui);
            });
        });
    }

    /// Umschalter für die getrennte Messung (grün: Hz, V und Strecke zusammen; orange: Strecke getrennt von Hz/V).
    fn zeichne_getrennt_knopf(&mut self, ui: &mut egui::Ui) {
        let (text, fill, rand, tip) = if self.getrennt_modus {
            ("HzV/S", Color32::from_rgb(210, 110, 20), Color32::from_rgb(240, 150, 50), "Getrennte Messung AN: Strecke messen, dann Hz/V übernehmen (Klick zum Wechseln)")
        } else {
            ("HzVS", Color32::from_rgb(42, 130, 65), Color32::from_rgb(28, 100, 45), "Hz, V und Strecke werden zusammen gemessen (Klick zum Wechseln)")
        };
        let knopf = egui::Button::new(egui::RichText::new(text).strong().size(11.0).color(Color32::WHITE))
            .fill(fill)
            .stroke(Stroke::new(1.0, rand))
            .min_size(Vec2::new(GETRENNT_KNOPF_BREITE, KNOPF_GROESSE));
        if ui.add(knopf).on_hover_text(tip).clicked() {
            self.getrennt_modus = !self.getrennt_modus;
            self.getrennt_messung = None;
        }
    }

    fn zeichne_modus_knopf(&mut self, ui: &mut egui::Ui) {
        let (text, fill, rand, tip) = if self.neupunkt_modus {
            ("Modus
Neupunkte", Color32::from_rgb(20, 80, 200), Color32::from_rgb(10, 60, 160), "Messmodus: Neupunkt (Klick zum Wechseln)")
        } else {
            ("Modus
Stationierung", Color32::from_rgb(42, 130, 65), Color32::from_rgb(28, 100, 45), "Messmodus: Stationierung (Klick zum Wechseln)")
        };
        let knopf = egui::Button::new(egui::RichText::new(text).strong().size(11.0).color(Color32::WHITE))
            .fill(fill)
            .stroke(Stroke::new(1.0, rand))
            .min_size(Vec2::new(MODUS_KNOPF_BREITE, KNOPF_GROESSE));
        if ui.add(knopf).on_hover_text(tip).clicked() {
            self.neupunkt_modus = !self.neupunkt_modus;
            self.aktualisiere_modus();
        }
    }

    fn toolbar_feld(&mut self, ui: &mut egui::Ui, bezeichnung: &str, label_breite: f32, breite: f32, art: FeldArt) -> egui::Response {
        let grosse_exz = matches!(art, FeldArt::ExzLaengs | FeldArt::ExzQuer) && {
            let text = match art {
                FeldArt::ExzLaengs => &self.txt_exz_laengs,
                _ => &self.txt_exz_quer,
            };
            parse_zahl(text).unwrap_or(0.0).abs() > EXZ_INFO_GRENZE_M
        };
        let (fill, textfarbe) = if grosse_exz { (EXZ_GROSS_HINTERGRUND, Color32::BLACK) } else { (FELD_FARBE, Color32::WHITE) };
        let text = self.feld_text_mut(art);
        let antwort = ui
            .scope(|ui| {
                ui.visuals_mut().extreme_bg_color = fill;
                ui.add_sized(
                    [breite, 24.0],
                    egui::TextEdit::singleline(text).font(FontId::monospace(13.0)).text_color(textfarbe),
                )
            })
            .inner;
        ui.add_sized(
            [label_breite, 24.0],
            egui::Label::new(egui::RichText::new(bezeichnung).strong().size(11.0).color(LABEL_FARBE)),
        );
        // Auf dem Toughpad: der Code bekommt die volle Tastatur, alle anderen Felder den Ziffernblock
        if art == FeldArt::Code {
            bildschirmtastatur::text_feld(&antwort);
        }
        antwort
    }

    /// Text des Toolbar-Feldes.
    fn feld_text_mut(&mut self, art: FeldArt) -> &mut String {
        match art {
            FeldArt::Code => &mut self.txt_code,
            FeldArt::PunktNr => &mut self.txt_punkt_nr,
            FeldArt::Zielhoehe => &mut self.txt_zielhoehe,
            FeldArt::ExzLaengs => &mut self.txt_exz_laengs,
            FeldArt::ExzQuer => &mut self.txt_exz_quer,
        }
    }

    fn toolbar_feld_mit_projekt(&mut self, ui: &mut egui::Ui, projekt: &mut Projekt, bezeichnung: &str, label_breite: f32, breite: f32) {
        let antwort = ui
            .scope(|ui| {
                ui.visuals_mut().extreme_bg_color = FELD_FARBE;
                ui.add_sized(
                    [breite, 24.0],
                    egui::TextEdit::singleline(&mut self.txt_instr_hoehe).font(FontId::monospace(13.0)).text_color(Color32::WHITE),
                )
            })
            .inner;
        ui.add_sized(
            [label_breite, 24.0],
            egui::Label::new(egui::RichText::new(bezeichnung).strong().size(11.0).color(LABEL_FARBE)),
        );
        if antwort.lost_focus() {
            self.instr_hoehe_geaendert(projekt);
        }
    }
}
