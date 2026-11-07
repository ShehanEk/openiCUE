# openiCUE Window Implementation Plan (Plan 2 of 3)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build `openicue.exe`, the admin window from the approved design. It picks an effect, previews it, shows its cost, and applies it to the RAM or starts and stops the background engine.

**Architecture:**
- **Testable logic in a library:** the window's logic lives in the `openicue` crate's library target, so it can be unit-tested without Windows or admin rights. That's `model.rs` (what to show), `preview.rs` (preview colors) and `layout.rs` (where everything goes, plus hit testing).
- **Windows code in the binary:** the binary holds `paint.rs` (GDI+ drawing), `window.rs` (a frameless Win32 window) and `actions.rs` (hardware and engine calls).
- **Shared engine logic in core:** engine control and cost storage go into `openicue-core`, because the engine, the window and later the installer share them.

**Tech Stack:**
- **Language and platform:** Rust 1.99 msvc, `windows-sys` 0.59 only, with no new crates.
- **Drawing:** GDI+ through its flat C API, with anti-aliasing and ClearType text.
- **Elevation:** a `requireAdministrator` manifest, embedded with linker arguments, so no resource-compiler crate is needed.

**Spec:** `docs/superpowers/specs/2026-11-05-openicue-design.md`. Also read `docs/roadmap.md` (Plan 2 inputs) and `docs/hardware-findings.md`. The design is https://claude.ai/artifact/BujCxKxotsdSKvibDfEHj4, and a local copy of the main screen is quoted where it's needed below.

## Global Constraints

- **No network code, no plugins, no web links or web pages** in any admin window.
- **No command-line options for `openicue.exe`:** any argument exits with code 2.
- **Bus writes go through `ram::*` only.** The window never calls `Bus` methods directly. Addresses stay limited to `0x58`–`0x5F` by `StickAddr`.
- **Write to the RAM only on an explicit Apply** (`docs/roadmap.md` Plan 2 input 1). Never write while picking colors or options.
- **One writer:** before writing, the window stops the engine and holds `system::claim_engine()` for as long as it writes.
- **Dependencies:** add only `windows-sys` features, and add no new crates. `Cargo.lock` must gain no packages other than our own `openicue`.
- **Colors** from the design: background `#0B0C0E`, text `#F2F4F5`, muted `#9BA3AB`, pill `#17191D`, pill text `#C9CED3`, ongoing-cost orange `#FFB066`, running dot on the selected tile `#B45A00`.
- **Window size:** 840 × 640 logical pixels, frameless, scaled by the monitor DPI.
- **Hardware:** subagents never run `openicue.exe`, `openicue-engine.exe` or `openicue-hwtest.exe`, and never install or download anything. The user runs the hardware stages.
- **Commits:** no AI attribution lines. Commits are GPG-signed: if signing times out, retry once, then stop and report.
- **Build:** run cargo from PowerShell. Git Bash has a different `link.exe` first on its PATH.

## Decisions in this plan (2026-11-07)

- **Drawing uses GDI+, not Direct2D.** GDI+ is reachable through `windows-sys`, which we already depend on. Direct2D needs the `windows` crate and its proc-macro dependencies.
- **CPU % is shown as a share of all logical processors**, like Task Manager. 13% of one core shows as about 0.8% on 16 threads.
- **Applying a built-in effect is synchronous.** The status line shows "Applying…" first. It takes about 0.5 s, or about 1 s for Static.
- **Window process detection:** the window runs `ram::detect` (read-only) at startup. It runs `read_info` only when it applies, after stopping the engine.
- **Native accessibility is out of scope.** The window supports keyboard Esc (close) and Enter (Apply) only.

---

### Task 1: Harden the PawnIO handle and split engine exit code 3

**Files:**
- Modify: `crates/core/src/pawnio.rs`, `crates/core/src/bus.rs`, `crates/engine/src/main.rs`, `docs/hardware-findings.md`

**Interfaces:**
- Produces: `BusError::ShortReply`, and engine exit code 9 for "couldn't create the engine lock".

- [ ] **Step 1: Write the failing tests**

Add to the `tests` module in `crates/core/src/pawnio.rs`:

```rust
    #[test]
    fn accepts_a_reply_of_exactly_the_requested_size() {
        assert_eq!(check_returned(8, 1), Ok(()));
        assert_eq!(check_returned(0, 0), Ok(()));
    }

    #[test]
    fn rejects_a_short_or_long_reply() {
        assert_eq!(check_returned(0, 1), Err(BusError::ShortReply));
        assert_eq!(check_returned(4, 1), Err(BusError::ShortReply));
        assert_eq!(check_returned(16, 1), Err(BusError::ShortReply));
    }
```

- [ ] **Step 2: Run them to confirm they fail**

Run: `cargo test -p openicue-core pawnio`
Expected: a compile error, because `check_returned` and `ShortReply` don't exist yet.

- [ ] **Step 3: Implement**

In `crates/core/src/bus.rs`, add a variant to `BusError`:

```rust
    /// The driver returned a different number of bytes than were asked for.
    ShortReply,
```

In `crates/core/src/pawnio.rs`, add these constants next to `GENERIC_READ_WRITE`:

```rust
/// Synchronous I/O, so a request can never complete after its buffers are gone.
const SYNCHRONIZE: u32 = 0x0010_0000;
const FILE_SYNCHRONOUS_IO_NONALERT: u32 = 0x0000_0020;
```

Change the `NtOpenFile` call in `open()` to:

```rust
            NtOpenFile(&mut device, GENERIC_READ_WRITE | SYNCHRONIZE, &attributes, &mut io_status,
                FILE_SHARE_ALL, FILE_SYNCHRONOUS_IO_NONALERT)
```

In `call()`, replace the final `Ok(())` with:

```rust
        check_returned(returned, output.len())
```

Add this function under `block_cells`:

```rust
/// The driver must fill exactly the cells we asked for, so a short reply
/// can never be read as zeros.
fn check_returned(returned: u32, cells: usize) -> Result<(), BusError> {
    if returned as usize == cells * 8 { Ok(()) } else { Err(BusError::ShortReply) }
}
```

In `crates/engine/src/main.rs`, replace the first line of `run()`:

```rust
    let _claim = match system::claim_engine() {
        Ok(Some(claim)) => claim,
        Ok(None) => return 3,
        Err(_) => return 9,
    };
```

Then edit the exit code list in the doc comment at the top of the file so it reads:

```rust
//! - 3: another engine is already running
```

Add this after the line for 8:

```rust
//! - 9: couldn't create the engine lock (usually not run as admin)
```

- [ ] **Step 4: Run all tests and build**

Run: `cargo test --workspace`
Expected: everything passes, including the 2 new tests.

Run: `cargo build --release --workspace`
Expected: success with no warnings.

- [ ] **Step 5: Commit**

```bash
git add crates
git commit -m "fix: open PawnIO synchronously, check reply size, split engine lock exit code"
```

- [ ] **Step 6: HARDWARE STAGE 7, the user runs it in an admin terminal**

This changes how every transfer is opened and checked, so repeat stage 1 and one effect:

```bash
.\target\release\openicue-hwtest.exe detect
```

```bash
.\target\release\openicue-hwtest.exe info
```

```bash
.\target\release\openicue-hwtest.exe effect rainbow-wave
```

Expected:
- `Found 4 sticks` at `0x58`–`0x5b`;
- 4 lines of product `0x0101`, protocol 4;
- `Ok(())` for all 4 sticks, and rainbow wave showing.

A `ShortReply` error means PawnIO returns a different byte count than expected. If you see it, stop and report the exact output. Don't loosen the check.

Then, in a **non-admin** PowerShell window:

```bash
$e = Start-Process .\target\release\openicue-engine.exe -ArgumentList '--restore' -PassThru -Wait; $e.ExitCode
```

Expected: `9` (it was `3` before this task).

- [ ] **Step 7: Record the result**

Append a dated "Stage 7" section to `docs/hardware-findings.md` with the outputs, then:

```bash
git add docs/hardware-findings.md
git commit -m "docs: record stage 7"
```

---

### Task 2: Store the last measured cost per custom effect

**Files:**
- Create: `crates/core/src/costs.rs`
- Modify: `crates/core/src/lib.rs`, `crates/core/src/system.rs`

**Interfaces:**
- Consumes: `Effect`, `ALL_EFFECTS`, plus `system::secure_data_dir`, `read_limited` and `os_error` (all private to `system.rs` except `secure_data_dir`).
- Produces:
  - `costs::Cost { pub mb: f32, pub cpu: f32 }` (Copy, PartialEq)
  - `costs::Costs` (Clone, Default, PartialEq) with `get(&self, Effect) -> Option<Cost>` and `set(&mut self, Effect, Cost)`
  - `costs::parse(&str) -> Option<Costs>`, `costs::serialize(&Costs) -> String`
  - `system::load_costs() -> Costs`, `system::save_costs(&Costs) -> Result<(), u32>`

- [ ] **Step 1: Write the failing tests**

Create `crates/core/src/costs.rs` with only the test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::effects::Effect;

    fn sample() -> Costs {
        let mut costs = Costs::default();
        costs.set(Effect::Breathing, Cost { mb: 1.4, cpu: 0.8 });
        costs.set(Effect::Chase, Cost { mb: 1.5, cpu: 0.6 });
        costs
    }

    #[test]
    fn round_trips() {
        let text = serialize(&sample());
        assert_eq!(text, "version=1\nbreathing=1.4,0.8\nchase=1.5,0.6\n");
        assert_eq!(parse(&text), Some(sample()));
    }

    #[test]
    fn set_replaces_and_ignores_built_in_or_invalid() {
        let mut costs = sample();
        costs.set(Effect::Breathing, Cost { mb: 2.0, cpu: 1.0 });
        assert_eq!(costs.get(Effect::Breathing), Some(Cost { mb: 2.0, cpu: 1.0 }));
        costs.set(Effect::Static, Cost { mb: 1.0, cpu: 1.0 });
        assert_eq!(costs.get(Effect::Static), None);
        costs.set(Effect::Gradient, Cost { mb: f32::NAN, cpu: 1.0 });
        costs.set(Effect::Gradient, Cost { mb: 1.0, cpu: 101.0 });
        assert_eq!(costs.get(Effect::Gradient), None);
    }

    #[test]
    fn rejects_bad_input() {
        let good = serialize(&sample());
        let cases = [
            String::new(),
            good.replace("version=1", "version=2"),
            good.replace("breathing", "static"),
            good.replace("breathing", "../evil"),
            good.replace("chase", "breathing"),
            good.replace("1.4", "-1.4"),
            good.replace("1.4", "inf"),
            good.replace("1.4", "NaN"),
            good.replace("1.4", "1.4.1"),
            good.replace("0.8", "100.1"),
            good.replace("1.4,0.8", "1.4"),
            good.replace("1.4", "123456789"),
            format!("{good}{}", "x".repeat(MAX_COSTS_BYTES)),
        ];
        for case in cases {
            assert_eq!(parse(&case), None, "accepted {case:?}");
        }
    }
}
```

Add `pub mod costs;` to `crates/core/src/lib.rs`.

- [ ] **Step 2: Run them to confirm they fail**

Run: `cargo test -p openicue-core costs`
Expected: a compile error, because `Costs`, `Cost`, `parse`, `serialize` and `MAX_COSTS_BYTES` don't exist yet.

- [ ] **Step 3: Implement**

Put this above the test module in `crates/core/src/costs.rs`:

```rust
//! The last measured cost of each custom effect, shown before the user applies it.
use crate::effects::{Effect, ALL_EFFECTS};

