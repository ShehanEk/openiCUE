//! Preview colors for the 4 x 10 LED grid, ported from the design canvas.
use openicue_core::effects::{Direction, Effect, Rgb, Settings, Speed};
use openicue_core::frames::{self, Frame, STICKS_IN_FRAME};
use openicue_core::ram::LED_COUNT;
use std::f32::consts::PI;

type Color = [f32; 3];

/// An unlit LED in the design.
const OFF: Color = [20.0, 22.0, 25.0];

fn rgb(c: Rgb) -> Color {
    [c.0 as f32, c.1 as f32, c.2 as f32]
}

fn to_rgb(c: Color) -> Rgb {
    let channel = |x: f32| x.round().clamp(0.0, 255.0) as u8;
    Rgb(channel(c[0]), channel(c[1]), channel(c[2]))
}

fn mix(a: Color, b: Color, k: f32) -> Color {
    [a[0] + (b[0] - a[0]) * k, a[1] + (b[1] - a[1]) * k, a[2] + (b[2] - a[2]) * k]
}

/// Dims toward an unlit LED.
fn scale(a: Color, k: f32) -> Color {
    mix(OFF, a, k.clamp(0.0, 1.0))
}

fn modulo(x: f32, m: f32) -> f32 {
    x.rem_euclid(m)
}

/// The design's `hsl(h, 90%, 58%)`.
fn hsl(h: f32) -> Color {
    let h = modulo(h, 360.0) / 60.0;
    let (s, l) = (0.9, 0.58);
    let c = (1.0 - (2.0 * l - 1.0f32).abs()) * s;
    let x = c * (1.0 - (modulo(h, 2.0) - 1.0).abs());
    let (r, g, b) = match h as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    [(r + m) * 255.0, (g + m) * 255.0, (b + m) * 255.0]
}

/// The design's `rgbFromHue`, used for random colors.
fn random_color(h: f32) -> Color {
    let x = modulo(h, 360.0) / 60.0;
    let c = 230.0;
    let y = c * (1.0 - (modulo(x, 2.0) - 1.0).abs());
    let t = match x as u32 {
        0 => [c, y, 0.0],
        1 => [y, c, 0.0],
        2 => [0.0, c, y],
        3 => [0.0, y, c],
        4 => [y, 0.0, c],
        _ => [c, 0.0, y],
    };
    [t[0] + 20.0, t[1] + 20.0, t[2] + 20.0]
}

pub fn colors(settings: &Settings, seconds: f32) -> Frame {
    let s = settings.normalized();
    if s.effect.is_custom() {
        return frames::render(&s, seconds);
    }
    let speed = match s.speed {
        Speed::Slow => 0.6,
        Speed::Medium => 1.2,
        Speed::Fast => 2.4,
    };
    let has_directions = !s.effect.directions().is_empty();
    let sign = if has_directions && matches!(s.direction, Direction::Left | Direction::Up) { -1.0 } else { 1.0 };
    let vertical = has_directions
        && matches!(s.direction, Direction::Up | Direction::Down | Direction::Vertical);
    let p = if s.effect.has_speed() { speed * seconds * sign } else { 0.0 };
    let (a, b) = if s.random {
        (random_color(p * 40.0), random_color(p * 40.0 + 140.0))
    } else {
        (rgb(s.color1), rgb(s.color2))
    };
    let mut frame = [[Rgb(0, 0, 0); LED_COUNT]; STICKS_IN_FRAME];
    for (si, stick) in frame.iter_mut().enumerate() {
        for (i, led) in stick.iter_mut().enumerate() {
            *led = to_rgb(builtin_led(s.effect, si as f32, i as f32, p, a, b, vertical));
        }
    }
    frame
}

