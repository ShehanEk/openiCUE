//! Draws the window with GDI+ into an off-screen bitmap, then copies it in one go.
use openicue::layout::{self, Font, Layout, Rect, Target};
use openicue::model::{self, Model, Screen, Slider};
use openicue::preview;
use openicue_core::effects::Rgb;
use openicue_gdi::{Align, Canvas, FontSpec, Graphics};
use windows_sys::Win32::Graphics::Gdi::HDC;

const BG: u32 = 0xFF0B0C0E;
const FG: u32 = 0xFFF2F4F5;
const MUTED: u32 = 0xFF9BA3AB;
const PILL: u32 = 0xFF17191D;
const PILL_TEXT: u32 = 0xFFC9CED3;
const ORANGE: u32 = 0xFFFFB066;
const DOT_ON_SELECTED: u32 = 0xFFB45A00;
const SWATCH_EDGE: u32 = 0xFF2A2D33;
const DIVIDER: u32 = 0xFF1E2126;
/// An empty slot in the stick-order picker.
const SLOT_EMPTY: u32 = 0xFF121418;
/// Alpha at the centre of an LED glow.
const GLOW_ALPHA: u8 = 0x55;
/// How far a glow reaches past its LED (`Canvas::glow`), with room to spare.
const GLOW_REACH: f32 = 20.0;

/// The open preview's stick drawing: body, ridges, gold pins and light bar.
const STICK_BODY: u32 = 0xFF15171A;
const STICK_RIDGE: u32 = 0xFF191B1F;
const STICK_PINS: u32 = 0xFF6E5C2F;
const LIGHT_BAR: u32 = 0xFF0F1012;

/// One preview row, part way between an LED bar (`k` 0) and a stick (`k` 1).
struct PreviewRow {
    k: f32,
    family: Option<model::Family>,
    row_y: f32,
    row_h: f32,
    body: Rect,
    bar_h: f32,
    /// Each frame LED's rect and weight: 1, or fading to 0 on an RS stick.
    leds: Vec<(Rect, f32)>,
}

fn preview_row(model: &Model, p: Rect, row: usize, row_y: f32) -> PreviewRow {
    let k = model.details_row(row);
    let family = model.slot_family(row);
    let row_h = model.preview_row_h(row);
    let full_h = match family {
        Some(model::Family::ProSl) => 46.0,
        Some(model::Family::Rs) => 42.0,
        _ => layout::PREVIEW_ROW_OPEN,
    };
    let body_h = layout::lerp(16.0, full_h, k).min(row_h);
    let body = Rect { x: p.x, y: row_y + row_h - body_h, w: layout::lerp(p.w, 540.0, k), h: body_h };
    // The light bar: the whole row when closed; 12 px with padding when open.
    let bar_h = layout::lerp(16.0, 12.0, k);
    let pad_x = layout::lerp(0.0, 6.0, k);
    let pad_top = layout::lerp(0.0, 3.0, k);
    // Each LED has 1 px either side, so neighbours sit 2 px apart. Closed, the
    // outer 1 px sits outside the row, so the LEDs line up as they always did.
    let edge = 1.0 - k;
    let bar = Rect {
        x: body.x + pad_x - edge,
        y: body.y + pad_top,
        w: body.w - 2.0 * pad_x + 2.0 * edge,
        h: bar_h - pad_top,
    };
    let six = model::family_leds(family) == 6;
    let weight = |i: usize| if six && [1, 3, 6, 8].contains(&i) { 1.0 - k } else { 1.0 };
    let total: f32 = (0..10).map(weight).sum();
    let unit = (bar.w - 2.0 * total) / total;
    let mut x = bar.x;
    let leds = (0..10)
        .map(|i| {
            let w = weight(i);
            x += w;
            let led = Rect { x, y: bar.y, w: unit * w, h: bar.h };
            x += unit * w + w;
            (led, w)
        })
        .collect();
    PreviewRow { k, family, row_y, row_h, body, bar_h, leds }
}

