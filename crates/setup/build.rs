fn main() {
    // The openiCUE icon (logo C), made by scripts/make-icon.ps1. Icon id 1 is
    // what Windows shows for the exe, its shortcuts and the window.
    let res = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/openicue.res");
    println!("cargo:rerun-if-changed={}", res.display());
    println!("cargo:rustc-link-arg-bins={}", res.display());
    // Setup changes Program Files, so Windows must start it as admin.
    println!("cargo:rustc-link-arg-bins=/MANIFEST:EMBED");
    println!("cargo:rustc-link-arg-bins=/MANIFESTUAC:level='requireAdministrator' uiAccess='false'");
    // Load-time imports (dwmapi, wintrust, bcrypt, ...) come from System32
    // only, never from Setup's own folder (Downloads): LOAD_LIBRARY_SEARCH_SYSTEM32.
    println!("cargo:rustc-link-arg-bins=/DEPENDENTLOADFLAG:0x800");
}
