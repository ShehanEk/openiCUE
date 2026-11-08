//! Draws Setup with GDI+ into an off-screen bitmap, then copies it in one go.
use openicue_gdi::{Align, Canvas, FontSpec, Graphics};
use openicue_setup::layout::{self, Font, Layout, Rect, Target};
use openicue_setup::model::{self, Model, Rgb, Screen, StepState, STEPS};
use windows_sys::Win32::Graphics::Gdi::HDC;

const BG: u32 = 0xFF0B0C0E;
const FG: u32 = 0xFFF2F4F5;
const MUTED: u32 = 0xFF9BA3AB;
const PILL: u32 = 0xFF17191D;
/// The unticked checkbox outline, as in the window's toggles.
const PILL_TEXT: u32 = 0xFFC9CED3;
/// The Failed screen's first LED.
const ORANGE: Rgb = Rgb(0xFF, 0xB0, 0x66);
/// Alpha at the centre of an LED glow.
const GLOW_ALPHA: u8 = 0x55;

fn argb(c: Rgb, alpha: u8) -> u32 {
    (u32::from(alpha) << 24) | (u32::from(c.0) << 16) | (u32::from(c.1) << 8) | u32::from(c.2)
}

const fn spec(family: &'static str, size: f32, bold: bool) -> FontSpec {
    FontSpec { family, size, bold }
}

/// Setup's fonts, in `Font` order.
const FONTS: [FontSpec; 6] = [
    spec("Segoe UI Semibold", 13.0, false),
    spec("Segoe UI", 14.0, false),
    spec("Segoe UI", 14.0, true),
    spec("Segoe UI", 13.0, false),
    spec("Segoe UI", 12.0, false),
    spec("Segoe UI Light", 28.0, false),
];

fn index(font: Font) -> usize {
    match font {
        Font::Brand => 0,
        Font::Body => 1,
        Font::BodyBold => 2,
        Font::Small => 3,
        Font::Hint => 4,
        Font::Heading => 5,
    }
}

/// The welcome screen's two items: a name, then a note under it, with the
/// note's height (the PawnIO note can take two lines).
const WELCOME: [(&str, &str, f32); 2] = [
    ("openiCUE", "Controls your RAM lighting.", 18.0),
    ("PawnIO driver", "The official driver that lets openiCUE reach the LEDs. Skipped if you already have it.", 36.0),
];

pub struct Painter {
    canvas: Canvas,
}

impl Painter {
    /// Call while an `openicue_gdi::GdiPlus` is alive.
    pub fn new() -> Painter {
        Painter { canvas: Canvas::new(&FONTS) }
    }

    pub fn paint(&self, hdc: HDC, width: i32, height: i32, scale: f32, model: &Model, seconds: f32) -> Layout {
        self.canvas.frame(hdc, width, height, scale, BG, |g| {
            let layout = layout::layout(model, &|text, font| self.canvas.measure(g, text, index(font)));
            self.draw(g, model, &layout, seconds);
            layout
        })
    }

    fn text(&self, g: Graphics, text: &str, font: Font, rect: Rect, color: u32, align: Align) {
        self.canvas.text(g, text, index(font), rect, color, align);
    }

    fn wrapped(&self, g: Graphics, text: &str, font: Font, rect: Rect, color: u32) {
        self.canvas.wrapped(g, text, index(font), rect, color, Align::Near);
    }

    fn draw(&self, g: Graphics, model: &Model, l: &Layout, seconds: f32) {
        let c = &self.canvas;
        let title = Rect { x: 32.0, y: 0.0, w: 320.0, h: layout::TITLE_H };
        self.text(g, model.window_title(), Font::Brand, title, FG, Align::Near);
        if l.buttons.iter().any(|b| b.target == Target::Close) {
            let (cx, cy) = (l.close.x + l.close.w / 2.0, l.close.y + l.close.h / 2.0);
            c.line(g, MUTED, 1.2, &[(cx - 5.5, cy - 5.5), (cx + 5.5, cy + 5.5)]);
            c.line(g, MUTED, 1.2, &[(cx + 5.5, cy - 5.5), (cx - 5.5, cy + 5.5)]);
        }
        self.strip(g, model, l.strip, seconds);
        self.text(g, model.heading(), Font::Heading, l.heading, FG, Align::Near);

        let content = l.content;
        match model.screen {
            Screen::Welcome => {
                let mut y = content.y;
                for (name, note, note_h) in WELCOME {
                    self.text(g, name, Font::Brand, Rect { y, h: 20.0, ..content }, FG, Align::Near);
                    y += 20.0;
                    self.wrapped(g, note, Font::Small, Rect { y, h: note_h, ..content }, MUTED);
                    y += note_h + 12.0;
                }
            }
            Screen::Progress => {
                for (i, name) in STEPS.iter().enumerate() {
                    let row = Rect { y: content.y + i as f32 * 26.0, h: 26.0, ..content };
                    self.step(g, model.step_state(i), name, row);
                }
            }
            Screen::Done | Screen::Failed | Screen::Removed => {
                if let Some(body) = model.body() {
                    self.wrapped(g, &body, Font::Body, content, MUTED);
                }
            }
            Screen::Uninstall => {
                if let Some(body) = model.body() {
                    self.wrapped(g, &body, Font::Body, Rect { h: 20.0, ..content }, MUTED);
                }
                if let Some(b) = l.buttons.iter().find(|b| b.target == Target::RemovePawnIo) {
                    let note = Rect { y: b.rect.y + b.rect.h, h: 18.0, ..content };
                    self.text(g, "Leave this off if other apps, such as Fan Control, use PawnIO.", Font::Small, note, MUTED, Align::Near);
                }
            }
        }

        self.text(g, model.hint(), Font::Hint, l.hint, MUTED, Align::Near);
        for b in &l.buttons {
            self.button(g, model, b.rect, b.target);
        }
    }

