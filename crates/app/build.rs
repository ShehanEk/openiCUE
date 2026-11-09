fn main() {
    // The openiCUE icon (logo C), made by scripts/make-icon.ps1. Icon id 1 is
    // what Windows shows for the exe, its shortcuts and the window.
    let res = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/openicue.res");
    println!("cargo:rerun-if-changed={}", res.display());
    println!("cargo:rustc-link-arg-bins={}", res.display());
    // The window writes to the RAM, so Windows must always start it as admin.
    // Only the binary gets the manifest, so `cargo test` still runs unelevated.
    println!("cargo:rustc-link-arg-bins=/MANIFEST:EMBED");
    println!("cargo:rustc-link-arg-bins=/MANIFESTUAC:level='requireAdministrator' uiAccess='false'");
}
