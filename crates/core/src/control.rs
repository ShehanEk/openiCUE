//! Starts, stops and measures the background engine for the window.
use crate::costs::Cost;
use crate::system;
use std::os::windows::ffi::OsStrExt;
use std::path::PathBuf;
use std::ptr::null;
use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, FILETIME, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows_sys::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS_EX};
use windows_sys::Win32::System::Threading::{
    CreateProcessW, GetProcessTimes, OpenProcess, QueryFullProcessImageNameW, PROCESS_INFORMATION,
    PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_VM_READ, STARTUPINFOW,
};

pub const ENGINE_EXE: &str = "openicue-engine.exe";
const STOP_POLL_MS: u64 = 50;

#[derive(Debug, PartialEq, Eq)]
pub enum StopError {
    Event(u32),
    StillRunning,
}

#[derive(Clone, Copy, Debug)]
pub struct Sample {
    pub private_bytes: u64,
    pub cpu_time_100ns: u64,
    pub at: Instant,
}

struct Owned(HANDLE);

impl Drop for Owned {
    fn drop(&mut self) {
        // SAFETY: `self.0` is a handle this value owns, closed exactly once.
        unsafe { CloseHandle(self.0) };
    }
}

fn wide(text: &std::ffi::OsStr) -> Vec<u16> {
    text.encode_wide().chain(std::iter::once(0)).collect()
}

/// The engine next to this program. Only this file is ever started or measured.
pub fn engine_path() -> Result<PathBuf, u32> {
    let exe = std::env::current_exe().map_err(|e| e.raw_os_error().unwrap_or(0) as u32)?;
    Ok(exe.parent().ok_or(0u32)?.join(ENGINE_EXE))
}

/// True while an engine holds its claim.
pub fn engine_running() -> bool {
    matches!(system::claim_engine(), Ok(None))
}

/// Asks a running engine to stop and waits until its claim is gone.
pub fn stop_engine(timeout: Duration) -> Result<(), StopError> {
    let event = system::open_stop_event().map_err(StopError::Event)?;
    // A running engine holds the event. If we just created it, nothing is
    // running, and signaling would only stop the next engine at its start.
    if !event.already_existed() {
        return Ok(());
    }
    event.signal();
    let deadline = Instant::now() + timeout;
    while engine_running() {
        if Instant::now() >= deadline {
            return Err(StopError::StillRunning);
        }
        std::thread::sleep(Duration::from_millis(STOP_POLL_MS));
    }
    Ok(())
}

/// Starts `openicue-engine.exe --run` from this program's folder, by full path.
pub fn start_engine() -> Result<(), u32> {
    let path = engine_path()?;
    let application = wide(path.as_os_str());
    let mut command: Vec<u16> = Vec::new();
    command.push(u16::from(b'"'));
    command.extend(path.as_os_str().encode_wide());
    command.extend("\" --run".encode_utf16());
    command.push(0);
    // SAFETY: zeroed is a valid starting value for both plain C structs.
    let mut startup: STARTUPINFOW = unsafe { std::mem::zeroed() };
    startup.cb = std::mem::size_of::<STARTUPINFOW>() as u32;
    // SAFETY: as above.
    let mut info: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };
    // SAFETY: both strings are NUL-terminated and outlive the call; `command`
    // is writable as CreateProcessW requires; no handles are inherited.
    let ok = unsafe {
        CreateProcessW(application.as_ptr(), command.as_mut_ptr(), null(), null(), 0, 0,
            null(), null(), &startup, &mut info)
    };
    if ok == 0 {
        // SAFETY: plain thread-local read.
        return Err(unsafe { GetLastError() });
    }
    drop(Owned(info.hThread));
    drop(Owned(info.hProcess));
    Ok(())
}

fn engine_process_ids() -> Vec<u32> {
    let mut ids = Vec::new();
    // SAFETY: plain call; the handle is checked and owned below.
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snapshot == INVALID_HANDLE_VALUE {
        return ids;
    }
    let snapshot = Owned(snapshot);
    // SAFETY: zeroed is valid; dwSize is set as the API requires.
    let mut entry: PROCESSENTRY32W = unsafe { std::mem::zeroed() };
    entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
    // SAFETY: `entry` is a valid, sized out structure for every call.
    let mut more = unsafe { Process32FirstW(snapshot.0, &mut entry) } != 0;
    while more {
        let len = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(entry.szExeFile.len());
        let name = String::from_utf16_lossy(&entry.szExeFile[..len]);
        if name.eq_ignore_ascii_case(ENGINE_EXE) {
            ids.push(entry.th32ProcessID);
        }
        // SAFETY: as for Process32FirstW.
        more = unsafe { Process32NextW(snapshot.0, &mut entry) } != 0;
    }
    ids
}