    /// The 10-LED strip: lit cells sweep through the hues; the Failed
    /// screen's first cell is orange; the rest are dark.
    fn strip(&self, g: Graphics, model: &Model, r: Rect, seconds: f32) {
        let c = &self.canvas;
        let w = (r.w - 9.0 * 2.0) / 10.0;
        let cell = |i: usize| Rect { x: r.x + i as f32 * (w + 2.0), y: r.y, w, h: r.h };
        let lit = model.lit_leds();
        let color = |i: usize| -> Option<Rgb> {
            if i < lit {
                Some(model::hsl(i as f32 * 26.0 - seconds * 40.0, 0.9, 0.58))
            } else if model.screen == Screen::Failed && i == 0 {
                Some(ORANGE)
            } else {
                None
            }
        };
        for i in 0..10 {
            if let Some(color) = color(i) {
                c.glow(g, cell(i), argb(color, GLOW_ALPHA));
            }
        }
        for i in 0..10 {
            let fill = color(i).map_or(PILL, |color| argb(color, 0xFF));
            c.rounded(g, cell(i), 2.0, fill);
        }
    }

    /// One progress row: a 16 px icon box, 10 px gap, then the step name.
    fn step(&self, g: Graphics, state: StepState, name: &str, row: Rect) {
        let c = &self.canvas;
        let icon = Rect { x: row.x, y: row.y + row.h / 2.0 - 8.0, w: 16.0, h: 16.0 };
        match state {
            StepState::Done => {
                // The design's 14 px check mark, centred in the 16 px box.
                let (x, y) = (icon.x + 1.0, icon.y + 1.0);
                c.line(g, FG, 1.5, &[(x + 2.0, y + 7.5), (x + 5.2, y + 10.7), (x + 12.0, y + 3.8)]);
            }
            StepState::Active => {
                let dot = Rect { x: icon.x + 4.5, y: icon.y + 4.5, w: 7.0, h: 7.0 };
                c.rounded(g, dot, 3.5, FG);
            }
            StepState::Waiting => {}
        }
        let color = if state == StepState::Waiting { MUTED } else { FG };
        let text = Rect { x: icon.right() + 10.0, w: row.right() - icon.right() - 10.0, ..row };
        self.text(g, name, Font::Body, text, color, Align::Near);
    }

    /// A 15 px box, ticked when `on`, then its label, as the window's toggles.
    fn checkbox(&self, g: Graphics, r: Rect, label: &str, on: bool) {
        let c = &self.canvas;
        let bx = Rect { x: r.x, y: r.y + r.h / 2.0 - 7.5, w: 15.0, h: 15.0 };
        if on {
            c.rounded(g, bx, 3.0, FG);
            c.line(g, BG, 1.6, &[(bx.x + 3.5, bx.y + 7.5), (bx.x + 6.5, bx.y + 10.5), (bx.x + 11.5, bx.y + 4.5)]);
        } else {
            c.rounded(g, bx, 3.0, PILL_TEXT);
            let hole = Rect { x: bx.x + 1.0, y: bx.y + 1.0, w: 13.0, h: 13.0 };
            c.rounded(g, hole, 2.0, BG);
        }
        let text = Rect { x: bx.right() + 10.0, y: r.y, w: r.right() - bx.right() - 10.0, h: r.h };
        self.text(g, label, Font::Body, text, FG, Align::Near);
    }

    fn button(&self, g: Graphics, model: &Model, r: Rect, target: Target) {
        let c = &self.canvas;
        match target {
            Target::Close => {}
            Target::RemovePawnIo => self.checkbox(g, r, "Also remove the PawnIO driver", model.remove_pawnio),
            Target::Primary => {
                c.rounded(g, r, 22.0, FG);
                self.text(g, model.primary().unwrap_or(""), Font::BodyBold, r, BG, Align::Center);
            }
            Target::Secondary => {
                c.rounded(g, r, 22.0, PILL);
                self.text(g, model.secondary().unwrap_or(""), Font::Body, r, FG, Align::Center);
            }
        }
    }
}
