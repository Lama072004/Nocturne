//! Build-Skript: läuft vor dem Kompilieren des Rust-Kerns.
//!
//! Einzige Aufgabe: die Android-Bibliothek auf 16-KB-Speicherseiten ausrichten.
//!
//! WARUM? Neue Android-Geräte (ab Android 15, vor allem aktuelle Pixel) nutzen
//! 16 KB große Speicherseiten statt 4 KB. Eine Bibliothek, die nur auf 4 KB
//! ausgerichtet ist, kann dort nicht geladen werden. Für die App hieße das:
//! Der Rust-Kern fehlt, und sie kommt nicht über das Startbild hinaus.
//! Das NDK 27 richtet standardmäßig noch auf 4 KB aus (erst NDK 28 nimmt
//! 16 KB), deshalb setzen wir es hier ausdrücklich. Auf Geräten mit 4-KB-Seiten
//! funktioniert eine 16-KB-Ausrichtung genauso; die Datei wird nur minimal
//! größer.

fn main() {
    // Die Umgebungsvariable beschreibt das ZIEL des Builds (nicht den PC, auf
    // dem gebaut wird). So bleibt der Build für Windows/Linux unberührt.
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os == "android" {
        println!("cargo:rustc-link-arg=-Wl,-z,max-page-size=16384");
    }
}
