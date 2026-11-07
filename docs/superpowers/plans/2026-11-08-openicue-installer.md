# openiCUE Installer Implementation Plan (Plan 3 of 3)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build `openiCUE-Setup.exe`, a dark, single-window installer from the approved design. It:
- installs openiCUE to `C:\Program Files\openiCUE`;
- runs the official, unmodified, hash-pinned PawnIO setup if PawnIO is missing;
- creates the logon restore task, a Start menu entry and an uninstall entry;
- uninstalls all of that again, optionally including PawnIO.

**Architecture:** a new `openicue-setup` crate, laid out like the window crate:
- **Library (pure, unit-tested):** the screen model, layout, the task XML, and the install plan values.
- **Binary (Windows calls):** paint, window, install and uninstall actions, and a background worker thread so the window never freezes.

GDI+ drawing primitives move out of the window crate into a shared `openicue-gdi` crate, so the window and Setup share one implementation. The payload is embedded with `include_bytes!` only under a `payload` feature, so `cargo test --workspace` never needs the release binaries.

**Tech Stack:**
- **Language and libraries:** Rust 1.99 msvc and `windows-sys` 0.59 only, with no new crates.
- **Drawing:** GDI+ through `openicue-gdi`.
- **Security APIs:** CNG `BCryptHash` for SHA-256, `WinVerifyTrust` for Authenticode, and `schtasks.exe` with an XML task file.
- **Start menu entry:** a hand-written `IShellLinkW` COM call for the shortcut.

**Spec:** `docs/superpowers/specs/2026-11-05-openicue-design.md`, sections "Installer flow" and "Security rules". Also read `docs/roadmap.md` ("Plan 3 inputs" and the locked decisions) and the design's Setup, SetupProgress, SetupDone, SetupFailed and Uninstall artboards. Setup's markup is quoted where it's needed below.

## Global Constraints

- **Admin and file safety:**
  - Setup runs as admin, through a `requireAdministrator` manifest.
  - It contains no network code, opens no web pages, and starts every program by full path, never through a search path.
  - It links statically, through the existing `.cargo/config.toml`.
- **Restricted DLL search:** Setup is double-clicked from Downloads, so call `SetDefaultDllDirectories(LOAD_LIBRARY_SEARCH_SYSTEM32)` as the very first statement of `main`.
- **Install location:** always `<Program Files>\openiCUE`. Get Program Files with `SHGetKnownFolderPath(FOLDERID_ProgramFiles)`, never from an environment variable. The folder must stay writable by admins only (roadmap Plan 3 input 4).
- **PawnIO:**
  - Run the official signed setup only after its SHA-256 equals `1F519A22E47187F70A1379A48CA604981C4FCF694F4E65B734AAA74A9FBA3032` and `WinVerifyTrust` accepts its signature.
  - On any failure, stop before changing anything. Never install the Unrestricted edition.
- **Check before changing:** check every payload file before the first change. A failure after that point reports what failed. It never claims "nothing was changed".
- **What to ship:** never ship `openicue-hwtest.exe` (roadmap Plan 3 input 2).
- **Dependencies:** add only `windows-sys` features. `Cargo.lock` gains only our own `openicue-gdi` and `openicue-setup`.
- **Colors and sizes:**
  - Background `#0B0C0E`, text `#F2F4F5`, muted `#9BA3AB`, pill `#17191D`, ongoing orange `#FFB066`.
  - The Setup window is 560 × 440 logical pixels and frameless.
- **Hardware and system:**
  - Subagents never run `openicue.exe`, `openicue-engine.exe`, `openicue-hwtest.exe` or `openiCUE-Setup.exe`, and never install or download anything.
  - The user runs every install test, in Windows Sandbox first.
- **Commits:** no AI attribution lines, and no pushing. If GPG signing times out, retry once, then stop and report.
- **Build:** run cargo from PowerShell. Git Bash has a different `link.exe` first on its PATH.

## Decisions in this plan (2026-11-08, chosen by the user)

- **Bundling:** PawnIO's official setup is bundled unchanged. Setup installs `LICENSE-PawnIO.txt` (GPL-2 text) and `PawnIO-SOURCE.txt` (where its source is) next to openiCUE. Recheck the licensing before any public release.
- **Admin prompt:** Setup asks for admin at launch, through its manifest. The welcome screen's "Windows will ask for admin approval next" hint is dropped, as is the uninstall screen's.
- **Start menu:** a Start menu shortcut, "openiCUE", for all users, made through a small hand-written `IShellLinkW` call.

## Further decisions (2026-11-08)

- **Logon task:**
  - Registered from an XML file with `schtasks.exe`, so no COM is needed.
  - It runs for any member of `BUILTIN\Users` at logon, with highest privileges. For an admin that means elevated. For a standard user the engine exits 9, which is harmless.
  - The task is named `\openiCUE\Restore`.
- **Uninstall:**
  - Setup copies itself to `<install dir>\setup.exe`. The uninstall entry runs `"<install dir>\setup.exe" --uninstall`.
  - The running `setup.exe` and the folder are removed at the next restart with `MoveFileExW(..., MOVEFILE_DELAY_UNTIL_REBOOT)`.
- **Removing PawnIO** on uninstall runs the embedded PawnIO setup with `-uninstall -silent`. The installer uses `-install -silent`. Both switches are verified in Sandbox (stage 9).
- **Upgrade:** Setup stops the engine, then replaces the install folder. If `openicue.exe` is open, the file copy fails and Setup reports "Close openiCUE, then run Setup again."

---

### Task 1: Move the GDI+ primitives into a shared crate

**Files:**
- Create: `crates/gdi/Cargo.toml`, `crates/gdi/src/lib.rs`
- Modify: root `Cargo.toml` (members), `crates/app/Cargo.toml`, `crates/app/src/layout.rs`, `crates/app/src/paint.rs`, `crates/app/src/window.rs`

**Interfaces:**
- Produces (`openicue_gdi`):
  - `struct Rect { pub x, pub y, pub w, pub h: f32 }`, with `contains(x, y)` and `right()` (moved from `layout.rs`)
  - `struct FontSpec { pub family: &'static str, pub size: f32, pub bold: bool }`
  - `enum Align { Near, Center, Far }`
  - `struct Canvas` with:
    - `new(&[FontSpec])`
    - `measure(g, &str, font: usize) -> f32`
    - `text(g, &str, font, Rect, argb: u32, Align)`
    - `wrapped(g, &str, font, Rect, argb, Align)` (top-aligned, word-wrapped)
    - `rounded(g, Rect, radius, argb)`
    - `glow(g, Rect, argb_center: u32)`
    - `line(g, argb, width, &[(f32, f32)])`
    - `frame(hdc, width_px, height_px, scale, background: u32, draw: impl FnOnce(Graphics) -> T) -> T`
  - `type Graphics = *mut GpGraphics`
  - `fn wide(&str) -> Vec<u16>`
  - `struct GdiPlus` (RAII: `GdiPlus::start() -> Option<GdiPlus>`, shutdown on drop)
- Consumes: the current `crates/app/src/paint.rs` code.

This task is a refactor with no behavior change. The window must look exactly as it does after Plan 2.

- [ ] **Step 1: Create the crate**

In the root `Cargo.toml`, add `"crates/gdi"` to `members`.

`crates/gdi/Cargo.toml`:

