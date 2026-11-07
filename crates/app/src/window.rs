//! The frameless window: messages, DPI, timers and clicks.
use crate::actions::{self, Startup};
use crate::paint::Painter;
use openicue::layout::{self, Layout, Target};
use openicue::model::{Model, Screen};
use openicue_core::effects::Rgb;
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
#[allow(dead_code)]
const TIMER_COST: usize = 2;
const PREVIEW_MS: u32 = 90;
#[allow(dead_code)]
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
            // Leak `raw` on purpose: if WM_NCCREATE ran before creation failed,
            // WM_NCDESTROY has already freed it, so freeing here could be a
            // double free. The process exits right after.
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
