//! The work Setup does. Runs on a worker thread; reports through `report`.
use openicue_core::pawnio::PawnIoBus;
use openicue_core::{control, ram, system};
use openicue_setup::model::Failure;
use openicue_setup::payload;
use openicue_setup::plan::{self, Value};
use openicue_setup::verify::{hex, is_signed, sha256, PAWNIO_SHA256};
use std::os::windows::ffi::OsStrExt;
use std::os::windows::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::ptr::null;
use std::time::Duration;
use windows_sys::Win32::Foundation::CloseHandle;
use windows_sys::Win32::System::Threading::{CreateProcessW, PROCESS_INFORMATION, STARTUPINFOW};
use windows_sys::Win32::Storage::FileSystem::{MoveFileExW, MOVEFILE_DELAY_UNTIL_REBOOT};
use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegDeleteTreeW, RegOpenKeyExW, RegSetValueExW, HKEY, HKEY_LOCAL_MACHINE,
    KEY_READ, KEY_WRITE, REG_DWORD, REG_OPTION_NON_VOLATILE, REG_SZ,
};
use windows_sys::Win32::UI::Shell::{FOLDERID_CommonPrograms, FOLDERID_ProgramFiles, FOLDERID_System};

pub enum Event {
    Step(usize),
    Done,
    Failed(Failure),
}

const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;

fn other(text: &str) -> Failure {
    Failure::Other(text.to_string())
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

fn install_dir() -> Result<PathBuf, Failure> {
    system::known_folder(&FOLDERID_ProgramFiles)
        .map(|p| p.join("openiCUE"))
        .map_err(|_| other("Setup couldn't find the Program Files folder. Nothing was changed."))
}

fn system32() -> Result<PathBuf, Failure> {
    system::known_folder(&FOLDERID_System).map_err(|_| other("Setup couldn't find the Windows folder."))
}

fn is_reparse_point(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|m| m.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0)
}

/// Deletes `path` without ever following a link or junction: a link is
/// removed itself, and `remove_dir_all` doesn't descend into links.
fn remove_tree(path: &Path) -> std::io::Result<()> {
    if !path.exists() && std::fs::symlink_metadata(path).is_err() {
        return Ok(());
    }
    if is_reparse_point(path) {
        std::fs::remove_dir(path).or_else(|_| std::fs::remove_file(path))
    } else {
        std::fs::remove_dir_all(path)
    }
}

/// Writes `bytes` to `path`, then reads it back and compares SHA-256.
fn write_checked(path: &Path, bytes: &[u8]) -> Result<(), Failure> {
    std::fs::write(path, bytes).map_err(|_| other("Close openiCUE, then run Setup again."))?;
    let written = std::fs::read(path).map_err(|_| other("Setup couldn't read back a file it wrote."))?;
    if sha256(&written).ok() != sha256(bytes).ok() {
        return Err(other("A file changed while Setup wrote it. Run Setup again."));
    }
    Ok(())
}

fn pawnio_installed() -> bool {
    let key = wide(r"SYSTEM\CurrentControlSet\Services\PawnIO");
    let mut handle: HKEY = std::ptr::null_mut();
    // SAFETY: NUL-terminated name, valid out handle, closed below.
    let ok = unsafe { RegOpenKeyExW(HKEY_LOCAL_MACHINE, key.as_ptr(), 0, KEY_READ, &mut handle) } == 0;
    if ok {
        // SAFETY: opened above.
        unsafe { RegCloseKey(handle) };
    }
    ok
}

