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
mod update;

/// Fenstertitel beim Start.
const WINDOW_TITLE: &str = "Feldbuch";
/// Anfangsgröße des Fensters in Pixeln (Breite, Höhe).
const WINDOW_SIZE: [f32; 2] = [1100.0, 760.0];

/// Programm-Icon (Fenster und Info-Seite).
const APP_ICON: &[u8] = include_bytes!("../icons/app_icon.png");

/// Dekodiert das Programm-Icon für die Titelleiste; ohne lesbares Bild bleibt das Standard-Icon.
fn fenster_icon() -> Option<eframe::egui::IconData> {
    let bild = image::load_from_memory(APP_ICON).ok()?.to_rgba8();
    let (width, height) = bild.dimensions();
    Some(eframe::egui::IconData { rgba: bild.into_raw(), width, height })
}

fn main() -> eframe::Result {
    // Installationsmodus der neuen Version nach einem Update: kein Fenster, nur Dateien austauschen und neu starten
    let argumente: Vec<String> = std::env::args().collect();
    if argumente.get(1).map(String::as_str) == Some(update::INSTALL_ARGUMENT) {
        update::fuehre_aus(&argumente);
        return Ok(());
    }
    let mut viewport = eframe::egui::ViewportBuilder::default().with_inner_size(WINDOW_SIZE);
    if let Some(icon) = fenster_icon() {
        viewport = viewport.with_icon(icon);
    }
    let options = eframe::NativeOptions { viewport, ..Default::default() };
    eframe::run_native(
        WINDOW_TITLE,
        options,
        Box::new(|cc| {
            egui_extras::install_image_loaders(&cc.egui_ctx);
            gui::bildschirmtastatur::installiere(&cc.egui_ctx);
            Ok(Box::new(gui::FeldbuchApp::neu()))
        }),
    )
}
