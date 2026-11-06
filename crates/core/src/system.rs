//! Windows objects that only Administrators and System can touch.
use crate::config;
use crate::effects::Settings;
use std::ffi::OsString;
use std::io::Read;
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
use std::os::windows::io::AsRawHandle;
use std::path::PathBuf;
use std::ptr::null_mut;
use windows_sys::core::PWSTR;
use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, LocalFree, ERROR_ALREADY_EXISTS, HANDLE, WAIT_TIMEOUT,
};
use windows_sys::Win32::Security::Authorization::{
    ConvertStringSecurityDescriptorToSecurityDescriptorW, GetSecurityInfo, SDDL_REVISION_1,
    SE_FILE_OBJECT, SE_KERNEL_OBJECT, SE_OBJECT_TYPE,
};
use windows_sys::Win32::Security::{
    IsWellKnownSid, WinBuiltinAdministratorsSid, WinLocalSystemSid,
    OWNER_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, PSID, SECURITY_ATTRIBUTES,
};
use windows_sys::Win32::Storage::FileSystem::CreateDirectoryW;
use windows_sys::Win32::System::Com::CoTaskMemFree;
use windows_sys::Win32::System::Threading::{CreateEventW, CreateMutexW, SetEvent, WaitForSingleObject};
use windows_sys::Win32::UI::Shell::{FOLDERID_ProgramData, SHGetKnownFolderPath};

/// The data folder exists but a non-admin could have made or swapped it.
/// The fix is to delete `C:\ProgramData\openiCUE`. Bit 29 marks it as ours.
pub const ERROR_DATA_DIR_UNTRUSTED: u32 = 0x2000_0001;
/// A named object already exists and its owner is not Administrators or System.
pub const ERROR_UNTRUSTED_OWNER: u32 = 0x2000_0002;

/// Owned by Administrators; only System and Administrators get access.
const ADMIN_ONLY: &str = "O:BAD:P(A;OICI;GA;;;SY)(A;OICI;GA;;;BA)";
const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
const FILE_READ_ATTRIBUTES: u32 = 0x80;
const READ_CONTROL: u32 = 0x0002_0000;
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
        AdminDescriptor::from_sddl(ADMIN_ONLY)
    }

    fn from_sddl(sddl: &str) -> Result<AdminDescriptor, u32> {
        let text = wide(sddl);
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

    /// Whether this descriptor's owner is Administrators or System.
    #[cfg(test)]
    fn owner_is_admin(&self) -> bool {
        use windows_sys::Win32::Security::GetSecurityDescriptorOwner;
        let mut owner: PSID = null_mut();
        let mut defaulted = 0;
        // SAFETY: `self.0` is a valid descriptor until drop; the out pointers
        // are valid locals. `owner` points into `self.0`, alive for the check.
        let ok = unsafe { GetSecurityDescriptorOwner(self.0, &mut owner, &mut defaulted) };
        // SAFETY: `owner` is null or a SID inside `self.0`.
        ok != 0 && unsafe { is_admin_sid(owner) }
    }
}

impl Drop for AdminDescriptor {
    fn drop(&mut self) {
        // SAFETY: `self.0` came from ConvertStringSecurityDescriptor..., which
        // allocates with LocalAlloc, and is freed exactly once here.
        unsafe { LocalFree(self.0) };
    }
}

/// True only for BUILTIN\Administrators and LocalSystem.
///
/// # Safety
/// `sid` must be null or point to a valid SID.
unsafe fn is_admin_sid(sid: PSID) -> bool {
    !sid.is_null()
        && (IsWellKnownSid(sid, WinBuiltinAdministratorsSid) != 0
            || IsWellKnownSid(sid, WinLocalSystemSid) != 0)
}

/// Fails with `ERROR_UNTRUSTED_OWNER` unless the object behind `handle` is
/// owned by Administrators or System. `handle` needs READ_CONTROL.
fn require_admin_owner(handle: HANDLE, object_type: SE_OBJECT_TYPE) -> Result<(), u32> {
    let mut owner: PSID = null_mut();
    let mut descriptor: PSECURITY_DESCRIPTOR = null_mut();
    // SAFETY: `handle` is open for the call; the out pointers are valid
    // locals, and the unused ones may be null.
    let status = unsafe {
        GetSecurityInfo(handle, object_type, OWNER_SECURITY_INFORMATION,
            &mut owner, null_mut(), null_mut(), null_mut(), &mut descriptor)
    };
    if status != 0 {
        return Err(status);
    }
    // SAFETY: `owner` points into `descriptor`, which is still allocated.
    let trusted = unsafe { is_admin_sid(owner) };
    // SAFETY: GetSecurityInfo allocated `descriptor` with LocalAlloc; freed once.
    unsafe { LocalFree(descriptor) };
    if trusted { Ok(()) } else { Err(ERROR_UNTRUSTED_OWNER) }
}

/// The ProgramData folder from the shell, not from an environment variable.
fn program_data() -> Result<PathBuf, u32> {
    let mut raw: PWSTR = null_mut();
    // SAFETY: the GUID is a static; a null token means the current user;
    // `raw` is a valid out pointer.
    let status = unsafe { SHGetKnownFolderPath(&FOLDERID_ProgramData, 0, null_mut(), &mut raw) };
    let result = if status < 0 || raw.is_null() {
        Err(status as u32)
    } else {
        let mut len = 0;
        // SAFETY: on success `raw` is a NUL-terminated string, read up to the NUL.
        while unsafe { *raw.add(len) } != 0 {
            len += 1;
        }
        // SAFETY: the `len` units before the NUL were just read as valid.
        let units = unsafe { std::slice::from_raw_parts(raw, len) };
        Ok(PathBuf::from(OsString::from_wide(units)))
    };
    // SAFETY: the buffer must be freed even on failure; null is allowed.
    unsafe { CoTaskMemFree(raw as *const _) };
    result
}