pub const MAX_COSTS_BYTES: usize = 256;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cost {
    /// Private memory in MB.
    pub mb: f32,
    /// Share of all logical processors, in percent, like Task Manager.
    pub cpu: f32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Costs {
    entries: Vec<(Effect, Cost)>,
}

impl Costs {
    pub fn get(&self, effect: Effect) -> Option<Cost> {
        self.entries.iter().find(|(e, _)| *e == effect).map(|(_, cost)| *cost)
    }

    /// Ignores built-in effects, which cost nothing once applied, and impossible values.
    pub fn set(&mut self, effect: Effect, cost: Cost) {
        if !effect.is_custom() || !valid(cost) {
            return;
        }
        match self.entries.iter_mut().find(|(e, _)| *e == effect) {
            Some(entry) => entry.1 = cost,
            None => self.entries.push((effect, cost)),
        }
    }
}

fn valid(cost: Cost) -> bool {
    (0.0..=4096.0).contains(&cost.mb) && (0.0..=100.0).contains(&cost.cpu)
}

/// Digits with at most one dot, so signs, "inf" and "NaN" are rejected.
fn number(text: &str) -> Option<f32> {
    let digits_and_dot = text.bytes().all(|b| b.is_ascii_digit() || b == b'.');
    let dots = text.bytes().filter(|&b| b == b'.').count();
    if text.is_empty() || text.len() > 8 || !digits_and_dot || dots > 1 {
        return None;
    }
    text.parse().ok()
}

/// Returns `None` for anything other than exactly the expected file.
pub fn parse(text: &str) -> Option<Costs> {
    if text.len() > MAX_COSTS_BYTES {
        return None;
    }
    let mut lines = text.lines();
    if lines.next()? != "version=1" {
        return None;
    }
    let mut costs = Costs::default();
    for line in lines {
        let (name, values) = line.split_once('=')?;
        let effect = Effect::from_name(name).filter(|e| e.is_custom())?;
        if costs.get(effect).is_some() {
            return None;
        }
        let (mb, cpu) = values.split_once(',')?;
        let cost = Cost { mb: number(mb)?, cpu: number(cpu)? };
        if !valid(cost) {
            return None;
        }
        costs.entries.push((effect, cost));
    }
    Some(costs)
}

pub fn serialize(costs: &Costs) -> String {
    let mut text = String::from("version=1\n");
    for effect in ALL_EFFECTS {
        if let Some(cost) = costs.get(effect) {
            text.push_str(&format!("{}={:.1},{:.1}\n", effect.name(), cost.mb, cost.cpu));
        }
    }
    text
}
```

`(0.0..=4096.0).contains(&NaN)` is false, so `valid` also rejects NaN.

In `crates/core/src/system.rs`, add `use crate::costs::{self, Costs};` and these functions after `save_settings`:

```rust
/// Any problem gives no stored costs, so the window shows "Not measured yet".
pub fn load_costs() -> Costs {
    let read = || -> Option<Costs> {
        let path = secure_data_dir().ok()?.join("costs.txt");
        costs::parse(&read_limited(std::fs::File::open(path).ok()?)?)
    };
    read().unwrap_or_default()
}

pub fn save_costs(costs: &Costs) -> Result<(), u32> {
    let path = secure_data_dir()?.join("costs.txt");
    std::fs::write(path, costs::serialize(costs)).map_err(os_error)
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test --workspace`
Expected: everything passes.

- [ ] **Step 5: Commit**

```bash
git add crates/core
git commit -m "feat: store last measured cost per custom effect"
```

---

### Task 3: Engine control for the window

**Files:**
- Create: `crates/core/src/control.rs`
- Modify: `crates/core/src/lib.rs`, `crates/core/Cargo.toml`

**Interfaces:**
- Consumes: `system::claim_engine`, `system::open_stop_event`, `StopEvent::{already_existed, signal}`, `costs::Cost`
- Produces:
  - `control::ENGINE_EXE: &str`, `control::engine_path() -> Result<PathBuf, u32>`
  - `control::engine_running() -> bool`
  - `control::stop_engine(timeout: Duration) -> Result<(), StopError>`, with `pub enum StopError { Event(u32), StillRunning }`
  - `control::start_engine() -> Result<(), u32>`
  - `control::Sample { pub private_bytes: u64, pub cpu_time_100ns: u64, pub at: Instant }` (Clone, Copy, Debug)
  - `control::sample_engine() -> Option<Sample>`
  - `control::cost_between(earlier: &Sample, later: &Sample, processors: usize) -> Cost`

- [ ] **Step 1: Write the failing tests**

Create `crates/core/src/control.rs` with only:

```rust
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
```

Add `pub mod control;` to `crates/core/src/lib.rs`.

- [ ] **Step 2: Run them to confirm they fail**

Run: `cargo test -p openicue-core control`
Expected: a compile error, because `Sample`, `cost_between`, `engine_path` and `ENGINE_EXE` don't exist yet.

- [ ] **Step 3: Add the windows-sys features**

In `crates/core/Cargo.toml`, add these to the `windows-sys` features list:

```toml
  "Win32_System_Diagnostics_ToolHelp",
  "Win32_System_ProcessStatus",
```

- [ ] **Step 4: Implement**

Put this above the test module in `crates/core/src/control.rs`. If a `windows-sys` 0.59 signature differs (a pointer's mutability, a `BOOL` type or a constant's type), adapt it minimally and say so in your report.

```rust
//! Starts, stops and measures the background engine for the window.
use crate::costs::Cost;
use crate::system;
use std::os::windows::ffi::OsStrExt;
use std::path::PathBuf;
use std::ptr::{null, null_mut};
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
```

`null_mut` is imported for the implementer's convenience. Remove it if it's unused.

- [ ] **Step 5: Run tests and build**

Run: `cargo test --workspace`, then `cargo build --release --workspace`
Expected: everything passes, with no warnings.

- [ ] **Step 6: Commit**

```bash
git add crates/core
git commit -m "feat: start, stop and measure the engine for the window"
```

---

### Task 4: Window crate and the view model

**Files:**
- Create: `crates/app/Cargo.toml`, `crates/app/build.rs`, `crates/app/src/lib.rs`, `crates/app/src/model.rs`, `crates/app/src/main.rs`
- Modify: `Cargo.toml` (root `members`)

**Interfaces:**
- Consumes: `Settings`, `Effect`, `Speed`, `Direction`, `Rgb`, `ALL_EFFECTS`, `costs::{Cost, Costs}`
- Produces (`openicue::model`):
  - `enum Screen { App, NoDriver, NoSticks }`
  - `struct Model { pub screen: Screen, pub sticks: usize, pub settings: Settings, pub running: Option<Effect>, pub costs: Costs, pub live: Option<Cost>, pub message: String }`
  - `Model::new(Settings) -> Model`
  - setters: `select(Effect)`, `set_speed(Speed)`, `set_direction(Direction)`, `toggle_random()`, `set_color(u8, Rgb)`
  - view: `show_colors`, `show_pickers`, `show_color2`, `show_random`, `show_speed`, `directions() -> &'static [Direction]`, `is_running_selected`, `cost_line() -> String`, `cost_body() -> &'static str`, `cost_is_ongoing`, `apply_label() -> &'static str`, `show_stop`, `status_line() -> String`, `error_title() -> &'static str`, `error_body() -> &'static str`
  - free functions: `display_name(Effect)`, `speed_name(Speed)`, `direction_name(Direction)`, `builtins() -> Vec<Effect>`, `customs() -> Vec<Effect>`, `SPEEDS: [Speed; 3]`

- [ ] **Step 1: Create the crate**

In the root `Cargo.toml`, change the `members` line to:

```toml
members = ["crates/core", "crates/hwtest", "crates/engine", "crates/app"]
```

Keep any members that already exist, in their current order, and add `"crates/app"` last.

`crates/app/Cargo.toml`:

```toml
[package]
name = "openicue"
version = "0.1.0"
edition = "2021"

[dependencies]
openicue-core = { path = "../core" }
windows-sys = { version = "0.59", features = [
  "Win32_Foundation",
  "Win32_Graphics_Dwm",
  "Win32_Graphics_Gdi",
  "Win32_Graphics_GdiPlus",
  "Win32_System_LibraryLoader",
  "Win32_UI_Controls_Dialogs",
  "Win32_UI_HiDpi",
  "Win32_UI_Input_KeyboardAndMouse",
  "Win32_UI_WindowsAndMessaging",
] }
```

`crates/app/build.rs`:

```rust
fn main() {
    // The window writes to the RAM, so Windows must always start it as admin.
    // Only the binary gets the manifest, so `cargo test` still runs unelevated.
    println!("cargo:rustc-link-arg-bins=/MANIFEST:EMBED");
    println!("cargo:rustc-link-arg-bins=/MANIFESTUAC:level='requireAdministrator' uiAccess='false'");
}
```

`crates/app/src/lib.rs`:

```rust
//! Window logic that needs no Windows calls, so it can be unit-tested.
pub mod model;
```

`crates/app/src/main.rs` (Task 7 replaces it):

```rust
#![windows_subsystem = "windows"]

fn main() {}
```

- [ ] **Step 2: Write the failing tests**

Create `crates/app/src/model.rs` with only:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use openicue_core::costs::{Cost, Costs};
    use openicue_core::effects::{Direction, Effect, Rgb, Settings, Speed};

    fn model(effect: Effect) -> Model {
        let mut m = Model::new(Settings::default());
        m.sticks = 4;
        m.select(effect);
        m
    }

    #[test]
    fn options_follow_the_effect() {
        let m = model(Effect::Static);
        assert!(m.show_colors() && m.show_pickers() && !m.show_color2());
        assert!(!m.show_random() && !m.show_speed() && m.directions().is_empty());

        let m = model(Effect::Rainbow);
        assert!(!m.show_colors() && m.show_speed());

        let mut m = model(Effect::ColorWave);
        assert!(m.show_color2() && m.show_random());
        assert_eq!(m.directions(), &[Direction::Down, Direction::Up, Direction::Left, Direction::Right]);
        m.toggle_random();
        assert!(m.show_colors() && !m.show_pickers(), "random hides the pickers");

        let m = model(Effect::Visor);
        assert_eq!(m.settings.direction, Direction::Vertical, "select keeps the direction valid");
    }

    #[test]
    fn setters_ignore_options_the_effect_lacks() {
        let mut m = model(Effect::Rainbow);
        m.toggle_random();
        assert!(!m.settings.random);
        m.set_direction(Direction::Vertical);
        assert_ne!(m.settings.direction, Direction::Vertical);
        m.set_color(3, Rgb(1, 2, 3));
        assert_eq!(m.settings.color1, Settings::default().color1);
        m.set_color(2, Rgb(1, 2, 3));
        m.set_speed(Speed::Fast);
        assert_eq!((m.settings.color2, m.settings.speed), (Rgb(1, 2, 3), Speed::Fast));
    }

    #[test]
    fn built_in_cost_is_zero_and_says_why() {
        let mut m = model(Effect::RainbowWave);
        assert_eq!(m.cost_line(), "0 MB · 0% CPU");
        assert!(!m.cost_is_ongoing());
        assert_eq!(m.cost_body(), "After you close this window. The RAM runs this itself.");
        m.running = Some(Effect::Chase);
        assert_eq!(m.cost_body(), "After you close this window. Applying stops the background process.");
    }

    #[test]
    fn custom_cost_comes_from_measurement() {
        let mut m = model(Effect::Chase);
        assert_eq!(m.cost_line(), "Not measured yet");
        assert_eq!(m.cost_body(), "Keeps running after you close this window. Apply it to see the real cost.");
        assert!(m.cost_is_ongoing());

        let mut costs = Costs::default();
        costs.set(Effect::Chase, Cost { mb: 1.4, cpu: 0.8 });
        m.costs = costs;
        assert_eq!(m.cost_line(), "1.4 MB · 0.8% CPU");
        assert_eq!(m.cost_body(), "Last run on this PC. Keeps running after you close this window.");

        m.running = Some(Effect::Chase);
        m.live = Some(Cost { mb: 1.5, cpu: 0.9 });
        assert_eq!(m.cost_line(), "1.5 MB · 0.9% CPU");
        assert_eq!(m.cost_body(), "Using this now, until you stop it or shut down.");
        assert_eq!(m.apply_label(), "Update");
        assert!(m.show_stop());

        m.live = None;
        assert_eq!(m.cost_line(), "Measuring…");
    }

    #[test]
    fn status_line_prefers_message_then_running() {
        let mut m = model(Effect::Static);
        assert_eq!(m.status_line(), "4 sticks · restores at logon");
        m.sticks = 1;
        assert_eq!(m.status_line(), "1 stick · restores at logon");
        m.running = Some(Effect::Gradient);
        assert_eq!(m.status_line(), "Background process running");
        m.message = "Applied. You can close this window".into();
        assert_eq!(m.status_line(), "Applied. You can close this window");
        m.select(Effect::Rain);
        assert_eq!(m.status_line(), "Background process running", "select clears the message");
        m.screen = Screen::NoDriver;
        assert_eq!(m.status_line(), "");
    }

    #[test]
    fn names_match_the_design() {
        let names: Vec<&str> = builtins().into_iter().map(display_name).collect();
        assert_eq!(names, ["Static", "Color shift", "Color pulse", "Rainbow wave", "Color wave",
            "Visor", "Rain", "Marquee", "Rainbow", "Sequential"]);
        let names: Vec<&str> = customs().into_iter().map(display_name).collect();
        assert_eq!(names, ["Breathing", "Chase", "Gradient flow"]);
        assert_eq!(speed_name(Speed::Medium), "Medium");
        assert_eq!(direction_name(Direction::Horizontal), "Horizontal");
    }

    #[test]
    fn error_screens_have_the_design_text() {
        let mut m = model(Effect::Static);
        m.screen = Screen::NoDriver;
        assert_eq!(m.error_title(), "PawnIO driver not found");
        m.screen = Screen::NoSticks;
        assert_eq!(m.error_title(), "No RAM lighting found");
        assert!(m.error_body().starts_with("No supported lighting controller answered"));
    }
}
```

Add `pub mod model;` to `lib.rs`. That's already done in Step 1.

- [ ] **Step 3: Run them to confirm they fail**

Run: `cargo test -p openicue`
Expected: a compile error, because `Model` and the rest don't exist yet.

- [ ] **Step 4: Implement**

Put this above the test module in `crates/app/src/model.rs`:

```rust
//! What the window shows. Pure logic, no Windows calls.
use openicue_core::costs::{Cost, Costs};
use openicue_core::effects::{Direction, Effect, Rgb, Settings, Speed, ALL_EFFECTS};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    App,
    NoDriver,
    NoSticks,
}