/// `color` with its alpha replaced.
fn with_alpha(color: u32, alpha: u8) -> u32 {
    (color & 0x00FF_FFFF) | (u32::from(alpha) << 24)
}

/// `r` inside its 1 px outline.
fn inset(r: Rect) -> Rect {
    Rect { x: r.x + 1.0, y: r.y + 1.0, w: r.w - 2.0, h: r.h - 2.0 }
}

fn argb(c: Rgb, alpha: u8) -> u32 {
    (u32::from(alpha) << 24) | (u32::from(c.0) << 16) | (u32::from(c.1) << 8) | u32::from(c.2)
}

const fn spec(family: &'static str, size: f32, bold: bool) -> FontSpec {
    FontSpec { family, size, bold }
}

/// The window's fonts, in `index()` order.
const FONTS: [FontSpec; 12] = [
    spec("Segoe UI Semibold", 13.0, false),
    spec("Segoe UI", 12.0, false),
    spec("Segoe UI", 12.0, false),
    spec("Segoe UI", 14.0, false),
    spec("Segoe UI", 14.0, true),
    spec("Segoe UI", 13.0, false),
    spec("Segoe UI Light", 30.0, false),
    spec("Segoe UI", 13.0, false),
    spec("Segoe UI", 15.0, false),
    spec("Segoe UI Light", 28.0, false),
    spec("Segoe UI Light", 22.0, false),
    // Consolas ships with every Windows, unlike the design's Cascadia Mono.
    spec("Consolas", 13.0, false),
];

pub struct Painter {
    canvas: Canvas,
}