/// Writes the embedded PawnIO setup into the admin-only install folder,
/// checks it there (so it can't be swapped between check and run), runs
/// it with `args`, then deletes it.
fn run_pawnio(dir: &Path, args: &[&str]) -> Result<(), Failure> {
    let path = dir.join("pawnio-setup.exe");
    #[allow(unused_mut)]
    let mut bytes = payload::PAWNIO.to_vec();
    #[cfg(feature = "tamper-test")]
    {
        let last = bytes.len() - 1;
        bytes[last] ^= 0xFF;
    }
    std::fs::write(&path, &bytes).map_err(|_| other("Setup couldn't write the PawnIO setup."))?;
    let written = std::fs::read(&path).map_err(|_| Failure::PawnIoCheck)?;
    let pinned = sha256(&written).map(|h| hex(&h) == PAWNIO_SHA256).unwrap_or(false);
    if !pinned || !is_signed(&path) {
        let _ = std::fs::remove_file(&path);
        return Err(Failure::PawnIoCheck);
    }
    let status = Command::new(&path).args(args).status();
    let _ = std::fs::remove_file(&path);
    match status {
        Ok(s) if s.success() => Ok(()),
        _ => Err(other("The PawnIO setup didn't finish. Run Setup again.")),
    }
}

fn schtasks(args: &[&str]) -> Result<(), Failure> {
    let exe = system32()?.join("schtasks.exe");
    let ok = Command::new(exe).args(args).status().is_ok_and(|s| s.success());
    if ok { Ok(()) } else { Err(other("Setup couldn't create the logon task.")) }
}