pub const SPEEDS: [Speed; 3] = [Speed::Slow, Speed::Medium, Speed::Fast];

pub fn display_name(effect: Effect) -> &'static str {
    match effect {
        Effect::Static => "Static",
        Effect::ColorShift => "Color shift",
        Effect::ColorPulse => "Color pulse",
        Effect::RainbowWave => "Rainbow wave",
        Effect::ColorWave => "Color wave",
        Effect::Visor => "Visor",
        Effect::Rain => "Rain",
        Effect::Marquee => "Marquee",
        Effect::Rainbow => "Rainbow",
        Effect::Sequential => "Sequential",
        Effect::Breathing => "Breathing",
        Effect::Chase => "Chase",
        Effect::Gradient => "Gradient flow",
    }
}

pub fn speed_name(speed: Speed) -> &'static str {
    match speed {
        Speed::Slow => "Slow",
        Speed::Medium => "Medium",
        Speed::Fast => "Fast",
    }
}

pub fn direction_name(direction: Direction) -> &'static str {
    match direction {
        Direction::Up => "Up",
        Direction::Down => "Down",
        Direction::Left => "Left",
        Direction::Right => "Right",
        Direction::Vertical => "Vertical",
        Direction::Horizontal => "Horizontal",
    }
}

pub fn builtins() -> Vec<Effect> {
    ALL_EFFECTS.into_iter().filter(|e| !e.is_custom()).collect()
}

pub fn customs() -> Vec<Effect> {
    ALL_EFFECTS.into_iter().filter(|e| e.is_custom()).collect()
}

fn format_cost(cost: Cost) -> String {
    format!("{:.1} MB · {:.1}% CPU", cost.mb, cost.cpu)
}

pub struct Model {
    pub screen: Screen,
    pub sticks: usize,
    /// The choice on screen. Always normalized, so it can be applied as is.
    pub settings: Settings,
    /// The custom effect the engine is running, if any.
    pub running: Option<Effect>,
    pub costs: Costs,
    /// The latest sample while the selected custom effect runs.
    pub live: Option<Cost>,
    /// Overrides the status line until the next selection.
    pub message: String,
}

impl Model {
    pub fn new(settings: Settings) -> Model {
        Model {
            screen: Screen::App,
            sticks: 0,
            settings: settings.normalized(),
            running: None,
            costs: Costs::default(),
            live: None,
            message: String::new(),
        }
    }

    pub fn select(&mut self, effect: Effect) {
        self.settings.effect = effect;
        self.settings = self.settings.normalized();
        self.message.clear();
    }

    pub fn set_speed(&mut self, speed: Speed) {
        self.settings.speed = speed;
    }

    pub fn set_direction(&mut self, direction: Direction) {
        if self.directions().contains(&direction) {
            self.settings.direction = direction;
        }
    }

    pub fn toggle_random(&mut self) {
        if self.settings.effect.allows_random() {
            self.settings.random = !self.settings.random;
        }
    }

    /// `index` is 1 or 2. Anything else is ignored.
    pub fn set_color(&mut self, index: u8, color: Rgb) {
        match index {
            1 => self.settings.color1 = color,
            2 => self.settings.color2 = color,
            _ => {}
        }
    }

    pub fn show_colors(&self) -> bool {
        self.settings.effect.color_count() > 0
    }

    pub fn show_pickers(&self) -> bool {
        self.show_colors() && !self.settings.random
    }

    pub fn show_color2(&self) -> bool {
        self.settings.effect.color_count() > 1
    }

    pub fn show_random(&self) -> bool {
        self.settings.effect.allows_random()
    }

    pub fn show_speed(&self) -> bool {
        self.settings.effect.has_speed()
    }

    pub fn directions(&self) -> &'static [Direction] {
        self.settings.effect.directions()
    }

    pub fn is_running_selected(&self) -> bool {
        self.running == Some(self.settings.effect)
    }

    pub fn cost_is_ongoing(&self) -> bool {
        self.settings.effect.is_custom()
    }

    pub fn cost_line(&self) -> String {
        let effect = self.settings.effect;
        if !effect.is_custom() {
            return "0 MB · 0% CPU".to_string();
        }
        if self.is_running_selected() {
            return self.live.map(format_cost).unwrap_or_else(|| "Measuring…".to_string());
        }
        self.costs.get(effect).map(format_cost).unwrap_or_else(|| "Not measured yet".to_string())
    }

    pub fn cost_body(&self) -> &'static str {
        let effect = self.settings.effect;
        if !effect.is_custom() {
            if self.running.is_some() {
                "After you close this window. Applying stops the background process."
            } else {
                "After you close this window. The RAM runs this itself."
            }
        } else if self.is_running_selected() {
            "Using this now, until you stop it or shut down."
        } else if self.costs.get(effect).is_some() {
            "Last run on this PC. Keeps running after you close this window."
        } else {
            "Keeps running after you close this window. Apply it to see the real cost."
        }
    }

    pub fn apply_label(&self) -> &'static str {
        if self.settings.effect.is_custom() && self.is_running_selected() { "Update" } else { "Apply" }
    }

    pub fn show_stop(&self) -> bool {
        self.running.is_some()
    }

    pub fn status_line(&self) -> String {
        if self.screen != Screen::App {
            return String::new();
        }
        if !self.message.is_empty() {
            return self.message.clone();
        }
        if self.running.is_some() {
            return "Background process running".to_string();
        }
        let noun = if self.sticks == 1 { "stick" } else { "sticks" };
        format!("{} {noun} · restores at logon", self.sticks)
    }

    pub fn error_title(&self) -> &'static str {
        match self.screen {
            Screen::NoDriver => "PawnIO driver not found",
            _ => "No RAM lighting found",
        }
    }

    pub fn error_body(&self) -> &'static str {
        match self.screen {
            Screen::NoDriver => "openiCUE needs this driver to reach the LEDs on your RAM. Run openiCUE Setup again to restore it, then check again.",
            _ => "No supported lighting controller answered, and nothing was written to your RAM. Close iCUE or other RGB apps, then check again.",
        }
    }
}
```

- [ ] **Step 5: Run tests and build**

Run: `cargo test --workspace`, then `cargo build --release --workspace`
Expected: everything passes. The release build makes `target\release\openicue.exe`, whose embedded manifest asks for admin. Don't run it.

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml Cargo.lock crates/app
git commit -m "feat: add window crate and view model"
```

---

### Task 5: Animated preview colors

**Files:**
- Create: `crates/app/src/preview.rs`
- Modify: `crates/app/src/lib.rs`

**Interfaces:**
- Consumes: `Settings`, `frames::{render, Frame, STICKS_IN_FRAME}`, `ram::LED_COUNT`
- Produces: `preview::colors(settings: &Settings, seconds: f32) -> Frame`

This ports the design's preview formulas so the window matches the canvas. Custom effects reuse the engine's own `frames::render`, so their preview is exact.

- [ ] **Step 1: Write the failing tests**

Create `crates/app/src/preview.rs` with only:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use openicue_core::effects::{Direction, Effect, Rgb, Settings};

    fn settings(effect: Effect) -> Settings {
        Settings { effect, ..Settings::default() }.normalized()
    }

    #[test]
    fn static_is_color1_everywhere() {
        let s = Settings { color1: Rgb(9, 8, 7), ..settings(Effect::Static) };
        assert!(colors(&s, 3.0).iter().flatten().all(|&c| c == Rgb(9, 8, 7)));
    }

    #[test]
    fn custom_effects_match_the_engine_exactly() {
        for effect in [Effect::Breathing, Effect::Chase, Effect::Gradient] {
            let s = settings(effect);
            assert_eq!(colors(&s, 2.5), openicue_core::frames::render(&s, 2.5));
        }
    }

    #[test]
    fn rainbow_ignores_the_chosen_colors() {
        let a = settings(Effect::Rainbow);
        let b = Settings { color1: Rgb(1, 1, 1), color2: Rgb(2, 2, 2), ..a };
        assert_eq!(colors(&a, 1.0), colors(&b, 1.0));
    }

    #[test]
    fn direction_changes_the_wave() {
        let up = Settings { direction: Direction::Up, ..settings(Effect::RainbowWave) };
        let down = Settings { direction: Direction::Down, ..up };
        assert_ne!(colors(&up, 1.0), colors(&down, 1.0));
    }

    #[test]
    fn random_replaces_the_chosen_colors() {
        let plain = settings(Effect::ColorShift);
        let random = Settings { random: true, ..plain };
        assert_ne!(colors(&plain, 1.0), colors(&random, 1.0));
    }

    #[test]
    fn marquee_mixes_lit_and_dark_leds() {
        let s = Settings { color1: Rgb(200, 0, 0), ..settings(Effect::Marquee) };
        let frame = colors(&s, 0.0);
        assert!(frame.iter().flatten().any(|&c| c == Rgb(200, 0, 0)));
        assert!(frame.iter().flatten().any(|&c| c == Rgb(20, 22, 25)));
    }

    #[test]
    fn every_built_in_animates_without_panicking() {
        for effect in openicue_core::effects::ALL_EFFECTS {
            for t in [0.0, 0.37, 10.0, 86_399.9] {
                let _ = colors(&settings(effect), t);
            }
        }
    }
}
```

Add `pub mod preview;` to `crates/app/src/lib.rs`.

- [ ] **Step 2: Run them to confirm they fail**

Run: `cargo test -p openicue preview`
Expected: a compile error, because `colors` doesn't exist yet.

- [ ] **Step 3: Implement**

Put this above the test module. Each arm mirrors the matching `case` in the design's `led()` function.

```rust
//! Preview colors for the 4 x 10 LED grid, ported from the design canvas.
use openicue_core::effects::{Direction, Effect, Rgb, Settings, Speed};
use openicue_core::frames::{self, Frame, STICKS_IN_FRAME};
use openicue_core::ram::LED_COUNT;
use std::f32::consts::PI;

