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
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Close,
    Primary,
    Secondary,
    RemovePawnIo,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Button {
    pub rect: Rect,
    pub target: Target,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Layout {
    pub close: Rect,
    /// The 10-LED strip.
    pub strip: Rect,
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
        strip,
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
    out.buttons.extend(placed);
    out
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

    #[test]
    fn hit_and_drag() {
        let l = layout(&Model::installing(), &measure);
        let p = l.buttons[2].rect;
        assert_eq!(hit(&l, p.x + 1.0, p.y + 1.0), Some(Target::Primary));
        assert!(is_drag_area(&l, 50.0, 20.0));
        assert!(!is_drag_area(&l, WIDTH - 10.0, 20.0));
    }
}
