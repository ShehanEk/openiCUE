//! The frameless window: messages, DPI, timers and clicks.
use crate::actions::{self, Startup};
use crate::paint::Painter;
use openicue::layout::{self, Layout, Target};
use openicue::model::{self, Model, Screen, Slider};
use openicue_core::costs::Cost;
use openicue_core::effects::Rgb;
use openicue_core::order::StickOrder;
use openicue_core::{control, system};
use std::ptr::{null, null_mut};
use std::time::Instant;
use windows_sys::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Dwm::DwmSetWindowAttribute;
use windows_sys::Win32::Graphics::Gdi::{BeginPaint, EndPaint, InvalidateRect, UpdateWindow, PAINTSTRUCT};
use openicue_gdi::wide;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Controls::Dialogs::{ChooseColorW, CC_FULLOPEN, CC_RGBINIT, CHOOSECOLORW};
use windows_sys::Win32::UI::HiDpi::{GetDpiForWindow, SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture, VK_ESCAPE, VK_LEFT, VK_RETURN, VK_RIGHT};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

const TIMER_PREVIEW: usize = 1;
const TIMER_COST: usize = 2;
/// Runs only while the stick details open or close.
const TIMER_DETAILS: usize = 3;
/// Windows fires timers on its ~15.6 ms tick, after the delay has passed:
/// 16 ms waits two ticks (about 32 fps), so ask for less to get every tick.
const DETAILS_FRAME_MS: u32 = 10;
const PREVIEW_MS: u32 = 90;
const COST_MS: u32 = 2000;
const DWMWA_USE_IMMERSIVE_DARK_MODE: u32 = 20;
const DWMWA_WINDOW_CORNER_PREFERENCE: u32 = 33;
const DWMWCP_ROUND: u32 = 2;

pub struct App {
    pub model: Model,
    pub hardware: Option<actions::Hardware>,
    pub painter: Painter,
    pub layout: Option<Layout>,
    pub start: Instant,
    pub scale: f32,
    pub last_sample: Option<control::Sample>,
    /// The engine claim and sticks while the stick-order picker is open.
    pub arranging: Option<actions::Arranging>,
    /// The slider being dragged, while the mouse is captured.
    pub dragging: Option<Slider>,
    /// The last slider touched, which the Left and Right arrow keys adjust.
    pub last_slider: Option<Slider>,
    /// When the stick details last started opening or closing, and the
    /// progress they started from.
    pub details_from: (Instant, f32),
}

impl App {
    fn seconds(&self) -> f32 {
        (self.start.elapsed().as_secs_f64() % 86_400.0) as f32
    }

    /// Moves the details open or close on to now. Returns true when it's done.
    fn advance_details(&mut self) -> bool {
        let (since, from) = self.details_from;
        let ms = since.elapsed().as_secs_f32() * 1000.0;
        self.model.details_progress = (from + ms / model::DETAILS_TOTAL_MS).min(1.0);
        self.model.details_progress >= 1.0
    }

    /// Opens the driver and finds sticks. Used at start and by "Check again".
    pub fn check(&mut self) {
        self.hardware = None;
        match actions::startup() {
            Startup::NoDriver => self.model.screen = Screen::NoDriver,
            Startup::NoSticks => self.model.screen = Screen::NoSticks,
            Startup::Ready(hardware, families) => {
                let count = families.len();
                self.model.screen = Screen::App;
                self.model.sticks = count;
                self.model.families = families;
                self.model.order = StickOrder::or_identity(system::load_order(), count);
                self.hardware = Some(hardware);
            }
        }
    }
}

