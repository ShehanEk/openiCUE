//! Windows objects that only Administrators and System can touch.
use crate::config;
use crate::effects::Settings;
use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
use std::os::windows::io::AsRawHandle;
use std::path::PathBuf;
use std::ptr::null_mut;
use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, LocalFree, ERROR_ALREADY_EXISTS, HANDLE, WAIT_OBJECT_0,
};
use windows_sys::Win32::Security::Authorization::{
    ConvertStringSecurityDescriptorToSecurityDescriptorW, SetSecurityInfo, SDDL_REVISION_1,
    SE_FILE_OBJECT,
};
use windows_sys::Win32::Security::{
    GetSecurityDescriptorDacl, GetSecurityDescriptorOwner, ACL, DACL_SECURITY_INFORMATION,
    OWNER_SECURITY_INFORMATION, PROTECTED_DACL_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, PSID,
    SECURITY_ATTRIBUTES,
};
use windows_sys::Win32::System::Threading::{CreateEventW, CreateMutexW, SetEvent, WaitForSingleObject};

/// Owned by Administrators; only System and Administrators get access.
const ADMIN_ONLY: &str = "O:BAD:P(A;OICI;GA;;;SY)(A;OICI;GA;;;BA)";
const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
const ERROR_REPARSE_POINT_FOUND: u32 = 4390;
const ERROR_INVALID_SECURITY_DESCR: u32 = 1338;
const FILE_READ_ATTRIBUTES: u32 = 0x80;
const READ_CONTROL: u32 = 0x0002_0000;
const WRITE_DAC: u32 = 0x0004_0000;
const WRITE_OWNER: u32 = 0x0008_0000;
const FILE_SHARE_READ_WRITE: u32 = 0x1 | 0x2;
const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

fn last_error() -> u32 {
    // SAFETY: takes no arguments and only reads this thread's last error.
    unsafe { GetLastError() }
}

fn os_error(error: std::io::Error) -> u32 {
    error.raw_os_error().unwrap_or(0) as u32
}

pub struct AdminDescriptor(PSECURITY_DESCRIPTOR);

impl AdminDescriptor {
    pub fn new() -> Result<AdminDescriptor, u32> {
        let text = wide(ADMIN_ONLY);
        let mut descriptor: PSECURITY_DESCRIPTOR = null_mut();
        // SAFETY: `text` is NUL-terminated and outlives the call; `descriptor`
        // is a valid out pointer, and the size pointer may be null.
        let ok = unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                text.as_ptr(), SDDL_REVISION_1, &mut descriptor, null_mut())
        };
        if ok == 0 { Err(last_error()) } else { Ok(AdminDescriptor(descriptor)) }
    }

    fn attributes(&self) -> SECURITY_ATTRIBUTES {
        SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: self.0,
            bInheritHandle: 0,
        }
    }

    /// The DACL inside this descriptor. Errors rather than return a null
    /// DACL, because a null DACL grants everyone full access.
    fn dacl(&self) -> Result<*mut ACL, u32> {
        let mut present = 0;
        let mut acl: *mut ACL = null_mut();
        let mut defaulted = 0;
        // SAFETY: `self.0` is a valid descriptor until drop; the out pointers
        // are valid locals. `acl` points into `self.0`, so it lives as long as `self`.
        let ok = unsafe { GetSecurityDescriptorDacl(self.0, &mut present, &mut acl, &mut defaulted) };
        if ok == 0 {
            return Err(last_error());
        }
        if present == 0 || acl.is_null() {
            return Err(ERROR_INVALID_SECURITY_DESCR);
        }
        Ok(acl)
    }

    /// The owner SID inside this descriptor (Administrators).
    fn owner(&self) -> Result<PSID, u32> {
        let mut owner: PSID = null_mut();
        let mut defaulted = 0;
        // SAFETY: as in `dacl`; `owner` points into `self.0`.
        let ok = unsafe { GetSecurityDescriptorOwner(self.0, &mut owner, &mut defaulted) };
        if ok == 0 {
            return Err(last_error());
        }
        if owner.is_null() {
            return Err(ERROR_INVALID_SECURITY_DESCR);
        }
        Ok(owner)
    }
}

impl Drop for AdminDescriptor {
    fn drop(&mut self) {
        // SAFETY: `self.0` came from ConvertStringSecurityDescriptor..., which
        // allocates with LocalAlloc, and is freed exactly once here.
        unsafe { LocalFree(self.0) };
    }
}

