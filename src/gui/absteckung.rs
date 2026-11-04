// Zweck:    Auswahl der Absteckungs- und Teilungsmodule (derzeit Flächenteilung)
// Hardware: keine
// Autor:    Johann Gerner

use eframe::egui;

use super::flaechenteilung::FlaechenteilungAnsicht;
use crate::daten::projekt::Projekt;

/// Größe der Auswahlknöpfe.
const KNOPF: [f32; 2] = [260.0, 34.0];

/// Noch nicht portierte Module der Absteckung.
const OFFEN: [&str; 3] = ["Punktabsteckung", "Achsabsteckung", "Schnurgerüst"];

/// Auswahl der Absteckungsmodule.
#[derive(Default)]
pub struct AbsteckungAnsicht {
    flaechenteilung: Option<Box<FlaechenteilungAnsicht>>,
    hinweis: bool,
}

impl AbsteckungAnsicht {
    pub fn zeichne(&mut self, ui: &mut egui::Ui, projekt: &Projekt) {
        if let Some(ansicht) = self.flaechenteilung.as_mut() {
            if ui.button("← Absteckung").clicked() {
                self.flaechenteilung = None;
                return;
            }
            egui::ScrollArea::both().show(ui, |ui| ansicht.zeichne(ui, projekt));
            return;
        }
        ui.heading("Absteckung");
        ui.add_space(8.0);
        for name in OFFEN {
            if ui.add_sized(KNOPF, egui::Button::new(name)).clicked() {
                self.hinweis = true;
            }
        }
        if ui.add_sized(KNOPF, egui::Button::new("Flächenteilung")).clicked() {
            self.hinweis = false;
            self.flaechenteilung = Some(Box::default());
        }
        if self.hinweis {
            ui.add_space(8.0);
            ui.label("Dieses Modul ist in Feldbuch-NT noch nicht umgesetzt.");
        }
    }
}
