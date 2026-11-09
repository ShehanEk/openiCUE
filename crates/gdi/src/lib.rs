//! GDI+ drawing shared by the openiCUE window and Setup.
// Graphics handles passed in only ever come from BeginPaint or the Canvas.
#![allow(clippy::not_unsafe_ptr_arg_deref)]
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
    /// Call while a `GdiPlus` is alive. Font `i` is `specs[i]`.
    pub fn new(specs: &[FontSpec]) -> Canvas {
        let mut families: Vec<(&'static str, *mut GpFontFamily)> = Vec::new();
        let mut fonts = Vec::with_capacity(specs.len());
        for spec in specs {
            let family = match families.iter().find(|(name, _)| *name == spec.family) {
                Some(&(_, f)) => f,
                None => {
                    let mut f = null_mut();
                    // SAFETY: NUL-terminated name, valid out pointer.
                    unsafe { GdipCreateFontFamilyFromName(wide(spec.family).as_ptr(), null_mut(), &mut f) };
                    families.push((spec.family, f));
                    f
                }
            };
            let style = if spec.bold { FontStyleBold } else { FontStyleRegular };
            let mut font = null_mut();
            // SAFETY: `family` came from GdipCreateFontFamilyFromName; valid out pointer.
            unsafe { GdipCreateFont(family, spec.size, style, UnitPixel, &mut font) };
            fonts.push(font);
        }
        let mut typographic = null_mut();
        let mut format = null_mut();
        // SAFETY: valid out pointers; the clone is ours to delete.
        unsafe {
            GdipStringFormatGetGenericTypographic(&mut typographic);
            GdipCloneStringFormat(typographic, &mut format);
        }
        Canvas { families, fonts, format }
    }

    pub fn measure(&self, g: Graphics, text: &str, font: usize) -> f32 {
        let wide = wide(text);
        let mut bounds = RectF { X: 0.0, Y: 0.0, Width: 0.0, Height: 0.0 };
        let area = RectF { X: 0.0, Y: 0.0, Width: 10_000.0, Height: 100.0 };
        // SAFETY: all pointers are live GDI+ objects or valid locals.
        unsafe {
            GdipSetStringFormatAlign(self.format, StringAlignmentNear);
            GdipMeasureString(g, wide.as_ptr(), (wide.len() - 1) as i32, self.fonts[font],
                &area, self.format, &mut bounds, null_mut(), null_mut());
        }
        bounds.Width
    }

    /// Text in `rect`, always vertically centred.
    pub fn text(&self, g: Graphics, text: &str, font: usize, rect: Rect, color: u32, a: Align) {
        self.draw_string(g, text, font, rect, color, a, StringAlignmentCenter);
    }

    /// Word-wrapped, top-aligned text (the error body, setup paragraphs).
    pub fn wrapped(&self, g: Graphics, text: &str, font: usize, rect: Rect, color: u32, a: Align) {
        self.draw_string(g, text, font, rect, color, a, StringAlignmentNear);
    }

    // One private helper behind 	ext and wrapped; a struct would be churn.
    #[allow(clippy::too_many_arguments)]
    fn draw_string(&self, g: Graphics, text: &str, font: usize, rect: Rect, color: u32, a: Align, line_align: i32) {
        let wide = wide(text);
        let mut brush = null_mut();
        // SAFETY: live GDI+ objects; the brush is deleted here.
        unsafe {
            GdipCreateSolidFill(color, &mut brush);
            GdipSetStringFormatAlign(self.format, align(a));
            GdipSetStringFormatLineAlign(self.format, line_align);
            GdipDrawString(g, wide.as_ptr(), (wide.len() - 1) as i32, self.fonts[font],
                &rect_f(rect), self.format, brush.cast());
            GdipDeleteBrush(brush.cast());
        }
    }

    pub fn rounded(&self, g: Graphics, r: Rect, radius: f32, color: u32) {
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

    /// Soft glow around `r`; `center` is the ARGB at the centre, fading to clear.
    pub fn glow(&self, g: Graphics, r: Rect, center: u32) {
        let mut path = null_mut();
        let mut brush = null_mut();
        let edge = center & 0x00FF_FFFF;
        let mut count = 1;
        // SAFETY: live GDI+ objects; path and brush are deleted here.
        unsafe {
            GdipCreatePath(FillModeAlternate, &mut path);
            GdipAddPathEllipse(path, r.x - 16.0, r.y - 16.0, r.w + 32.0, r.h + 32.0);
            GdipCreatePathGradientFromPath(path, &mut brush);
            GdipSetPathGradientCenterColor(brush, center);
            GdipSetPathGradientSurroundColorsWithCount(brush, &edge, &mut count);
            GdipFillPath(g, brush.cast(), path);
            GdipDeleteBrush(brush.cast());
            GdipDeletePath(path);
        }
    }

    pub fn line(&self, g: Graphics, color: u32, width: f32, points: &[(f32, f32)]) {
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

    /// Runs `draw` with drawing clipped to `r`, then removes the clip.
    pub fn clipped<T>(&self, g: Graphics, r: Rect, draw: impl FnOnce() -> T) -> T {
        // SAFETY: live graphics; the clip is reset right after.
        unsafe { GdipSetClipRect(g, r.x, r.y, r.w, r.h, CombineModeReplace) };
        let out = draw();
        // SAFETY: as above.
        unsafe { GdipResetClip(g) };
        out
    }

    /// Draws off-screen at `scale`, then copies to `hdc` in one go.
    pub fn frame<T>(&self, hdc: HDC, width: i32, height: i32, scale: f32, background: u32, draw: impl FnOnce(Graphics) -> T) -> T {
        // SAFETY: standard off-screen buffer; every object is released below.
        unsafe {
            let memory = CreateCompatibleDC(hdc);
            let bitmap = CreateCompatibleBitmap(hdc, width, height);
            let old = SelectObject(memory, bitmap);
            let mut g = null_mut();
            GdipCreateFromHDC(memory, &mut g);
            GdipSetSmoothingMode(g, SmoothingModeAntiAlias);
            GdipSetTextRenderingHint(g, TextRenderingHintClearTypeGridFit);
            GdipGraphicsClear(g, background);
            GdipScaleWorldTransform(g, scale, scale, MatrixOrderPrepend);

            let out = draw(g);

            GdipDeleteGraphics(g);
            BitBlt(hdc, 0, 0, width, height, memory, 0, 0, SRCCOPY);
            SelectObject(memory, old);
            DeleteObject(bitmap);
            DeleteDC(memory);
            out
        }
    }
}

impl Drop for Canvas {
    fn drop(&mut self) {
        // SAFETY: each object was created in `new` and is deleted once.
        unsafe {
            for &font in &self.fonts {
                GdipDeleteFont(font);
            }
            for &(_, family) in &self.families {
                GdipDeleteFontFamily(family);
            }
            GdipDeleteStringFormat(self.format);
        }
    }
}