fn builtin_led(effect: Effect, s: f32, i: f32, p: f32, a: Color, b: Color, vertical: bool) -> Color {
    let pos = if vertical { s / 3.0 } else { i / 9.0 };
    let seed = modulo((i * 12.9 + s * 78.2).sin() * 43758.5, 1.0);
    match effect {
        Effect::ColorShift => mix(a, b, ((p * 2.0).sin() + 1.0) / 2.0),
        Effect::ColorPulse => {
            let second = modulo((p * 2.0 / PI).floor(), 2.0) != 0.0;
            scale(if second { b } else { a }, (p * 2.0).sin().abs())
        }
        Effect::RainbowWave => hsl(pos * 200.0 - p * 80.0),
        Effect::ColorWave => mix(a, b, ((2.0 * PI * (pos - p * 0.25)).sin() + 1.0) / 2.0),
        Effect::Visor => {
            let c = (p.sin() + 1.0) / 2.0;
            scale(mix(a, b, c), 1.0 - (pos - c).abs() * 4.0)
        }
        Effect::Rain => {
            let f = modulo(p * 0.5 + seed, 1.0);
            scale(mix(a, b, seed), if f < 0.25 { 1.0 - f * 4.0 } else { 0.0 })
        }
        Effect::Marquee => {
            if modulo(i + s * 2.0 + (p * 3.0).floor(), 3.0) == 0.0 { a } else { OFF }
        }
        Effect::Rainbow => hsl(p * 50.0),
        Effect::Sequential => {
            let k = modulo((p * 6.0).floor(), 80.0);
            let order = s * 10.0 + i;
            if k < 40.0 {
                if order <= k { a } else { OFF }
            } else if order > k - 40.0 {
                a
            } else {
                b
            }
        }
        // Static, plus custom effects (never reached: `colors` sends them to `frames`).
        _ => a,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openicue_core::effects::{Direction, Effect, Rgb, Settings};

    fn settings(effect: Effect) -> Settings {
        Settings { effect, ..Settings::default() }.normalized()
    }

    #[test]
    fn static_is_color1_everywhere() {
        let s = Settings { color1: Rgb(9, 8, 7), ..settings(Effect::Static) };
        assert!(colors(&s, 3.0).iter().flatten().all(|&c| c == Rgb(9, 8, 7)));
    }

    #[test]
    fn custom_effects_match_the_engine_exactly() {
        for effect in [Effect::Breathing, Effect::Chase, Effect::Gradient] {
            let s = settings(effect);
            assert_eq!(colors(&s, 2.5), openicue_core::frames::render(&s, 2.5));
        }
    }

    #[test]
    fn rainbow_ignores_the_chosen_colors() {
        let a = settings(Effect::Rainbow);
        let b = Settings { color1: Rgb(1, 1, 1), color2: Rgb(2, 2, 2), ..a };
        assert_eq!(colors(&a, 1.0), colors(&b, 1.0));
    }

    #[test]
    fn direction_changes_the_wave() {
        let up = Settings { direction: Direction::Up, ..settings(Effect::RainbowWave) };
        let down = Settings { direction: Direction::Down, ..up };
        assert_ne!(colors(&up, 1.0), colors(&down, 1.0));
    }

    #[test]
    fn random_replaces_the_chosen_colors() {
        let plain = settings(Effect::ColorShift);
        let random = Settings { random: true, ..plain };
        assert_ne!(colors(&plain, 1.0), colors(&random, 1.0));
    }

    #[test]
    fn marquee_mixes_lit_and_dark_leds() {
        let s = Settings { color1: Rgb(200, 0, 0), ..settings(Effect::Marquee) };
        let frame = colors(&s, 0.0);
        assert!(frame.iter().flatten().any(|&c| c == Rgb(200, 0, 0)));
        assert!(frame.iter().flatten().any(|&c| c == Rgb(20, 22, 25)));
    }

    #[test]
    fn every_built_in_animates_without_panicking() {
        for effect in openicue_core::effects::ALL_EFFECTS {
            for t in [0.0, 0.37, 10.0, 86_399.9] {
                let _ = colors(&settings(effect), t);
            }
        }
    }
}