/// Matches `running` and the cost timer to whether an engine holds its claim.
/// The engine runs the saved effect, which may differ from the one on screen.
fn sync_running(hwnd: HWND) {
    let saved = system::load_settings();
    let running = (system::engine_claimed() && saved.runs_in_software()).then_some(saved.effect);
    let Some(a) = app(hwnd) else { return };
    if running != a.model.running {
        a.model.live = None;
        a.last_sample = None;
    }
    a.model.running = running;
    // SAFETY: plain calls on our own window.
    unsafe {
        if running.is_some() {
            SetTimer(hwnd, TIMER_COST, COST_MS, None);
        } else {
            KillTimer(hwnd, TIMER_COST);
        }
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

/// Shows `message` at once, before a call that blocks for up to a second.
fn show_now(hwnd: HWND, message: &str) {
    if let Some(a) = app(hwnd) {
        a.model.message = message.to_string();
    }
    invalidate(hwnd);
    // SAFETY: plain call on our own window; paints synchronously.
    unsafe { UpdateWindow(hwnd) };
}

/// Sets the status line after an action. Every outcome, success or not,
/// re-reads whether an engine runs, so the window never shows a stale state.
fn finish(hwnd: HWND, result: Result<(), String>, ok_message: &str) {
    sync_running(hwnd);
    if let Some(a) = app(hwnd) {
        a.model.message = match result {
            Ok(()) => ok_message.to_string(),
            Err(e) => e,
        };
    }
}

fn apply(hwnd: HWND) {
    let Some(a) = app(hwnd) else { return };
    let settings = a.model.settings;
    if settings.runs_in_software() {
        show_now(hwnd, "Starting…");
        let result = actions::apply_custom(&settings);
        finish(hwnd, result, "Running in the background");
        return;
    }
    show_now(hwnd, "Applying…");
    let result = match app(hwnd).and_then(|a| a.hardware.as_mut()) {
        Some(hardware) => hardware.apply_builtin(&settings),
        None => Err("Couldn't reach the RAM. Check again.".to_string()),
    };
    finish(hwnd, result, "Applied. You can close this window");
}

fn stop(hwnd: HWND) {
    show_now(hwnd, "Stopping…");
    let result = actions::stop();
    finish(hwnd, result, "Background process stopped");
}

/// "Change": stops the engine, takes its claim for as long as the picker is
/// open, and lights the first stick white.
fn change_order(hwnd: HWND) {
    show_now(hwnd, "Getting ready…");
    let result = match app(hwnd).and_then(|a| a.hardware.as_mut()) {
        Some(hardware) => hardware.begin_arrange(),
        None => Err("Couldn't reach the RAM. Check again.".to_string()),
    };
    match result {
        Ok(arranging) => {
            if let Some(a) = app(hwnd) {
                a.model.start_arrange(arranging.count());
                a.arranging = Some(arranging);
                // The engine is stopped, and our own claim would read as a running
                // engine, so stop measuring until the lights are put back.
                a.model.running = None;
                a.model.live = None;
                a.last_sample = None;
            }
            // SAFETY: plain call on our own window.
            unsafe { KillTimer(hwnd, TIMER_COST) };
        }
        Err(error) => {
            // The engine may have been stopped, or some sticks lit: put the saved effect back.
            let _ = restore_lights(hwnd);
            finish(hwnd, Err(error), "");
        }
    }
}

/// Lights `stick` white and the rest black, or shows why it couldn't.
fn light(hwnd: HWND, stick: usize) {
    let Some(a) = app(hwnd) else { return };
    let (Some(hardware), Some(arranging)) = (a.hardware.as_mut(), a.arranging.as_ref()) else { return };
    match hardware.light(arranging, stick) {
        Ok(()) => a.model.message.clear(),
        Err(error) => a.model.message = error,
    }
}

fn pick_slot(hwnd: HWND, slot: usize) {
    let Some(a) = app(hwnd) else { return };
    let next = if a.model.pick_slot(slot) { a.model.lit_stick() } else { None };
    if let Some(next) = next {
        light(hwnd, next);
    }
}

fn start_over(hwnd: HWND) {
    if let Some(a) = app(hwnd) {
        a.model.start_over();
    }
    light(hwnd, 0);
}

/// Releases the claim, then re-applies the saved effect, which puts the lights back.
fn restore_lights(hwnd: HWND) -> Result<(), String> {
    if let Some(a) = app(hwnd) {
        a.arranging = None;
    }
    actions::restore_saved(app(hwnd).and_then(|a| a.hardware.as_mut()))
}

/// Cancel, Back or Esc in the picker: nothing is saved.
fn cancel_arrange(hwnd: HWND) {
    show_now(hwnd, "Putting your lights back…");
    let result = restore_lights(hwnd);
    if let Some(a) = app(hwnd) {
        a.model.back();
    }
    finish(hwnd, result, "");
}

/// Saves the order, then puts the lights back. If saving fails, the picker
/// stays open with its claim, so the user can try again or cancel.
fn save_order(hwnd: HWND) {
    let Some(order) = app(hwnd).and_then(|a| a.model.arrange_result()) else { return };
    if system::save_order(&order).is_err() {
        if let Some(a) = app(hwnd) {
            a.model.message = "Couldn't save the stick order. Try again.".to_string();
        }
        return;
    }
    if let Some(a) = app(hwnd) {
        a.model.order = order;
    }
    show_now(hwnd, "Putting your lights back…");
    let result = restore_lights(hwnd);
    if let Some(a) = app(hwnd) {
        a.model.back();
    }
    finish(hwnd, result, "Stick order saved");
}

/// Closing from the picker puts the lights back first.
fn close(hwnd: HWND) {
    if app(hwnd).is_some_and(|a| a.arranging.is_some()) {
        cancel_arrange(hwnd);
    }
    // SAFETY: plain call on our own window.
    unsafe { DestroyWindow(hwnd) };
}

fn tenth(x: f32) -> f32 {
    (x * 10.0).round() / 10.0
}

/// Every 2 s while an effect runs: measure the engine and remember the cost.
/// Whether it runs comes from its claim; a failed measurement only skips a sample.
fn sample_cost(hwnd: HWND) {
    if !system::engine_claimed() {
        // The engine exited by itself, for example after repeated bus errors.
        finish(hwnd, Ok(()), "Background process stopped");
        return;
    }
    let Some(a) = app(hwnd) else { return };
    let Some(effect) = a.model.running else { return };
    let Some(sample) = control::sample_engine() else { return };
    if let Some(previous) = a.last_sample {
        let processors = std::thread::available_parallelism().map_or(1, |n| n.get());
        let cost = control::cost_between(&previous, &sample, processors);
        a.model.live = Some(cost);
        // Saved as shown, and only when it changes, so the file isn't rewritten every 2 s.
        let shown = Cost { mb: tenth(cost.mb), cpu: tenth(cost.cpu) };
        if a.model.costs.get(effect) != Some(shown) {
            a.model.costs.set(effect, shown);
            let _ = system::save_costs(&a.model.costs);
        }
    }
    a.last_sample = Some(sample);
}

fn click(hwnd: HWND, target: Target) {
    match target {
        Target::Close => {
            close(hwnd);
            return;
        }
        Target::ChangeOrder => change_order(hwnd),
        Target::Slot(slot) => pick_slot(hwnd, slot),
        Target::StartOver => start_over(hwnd),
        Target::SaveOrder => save_order(hwnd),
        Target::Cancel => cancel_arrange(hwnd),
        Target::Back => {
            if app(hwnd).is_some_and(|a| a.model.screen == Screen::Arrange) {
                cancel_arrange(hwnd);
            } else if let Some(a) = app(hwnd) {
                a.model.back();
            }
        }
        Target::Color(n) => {
            let Some(current) = app(hwnd).and_then(|a| a.model.color(n)) else { return };
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
            sync_running(hwnd);
        }
        Target::Apply => apply(hwnd),
        Target::Stop => stop(hwnd),
        Target::Preview => {
            let Some(a) = app(hwnd) else { return };
            a.advance_details();
            a.model.toggle_details();
            a.details_from = (Instant::now(), a.model.details_progress);
            // A faster timer while it moves, so the open and close look smooth.
            // SAFETY: plain call on our own window.
            unsafe { SetTimer(hwnd, TIMER_DETAILS, DETAILS_FRAME_MS, None) };
        }
        other => {
            let Some(a) = app(hwnd) else { return };
            match other {
                Target::Effect(e) => a.model.select(e),
                Target::Speed(s) => a.model.set_speed(s),
                Target::Direction(d) => a.model.set_direction(d),
                Target::Random => a.model.toggle_random(),
                Target::Sync => a.model.toggle_sync(),
                Target::AddColor => a.model.add_color(),
                Target::RemoveColor => a.model.remove_color(),
                Target::Reverse => a.model.toggle_reverse(),
                Target::Settings => a.model.open_settings(),
                _ => {}
            }
        }
    }
    invalidate(hwnd);
}

/// Sets `slider` from the pointer's `x`, on its track in the last layout.
fn drag_slider(hwnd: HWND, slider: Slider, x: f32) {
    let Some(a) = app(hwnd) else { return };
    let target = match slider {
        Slider::Speed => Target::SpeedSlider,
        Slider::Brightness => Target::BrightnessSlider,
    };
    let Some(track) = a.layout.as_ref().and_then(|l| l.buttons.iter().find(|b| b.target == target)).map(|b| b.rect) else { return };
    a.model.set_slider(slider, layout::slider_value_at(track, x, slider.range()));
    invalidate(hwnd);
}

/// The slider the arrow keys move: the last one touched, if it's on screen.
fn arrow_slider(hwnd: HWND) -> Option<Slider> {
    let a = app(hwnd)?;
    let slider = a.last_slider?;
    let shown = a.layout.as_ref()?.buttons.iter().any(|b| b.target.slider() == Some(slider));
    (a.model.screen == Screen::App && shown).then_some(slider)
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
                a.advance_details();
                a.layout = Some(a.painter.paint(hdc, rect.right, rect.bottom, a.scale, &a.model, seconds));
            }
            EndPaint(hwnd, &ps);
            0
        }
        WM_ERASEBKGND => 1,
        // A pointing hand over anything clickable; the arrow everywhere else.
        WM_SETCURSOR if (lparam & 0xFFFF) as u32 == HTCLIENT => {
            let mut point = windows_sys::Win32::Foundation::POINT { x: 0, y: 0 };
            GetCursorPos(&mut point);
            windows_sys::Win32::Graphics::Gdi::ScreenToClient(hwnd, &mut point);
            let over = app(hwnd).is_some_and(|a| {
                let (x, y) = (point.x as f32 / a.scale, point.y as f32 / a.scale);
                a.layout.as_ref().and_then(|l| layout::hit(l, x, y)).is_some()
            });
            SetCursor(LoadCursorW(null_mut(), if over { IDC_HAND } else { IDC_ARROW }));
            1
        }
        WM_TIMER => {
            if wparam == TIMER_PREVIEW {
                invalidate(hwnd);
            } else if wparam == TIMER_DETAILS {
                if app(hwnd).is_none_or(|a| a.advance_details()) {
                    KillTimer(hwnd, TIMER_DETAILS);
                }
                invalidate(hwnd);
            } else if wparam == TIMER_COST {
                sample_cost(hwnd);
                invalidate(hwnd);
            }
            0
        }
        WM_LBUTTONDOWN => {
            // A slider follows the pointer from the press until the release.
            let (x, y) = logical(hwnd, lparam);
            let slider = app(hwnd).and_then(|a| a.layout.as_ref().and_then(|l| layout::hit(l, x, y))).and_then(Target::slider);
            if let Some(slider) = slider {
                if let Some(a) = app(hwnd) {
                    a.dragging = Some(slider);
                    a.last_slider = Some(slider);
                }
                SetCapture(hwnd);
                drag_slider(hwnd, slider, x);
                return 0;
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_MOUSEMOVE => {
            if let Some(slider) = app(hwnd).and_then(|a| a.dragging) {
                drag_slider(hwnd, slider, logical(hwnd, lparam).0);
                return 0;
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_CAPTURECHANGED => {
            if let Some(a) = app(hwnd) {
                a.dragging = None;
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_LBUTTONUP => {
            if app(hwnd).is_some_and(|a| a.dragging.is_some()) {
                // Clears `dragging` through WM_CAPTURECHANGED; a drag is never a click.
                ReleaseCapture();
                return 0;
            }
            let (x, y) = logical(hwnd, lparam);
            let target = app(hwnd).and_then(|a| a.layout.as_ref().and_then(|l| layout::hit(l, x, y)));
            if let Some(target) = target {
                click(hwnd, target);
            }
            0
        }
        WM_KEYDOWN => {
            if wparam == VK_ESCAPE as usize {
                // Esc is Back on Settings and Cancel in the picker; elsewhere it closes.
                match app(hwnd).map(|a| a.model.screen) {
                    Some(Screen::Settings) => click(hwnd, Target::Back),
                    Some(Screen::Arrange) => click(hwnd, Target::Cancel),
                    _ => close(hwnd),
                }
            } else if wparam == VK_RETURN as usize
                && app(hwnd).is_some_and(|a| a.model.screen == Screen::App)
            {
                click(hwnd, Target::Apply);
            } else if wparam == VK_LEFT as usize || wparam == VK_RIGHT as usize {
                if let Some(slider) = arrow_slider(hwnd) {
                    let step = if wparam == VK_LEFT as usize { -1 } else { 1 };
                    if let Some(a) = app(hwnd) {
                        a.model.nudge(slider, step);
                    }
                    invalidate(hwnd);
                }
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
        WM_CLOSE => {
            // Alt+F4 and the taskbar: like the close button.
            close(hwnd);
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
        // Lives until after the message loop; the App (and its canvas) is
        // dropped at WM_NCDESTROY, inside the loop.
        let Some(gdi) = openicue_gdi::GdiPlus::start() else { return 1 };

        let instance = GetModuleHandleW(null());
        let class = wide("openiCUE.Window");
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(window_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: instance,
            // Icon id 1, embedded from assets/openicue.res.
            hIcon: LoadIconW(instance, 1 as _),
            hCursor: LoadCursorW(null_mut(), IDC_ARROW),
            hbrBackground: null_mut(),
            lpszMenuName: null(),
            lpszClassName: class.as_ptr(),
            hIconSm: LoadIconW(instance, 1 as _),
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
            last_sample: None,
            arranging: None,
            dragging: None,
            last_slider: None,
            details_from: (Instant::now(), 1.0),
        });
        let raw = Box::into_raw(app);
        (*raw).check();

        let title = wide("openiCUE");
        let hwnd = CreateWindowExW(WS_EX_APPWINDOW, class.as_ptr(), title.as_ptr(),
            WS_POPUP | WS_SYSMENU | WS_MINIMIZEBOX, 0, 0, layout::WIDTH as i32, layout::HEIGHT as i32,
            null_mut(), null_mut(), instance, raw.cast());
        if hwnd.is_null() {
            // Leak `raw` on purpose: if WM_NCCREATE ran before creation failed,
            // WM_NCDESTROY has already freed it, so freeing here could be a
            // double free. The process exits right after.
            drop(gdi);
            return 1;
        }
        sync_running(hwnd);

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
        drop(gdi);
        0
    }
}