fn write_task(dir: &Path) -> Result<(), Failure> {
    let xml_path = dir.join("restore-task.xml");
    let xml = plan::task_xml(&dir.join("openicue-engine.exe"));
    // UTF-16 LE with a byte-order mark, as the XML header declares.
    let mut bytes = vec![0xFF, 0xFE];
    for unit in xml.encode_utf16() {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    std::fs::write(&xml_path, bytes).map_err(|_| other("Setup couldn't write the logon task."))?;
    let path = xml_path.to_string_lossy().to_string();
    let result = schtasks(&["/Create", "/TN", plan::TASK_NAME, "/XML", &path, "/F"]);
    let _ = std::fs::remove_file(xml_path);
    result
}

fn set_uninstall_entry(dir: &Path) -> Result<(), Failure> {
    let fail = || other("Setup couldn't add its uninstall entry.");
    let size_kb = ((payload::APP.len() + payload::ENGINE.len()) / 1024) as u32;
    let key = wide(plan::UNINSTALL_KEY);
    let mut handle: HKEY = std::ptr::null_mut();
    // SAFETY: NUL-terminated key; valid out handle, closed below.
    let created = unsafe {
        RegCreateKeyExW(HKEY_LOCAL_MACHINE, key.as_ptr(), 0, std::ptr::null(), REG_OPTION_NON_VOLATILE,
            KEY_WRITE, std::ptr::null(), &mut handle, std::ptr::null_mut())
    } == 0;
    if !created {
        return Err(fail());
    }
    let mut ok = true;
    for (name, value) in plan::uninstall_values(dir, size_kb) {
        let name = wide(name);
        // SAFETY: each buffer is valid for the byte length passed.
        let status = unsafe {
            match &value {
                Value::Text(text) => {
                    let data = wide(text);
                    RegSetValueExW(handle, name.as_ptr(), 0, REG_SZ, data.as_ptr().cast(), (data.len() * 2) as u32)
                }
                Value::Number(n) => RegSetValueExW(handle, name.as_ptr(), 0, REG_DWORD, (n as *const u32).cast(), 4),
            }
        };
        ok &= status == 0;
    }
    // SAFETY: created above.
    unsafe { RegCloseKey(handle) };
    if ok { Ok(()) } else { Err(fail()) }
}

/// Uses the settings folder if it's trusted; otherwise removes it (never
/// following links) and creates it fresh, admin-only.
fn reset_data_dir() -> Result<(), Failure> {
    if system::secure_data_dir().is_ok() {
        return Ok(());
    }
    let path = system::known_folder(&windows_sys::Win32::UI::Shell::FOLDERID_ProgramData)
        .map_err(|_| other("Setup couldn't find the ProgramData folder."))?
        .join("openiCUE");
    remove_tree(&path).map_err(|_| other(r"Delete C:\ProgramData\openiCUE, then run Setup again."))?;
    system::secure_data_dir().map(|_| ()).map_err(|_| other(r"Delete C:\ProgramData\openiCUE, then run Setup again."))
}

pub fn run_install(report: impl Fn(Event)) {
    match install(&report) {
        Ok(()) => report(Event::Done),
        Err(failure) => report(Event::Failed(failure)),
    }
}

fn install(report: &impl Fn(Event)) -> Result<(), Failure> {
    // Step 0: check every file before changing anything.
    report(Event::Step(0));
    if !payload::present() {
        return Err(other("This copy of Setup is incomplete. Nothing was changed."));
    }
    let pinned = sha256(payload::PAWNIO).map(|h| hex(&h) == PAWNIO_SHA256).unwrap_or(false);
    #[cfg(feature = "tamper-test")]
    let pinned = pinned && false;
    if !pinned {
        return Err(Failure::PawnIoCheck);
    }
    let dir = install_dir()?;
    let self_bytes = std::env::current_exe()
        .and_then(std::fs::read)
        .map_err(|_| other("Setup couldn't read itself. Nothing was changed."))?;

    // Step 1: replace the app folder.
    report(Event::Step(1));
    let _ = control::stop_engine(Duration::from_secs(2));
    if is_reparse_point(&dir) {
        return Err(other(r"C:\Program Files\openiCUE is a link. Remove it, then run Setup again."));
    }
    remove_tree(&dir).map_err(|_| other("Close openiCUE, then run Setup again."))?;
    std::fs::create_dir(&dir).map_err(|_| other("Setup couldn't create its folder."))?;
    write_checked(&dir.join("openicue.exe"), payload::APP)?;
    write_checked(&dir.join("openicue-engine.exe"), payload::ENGINE)?;
    write_checked(&dir.join("setup.exe"), &self_bytes)?;
    write_checked(&dir.join("LICENSE-PawnIO.txt"), payload::LICENSE_PAWNIO)?;
    write_checked(&dir.join("PawnIO-SOURCE.txt"), payload::PAWNIO_SOURCE)?;
    write_checked(&dir.join("LICENSE-PawnIO-Modules.txt"), payload::LICENSE_MODULES)?;

    // Step 2: PawnIO, unless it's already installed.
    report(Event::Step(2));
    if !pawnio_installed() {
        run_pawnio(&dir, &["-install", "-silent"])?;
        if !pawnio_installed() {
            return Err(other("The PawnIO driver didn't install. Run Setup again."));
        }
    }

    // Step 3: settings folder, logon task, Start menu entry, uninstall entry.
    report(Event::Step(3));
    reset_data_dir()?;
    write_task(&dir)?;
    let programs = system::known_folder(&FOLDERID_CommonPrograms)
        .map_err(|_| other("Setup couldn't find the Start menu."))?;
    crate::shortcut::create(&programs.join("openiCUE.lnk"), &dir.join("openicue.exe"))
        .map_err(|_| other("Setup couldn't add openiCUE to the Start menu."))?;
    set_uninstall_entry(&dir)?;
    Ok(())
}

/// "Open openiCUE": starts the installed window by full path, with no
/// arguments, in its own folder.
pub fn open_app() -> Result<(), Failure> {
    let fail = || other("Setup couldn't start openiCUE.");
    let dir = install_dir()?;
    let exe = dir.join("openicue.exe");
    let to_wide = |p: &Path| -> Vec<u16> { p.as_os_str().encode_wide().chain(std::iter::once(0)).collect() };
    let application = to_wide(&exe);
    let folder = to_wide(&dir);
    let mut command: Vec<u16> = Vec::new();
    command.push(u16::from(b'"'));
    command.extend(exe.as_os_str().encode_wide());
    command.push(u16::from(b'"'));
    command.push(0);
    // SAFETY: zeroed is a valid starting value for both plain C structs.
    let mut startup: STARTUPINFOW = unsafe { std::mem::zeroed() };
    startup.cb = std::mem::size_of::<STARTUPINFOW>() as u32;
    // SAFETY: as above.
    let mut info: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };
    // SAFETY: every string is NUL-terminated and outlives the call; `command`
    // is writable as CreateProcessW requires; no handles are inherited.
    let ok = unsafe {
        CreateProcessW(application.as_ptr(), command.as_mut_ptr(), null(), null(), 0, 0,
            null(), folder.as_ptr(), &startup, &mut info)
    };
    if ok == 0 {
        return Err(fail());
    }
    // SAFETY: both handles came from the successful call and are closed once.
    unsafe {
        CloseHandle(info.hThread);
        CloseHandle(info.hProcess);
    }
    Ok(())
}

