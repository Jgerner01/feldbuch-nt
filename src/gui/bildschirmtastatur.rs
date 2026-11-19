// Zweck:    Bildschirmtastatur für alle Eingabefelder (alle Fenster) auf dem Panasonic Toughpad/Toughbook:
//           Textfelder (Standpunkt, Projekt-/Dateinamen) bekommen eine QWERTZ-Tastatur, alle anderen Felder
//           (auch in Tabellen) einen Ziffernblock. Als egui-Plugin: die Tasten werden im Eingabestrom
//           bearbeitet, das Feld behält dadurch den Fokus und erhält normale Text-/Tastenereignisse.
// Hardware: Panasonic Toughpad/Toughbook (Touch-Bedienung im Feld)
// Autor:    Johann Gerner

use std::collections::{HashMap, HashSet};
use std::sync::{LazyLock, Mutex};

use eframe::egui::{self, Align2, Color32, Event, FontId, Id, Key, LayerId, Modifiers, Order, Pos2, Rect, Stroke, Vec2, ViewportId};

/// Größe einer Taste und Abstand [Punkte] (fingertauglich).
const TASTE_BREITE: f32 = 56.0;
const TASTE_HOEHE: f32 = 50.0;
const TASTE_ABSTAND: f32 = 4.0;
/// Innenabstand des Tastaturrahmens und Abstand zum Fensterrand.
const RAHMEN_ABSTAND: f32 = 8.0;
const FENSTER_RAND: f32 = 8.0;
/// Abgerundete Ecken.
const ECKEN_RADIUS: f32 = 6.0;
/// Schriftgröße der Tastenbeschriftung.
const TASTE_SCHRIFT: f32 = 22.0;
/// Breiten in Tasteneinheiten.
const BREITE_NORMAL: f32 = 1.0;
const BREITE_SONDER: f32 = 1.5;
const BREITE_ENTER: f32 = 2.0;
const BREITE_LEERTASTE: f32 = 5.0;
/// Farben.
const FARBE_RAHMEN: Color32 = Color32::from_rgb(30, 36, 52);
const FARBE_TASTE: Color32 = Color32::from_rgb(60, 95, 160);
const FARBE_TASTE_GEDRUECKT: Color32 = Color32::from_rgb(110, 150, 220);
const FARBE_SONDERTASTE: Color32 = Color32::from_rgb(90, 98, 120);
const FARBE_ENTER: Color32 = Color32::from_rgb(42, 130, 65);
const FARBE_RUECK: Color32 = Color32::from_rgb(160, 60, 20);
const FARBE_UMSCHALT_AKTIV: Color32 = Color32::from_rgb(210, 150, 30);
const FARBE_RAND: Color32 = Color32::from_rgb(120, 140, 190);

/// Beschriftungen der Sondertasten.
const TXT_RUECK: &str = "⌫";
const TXT_ENTER: &str = "OK";
const TXT_TAB: &str = "⇥";
const TXT_UMSCHALT: &str = "⇧";
const TXT_LEERTASTE: &str = "Leer";

/// Zeilen der QWERTZ-Tastatur (Zeichen je Taste).
const ZEILE_ZAHLEN: &str = "1234567890ß";
const ZEILE_OBEN: &str = "qwertzuiopü";
const ZEILE_MITTE: &str = "asdfghjklöä";
const ZEILE_UNTEN: &str = "yxcvbnm,.-_";
const ZEILE_SONDER: &str = "/\\:";

/// Felder, die Text statt Zahlen erwarten (QWERTZ-Tastatur).
static TEXT_FELDER: LazyLock<Mutex<HashSet<Id>>> = LazyLock::new(|| Mutex::new(HashSet::new()));

/// Kennzeichnet ein Eingabefeld als Textfeld: auf dem Toughpad erscheint dafür die volle Tastatur.
pub fn text_feld(antwort: &egui::Response) {
    if let Ok(mut felder) = TEXT_FELDER.lock() {
        felder.insert(antwort.id);
    }
}

fn ist_text_feld(id: Id) -> bool {
    TEXT_FELDER.lock().is_ok_and(|felder| felder.contains(&id))
}