type Color = [f32; 3];

/// An unlit LED in the design.
const OFF: Color = [20.0, 22.0, 25.0];

fn rgb(c: Rgb) -> Color {
    [c.0 as f32, c.1 as f32, c.2 as f32]
}

fn to_rgb(c: Color) -> Rgb {
    let channel = |x: f32| x.round().clamp(0.0, 255.0) as u8;
    Rgb(channel(c[0]), channel(c[1]), channel(c[2]))
}

fn mix(a: Color, b: Color, k: f32) -> Color {
    [a[0] + (b[0] - a[0]) * k, a[1] + (b[1] - a[1]) * k, a[2] + (b[2] - a[2]) * k]
}

/// Dims toward an unlit LED.
fn scale(a: Color, k: f32) -> Color {
    mix(OFF, a, k.clamp(0.0, 1.0))
}

fn modulo(x: f32, m: f32) -> f32 {
    x.rem_euclid(m)
}

/// The design's `hsl(h, 90%, 58%)`.
fn hsl(h: f32) -> Color {
    let h = modulo(h, 360.0) / 60.0;
    let (s, l) = (0.9, 0.58);
    let c = (1.0 - (2.0 * l - 1.0f32).abs()) * s;
    let x = c * (1.0 - (modulo(h, 2.0) - 1.0).abs());
    let (r, g, b) = match h as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    [(r + m) * 255.0, (g + m) * 255.0, (b + m) * 255.0]
}

/// The design's `rgbFromHue`, used for random colors.
fn random_color(h: f32) -> Color {
    let x = modulo(h, 360.0) / 60.0;
    let c = 230.0;
    let y = c * (1.0 - (modulo(x, 2.0) - 1.0).abs());
    let t = match x as u32 {
        0 => [c, y, 0.0],
        1 => [y, c, 0.0],
        2 => [0.0, c, y],
        3 => [0.0, y, c],
        4 => [y, 0.0, c],
        _ => [c, 0.0, y],
    };
    [t[0] + 20.0, t[1] + 20.0, t[2] + 20.0]
}

pub fn colors(settings: &Settings, seconds: f32) -> Frame {
    let s = settings.normalized();
    if s.effect.is_custom() {
        return frames::render(&s, seconds);
    }
    let speed = match s.speed {
        Speed::Slow => 0.6,
        Speed::Medium => 1.2,
        Speed::Fast => 2.4,
    };
    let has_directions = !s.effect.directions().is_empty();
    let sign = if has_directions && matches!(s.direction, Direction::Left | Direction::Up) { -1.0 } else { 1.0 };
    let vertical = has_directions
        && matches!(s.direction, Direction::Up | Direction::Down | Direction::Vertical);
    let p = if s.effect.has_speed() { speed * seconds * sign } else { 0.0 };
    let (a, b) = if s.random {
        (random_color(p * 40.0), random_color(p * 40.0 + 140.0))
    } else {
        (rgb(s.color1), rgb(s.color2))
    };
    let mut frame = [[Rgb(0, 0, 0); LED_COUNT]; STICKS_IN_FRAME];
    for (si, stick) in frame.iter_mut().enumerate() {
        for (i, led) in stick.iter_mut().enumerate() {
            *led = to_rgb(builtin_led(s.effect, si as f32, i as f32, p, a, b, vertical));
        }
    }
    frame
}

