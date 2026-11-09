//! The frameless Setup window: messages, DPI, the timer, clicks, and the
//! worker thread's events.
use crate::install::{self, Event};
use crate::paint::Painter;
use openicue_core::system;
use openicue_gdi::wide;
use openicue_setup::layout::{self, Layout, Target};
use openicue_setup::model::{Failure, Model, Screen};
use std::panic::AssertUnwindSafe;
use std::ptr::{null, null_mut};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::Instant;
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Dwm::DwmSetWindowAttribute;
use windows_sys::Win32::Graphics::Gdi::{BeginPaint, EndPaint, InvalidateRect, ScreenToClient, UpdateWindow, PAINTSTRUCT};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::HiDpi::{GetDpiForWindow, SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::VK_ESCAPE;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

const WM_APP_EVENT: u32 = WM_APP + 1;
const TIMER_ANIMATE: usize = 1;
const ANIMATE_MS: u32 = 90;
const DWMWA_USE_IMMERSIVE_DARK_MODE: u32 = 20;
const DWMWA_WINDOW_CORNER_PREFERENCE: u32 = 33;
const DWMWCP_ROUND: u32 = 2;

pub struct App {
    pub model: Model,
    pub painter: Painter,
    pub layout: Option<Layout>,
    pub start: Instant,
    pub scale: f32,
    /// Install or uninstall events from the worker thread.
    pub events: Receiver<Event>,
    pub sender: Sender<Event>,
    /// True once Uninstall is pressed, so `Done` shows the Removed screen.
    pub uninstalling: bool,
    /// The driver, engine claim and sticks while the stick-order picker is
    /// open. Dropping it sets the sticks black and releases the claim.
    pub picker: Option<install::Picker>,
}

impl App {
    fn seconds(&self) -> f32 {
        (self.start.elapsed().as_secs_f64() % 86_400.0) as f32
    }
}

/// The App lives in GWLP_USERDATA from WM_NCCREATE until WM_NCDESTROY.
/// Borrow it only briefly: never across a call that runs a message loop or
/// destroys the window.
fn app<'a>(hwnd: HWND) -> Option<&'a mut App> {
    // SAFETY: the pointer is either null or the leaked Box set in WM_NCCREATE.
    unsafe { (GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut App).as_mut() }
}

fn invalidate(hwnd: HWND) {
    // SAFETY: plain call on our own window.
    unsafe { InvalidateRect(hwnd, null(), 0) };
}

fn destroy(hwnd: HWND) {
    // SAFETY: plain call on our own window. It frees the App (WM_NCDESTROY),
    // so no App borrow may be live here.
    unsafe { DestroyWindow(hwnd) };
}

/// Moves to Progress and runs `work` on a worker thread. The thread only
/// sends events and posts WM_APP_EVENT; the window handle crosses as an
/// `isize`, since `HWND` is not `Send`.
fn start_work(hwnd: HWND, work: impl FnOnce(Box<dyn Fn(Event)>) + Send + 'static) {
    let Some(a) = app(hwnd) else { return };
    a.model.screen = Screen::Progress;
    a.model.step = 0;
    let sender = a.sender.clone();
    let window = hwnd as isize;
    std::thread::spawn(move || {
        let post = move |event: Event| {
            let _ = sender.send(event);
            // SAFETY: PostMessageW may be called from any thread; a stale
            // handle only makes it fail.
            unsafe { PostMessageW(window as HWND, WM_APP_EVENT, 0, 0) };
        };
        // A panic must not leave the window stuck on Progress. (Release
        // builds abort on panic, which closes Setup instead.)
        let report = post.clone();
        let result = std::panic::catch_unwind(AssertUnwindSafe(move || work(Box::new(report))));
        if result.is_err() {
            post(Event::Failed(Failure::Other("Setup hit an unexpected error. Run it again.".into())));
        }
    });
}

/// Drains the worker's events into the model.
fn on_events(hwnd: HWND) {
    let mut installed = false;
    if let Some(a) = app(hwnd) {
        while let Ok(event) = a.events.try_recv() {
            match event {
                Event::Step(i) => a.model.step = i,
                Event::Done if a.uninstalling => {
                    a.model.step = 4;
                    a.model.screen = Screen::Removed;
                }
                Event::Done => {
                    // Stays on Progress, all ticked, until the picker is decided.
                    a.model.step = 4;
                    installed = true;
                }
                Event::Failed(failure) => {
                    a.model.screen = Screen::Failed;
                    a.model.failure = Some(failure);
                }
            }
        }
    }
    invalidate(hwnd);
    if installed {
        // Show the ticked steps while the RAM is read, then ask the stick
        // order, or go straight to Done when there is nothing to ask.
        // SAFETY: plain call on our own window; no App borrow is live.
        unsafe { UpdateWindow(hwnd) };
        let picker = install::Picker::begin();
        if let Some(a) = app(hwnd) {
            match picker {
                Some(picker) => {
                    a.model.start_arrange(picker.count());
                    a.picker = Some(picker);
                }
                None => {
                    a.model.screen = Screen::Done;
                    install::start_restore();
                }
            }
        }
        invalidate(hwnd);
    }
}

/// Leaves the picker: drops it, which sets the sticks black and releases
/// the claim, then shows Done and puts the saved effect back.
fn end_picker(hwnd: HWND) {
    if let Some(a) = app(hwnd) {
        a.picker = None;
        a.model.screen = Screen::Done;
        install::start_restore();
    }
}

/// Lights `stick` white. If that fails, the picker ends without saving, so
/// the user is never asked about a stick that isn't lit.
fn light(hwnd: HWND, stick: usize) {
    let lit = app(hwnd).and_then(|a| a.picker.as_mut()).is_some_and(|p| p.light(stick).is_ok());
    if !lit {
        end_picker(hwnd);
    }
}

/// Close, Esc and Alt+F4: put the lights out first if the picker is open.
fn close(hwnd: HWND) {
    // The picker only opens after a successful install, so the effect is
    // put back once it has released the claim.
    let picker = app(hwnd).and_then(|a| a.picker.take());
    if picker.is_some() {
        // Sets the sticks black and releases the claim before the engine starts.
        drop(picker);
        install::start_restore();
    }
    destroy(hwnd);
}

fn click(hwnd: HWND, target: Target) {
    let Some((screen, done)) = app(hwnd).map(|a| (a.model.screen, a.model.arrange_done())) else { return };
    match (target, screen) {
        (Target::Close, _) => return close(hwnd),
        (Target::Slot(slot), Screen::Arrange) => {
            let next = app(hwnd).and_then(|a| if a.model.pick_slot(slot) { a.model.lit_stick() } else { None });
            if let Some(next) = next {
                light(hwnd, next);
            }
        }
        (Target::Secondary, Screen::Arrange) if done => {
            if let Some(a) = app(hwnd) {
                a.model.start_over();
            }
            light(hwnd, 0);
        }
        // Skip: nothing is saved.
        (Target::Secondary, Screen::Arrange) => end_picker(hwnd),
        (Target::Primary, Screen::Arrange) if done => {
            // A failed save only loses the order; the user can set it
            // again in openiCUE's Settings.
            if let Some(order) = app(hwnd).and_then(|a| a.model.arrange_result()) {
                let _ = system::save_order(&order);
            }
            end_picker(hwnd);
        }
        (Target::Slot(_), _) | (Target::Primary, Screen::Arrange) => {}
        (Target::Secondary, _) => return destroy(hwnd),
        (Target::RemovePawnIo, _) => {
            if let Some(a) = app(hwnd) {
                a.model.remove_pawnio = !a.model.remove_pawnio;
            }
        }
        (Target::Primary, Screen::Welcome) => {
            start_work(hwnd, install::run_install);
        }
        (Target::Primary, Screen::Uninstall) => {
            let Some(remove_pawnio) = app(hwnd).map(|a| {
                a.uninstalling = true;
                a.model.remove_pawnio
            }) else { return };
            start_work(hwnd, move |report| install::run_uninstall(remove_pawnio, report));
        }
        (Target::Primary, Screen::Done) => {
            // Close either way: if the start fails, openiCUE is in the Start menu.
            let _ = install::open_app();
            return destroy(hwnd);
        }
        (Target::Primary, Screen::Failed | Screen::Removed) => return destroy(hwnd),
        (Target::Primary, Screen::Progress) => {}
    }
    invalidate(hwnd);
}

fn logical(hwnd: HWND, lparam: LPARAM) -> (f32, f32) {
    let x = (lparam & 0xFFFF) as i16 as f32;
    let y = ((lparam >> 16) & 0xFFFF) as i16 as f32;
    let scale = app(hwnd).map_or(1.0, |a| a.scale);
    (x / scale, y / scale)
}

fn on_progress(hwnd: HWND) -> bool {
    app(hwnd).is_some_and(|a| a.model.screen == Screen::Progress)
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
            let mut point = POINT { x: (lparam & 0xFFFF) as i16 as i32, y: ((lparam >> 16) & 0xFFFF) as i16 as i32 };
            ScreenToClient(hwnd, &mut point);
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
            if wparam == TIMER_ANIMATE {
                invalidate(hwnd);
            }
            0
        }
        WM_APP_EVENT => {
            on_events(hwnd);
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
            // Esc closes, except while files are being changed.
            if wparam == VK_ESCAPE as usize && !on_progress(hwnd) {
                close(hwnd);
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
            // Alt+F4 and the taskbar. Files are half-changed during
            // Progress, so the window must not close then.
            if !on_progress(hwnd) {
                close(hwnd);
            }
            0
        }
        WM_ENDSESSION => {
            // Logoff or shutdown: the process may end without WM_DESTROY,
            // so put the picker's lights out now.
            if wparam != 0 {
                if let Some(a) = app(hwnd) {
                    a.picker = None;
                }
            }
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

pub fn run(uninstall: bool) -> i32 {
    // SAFETY: all calls below are standard window setup; pointers are to
    // live locals or statics, and every handle belongs to this process.
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        // Lives until after the message loop; the App (and its canvas) is
        // dropped at WM_NCDESTROY, inside the loop.
        let Some(gdi) = openicue_gdi::GdiPlus::start() else { return 1 };

        let instance = GetModuleHandleW(null());
        let class = wide("openiCUE.Setup");
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

        let model = if uninstall { Model::uninstalling() } else { Model::installing() };
        let title = wide(model.window_title());
        let (sender, events) = channel();
        let app = Box::new(App {
            model,
            painter: Painter::new(),
            layout: None,
            start: Instant::now(),
            scale: 1.0,
            events,
            sender,
            uninstalling: false,
            picker: None,
        });
        let raw = Box::into_raw(app);

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

        let scale = GetDpiForWindow(hwnd) as f32 / 96.0;
        (*raw).scale = scale;
        let (w, h) = ((layout::WIDTH * scale) as i32, (layout::HEIGHT * scale) as i32);
        let (sw, sh) = (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN));
        SetWindowPos(hwnd, null_mut(), (sw - w) / 2, (sh - h) / 2, w, h, SWP_NOZORDER);

        let on: u32 = 1;
        DwmSetWindowAttribute(hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE, (&on as *const u32).cast(), 4);
        DwmSetWindowAttribute(hwnd, DWMWA_WINDOW_CORNER_PREFERENCE, (&DWMWCP_ROUND as *const u32).cast(), 4);

        SetTimer(hwnd, TIMER_ANIMATE, ANIMATE_MS, None);
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
