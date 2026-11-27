//! Zweck:    Einrichtungsassistent („Schnelleinrichtung“): führt Schritt für Schritt
//!           durch die Unterreiter der Konfiguration, beginnend mit der
//!           Datenverbindung zum Empfänger.
//! Hardware: simpleRTK2B (u-blox ZED-F9P), SAPOS HEPS
//! Autor:    Johann Gerner
//!
//! Übernommen aus GNSS-NT (src/bin/gnss-gui/view_wizard.rs, Stand 27.11.2026), eigenständig gepflegt in Feldbuch-NT.
//! Letzter Schritt angepasst: Farbschema, Vergrößerung und Update-Suche richten sich nach Feldbuch-NT.

use crate::gnss::ansicht::app::Action;
use crate::gnss::ansicht::view_config::ConfigTab;
use crate::gnss::ansicht::widgets::*;
use eframe::egui::{self, Color32, Ui};
use crate::gnss::kern::measure::Criteria;
use crate::gnss::kern::model::{NtripPhase, State};
use crate::gnss::kern::settings::{HeightMode, Settings};
use std::path::Path;

/// Ergebnis der Prüfung eines Schrittes
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum StepStatus {
    /// eingerichtet und geprüft
    Done,
    /// eingetragen bzw. Vorgabe, aber noch nicht geprüft
    Open,
    /// Pflichtangabe fehlt
    Missing,
}

impl StepStatus {
    fn show(self) -> (Color32, &'static str) {
        match self {
            StepStatus::Done => (GOOD, "erledigt"),
            StepStatus::Open => (WARN, "eingetragen, nicht geprüft"),
            StepStatus::Missing => (BAD, "Angabe fehlt"),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Step {
    Port,
    Receiver,
    SaposAccess,
    Mountpoint,
    Antenna,
    Measure,
    Output,
    Finish,
}

const STEPS: [Step; 8] = [
    Step::Port,
    Step::Receiver,
    Step::SaposAccess,
    Step::Mountpoint,
    Step::Antenna,
    Step::Measure,
    Step::Output,
    Step::Finish,
];

impl Step {
    fn title(self) -> &'static str {
        match self {
            Step::Port => "Datenverbindung zum Empfänger",
            Step::Receiver => "Empfänger einstellen",
            Step::SaposAccess => "SAPOS-Zugang",
            Step::Mountpoint => "Mountpoint und SAPOS-Start",
            Step::Antenna => "Antenne und Höhenbezug",
            Step::Measure => "Messvorgaben",
            Step::Output => "Ausgabe",
            Step::Finish => "Töne und Abschluss",
        }
    }

    fn hint(self) -> &'static str {
        match self {
            Step::Port => "COM-Port des simpleRTK2B wählen (USB-Buchse des ZED-F9P). Die Baudrate ist bei USB ohne Bedeutung. \
                           Danach mit „Verbindung testen“ prüfen, ob Daten eintreffen.",
            Step::Receiver => "Messrate, Dynamikmodell und Elevationsmaske. Die Vorgaben passen für die Punktaufnahme. \
                               Bei bestehender Verbindung lassen sich die Werte übertragen und zurücklesen.",
            Step::SaposAccess => "Zugangsdaten des SAPOS-HEPS-Dienstes eintragen. Das Passwort wird in der \
                                  Windows-Anmeldeinformationsverwaltung abgelegt. Mit „Mountpoints abrufen“ wird der Zugang geprüft.",
            Step::Mountpoint => "Mountpoint wählen (Vorgabe VRS_3_4G_BY). Für den Test muss der Empfänger verbunden sein; \
                                 „SAPOS verbinden“ prüft die Anmeldung, grün bei aktiver Verbindung.",
            Step::Antenna => "Stabhöhe und Antennenoffset prüfen und den Höhenbezug wählen. \
                              Für DHHN2016 entweder SAPOS-Transformation, Quasigeoid-Gitter oder konstante Höhenanomalie.",
            Step::Measure => "Abbruchkriterien der Punktmessung. Die Vorgaben genügen üblichen Katasteranforderungen; \
                              mit „Vorgaben übernehmen“ werden sie wiederhergestellt.",
            Step::Output => "Punktdatei sowie Projekt und Bearbeiter für das Messprotokoll festlegen.",
            Step::Finish => "Töne nach Wunsch (Farbschema und Vergrößerung richten sich nach Feldbuch-NT). \
                             „Fertig“ speichert alle Einstellungen.",
        }
    }

    fn config_tab(self) -> ConfigTab {
        match self {
            Step::Port => ConfigTab::Port,
            Step::Receiver => ConfigTab::Receiver,
            Step::SaposAccess | Step::Mountpoint => ConfigTab::Sapos,
            Step::Antenna => ConfigTab::Antenna,
            Step::Measure => ConfigTab::Measure,
            Step::Output => ConfigTab::Output,
            Step::Finish => ConfigTab::Display,
        }
    }

    /// Beschriftungen der Felder, die im Konfigurationsreiter hervorgehoben werden
    fn fields(self) -> &'static [&'static str] {
        match self {
            Step::Port => &["Schnittstelle", "Baudrate"],
            Step::Receiver => &["Messrate [ms]", "Dynamikmodell", "Elevationsmaske [°]"],
            Step::SaposAccess => &["Caster", "Port", "Benutzer", "Passwort"],
            Step::Mountpoint => &["Mountpoint", "Beim Verbinden SAPOS starten"],
            Step::Antenna => &["Stabhöhe [m]", "Antennenoffset ARP → Phasenzentrum [m]", "Höhenbezug"],
            Step::Measure => &["Epochen je Punkt", "max. σ 2D [m]", "max. σ Höhe [m]"],
            Step::Output => &["Punktdatei", "Projekt", "Bearbeiter"],
            Step::Finish => &["Töne bei Fix-Gewinn, Fix-Verlust und fertigem Punkt"],
        }
    }