fn builtin_led(effect: Effect, s: f32, i: f32, p: f32, a: Color, b: Color, vertical: bool) -> Color {
    let pos = if vertical { s / 3.0 } else { i / 9.0 };
    let seed = modulo((i * 12.9 + s * 78.2).sin() * 43758.5, 1.0);
    match effect {
        Effect::ColorShift => mix(a, b, ((p * 2.0).sin() + 1.0) / 2.0),
        Effect::ColorPulse => {
            let second = modulo((p * 2.0 / PI).floor(), 2.0) != 0.0;
            scale(if second { b } else { a }, (p * 2.0).sin().abs())
        }
        Effect::RainbowWave => hsl(pos * 200.0 - p * 80.0),
        Effect::ColorWave => mix(a, b, ((2.0 * PI * (pos - p * 0.25)).sin() + 1.0) / 2.0),
        Effect::Visor => {
            let c = (p.sin() + 1.0) / 2.0;
            scale(mix(a, b, c), 1.0 - (pos - c).abs() * 4.0)
        }
        Effect::Rain => {
            let f = modulo(p * 0.5 + seed, 1.0);
            scale(mix(a, b, seed), if f < 0.25 { 1.0 - f * 4.0 } else { 0.0 })
        }
        Effect::Marquee => {
            if modulo(i + s * 2.0 + (p * 3.0).floor(), 3.0) == 0.0 { a } else { OFF }
        }
        Effect::Rainbow => hsl(p * 50.0),
        Effect::Sequential => {
            let k = modulo((p * 6.0).floor(), 80.0);
            let order = s * 10.0 + i;
            if k < 40.0 {
                if order <= k { a } else { OFF }
            } else if order > k - 40.0 {
                a
            } else {
                b
            }
        }
        // Static, plus custom effects (never reached: `colors` sends them to `frames`).
        _ => a,
    }
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p openicue`
Expected: everything passes.

- [ ] **Step 5: Commit**

```bash
git add crates/app
git commit -m "feat: add animated effect preview colors"
```

---

### Task 6: Layout and hit testing

**Files:**
- Create: `crates/app/src/layout.rs`
- Modify: `crates/app/src/lib.rs`

**Interfaces:**
- Consumes: `model::{Model, Screen, builtins, customs, SPEEDS, display_name, speed_name, direction_name}`
- Produces (`openicue::layout`), all in logical pixels:
  - constants `WIDTH = 840.0`, `HEIGHT = 640.0`, `TITLE_H = 44.0`
  - `struct Rect { pub x, pub y, pub w, pub h: f32 }` with `contains(x, y)` and `right()`
  - `enum Font { Brand, Status, Label, Body, BodyBold, Segment, Big, Small, ErrorBody }`
  - `enum Target { Close, Effect(Effect), Color(u8), Random, Speed(Speed), Direction(Direction), Stop, Apply, CheckAgain }`
  - `struct Button { pub rect: Rect, pub target: Target }`
  - `struct Layout` (fields below)
  - `layout(model: &Model, measure: &dyn Fn(&str, Font) -> f32) -> Layout`
  - `hit(layout: &Layout, x: f32, y: f32) -> Option<Target>`
  - `is_drag_area(layout: &Layout, x: f32, y: f32) -> bool`

Positions follow the design's CSS:
- 32 px side padding, a 44 px title bar, and 44 px pills.
- 6 px gaps between tiles, 20 px gaps between option groups, and 24 px bottom padding.

- [ ] **Step 1: Write the failing tests**

Create `crates/app/src/layout.rs` with only:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Model, Screen};
    use openicue_core::effects::{Effect, Settings};

    fn measure(text: &str, _: Font) -> f32 {
        text.chars().count() as f32 * 7.0
    }

    fn app(effect: Effect) -> Model {
        let mut m = Model::new(Settings::default());
        m.sticks = 4;
        m.select(effect);
        m
    }

    fn targets(l: &Layout) -> Vec<Target> {
        l.buttons.iter().map(|b| b.target).collect()
    }

    #[test]
    fn every_button_fits_the_window_and_none_overlap() {
        for effect in openicue_core::effects::ALL_EFFECTS {
            let mut m = app(effect);
            m.running = Some(Effect::Chase);
            let l = layout(&m, &measure);
            for (i, a) in l.buttons.iter().enumerate() {
                assert!(a.rect.x >= 0.0 && a.rect.right() <= WIDTH, "{:?}", a.target);
                assert!(a.rect.y >= 0.0 && a.rect.y + a.rect.h <= HEIGHT, "{:?}", a.target);
                for b in &l.buttons[i + 1..] {
                    let apart = a.rect.right() <= b.rect.x || b.rect.right() <= a.rect.x
                        || a.rect.y + a.rect.h <= b.rect.y || b.rect.y + b.rect.h <= a.rect.y;
                    assert!(apart, "{:?} overlaps {:?}", a.target, b.target);
                }
            }
        }
    }

    #[test]
    fn app_screen_has_all_thirteen_tiles_and_the_right_options() {
        let l = layout(&app(Effect::ColorWave), &measure);
        let t = targets(&l);
        let tiles = t.iter().filter(|t| matches!(t, Target::Effect(_))).count();
        assert_eq!(tiles, 13);
        assert!(t.contains(&Target::Color(1)) && t.contains(&Target::Color(2)) && t.contains(&Target::Random));
        assert_eq!(t.iter().filter(|t| matches!(t, Target::Speed(_))).count(), 3);
        assert_eq!(t.iter().filter(|t| matches!(t, Target::Direction(_))).count(), 4);
        assert!(t.contains(&Target::Apply) && !t.contains(&Target::Stop));
    }

    #[test]
    fn stop_appears_only_while_running() {
        let mut m = app(Effect::Static);
        assert!(!targets(&layout(&m, &measure)).contains(&Target::Stop));
        m.running = Some(Effect::Breathing);
        assert!(targets(&layout(&m, &measure)).contains(&Target::Stop));
    }

    #[test]
    fn wide_text_wraps_tiles_onto_more_rows() {
        let m = app(Effect::Static);
        let narrow = layout(&m, &measure);
        let wide = layout(&m, &|t: &str, _: Font| t.chars().count() as f32 * 14.0);
        let last = |l: &Layout| l.buttons.iter().filter(|b| matches!(b.target, Target::Effect(_))).last().unwrap().rect.y;
        assert!(last(&wide) > last(&narrow));
    }

    #[test]
    fn hit_and_drag_areas() {
        let l = layout(&app(Effect::Static), &measure);
        let apply = l.buttons.iter().find(|b| b.target == Target::Apply).unwrap().rect;
        assert_eq!(hit(&l, apply.x + 1.0, apply.y + 1.0), Some(Target::Apply));
        assert_eq!(hit(&l, WIDTH - 10.0, 10.0), Some(Target::Close));
        assert_eq!(hit(&l, 400.0, 600.0), None);
        assert!(is_drag_area(&l, 100.0, 20.0));
        assert!(!is_drag_area(&l, WIDTH - 10.0, 20.0));
        assert!(!is_drag_area(&l, 100.0, 60.0));
    }

    #[test]
    fn error_screens_offer_only_check_again_and_close() {
        let mut m = app(Effect::Static);
        m.screen = Screen::NoSticks;
        let l = layout(&m, &measure);
        assert_eq!(targets(&l), vec![Target::Close, Target::CheckAgain]);
        assert!(l.preview.is_none());
    }
}
```

Add `pub mod layout;` to `crates/app/src/lib.rs`.

- [ ] **Step 2: Run them to confirm they fail**

Run: `cargo test -p openicue layout`
Expected: a compile error, because `layout` and the related types don't exist yet.

- [ ] **Step 3: Implement**

Put this above the test module:

```rust
//! Where everything goes, in logical pixels (96 DPI). Pure, so it is tested.
use crate::model::{self, Model, Screen, SPEEDS};
use openicue_core::effects::{Direction, Effect, Speed};

pub const WIDTH: f32 = 840.0;
pub const HEIGHT: f32 = 640.0;
pub const TITLE_H: f32 = 44.0;
const SIDE: f32 = 32.0;
const CONTENT_RIGHT: f32 = WIDTH - SIDE;
const BOTTOM: f32 = HEIGHT - 24.0;
const PILL_H: f32 = 44.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && x < self.x + self.w && y >= self.y && y < self.y + self.h
    }
    pub fn right(&self) -> f32 {
        self.x + self.w
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Font {
    /// 13 px semibold, the window title.
    Brand,
    /// 12 px, the status line.
    Status,
    /// 12 px, section labels.
    Label,
    /// 14 px, tiles and buttons.
    Body,
    /// 14 px bold, Apply.
    BodyBold,
    /// 13 px, segmented options.
    Segment,
    /// 30 px light, the cost and error titles.
    Big,
    /// 13 px, the cost explanation.
    Small,
    /// 15 px, error explanations.
    ErrorBody,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Close,
    Effect(Effect),
    Color(u8),
    Random,
    Speed(Speed),
    Direction(Direction),
    Stop,
    Apply,
    CheckAgain,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Button {
    pub rect: Rect,
    pub target: Target,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Layout {
    pub close: Rect,
    /// The status line, right-aligned in this rect.
    pub status: Rect,
    /// The 4 x 10 LED grid, on the app screen only.
    pub preview: Option<Rect>,
    pub builtin_label: Option<Rect>,
    pub custom_label: Option<Rect>,
    /// Backgrounds of the speed and direction groups.
    pub segment_groups: Vec<Rect>,
    pub cost_line: Option<Rect>,
    pub cost_body: Option<Rect>,
    pub error_title: Option<Rect>,
    pub error_body: Option<Rect>,
    pub buttons: Vec<Button>,
}

/// Places pills left to right, wrapping at the content edge. Returns the bottom.
fn flow(buttons: &mut Vec<Button>, items: &[(Target, f32)], top: f32) -> f32 {
    let (mut x, mut y) = (SIDE, top);
    for &(target, w) in items {
        if x > SIDE && x + w > CONTENT_RIGHT {
            x = SIDE;
            y += PILL_H + 6.0;
        }
        buttons.push(Button { rect: Rect { x, y, w, h: PILL_H }, target });
        x += w + 6.0;
    }
    y + PILL_H
}

/// A segmented group: 2 px padding, 40 px buttons. Returns the group's width.
fn segments(buttons: &mut Vec<Button>, groups: &mut Vec<Rect>, items: &[(Target, f32)], x: f32, y: f32) -> f32 {
    let mut cx = x + 2.0;
    for &(target, w) in items {
        buttons.push(Button { rect: Rect { x: cx, y: y + 2.0, w, h: 40.0 }, target });
        cx += w;
    }
    let width = cx + 2.0 - x;
    groups.push(Rect { x, y, w: width, h: PILL_H });
    width
}

pub fn layout(model: &Model, measure: &dyn Fn(&str, Font) -> f32) -> Layout {
    let close = Rect { x: WIDTH - 48.0, y: 0.0, w: 48.0, h: TITLE_H };
    let mut out = Layout {
        close,
        status: Rect { x: 200.0, y: 0.0, w: close.x - 12.0 - 200.0, h: TITLE_H },
        preview: None,
        builtin_label: None,
        custom_label: None,
        segment_groups: Vec::new(),
        cost_line: None,
        cost_body: None,
        error_title: None,
        error_body: None,
        buttons: vec![Button { rect: close, target: Target::Close }],
    };
    if model.screen != Screen::App {
        // Title 40, gap 14, body up to 3 lines (64), gap 28, button 44; centred
        // in the area under the title bar, with the design's 48/92 padding.
        let top = TITLE_H + 48.0 + ((HEIGHT - TITLE_H - 48.0 - 92.0) - 190.0) / 2.0;
        out.error_title = Some(Rect { x: SIDE, y: top, w: WIDTH - 2.0 * SIDE, h: 40.0 });
        out.error_body = Some(Rect { x: (WIDTH - 440.0) / 2.0, y: top + 54.0, w: 440.0, h: 64.0 });
        let w = 48.0 + measure("Check again", Font::Body);
        let rect = Rect { x: (WIDTH - w) / 2.0, y: top + 146.0, w, h: PILL_H };
        out.buttons.push(Button { rect, target: Target::CheckAgain });
        return out;
    }

    let preview = Rect { x: SIDE, y: TITLE_H + 16.0, w: CONTENT_RIGHT - SIDE, h: 4.0 * 16.0 + 3.0 * 8.0 };
    out.preview = Some(preview);
    let mut y = preview.y + preview.h + 28.0;

    out.builtin_label = Some(Rect { x: SIDE, y, w: 300.0, h: 16.0 });
    let tiles: Vec<(Target, f32)> = model::builtins()
        .into_iter()
        .map(|e| (Target::Effect(e), 36.0 + measure(model::display_name(e), Font::Body)))
        .collect();
    y = flow(&mut out.buttons, &tiles, y + 24.0) + 16.0;

    out.custom_label = Some(Rect { x: SIDE, y, w: 300.0, h: 16.0 });
    let tiles: Vec<(Target, f32)> = model::customs()
        .into_iter()
        .map(|e| {
            let dot = if model.running == Some(e) { 15.0 } else { 0.0 };
            (Target::Effect(e), 36.0 + dot + measure(model::display_name(e), Font::Body))
        })
        .collect();
    y = flow(&mut out.buttons, &tiles, y + 24.0) + 20.0;

    // Options row.
    let mut x = SIDE;
    if model.show_colors() {
        let mut gap = 0.0;
        if model.show_pickers() {
            out.buttons.push(Button { rect: Rect { x, y, w: 44.0, h: PILL_H }, target: Target::Color(1) });
            x += 44.0;
            if model.show_color2() {
                x += 4.0;
                out.buttons.push(Button { rect: Rect { x, y, w: 44.0, h: PILL_H }, target: Target::Color(2) });
                x += 44.0;
            }
            gap = 4.0;
        }
        if model.show_random() {
            x += gap;
            let w = 8.0 + 15.0 + 8.0 + measure("Random", Font::Body) + 8.0;
            out.buttons.push(Button { rect: Rect { x, y, w, h: PILL_H }, target: Target::Random });
            x += w;
        }
        x += 20.0;
    }
    if model.show_speed() {
        let items: Vec<(Target, f32)> = SPEEDS
            .iter()
            .map(|&s| (Target::Speed(s), 28.0 + measure(model::speed_name(s), Font::Segment)))
            .collect();
        x += segments(&mut out.buttons, &mut out.segment_groups, &items, x, y) + 20.0;
    }
    if !model.directions().is_empty() {
        let items: Vec<(Target, f32)> = model
            .directions()
            .iter()
            .map(|&d| (Target::Direction(d), 24.0 + measure(model::direction_name(d), Font::Segment)))
            .collect();
        segments(&mut out.buttons, &mut out.segment_groups, &items, x, y);
    }

    // Bottom row: cost on the left, Stop and Apply on the right.
    let w = 64.0 + measure(model.apply_label(), Font::BodyBold);
    let apply = Rect { x: CONTENT_RIGHT - w, y: BOTTOM - PILL_H, w, h: PILL_H };
    let mut left_of_buttons = apply.x;
    if model.show_stop() {
        let w = 40.0 + measure("Stop", Font::Body);
        let stop = Rect { x: apply.x - 8.0 - w, y: apply.y, w, h: PILL_H };
        out.buttons.push(Button { rect: stop, target: Target::Stop });
        left_of_buttons = stop.x;
    }
    out.buttons.push(Button { rect: apply, target: Target::Apply });
    let cost_w = left_of_buttons - 24.0 - SIDE;
    out.cost_line = Some(Rect { x: SIDE, y: BOTTOM - 60.0, w: cost_w, h: 40.0 });
    out.cost_body = Some(Rect { x: SIDE, y: BOTTOM - 18.0, w: cost_w, h: 18.0 });
    out
}

pub fn hit(layout: &Layout, x: f32, y: f32) -> Option<Target> {
    layout.buttons.iter().find(|b| b.rect.contains(x, y)).map(|b| b.target)
}

/// The title bar, except the close button, moves the window.
pub fn is_drag_area(layout: &Layout, x: f32, y: f32) -> bool {
    y < TITLE_H && !layout.close.contains(x, y)
}
```

The `stop` rect is pushed before `apply`, so Tab order and hit tests stay simple. `Rect` and `Font` are `Copy`, so paint can reuse them freely.

- [ ] **Step 4: Run tests**

Run: `cargo test -p openicue`
Expected: everything passes. If `every_button_fits_the_window_and_none_overlap` fails for one effect, a width calculation above was changed. Fix the code, not the test.

- [ ] **Step 5: Commit**

```bash
git add crates/app
git commit -m "feat: add window layout and hit testing"
```

---

### Task 7: Draw the window (read-only)

**Files:**
- Create: `crates/app/src/paint.rs`, `crates/app/src/window.rs`, `crates/app/src/actions.rs`
- Modify: `crates/app/src/main.rs`

**Interfaces:**
- Consumes: `layout::*`, `model::*`, `preview::colors`, `system::{load_settings, load_costs}`, `control::engine_running`, `PawnIoBus::open`, `ram::detect`
- Produces (binary-private):
  - `paint::Painter::new() -> Painter`, `Painter::paint(&self, hdc: HDC, width_px: i32, height_px: i32, scale: f32, model: &Model, seconds: f32) -> Layout`
  - `actions::Hardware { bus: PawnIoBus }`, `actions::Startup { NoDriver, NoSticks, Ready(Hardware, usize) }`, `actions::startup() -> Startup`
  - `window::run() -> i32`

This task makes the window open, look like the design, animate the preview and respond to every option. Apply and Stop do nothing until Task 8. GDI+ code can't be unit-tested, so the user checks it visually in Step 6.

- [ ] **Step 1: Write `crates/app/src/actions.rs` (read-only startup)**

```rust
//! Calls that reach the hardware or the engine. The window logic stays in `model`.
use openicue_core::pawnio::PawnIoBus;
use openicue_core::ram;

pub struct Hardware {
    pub bus: PawnIoBus,
}

pub enum Startup {
    NoDriver,
    NoSticks,
    Ready(Hardware, usize),
}

/// Opens PawnIO and finds the sticks. Detection only reads ID registers, so
/// it's safe while an engine is animating.
pub fn startup() -> Startup {
    let Ok(mut bus) = PawnIoBus::open() else { return Startup::NoDriver };
    let count = ram::detect(&mut bus).len();
    if count == 0 { Startup::NoSticks } else { Startup::Ready(Hardware { bus }, count) }
}
```

- [ ] **Step 2: Write `crates/app/src/paint.rs`**

GDI+ through `windows_sys::Win32::Graphics::GdiPlus`. Every GDI+ object made here is freed in the same function, or in `Drop` for the long-lived fonts. The names below are as in `windows-sys` 0.59. If a constant's type or a pointer's mutability differs, adapt it minimally and list it in your report.

```rust
//! Draws the window with GDI+ into an off-screen bitmap, then copies it in one go.
use crate::window::rgb_wide;
use openicue::layout::{self, Font, Layout, Rect, Target};
use openicue::model::{self, Model, Screen};
use openicue::preview;
use openicue_core::effects::Rgb;
use std::ptr::{null, null_mut};
use windows_sys::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, SelectObject, HDC, SRCCOPY,
};
use windows_sys::Win32::Graphics::GdiPlus::*;

const BG: u32 = 0xFF0B0C0E;
const FG: u32 = 0xFFF2F4F5;
const MUTED: u32 = 0xFF9BA3AB;
const PILL: u32 = 0xFF17191D;
const PILL_TEXT: u32 = 0xFFC9CED3;
const ORANGE: u32 = 0xFFFFB066;
const DOT_ON_SELECTED: u32 = 0xFFB45A00;
const SWATCH_EDGE: u32 = 0xFF2A2D33;

fn argb(c: Rgb, alpha: u8) -> u32 {
    (u32::from(alpha) << 24) | (u32::from(c.0) << 16) | (u32::from(c.1) << 8) | u32::from(c.2)
}

fn rect_f(r: Rect) -> RectF {
    RectF { X: r.x, Y: r.y, Width: r.w, Height: r.h }
}

pub struct Painter {
    families: [*mut GpFontFamily; 3],
    fonts: [*mut GpFont; 9],
    format: *mut GpStringFormat,
}

/// Font index for each `Font`, matching the order `Painter::new` creates them.
fn index(font: Font) -> usize {
    match font {
        Font::Brand => 0,
        Font::Status => 1,
        Font::Label => 2,
        Font::Body => 3,
        Font::BodyBold => 4,
        Font::Segment => 5,
        Font::Big => 6,
        Font::Small => 7,
        Font::ErrorBody => 8,
    }
}

impl Painter {
    /// Call after `GdiplusStartup`.
    pub fn new() -> Painter {
        let family = |name: &str| {
            let mut f = null_mut();
            // SAFETY: NUL-terminated name, valid out pointer.
            unsafe { GdipCreateFontFamilyFromName(rgb_wide(name).as_ptr(), null_mut(), &mut f) };
            f
        };
        let regular = family("Segoe UI");
        let semibold = family("Segoe UI Semibold");
        let light = family("Segoe UI Light");
        let font = |fam: *mut GpFontFamily, size: f32, style: i32| {
            let mut f = null_mut();
            // SAFETY: `fam` came from GdipCreateFontFamilyFromName; valid out pointer.
            unsafe { GdipCreateFont(fam, size, style, UnitPixel, &mut f) };
            f
        };
        let fonts = [
            font(semibold, 13.0, FontStyleRegular),
            font(regular, 12.0, FontStyleRegular),
            font(regular, 12.0, FontStyleRegular),
            font(regular, 14.0, FontStyleRegular),
            font(regular, 14.0, FontStyleBold),
            font(regular, 13.0, FontStyleRegular),
            font(light, 30.0, FontStyleRegular),
            font(regular, 13.0, FontStyleRegular),
            font(regular, 15.0, FontStyleRegular),
        ];
        let mut typographic = null_mut();
        let mut format = null_mut();
        // SAFETY: valid out pointers; the clone is ours to delete.
        unsafe {
            GdipStringFormatGetGenericTypographic(&mut typographic);
            GdipCloneStringFormat(typographic, &mut format);
        }
        Painter { families: [regular, semibold, light], fonts, format }
    }

    fn measure(&self, graphics: *mut GpGraphics, text: &str, font: Font) -> f32 {
        let wide = rgb_wide(text);
        let mut bounds = RectF { X: 0.0, Y: 0.0, Width: 0.0, Height: 0.0 };
        let area = RectF { X: 0.0, Y: 0.0, Width: 10_000.0, Height: 100.0 };
        // SAFETY: all pointers are live GDI+ objects or valid locals.
        unsafe {
            GdipSetStringFormatAlign(self.format, StringAlignmentNear);
            GdipMeasureString(graphics, wide.as_ptr(), (wide.len() - 1) as i32, self.fonts[index(font)],
                &area, self.format, &mut bounds, null_mut(), null_mut());
        }
        bounds.Width
    }

    /// Text in `rect`. `align`: 0 near, 1 centre, 2 far. Always vertically centred.
    fn text(&self, g: *mut GpGraphics, text: &str, font: Font, rect: Rect, color: u32, align: i32) {
        let wide = rgb_wide(text);
        let mut brush = null_mut();
        // SAFETY: live GDI+ objects; the brush is deleted here.
        unsafe {
            GdipCreateSolidFill(color, &mut brush);
            GdipSetStringFormatAlign(self.format, align);
            GdipSetStringFormatLineAlign(self.format, StringAlignmentCenter);
            GdipDrawString(g, wide.as_ptr(), (wide.len() - 1) as i32, self.fonts[index(font)],
                &rect_f(rect), self.format, brush.cast());
            GdipDeleteBrush(brush.cast());
        }
    }

    fn rounded(&self, g: *mut GpGraphics, r: Rect, radius: f32, color: u32) {
        let d = (radius * 2.0).min(r.w).min(r.h);
        let mut path = null_mut();
        let mut brush = null_mut();
        // SAFETY: live GDI+ objects; path and brush are deleted here.
        unsafe {
            GdipCreatePath(FillModeAlternate, &mut path);
            GdipAddPathArc(path, r.x, r.y, d, d, 180.0, 90.0);
            GdipAddPathArc(path, r.x + r.w - d, r.y, d, d, 270.0, 90.0);
            GdipAddPathArc(path, r.x + r.w - d, r.y + r.h - d, d, d, 0.0, 90.0);
            GdipAddPathArc(path, r.x, r.y + r.h - d, d, d, 90.0, 90.0);
            GdipClosePathFigure(path);
            GdipCreateSolidFill(color, &mut brush);
            GdipFillPath(g, brush.cast(), path);
            GdipDeleteBrush(brush.cast());
            GdipDeletePath(path);
        }
    }

    /// A soft glow: an ellipse that fades from `color` at the centre to clear.
    fn glow(&self, g: *mut GpGraphics, r: Rect, color: Rgb) {
        let mut path = null_mut();
        let mut brush = null_mut();
        let mut edge = argb(color, 0);
        let mut count = 1;
        // SAFETY: live GDI+ objects; path and brush are deleted here.
        unsafe {
            GdipCreatePath(FillModeAlternate, &mut path);
            GdipAddPathEllipse(path, r.x - 16.0, r.y - 16.0, r.w + 32.0, r.h + 32.0);
            GdipCreatePathGradientFromPath(path, &mut brush);
            GdipSetPathGradientCenterColor(brush, argb(color, 0x55));
            GdipSetPathGradientSurroundColorsWithCount(brush, &mut edge, &mut count);
            GdipFillPath(g, brush.cast(), path);
            GdipDeleteBrush(brush.cast());
            GdipDeletePath(path);
        }
    }

    fn line(&self, g: *mut GpGraphics, color: u32, width: f32, points: &[(f32, f32)]) {
        let mut pen = null_mut();
        // SAFETY: live graphics; the pen is deleted here.
        unsafe {
            GdipCreatePen1(color, width, UnitWorld, &mut pen);
            for pair in points.windows(2) {
                GdipDrawLine(g, pen, pair[0].0, pair[0].1, pair[1].0, pair[1].1);
            }
            GdipDeletePen(pen);
        }
    }

    pub fn paint(&self, hdc: HDC, width: i32, height: i32, scale: f32, model: &Model, seconds: f32) -> Layout {
        // SAFETY: standard off-screen buffer; every object is released below.
        unsafe {
            let memory = CreateCompatibleDC(hdc);
            let bitmap = CreateCompatibleBitmap(hdc, width, height);
            let old = SelectObject(memory, bitmap);
            let mut g = null_mut();
            GdipCreateFromHDC(memory, &mut g);
            GdipSetSmoothingMode(g, SmoothingModeAntiAlias);
            GdipSetTextRenderingHint(g, TextRenderingHintClearTypeGridFit);
            GdipGraphicsClear(g, BG);
            GdipScaleWorldTransform(g, scale, scale, MatrixOrderPrepend);

            let layout = layout::layout(model, &|text, font| self.measure(g, text, font));
            self.draw(g, model, &layout, seconds);

            GdipDeleteGraphics(g);
            BitBlt(hdc, 0, 0, width, height, memory, 0, 0, SRCCOPY);
            SelectObject(memory, old);
            DeleteObject(bitmap);
            DeleteDC(memory);
            layout
        }
    }

    fn draw(&self, g: *mut GpGraphics, model: &Model, l: &Layout, seconds: f32) {
        self.text(g, "openiCUE", Font::Brand, Rect { x: 32.0, y: 0.0, w: 160.0, h: layout::TITLE_H }, FG, 0);
        self.text(g, &model.status_line(), Font::Status, l.status, MUTED, 2);
        let c = l.close;
        let (cx, cy) = (c.x + c.w / 2.0, c.y + c.h / 2.0);
        self.line(g, MUTED, 1.2, &[(cx - 5.5, cy - 5.5), (cx + 5.5, cy + 5.5)]);
        self.line(g, MUTED, 1.2, &[(cx + 5.5, cy - 5.5), (cx - 5.5, cy + 5.5)]);

        if model.screen != Screen::App {
            self.text(g, model.error_title(), Font::Big, l.error_title.unwrap(), FG, 1);
            if let Some(body) = l.error_body {
                let mut brush = null_mut();
                let wide = rgb_wide(model.error_body());
                // SAFETY: as in `text`, with word-wrapping (no line alignment change needed).
                unsafe {
                    GdipCreateSolidFill(MUTED, &mut brush);
                    GdipSetStringFormatAlign(self.format, StringAlignmentCenter);
                    GdipSetStringFormatLineAlign(self.format, StringAlignmentNear);
                    GdipDrawString(g, wide.as_ptr(), (wide.len() - 1) as i32, self.fonts[index(Font::ErrorBody)],
                        &rect_f(body), self.format, brush.cast());
                    GdipDeleteBrush(brush.cast());
                }
            }
            for b in &l.buttons {
                if b.target == Target::CheckAgain {
                    self.rounded(g, b.rect, 22.0, PILL);
                    self.text(g, "Check again", Font::Body, b.rect, FG, 1);
                }
            }
            return;
        }

        if let Some(p) = l.preview {
            let frame = preview::colors(&model.settings, seconds);
            let col_w = (p.w - 9.0 * 2.0) / 10.0;
            let led = |stick: usize, i: usize| Rect {
                x: p.x + i as f32 * (col_w + 2.0),
                y: p.y + stick as f32 * 24.0,
                w: col_w,
                h: 16.0,
            };
            for (s, row) in frame.iter().enumerate() {
                for (i, &color) in row.iter().enumerate() {
                    self.glow(g, led(s, i), color);
                }
            }
            for (s, row) in frame.iter().enumerate() {
                for (i, &color) in row.iter().enumerate() {
                    self.rounded(g, led(s, i), 2.0, argb(color, 0xFF));
                }
            }
        }
        if let Some(r) = l.builtin_label {
            self.text(g, "On the RAM", Font::Label, r, MUTED, 0);
        }
        if let Some(r) = l.custom_label {
            self.text(g, "In the background", Font::Label, r, ORANGE, 0);
        }
        for group in &l.segment_groups {
            self.rounded(g, *group, 22.0, PILL);
        }
        for b in &l.buttons {
            self.button(g, model, b.rect, b.target);
        }
        let ongoing = model.cost_is_ongoing();
        if let Some(r) = l.cost_line {
            self.text(g, &model.cost_line(), Font::Big, r, if ongoing { ORANGE } else { FG }, 0);
        }
        if let Some(r) = l.cost_body {
            self.text(g, model.cost_body(), Font::Small, r, MUTED, 0);
        }
    }

    fn pill(&self, g: *mut GpGraphics, r: Rect, label: &str, font: Font, on: bool) {
        self.rounded(g, r, r.h / 2.0, if on { FG } else { PILL });
        self.text(g, label, font, r, if on { BG } else { PILL_TEXT }, 1);
    }

    fn button(&self, g: *mut GpGraphics, model: &Model, r: Rect, target: Target) {
        let s = &model.settings;
        match target {
            Target::Close => {}
            Target::Effect(e) => {
                let on = s.effect == e;
                self.rounded(g, r, 22.0, if on { FG } else { PILL });
                let mut text = r;
                if model.running == Some(e) {
                    let dot = Rect { x: r.x + 18.0, y: r.y + r.h / 2.0 - 3.5, w: 7.0, h: 7.0 };
                    self.rounded(g, dot, 3.5, if on { DOT_ON_SELECTED } else { ORANGE });
                    text = Rect { x: r.x + 15.0, ..r };
                }
                self.text(g, model::display_name(e), Font::Body, text, if on { BG } else { PILL_TEXT }, 1);
            }
            Target::Color(n) => {
                let color = if n == 1 { s.color1 } else { s.color2 };
                let swatch = Rect { x: r.x + 8.0, y: r.y + 8.0, w: 28.0, h: 28.0 };
                self.rounded(g, swatch, 6.0, SWATCH_EDGE);
                let inner = Rect { x: swatch.x + 1.0, y: swatch.y + 1.0, w: 26.0, h: 26.0 };
                self.rounded(g, inner, 5.0, argb(color, 0xFF));
            }
            Target::Random => {
                let bx = Rect { x: r.x + 8.0, y: r.y + r.h / 2.0 - 7.5, w: 15.0, h: 15.0 };
                if s.random {
                    self.rounded(g, bx, 3.0, FG);
                    self.line(g, BG, 1.6, &[(bx.x + 3.5, bx.y + 7.5), (bx.x + 6.5, bx.y + 10.5), (bx.x + 11.5, bx.y + 4.5)]);
                } else {
                    self.rounded(g, bx, 3.0, PILL_TEXT);
                    let hole = Rect { x: bx.x + 1.0, y: bx.y + 1.0, w: 13.0, h: 13.0 };
                    self.rounded(g, hole, 2.0, BG);
                }
                let label = Rect { x: bx.right() + 8.0, y: r.y, w: r.right() - bx.right() - 8.0, h: r.h };
                self.text(g, "Random", Font::Body, label, PILL_TEXT, 0);
            }
            Target::Speed(v) => {
                if s.speed == v { self.rounded(g, r, 20.0, FG); }
                self.text(g, model::speed_name(v), Font::Segment, r, if s.speed == v { BG } else { PILL_TEXT }, 1);
            }
            Target::Direction(d) => {
                if s.direction == d { self.rounded(g, r, 20.0, FG); }
                self.text(g, model::direction_name(d), Font::Segment, r, if s.direction == d { BG } else { PILL_TEXT }, 1);
            }
            Target::Stop => {
                self.rounded(g, r, 22.0, PILL);
                self.text(g, "Stop", Font::Body, r, ORANGE, 1);
            }
            Target::Apply => {
                self.rounded(g, r, 22.0, FG);
                self.text(g, model.apply_label(), Font::BodyBold, r, BG, 1);
            }
            Target::CheckAgain => self.pill(g, r, "Check again", Font::Body, false),
        }
    }
}

impl Drop for Painter {
    fn drop(&mut self) {
        // SAFETY: each object was created in `new` and is deleted once.
        unsafe {
            for font in self.fonts {
                GdipDeleteFont(font);
            }
            for family in self.families {
                GdipDeleteFontFamily(family);
            }
            GdipDeleteStringFormat(self.format);
        }
    }
}
```

The typographic string format is cloned, so changing its alignment never touches the shared GDI+ instance.

- [ ] **Step 3: Write `crates/app/src/window.rs`**

```rust
//! The frameless window: messages, DPI, timers and clicks.
use crate::actions::{self, Startup};
use crate::paint::Painter;
use openicue::layout::{self, Layout, Target};
use openicue::model::{Model, Screen};
use openicue_core::effects::{Effect, Rgb};
use openicue_core::{control, system};
use std::ptr::{null, null_mut};
use std::time::Instant;
use windows_sys::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Dwm::DwmSetWindowAttribute;
use windows_sys::Win32::Graphics::Gdi::{BeginPaint, EndPaint, InvalidateRect, UpdateWindow, PAINTSTRUCT};
use windows_sys::Win32::Graphics::GdiPlus::{GdiplusShutdown, GdiplusStartup, GdiplusStartupInput};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Controls::Dialogs::{ChooseColorW, CC_FULLOPEN, CC_RGBINIT, CHOOSECOLORW};
use windows_sys::Win32::UI::HiDpi::{GetDpiForWindow, SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{VK_ESCAPE, VK_RETURN};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

const TIMER_PREVIEW: usize = 1;
const TIMER_COST: usize = 2;
const PREVIEW_MS: u32 = 90;
const COST_MS: u32 = 2000;
const DWMWA_USE_IMMERSIVE_DARK_MODE: u32 = 20;
const DWMWA_WINDOW_CORNER_PREFERENCE: u32 = 33;
const DWMWCP_ROUND: u32 = 2;

pub fn rgb_wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

pub struct App {
    pub model: Model,
    pub hardware: Option<actions::Hardware>,
    pub painter: Painter,
    pub layout: Option<Layout>,
    pub start: Instant,
    pub scale: f32,
}

impl App {
    fn seconds(&self) -> f32 {
        (self.start.elapsed().as_secs_f64() % 86_400.0) as f32
    }

    /// Opens the driver and finds sticks. Used at start and by "Check again".
    pub fn check(&mut self) {
        self.hardware = None;
        match actions::startup() {
            Startup::NoDriver => self.model.screen = Screen::NoDriver,
            Startup::NoSticks => self.model.screen = Screen::NoSticks,
            Startup::Ready(hardware, count) => {
                self.model.screen = Screen::App;
                self.model.sticks = count;
                self.hardware = Some(hardware);
            }
        }
        let effect = self.model.settings.effect;
        self.model.running = (control::engine_running() && effect.is_custom()).then_some(effect);
    }
}

/// The App lives in GWLP_USERDATA from WM_NCCREATE until WM_NCDESTROY.
/// Borrow it only briefly: never across a call that runs a message loop.
fn app<'a>(hwnd: HWND) -> Option<&'a mut App> {
    // SAFETY: the pointer is either null or the leaked Box set in WM_NCCREATE.
    unsafe { (GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut App).as_mut() }
}

fn invalidate(hwnd: HWND) {
    // SAFETY: plain call on our own window.
    unsafe { InvalidateRect(hwnd, null(), 0) };
}

fn to_colorref(c: Rgb) -> COLORREF {
    u32::from(c.0) | (u32::from(c.1) << 8) | (u32::from(c.2) << 16)
}

fn from_colorref(c: COLORREF) -> Rgb {
    Rgb(c as u8, (c >> 8) as u8, (c >> 16) as u8)
}

/// Opens the standard color dialog. Returns `None` if the user cancels.
fn pick_color(hwnd: HWND, current: Rgb) -> Option<Rgb> {
    let mut custom = [0u32; 16];
    // SAFETY: zeroed is valid; every pointer field is set before the call.
    let mut dialog: CHOOSECOLORW = unsafe { std::mem::zeroed() };
    dialog.lStructSize = std::mem::size_of::<CHOOSECOLORW>() as u32;
    dialog.hwndOwner = hwnd;
    dialog.rgbResult = to_colorref(current);
    dialog.lpCustColors = custom.as_mut_ptr();
    dialog.Flags = CC_FULLOPEN | CC_RGBINIT;
    // SAFETY: `dialog` and `custom` outlive the modal call.
    (unsafe { ChooseColorW(&mut dialog) } != 0).then(|| from_colorref(dialog.rgbResult))
}

fn click(hwnd: HWND, target: Target) {
    match target {
        Target::Close => {
            // SAFETY: plain call on our own window.
            unsafe { DestroyWindow(hwnd) };
            return;
        }
        Target::Color(n) => {
            let Some(current) = app(hwnd).map(|a| if n == 1 { a.model.settings.color1 } else { a.model.settings.color2 }) else { return };
            // The dialog runs its own message loop, so no App borrow is held here.
            if let Some(color) = pick_color(hwnd, current) {
                if let Some(a) = app(hwnd) {
                    a.model.set_color(n, color);
                }
            }
        }
        Target::CheckAgain => {
            if let Some(a) = app(hwnd) {
                a.check();
            }
        }
        // Task 8 wires these to the hardware.
        Target::Apply | Target::Stop => {}
        other => {
            let Some(a) = app(hwnd) else { return };
            match other {
                Target::Effect(e) => a.model.select(e),
                Target::Speed(s) => a.model.set_speed(s),
                Target::Direction(d) => a.model.set_direction(d),
                Target::Random => a.model.toggle_random(),
                _ => {}
            }
        }
    }
    invalidate(hwnd);
}

fn logical(hwnd: HWND, lparam: LPARAM) -> (f32, f32) {
    let x = (lparam & 0xFFFF) as i16 as f32;
    let y = ((lparam >> 16) & 0xFFFF) as i16 as f32;
    let scale = app(hwnd).map_or(1.0, |a| a.scale);
    (x / scale, y / scale)
}

unsafe extern "system" fn window_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_NCCREATE => {
            let create = &*(lparam as *const CREATESTRUCTW);
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lpCreateParams as isize);
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_NCHITTEST => {
            let hit = DefWindowProcW(hwnd, msg, wparam, lparam);
            if hit != HTCLIENT as LRESULT {
                return hit;
            }
            let mut point = windows_sys::Win32::Foundation::POINT {
                x: (lparam & 0xFFFF) as i16 as i32,
                y: ((lparam >> 16) & 0xFFFF) as i16 as i32,
            };
            windows_sys::Win32::Graphics::Gdi::ScreenToClient(hwnd, &mut point);
            let Some(a) = app(hwnd) else { return hit };
            let (x, y) = (point.x as f32 / a.scale, point.y as f32 / a.scale);
            match &a.layout {
                Some(l) if layout::is_drag_area(l, x, y) => HTCAPTION as LRESULT,
                _ => hit,
            }
        }
        WM_PAINT => {
            let mut ps: PAINTSTRUCT = std::mem::zeroed();
            let hdc = BeginPaint(hwnd, &mut ps);
            if let Some(a) = app(hwnd) {
                let mut rect: RECT = std::mem::zeroed();
                GetClientRect(hwnd, &mut rect);
                let seconds = a.seconds();
                a.layout = Some(a.painter.paint(hdc, rect.right, rect.bottom, a.scale, &a.model, seconds));
            }
            EndPaint(hwnd, &ps);
            0
        }
        WM_ERASEBKGND => 1,
        WM_TIMER => {
            if wparam == TIMER_PREVIEW {
                invalidate(hwnd);
            }
            0
        }
        WM_LBUTTONUP => {
            let (x, y) = logical(hwnd, lparam);
            let target = app(hwnd).and_then(|a| a.layout.as_ref().and_then(|l| layout::hit(l, x, y)));
            if let Some(target) = target {
                click(hwnd, target);
            }
            0
        }
        WM_KEYDOWN => {
            if wparam == VK_ESCAPE as usize {
                DestroyWindow(hwnd);
            } else if wparam == VK_RETURN as usize
                && app(hwnd).is_some_and(|a| a.model.screen == Screen::App)
            {
                click(hwnd, Target::Apply);
            }
            0
        }
        WM_DPICHANGED => {
            if let Some(a) = app(hwnd) {
                a.scale = (wparam & 0xFFFF) as f32 / 96.0;
            }
            let r = &*(lparam as *const RECT);
            SetWindowPos(hwnd, null_mut(), r.left, r.top, r.right - r.left, r.bottom - r.top, SWP_NOZORDER | SWP_NOACTIVATE);
            0
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            0
        }
        WM_NCDESTROY => {
            let raw = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut App;
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            if !raw.is_null() {
                drop(Box::from_raw(raw));
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

pub fn run() -> i32 {
    // SAFETY: all calls below are standard window setup; pointers are to
    // live locals or statics, and every handle belongs to this process.
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let mut token = 0usize;
        let input = GdiplusStartupInput { GdiplusVersion: 1, DebugEventCallback: 0, SuppressBackgroundThread: 0, SuppressExternalCodecs: 0 };
        if GdiplusStartup(&mut token, &input, null_mut()) != 0 {
            return 1;
        }

        let instance = GetModuleHandleW(null());
        let class = rgb_wide("openiCUE.Window");
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(window_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: instance,
            hIcon: null_mut(),
            hCursor: LoadCursorW(null_mut(), IDC_ARROW),
            hbrBackground: null_mut(),
            lpszMenuName: null(),
            lpszClassName: class.as_ptr(),
            hIconSm: null_mut(),
        };
        RegisterClassExW(&wc);

        let mut model = Model::new(system::load_settings());
        model.costs = system::load_costs();
        let app = Box::new(App {
            model,
            hardware: None,
            painter: crate::paint::Painter::new(),
            layout: None,
            start: Instant::now(),
            scale: 1.0,
        });
        let raw = Box::into_raw(app);
        (*raw).check();

        let title = rgb_wide("openiCUE");
        let hwnd = CreateWindowExW(WS_EX_APPWINDOW, class.as_ptr(), title.as_ptr(),
            WS_POPUP | WS_SYSMENU | WS_MINIMIZEBOX, 0, 0, layout::WIDTH as i32, layout::HEIGHT as i32,
            null_mut(), null_mut(), instance, raw.cast());
        if hwnd.is_null() {
            drop(Box::from_raw(raw));
            GdiplusShutdown(token);
            return 1;
        }

        let scale = GetDpiForWindow(hwnd) as f32 / 96.0;
        (*raw).scale = scale;
        let (w, h) = ((layout::WIDTH * scale) as i32, (layout::HEIGHT * scale) as i32);
        let (sw, sh) = (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN));
        SetWindowPos(hwnd, null_mut(), (sw - w) / 2, (sh - h) / 2, w, h, SWP_NOZORDER);

        let on: u32 = 1;
        DwmSetWindowAttribute(hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE, (&on as *const u32).cast(), 4);
        DwmSetWindowAttribute(hwnd, DWMWA_WINDOW_CORNER_PREFERENCE, (&DWMWCP_ROUND as *const u32).cast(), 4);

        SetTimer(hwnd, TIMER_PREVIEW, PREVIEW_MS, None);
        ShowWindow(hwnd, SW_SHOW);
        UpdateWindow(hwnd);

        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        GdiplusShutdown(token);
        0
    }
}
```

`TIMER_COST`, `COST_MS`, `Effect` and `UpdateWindow` are used in Task 8. Add `#[allow(dead_code)]` only to `TIMER_COST` and `COST_MS`, and remove `Effect` from the imports if the compiler warns. Task 8 adds it back.

- [ ] **Step 4: Replace `crates/app/src/main.rs`**

```rust
#![windows_subsystem = "windows"]
mod actions;
mod paint;
mod window;

fn main() {
    // The window takes no options, so anything else is refused.
    if std::env::args().len() > 1 {
        std::process::exit(2);
    }
    std::process::exit(window::run());
}
```

- [ ] **Step 5: Build and test**

Run: `cargo test --workspace`, then `cargo build --release --workspace`
Expected: everything passes, and the build succeeds with no warnings other than the ones Step 3 allows.

Commit:

```bash
git add crates/app
git commit -m "feat: draw the window with GDI+ (read-only)"
```

- [ ] **Step 6: VISUAL CHECK, the user runs it**

Ask the user to:
1. Double-click `D:\Personal Projects\openiCUE\target\release\openicue.exe` and accept the admin prompt.
2. Compare it side by side with the design canvas, using the "Main" artboard.
3. Click several effects, speeds, directions, Random and a color swatch. Check that the preview animates and the options change.
4. Try to drag the window by its title area, then close it with ✕ or Esc.

Apply and Stop deliberately do nothing yet. Collect any visual differences as a list, then fix the clear mismatches in `paint.rs`. Use the design's values; don't redesign. Record the remaining gaps in `docs/roadmap.md`.

---

### Task 8: Apply, Stop and live cost

**Files:**
- Modify: `crates/app/src/actions.rs`, `crates/app/src/window.rs`, `docs/hardware-findings.md`

**Interfaces:**
- Consumes: `control::{stop_engine, start_engine, sample_engine, cost_between, Sample}`, `system::{claim_engine, save_settings, save_costs}`, `ram::{detect, read_info, apply_builtin}`
- Produces:
  - `Hardware::apply_builtin(&mut self, &Settings) -> Result<(), String>`
  - `actions::apply_custom(&Settings) -> Result<(), String>`
  - `actions::stop() -> Result<(), String>`

- [ ] **Step 1: Add the write actions to `crates/app/src/actions.rs`**

```rust
use openicue_core::control;
use openicue_core::effects::Settings;
use openicue_core::system;
use std::time::Duration;

const STOP_TIMEOUT: Duration = Duration::from_secs(2);
const BUSY: &str = "Couldn't reach the RAM. Close other RGB apps and try again.";

fn stop_first() -> Result<(), String> {
    control::stop_engine(STOP_TIMEOUT).map_err(|_| "The background process didn't stop. Try again.".to_string())
}

impl Hardware {
    /// Stops any engine, then writes the effect to every stick while holding
    /// the engine claim, so no other openiCUE process writes at the same time.
    pub fn apply_builtin(&mut self, settings: &Settings) -> Result<(), String> {
        stop_first()?;
        let Ok(Some(_claim)) = system::claim_engine() else { return Err(BUSY.to_string()) };
        let sticks = ram::detect(&mut self.bus);
        if sticks.is_empty() {
            return Err(BUSY.to_string());
        }
        let failed = sticks
            .iter()
            .filter(|&&addr| {
                ram::read_info(&mut self.bus, addr)
                    .and_then(|info| ram::apply_builtin(&mut self.bus, &info, settings))
                    .is_err()
            })
            .count();
        if failed > 0 {
            return Err(format!("{failed} of {} sticks didn't take it. Close other RGB apps and try again.", sticks.len()));
        }
        system::save_settings(settings).map_err(|_| "Applied, but couldn't save it for next time.".to_string())
    }
}

/// Saves the effect, then starts a fresh engine to run it.
pub fn apply_custom(settings: &Settings) -> Result<(), String> {
    stop_first()?;
    system::save_settings(settings).map_err(|_| "Couldn't save the effect.".to_string())?;
    control::start_engine().map_err(|_| "Couldn't start the background process.".to_string())
}

pub fn stop() -> Result<(), String> {
    stop_first()
}
```

- [ ] **Step 2: Wire them in `crates/app/src/window.rs`**

Add these fields to `App`:

```rust
    pub last_sample: Option<control::Sample>,
```

Initialize `last_sample: None` in `run()`. After `(*raw).check();`, start the cost timer if an effect is running. The timer needs the window, so do it after `CreateWindowExW` succeeds:

```rust
        if (*raw).model.running.is_some() {
            SetTimer(hwnd, TIMER_COST, COST_MS, None);
        }
```

Remove the `#[allow(dead_code)]` lines from Task 7, and add these functions:

```rust
/// Shows `message` at once, before a call that blocks for up to a second.
fn show_now(hwnd: HWND, message: &str) {
    if let Some(a) = app(hwnd) {
        a.model.message = message.to_string();
    }
    invalidate(hwnd);
    // SAFETY: plain call on our own window; paints synchronously.
    unsafe { UpdateWindow(hwnd) };
}

fn apply(hwnd: HWND) {
    let Some(a) = app(hwnd) else { return };
    let settings = a.model.settings;
    if settings.effect.is_custom() {
        let result = actions::apply_custom(&settings);
        let a = app(hwnd).unwrap();
        match result {
            Ok(()) => {
                a.model.running = Some(settings.effect);
                a.model.live = None;
                a.last_sample = None;
                a.model.message = "Running in the background".to_string();
                // SAFETY: plain call on our own window.
                unsafe { SetTimer(hwnd, TIMER_COST, COST_MS, None) };
            }
            Err(e) => a.model.message = e,
        }
        return;
    }
    show_now(hwnd, "Applying…");
    let a = app(hwnd).unwrap();
    let Some(hardware) = a.hardware.as_mut() else { return };
    let result = hardware.apply_builtin(&settings);
    match result {
        Ok(()) => {
            a.model.running = None;
            a.model.live = None;
            a.model.message = "Applied. You can close this window".to_string();
            // SAFETY: plain call on our own window.
            unsafe { KillTimer(hwnd, TIMER_COST) };
        }
        Err(e) => a.model.message = e,
    }
}

fn stop(hwnd: HWND) {
    let result = actions::stop();
    let Some(a) = app(hwnd) else { return };
    match result {
        Ok(()) => {
            a.model.running = None;
            a.model.live = None;
            a.model.message = "Background process stopped".to_string();
            // SAFETY: plain call on our own window.
            unsafe { KillTimer(hwnd, TIMER_COST) };
        }
        Err(e) => a.model.message = e,
    }
}

/// Every 2 s while an effect runs: measure the engine and remember the cost.
fn sample_cost(hwnd: HWND) {
    let Some(a) = app(hwnd) else { return };
    let Some(effect) = a.model.running else { return };
    match control::sample_engine() {
        None => {
            // The engine exited by itself, for example after repeated bus errors.
            a.model.running = None;
            a.model.live = None;
            a.model.message = "Background process stopped".to_string();
            // SAFETY: plain call on our own window.
            unsafe { KillTimer(hwnd, TIMER_COST) };
        }
        Some(sample) => {
            if let Some(previous) = a.last_sample {
                let processors = std::thread::available_parallelism().map_or(1, |n| n.get());
                let cost = control::cost_between(&previous, &sample, processors);
                a.model.live = Some(cost);
                a.model.costs.set(effect, cost);
                let _ = system::save_costs(&a.model.costs);
            }
            a.last_sample = Some(sample);
        }
    }
}
```

In `click`, replace `Target::Apply | Target::Stop => {}` with:

```rust
        Target::Apply => apply(hwnd),
        Target::Stop => stop(hwnd),
```

In `WM_TIMER`, add a branch:

```rust
            } else if wparam == TIMER_COST {
                sample_cost(hwnd);
                invalidate(hwnd);
```

Each `app(hwnd)` borrow above ends before the next one starts, and none is held across `UpdateWindow`. `UpdateWindow` sends `WM_PAINT`, which borrows the App again.

- [ ] **Step 3: Build and test**

Run: `cargo test --workspace`, then `cargo build --release --workspace`
Expected: everything passes, with no warnings.

```bash
git add crates/app
git commit -m "feat: apply effects, stop the engine and show live cost"
```

- [ ] **Step 4: HARDWARE STAGE 8, the user runs it**

The user double-clicks `target\release\openicue.exe` and accepts the admin prompt, then checks each item in order. Stop at anything unexpected.

1. **Built-in effect:** select Rainbow wave and press Apply. Expected: "Applying…", then "Applied. You can close this window". The sticks show rainbow wave.
2. **Static with a color:** select Static, pick a color in the swatch dialog, then press Apply. Expected: all 4 sticks show that color.
3. **Custom effect:** select Breathing and press Apply. Expected: "Running in the background", an orange dot on the Breathing tile, Stop appears, and within about 4 s the cost shows `x.x MB · y.y% CPU`. Note the numbers.
4. **Close and reopen:** close the window, check the sticks keep breathing, then reopen it. Expected: Breathing is still marked running, Stop shows, and the cost updates.
5. **Stop:** press Stop. Expected: "Background process stopped", and the sticks freeze on their last frame. Then select Chase. Expected: its cost reads "Not measured yet", or a stored value if Chase was measured in this session.
6. **Switch from custom to built-in:** start Gradient flow, then select Color pulse and press Apply. Expected: the engine stops and Color pulse shows on all sticks.

- [ ] **Step 5: Record and commit**

Append a dated "Stage 8" section to `docs/hardware-findings.md` with what each check showed and the measured costs:

```bash
git add docs/hardware-findings.md
git commit -m "docs: record stage 8"
```

---

## After this plan

- **Update the docs:** mark Plan 2 done in `docs/roadmap.md`, and move any visual gaps from the Task 7 check there.
- **Then Plan 3, the installer:** use the "Plan 3 inputs" in `docs/roadmap.md`, plus the Setup, SetupProgress, SetupDone, SetupFailed and Uninstall artboards of the design.
