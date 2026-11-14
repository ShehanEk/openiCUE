//! Puts the sample unsupported-stick report on the clipboard with the app's
//! own copy code, so it can be checked by pasting. Touches no driver, but
//! replaces whatever is on the clipboard.
//!
//! `cargo run -p openicue --example clipcheck`
#[path = "../src/shell.rs"]
#[allow(dead_code, reason = "only the copy and OS label are checked here")]
mod shell;

use openicue::model::{InfoSummary, Model};
use openicue_core::effects::Settings;
use std::ptr::null_mut;
use windows_sys::Win32::UI::WindowsAndMessaging::{CreateWindowExW, DestroyWindow, HWND_MESSAGE};

fn main() {
    let mut m = Model::new(Settings::default());
    m.sticks = 3;
    m.os = shell::os_label();
    m.unsupported = vec![(0x5B, InfoSummary { vendor: 0x1B1C, product_id: 0x0201, firmware: (0, 6, 36), protocol: 4 })];
    // The clipboard needs a window to own it; a message-only one is enough.
    let class = openicue_gdi::wide("STATIC");
    // SAFETY: a message-only window, destroyed below.
    let hwnd = unsafe { CreateWindowExW(0, class.as_ptr(), null_mut(), 0, 0, 0, 0, 0, HWND_MESSAGE, null_mut(), null_mut(), null_mut()) };
    assert!(!hwnd.is_null(), "no window");
    let copied = shell::copy_text(hwnd, &m.report_text());
    // SAFETY: our own window.
    unsafe { DestroyWindow(hwnd) };
    println!("copied: {copied}");
    println!("link ({} chars): {}", m.report_url().len(), m.report_url());
}