    /// Pflichtschritte: „Weiter“ erst bei vorhandener Angabe
    fn required(self) -> bool {
        matches!(self, Step::Port | Step::SaposAccess | Step::Mountpoint | Step::Output | Step::Antenna)
    }
}

/// Zustand, den der Assistent zur Prüfung braucht
pub struct Inputs<'a> {
    pub st: &'a State,
    pub connected: bool,
    pub ntrip_running: bool,
    pub password_set: bool,
    pub mountpoints: usize,
}

fn status(step: Step, s: &Settings, i: &Inputs) -> StepStatus {
    match step {
        Step::Port if s.device.trim().is_empty() => StepStatus::Missing,
        Step::Port if i.connected && !i.st.is_stale() => StepStatus::Done,
        Step::Receiver if !i.st.readback.is_empty() => StepStatus::Done,
        Step::SaposAccess if s.user.trim().is_empty() || !i.password_set => StepStatus::Missing,
        Step::SaposAccess if i.mountpoints > 0 || i.st.ntrip.phase == NtripPhase::Active => StepStatus::Done,
        Step::Mountpoint if s.mountpoint.trim().is_empty() => StepStatus::Missing,
        Step::Mountpoint if i.st.ntrip.phase == NtripPhase::Active => StepStatus::Done,
        Step::Antenna if !s.sapos_transform && s.height_mode == HeightMode::Grid && !Path::new(&s.geoid_file).is_file() => {
            StepStatus::Missing
        }
        Step::Antenna | Step::Measure | Step::Finish => StepStatus::Done,
        Step::Output if s.output.trim().is_empty() => StepStatus::Missing,
        Step::Output if Path::new(&s.output).parent().is_some_and(Path::is_dir) => StepStatus::Done,
        _ => StepStatus::Open,
    }
}

#[derive(Default)]
pub struct Wizard {
    pub active: bool,
    step: usize,
}

impl Wizard {
    fn current(&self) -> Step {
        STEPS[self.step.min(STEPS.len() - 1)]
    }

    /// Springt zu einem Schritt; liefert den zugehörigen Konfigurationsreiter.
    pub fn goto(&mut self, index: usize) -> ConfigTab {
        self.active = true;
        self.step = index.min(STEPS.len() - 1);
        self.current().config_tab()
    }

