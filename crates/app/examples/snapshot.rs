//! Renders the window off-screen to BMP files, for checking it against the
//! design without the hardware or admin rights. Touches no driver.
//!
//! `cargo run -p openicue --example snapshot -- <out dir>`
#[path = "../src/paint.rs"]
mod paint;

use openicue::model::{Family, InfoSummary, Model, Screen};
use openicue_core::effects::{Direction, Effect, Settings};
use openicue_core::order::StickOrder;
use std::ptr::null_mut;
use windows_sys::Win32::Graphics::Gdi::*;

const W: i32 = 840;
const H: i32 = 640;

fn main() {
    let out = std::path::PathBuf::from(std::env::args().nth(1).expect("usage: snapshot <out dir>"));
    std::fs::create_dir_all(&out).unwrap();
    let _gdi = openicue_gdi::GdiPlus::start().expect("GDI+");
    let painter = paint::Painter::new();
    let kits = [
        ("mixed", [Some(Family::Rs), Some(Family::Rs), Some(Family::ProSl), Some(Family::ProSl)]),
        ("pro", [Some(Family::Pro); 4]),
    ];
    for (kit, families) in kits {
        // (name, open, progress)
        for (state, open, progress) in [("closed", false, 1.0), ("opening", true, 0.35), ("open", true, 1.0), ("closing", false, 0.35)] {
            let mut m = Model::new(Settings { effect: Effect::RainbowWave, ..Settings::default() });
            m.sticks = 4;
            m.families = families.to_vec();
            m.order = StickOrder::identity(4);
            m.details_open = open;
            m.details_progress = progress;
            let path = out.join(format!("{kit}-{state}.bmp"));
            render(&painter, &m, &path);
            println!("{}", path.display());
        }
    }
    // The README's screenshots: a wave moving across the sticks.
    let readme = [
        ("readme-main", [Some(Family::Pro); 4], false),
        ("readme-sticks", [Some(Family::Rs), Some(Family::Rs), Some(Family::ProSl), Some(Family::ProSl)], true),
    ];
    for (name, families, open) in readme {
        let settings = Settings { effect: Effect::RainbowWave, direction: Direction::Right, ..Settings::default() };
        let mut m = Model::new(settings);
        m.sticks = 4;
        m.families = families.to_vec();
        m.order = StickOrder::identity(4);
        m.details_open = open;
        m.details_progress = 1.0;
        let path = out.join(format!("{name}.bmp"));
        render(&painter, &m, &path);
        println!("{}", path.display());
    }
    // An unsupported stick: the notice with the tallest settings, and the report.
    for (name, effect, screen) in [
        ("report-notice", Effect::ColorWave, Screen::App),
        ("report-notice-chase", Effect::Chase, Screen::App),
        ("report-screen", Effect::RainbowWave, Screen::Report),
    ] {
        let mut m = Model::new(Settings { effect, ..Settings::default() });
        m.sticks = 3;
        m.families = vec![Some(Family::Pro); 3];
        m.order = StickOrder::identity(3);
        m.os = "Windows 11, build 26200".into();
        m.unsupported = vec![(0x5B, InfoSummary { vendor: 0x1B1C, product_id: 0x0201, firmware: (0, 6, 36), protocol: 4 })];
        m.screen = screen;
        let path = out.join(format!("{name}.bmp"));
        render(&painter, &m, &path);
        println!("{}", path.display());
    }
}

fn render(painter: &paint::Painter, model: &Model, path: &std::path::Path) {
    // SAFETY: a 32-bit top-down DIB section, released before returning.
    unsafe {
        let screen = GetDC(null_mut());
        let dc = CreateCompatibleDC(screen);
        let mut info: BITMAPINFO = std::mem::zeroed();
        info.bmiHeader.biSize = size_of::<BITMAPINFOHEADER>() as u32;
        info.bmiHeader.biWidth = W;
        info.bmiHeader.biHeight = -H;
        info.bmiHeader.biPlanes = 1;
        info.bmiHeader.biBitCount = 32;
        let mut bits = null_mut();
        let bitmap = CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
        let old = SelectObject(dc, bitmap);
        painter.paint(dc, W, H, 1.0, model, 2.0);
        if std::env::var_os("SNAPSHOT_TIME").is_some() {
            let start = std::time::Instant::now();
            for _ in 0..60 {
                painter.paint(dc, W, H, 1.0, model, 2.0);
            }
            println!("  {:.2} ms per frame", start.elapsed().as_secs_f64() * 1000.0 / 60.0);
        }
        let pixels = std::slice::from_raw_parts(bits as *const u8, (W * H * 4) as usize);
        let mut file = Vec::with_capacity(54 + pixels.len());
        let size = 54 + pixels.len() as u32;
        file.extend_from_slice(b"BM");
        file.extend_from_slice(&size.to_le_bytes());
        file.extend_from_slice(&[0; 4]);
        file.extend_from_slice(&54u32.to_le_bytes());
        file.extend_from_slice(&40u32.to_le_bytes());
        file.extend_from_slice(&W.to_le_bytes());
        file.extend_from_slice(&(-H).to_le_bytes());
        file.extend_from_slice(&1u16.to_le_bytes());
        file.extend_from_slice(&32u16.to_le_bytes());
        file.extend_from_slice(&[0; 24]);
        file.extend_from_slice(pixels);
        std::fs::write(path, file).unwrap();
        SelectObject(dc, old);
        DeleteObject(bitmap);
        DeleteDC(dc);
        ReleaseDC(null_mut(), screen);
    }
}
