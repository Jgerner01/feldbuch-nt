// Zweck:    Fenster „Update verfügbar“: zeigt die neue Version samt Hinweisen, lädt sie im Hintergrund mit Fortschritt
//           und stößt die Installation an; die Suche läuft ebenfalls im Hintergrund
// Hardware: keine
// Autor:    Johann Gerner

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::Duration;

use eframe::egui::{self, Align2, Color32, RichText, Vec2};

use crate::grundlagen::fehler_log;
use crate::update::{self, Fehler, UpdateInfo, Version};

/// Ergebnis der Hintergrundsuche.
pub type SuchErgebnis = Result<Option<UpdateInfo>, String>;

/// Startet die Suche nach einer neueren Version in einem eigenen Thread.
pub fn starte_suche() -> Receiver<SuchErgebnis> {
    let (sender, empfaenger) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = sender.send(update::suche(&Version::aktuell()));
    });
    empfaenger
}

/// Was die Anwendung nach dem Zeichnen des Fensters tun soll.
#[derive(Debug, PartialEq, Eq)]
pub enum Aktion {
    Weiter,
    Schliessen,
    /// Das Paket liegt im Ordner bereit: Installation starten und Programm beenden.
    Neustart(PathBuf),
}

/// Das Update-Fenster.
pub struct UpdateDialog {
    info: UpdateInfo,
    fortschritt: Arc<AtomicU64>,
    abbruch: Arc<AtomicBool>,
    laden: Option<Receiver<Result<PathBuf, Fehler>>>,
    meldung: String,
}

impl UpdateDialog {
    pub fn neu(info: UpdateInfo) -> Self {
        Self { info, fortschritt: Arc::new(AtomicU64::new(0)), abbruch: Arc::new(AtomicBool::new(false)), laden: None, meldung: String::new() }
    }

    pub fn ist_am_laden(&self) -> bool {
        self.laden.is_some()
    }

    fn starte_laden(&mut self) {
        self.meldung.clear();
        self.fortschritt.store(0, Ordering::Relaxed);
        self.abbruch.store(false, Ordering::Relaxed);
        let (sender, empfaenger) = mpsc::channel();
        let (info, fortschritt, abbruch) = (self.info.clone(), Arc::clone(&self.fortschritt), Arc::clone(&self.abbruch));
        std::thread::spawn(move || {
            let _ = sender.send(update::lade(&info, &update::update_ordner(), &fortschritt, &abbruch));
        });
        self.laden = Some(empfaenger);
    }

    /// Holt das Ergebnis des Downloads ab, falls es vorliegt.
    fn pruefe_ergebnis(&mut self) -> Aktion {
        let Some(empfaenger) = &self.laden else {
            return Aktion::Weiter;
        };
        match empfaenger.try_recv() {
            Ok(Ok(ordner)) => {
                self.laden = None;
                Aktion::Neustart(ordner)
            }
            Ok(Err(Fehler::Abgebrochen)) => {
                self.laden = None;
                Aktion::Schliessen
            }
            Ok(Err(Fehler::Meldung(text))) => {
                self.laden = None;
                fehler_log::log("Update", &text);
                self.fortschritt.store(0, Ordering::Relaxed);
                self.meldung = format!("Das Update ist fehlgeschlagen. Die installierte Version bleibt unverändert.\n{text}");
                Aktion::Weiter
            }
            Err(TryRecvError::Empty) => Aktion::Weiter,
            Err(TryRecvError::Disconnected) => {
                self.laden = None;
                self.meldung = "Das Update ist fehlgeschlagen (Abbruch im Hintergrund).".to_string();
                Aktion::Weiter
            }
        }
    }