/// Installiert die Bildschirmtastatur – nur auf einem Panasonic Toughpad/Toughbook.
pub fn installiere(ctx: &egui::Context) {
    if crate::grundlagen::geraetetyp::ist_panasonic_toughpad() {
        ctx.add_plugin(Bildschirmtastatur::default());
    }
}

/// Wirkung einer Taste.
#[derive(Clone, Debug, PartialEq)]
enum Aktion {
    Zeichen(String),
    Rueck,
    Enter,
    Tab,
    Umschalt,
}

struct Taste {
    rect: Rect,
    beschriftung: String,
    aktion: Aktion,
}

/// Zustand je Fenster (Viewport).
struct Zustand {
    sichtbar: bool,
    text_modus: bool,
    /// Tastatur am oberen Fensterrand (wenn das Feld in der unteren Hälfte liegt).
    oben: bool,
    grossbuchstaben: bool,
    gedrueckt: Option<usize>,
    fokus: Option<Id>,
    /// Beim ersten Fokus den Feldinhalt markieren, damit die erste Eingabe ihn ersetzt.
    alles_markieren: bool,
    bildschirm: Rect,
}

impl Default for Zustand {
    fn default() -> Self {
        Self {
            sichtbar: false,
            text_modus: false,
            oben: false,
            grossbuchstaben: false,
            gedrueckt: None,
            fokus: None,
            alles_markieren: false,
            bildschirm: Rect::NOTHING,
        }
    }
}

#[derive(Default)]
struct Bildschirmtastatur {
    zustaende: HashMap<ViewportId, Zustand>,
}

/// Eine Taste der Zeilenbeschreibung: Beschriftung, Aktion, Breite in Tasteneinheiten.
type TastenDef = (String, Aktion, f32);

fn zeichen_tasten(zeichen: &str, gross: bool) -> Vec<TastenDef> {
    zeichen
        .chars()
        .map(|c| {
            let mut wert: String = if gross { c.to_uppercase().collect() } else { c.to_string() };
            if wert.chars().count() != 1 {
                wert = c.to_string();
            }
            (wert.clone(), Aktion::Zeichen(wert), BREITE_NORMAL)
        })
        .collect()
}

fn text_zeilen(gross: bool) -> Vec<Vec<TastenDef>> {
    let mut zahlen = zeichen_tasten(ZEILE_ZAHLEN, false);
    zahlen.push((TXT_RUECK.into(), Aktion::Rueck, BREITE_SONDER));
    let mut unten = vec![(TXT_UMSCHALT.into(), Aktion::Umschalt, BREITE_SONDER)];
    unten.extend(zeichen_tasten(ZEILE_UNTEN, gross));
    let mut letzte = zeichen_tasten(ZEILE_SONDER, false);
    letzte.push((TXT_LEERTASTE.into(), Aktion::Zeichen(" ".into()), BREITE_LEERTASTE));
    letzte.push((TXT_ENTER.into(), Aktion::Enter, BREITE_ENTER));
    vec![zahlen, zeichen_tasten(ZEILE_OBEN, gross), zeichen_tasten(ZEILE_MITTE, gross), unten, letzte]
}

fn ziffern_zeilen() -> Vec<Vec<TastenDef>> {
    let z = |c: &str| (c.to_string(), Aktion::Zeichen(c.to_string()), BREITE_NORMAL);
    vec![
        vec![z("7"), z("8"), z("9"), (TXT_RUECK.into(), Aktion::Rueck, BREITE_NORMAL)],
        vec![z("4"), z("5"), z("6"), (TXT_TAB.into(), Aktion::Tab, BREITE_NORMAL)],
        vec![z("1"), z("2"), z("3"), z("-")],
        vec![z("0"), z("."), (TXT_ENTER.into(), Aktion::Enter, BREITE_ENTER)],
    ]
}

/// Breite einer Zeile in Punkten.
fn zeilen_breite(zeile: &[TastenDef]) -> f32 {
    zeile.iter().map(|(_, _, b)| b * TASTE_BREITE + (b - 1.0) * TASTE_ABSTAND).sum::<f32>() + TASTE_ABSTAND * (zeile.len().saturating_sub(1)) as f32
}