    /// Felder, die im Konfigurationsreiter hervorgehoben werden
    pub fn highlight(&self) -> &'static [&'static str] {
        if self.active { self.current().fields() } else { &[] }
    }

    /// Assistentenleiste über der Statusleiste.
    pub fn show(&self, ui: &mut Ui, s: &mut Settings, i: &Inputs, actions: &mut Vec<Action>) {
        let step = self.current();
        let state = status(step, s, i);
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.strong(format!("Schnelleinrichtung – Schritt {} von {}: {}", self.step + 1, STEPS.len(), step.title()));
            let (color, text) = state.show();
            ui.colored_label(color, format!("● {text}"));
        });
        ui.label(step.hint());
        ui.horizontal_wrapped(|ui| step_tools(ui, step, s, i, actions));
        if step == Step::Finish {
            summary(ui, s, i);
        }
        ui.separator();
        ui.horizontal(|ui| {
            if ui.add_enabled(self.step > 0, egui::Button::new("◀ Zurück")).clicked() {
                actions.push(Action::WizardGoto(self.step - 1));
            }
            let can_go = !(step.required() && state == StepStatus::Missing);
            let last = self.step + 1 == STEPS.len();
            let next = egui::Button::new(if last { "✔ Fertig" } else { "Weiter ▶" });
            let response = ui
                .add_enabled(can_go, next)
                .on_disabled_hover_text("Erst die Pflichtangabe eintragen oder „Überspringen“ wählen");
            if response.clicked() {
                actions.push(Action::SaveSettings);
                actions.push(if last { Action::WizardFinish } else { Action::WizardGoto(self.step + 1) });
            }
            if !last && ui.button("Überspringen").clicked() {
                actions.push(Action::WizardGoto(self.step + 1));
            }
            if ui.button("Abbrechen").on_hover_text("Bereits gespeicherte Schritte bleiben erhalten").clicked() {
                actions.push(Action::WizardClose);
            }
        });
        ui.add_space(4.0);
    }
}

/// Prüf- und Hilfsknöpfe des jeweiligen Schrittes.
fn step_tools(ui: &mut Ui, step: Step, s: &mut Settings, i: &Inputs, actions: &mut Vec<Action>) {
    match step {
        Step::Port => {
            if i.connected {
                if i.st.is_stale() {
                    ui.colored_label(WARN, "verbunden, aber noch keine Positionsdaten");
                } else {
                    ui.colored_label(GOOD, "Daten vom Empfänger treffen ein");
                }
                if let Some((sw, hw, _)) = &i.st.version {
                    ui.label(format!("Empfänger: {sw}, {hw}"));
                }
            } else if ui.add_enabled(!s.device.is_empty(), egui::Button::new("Verbindung testen")).clicked() {
                actions.push(Action::Connect);
            }
        }
        Step::Receiver => {
            let enabled = i.connected && !i.st.config_running;
            if ui.add_enabled(enabled, egui::Button::new("Übertragen (bis zum Ausschalten)")).clicked() {
                actions.push(Action::Configure { persist: false });
            }
            if !i.connected {
                ui.weak("Empfänger nicht verbunden – Vorgaben werden beim Verbinden übertragen.");
            }
        }
        Step::SaposAccess => {
            if ui.button("Mountpoints abrufen").clicked() {
                actions.push(Action::FetchSourcetable);
            }
            if i.mountpoints > 0 {
                ui.colored_label(GOOD, format!("{} Mountpoints empfangen", i.mountpoints));
            }
        }
        Step::Mountpoint => {
            if !i.connected {
                ui.weak("Für den Test zuerst den Empfänger verbinden (Schritt 1).");
            } else if i.ntrip_running {
                let color = if i.st.ntrip.phase == NtripPhase::Active { GOOD } else { WARN };
                ui.colored_label(color, format!("SAPOS {}", i.st.ntrip.phase.label()));
            } else if ui.button("SAPOS verbinden").clicked() {
                actions.push(Action::NtripStart);
            }
        }
        Step::Measure => {
            if ui.button("Vorgaben übernehmen").clicked() {
                s.set_criteria(&Criteria::default());
            }
        }
        Step::Antenna | Step::Output | Step::Finish => {}
    }
}

/// Übersicht aller Schritte mit Ampel.
fn summary(ui: &mut Ui, s: &Settings, i: &Inputs) {
    egui::Grid::new("wizard_summary").num_columns(2).spacing([16.0, 2.0]).show(ui, |ui| {
        for (n, step) in STEPS.iter().enumerate().take(STEPS.len() - 1) {
            ui.label(format!("{}. {}", n + 1, step.title()));
            let (color, text) = status(*step, s, i).show();
            ui.colored_label(color, format!("● {text}"));
            ui.end_row();
        }
    });
}
