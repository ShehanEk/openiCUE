//! The clipboard, the user's browser and the Windows version, for the
//! unsupported-stick report.
use openicue_gdi::wide;
use std::ptr::{null, null_mut};
use windows_sys::Win32::Foundation::{CloseHandle, GlobalFree, HWND};
use windows_sys::Win32::System::DataExchange::{CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData};
use windows_sys::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
use windows_sys::Win32::System::Ole::CF_UNICODETEXT;
use windows_sys::Win32::System::Registry::{RegGetValueW, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ};
use windows_sys::Win32::System::SystemInformation::GetWindowsDirectoryW;
use windows_sys::Win32::System::Threading::{CreateProcessW, PROCESS_INFORMATION, STARTUPINFOW};

/// "Windows 11, build 26200". The build number comes from the registry,
/// because GetVersionEx reports what the manifest declares, not the truth.
pub fn os_label() -> String {
    let key = wide(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion");
    let name = wide("CurrentBuildNumber");
    let mut buffer = [0u16; 32];
    let mut size = std::mem::size_of_val(&buffer) as u32;
    // SAFETY: the buffer and its size in bytes match; the value is a short string.
    let status = unsafe {
        RegGetValueW(HKEY_LOCAL_MACHINE, key.as_ptr(), name.as_ptr(), RRF_RT_REG_SZ, null_mut(), buffer.as_mut_ptr().cast(), &mut size)
    };
    let len = buffer.iter().position(|&c| c == 0).unwrap_or(0);
    let build: u32 = if status == 0 { String::from_utf16_lossy(&buffer[..len]).parse().unwrap_or(0) } else { 0 };
    match build {
        0 => "Windows, build unknown".to_string(),
        22000.. => format!("Windows 11, build {build}"),
        _ => format!("Windows 10, build {build}"),
    }
}

/// Puts `text` on the clipboard. Returns false if Windows refused.
pub fn copy_text(hwnd: HWND, text: &str) -> bool {
    let units: Vec<u16> = text.replace('\n', "\r\n").encode_utf16().chain(std::iter::once(0)).collect();
    let bytes = units.len() * 2;
    // SAFETY: standard clipboard sequence. The memory is ours until
    // SetClipboardData succeeds, after which the clipboard owns it.
    unsafe {
        if OpenClipboard(hwnd) == 0 {
            return false;
        }
        EmptyClipboard();
        let memory = GlobalAlloc(GMEM_MOVEABLE, bytes);
        let mut ok = false;
        if !memory.is_null() {
            let target = GlobalLock(memory) as *mut u16;
            if !target.is_null() {
                std::ptr::copy_nonoverlapping(units.as_ptr(), target, units.len());
                GlobalUnlock(memory);
                ok = !SetClipboardData(u32::from(CF_UNICODETEXT), memory).is_null();
            }
            if !ok {
                GlobalFree(memory);
            }
        }
        CloseClipboard();
        ok
    }
}

/// Opens an https link in the user's browser, without admin rights.
/// The window runs as admin, and a browser started straight from it would
/// be too. Explorer hands the link to the signed-in user's shell instead.
/// Explorer comes from the Windows folder by full path, never a search.
pub fn open_link(url: &str) -> bool {
    if !url.starts_with("https://") || url.contains('"') {
        return false;
    }
    let mut dir = [0u16; 260];
    // SAFETY: the buffer length is passed in characters.
    let len = unsafe { GetWindowsDirectoryW(dir.as_mut_ptr(), dir.len() as u32) } as usize;
    if len == 0 || len >= dir.len() {
        return false;
    }
    let explorer = format!("{}\\explorer.exe", String::from_utf16_lossy(&dir[..len]));
    let app = wide(&explorer);
    let mut command = wide(&format!("\"{explorer}\" \"{url}\""));
    // SAFETY: zeroed structs with their sizes set; both handles are closed.
    unsafe {
        let mut startup: STARTUPINFOW = std::mem::zeroed();
        startup.cb = std::mem::size_of::<STARTUPINFOW>() as u32;
        let mut info: PROCESS_INFORMATION = std::mem::zeroed();
        let ok = CreateProcessW(app.as_ptr(), command.as_mut_ptr(), null(), null(), 0, 0, null(), null(), &startup, &mut info);
        if ok == 0 {
            return false;
        }
        CloseHandle(info.hThread);
        CloseHandle(info.hProcess);
        true
    }
}