/// Creates the data folder admin-only. If it already exists, it is used only
/// when it is a real folder owned by Administrators or System; otherwise this
/// returns `ERROR_DATA_DIR_UNTRUSTED`. It never changes an existing folder's
/// permissions, because a non-admin's earlier handle would keep working.
pub fn secure_data_dir() -> Result<PathBuf, u32> {
    let dir = program_data()?.join("openiCUE");
    let path: Vec<u16> = dir.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
    let descriptor = AdminDescriptor::new()?;
    let attributes = descriptor.attributes();
    // SAFETY: `path` is NUL-terminated; `attributes` points into `descriptor`;
    // both are alive for the call.
    if unsafe { CreateDirectoryW(path.as_ptr(), &attributes) } != 0 {
        return Ok(dir);
    }
    let error = last_error();
    if error != ERROR_ALREADY_EXISTS {
        return Err(error);
    }
    // Open the entry itself, never what a link or junction points at.
    let handle = std::fs::OpenOptions::new()
        .access_mode(FILE_READ_ATTRIBUTES | READ_CONTROL)
        .share_mode(FILE_SHARE_READ_WRITE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(&dir)
        .map_err(os_error)?;
    let metadata = handle.metadata().map_err(os_error)?;
    if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 || !metadata.is_dir() {
        return Err(ERROR_DATA_DIR_UNTRUSTED);
    }
    match require_admin_owner(handle.as_raw_handle() as HANDLE, SE_FILE_OBJECT) {
        Ok(()) => Ok(dir),
        Err(ERROR_UNTRUSTED_OWNER) => Err(ERROR_DATA_DIR_UNTRUSTED),
        Err(other) => Err(other),
    }
}

/// Reads at most one byte past the limit, and rejects anything over it.
fn read_limited(reader: impl Read) -> Option<String> {
    let mut text = String::new();
    reader.take(config::MAX_SETTINGS_BYTES as u64 + 1).read_to_string(&mut text).ok()?;
    (text.len() <= config::MAX_SETTINGS_BYTES).then_some(text)
}

/// Any problem gives the default settings, and nothing from a bad file is used.
pub fn load_settings() -> Settings {
    let read = || -> Option<Settings> {
        let path = secure_data_dir().ok()?.join("settings.txt");
        config::parse(&read_limited(std::fs::File::open(path).ok()?)?)
    };
    read().unwrap_or_default()
}

pub fn save_settings(settings: &Settings) -> Result<(), u32> {
    let path = secure_data_dir()?.join("settings.txt");
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
    if handle.is_null() {
        return Err(last_error());
    }
    let existed = last_error() == ERROR_ALREADY_EXISTS;
    let event = StopEvent(handle);
    if existed {
        // Dropping `event` on failure closes the handle.
        require_admin_owner(event.0, SE_KERNEL_OBJECT)?;
    }
    Ok(event)
}

impl StopEvent {
    pub fn signal(&self) {
        // SAFETY: `self.0` is an open event handle until drop.
        unsafe { SetEvent(self.0) };
    }
    /// Waits up to `ms`. Returns true if a stop was requested, and also on
    /// any wait failure, so a broken handle can't spin the frame loop.
    pub fn wait_ms(&self, ms: u32) -> bool {
        // SAFETY: `self.0` is an open event handle until drop.
        unsafe { WaitForSingleObject(self.0, ms) != WAIT_TIMEOUT }
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

/// `Ok(None)` means an admin-owned engine already holds the claim.
pub fn claim_engine() -> Result<Option<EngineClaim>, u32> {
    let descriptor = AdminDescriptor::new()?;
    let attributes = descriptor.attributes();
    let name = wide("Global\\openiCUE.Engine");
    // SAFETY: as in `open_stop_event`.
    let handle = unsafe { CreateMutexW(&attributes, 0, name.as_ptr()) };
    if handle.is_null() {
        return Err(last_error());
    }
    let existed = last_error() == ERROR_ALREADY_EXISTS;
    let claim = EngineClaim(handle);
    if existed {
        // Dropping `claim` closes the handle on either path.
        require_admin_owner(claim.0, SE_KERNEL_OBJECT)?;
        return Ok(None);
    }
    Ok(Some(claim))
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
    fn only_administrators_and_system_are_trusted_owners() {
        let owner = |sddl: &str| AdminDescriptor::from_sddl(sddl).expect("descriptor").owner_is_admin();
        assert!(owner(ADMIN_ONLY));
        assert!(owner("O:SY"));
        assert!(!owner("O:WD"), "Everyone");
        assert!(!owner("O:BU"), "Users");
        assert!(!owner("O:AU"), "Authenticated Users");
        assert!(!owner("D:P(A;;GA;;;BA)"), "no owner at all");
    }

    #[test]
    fn settings_reads_stop_past_the_limit() {
        let max = config::MAX_SETTINGS_BYTES;
        assert_eq!(read_limited(&vec![b'a'; max][..]).map(|t| t.len()), Some(max));
        assert_eq!(read_limited(&vec![b'a'; max + 1][..]), None);
        assert_eq!(read_limited(&vec![b'a'; max * 50][..]), None);
        assert_eq!(read_limited(&[0xFF, 0xFE][..]), None, "invalid UTF-8");
    }
}