```toml
[package]
name = "openicue-gdi"
version = "0.1.0"
edition = "2021"

[dependencies]
windows-sys = { version = "0.59", features = [
  "Win32_Foundation",
  "Win32_Graphics_Gdi",
  "Win32_Graphics_GdiPlus",
] }
```

- [ ] **Step 2: Write `crates/gdi/src/lib.rs`**

Move the drawing code out of `crates/app/src/paint.rs` into this shape. The bodies are the ones already in `paint.rs` (`rounded`, `glow`, `line`, `measure`, `text`, plus the off-screen bitmap code from `paint`). Only the signatures change, as shown. `rgb_wide` becomes `wide`.

```rust
//! GDI+ drawing shared by the openiCUE window and Setup.
use std::ptr::null_mut;
use windows_sys::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, SelectObject, HDC, SRCCOPY,
};
use windows_sys::Win32::Graphics::GdiPlus::*;

pub type Graphics = *mut GpGraphics;

pub fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

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

#[derive(Clone, Copy, Debug)]
pub struct FontSpec {
    pub family: &'static str,
    pub size: f32,
    pub bold: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    Near,
    Center,
    Far,
}

fn align(a: Align) -> i32 {
    match a {
        Align::Near => StringAlignmentNear,
        Align::Center => StringAlignmentCenter,
        Align::Far => StringAlignmentFar,
    }
}

fn rect_f(r: Rect) -> RectF {
    RectF { X: r.x, Y: r.y, Width: r.w, Height: r.h }
}

/// Starts GDI+ for this process and shuts it down when dropped. Drop every
/// `Canvas` before this value.
pub struct GdiPlus(usize);

impl GdiPlus {
    pub fn start() -> Option<GdiPlus> {
        let mut token = 0usize;
        let input = GdiplusStartupInput { GdiplusVersion: 1, DebugEventCallback: 0, SuppressBackgroundThread: 0, SuppressExternalCodecs: 0 };
        // SAFETY: valid in/out pointers; no output struct is requested.
        (unsafe { GdiplusStartup(&mut token, &input, null_mut()) } == 0).then_some(GdiPlus(token))
    }
}

impl Drop for GdiPlus {
    fn drop(&mut self) {
        // SAFETY: the token came from a successful GdiplusStartup.
        unsafe { GdiplusShutdown(self.0) };
    }
}

/// Fonts plus drawing helpers. Font families are created once per distinct name.
pub struct Canvas {
    families: Vec<(&'static str, *mut GpFontFamily)>,
    fonts: Vec<*mut GpFont>,
    format: *mut GpStringFormat,
}

impl Canvas {
    pub fn new(specs: &[FontSpec]) -> Canvas { /* as Painter::new, but looping over `specs`:
        reuse a family when `family` names match, style FontStyleBold when `bold`. */ }
    pub fn measure(&self, g: Graphics, text: &str, font: usize) -> f32 { /* Painter::measure */ }
    pub fn text(&self, g: Graphics, text: &str, font: usize, rect: Rect, color: u32, a: Align) { /* Painter::text, line align centre */ }
    /// Word-wrapped, top-aligned text (the error body, setup paragraphs).
    pub fn wrapped(&self, g: Graphics, text: &str, font: usize, rect: Rect, color: u32, a: Align) { /* as `text` but StringAlignmentNear line align */ }
    pub fn rounded(&self, g: Graphics, r: Rect, radius: f32, color: u32) { /* Painter::rounded */ }
    /// Soft glow around `r`; `center` is the ARGB at the centre, fading to clear.
    pub fn glow(&self, g: Graphics, r: Rect, center: u32) { /* Painter::glow, edge = center & 0x00FF_FFFF */ }
    pub fn line(&self, g: Graphics, color: u32, width: f32, points: &[(f32, f32)]) { /* Painter::line */ }
    /// Draws off-screen at `scale`, then copies to `hdc` in one go.
    pub fn frame<T>(&self, hdc: HDC, width: i32, height: i32, scale: f32, background: u32, draw: impl FnOnce(Graphics) -> T) -> T { /* Painter::paint's bitmap/graphics setup, calling `draw(g)` in place of layout+draw */ }
}

impl Drop for Canvas { /* delete fonts, families, format: as Painter's Drop */ }
```

The `/* ... */` comments name the existing function each body comes from. Copy those bodies verbatim and adapt only the parameter names. In `glow`, the centre alpha the window used was `0x55`, so the window now passes `argb(color, 0x55)`.

- [ ] **Step 3: Switch the window crate to it**

1. Add the dependency to `crates/app/Cargo.toml`: `openicue-gdi = { path = "../gdi" }`.
2. In `crates/app/src/layout.rs`, delete `struct Rect` and its `impl`, and add `pub use openicue_gdi::Rect;` so `layout::Rect` still works.
3. In `crates/app/src/paint.rs`, keep `Painter`, but make it a thin wrapper:
   - **Fields:** it holds `canvas: Canvas`.
   - **Fonts:** `Painter::new` builds the canvas from 9 `FontSpec`s, in `index()` order:

     | Font | Family | Size | Weight |
     |---|---|---|---|
     | 0 | Segoe UI Semibold | 13 | regular |
     | 1 | Segoe UI | 12 | regular |
     | 2 | Segoe UI | 12 | regular |
     | 3 | Segoe UI | 14 | regular |
     | 4 | Segoe UI | 14 | bold |
     | 5 | Segoe UI | 13 | regular |
     | 6 | Segoe UI Light | 30 | regular |
     | 7 | Segoe UI | 13 | regular |
     | 8 | Segoe UI | 15 | regular |

   - **Drawing:** every `self.text(...)`, `self.rounded(...)` and similar call becomes `self.canvas.text(g, …, index(font), …, Align::…)`, with alignment 0→`Near`, 1→`Center`, 2→`Far`.
   - **Error body:** use `canvas.wrapped`.
   - **`paint`:** becomes `self.canvas.frame(hdc, w, h, scale, BG, |g| { let layout = …; self.draw(g, …); layout })`.
4. In `crates/app/src/window.rs`:
   - Replace `GdiplusStartup` and `GdiplusShutdown` with `let Some(gdi) = openicue_gdi::GdiPlus::start() else { return 1 };`.
   - Keep `gdi` alive until after the message loop. Window destruction drops the App, and with it the canvas, inside the loop, so the order is right.
   - Replace `rgb_wide` with `openicue_gdi::wide`.

- [ ] **Step 4: Build and test**

