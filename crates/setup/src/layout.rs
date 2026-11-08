//! Setup window layout in logical pixels. Pure, so it is tested.
use crate::model::{Model, Screen};
pub use openicue_gdi::Rect;

pub const WIDTH: f32 = 560.0;
pub const HEIGHT: f32 = 440.0;
pub const TITLE_H: f32 = 44.0;
const SIDE: f32 = 32.0;
const PILL_H: f32 = 44.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Font {
    /// 13 px semibold: window title, item names.
    Brand,
    /// 14 px: body text and buttons.
    Body,
    /// 14 px bold: the primary button.
    BodyBold,
    /// 13 px: notes under items.
    Small,
    /// 12 px: the hint.
    Hint,
    /// 28 px light: the heading.
    Heading,
    /// 22 px light: the stick number in a picker slot.
    SlotNumber,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Close,
    Primary,
    Secondary,
    RemovePawnIo,
    /// A RAM slot in the picker, from 0 on the left.
    Slot(usize),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Button {
    pub rect: Rect,
    pub target: Target,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Layout {
    pub close: Rect,
    /// The 10-LED strip; the picker has none.
    pub strip: Option<Rect>,
    /// The picker's "Stick k of n" line.
    pub progress: Option<Rect>,
    pub heading: Rect,
    /// Where the screen's content starts (paragraph, list or steps).
    pub content: Rect,
    pub hint: Rect,
    pub buttons: Vec<Button>,
}

pub fn layout(model: &Model, measure: &dyn Fn(&str, Font) -> f32) -> Layout {
    let close = Rect { x: WIDTH - 48.0, y: 0.0, w: 48.0, h: TITLE_H };
    let strip = Rect { x: SIDE, y: TITLE_H + 12.0, w: WIDTH - 2.0 * SIDE, h: 8.0 };
    let heading = Rect { x: SIDE, y: strip.y + strip.h + 28.0, w: WIDTH - 2.0 * SIDE, h: 38.0 };
    let content_y = heading.y + heading.h + 16.0;
    let bottom = HEIGHT - 24.0;
    let mut out = Layout {
        close,
        strip: Some(strip),
        progress: None,
        heading,
        content: Rect { x: SIDE, y: content_y, w: WIDTH - 2.0 * SIDE, h: bottom - PILL_H - 12.0 - content_y },
        hint: Rect { x: SIDE, y: bottom - PILL_H, w: 260.0, h: PILL_H },
        buttons: Vec::new(),
    };
    if model.screen == Screen::Progress {
        // Nothing to press while files are being changed, not even close.
        return out;
    }
    out.buttons.push(Button { rect: close, target: Target::Close });
    if model.screen == Screen::Arrange {
        arrange(&mut out, model, measure);
    }
    if model.screen == Screen::Uninstall {
        // Paragraph (20) + gap 8, then the 44 px checkbox row.
        let rect = Rect { x: SIDE, y: content_y + 28.0, w: 32.0 + measure("Also remove the PawnIO driver", Font::Body), h: PILL_H };
        out.buttons.push(Button { rect, target: Target::RemovePawnIo });
    }
    let mut right = WIDTH - SIDE;
    let mut placed = Vec::new();
    if let Some(label) = model.primary() {
        let w = 56.0 + measure(label, Font::BodyBold);
        placed.push(Button { rect: Rect { x: right - w, y: bottom - PILL_H, w, h: PILL_H }, target: Target::Primary });
        right -= w + 8.0;
    }
    if let Some(label) = model.secondary() {
        let w = 40.0 + measure(label, Font::Body);
        placed.push(Button { rect: Rect { x: right - w, y: bottom - PILL_H, w, h: PILL_H }, target: Target::Secondary });
    }
    placed.reverse();
    if let Some(first) = placed.first() {
        // The hint takes the space left of the buttons, 16 px away.
        out.hint.w = out.hint.w.min(first.rect.x - 16.0 - SIDE);
    }
    out.buttons.extend(placed);
    out
}

/// "Which slot is lit?" at Setup size: progress, heading and body at the
/// top, then one 72 x 150 slot per stick, centred between the body and the
/// buttons. No LED strip.
fn arrange(out: &mut Layout, model: &Model, measure: &dyn Fn(&str, Font) -> f32) {
    out.strip = None;
    let mut y = TITLE_H + 12.0;
    out.progress = Some(Rect { x: SIDE, y, w: WIDTH - 2.0 * SIDE, h: 17.0 });
    y += 17.0 + 6.0;
    out.heading = Rect { y, h: 40.0, ..out.heading };
    y += 40.0 + 10.0;
    let body = model.body().unwrap_or_default();
    let lines = (measure(&body, Font::Body) / 460.0).ceil().clamp(1.0, 3.0);
    out.content = Rect { x: SIDE, y, w: 460.0, h: 20.0 * lines };
    y += out.content.h;

    let buttons_y = HEIGHT - 24.0 - PILL_H;
    let n = model.sticks.max(1);
    let gap = 10.0;
    // The design's 72 px slots, narrower only when 7 or 8 wouldn't fit.
    let w = ((WIDTH - 2.0 * SIDE - gap * (n - 1) as f32) / n as f32).min(72.0);
    let total = n as f32 * w + (n - 1) as f32 * gap;
    let slot_y = y + (buttons_y - y - 150.0) / 2.0;
    for i in 0..model.sticks {
        let rect = Rect { x: (WIDTH - total) / 2.0 + i as f32 * (w + gap), y: slot_y, w, h: 150.0 };
        out.buttons.push(Button { rect, target: Target::Slot(i) });
    }
}

pub fn hit(layout: &Layout, x: f32, y: f32) -> Option<Target> {
    layout.buttons.iter().find(|b| b.rect.contains(x, y)).map(|b| b.target)
}

pub fn is_drag_area(layout: &Layout, x: f32, y: f32) -> bool {
    y < TITLE_H && !layout.close.contains(x, y)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Model, Screen};

    fn measure(text: &str, _: Font) -> f32 {
        text.chars().count() as f32 * 7.0
    }

    fn targets(l: &Layout) -> Vec<Target> {
        l.buttons.iter().map(|b| b.target).collect()
    }

    #[test]
    fn welcome_has_install_and_cancel_on_the_bottom_right() {
        let l = layout(&Model::installing(), &measure);
        assert_eq!(targets(&l), vec![Target::Close, Target::Secondary, Target::Primary]);
        let primary = l.buttons[2].rect;
        assert_eq!(primary.right(), WIDTH - 32.0);
        assert_eq!(primary.y + primary.h, HEIGHT - 24.0);
    }

    #[test]
    fn uninstall_has_the_pawnio_checkbox() {
        let l = layout(&Model::uninstalling(), &measure);
        assert!(targets(&l).contains(&Target::RemovePawnIo));
    }

    #[test]
    fn progress_has_no_buttons_but_close_is_hidden() {
        let mut m = Model::installing();
        m.screen = Screen::Progress;
        let l = layout(&m, &measure);
        assert!(targets(&l).is_empty(), "no way to cancel half way");
    }

    fn picker(n: usize, done: bool) -> Model {
        let mut m = Model::installing();
        m.start_arrange(n);
        if done {
            for slot in 0..n {
                m.pick_slot(slot);
            }
        }
        m
    }

    fn overlaps(a: Rect, b: Rect) -> bool {
        a.x < b.right() && b.x < a.right() && a.y < b.y + b.h && b.y < a.y + a.h
    }

    #[test]
    fn picker_slots_are_centred_and_fit_for_every_stick_count() {
        for n in 2..=8 {
            for done in [false, true] {
                let l = layout(&picker(n, done), &measure);
                let slots: Vec<Rect> = (0..n)
                    .map(|i| l.buttons.iter().find(|b| b.target == Target::Slot(i)).expect("slot").rect)
                    .collect();
                assert!(!targets(&l).contains(&Target::Slot(n)));
                let (left, right) = (slots[0].x, WIDTH - slots[n - 1].right());
                assert!((left - right).abs() < 0.01, "n={n}: {left} vs {right}");
                assert!(left >= 32.0, "n={n}");
                assert!(slots.windows(2).all(|p| p[0].y == p[1].y && (p[1].x - p[0].right() - 10.0).abs() < 0.01));
                if n <= 6 {
                    assert_eq!((slots[0].w, slots[0].h), (72.0, 150.0), "the design's size when it fits");
                }
                assert!(l.strip.is_none(), "no LED strip on the picker");
                let progress = l.progress.expect("progress line");
                assert!(progress.y + progress.h <= l.heading.y);
                assert!(l.heading.y + l.heading.h <= l.content.y);
                assert!(slots[0].y >= l.content.y + l.content.h, "n={n}: slots below the body");

                let t = targets(&l);
                assert_eq!(t.contains(&Target::Primary), done, "Continue only once done");
                assert!(t.contains(&Target::Secondary), "Skip or Start over");
                let bottom: Vec<Rect> = l.buttons.iter()
                    .filter(|b| matches!(b.target, Target::Primary | Target::Secondary))
                    .map(|b| b.rect)
                    .collect();
                assert_eq!(bottom.last().unwrap().right(), WIDTH - 32.0);
                assert!(slots[0].y + slots[0].h <= bottom[0].y, "n={n}: slots above the buttons");

                let mut all = vec![progress, l.heading, l.content, l.hint];
                all.extend(slots.iter().copied());
                all.extend(bottom.iter().copied());
                for (i, a) in all.iter().enumerate() {
                    assert!(a.x >= 0.0 && a.y >= TITLE_H && a.right() <= WIDTH && a.y + a.h <= HEIGHT, "n={n}: {a:?}");
                    for b in &all[i + 1..] {
                        assert!(!overlaps(*a, *b), "n={n} done={done}: {a:?} overlaps {b:?}");
                    }
                }
            }
        }
    }

    #[test]
    fn only_the_picker_hides_the_strip() {
        for m in [Model::installing(), Model::uninstalling()] {
            let l = layout(&m, &measure);
            assert!(l.strip.is_some());
            assert!(l.progress.is_none());
        }
    }

    #[test]
    fn hit_and_drag() {
        let l = layout(&Model::installing(), &measure);
        let p = l.buttons[2].rect;
        assert_eq!(hit(&l, p.x + 1.0, p.y + 1.0), Some(Target::Primary));
        assert!(is_drag_area(&l, 50.0, 20.0));
        assert!(!is_drag_area(&l, WIDTH - 10.0, 20.0));
    }
}
