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
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Close,
    Effect(Effect),
    Color(u8),
    Random,
    Speed(Speed),
    Direction(Direction),
    Stop,
    Apply,
    CheckAgain,
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
    /// Backgrounds of the speed and direction groups.
    pub segment_groups: Vec<Rect>,
    pub cost_line: Option<Rect>,
    pub cost_body: Option<Rect>,
    pub error_title: Option<Rect>,
    pub error_body: Option<Rect>,
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
        segment_groups: Vec::new(),
        cost_line: None,
        cost_body: None,
        error_title: None,
        error_body: None,
        buttons: vec![Button { rect: close, target: Target::Close }],
    };
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

    let preview = Rect { x: SIDE, y: TITLE_H + 16.0, w: CONTENT_RIGHT - SIDE, h: 4.0 * 16.0 + 3.0 * 8.0 };
    out.preview = Some(preview);
    let mut y = preview.y + preview.h + 28.0;

    out.builtin_label = Some(Rect { x: SIDE, y, w: 300.0, h: 16.0 });
    let tiles: Vec<(Target, f32)> = model::builtins()
        .into_iter()
        .map(|e| (Target::Effect(e), 36.0 + measure(model::display_name(e), Font::Body)))
        .collect();
    y = flow(&mut out.buttons, &tiles, y + 24.0) + 16.0;

    out.custom_label = Some(Rect { x: SIDE, y, w: 300.0, h: 16.0 });
    let tiles: Vec<(Target, f32)> = model::customs()
        .into_iter()
        .map(|e| {
            let dot = if model.running == Some(e) { 15.0 } else { 0.0 };
            (Target::Effect(e), 36.0 + dot + measure(model::display_name(e), Font::Body))
        })
        .collect();
    y = flow(&mut out.buttons, &tiles, y + 24.0) + 20.0;

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

pub fn hit(layout: &Layout, x: f32, y: f32) -> Option<Target> {
    layout.buttons.iter().find(|b| b.rect.contains(x, y)).map(|b| b.target)
}

/// The title bar, except the close button, moves the window.
pub fn is_drag_area(layout: &Layout, x: f32, y: f32) -> bool {
    y < TITLE_H && !layout.close.contains(x, y)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Model, Screen};
    use openicue_core::effects::{Effect, Settings};

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

    #[test]
    fn error_screens_offer_only_check_again_and_close() {
        let mut m = app(Effect::Static);
        m.screen = Screen::NoSticks;
        let l = layout(&m, &measure);
        assert_eq!(targets(&l), vec![Target::Close, Target::CheckAgain]);
        assert!(l.preview.is_none());
    }
}
