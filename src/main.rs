// Zweck:    Einstiegspunkt des Feldbuchs (Rust-Neuimplementierung "Feldbuch-NT")
// Hardware: Tachymeter über serielle Schnittstelle (Leica, Sokkia, Topcon), GNSS-Empfänger u-blox ZED-F9P
// Autor:    Johann Gerner

#![windows_subsystem = "windows"]

mod daten;
mod dxf;
mod gnss;
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

/// Umgebungsvariable, mit der die Grafikschnittstelle von Hand gewählt werden kann (z. B. `dx12`, `vulkan`, `opengl`).
const UMGEBUNG_GRAFIK: &str = "WGPU_BACKEND";

/// Wählt DirectX 12 (mit OpenGL als Ersatz) statt Vulkan: Der Vulkan-Treiber älterer Intel-Grafik (igvk64.dll) stürzt
/// beim Start ab, zum Beispiel auf dem Panasonic Toughpad FZ-M1. Eine gesetzte Umgebungsvariable hat Vorrang.
fn waehle_grafikschnittstelle(options: &mut eframe::NativeOptions) {
    use eframe::egui_wgpu::WgpuSetup;
    use eframe::wgpu::Backends;
    if std::env::var_os(UMGEBUNG_GRAFIK).is_some() {
        return;
    }
    if let WgpuSetup::CreateNew(neu) = &mut options.wgpu_options.wgpu_setup {
        neu.instance_descriptor.backends = Backends::DX12 | Backends::GL;
    }
}

fn main() -> eframe::Result {
    // Installationsmodus der neuen Version nach einem Update: kein Fenster, nur Dateien austauschen und neu starten
    let argumente: Vec<String> = std::env::args().collect();
    if argumente.get(1).map(String::as_str) == Some(update::INSTALL_ARGUMENT) {
        update::fuehre_aus(&argumente);
        return Ok(());
    }
    // nur eine Instanz: zwei Feldbücher würden dieselben Projektdateien und Schnittstellen benutzen
    if !grundlagen::einzelinstanz::belegen() {
        return Ok(());
    }
    let mut viewport = eframe::egui::ViewportBuilder::default().with_inner_size(WINDOW_SIZE);
    if let Some(icon) = fenster_icon() {
        viewport = viewport.with_icon(icon);
    }
    let mut options = eframe::NativeOptions { viewport, ..Default::default() };
    waehle_grafikschnittstelle(&mut options);
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
