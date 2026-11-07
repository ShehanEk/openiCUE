//! Draws the window with GDI+ into an off-screen bitmap, then copies it in one go.
use crate::window::rgb_wide;
use openicue::layout::{self, Font, Layout, Rect, Target};
use openicue::model::{self, Model, Screen};
use openicue::preview;
use openicue_core::effects::Rgb;
use std::ptr::null_mut;
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
const DIVIDER: u32 = 0xFF1E2126;

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
        if let Some(r) = l.divider {
            self.rounded(g, r, 0.0, DIVIDER);
        }
        if let Some(r) = l.settings_label {
            let label = format!("{} settings", model::display_name(model.settings.effect));
            self.text(g, &label, Font::Label, r, MUTED, 0);
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

    /// A 15 px box, ticked when `on`, then its label.
    fn checkbox(&self, g: *mut GpGraphics, r: Rect, label: &str, on: bool) {
        let bx = Rect { x: r.x + 8.0, y: r.y + r.h / 2.0 - 7.5, w: 15.0, h: 15.0 };
        if on {
            self.rounded(g, bx, 3.0, FG);
            self.line(g, BG, 1.6, &[(bx.x + 3.5, bx.y + 7.5), (bx.x + 6.5, bx.y + 10.5), (bx.x + 11.5, bx.y + 4.5)]);
        } else {
            self.rounded(g, bx, 3.0, PILL_TEXT);
            let hole = Rect { x: bx.x + 1.0, y: bx.y + 1.0, w: 13.0, h: 13.0 };
            self.rounded(g, hole, 2.0, BG);
        }
        let text = Rect { x: bx.right() + 8.0, y: r.y, w: r.right() - bx.right() - 8.0, h: r.h };
        self.text(g, label, Font::Body, text, PILL_TEXT, 0);
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
            Target::Random => self.checkbox(g, r, "Random", s.random),
            Target::Sync => self.checkbox(g, r, "Keep sticks in sync", s.synced),
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