    /// Zeichnet das Fenster. `andere_fenster_offen` verhindert die Installation, solange noch etwas anderes läuft.
    pub fn zeige(&mut self, ctx: &egui::Context, andere_fenster_offen: bool) -> Aktion {
        let ergebnis = self.pruefe_ergebnis();
        if ergebnis != Aktion::Weiter {
            return ergebnis;
        }
        if self.ist_am_laden() {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
        let (mut jetzt, mut spaeter) = (false, false);
        egui::Window::new("Update verfügbar").collapsible(false).resizable(false).anchor(Align2::CENTER_CENTER, Vec2::ZERO).show(ctx, |ui| {
            ui.set_width(480.0);
            ui.label(RichText::new(format!("Version {} ist verfügbar (installiert: {})", self.info.version, Version::aktuell())).strong());
            ui.add_space(4.0);
            egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
                ui.add(egui::TextEdit::multiline(&mut self.info.notizen.as_str()).desired_width(f32::INFINITY));
            });
            ui.add_space(6.0);
            let status = if self.ist_am_laden() {
                "Update wird heruntergeladen …".to_string()
            } else {
                format!("Download: {:.0} MB. Das Programm wird dabei neu gestartet.", self.info.groesse as f64 / 1024.0 / 1024.0)
            };
            ui.label(status);
            ui.add(egui::ProgressBar::new(self.fortschritt.load(Ordering::Relaxed) as f32 / 100.0).show_percentage());
            if !self.meldung.is_empty() {
                ui.colored_label(Color32::from_rgb(180, 40, 40), &self.meldung);
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.add_enabled(!self.ist_am_laden(), egui::Button::new(RichText::new("Jetzt aktualisieren").strong())).clicked() {
                    jetzt = true;
                }
                if ui.button(if self.ist_am_laden() { "Abbrechen" } else { "Später" }).clicked() {
                    spaeter = true;
                }
            });
        });
        if spaeter {
            if self.ist_am_laden() {
                self.abbruch.store(true, Ordering::Relaxed);
            } else {
                return Aktion::Schliessen;
            }
        }
        if jetzt {
            if andere_fenster_offen {
                self.meldung = "Bitte zuerst alle anderen Feldbuch-Fenster schließen.".to_string();
            } else {
                self.starte_laden();
            }
        }
        Aktion::Weiter
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info() -> UpdateInfo {
        UpdateInfo {
            version: Version::parse("1.0.0").unwrap(),
            tag: "v1.0.0".into(),
            notizen: "Neu".into(),
            zip_url: String::new(),
            pruefsummen_url: String::new(),
            groesse: 10 * 1024 * 1024,
        }
    }

    fn mit_kanal() -> (UpdateDialog, mpsc::Sender<Result<PathBuf, Fehler>>) {
        let (sender, empfaenger) = mpsc::channel();
        let mut d = UpdateDialog::neu(info());
        d.laden = Some(empfaenger);
        (d, sender)
    }

    #[test]
    fn ohne_download_passiert_nichts() {
        let mut d = UpdateDialog::neu(info());
        assert!(!d.ist_am_laden());
        assert_eq!(d.pruefe_ergebnis(), Aktion::Weiter);
    }

    #[test]
    fn erfolg_fuehrt_zum_neustart() {
        let (mut d, sender) = mit_kanal();
        assert_eq!(d.pruefe_ergebnis(), Aktion::Weiter, "noch nichts da");
        assert!(d.ist_am_laden());
        sender.send(Ok(PathBuf::from("C:/neu"))).unwrap();
        assert_eq!(d.pruefe_ergebnis(), Aktion::Neustart(PathBuf::from("C:/neu")));
        assert!(!d.ist_am_laden());
    }

    #[test]
    fn abbruch_schliesst_und_fehler_bleibt_sichtbar() {
        let (mut d, sender) = mit_kanal();
        sender.send(Err(Fehler::Abgebrochen)).unwrap();
        assert_eq!(d.pruefe_ergebnis(), Aktion::Schliessen);

        let (mut d, sender) = mit_kanal();
        d.fortschritt.store(40, Ordering::Relaxed);
        sender.send(Err(Fehler::Meldung("Die Prüfsumme des Updates stimmt nicht.".into()))).unwrap();
        assert_eq!(d.pruefe_ergebnis(), Aktion::Weiter);
        assert!(d.meldung.contains("Die installierte Version bleibt unverändert.") && d.meldung.contains("Prüfsumme"));
        assert_eq!(d.fortschritt.load(Ordering::Relaxed), 0);
        assert!(!d.ist_am_laden(), "danach ist ein neuer Versuch möglich");
    }

    #[test]
    fn abgebrochener_hintergrundthread_wird_gemeldet() {
        let (mut d, sender) = mit_kanal();
        drop(sender);
        assert_eq!(d.pruefe_ergebnis(), Aktion::Weiter);
        assert!(d.meldung.starts_with("Das Update ist fehlgeschlagen"));
        assert!(!d.ist_am_laden());
    }
}