fn image_path(process: HANDLE) -> Option<String> {
    let mut buffer = [0u16; 1024];
    let mut size = buffer.len() as u32;
    // SAFETY: `buffer` and `size` describe a valid writable buffer.
    let ok = unsafe { QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, buffer.as_mut_ptr(), &mut size) };
    (ok != 0).then(|| String::from_utf16_lossy(&buffer[..size as usize]))
}

fn ticks(time: FILETIME) -> u64 {
    (u64::from(time.dwHighDateTime) << 32) | u64::from(time.dwLowDateTime)
}

/// Memory and CPU time of the engine started from this program's folder.
/// Any other process with the same name is ignored.
pub fn sample_engine() -> Option<Sample> {
    let expected = engine_path().ok()?;
    let expected = expected.to_string_lossy();
    for id in engine_process_ids() {
        // SAFETY: plain call; the handle is checked and owned below.
        let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ, 0, id) };
        if handle.is_null() {
            continue;
        }
        let process = Owned(handle);
        if !image_path(process.0).is_some_and(|p| p.eq_ignore_ascii_case(&expected)) {
            continue;
        }
        // SAFETY: zeroed is valid for these C structs; sizes are passed.
        let mut memory: PROCESS_MEMORY_COUNTERS_EX = unsafe { std::mem::zeroed() };
        let size = std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32;
        // SAFETY: FILETIME is two u32s, so all-zero is valid.
        let mut times: [FILETIME; 4] = unsafe { std::mem::zeroed() };
        // SAFETY: the out pointers are valid for the sizes given.
        let ok = unsafe {
            GetProcessMemoryInfo(process.0, (&mut memory as *mut PROCESS_MEMORY_COUNTERS_EX).cast(), size) != 0
                && GetProcessTimes(process.0, &mut times[0], &mut times[1], &mut times[2], &mut times[3]) != 0
        };
        if ok {
            return Some(Sample {
                private_bytes: memory.PrivateUsage as u64,
                cpu_time_100ns: ticks(times[2]) + ticks(times[3]),
                at: Instant::now(),
            });
        }
    }
    None
}

/// Cost over the time between two samples. CPU is a share of all logical
/// processors, like Task Manager, and always between 0 and 100.
pub fn cost_between(earlier: &Sample, later: &Sample, processors: usize) -> Cost {
    let wall = later.at.saturating_duration_since(earlier.at).as_secs_f64();
    let busy = later.cpu_time_100ns.saturating_sub(earlier.cpu_time_100ns) as f64 / 10_000_000.0;
    let cpu = if wall > 0.0 && processors > 0 {
        (busy / wall / processors as f64 * 100.0).clamp(0.0, 100.0)
    } else {
        0.0
    };
    Cost { mb: (later.private_bytes as f64 / 1_048_576.0) as f32, cpu: cpu as f32 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn at(start: Instant, ms: u64, cpu_ms: u64, bytes: u64) -> Sample {
        Sample { private_bytes: bytes, cpu_time_100ns: cpu_ms * 10_000, at: start + Duration::from_millis(ms) }
    }

    #[test]
    fn cpu_is_a_share_of_all_processors() {
        let t = Instant::now();
        let cost = cost_between(&at(t, 0, 0, 0), &at(t, 2000, 260, 1_468_006), 1);
        assert!((cost.cpu - 13.0).abs() < 0.01, "{}", cost.cpu);
        assert!((cost.mb - 1.4).abs() < 0.01, "{}", cost.mb);
        let cost = cost_between(&at(t, 0, 0, 0), &at(t, 2000, 260, 0), 16);
        assert!((cost.cpu - 0.8125).abs() < 0.001, "{}", cost.cpu);
    }

    #[test]
    fn odd_samples_never_give_impossible_values() {
        let t = Instant::now();
        assert_eq!(cost_between(&at(t, 0, 0, 0), &at(t, 0, 50, 0), 4).cpu, 0.0);
        assert_eq!(cost_between(&at(t, 0, 500, 0), &at(t, 1000, 100, 0), 4).cpu, 0.0);
        assert_eq!(cost_between(&at(t, 0, 0, 0), &at(t, 1000, 9000, 0), 1).cpu, 100.0);
        assert_eq!(cost_between(&at(t, 0, 0, 0), &at(t, 1000, 100, 0), 0).cpu, 0.0);
    }

    #[test]
    fn the_engine_lives_next_to_this_program() {
        let path = engine_path().unwrap();
        assert_eq!(path.file_name().unwrap(), ENGINE_EXE);
        assert_eq!(path.parent(), std::env::current_exe().unwrap().parent());
    }
}
