// Zweck:    Auswahl der Absteckungs- und Teilungsmodule (Punkt-, Achs-, Raster-, Profilabsteckung, Schnurgerüst, Flächenteilung)
// Hardware: Tachymeter nur im Schnurgerüst (optional)
// Autor:    Johann Gerner

mod achse;
pub(crate) mod gemeinsam;
mod profil;
mod punkt;
mod raster;
mod schnurgeruest;

use eframe::egui;

use super::flaechenteilung::FlaechenteilungAnsicht;
use super::geraet::Geraet;
use crate::daten::projekt::Projekt;

/// Größe der Auswahlknöpfe.
const KNOPF: [f32; 2] = [260.0, 34.0];

/// Das geöffnete Modul.
enum Modul {
    Punkt(Box<punkt::PunktAnsicht>),
    Achse(Box<achse::AchseAnsicht>),
    Raster(Box<raster::RasterAnsicht>),
    Profil(Box<profil::ProfilAnsicht>),
    Schnurgeruest(Box<schnurgeruest::SchnurgeruestAnsicht>),
    Flaechenteilung(Box<FlaechenteilungAnsicht>),
}

/// Auswahl der Absteckungsmodule.
#[derive(Default)]
pub struct AbsteckungAnsicht {
    modul: Option<Modul>,
}

impl AbsteckungAnsicht {
    pub fn zeichne(&mut self, ui: &mut egui::Ui, projekt: &Projekt, geraet: &mut Geraet) {
        if let Some(modul) = self.modul.as_mut() {
            if ui.button("← Absteckung").clicked() {
                // Beim Verlassen schreiben die Module ihr Protokoll (Drop)
                self.modul = None;
                return;
            }
            ui.add_space(4.0);
            egui::ScrollArea::both().show(ui, |ui| match modul {
                Modul::Punkt(m) => m.zeichne(ui, projekt),
                Modul::Achse(m) => m.zeichne(ui, projekt),
                Modul::Raster(m) => m.zeichne(ui, projekt),
                Modul::Profil(m) => m.zeichne(ui, projekt),
                Modul::Schnurgeruest(m) => m.zeichne(ui, projekt, geraet),
                Modul::Flaechenteilung(m) => m.zeichne(ui, projekt),
            });
            return;
        }
        ui.heading("Absteckung");
        ui.add_space(8.0);
        let knopf = |ui: &mut egui::Ui, name: &str| ui.add_sized(KNOPF, egui::Button::new(name)).clicked();
        if knopf(ui, "Punktabsteckung") {
            self.modul = Some(Modul::Punkt(Box::new(punkt::PunktAnsicht::neu(projekt))));
        }
        if knopf(ui, "Achsabsteckung") {
            self.modul = Some(Modul::Achse(Box::new(achse::AchseAnsicht::neu(projekt))));
        }
        if knopf(ui, "Rasterabsteckung") {
            self.modul = Some(Modul::Raster(Box::new(raster::RasterAnsicht::neu(projekt))));
        }
        if knopf(ui, "Profilabsteckung") {
            self.modul = Some(Modul::Profil(Box::new(profil::ProfilAnsicht::neu(projekt))));
        }
        if knopf(ui, "Schnurgerüst") {
            self.modul = Some(Modul::Schnurgeruest(Box::new(schnurgeruest::SchnurgeruestAnsicht::neu(projekt))));
        }
        if knopf(ui, "Flächenteilung") {
            self.modul = Some(Modul::Flaechenteilung(Box::default()));
        }
    }
}
