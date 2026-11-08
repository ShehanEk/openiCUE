//! The files Setup installs. Embedded only with the `payload` feature, so
//! tests never need the release binaries.

#[cfg(feature = "payload")]
mod embedded {
    pub static APP: &[u8] = include_bytes!("../../../target/release/openicue.exe");
    pub static ENGINE: &[u8] = include_bytes!("../../../target/release/openicue-engine.exe");
    pub static PAWNIO: &[u8] = include_bytes!("../payload/PawnIO_setup.exe");
    pub static LICENSE_PAWNIO: &[u8] = include_bytes!("../payload/LICENSE-PawnIO.txt");
    pub static PAWNIO_SOURCE: &[u8] = include_bytes!("../payload/PawnIO-SOURCE.txt");
    pub static LICENSE_MODULES: &[u8] = include_bytes!("../../core/modules/LICENSE-PawnIO-Modules.txt");
}

#[cfg(not(feature = "payload"))]
mod embedded {
    pub static APP: &[u8] = &[];
    pub static ENGINE: &[u8] = &[];
    pub static PAWNIO: &[u8] = &[];
    pub static LICENSE_PAWNIO: &[u8] = &[];
    pub static PAWNIO_SOURCE: &[u8] = &[];
    pub static LICENSE_MODULES: &[u8] = &[];
}

pub use embedded::*;

/// False in a build without the payload; such a Setup refuses to run.
pub fn present() -> bool {
    !APP.is_empty() && !ENGINE.is_empty() && !PAWNIO.is_empty()
}