Run: `cargo test --workspace`, then `cargo build --release --workspace`
Expected: all 87 tests pass, and the build has no warnings.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml Cargo.lock crates
git commit -m "refactor: share GDI+ drawing in openicue-gdi"
```

The user glances at the window during stage 10. It must look unchanged.

---

### Task 2: Setup crate, screen model and layout

**Files:**
- Create: `crates/setup/Cargo.toml`, `crates/setup/build.rs`, `crates/setup/src/lib.rs`, `crates/setup/src/model.rs`, `crates/setup/src/layout.rs`, `crates/setup/src/main.rs`
- Modify: root `Cargo.toml` (members)

**Interfaces:**
- Consumes: `openicue_gdi::Rect`
- Produces (`openicue_setup`):
  - `model::Screen { Welcome, Progress, Done, Failed, Uninstall, Removed }`
  - `model::Failure { PawnIoCheck, Other(String) }`
  - `model::StepState { Done, Active, Waiting }`
  - `model::STEPS: [&str; 4]`
  - `model::Model { pub screen, pub step: usize, pub failure: Option<Failure>, pub remove_pawnio: bool }`, with:
    - `Model::installing() -> Model` and `Model::uninstalling() -> Model`
    - `window_title`, `heading`, `body() -> Option<String>`, `primary() -> Option<&'static str>`, `secondary() -> Option<&'static str>`, `hint`
    - `step_state(i) -> StepState`, `lit_leds() -> usize`
  - `layout::{WIDTH = 560.0, HEIGHT = 440.0, Target { Close, Primary, Secondary, RemovePawnIo }, Button, Layout, layout(&Model, measure) -> Layout, hit, is_drag_area}`

- [ ] **Step 1: Create the crate**

Add `"crates/setup"` to the root `members`.

`crates/setup/Cargo.toml`:

```toml
[package]
name = "openicue-setup"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "openiCUE-Setup"
path = "src/main.rs"
test = false

[features]
# Embeds the release binaries and PawnIO. Only scripts/build-setup.ps1 turns it on.
payload = []
# Test builds only: corrupts the embedded PawnIO copy in memory, to show the failure path.
tamper-test = ["payload"]

[dependencies]
openicue-core = { path = "../core" }
openicue-gdi = { path = "../gdi" }
windows-sys = { version = "0.59", features = [
  "Win32_Foundation",
  "Win32_Graphics_Dwm",
  "Win32_Graphics_Gdi",
  "Win32_Graphics_GdiPlus",
  "Win32_Security_Cryptography",
  "Win32_Security_WinTrust",
  "Win32_Storage_FileSystem",
  "Win32_System_Com",
  "Win32_System_LibraryLoader",
  "Win32_System_Registry",
  "Win32_System_Threading",
  "Win32_UI_HiDpi",
  "Win32_UI_Input_KeyboardAndMouse",
  "Win32_UI_Shell",
  "Win32_UI_WindowsAndMessaging",
] }
```

`crates/setup/build.rs`:

```rust
fn main() {
    // Setup changes Program Files, so Windows must start it as admin.
    println!("cargo:rustc-link-arg-bins=/MANIFEST:EMBED");
    println!("cargo:rustc-link-arg-bins=/MANIFESTUAC:level='requireAdministrator' uiAccess='false'");
}
```

`crates/setup/src/lib.rs`:

```rust
//! Setup logic that needs no Windows calls, so it can be unit-tested.
pub mod layout;
pub mod model;
```

`crates/setup/src/main.rs` (Task 5 replaces it):

```rust
#![windows_subsystem = "windows"]

fn main() {}
```

- [ ] **Step 2: Write the failing tests**

`crates/setup/src/model.rs`, with tests only:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn welcome_matches_the_design() {
        let m = Model::installing();
        assert_eq!(m.window_title(), "openiCUE Setup");
        assert_eq!(m.heading(), "Install openiCUE");
        assert_eq!((m.primary(), m.secondary()), (Some("Install"), Some("Cancel")));
        assert_eq!(m.lit_leds(), 10);
        assert_eq!(m.hint(), "");
    }

    #[test]
    fn progress_marks_steps_and_lights_leds() {
        let mut m = Model::installing();
        m.screen = Screen::Progress;
        m.step = 2;
        assert_eq!(m.heading(), "Installing");
        assert_eq!(m.step_state(1), StepState::Done);
        assert_eq!(m.step_state(2), StepState::Active);
        assert_eq!(m.step_state(3), StepState::Waiting);
        assert_eq!(m.lit_leds(), 5);
        assert_eq!((m.primary(), m.secondary()), (None, None));
        assert_eq!(m.hint(), "This usually takes under a minute.");
    }

    #[test]
    fn done_and_failed_have_the_design_text() {
        let mut m = Model::installing();
        m.screen = Screen::Done;
        assert_eq!(m.heading(), "openiCUE is ready");
        assert_eq!((m.primary(), m.secondary()), (Some("Open openiCUE"), Some("Close")));
        m.screen = Screen::Failed;
        m.failure = Some(Failure::PawnIoCheck);
        assert_eq!(m.heading(), "Setup stopped");
        assert!(m.body().unwrap().starts_with("The PawnIO file failed its security check"));
        assert_eq!((m.primary(), m.secondary()), (Some("Close"), None));
        m.failure = Some(Failure::Other("Close openiCUE, then run Setup again.".into()));
        assert_eq!(m.body().unwrap(), "Close openiCUE, then run Setup again.");
        assert_eq!(m.lit_leds(), 0);
    }

    #[test]
    fn uninstall_screens() {
        let mut m = Model::uninstalling();
        assert_eq!(m.window_title(), "openiCUE");
        assert_eq!(m.heading(), "Uninstall openiCUE");
        assert_eq!((m.primary(), m.secondary()), (Some("Uninstall"), Some("Cancel")));
        assert!(!m.remove_pawnio);
        m.screen = Screen::Removed;
        assert_eq!(m.heading(), "openiCUE is removed");
        assert_eq!((m.primary(), m.secondary()), (Some("Close"), None));
    }
}
```

