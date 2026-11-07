//! Where everything goes, in logical pixels (96 DPI). Pure, so it is tested.
use crate::model::{self, Model, Screen, SPEEDS};
use openicue_core::effects::{Direction, Effect, Speed};

pub const WIDTH: f32 = 840.0;
pub const HEIGHT: f32 = 640.0;
pub const TITLE_H: f32 = 44.0;
const SIDE: f32 = 32.0;
const CONTENT_RIGHT: f32 = WIDTH - SIDE;
const BOTTOM: f32 = HEIGHT - 24.0;
const PILL_H: f32 = 44.0;

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Font {
    /// 13 px semibold, the window title.
    Brand,
    /// 12 px, the status line.
    Status,
    /// 12 px, section labels.
    Label,
    /// 14 px, tiles and buttons.
    Body,
    /// 14 px bold, Apply.
    BodyBold,
    /// 13 px, segmented options.
    Segment,
    /// 30 px light, the cost and error titles.
    Big,
    /// 13 px, the cost explanation.
    Small,
    /// 15 px, error explanations.
    ErrorBody,
    /// 28 px light, the Settings and picker headings.
    Heading,
    /// 22 px light, the stick number in a picker slot.
    SlotNumber,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Close,
    Effect(Effect),
    Color(u8),
    Random,
    /// "Keep sticks in sync", in the settings header.
    Sync,
    Speed(Speed),
    Direction(Direction),
    Stop,
    Apply,
    CheckAgain,
    /// "Settings", in the app screen's title bar.
    Settings,
    /// "‹ Back", in the Settings screen's title bar.
    Back,
    /// "Change", which opens the stick-order picker.
    ChangeOrder,
    /// A RAM slot in the picker, from 0 on the left.
    Slot(usize),
    Cancel,
    StartOver,
    SaveOrder,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Button {
    pub rect: Rect,
    pub target: Target,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Layout {
    pub close: Rect,
    /// The status line, right-aligned in this rect.
    pub status: Rect,
    /// The 4 x 10 LED grid, on the app screen only.
    pub preview: Option<Rect>,
    pub builtin_label: Option<Rect>,
    pub custom_label: Option<Rect>,
    /// A 1 px line between the effect tiles and the selected effect's settings.
    pub divider: Option<Rect>,
    /// "<effect> settings", so the options read as belonging to the selection.
    pub settings_label: Option<Rect>,
    /// Backgrounds of the speed and direction groups.
    pub segment_groups: Vec<Rect>,
    pub cost_line: Option<Rect>,
    pub cost_body: Option<Rect>,
    pub error_title: Option<Rect>,
    pub error_body: Option<Rect>,
    /// "Settings" or "Which slot is lit?".
    pub heading: Option<Rect>,
    /// "Stick k of n", above the picker heading.
    pub progress: Option<Rect>,
    /// The picker's explanation, wrapped.
    pub body: Option<Rect>,
    /// "Stick order", in the Settings row.
    pub row_title: Option<Rect>,
    /// The note under "Stick order", wrapped.
    pub row_note: Option<Rect>,
    /// 1 px lines above and below the Settings row.
    pub lines: Vec<Rect>,
    /// The saved order's small slots, left to right.
    pub order_slots: Vec<Rect>,
    pub buttons: Vec<Button>,
}

/// Places pills left to right, wrapping at the content edge. Returns the bottom.
fn flow(buttons: &mut Vec<Button>, items: &[(Target, f32)], top: f32) -> f32 {
    let (mut x, mut y) = (SIDE, top);
    for &(target, w) in items {
        if x > SIDE && x + w > CONTENT_RIGHT {
            x = SIDE;
            y += PILL_H + 6.0;
        }
        buttons.push(Button { rect: Rect { x, y, w, h: PILL_H }, target });
        x += w + 6.0;
    }
    y + PILL_H
}

/// A segmented group: 2 px padding, 40 px buttons. Returns the group's width.
fn segments(buttons: &mut Vec<Button>, groups: &mut Vec<Rect>, items: &[(Target, f32)], x: f32, y: f32) -> f32 {
    let mut cx = x + 2.0;
    for &(target, w) in items {
        buttons.push(Button { rect: Rect { x: cx, y: y + 2.0, w, h: 40.0 }, target });
        cx += w;
    }
    let width = cx + 2.0 - x;
    groups.push(Rect { x, y, w: width, h: PILL_H });
    width
}

pub fn layout(model: &Model, measure: &dyn Fn(&str, Font) -> f32) -> Layout {
    let close = Rect { x: WIDTH - 48.0, y: 0.0, w: 48.0, h: TITLE_H };
    let mut out = Layout {
        close,
        status: Rect { x: 200.0, y: 0.0, w: close.x - 12.0 - 200.0, h: TITLE_H },
        preview: None,
        builtin_label: None,
        custom_label: None,
        divider: None,
        settings_label: None,
        segment_groups: Vec::new(),
        cost_line: None,
        cost_body: None,
        error_title: None,
        error_body: None,
        heading: None,
        progress: None,
        body: None,
        row_title: None,
        row_note: None,
        lines: Vec::new(),
        order_slots: Vec::new(),
        buttons: vec![Button { rect: close, target: Target::Close }],
    };
    match model.screen {
        Screen::Settings => return settings_screen(out, model, measure),
        Screen::Arrange => return arrange_screen(out, model, measure),
        _ => {}
    }
    if model.screen != Screen::App {
        // Title 40, gap 14, body up to 3 lines (64), gap 28, button 44; centred
        // in the area under the title bar, with the design's 48/92 padding.
        let top = TITLE_H + 48.0 + ((HEIGHT - TITLE_H - 48.0 - 92.0) - 190.0) / 2.0;
        out.error_title = Some(Rect { x: SIDE, y: top, w: WIDTH - 2.0 * SIDE, h: 40.0 });
        out.error_body = Some(Rect { x: (WIDTH - 440.0) / 2.0, y: top + 54.0, w: 440.0, h: 64.0 });
        let w = 48.0 + measure("Check again", Font::Body);
        let rect = Rect { x: (WIDTH - w) / 2.0, y: top + 146.0, w, h: PILL_H };
        out.buttons.push(Button { rect, target: Target::CheckAgain });
        return out;
    }

    // "Settings" in the title bar, left of close; the status line ends before it.
    let w = 24.0 + measure("Settings", Font::Status);
    let settings = Rect { x: close.x - 4.0 - w, y: 0.0, w, h: TITLE_H };
    out.buttons.push(Button { rect: settings, target: Target::Settings });
    out.status.w = settings.x - 12.0 - out.status.x;

    let preview = Rect { x: SIDE, y: TITLE_H + 16.0, w: CONTENT_RIGHT - SIDE, h: 4.0 * 16.0 + 3.0 * 8.0 };
    out.preview = Some(preview);
    let mut y = preview.y + preview.h + 28.0;

    // A running effect's tile has room for its dot. Built-ins kept in sync run too.
    let tile = |e| {
        let dot = if model.running == Some(e) { 15.0 } else { 0.0 };
        (Target::Effect(e), 36.0 + dot + measure(model::display_name(e), Font::Body))
    };
    out.builtin_label = Some(Rect { x: SIDE, y, w: 300.0, h: 16.0 });
    let tiles: Vec<(Target, f32)> = model::builtins().into_iter().map(tile).collect();
    y = flow(&mut out.buttons, &tiles, y + 24.0) + 16.0;

    out.custom_label = Some(Rect { x: SIDE, y, w: 300.0, h: 16.0 });
    let tiles: Vec<(Target, f32)> = model::customs().into_iter().map(tile).collect();
    y = flow(&mut out.buttons, &tiles, y + 24.0) + 20.0;

    // The selected effect's settings, set apart from the tiles (user feedback,
    // 2026-11-07: without this they read as belonging to the background effects).
    out.divider = Some(Rect { x: SIDE, y, w: CONTENT_RIGHT - SIDE, h: 1.0 });
    y += 1.0 + 8.0;
    // Header row: "<effect> settings" on the left, the sync checkbox on the right.
    let mut label_right = SIDE + 400.0;
    if model.show_sync() {
        let w = 8.0 + 15.0 + 8.0 + measure("Keep sticks in sync", Font::Body) + 8.0;
        let rect = Rect { x: CONTENT_RIGHT - w, y, w, h: PILL_H };
        out.buttons.push(Button { rect, target: Target::Sync });
        label_right = label_right.min(rect.x - 8.0);
    }
    out.settings_label = Some(Rect { x: SIDE, y, w: label_right - SIDE, h: PILL_H });
    y += PILL_H + 8.0;

    // Options row.
    let mut x = SIDE;
    if model.show_colors() {
        let mut gap = 0.0;
        if model.show_pickers() {
            out.buttons.push(Button { rect: Rect { x, y, w: 44.0, h: PILL_H }, target: Target::Color(1) });
            x += 44.0;
            if model.show_color2() {
                x += 4.0;
                out.buttons.push(Button { rect: Rect { x, y, w: 44.0, h: PILL_H }, target: Target::Color(2) });
                x += 44.0;
            }
            gap = 4.0;
        }
        if model.show_random() {
            x += gap;
            let w = 8.0 + 15.0 + 8.0 + measure("Random", Font::Body) + 8.0;
            out.buttons.push(Button { rect: Rect { x, y, w, h: PILL_H }, target: Target::Random });
            x += w;
        }
        x += 20.0;
    }
    if model.show_speed() {
        let items: Vec<(Target, f32)> = SPEEDS
            .iter()
            .map(|&s| (Target::Speed(s), 28.0 + measure(model::speed_name(s), Font::Segment)))
            .collect();
        x += segments(&mut out.buttons, &mut out.segment_groups, &items, x, y) + 20.0;
    }
    if !model.directions().is_empty() {
        let items: Vec<(Target, f32)> = model
            .directions()
            .iter()
            .map(|&d| (Target::Direction(d), 24.0 + measure(model::direction_name(d), Font::Segment)))
            .collect();
        segments(&mut out.buttons, &mut out.segment_groups, &items, x, y);
    }

    // Bottom row: cost on the left, Stop and Apply on the right.
    let w = 64.0 + measure(model.apply_label(), Font::BodyBold);
    let apply = Rect { x: CONTENT_RIGHT - w, y: BOTTOM - PILL_H, w, h: PILL_H };
    let mut left_of_buttons = apply.x;
    if model.show_stop() {
        let w = 40.0 + measure("Stop", Font::Body);
        let stop = Rect { x: apply.x - 8.0 - w, y: apply.y, w, h: PILL_H };
        out.buttons.push(Button { rect: stop, target: Target::Stop });
        left_of_buttons = stop.x;
    }
    out.buttons.push(Button { rect: apply, target: Target::Apply });
    let cost_w = left_of_buttons - 24.0 - SIDE;
    out.cost_line = Some(Rect { x: SIDE, y: BOTTOM - 60.0, w: cost_w, h: 40.0 });
    out.cost_body = Some(Rect { x: SIDE, y: BOTTOM - 18.0, w: cost_w, h: 18.0 });
    out
}

/// Lines of wrapped text, from 1 to 3, when `text` is set `width` wide.
fn lines(text: &str, font: Font, width: f32, measure: &dyn Fn(&str, Font) -> f32) -> f32 {
    (measure(text, font) / width).ceil().clamp(1.0, 3.0)
}

/// "‹ Back", "Settings", then one row: "Stick order", its note and the saved
/// order as small slots, with "Change" on the right, between two lines.
fn settings_screen(mut out: Layout, model: &Model, measure: &dyn Fn(&str, Font) -> f32) -> Layout {
    // Chevron 11, gap 8, label, with 12 px padding, from the design's 20 px edge.
    let w = 12.0 + 11.0 + 8.0 + measure("Back", Font::Small) + 12.0;
    out.buttons.push(Button { rect: Rect { x: 20.0, y: 0.0, w, h: TITLE_H }, target: Target::Back });

    let mut y = TITLE_H + 16.0;
    out.heading = Some(Rect { x: SIDE, y, w: CONTENT_RIGHT - SIDE, h: 40.0 });
    y += 40.0 + 24.0;
    let top = Rect { x: SIDE, y, w: CONTENT_RIGHT - SIDE, h: 1.0 };
    y += 1.0 + 16.0;

    let change_w = 40.0 + measure("Change", Font::Body);
    let text_w = CONTENT_RIGHT - change_w - 24.0 - SIDE;
    out.row_title = Some(Rect { x: SIDE, y, w: text_w, h: 20.0 });
    y += 20.0 + 4.0;
    let h = 18.0 * lines(model::ORDER_NOTE, Font::Small, text_w, measure);
    out.row_note = Some(Rect { x: SIDE, y, w: text_w, h });
    y += h + 4.0 + 6.0;
    out.order_slots = (0..model.order.len())
        .map(|p| Rect { x: SIDE + p as f32 * (28.0 + 6.0), y, w: 28.0, h: 40.0 })
        .collect();
    y += 40.0 + 16.0;
    let bottom = Rect { y, ..top };

    let middle = (top.y + 1.0 + bottom.y) / 2.0;
    let rect = Rect { x: CONTENT_RIGHT - change_w, y: middle - PILL_H / 2.0, w: change_w, h: PILL_H };
    out.buttons.push(Button { rect, target: Target::ChangeOrder });
    out.lines = vec![top, bottom];
    out
}

/// "Which slot is lit?": progress, heading and body at the top, one button
/// per slot centred below, and Cancel, or Start over and Save, at the bottom.
fn arrange_screen(mut out: Layout, model: &Model, measure: &dyn Fn(&str, Font) -> f32) -> Layout {
    let mut y = TITLE_H + 16.0;
    out.progress = Some(Rect { x: SIDE, y, w: CONTENT_RIGHT - SIDE, h: 17.0 });
    y += 17.0 + 6.0;
    out.heading = Some(Rect { x: SIDE, y, w: CONTENT_RIGHT - SIDE, h: 40.0 });
    y += 40.0 + 10.0;
    let h = 20.0 * lines(model.arrange_body(), Font::Body, 460.0, measure);
    out.body = Some(Rect { x: SIDE, y, w: 460.0, h });
    y += h;

    let buttons_y = BOTTOM - PILL_H;
    let n = model.arranging.max(1);
    // The design's 96 px slots, narrower only when 7 or 8 wouldn't fit.
    let gap = 16.0;
    let w = ((CONTENT_RIGHT - SIDE - gap * (n - 1) as f32) / n as f32).min(96.0);
    let total = n as f32 * w + (n - 1) as f32 * gap;
    let slot_y = y + (buttons_y - y - 220.0) / 2.0;
    for i in 0..model.arranging {
        let rect = Rect { x: (WIDTH - total) / 2.0 + i as f32 * (w + gap), y: slot_y, w, h: 220.0 };
        out.buttons.push(Button { rect, target: Target::Slot(i) });
    }

    let mut right = CONTENT_RIGHT;
    if model.arrange_done() {
        let w = 56.0 + measure("Save", Font::BodyBold);
        out.buttons.push(Button { rect: Rect { x: right - w, y: buttons_y, w, h: PILL_H }, target: Target::SaveOrder });
        right -= w + 8.0;
    }
    let target = if model.arrange_done() { Target::StartOver } else { Target::Cancel };
    let w = 40.0 + measure(model.arrange_secondary(), Font::Body);
    out.buttons.push(Button { rect: Rect { x: right - w, y: buttons_y, w, h: PILL_H }, target });
    out
}

pub fn hit(layout: &Layout, x: f32, y: f32) -> Option<Target> {
    layout.buttons.iter().find(|b| b.rect.contains(x, y)).map(|b| b.target)
}

/// The title bar, except its buttons, moves the window.
pub fn is_drag_area(layout: &Layout, x: f32, y: f32) -> bool {
    y < TITLE_H && hit(layout, x, y).is_none()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Model, Screen};
    use openicue_core::effects::{Effect, Settings};
    use openicue_core::order::StickOrder;

    fn measure(text: &str, _: Font) -> f32 {
        text.chars().count() as f32 * 7.0
    }

    fn app(effect: Effect) -> Model {
        let mut m = Model::new(Settings::default());
        m.sticks = 4;
        m.select(effect);
        m
    }

    fn targets(l: &Layout) -> Vec<Target> {
        l.buttons.iter().map(|b| b.target).collect()
    }

    #[test]
    fn every_button_fits_the_window_and_none_overlap() {
        for effect in openicue_core::effects::ALL_EFFECTS {
            let mut m = app(effect);
            m.running = Some(Effect::Chase);
            let l = layout(&m, &measure);
            for (i, a) in l.buttons.iter().enumerate() {
                assert!(a.rect.x >= 0.0 && a.rect.right() <= WIDTH, "{:?}", a.target);
                assert!(a.rect.y >= 0.0 && a.rect.y + a.rect.h <= HEIGHT, "{:?}", a.target);
                for b in &l.buttons[i + 1..] {
                    let apart = a.rect.right() <= b.rect.x || b.rect.right() <= a.rect.x
                        || a.rect.y + a.rect.h <= b.rect.y || b.rect.y + b.rect.h <= a.rect.y;
                    assert!(apart, "{:?} overlaps {:?}", a.target, b.target);
                }
            }
        }
    }

    #[test]
    fn settings_sit_below_a_divider_under_every_tile() {
        for effect in openicue_core::effects::ALL_EFFECTS {
            let mut m = app(effect);
            m.running = Some(Effect::Chase);
            for l in [layout(&m, &measure), layout(&m, &|t: &str, _: Font| t.chars().count() as f32 * 14.0)] {
                let divider = l.divider.unwrap();
                let label = l.settings_label.unwrap();
                let tiles_bottom = l.buttons.iter()
                    .filter(|b| matches!(b.target, Target::Effect(_)))
                    .map(|b| b.rect.y + b.rect.h).fold(0.0, f32::max);
                assert!(divider.y > tiles_bottom && label.y > divider.y);
                for b in l.buttons.iter().filter(|b| matches!(b.target,
                    Target::Color(_) | Target::Random | Target::Speed(_) | Target::Direction(_))) {
                    assert!(b.rect.y > label.y + label.h, "{:?} above its label", b.target);
                    assert!(b.rect.y + b.rect.h <= l.cost_line.unwrap().y, "{:?} hits the cost", b.target);
                }
            }
        }
    }

    #[test]
    fn app_screen_has_all_thirteen_tiles_and_the_right_options() {
        let l = layout(&app(Effect::ColorWave), &measure);
        let t = targets(&l);
        let tiles = t.iter().filter(|t| matches!(t, Target::Effect(_))).count();
        assert_eq!(tiles, 13);
        assert!(t.contains(&Target::Color(1)) && t.contains(&Target::Color(2)) && t.contains(&Target::Random));
        assert_eq!(t.iter().filter(|t| matches!(t, Target::Speed(_))).count(), 3);
        assert_eq!(t.iter().filter(|t| matches!(t, Target::Direction(_))).count(), 4);
        assert!(t.contains(&Target::Apply) && !t.contains(&Target::Stop));
    }

    #[test]
    fn sync_sits_in_the_settings_header_for_animated_built_ins_only() {
        for (effect, shown) in [(Effect::ColorWave, true), (Effect::Static, false), (Effect::Breathing, false)] {
            let l = layout(&app(effect), &measure);
            let sync = l.buttons.iter().find(|b| b.target == Target::Sync);
            assert_eq!(sync.is_some(), shown, "{effect:?}");
            let Some(sync) = sync else { continue };
            let (divider, label) = (l.divider.unwrap(), l.settings_label.unwrap());
            assert!(sync.rect.y > divider.y && sync.rect.right() == CONTENT_RIGHT);
            assert_eq!((sync.rect.y, sync.rect.h), (label.y, label.h), "same row as the label");
            assert!(label.right() <= sync.rect.x, "label runs into the checkbox");
            for b in l.buttons.iter().filter(|b| b.target != Target::Sync) {
                let apart = sync.rect.right() <= b.rect.x || b.rect.right() <= sync.rect.x
                    || sync.rect.y + sync.rect.h <= b.rect.y || b.rect.y + b.rect.h <= sync.rect.y;
                assert!(apart, "Sync overlaps {:?}", b.target);
            }
        }
    }

    #[test]
    fn stop_appears_only_while_running() {
        let mut m = app(Effect::Static);
        assert!(!targets(&layout(&m, &measure)).contains(&Target::Stop));
        m.running = Some(Effect::Breathing);
        assert!(targets(&layout(&m, &measure)).contains(&Target::Stop));
    }

    #[test]
    fn wide_text_wraps_tiles_onto_more_rows() {
        let m = app(Effect::Static);
        let narrow = layout(&m, &measure);
        let wide = layout(&m, &|t: &str, _: Font| t.chars().count() as f32 * 14.0);
        let last = |l: &Layout| l.buttons.iter().filter(|b| matches!(b.target, Target::Effect(_))).last().unwrap().rect.y;
        assert!(last(&wide) > last(&narrow));
    }

    #[test]
    fn hit_and_drag_areas() {
        let l = layout(&app(Effect::Static), &measure);
        let apply = l.buttons.iter().find(|b| b.target == Target::Apply).unwrap().rect;
        assert_eq!(hit(&l, apply.x + 1.0, apply.y + 1.0), Some(Target::Apply));
        assert_eq!(hit(&l, WIDTH - 10.0, 10.0), Some(Target::Close));
        assert_eq!(hit(&l, 400.0, 600.0), None);
        assert!(is_drag_area(&l, 100.0, 20.0));
        assert!(!is_drag_area(&l, WIDTH - 10.0, 20.0));
        assert!(!is_drag_area(&l, 100.0, 60.0));
    }

    fn assert_fits_and_apart(l: &Layout) {
        for (i, a) in l.buttons.iter().enumerate() {
            assert!(a.rect.x >= 0.0 && a.rect.right() <= WIDTH, "{:?}", a.target);
            assert!(a.rect.y >= 0.0 && a.rect.y + a.rect.h <= HEIGHT, "{:?}", a.target);
            for b in &l.buttons[i + 1..] {
                let apart = a.rect.right() <= b.rect.x || b.rect.right() <= a.rect.x
                    || a.rect.y + a.rect.h <= b.rect.y || b.rect.y + b.rect.h <= a.rect.y;
                assert!(apart, "{:?} overlaps {:?}", a.target, b.target);
            }
        }
    }

    fn button(l: &Layout, target: Target) -> Rect {
        l.buttons.iter().find(|b| b.target == target).unwrap_or_else(|| panic!("no {target:?}")).rect
    }

    #[test]
    fn settings_sits_in_the_title_bar_left_of_close_on_the_app_only() {
        let l = layout(&app(Effect::Static), &measure);
        let settings = button(&l, Target::Settings);
        assert_eq!((settings.y, settings.h), (0.0, TITLE_H));
        assert!(settings.right() <= l.close.x && l.status.right() <= settings.x);
        assert!(!is_drag_area(&l, settings.x + 1.0, 10.0), "the button isn't a drag handle");
        for screen in [Screen::NoSticks, Screen::Settings, Screen::Arrange] {
            let mut m = app(Effect::Static);
            m.screen = screen;
            m.arranging = 4;
            assert!(!targets(&layout(&m, &measure)).contains(&Target::Settings), "{screen:?}");
        }
    }

    fn settings_screen(order: &[usize]) -> Model {
        let mut m = app(Effect::Static);
        m.order = StickOrder::from_picks(order).unwrap();
        m.open_settings();
        m
    }

    #[test]
    fn settings_screen_has_back_change_and_the_order() {
        for wide in [7.0, 14.0] {
            let m = settings_screen(&[0, 2, 1, 3]);
            let l = layout(&m, &|t: &str, _: Font| t.chars().count() as f32 * wide);
            assert_eq!(targets(&l), vec![Target::Close, Target::Back, Target::ChangeOrder]);
            assert_fits_and_apart(&l);
            let back = button(&l, Target::Back);
            assert!(back.y == 0.0 && back.x < 40.0);
            assert!(!is_drag_area(&l, back.x + 1.0, 10.0));
            let [top, bottom] = l.lines[..] else { panic!("two row lines") };
            let change = button(&l, Target::ChangeOrder);
            assert!(change.right() == CONTENT_RIGHT && change.y > top.y && change.y + change.h < bottom.y);
            assert_eq!(l.order_slots.len(), 4);
            for slot in &l.order_slots {
                assert_eq!((slot.w, slot.h), (28.0, 40.0));
                assert!(slot.y > l.row_note.unwrap().y && slot.y + slot.h < bottom.y);
                assert!(slot.right() < change.x);
            }
            assert!(l.heading.unwrap().y + l.heading.unwrap().h <= top.y);
            assert!(l.preview.is_none());
        }
    }

    fn picker(n: usize, done: bool) -> Model {
        let mut m = app(Effect::Static);
        m.open_settings();
        m.start_arrange(n);
        if done {
            for slot in 0..n {
                m.pick_slot(slot);
            }
        }
        m
    }

    #[test]
    fn picker_slots_are_centred_and_fit_for_every_stick_count() {
        for n in 2..=8 {
            for done in [false, true] {
                let l = layout(&picker(n, done), &measure);
                assert_fits_and_apart(&l);
                let slots: Vec<Rect> = (0..n).map(|i| button(&l, Target::Slot(i))).collect();
                assert!(!targets(&l).contains(&Target::Slot(n)));
                let (left, right) = (slots[0].x, WIDTH - slots[n - 1].right());
                assert!((left - right).abs() < 0.01, "n={n}: {left} vs {right}");
                assert!(left >= SIDE, "n={n}");
                assert!(slots.windows(2).all(|p| p[0].y == p[1].y && (p[1].x - p[0].right() - 16.0).abs() < 0.01));
                if n <= 6 {
                    assert_eq!((slots[0].w, slots[0].h), (96.0, 220.0), "the design's size when it fits");
                }
                let body = l.body.unwrap();
                assert!(slots[0].y >= body.y + body.h, "n={n}");
                let t = targets(&l);
                assert_eq!(t.contains(&Target::Cancel), !done);
                assert_eq!(t.contains(&Target::StartOver), done);
                assert_eq!(t.contains(&Target::SaveOrder), done);
                let last = if done { button(&l, Target::SaveOrder) } else { button(&l, Target::Cancel) };
                assert_eq!(last.right(), CONTENT_RIGHT);
                assert!(slots[0].y + slots[0].h <= last.y);
            }
        }
    }

    #[test]
    fn error_screens_offer_only_check_again_and_close() {
        let mut m = app(Effect::Static);
        m.screen = Screen::NoSticks;
        let l = layout(&m, &measure);
        assert_eq!(targets(&l), vec![Target::Close, Target::CheckAgain]);
        assert!(l.preview.is_none());
    }
}
