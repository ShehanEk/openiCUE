fn main() {
    // The window writes to the RAM, so Windows must always start it as admin.
    // Only the binary gets the manifest, so `cargo test` still runs unelevated.
    println!("cargo:rustc-link-arg-bins=/MANIFEST:EMBED");
    println!("cargo:rustc-link-arg-bins=/MANIFESTUAC:level='requireAdministrator' uiAccess='false'");
}