/// Rahmen der Tastatur und alle Tasten für das Fenster `bildschirm`.
fn layout(text_modus: bool, gross: bool, oben: bool, bildschirm: Rect) -> (Rect, Vec<Taste>) {
    let zeilen = if text_modus { text_zeilen(gross) } else { ziffern_zeilen() };
    let breite = zeilen.iter().map(|z| zeilen_breite(z)).fold(0.0, f32::max) + 2.0 * RAHMEN_ABSTAND;
    let hoehe = zeilen.len() as f32 * (TASTE_HOEHE + TASTE_ABSTAND) - TASTE_ABSTAND + 2.0 * RAHMEN_ABSTAND;
    let links = if text_modus { bildschirm.center().x - breite / 2.0 } else { bildschirm.right() - breite - FENSTER_RAND };
    let links = links.max(bildschirm.left());
    let oberkante = if oben { bildschirm.top() + FENSTER_RAND } else { bildschirm.bottom() - hoehe - FENSTER_RAND };
    let rahmen = Rect::from_min_size(Pos2::new(links, oberkante), Vec2::new(breite, hoehe));
    let mut tasten = Vec::new();
    for (nr, zeile) in zeilen.into_iter().enumerate() {
        let mut x = rahmen.left() + (breite - zeilen_breite(&zeile)) / 2.0;
        let y = rahmen.top() + RAHMEN_ABSTAND + nr as f32 * (TASTE_HOEHE + TASTE_ABSTAND);
        for (beschriftung, aktion, b) in zeile {
            let w = b * TASTE_BREITE + (b - 1.0) * TASTE_ABSTAND;
            tasten.push(Taste { rect: Rect::from_min_size(Pos2::new(x, y), Vec2::new(w, TASTE_HOEHE)), beschriftung, aktion });
            x += w + TASTE_ABSTAND;
        }
    }
    (rahmen, tasten)
}

fn taste_ereignis(key: Key, pressed: bool, modifiers: Modifiers) -> Event {
    Event::Key { key, physical_key: Some(key), pressed, repeat: false, modifiers }
}

/// Drücken und Loslassen einer Taste.
fn taste_anschlag(key: Key, modifiers: Modifiers) -> [Event; 2] {
    [taste_ereignis(key, true, modifiers), taste_ereignis(key, false, modifiers)]
}

