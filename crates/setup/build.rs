fn main() {
    // Setup changes Program Files, so Windows must start it as admin.
    println!("cargo:rustc-link-arg-bins=/MANIFEST:EMBED");
    println!("cargo:rustc-link-arg-bins=/MANIFESTUAC:level='requireAdministrator' uiAccess='false'");
    // Load-time imports (dwmapi, wintrust, bcrypt, ...) come from System32
    // only, never from Setup's own folder (Downloads): LOAD_LIBRARY_SEARCH_SYSTEM32.
    println!("cargo:rustc-link-arg-bins=/DEPENDENTLOADFLAG:0x800");
}