`crates/setup/src/layout.rs`, with tests only:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Model, Screen};

    fn measure(text: &str, _: Font) -> f32 {
        text.chars().count() as f32 * 7.0
    }

    fn targets(l: &Layout) -> Vec<Target> {
        l.buttons.iter().map(|b| b.target).collect()
    }

    #[test]
    fn welcome_has_install_and_cancel_on_the_bottom_right() {
        let l = layout(&Model::installing(), &measure);
        assert_eq!(targets(&l), vec![Target::Close, Target::Secondary, Target::Primary]);
        let primary = l.buttons[2].rect;
        assert_eq!(primary.right(), WIDTH - 32.0);
        assert_eq!(primary.y + primary.h, HEIGHT - 24.0);
    }

    #[test]
    fn uninstall_has_the_pawnio_checkbox() {
        let l = layout(&Model::uninstalling(), &measure);
        assert!(targets(&l).contains(&Target::RemovePawnIo));
    }

    #[test]
    fn progress_has_no_buttons_but_close_is_hidden() {
        let mut m = Model::installing();
        m.screen = Screen::Progress;
        let l = layout(&m, &measure);
        assert!(targets(&l).is_empty(), "no way to cancel half way");
    }

    #[test]
    fn hit_and_drag() {
        let l = layout(&Model::installing(), &measure);
        let p = l.buttons[2].rect;
        assert_eq!(hit(&l, p.x + 1.0, p.y + 1.0), Some(Target::Primary));
        assert!(is_drag_area(&l, 50.0, 20.0));
        assert!(!is_drag_area(&l, WIDTH - 10.0, 20.0));
    }
}
```

Add `pub mod layout; pub mod model;` to `lib.rs` (already in Step 1).

- [ ] **Step 3: Run them to confirm they fail**

Run: `cargo test -p openicue-setup`
Expected: a compile error, because the types don't exist yet.

- [ ] **Step 4: Implement `model.rs`**

Put this above the test module:

```rust
//! What each Setup screen says and offers. Pure, so it is tested.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    Welcome,
    Progress,
    Done,
    Failed,
    Uninstall,
    Removed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Failure {
    /// The bundled PawnIO setup failed its hash or signature check.
    PawnIoCheck,
    /// Anything else, with a sentence for the user.
    Other(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepState {
    Done,
    Active,
    Waiting,
}

pub const STEPS: [&str; 4] = [
    "Checking files",
    "Installing openiCUE",
    "Installing PawnIO driver",
    "Setting up restore at logon",
];

pub struct Model {
    pub screen: Screen,
    /// The step in progress, 0 to 3; 4 once all are done.
    pub step: usize,
    pub failure: Option<Failure>,
    pub remove_pawnio: bool,
}

impl Model {
    pub fn installing() -> Model {
        Model { screen: Screen::Welcome, step: 0, failure: None, remove_pawnio: false }
    }

    pub fn uninstalling() -> Model {
        Model { screen: Screen::Uninstall, ..Model::installing() }
    }

    pub fn window_title(&self) -> &'static str {
        match self.screen {
            Screen::Uninstall | Screen::Removed => "openiCUE",
            _ => "openiCUE Setup",
        }
    }

    pub fn heading(&self) -> &'static str {
        match self.screen {
            Screen::Welcome => "Install openiCUE",
            Screen::Progress => "Installing",
            Screen::Done => "openiCUE is ready",
            Screen::Failed => "Setup stopped",
            Screen::Uninstall => "Uninstall openiCUE",
            Screen::Removed => "openiCUE is removed",
        }
    }

    /// The paragraph under the heading, where the screen has one.
    pub fn body(&self) -> Option<String> {
        match self.screen {
            Screen::Done => Some("openiCUE and the PawnIO driver are installed. Your effect will come back each time you log on.".into()),
            Screen::Failed => Some(match &self.failure {
                Some(Failure::Other(reason)) => reason.clone(),
                _ => "The PawnIO file failed its security check, so it was not run and nothing was installed. Don't retry this copy of Setup. Download openiCUE Setup again from its official source.".into(),
            }),
            Screen::Uninstall => Some("Removes the app, the logon task and your settings.".into()),
            Screen::Removed => Some("The app, the logon task and your settings are gone. A leftover folder is removed when you next restart.".into()),
            _ => None,
        }
    }

    pub fn primary(&self) -> Option<&'static str> {
        match self.screen {
            Screen::Welcome => Some("Install"),
            Screen::Done => Some("Open openiCUE"),
            Screen::Failed | Screen::Removed => Some("Close"),
            Screen::Uninstall => Some("Uninstall"),
            Screen::Progress => None,
        }
    }

    pub fn secondary(&self) -> Option<&'static str> {
        match self.screen {
            Screen::Welcome | Screen::Uninstall => Some("Cancel"),
            Screen::Done => Some("Close"),
            _ => None,
        }
    }

    pub fn hint(&self) -> &'static str {
        if self.screen == Screen::Progress { "This usually takes under a minute." } else { "" }
    }

    pub fn step_state(&self, i: usize) -> StepState {
        if i < self.step {
            StepState::Done
        } else if i == self.step {
            StepState::Active
        } else {
            StepState::Waiting
        }
    }

    /// How many of the 10 strip LEDs glow, as in the design.
    pub fn lit_leds(&self) -> usize {
        match self.screen {
            Screen::Welcome | Screen::Done => 10,
            // Steps 0..4 light 0, 2, 5, 7, 10 LEDs, as the design's progress sweep.
            Screen::Progress => self.step * 10 / STEPS.len(),
            _ => 0,
        }
    }
}
```

- [ ] **Step 5: Implement `layout.rs`**

```rust
//! Setup window layout in logical pixels. Pure, so it is tested.
use crate::model::{Model, Screen, STEPS};
pub use openicue_gdi::Rect;

