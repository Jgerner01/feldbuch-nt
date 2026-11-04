// Zweck:    Einstiegspunkt des Feldbuchs (Rust-Neuimplementierung "Feldbuch-NT")
// Hardware: Tachymeter über serielle Schnittstelle (Leica, Sokkia, Topcon, GNSS)
// Autor:    Johann Gerner

#![windows_subsystem = "windows"]

mod daten;
mod dxf;
mod grundlagen;
mod gui;
mod protokolle;
mod rechnen;
mod tachymeter;

/// Fenstertitel beim Start.
const WINDOW_TITLE: &str = "Feldbuch";
/// Anfangsgröße des Fensters in Pixeln (Breite, Höhe).
const WINDOW_SIZE: [f32; 2] = [1100.0, 760.0];

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default().with_inner_size(WINDOW_SIZE),
        ..Default::default()
    };
    eframe::run_native(
        WINDOW_TITLE,
        options,
        Box::new(|cc| {
            egui_extras::install_image_loaders(&cc.egui_ctx);
            Ok(Box::new(gui::FeldbuchApp::neu()))
        }),
    )
}