/// Font index for each `Font`, matching the order of `FONTS`.
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
        Font::Heading => 9,
        Font::SlotNumber => 10,
        Font::Mono => 11,
    }
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

    /// Text in `rect`, always vertically centred.
    fn text(&self, g: Graphics, text: &str, font: Font, rect: Rect, color: u32, align: Align) {
        self.canvas.text(g, text, index(font), rect, color, align);
    }

    /// Word-wrapped text from the top of `rect`.
    fn wrapped(&self, g: Graphics, text: &str, font: Font, rect: Rect, color: u32, align: Align) {
        self.canvas.wrapped(g, text, index(font), rect, color, align);
    }

    fn draw(&self, g: Graphics, model: &Model, l: &Layout, seconds: f32) {
        let c = &self.canvas;
        // Settings has "‹ Back" where the other screens have the name.
        if !matches!(model.screen, Screen::Settings | Screen::Report) {
            self.text(g, "openiCUE", Font::Brand, Rect { x: 32.0, y: 0.0, w: 160.0, h: layout::TITLE_H }, FG, Align::Near);
        }
        self.text(g, &model.status_line(), Font::Status, l.status, MUTED, Align::Far);
        let close = l.close;
        let (cx, cy) = (close.x + close.w / 2.0, close.y + close.h / 2.0);
        c.line(g, MUTED, 1.2, &[(cx - 5.5, cy - 5.5), (cx + 5.5, cy + 5.5)]);
        c.line(g, MUTED, 1.2, &[(cx + 5.5, cy - 5.5), (cx - 5.5, cy + 5.5)]);

        match model.screen {
            Screen::NoDriver | Screen::NoSticks => {
                self.text(g, model.error_title(), Font::Big, l.error_title.unwrap(), FG, Align::Center);
                if let Some(body) = l.error_body {
                    self.wrapped(g, model.error_body(), Font::ErrorBody, body, MUTED, Align::Center);
                }
                for b in &l.buttons {
                    if b.target == Target::CheckAgain {
                        c.rounded(g, b.rect, 22.0, PILL);
                        self.text(g, "Check again", Font::Body, b.rect, FG, Align::Center);
                    }
                }
                return;
            }
            Screen::Settings | Screen::Arrange => {
                self.draw_order_screens(g, model, l);
                return;
            }
            Screen::Report => {
                self.draw_report(g, model, l);
                return;
            }
            Screen::App => {}
        }

        if let Some(p) = l.preview {
            self.draw_preview(g, model, p, seconds);
        }
        if let (Some(bar), Some(text), Some(notice)) = (l.notice, l.notice_text, model.unsupported_notice()) {
            c.rounded(g, bar, bar.h / 2.0, SLOT_EMPTY);
            let dot = Rect { x: bar.x + 16.0, y: bar.y + bar.h / 2.0 - 3.5, w: 7.0, h: 7.0 };
            c.rounded(g, dot, 3.5, ORANGE);
            self.text(g, &notice, Font::Body, text, PILL_TEXT, Align::Near);
        }
        if let Some(r) = l.effects_label {
            self.text(g, "Effects", Font::Label, r, MUTED, Align::Near);
        }
        for s in &l.sliders {
            let value = match s.slider {
                Slider::Speed => model.speed_text(),
                Slider::Brightness => model.brightness_text(),
            };
            self.text(g, s.slider.label(), Font::Small, s.label, PILL_TEXT, Align::Near);
            self.text(g, &value, Font::Small, s.value, MUTED, Align::Near);
        }
        if let Some(r) = l.divider {
            c.rounded(g, r, 0.0, DIVIDER);
        }
        if let Some(r) = l.settings_label {
            let label = format!("{} settings", model::display_name(model.settings.effect));
            self.text(g, &label, Font::Label, r, MUTED, Align::Near);
        }
        for group in &l.segment_groups {
            c.rounded(g, *group, 22.0, PILL);
        }
        for b in &l.buttons {
            self.button(g, model, b.rect, b.target);
        }
        let ongoing = model.cost_is_ongoing();
        if let Some(r) = l.cost_line {
            self.text(g, &model.cost_line(), Font::Big, r, if ongoing { ORANGE } else { FG }, Align::Near);
        }
        if let Some(r) = l.cost_body {
            self.text(g, model.cost_body(), Font::Small, r, MUTED, Align::Near);
        }
    }

    /// The LED preview. Closed, it is 4 rows of 10 LEDs. Open, each row becomes
    /// a drawing of the stick in that slot, with its model beside it: an RS
    /// stick's 4 extra LEDs fold away, leaving frame LEDs 0, 2, 4, 5, 7 and 9.
    fn draw_preview(&self, g: Graphics, model: &Model, p: Rect, seconds: f32) {
        let c = &self.canvas;
        let mut frame = preview::colors(&model.settings, seconds);
        // Rows past the supported sticks stay dark while some aren't
        // supported: openiCUE leaves those sticks alone (design, 2026-11-15).
        if !model.unsupported.is_empty() {
            for row in frame.iter_mut().skip(model.sticks) {
                *row = [Rgb(0x17, 0x19, 0x1D); 10];
            }
        }
        let dark = |row: usize| !model.unsupported.is_empty() && row >= model.sticks;
        let mut row_y = p.y;
        let rows: Vec<PreviewRow> = (0..frame.len())
            .map(|row| {
                let r = preview_row(model, p, row, row_y);
                row_y += r.row_h + model.preview_gap();
                r
            })
            .collect();
        for (row, (r, colors)) in rows.iter().zip(&frame).enumerate() {
            let shown = (r.k * 255.0).round() as u8;
            if shown > 0 {
                self.stick_body(g, r.body, shown);
                c.rounded(g, Rect { h: r.bar_h, ..inset(r.body) }, 3.0, with_alpha(LIGHT_BAR, shown));
            }
            // A closed LED glows freely (16 px around it); an open stick keeps
            // its glow inside its body, as the design does. The clip shrinks
            // in between, so each glow is drawn once per frame.
            let reach = GLOW_REACH * (1.0 - r.k);
            let clip = Rect { x: r.body.x + 1.0 - reach, y: r.body.y + 1.0 - reach, w: r.body.w - 2.0 + 2.0 * reach, h: r.body.h - 2.0 + 2.0 * reach };
            let glows = || {
                for (&(led, w), &color) in r.leds.iter().zip(colors) {
                    let alpha = if dark(row) { 0.0 } else { f32::from(GLOW_ALPHA) * w };
                    if alpha >= 1.0 {
                        c.glow(g, led, argb(color, alpha.round() as u8));
                    }
                }
            };
            // Fully closed needs no clip, which keeps the usual frame cheap.
            if r.k > 0.0 { c.clipped(g, clip, glows) } else { glows() }
            for (&(led, w), &color) in r.leds.iter().zip(colors) {
                if w > 0.0 {
                    c.rounded(g, led, 2.0, argb(color, (255.0 * w).round() as u8));
                }
            }
            // The label slides in from 12 px left, inside the room the stick
            // gave up, so it never runs past the content edge.
            let label = model.details_label(row);
            let label_alpha = (label * 255.0).round() as u8;
            if label_alpha > 0 && model.order.stick_at(row).is_some() {
                // Clipped at the sides only, like the design's label box.
                let room = Rect { x: r.body.x + r.body.w, y: r.row_y - 20.0, w: p.x + p.w - (r.body.x + r.body.w), h: r.row_h + 40.0 };
                let x = room.x + 20.0 - 12.0 * (1.0 - label);
                let mid = r.row_y + r.row_h / 2.0;
                let detail = format!("Slot {} · {} LEDs", row + 1, model::family_leds(r.family));
                c.clipped(g, room, || {
                    let name = Rect { x, y: mid - 18.0, w: 236.0, h: 18.0 };
                    self.text(g, model::family_name(r.family), Font::Small, name, with_alpha(FG, label_alpha), Align::Near);
                    let below = Rect { x, y: mid, w: 236.0, h: 16.0 };
                    self.text(g, &detail, Font::Label, below, with_alpha(MUTED, label_alpha), Align::Near);
                });
            }
        }
    }

    /// A stick's outline, heat spreader ridges and gold pins, at `alpha`.
    fn stick_body(&self, g: Graphics, body: Rect, alpha: u8) {
        let c = &self.canvas;
        c.rounded(g, body, 4.0, with_alpha(SWATCH_EDGE, alpha));
        let inner = inset(body);
        c.rounded(g, inner, 3.0, with_alpha(STICK_BODY, alpha));
        // Ridges between the light bar and the pins: 2 px every 20 px.
        let ridges = Rect { y: inner.y + 12.0, h: (inner.h - 16.0).max(0.0), ..inner };
        if ridges.h > 0.0 {
            c.rounded(g, ridges, 0.0, with_alpha(STICK_RIDGE, alpha));
            let mut x = ridges.x + 18.0;
            while x + 2.0 < ridges.right() {
                c.rounded(g, Rect { x, w: 2.0, ..ridges }, 0.0, with_alpha(STICK_BODY, alpha));
                x += 20.0;
            }
        }
        // Gold pins along the bottom edge: 3 px every 5 px.
        let pins = Rect { y: inner.y + inner.h - 4.0, h: 4.0, ..inner };
        if inner.h > 20.0 {
            let mut x = pins.x;
            while x + 3.0 <= pins.right() {
                c.rounded(g, Rect { x, w: 3.0, ..pins }, 0.0, with_alpha(STICK_PINS, alpha));
                x += 5.0;
            }
        }
    }

    /// "Report an unsupported stick".
    fn draw_report(&self, g: Graphics, model: &Model, l: &Layout) {
        let c = &self.canvas;
        if let Some(r) = l.heading {
            self.text(g, model::REPORT_TITLE, Font::Heading, r, FG, Align::Near);
        }
        if let Some(r) = l.body {
            self.wrapped(g, model::REPORT_BODY, Font::Body, r, MUTED, Align::Near);
        }
        if let Some(r) = l.report_box {
            c.rounded(g, r, 12.0, DIVIDER);
            c.rounded(g, inset(r), 11.0, SLOT_EMPTY);
            for (i, line) in model.report_text().lines().enumerate() {
                let row = Rect { x: r.x + 20.0, y: r.y + 16.0 + i as f32 * 20.0, w: r.w - 40.0, h: 20.0 };
                self.text(g, line, Font::Mono, row, PILL_TEXT, Align::Near);
            }
        }
        if let Some(r) = l.privacy {
            // A 16 px padlock to the left of the note.
            let (x, y) = (r.x - 26.0, r.y + 2.0);
            c.line(g, MUTED, 1.2, &[(x + 3.5, y + 7.5), (x + 12.5, y + 7.5), (x + 12.5, y + 14.5), (x + 3.5, y + 14.5), (x + 3.5, y + 7.5)]);
            c.line(g, MUTED, 1.2, &[(x + 5.5, y + 7.5), (x + 5.5, y + 4.5), (x + 6.5, y + 2.8), (x + 8.0, y + 2.3), (x + 9.5, y + 2.8), (x + 10.5, y + 4.5), (x + 10.5, y + 7.5)]);
            self.wrapped(g, model::REPORT_PRIVACY, Font::Small, r, PILL_TEXT, Align::Near);
        }
        if let Some(r) = l.footer {
            self.text(g, model::REPORT_FOOTER, Font::Small, r, MUTED, Align::Near);
        }
        for b in &l.buttons {
            self.button(g, model, b.rect, b.target);
        }
    }

    /// The Settings screen and the "Which slot is lit?" picker.
    fn draw_order_screens(&self, g: Graphics, model: &Model, l: &Layout) {
        let c = &self.canvas;
        if let Some(r) = l.progress {
            self.text(g, &model.arrange_progress(), Font::Label, r, MUTED, Align::Near);
        }
        if let Some(r) = l.heading {
            let heading = if model.screen == Screen::Settings { model::SETTINGS_TITLE } else { model.arrange_heading() };
            self.text(g, heading, Font::Heading, r, FG, Align::Near);
        }
        if let Some(r) = l.body {
            self.wrapped(g, model.arrange_body(), Font::Body, r, MUTED, Align::Near);
        }
        for line in &l.lines {
            c.rounded(g, *line, 0.0, DIVIDER);
        }
        if let Some(r) = l.row_title {
            self.text(g, model::ORDER_TITLE, Font::Body, r, FG, Align::Near);
        }
        if let Some(r) = l.row_note {
            self.wrapped(g, model::ORDER_NOTE, Font::Small, r, MUTED, Align::Near);
        }
        for (r, (number, color)) in l.order_slots.iter().zip(model.order_slots()) {
            let inner = self.slot_body(g, *r, 4.0, PILL);
            self.slot_bar(g, inner, 4.0, 3.0, argb(color, 0xFF), PILL);
            let below = Rect { y: inner.y + 4.0, h: inner.h - 4.0, ..inner };
            self.text(g, &number.to_string(), Font::Label, below, FG, Align::Center);
        }
        for b in &l.buttons {
            self.button(g, model, b.rect, b.target);
        }
    }

    /// A slot's outline and fill. Returns the area inside the 1 px outline.
    fn slot_body(&self, g: Graphics, r: Rect, radius: f32, fill: u32) -> Rect {
        self.canvas.rounded(g, r, radius, SWATCH_EDGE);
        let inner = Rect { x: r.x + 1.0, y: r.y + 1.0, w: r.w - 2.0, h: r.h - 2.0 };
        self.canvas.rounded(g, inner, radius - 1.0, fill);
        inner
    }

    /// The LED bar across the top of a slot, rounded only at the top.
    fn slot_bar(&self, g: Graphics, inner: Rect, height: f32, radius: f32, color: u32, fill: u32) {
        self.canvas.rounded(g, Rect { h: height + radius, ..inner }, radius, color);
        self.canvas.rounded(g, Rect { y: inner.y + height, h: radius, ..inner }, 0.0, fill);
    }

    /// One picker slot: a body with an 8 px LED bar and the stick number
    /// once placed, then "Slot i" underneath.
    fn picker_slot(&self, g: Graphics, model: &Model, r: Rect, slot: usize) {
        let stick = model.slot_stick(slot);
        let fill = if stick.is_some() { PILL } else { SLOT_EMPTY };
        let label = Rect { x: r.x, y: r.y + r.h - 17.0, w: r.w, h: 17.0 };
        let inner = self.slot_body(g, Rect { h: label.y - 10.0 - r.y, ..r }, 6.0, fill);
        let bar = Rect { h: 8.0, ..inner };
        match stick {
            Some(stick) => {
                let color = model::stick_color(stick);
                // The glow stays inside the slot, as the design clips it.
                self.canvas.clipped(g, inner, || self.canvas.glow(g, bar, argb(color, GLOW_ALPHA)));
                self.slot_bar(g, inner, 8.0, 5.0, argb(color, 0xFF), fill);
                let below = Rect { y: inner.y + 8.0, h: inner.h - 8.0, ..inner };
                self.text(g, &(stick + 1).to_string(), Font::SlotNumber, below, FG, Align::Center);
            }
            None => self.slot_bar(g, inner, 8.0, 5.0, DIVIDER, fill),
        }
        let color = if stick.is_some() { FG } else { MUTED };
        self.text(g, &format!("Slot {}", slot + 1), Font::Label, label, color, Align::Center);
    }

    fn pill(&self, g: Graphics, r: Rect, label: &str, font: Font, on: bool) {
        self.canvas.rounded(g, r, r.h / 2.0, if on { FG } else { PILL });
        self.text(g, label, font, r, if on { BG } else { PILL_TEXT }, Align::Center);
    }

    /// A 15 px box, ticked when `on`, then its label.
    fn checkbox(&self, g: Graphics, r: Rect, label: &str, on: bool) {
        let c = &self.canvas;
        let bx = Rect { x: r.x + 8.0, y: r.y + r.h / 2.0 - 7.5, w: 15.0, h: 15.0 };
        if on {
            c.rounded(g, bx, 3.0, FG);
            c.line(g, BG, 1.6, &[(bx.x + 3.5, bx.y + 7.5), (bx.x + 6.5, bx.y + 10.5), (bx.x + 11.5, bx.y + 4.5)]);
        } else {
            c.rounded(g, bx, 3.0, PILL_TEXT);
            let hole = Rect { x: bx.x + 1.0, y: bx.y + 1.0, w: 13.0, h: 13.0 };
            c.rounded(g, hole, 2.0, BG);
        }
        let text = Rect { x: bx.right() + 8.0, y: r.y, w: r.right() - bx.right() - 8.0, h: r.h };
        self.text(g, label, Font::Body, text, PILL_TEXT, Align::Near);
    }

    fn button(&self, g: Graphics, model: &Model, r: Rect, target: Target) {
        let c = &self.canvas;
        let s = &model.settings;
        match target {
            Target::Close | Target::Preview => {}
            Target::ReportIt | Target::CopyReport => {
                let label = if target == Target::ReportIt { "Report it" } else { "Copy report" };
                c.rounded(g, r, 22.0, PILL);
                self.text(g, label, Font::Body, r, FG, Align::Center);
            }
            Target::OpenIssue => {
                c.rounded(g, r, 22.0, FG);
                self.text(g, "Open GitHub issue", Font::BodyBold, r, BG, Align::Center);
            }
            Target::Effect(e) => {
                let on = s.effect == e;
                c.rounded(g, r, 22.0, if on { FG } else { PILL });
                let mut text = r;
                if model.running == Some(e) {
                    let dot = Rect { x: r.x + 18.0, y: r.y + r.h / 2.0 - 3.5, w: 7.0, h: 7.0 };
                    c.rounded(g, dot, 3.5, if on { DOT_ON_SELECTED } else { ORANGE });
                    text = Rect { x: r.x + 15.0, ..r };
                }
                self.text(g, model::display_name(e), Font::Body, text, if on { BG } else { PILL_TEXT }, Align::Center);
            }
            Target::AddColor | Target::RemoveColor => {
                // A 32 px round button inside the 44 px hit area, with a 10 px icon.
                let button = Rect { x: r.x + 6.0, y: r.y + 6.0, w: 32.0, h: 32.0 };
                c.rounded(g, button, 16.0, PILL);
                let (cx, cy) = (r.x + r.w / 2.0, r.y + r.h / 2.0);
                c.line(g, PILL_TEXT, 1.4, &[(cx - 4.0, cy), (cx + 4.0, cy)]);
                if target == Target::AddColor {
                    c.line(g, PILL_TEXT, 1.4, &[(cx, cy - 4.0), (cx, cy + 4.0)]);
                }
            }
            Target::SpeedSlider | Target::BrightnessSlider => {
                let Some(slider) = target.slider() else { return };
                let k = layout::slider_fraction(model.slider_value(slider), slider.range());
                // A 4 px track, filled up to a 14 px thumb.
                let cy = r.y + r.h / 2.0;
                c.rounded(g, Rect { x: r.x, y: cy - 2.0, w: r.w, h: 4.0 }, 2.0, SWATCH_EDGE);
                if k > 0.0 {
                    c.rounded(g, Rect { x: r.x, y: cy - 2.0, w: r.w * k, h: 4.0 }, 2.0, FG);
                }
                let thumb_x = r.x + r.w * k;
                c.rounded(g, Rect { x: thumb_x - 7.0, y: cy - 7.0, w: 14.0, h: 14.0 }, 7.0, FG);
            }
            Target::Reverse => self.checkbox(g, r, "Reverse", s.reverse),
            Target::Color(n) => {
                let color = model.color(n).unwrap_or(s.color1);
                let swatch = Rect { x: r.x + 8.0, y: r.y + 8.0, w: 28.0, h: 28.0 };
                c.rounded(g, swatch, 6.0, SWATCH_EDGE);
                let inner = Rect { x: swatch.x + 1.0, y: swatch.y + 1.0, w: 26.0, h: 26.0 };
                c.rounded(g, inner, 5.0, argb(color, 0xFF));
            }
            Target::Random => self.checkbox(g, r, "Random", s.random),
            Target::Sync => self.checkbox(g, r, "Keep sticks in sync", s.synced),
            Target::Speed(v) => {
                if s.speed == v { c.rounded(g, r, 20.0, FG); }
                self.text(g, model::speed_name(v), Font::Segment, r, if s.speed == v { BG } else { PILL_TEXT }, Align::Center);
            }
            Target::Direction(d) => {
                if s.direction == d { c.rounded(g, r, 20.0, FG); }
                self.text(g, model::direction_name(d), Font::Segment, r, if s.direction == d { BG } else { PILL_TEXT }, Align::Center);
            }
            Target::Stop => {
                c.rounded(g, r, 22.0, PILL);
                self.text(g, "Stop", Font::Body, r, ORANGE, Align::Center);
            }
            Target::Apply => {
                c.rounded(g, r, 22.0, FG);
                self.text(g, model.apply_label(), Font::BodyBold, r, BG, Align::Center);
            }
            Target::CheckAgain => self.pill(g, r, "Check again", Font::Body, false),
            Target::Settings => self.text(g, "Settings", Font::Status, r, PILL_TEXT, Align::Center),
            Target::Back => {
                // An 11 px chevron, then the label, inside 12 px padding.
                let (x, cy) = (r.x + 12.0, r.y + r.h / 2.0);
                let k = 11.0 / 12.0;
                c.line(g, PILL_TEXT, 1.2, &[(x + 8.0 * k, cy - 5.0 * k), (x + 3.0 * k, cy), (x + 8.0 * k, cy + 5.0 * k)]);
                let text = Rect { x: x + 11.0 + 8.0, y: r.y, w: r.right() - x - 19.0, h: r.h };
                self.text(g, "Back", Font::Small, text, PILL_TEXT, Align::Near);
            }
            Target::ChangeOrder | Target::Cancel | Target::StartOver => {
                let label = match target {
                    Target::ChangeOrder => "Change",
                    _ => model.arrange_secondary(),
                };
                c.rounded(g, r, 22.0, PILL);
                self.text(g, label, Font::Body, r, FG, Align::Center);
            }
            Target::SaveOrder => {
                c.rounded(g, r, 22.0, FG);
                self.text(g, "Save", Font::BodyBold, r, BG, Align::Center);
            }
            Target::Slot(i) => self.picker_slot(g, model, r, i),
        }
    }
}
