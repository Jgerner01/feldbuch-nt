// Zweck:    Bettet das Programm-Icon und die Versionsangaben in die EXE ein (Windows-Ressource)
// Hardware: keine
// Autor:    Johann Gerner

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=BUILDNUMMER");
    let buildnummer: u32 = include_str!("BUILDNUMMER").trim().parse().expect("BUILDNUMMER muss eine positive ganze Zahl enthalten");
    assert!(buildnummer > 0, "BUILDNUMMER muss größer als null sein");
    println!("cargo:rustc-env=FELDBUCH_BUILD_NUMMER={buildnummer}");
    println!("cargo:rerun-if-changed=icons/app.ico");
    #[cfg(windows)]
    {
        let mut ressource = winresource::WindowsResource::new();
        ressource
            .set_icon("icons/app.ico")
            .set("ProductName", "Feldbuch-NT")
            .set("FileDescription", "Feldbuch-NT - Geodaetisches Feldbuch")
            .set("LegalCopyright", "(c) 2026 Johann Gerner");
        if let Err(fehler) = ressource.compile() {
            // Ohne Ressourcen-Compiler (rc.exe) entsteht die EXE trotzdem, nur ohne Icon im Explorer
            println!("cargo:warning=Icon konnte nicht eingebettet werden: {fehler}");
        }
    }
}