impl egui::Plugin for Bildschirmtastatur {
    fn debug_name(&self) -> &'static str {
        "Bildschirmtastatur"
    }

    /// Tippt auf die Tastatur werden hier aus dem Eingabestrom genommen und in Text-/Tastenereignisse verwandelt.
    fn input_hook(&mut self, _ctx: &egui::Context, input: &mut egui::RawInput) {
        let Some(z) = self.zustaende.get_mut(&input.viewport_id) else {
            return;
        };
        if !z.sichtbar {
            return;
        }
        let bildschirm = input.screen_rect.unwrap_or(z.bildschirm);
        let (rahmen, tasten) = layout(z.text_modus, z.grossbuchstaben, z.oben, bildschirm);
        let mut neue: Vec<Event> = Vec::new();
        if z.alles_markieren {
            z.alles_markieren = false;
            let strg = Modifiers { ctrl: true, command: true, ..Modifiers::NONE };
            neue.extend(taste_anschlag(Key::A, strg));
        }
        input.events.retain(|e| {
            let Event::PointerButton { pos, pressed, .. } = e else {
                return true;
            };
            if !pressed {
                z.gedrueckt = None;
            }
            if !rahmen.contains(*pos) {
                return true;
            }
            if *pressed {
                z.gedrueckt = tasten.iter().position(|t| t.rect.contains(*pos));
                if let Some(taste) = z.gedrueckt.map(|i| &tasten[i]) {
                    match &taste.aktion {
                        Aktion::Zeichen(text) => {
                            neue.push(Event::Text(text.clone()));
                            if text.chars().any(char::is_alphabetic) {
                                z.grossbuchstaben = false;
                            }
                        }
                        Aktion::Rueck => neue.extend(taste_anschlag(Key::Backspace, Modifiers::NONE)),
                        Aktion::Enter => neue.extend(taste_anschlag(Key::Enter, Modifiers::NONE)),
                        Aktion::Tab => neue.extend(taste_anschlag(Key::Tab, Modifiers::NONE)),
                        Aktion::Umschalt => z.grossbuchstaben = !z.grossbuchstaben,
                    }
                }
            }
            false
        });
        input.events.extend(neue);
    }

    fn on_end_pass(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let bildschirm = ctx.content_rect();
        let z = self.zustaende.entry(ctx.viewport_id()).or_default();
        z.bildschirm = bildschirm;
        let fokus = ctx.memory(|m| m.focused()).filter(|_| ctx.text_edit_focused());
        match fokus {
            Some(id) => {
                if z.fokus != Some(id) {
                    z.alles_markieren = true;
                    ctx.request_repaint();
                }
                z.fokus = Some(id);
                z.sichtbar = true;
                z.text_modus = ist_text_feld(id);
                z.oben = ctx.read_response(id).is_some_and(|r| r.rect.center().y > bildschirm.center().y);
            }
            None => {
                z.sichtbar = false;
                z.fokus = None;
                z.gedrueckt = None;
            }
        }
        if !z.sichtbar {
            return;
        }
        let (rahmen, tasten) = layout(z.text_modus, z.grossbuchstaben, z.oben, bildschirm);
        let maler = ctx.layer_painter(LayerId::new(Order::Foreground, Id::NULL));
        maler.rect(rahmen, ECKEN_RADIUS, FARBE_RAHMEN, Stroke::new(1.0, FARBE_RAND), egui::StrokeKind::Inside);
        for (nr, taste) in tasten.iter().enumerate() {
            let farbe = if z.gedrueckt == Some(nr) {
                FARBE_TASTE_GEDRUECKT
            } else {
                match taste.aktion {
                    Aktion::Enter => FARBE_ENTER,
                    Aktion::Rueck => FARBE_RUECK,
                    Aktion::Umschalt if z.grossbuchstaben => FARBE_UMSCHALT_AKTIV,
                    Aktion::Umschalt | Aktion::Tab => FARBE_SONDERTASTE,
                    Aktion::Zeichen(_) => FARBE_TASTE,
                }
            };
            maler.rect_filled(taste.rect, ECKEN_RADIUS, farbe);
            maler.text(taste.rect.center(), Align2::CENTER_CENTER, &taste.beschriftung, FontId::proportional(TASTE_SCHRIFT), Color32::WHITE);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bildschirm() -> Rect {
        Rect::from_min_size(Pos2::ZERO, Vec2::new(1280.0, 800.0))
    }

    #[test]
    fn tasten_liegen_im_fenster_und_im_rahmen() {
        for text in [false, true] {
            for oben in [false, true] {
                let (rahmen, tasten) = layout(text, false, oben, bildschirm());
                assert!(bildschirm().contains_rect(rahmen), "Rahmen außerhalb (text={text})");
                assert!(tasten.iter().all(|t| rahmen.contains_rect(t.rect)));
            }
        }
    }

    #[test]
    fn ziffernblock_enthaelt_alle_ziffern() {
        let (_, tasten) = layout(false, false, false, bildschirm());
        for ziffer in "0123456789.-".chars() {
            assert!(tasten.iter().any(|t| t.aktion == Aktion::Zeichen(ziffer.to_string())), "{ziffer} fehlt");
        }
        assert!(tasten.iter().any(|t| t.aktion == Aktion::Rueck));
    }

    #[test]
    fn grossbuchstaben_nur_bei_umschalt() {
        let klein = zeichen_tasten("aß", false);
        let gross = zeichen_tasten("aß", true);
        assert_eq!(klein[0].1, Aktion::Zeichen("a".into()));
        assert_eq!(gross[0].1, Aktion::Zeichen("A".into()));
        // ß bleibt ß (Großschreibung wäre „SS")
        assert_eq!(gross[1].1, Aktion::Zeichen("ß".into()));
    }
}