pub const WIDTH: f32 = 560.0;
pub const HEIGHT: f32 = 440.0;
pub const TITLE_H: f32 = 44.0;
const SIDE: f32 = 32.0;
const PILL_H: f32 = 44.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Font {
    /// 13 px semibold: window title, item names.
    Brand,
    /// 14 px: body text and buttons.
    Body,
    /// 14 px bold: the primary button.
    BodyBold,
    /// 13 px: notes under items.
    Small,
    /// 12 px: the hint.
    Hint,
    /// 28 px light: the heading.
    Heading,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Close,
    Primary,
    Secondary,
    RemovePawnIo,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Button {
    pub rect: Rect,
    pub target: Target,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Layout {
    pub close: Rect,
    /// The 10-LED strip.
    pub strip: Rect,
    pub heading: Rect,
    /// Where the screen's content starts (paragraph, list or steps).
    pub content: Rect,
    pub hint: Rect,
    pub buttons: Vec<Button>,
}

pub fn layout(model: &Model, measure: &dyn Fn(&str, Font) -> f32) -> Layout {
    let close = Rect { x: WIDTH - 48.0, y: 0.0, w: 48.0, h: TITLE_H };
    let strip = Rect { x: SIDE, y: TITLE_H + 12.0, w: WIDTH - 2.0 * SIDE, h: 8.0 };
    let heading = Rect { x: SIDE, y: strip.y + strip.h + 28.0, w: WIDTH - 2.0 * SIDE, h: 38.0 };
    let content_y = heading.y + heading.h + 16.0;
    let bottom = HEIGHT - 24.0;
    let mut out = Layout {
        close,
        strip,
        heading,
        content: Rect { x: SIDE, y: content_y, w: WIDTH - 2.0 * SIDE, h: bottom - PILL_H - 12.0 - content_y },
        hint: Rect { x: SIDE, y: bottom - PILL_H, w: 260.0, h: PILL_H },
        buttons: Vec::new(),
    };
    if model.screen == Screen::Progress {
        // Nothing to press while files are being changed, not even close.
        return out;
    }
    out.buttons.push(Button { rect: close, target: Target::Close });
    if model.screen == Screen::Uninstall {
        // Paragraph (20) + gap 8, then the 44 px checkbox row.
        let rect = Rect { x: SIDE, y: content_y + 28.0, w: 32.0 + measure("Also remove the PawnIO driver", Font::Body), h: PILL_H };
        out.buttons.push(Button { rect, target: Target::RemovePawnIo });
    }
    let mut right = WIDTH - SIDE;
    let mut placed = Vec::new();
    if let Some(label) = model.primary() {
        let w = 56.0 + measure(label, Font::BodyBold);
        placed.push(Button { rect: Rect { x: right - w, y: bottom - PILL_H, w, h: PILL_H }, target: Target::Primary });
        right -= w + 8.0;
    }
    if let Some(label) = model.secondary() {
        let w = 40.0 + measure(label, Font::Body);
        placed.push(Button { rect: Rect { x: right - w, y: bottom - PILL_H, w, h: PILL_H }, target: Target::Secondary });
    }
    placed.reverse();
    out.buttons.extend(placed);
    out
}

pub fn hit(layout: &Layout, x: f32, y: f32) -> Option<Target> {
    layout.buttons.iter().find(|b| b.rect.contains(x, y)).map(|b| b.target)
}

pub fn is_drag_area(layout: &Layout, x: f32, y: f32) -> bool {
    y < TITLE_H && !layout.close.contains(x, y)
}
```

Change the import to `use crate::model::{Model, Screen};`. Paint draws the steps from `content`, one 26 px row each.

The test `progress_has_no_buttons_but_close_is_hidden` expects an empty list. That's correct: the Progress screen has no Close button.

- [ ] **Step 6: Run tests, then commit**

Run: `cargo test --workspace`
Expected: everything passes.

```bash
git add Cargo.toml Cargo.lock crates/setup
git commit -m "feat: add setup crate with screen model and layout"
```

---

### Task 3: Payload, hashing and signature checks

**Files:**
- Create: `crates/setup/src/verify.rs`, `crates/setup/src/payload.rs`, `crates/setup/payload/PawnIO_setup.exe`, `crates/setup/payload/LICENSE-PawnIO.txt`, `crates/setup/payload/PawnIO-SOURCE.txt`, `scripts/build-setup.ps1`
- Modify: `crates/setup/src/lib.rs`

**Interfaces:**
- Produces:
  - `verify::sha256(&[u8]) -> Result<[u8; 32], i32>`
  - `verify::hex(&[u8; 32]) -> String`
  - `verify::PAWNIO_SHA256: &str`
  - `verify::is_signed(&Path) -> bool`
  - `payload::{APP, ENGINE, PAWNIO, LICENSE_PAWNIO, PAWNIO_SOURCE, LICENSE_MODULES}: &'static [u8]`
  - `payload::present() -> bool`

- [ ] **Step 1: The controller (not a subagent) adds the PawnIO files, with the user's OK**

Ask the user for permission first. These three files go into the repository:

1. **`crates/setup/payload/PawnIO_setup.exe`:** the file the user already downloaded and checked. In PowerShell:

   ```powershell
   Get-ChildItem "$env:USERPROFILE\Downloads\PawnIO_setup*.exe" | ForEach-Object { "{0}  {1}" -f (Get-FileHash $_ -Algorithm SHA256).Hash, $_.Name }
   ```

   Copy the file whose hash equals `1F519A22E47187F70A1379A48CA604981C4FCF694F4E65B734AAA74A9FBA3032`. If none matches, stop.

2. **`crates/setup/payload/LICENSE-PawnIO.txt`:** PawnIO's `COPYING` (GPL-2), downloaded only after the user agrees:

   ```bash
   gh api -H "Accept: application/vnd.github.raw" repos/namazso/PawnIO/contents/COPYING > crates/setup/payload/LICENSE-PawnIO.txt
   ```

3. **`crates/setup/payload/PawnIO-SOURCE.txt`:**

   ```text
   openiCUE Setup includes the official, unmodified PawnIO 2.2.0 setup
   (SHA-256 1F519A22E47187F70A1379A48CA604981C4FCF694F4E65B734AAA74A9FBA3032).
   PawnIO is free software under the GNU General Public License version 2;
   see LICENSE-PawnIO.txt. Its complete source code is published at
   https://github.com/namazso/PawnIO and https://pawnio.eu
   ```

Commit them:

```bash
git add crates/setup/payload
git commit -m "chore: add the official PawnIO setup and its license for bundling"
```

- [ ] **Step 2: Write the failing tests**

`crates/setup/src/verify.rs`, with tests only:

```rust
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
```

Add `pub mod verify;` and `pub mod payload;` to `crates/setup/src/lib.rs`.

- [ ] **Step 3: Run them to confirm they fail**

Run: `cargo test -p openicue-setup verify`
Expected: a compile error.

- [ ] **Step 4: Implement `verify.rs`**

```rust
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
```

If the `windows-sys` 0.59 names differ (field names, constant types or union shape), adapt them minimally and list the changes.

- [ ] **Step 5: Implement `payload.rs`**

```rust
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
```

- [ ] **Step 6: Write `scripts/build-setup.ps1`**

```powershell
# Builds openiCUE, then a Setup that embeds the fresh binaries and PawnIO.
$ErrorActionPreference = 'Stop'
Set-Location (Split-Path $PSScriptRoot -Parent)
cargo build --release -p openicue -p openicue-engine
if ($LASTEXITCODE -ne 0) { exit 1 }
cargo build --release -p openicue-setup --features payload
if ($LASTEXITCODE -ne 0) { exit 1 }
Get-Item target\release\openiCUE-Setup.exe | Select-Object Name, Length, LastWriteTime
```

- [ ] **Step 7: Run tests, then commit**

Run: `cargo test --workspace`
Expected: everything passes, including the pin test against the real file.

```bash
git add crates/setup scripts/build-setup.ps1
git commit -m "feat: add setup payload, SHA-256 and signature checks"
```

---

### Task 4: Install and uninstall actions

**Files:**
- Create: `crates/setup/src/plan.rs` (pure), `crates/setup/src/install.rs`, `crates/setup/src/shortcut.rs`
- Modify: `crates/setup/src/lib.rs` (add `pub mod plan;`), `crates/core/src/system.rs`

**Interfaces:**
- Consumes:
  - `verify::{sha256, hex, PAWNIO_SHA256, is_signed}`, `payload::*`
  - `system::{secure_data_dir, ERROR_DATA_DIR_UNTRUSTED}`, `control::stop_engine`
- Produces:
  - In core: `system::known_folder(&GUID) -> Result<PathBuf, u32>` (public). `program_data()` now calls it.
  - `plan::{TASK_NAME, task_xml(engine: &Path) -> String, xml_escape(&str) -> String, UNINSTALL_KEY, uninstall_values(dir: &Path, size_kb: u32) -> Vec<(&'static str, Value)>, enum Value { Text(String), Number(u32) }}`
  - `install::{Event { Step(usize), Done, Failed(Failure) }, run_install(report: impl Fn(Event)), run_uninstall(remove_pawnio: bool, report: impl Fn(Event))}`
  - `shortcut::create(link: &Path, target: &Path) -> Result<(), i32>`

- [ ] **Step 1: Write the failing tests for `plan.rs`**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn task_runs_the_engine_restore_at_logon_with_highest_privileges() {
        let xml = task_xml(Path::new(r"C:\Program Files\openiCUE\openicue-engine.exe"));
        assert!(xml.contains(r"<Command>C:\Program Files\openiCUE\openicue-engine.exe</Command>"));
        assert!(xml.contains("<Arguments>--restore</Arguments>"));
        assert!(xml.contains("<LogonTrigger>"));
        assert!(xml.contains("<GroupId>S-1-5-32-545</GroupId>"));
        assert!(xml.contains("<RunLevel>HighestAvailable</RunLevel>"));
        assert!(xml.contains("<ExecutionTimeLimit>PT0S</ExecutionTimeLimit>"));
        assert!(xml.contains("<DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>"));
    }

    #[test]
    fn xml_escapes_paths() {
        assert_eq!(xml_escape(r"C:\A&B<C>"), r"C:\A&amp;B&lt;C&gt;");
        let xml = task_xml(Path::new(r"C:\A&B\openicue-engine.exe"));
        assert!(xml.contains(r"C:\A&amp;B\openicue-engine.exe"));
    }

    #[test]
    fn uninstall_entry_points_at_setup_with_the_uninstall_flag() {
        let values = uninstall_values(Path::new(r"C:\Program Files\openiCUE"), 1200);
        let get = |name: &str| values.iter().find(|(n, _)| *n == name).map(|(_, v)| v.clone());
        assert_eq!(get("DisplayName"), Some(Value::Text("openiCUE".into())));
        assert_eq!(get("UninstallString"), Some(Value::Text(r#""C:\Program Files\openiCUE\setup.exe" --uninstall"#.into())));
        assert_eq!(get("InstallLocation"), Some(Value::Text(r"C:\Program Files\openiCUE".into())));
        assert_eq!(get("NoModify"), Some(Value::Number(1)));
        assert_eq!(get("NoRepair"), Some(Value::Number(1)));
        assert_eq!(get("EstimatedSize"), Some(Value::Number(1200)));
    }
}
```

- [ ] **Step 2: Run them to confirm they fail**

Run: `cargo test -p openicue-setup plan`
Expected: a compile error.

- [ ] **Step 3: Implement `plan.rs`**

```rust
//! Exact text of what Setup registers. Pure, so it is tested.
use std::path::Path;

pub const TASK_NAME: &str = r"\openiCUE\Restore";
pub const UNINSTALL_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\openiCUE";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    Text(String),
    Number(u32),
}

pub fn xml_escape(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// Runs `engine --restore` at any user's logon, elevated for admins, with
/// no time limit (a custom effect keeps running).
pub fn task_xml(engine: &Path) -> String {
    let command = xml_escape(&engine.to_string_lossy());
    format!(r#"<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.2" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <RegistrationInfo><Description>Restores the openiCUE lighting effect at logon.</Description></RegistrationInfo>
  <Triggers><LogonTrigger><Enabled>true</Enabled></LogonTrigger></Triggers>
  <Principals><Principal id="Users"><GroupId>S-1-5-32-545</GroupId><RunLevel>HighestAvailable</RunLevel></Principal></Principals>
  <Settings>
    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>
    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>
    <ExecutionTimeLimit>PT0S</ExecutionTimeLimit>
    <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>
    <Priority>7</Priority>
  </Settings>
  <Actions Context="Users"><Exec><Command>{command}</Command><Arguments>--restore</Arguments></Exec></Actions>
</Task>
"#)
}

pub fn uninstall_values(dir: &Path, size_kb: u32) -> Vec<(&'static str, Value)> {
    let dir = dir.to_string_lossy().to_string();
    vec![
        ("DisplayName", Value::Text("openiCUE".into())),
        ("DisplayVersion", Value::Text(env!("CARGO_PKG_VERSION").into())),
        ("Publisher", Value::Text("openiCUE".into())),
        ("DisplayIcon", Value::Text(format!(r"{dir}\openicue.exe"))),
        ("InstallLocation", Value::Text(dir.clone())),
        ("UninstallString", Value::Text(format!(r#""{dir}\setup.exe" --uninstall"#))),
        ("NoModify", Value::Number(1)),
        ("NoRepair", Value::Number(1)),
        ("EstimatedSize", Value::Number(size_kb)),
    ]
}
```

- [ ] **Step 4: Make `known_folder` public in core**

In `crates/core/src/system.rs`:
1. Rename `fn program_data()` to `pub fn known_folder(id: &windows_sys::core::GUID) -> Result<PathBuf, u32>`, and pass `id` in place of `&FOLDERID_ProgramData`.
2. Add `fn program_data() -> Result<PathBuf, u32> { known_folder(&FOLDERID_ProgramData) }`, so existing callers are unchanged.

Run `cargo test -p openicue-core`. Expected: all tests still pass.

- [ ] **Step 5: Implement `shortcut.rs`**

`windows-sys` has no COM interfaces, so call the two `IShellLinkW` and `IPersistFile` methods through their vtables. The slot numbers come from `ShObjIdl_core.h` and `ObjIdl.h`:
- `IShellLinkW`: `SetDescription` = 7, `SetWorkingDirectory` = 9, `SetPath` = 20.
- `IPersistFile`: `Save` = 6.
- Every interface: `QueryInterface` = 0, `Release` = 2.

```rust
//! Creates a .lnk shortcut through IShellLinkW, the only supported way.
use std::ffi::c_void;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::ptr::null_mut;
use windows_sys::core::{GUID, HRESULT, PCWSTR};
use windows_sys::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED};

const CLSID_SHELL_LINK: GUID = GUID::from_u128(0x00021401_0000_0000_c000_000000000046);
const IID_SHELL_LINK_W: GUID = GUID::from_u128(0x000214f9_0000_0000_c000_000000000046);
const IID_PERSIST_FILE: GUID = GUID::from_u128(0x0000010b_0000_0000_c000_000000000046);

fn wide(path: &Path) -> Vec<u16> {
    path.as_os_str().encode_wide().chain(std::iter::once(0)).collect()
}

/// A COM pointer released exactly once.
struct Com(*mut c_void);

impl Com {
    /// Method `slot` of this object's vtable.
    unsafe fn slot(&self, slot: usize) -> *const c_void {
        let vtable = *(self.0 as *const *const *const c_void);
        *vtable.add(slot)
    }
}

impl Drop for Com {
    fn drop(&mut self) {
        // SAFETY: slot 2 is IUnknown::Release on every COM interface.
        unsafe {
            let release: extern "system" fn(*mut c_void) -> u32 = std::mem::transmute(self.slot(2));
            release(self.0);
        }
    }
}

fn check(hr: HRESULT) -> Result<(), i32> {
    if hr < 0 { Err(hr) } else { Ok(()) }
}

pub fn create(link: &Path, target: &Path) -> Result<(), i32> {
    // SAFETY: COM is initialised and uninitialised on this thread around the work.
    unsafe {
        check(CoInitializeEx(null_mut(), COINIT_APARTMENTTHREADED as u32))?;
        let result = create_inner(link, target);
        CoUninitialize();
        result
    }
}

unsafe fn create_inner(link: &Path, target: &Path) -> Result<(), i32> {
    let mut raw = null_mut();
    check(CoCreateInstance(&CLSID_SHELL_LINK, null_mut(), CLSCTX_INPROC_SERVER, &IID_SHELL_LINK_W, &mut raw))?;
    let shell_link = Com(raw);
    let target_w = wide(target);
    let dir_w = wide(target.parent().ok_or(-1)?);
    let description: Vec<u16> = "Change your RAM lighting".encode_utf16().chain(std::iter::once(0)).collect();

    type SetText = extern "system" fn(*mut c_void, PCWSTR) -> HRESULT;
    let set_path: SetText = std::mem::transmute(shell_link.slot(20));
    check(set_path(shell_link.0, target_w.as_ptr()))?;
    let set_dir: SetText = std::mem::transmute(shell_link.slot(9));
    check(set_dir(shell_link.0, dir_w.as_ptr()))?;
    let set_description: SetText = std::mem::transmute(shell_link.slot(7));
    check(set_description(shell_link.0, description.as_ptr()))?;

    type Query = extern "system" fn(*mut c_void, *const GUID, *mut *mut c_void) -> HRESULT;
    let query: Query = std::mem::transmute(shell_link.slot(0));
    let mut raw = null_mut();
    check(query(shell_link.0, &IID_PERSIST_FILE, &mut raw))?;
    let persist = Com(raw);
    type Save = extern "system" fn(*mut c_void, PCWSTR, i32) -> HRESULT;
    let save: Save = std::mem::transmute(persist.slot(6));
    let link_w = wide(link);
    check(save(persist.0, link_w.as_ptr(), 1))
}
```

If `CoInitializeEx` returns `S_FALSE` (COM already initialised), that's success. `check` accepts every non-negative HRESULT.

- [ ] **Step 6: Implement `install.rs`**

Each step reports progress through `report`, and stops at the first error with a `Failure` for the user.

`install.rs` belongs to the binary, so it reaches the library through the crate name `openicue_setup`:

```rust
//! The work Setup does. Runs on a worker thread; reports through `report`.
use openicue_core::{control, system};
use openicue_setup::model::Failure;
use openicue_setup::payload;
use openicue_setup::plan::{self, Value};
use openicue_setup::verify::{hex, is_signed, sha256, PAWNIO_SHA256};
use std::os::windows::ffi::OsStrExt;
use std::os::windows::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;
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
```

`install.rs` and `shortcut.rs` belong to the **binary**, because they make system changes. `plan.rs`, `verify.rs` and `payload.rs` are in the library. In `install.rs`, `crate::shortcut::create` refers to the binary's own module.

`Command::new(path)` starts the program by its full path, with no search-path lookup. Its arguments are fixed strings.

So the compiler checks this task now, change `crates/setup/src/main.rs` to declare the two modules. Task 5 replaces this file and removes the `allow`:

```rust
#![windows_subsystem = "windows"]
// Used from Task 5's window; until then only compiled.
#![allow(dead_code)]
mod install;
mod shortcut;

fn main() {}
```

- [ ] **Step 7: Run tests and build**

Run: `cargo test --workspace`, then `cargo build --release -p openicue-setup`
Expected: everything passes, and the build has no warnings.

- [ ] **Step 8: Commit**

```bash
git add crates
git commit -m "feat: add install, uninstall and shortcut actions"
```

---

### Task 5: Setup window

**Files:**
- Create: `crates/setup/src/paint.rs`, `crates/setup/src/window.rs`
- Modify: `crates/setup/src/main.rs`

**Interfaces:**
- Consumes: `openicue_gdi::{Canvas, FontSpec, Align, GdiPlus, wide}`, `layout::*`, `model::*`, `install::{run_install, run_uninstall, Event}`
- Produces: `window::run(uninstall: bool) -> i32`

- [ ] **Step 1: Write `main.rs`**

```rust
#![windows_subsystem = "windows"]
mod install;
mod paint;
mod shortcut;
mod window;

use windows_sys::Win32::System::LibraryLoader::{SetDefaultDllDirectories, LOAD_LIBRARY_SEARCH_SYSTEM32};

fn main() {
    // Setup is started from Downloads, so never load a DLL from beside it.
    // SAFETY: plain call, first thing in the process.
    unsafe { SetDefaultDllDirectories(LOAD_LIBRARY_SEARCH_SYSTEM32) };
    let args: Vec<String> = std::env::args().skip(1).collect();
    let uninstall = match args.as_slice() {
        [] => false,
        [flag] if flag == "--uninstall" => true,
        _ => std::process::exit(2),
    };
    std::process::exit(window::run(uninstall));
}
```

- [ ] **Step 2: Write `paint.rs`**

Draw each element from `layout` with the shared `Canvas`, following the design's markup:
- **Title:** the window title, in the Brand font at x 32, vertically centred in the 44 px bar, colour `#F2F4F5`.
- **Close:** the ✕ (two 11 px lines, `#9BA3AB`, 1.2 px), only when `Target::Close` is among the buttons.
- **LED strip:** 10 cells across `strip`, with 2 px gaps and radius 2.
  - **Lit cells** (`i < model.lit_leds()`) use hue `(i·26 − t·40) mod 360` at HSL(…, 90 %, 58 %), plus a glow.
  - **The Failed screen's first cell** is `#FFB066` with a glow.
  - **Other cells** are `#17191D`.
  - Port the HSL conversion from `crates/app/src/preview.rs` `hsl`. Copy the formula, don't import it: the app's preview module is its library's private detail.
- **Heading:** Heading font (Segoe UI Light 28), `#F2F4F5`, near-aligned in `heading`.
- **Welcome content:** two items, 12 px apart, starting at `content.y`.
  - A Brand-weight name ("openiCUE" / "PawnIO driver").
  - Under each name, a Small `#9BA3AB` note ("Controls your RAM lighting." / "The official driver that lets openiCUE reach the LEDs. Skipped if you already have it.").
- **Progress content:** the 4 `STEPS`, one 26 px row each.
  - **Done** steps get a 14 px check mark (lines `(2,7.5)→(5.2,10.7)→(12,3.8)` inside a 14 px box), in `#F2F4F5`.
  - **The active step** gets a 7 px dot, in `#F2F4F5`.
  - **Waiting steps** are text only, in `#9BA3AB`.
  - Done and active text is `#F2F4F5`.
- **Done, Failed and Removed:** `body()` in `wrapped`, `#9BA3AB`.
- **Uninstall:**
  - `body()` in `#9BA3AB`.
  - A checkbox in the `RemovePawnIo` button: a 15 px box drawn as in the window's Random toggle, followed by "Also remove the PawnIO driver" in `#F2F4F5`.
  - Below the button, the note "Leave this off if other apps, such as Fan Control, use PawnIO." in Small `#9BA3AB`.
- **Hint:** `hint()` in `layout.hint`, Hint font, `#9BA3AB`.
- **Buttons:**
  - **Primary:** fill `#F2F4F5`, BodyBold text `#0B0C0E`.
  - **Secondary:** fill `#17191D`, Body text `#F2F4F5`.
  - Both have radius 22.

Font specs, in `Font` order: Brand (Segoe UI Semibold 13), Body (Segoe UI 14), BodyBold (Segoe UI 14 bold), Small (Segoe UI 13), Hint (Segoe UI 12), Heading (Segoe UI Light 28).

`Painter::paint(&self, hdc, w, h, scale, model, seconds) -> Layout` uses `canvas.frame`, exactly like the window's painter.

- [ ] **Step 3: Write `window.rs`**

Follow the window crate's `window.rs` structure:
- the frameless popup and DPI scale;
- `WM_NCHITTEST` dragging, double-buffered `WM_PAINT` and `WM_ERASEBKGND`;
- the App pointer in `GWLP_USERDATA`, freed in `WM_NCDESTROY`;
- a 90 ms animation timer;
- dark title and rounded corners through DWM;
- `GdiPlus::start()` held for the whole run.

Use a 560 × 440 logical size, centred on screen, and the title from `model.window_title()`. What's different:

```rust
const WM_APP_EVENT: u32 = WM_APP + 1;

pub struct App {
    pub model: Model,
    pub painter: Painter,
    pub layout: Option<Layout>,
    pub start: Instant,
    pub scale: f32,
    /// Install or uninstall events from the worker thread.
    pub events: std::sync::mpsc::Receiver<install::Event>,
    pub sender: std::sync::mpsc::Sender<install::Event>,
}
```

Click and key handling:
- **`Target::Close` or `Secondary`:** `DestroyWindow`. **Esc** does the same, except on the Progress screen.
- **`Target::RemovePawnIo`:** toggles `model.remove_pawnio`.
- **`Target::Primary`:**
  - **Welcome:**
    1. Set `screen = Progress, step = 0`.
    2. Clone the sender and store `hwnd as isize` in a local.
    3. `std::thread::spawn(move || install::run_install(|e| { let _ = sender.send(e); unsafe { PostMessageW(hwnd as HWND, WM_APP_EVENT, 0, 0) }; }))`.
  - **Uninstall:**
    1. Set `screen = Progress, step = 0`.
    2. Spawn `run_uninstall(remove_pawnio, …)` the same way.
    3. Record that this is an uninstall (`bool` in App), so `Done` maps to `Screen::Removed`.
  - **Done:** start `"<Program Files>\openiCUE\openicue.exe"` with `control`-style `CreateProcessW` by full path (as in `control::start_engine`, without arguments), then `DestroyWindow`.
  - **Failed or Removed:** `DestroyWindow`.
- **`WM_APP_EVENT`:** drain `events` with `try_recv`:
  - `Step(i)` → `model.step = i`
  - `Done` → `model.step = 4`, then `screen = Done`, or `Removed` for an uninstall
  - `Failed(f)` → `screen = Failed`, `failure = Some(f)`

  Then invalidate.
- **`WM_CLOSE` on the Progress screen:** return 0 without destroying. Files are half-changed, so the window must not close mid-install.

`HWND` is a raw pointer, so it is not `Send`. That's why the worker gets `hwnd as isize`, and only `PostMessageW` uses it. `PostMessageW` is safe to call from any thread.

- [ ] **Step 4: Build and test**

Run: `cargo test --workspace`, then `cargo build --release --workspace`, then `powershell -File scripts/build-setup.ps1`
Expected: everything passes with no warnings, and the script prints `openiCUE-Setup.exe` with a size of about 2–4 MB.

- [ ] **Step 5: Commit**

```bash
git add crates scripts
git commit -m "feat: add the Setup window and wire install and uninstall"
```

---

### Task 5b: Stick order step in Setup (added 2026-11-08)

**Why:** effects drawn in software need the physical slot order. The user approved Setup asking for it once, right after installing ("Setup: which slot is lit?" board on the design canvas). The window's Settings screen already does the same thing: `core::order::StickOrder`, `system::{load_order, save_order}` and `ram::light_only` exist, and the window's picker model and paint are the reference.

**Files:**
- Modify: `crates/setup/src/model.rs`, `crates/setup/src/layout.rs`, `crates/setup/src/paint.rs`, `crates/setup/src/window.rs`, `crates/setup/src/install.rs`

**Behaviour:**
1. **Flow:**
   - **Install:** after `run_install` reports `Done`, Setup shows the picker (`Screen::Arrange`) instead of `Done`.
   - **Uninstall:** never shows the picker.
2. **Before the picker:** open `PawnIoBus`, then `detect` and `read_info` the sticks. Skip straight to `Done` when:
   - fewer than 2 sticks answer;
   - the driver can't be opened (for example, PawnIO needs a restart);
   - a valid order is already saved and fits (a reinstall). The user can still change it later in Settings.
3. **While the picker is open:**
   - Hold `system::claim_engine()` and light the current stick with `ram::light_only`.
   - The screen is "Stick k of n" / "Which slot is lit?", with slots 72 × 150 at Setup size, the same pick logic, and auto-fill of the last stick.
   - **Buttons:** "Skip" (secondary) while picking. Once done, "Start over" and "Continue".
   - The hint reads "You can change this later in openiCUE."
4. **Continue:** `save_order`, release the claim, set every stick black with `light_only`'s black path (or one direct black frame), then show `Done`.
5. **Skip:** save nothing, release the claim, set the sticks black, then show `Done`.

   Setup doesn't re-apply an effect. The window or the logon task does that.
6. **Tests:**
   - Model tests for the new screen: the text, the buttons, picks and auto-fill. Reuse `StickOrder::from_picks`.
   - Layout tests: slots centred, inside 560 × 440, no overlaps.

**Commit:** `feat: ask the stick order in Setup`.

The Stage 9 Sandbox tests can't reach the RAM, so there the picker is skipped (no sticks). Stage 10 checks it on the real PC.

---

### Task 6: Windows Sandbox tests (HARDWARE STAGE 9, the user runs them)

Nothing in Sandbox can reach the RAM, so these tests check only Setup. Steps:

1. **Turn on Windows Sandbox** (user, once): Start → "Turn Windows features on or off" → tick **Windows Sandbox** → OK → restart. This changes a system setting, so only the user does it.
2. **Build the two test copies:**

   ```powershell
   powershell -File scripts/build-setup.ps1
   ```

   ```powershell
   Copy-Item target\release\openiCUE-Setup.exe "$env:USERPROFILE\Desktop\openiCUE-Setup.exe"
   ```

   ```powershell
   cargo build --release -p openicue-setup --features tamper-test; Copy-Item target\release\openiCUE-Setup.exe "$env:USERPROFILE\Desktop\openiCUE-Setup-TAMPERED.exe"
   ```

   Then rebuild the normal copy with `powershell -File scripts/build-setup.ps1`.
3. **Open Windows Sandbox** and copy both files from the Desktop into it, by copy and paste.
4. **In Sandbox:**
   1. **Tampered copy:** run it. Expected: the "Setup stopped" screen with the PawnIO security message. `C:\Program Files\openiCUE` must not exist.
   2. **Clean install:** run the normal copy and press Install. Expected:
      - the 4 steps tick off, then "openiCUE is ready";
      - `C:\Program Files\openiCUE` holds `openicue.exe`, `openicue-engine.exe`, `setup.exe` and the 3 text files;
      - the Start menu has "openiCUE";
      - `schtasks /Query /TN \openiCUE\Restore /XML` shows the task;
      - `sc query PawnIO` shows the service;
      - "Apps & features" lists openiCUE.
   3. **Run Setup again** (the upgrade case, with PawnIO already present). Expected: it succeeds and skips the PawnIO install.
   4. **Uninstall** from "Apps & features", leaving the box off. Expected:
      - "openiCUE is removed";
      - the task, Start menu entry, uninstall entry and `C:\ProgramData\openiCUE` are gone;
      - PawnIO is still installed.
   5. **Install again, then uninstall with "Also remove the PawnIO driver" ticked.** Expected: `sc query PawnIO` says the service doesn't exist.
5. **Report** each result. Tell me exactly if `-install -silent` or `-uninstall -silent` showed any PawnIO window or failed, because those switches are the plan's main unknown.

Record the results in `docs/hardware-findings.md` as "Stage 9", then commit:

```bash
git add docs/hardware-findings.md
git commit -m "docs: record setup sandbox tests"
```

---

### Task 7: Real install on the target PC (HARDWARE STAGE 10, the user runs it)

Only after stage 9 passes.

1. **Install:** run `openiCUE-Setup.exe` and press Install. PawnIO is already installed, so step 3 skips it. If a stick order is already saved, the picker is skipped. Otherwise, pick the slot of each lit stick, then press Continue.
2. **Open the window** from the Start menu. It must look the same as before Task 1, which confirms the shared drawing code changed nothing.
3. **Restore at logon:** apply **Breathing**, then restart Windows. Expected: Breathing runs after logon, with no window and no UAC prompt.
4. **Built-in after restart:** apply **Rainbow wave**, then restart. Expected: rainbow wave shows. The engine doesn't keep running: `Get-Process openicue-engine` finds nothing a minute after logon.

Record the results as "Stage 10" in `docs/hardware-findings.md`, update `docs/roadmap.md` (all three plans done), and commit:

```bash
git add docs
git commit -m "docs: record real install and logon restore"
```

---

## After this plan

- **Final review:** a whole-branch review of Plans 2 and 3 together, on the most capable model.
- **Before any public release:** code signing (the spec's open question), and the license review of PawnIO bundling and of the OpenRGB-derived protocol code.