/// Creates the data folder if needed and always resets its owner and
/// permissions, because a non-admin could have created the folder first.
pub fn secure_data_dir() -> Result<PathBuf, u32> {
    let base = std::env::var_os("ProgramData").ok_or(0u32)?;
    let dir = PathBuf::from(base).join("openiCUE");
    std::fs::create_dir_all(&dir).map_err(os_error)?;
    // Open the folder itself, never a link or junction a non-admin planted
    // here, and keep it open so it can't be swapped before the reset.
    // Delete sharing is off, so nobody can rename or remove it meanwhile.
    let handle = std::fs::OpenOptions::new()
        .access_mode(FILE_READ_ATTRIBUTES | READ_CONTROL | WRITE_DAC | WRITE_OWNER)
        .share_mode(FILE_SHARE_READ_WRITE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(&dir)
        .map_err(os_error)?;
    let metadata = handle.metadata().map_err(os_error)?;
    if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 || !metadata.is_dir() {
        return Err(ERROR_REPARSE_POINT_FOUND);
    }
    let descriptor = AdminDescriptor::new()?;
    let (owner, acl) = (descriptor.owner()?, descriptor.dacl()?);
    // SAFETY: `handle` is an open directory handle with WRITE_DAC and
    // WRITE_OWNER; `owner` and `acl` point into `descriptor`, alive here.
    let status = unsafe {
        SetSecurityInfo(
            handle.as_raw_handle() as HANDLE, SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
            owner, null_mut(), acl, null_mut())
    };
    if status != 0 { Err(status) } else { Ok(dir) }
}

/// Any problem gives the default settings, and nothing from a bad file is used.
pub fn load_settings() -> Settings {
    let read = || -> Option<Settings> {
        let path = secure_data_dir().ok()?.join("settings.txt");
        if std::fs::metadata(&path).ok()?.len() > config::MAX_SETTINGS_BYTES as u64 {
            return None;
        }
        config::parse(&std::fs::read_to_string(path).ok()?)
    };
    read().unwrap_or_default()
}

pub fn save_settings(settings: &Settings) -> Result<(), u32> {
    let path = secure_data_dir()?.join("settings.txt");
    // Replace rather than overwrite, so a file planted before the folder was
    // secured goes away and the new one inherits the admin-only permissions.
    match std::fs::remove_file(&path) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(os_error(e)),
        _ => {}
    }
    std::fs::write(path, config::serialize(settings)).map_err(os_error)
}

pub struct StopEvent(HANDLE);

pub fn open_stop_event() -> Result<StopEvent, u32> {
    let descriptor = AdminDescriptor::new()?;
    let attributes = descriptor.attributes();
    let name = wide("Global\\openiCUE.Engine.Stop");
    // Manual reset is off, so one signal stops one engine.
    // SAFETY: `attributes` and `name` are valid and outlive the call;
    // `attributes` points into `descriptor`, which is alive here.
    let handle = unsafe { CreateEventW(&attributes, 0, 0, name.as_ptr()) };
    if handle.is_null() { Err(last_error()) } else { Ok(StopEvent(handle)) }
}

impl StopEvent {
    pub fn signal(&self) {
        // SAFETY: `self.0` is an open event handle until drop.
        unsafe { SetEvent(self.0) };
    }
    /// Waits up to `ms`. Returns true if a stop was requested.
    pub fn wait_ms(&self, ms: u32) -> bool {
        // SAFETY: `self.0` is an open event handle until drop.
        unsafe { WaitForSingleObject(self.0, ms) == WAIT_OBJECT_0 }
    }
}

impl Drop for StopEvent {
    fn drop(&mut self) {
        // SAFETY: `self.0` is owned by this value and closed exactly once.
        unsafe { CloseHandle(self.0) };
    }
}

/// Held for the engine's lifetime, so only one engine runs.
pub struct EngineClaim(HANDLE);

pub fn claim_engine() -> Result<Option<EngineClaim>, u32> {
    let descriptor = AdminDescriptor::new()?;
    let attributes = descriptor.attributes();
    let name = wide("Global\\openiCUE.Engine");
    // SAFETY: as in `open_stop_event`.
    let handle = unsafe { CreateMutexW(&attributes, 0, name.as_ptr()) };
    if handle.is_null() {
        return Err(last_error());
    }
    if last_error() == ERROR_ALREADY_EXISTS {
        // SAFETY: `handle` is open, not stored anywhere, and closed once.
        unsafe { CloseHandle(handle) };
        return Ok(None);
    }
    Ok(Some(EngineClaim(handle)))
}

impl Drop for EngineClaim {
    fn drop(&mut self) {
        // SAFETY: `self.0` is owned by this value and closed exactly once.
        unsafe { CloseHandle(self.0) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wide_strings_end_in_nul() {
        assert_eq!(wide("ab"), vec![b'a' as u16, b'b' as u16, 0]);
    }

    #[test]
    fn admin_descriptor_has_an_owner_and_a_real_dacl() {
        let descriptor = AdminDescriptor::new().expect("descriptor");
        assert!(descriptor.owner().is_ok());
        assert!(descriptor.dacl().is_ok());
    }
}