/// Held while the stick-order picker is open: the driver, the engine claim
/// (so no other openiCUE process writes to the RAM meanwhile) and the
/// sticks being placed. Runs on the window thread only.
pub struct Picker {
    bus: PawnIoBus,
    sticks: Vec<ram::StickInfo>,
    _claim: system::EngineClaim,
}

impl Picker {
    /// Opens the driver, takes the claim, reads the sticks and lights the
    /// first one white. `None` means skip the picker: the driver can't be
    /// opened (PawnIO may need a restart), an engine holds the claim, fewer
    /// than 2 sticks answer, a saved order already fits, or the first light
    /// fails.
    pub fn begin() -> Option<Picker> {
        let mut bus = PawnIoBus::open().ok()?;
        let claim = system::claim_engine().ok()??;
        let sticks: Vec<ram::StickInfo> = ram::detect(&mut bus)
            .into_iter()
            .filter_map(|addr| ram::read_info(&mut bus, addr).ok())
            .collect();
        if sticks.len() < 2 || system::load_order().is_some_and(|order| order.fits(sticks.len())) {
            return None;
        }
        let mut picker = Picker { bus, sticks, _claim: claim };
        // On failure, dropping `picker` sets the sticks black.
        picker.light(0).ok()?;
        Some(picker)
    }

    pub fn count(&self) -> usize {
        self.sticks.len()
    }

    /// Lights `stick` (address order) white and the others black.
    pub fn light(&mut self, stick: usize) -> Result<(), ()> {
        ram::light_only(&mut self.bus, &self.sticks, stick).map_err(|_| ())
    }
}

impl Drop for Picker {
    /// Sets every stick black while the claim is still held; the fields,
    /// claim and driver included, are released right after. Setup doesn't
    /// re-apply an effect: the window or the logon task does that.
    fn drop(&mut self) {
        // No index matches `usize::MAX`, so every stick gets black.
        let _ = ram::light_only(&mut self.bus, &self.sticks, usize::MAX);
    }
}

pub fn run_uninstall(remove_pawnio: bool, report: impl Fn(Event)) {
    match uninstall(remove_pawnio) {
        Ok(()) => report(Event::Done),
        Err(failure) => report(Event::Failed(failure)),
    }
}

fn uninstall(remove_pawnio: bool) -> Result<(), Failure> {
    let dir = install_dir()?;
    let _ = control::stop_engine(Duration::from_secs(2));
    if remove_pawnio {
        run_pawnio(&dir, &["-uninstall", "-silent"])?;
    }
    // Missing pieces are fine: uninstall must work on a half-installed PC.
    let _ = schtasks(&["/Delete", "/TN", plan::TASK_NAME, "/F"]);
    if let Ok(programs) = system::known_folder(&FOLDERID_CommonPrograms) {
        let _ = std::fs::remove_file(programs.join("openiCUE.lnk"));
    }
    let key = wide(plan::UNINSTALL_KEY);
    // SAFETY: NUL-terminated key under HKLM; failure means it's already gone.
    unsafe { RegDeleteTreeW(HKEY_LOCAL_MACHINE, key.as_ptr()) };
    if let Ok(data) = system::known_folder(&windows_sys::Win32::UI::Shell::FOLDERID_ProgramData) {
        let _ = remove_tree(&data.join("openiCUE"));
    }
    // Everything but the running setup.exe goes now; it and the folder go at restart.
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            if entry.file_name() != "setup.exe" {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }
    for path in [dir.join("setup.exe"), dir.clone()] {
        let w: Vec<u16> = path.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
        // SAFETY: NUL-terminated path; a null target means delete.
        unsafe { MoveFileExW(w.as_ptr(), std::ptr::null(), MOVEFILE_DELAY_UNTIL_REBOOT) };
    }
    Ok(())
}
