//! SHA-256 (Windows CNG) and Authenticode (WinVerifyTrust) checks.
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::ptr::null_mut;
use windows_sys::Win32::Security::Cryptography::{BCryptHash, BCRYPT_SHA256_ALG_HANDLE};
use windows_sys::Win32::Security::WinTrust::{
    WinVerifyTrust, WINTRUST_ACTION_GENERIC_VERIFY_V2, WINTRUST_DATA, WINTRUST_DATA_0, WINTRUST_FILE_INFO,
    WTD_CACHE_ONLY_URL_RETRIEVAL, WTD_CHOICE_FILE, WTD_REVOKE_NONE, WTD_STATEACTION_CLOSE,
    WTD_STATEACTION_VERIFY, WTD_UI_NONE,
};

/// The official PawnIO 2.2.0 setup. Any other file is never run.
pub const PAWNIO_SHA256: &str = "1F519A22E47187F70A1379A48CA604981C4FCF694F4E65B734AAA74A9FBA3032";

pub fn sha256(data: &[u8]) -> Result<[u8; 32], i32> {
    let mut out = [0u8; 32];
    let len = u32::try_from(data.len()).map_err(|_| -1)?;
    // SAFETY: SHA-256 pseudo-handle (no secret); `data` and `out` are valid for the lengths given.
    let status = unsafe { BCryptHash(BCRYPT_SHA256_ALG_HANDLE, std::ptr::null(), 0, data.as_ptr(), len, out.as_mut_ptr(), 32) };
    if status < 0 { Err(status) } else { Ok(out) }
}

pub fn hex(bytes: &[u8; 32]) -> String {
    bytes.iter().map(|b| format!("{b:02X}")).collect()
}

/// True only for a file with a valid, trusted Authenticode signature. Uses
/// cached revocation data only, so it never reaches the network.
pub fn is_signed(path: &Path) -> bool {
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
    // SAFETY: zeroed is valid for these plain C structs; sizes are set below.
    let mut file: WINTRUST_FILE_INFO = unsafe { std::mem::zeroed() };
    file.cbStruct = std::mem::size_of::<WINTRUST_FILE_INFO>() as u32;
    file.pcwszFilePath = wide.as_ptr();
    // SAFETY: as above; every field is an integer, a pointer or a union of pointers.
    let mut data: WINTRUST_DATA = unsafe { std::mem::zeroed() };
    data.cbStruct = std::mem::size_of::<WINTRUST_DATA>() as u32;
    data.dwUIChoice = WTD_UI_NONE;
    data.fdwRevocationChecks = WTD_REVOKE_NONE;
    data.dwUnionChoice = WTD_CHOICE_FILE;
    data.Anonymous = WINTRUST_DATA_0 { pFile: &mut file };
    data.dwStateAction = WTD_STATEACTION_VERIFY;
    data.dwProvFlags = WTD_CACHE_ONLY_URL_RETRIEVAL;
    let mut action = WINTRUST_ACTION_GENERIC_VERIFY_V2;
    // SAFETY: `data`, `file` and `wide` outlive both calls; a null window means no UI.
    let result = unsafe { WinVerifyTrust(null_mut(), &mut action, (&mut data as *mut WINTRUST_DATA).cast()) };
    data.dwStateAction = WTD_STATEACTION_CLOSE;
    // SAFETY: as above; releases the state the first call kept.
    unsafe { WinVerifyTrust(null_mut(), &mut action, (&mut data as *mut WINTRUST_DATA).cast()) };
    result == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_matches_the_standard_vectors() {
        assert_eq!(hex(&sha256(b"abc").unwrap()), "BA7816BF8F01CFEA414140DE5DAE2223B00361A396177A9CB410FF61F20015AD");
        assert_eq!(hex(&sha256(b"").unwrap()), "E3B0C44298FC1C149AFBF4C8996FB92427AE41E4649B934CA495991B7852B855");
    }

    #[test]
    fn the_bundled_pawnio_matches_its_pin() {
        let bytes = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/payload/PawnIO_setup.exe")).unwrap();
        assert_eq!(hex(&sha256(&bytes).unwrap()), PAWNIO_SHA256);
    }

    #[test]
    fn an_unsigned_file_is_rejected() {
        let path = std::env::temp_dir().join("openicue-unsigned-test.exe");
        std::fs::write(&path, b"MZ not really a program").unwrap();
        assert!(!is_signed(&path));
        std::fs::remove_file(path).unwrap();
    }
}
