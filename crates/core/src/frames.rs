use crate::effects::{Direction, Effect, Rgb, Settings, Speed, MAX_COLORS};
use crate::ram::LED_COUNT;
use std::f32::consts::PI;

pub const STICKS_IN_FRAME: usize = 4;
pub type Frame = [[Rgb; LED_COUNT]; STICKS_IN_FRAME];

fn dim(color: Rgb, k: f32) -> Rgb {
    to_rgb(scale(color_of(color), k))
}

fn wave(x: f32) -> f32 {
    (x.sin() + 1.0) / 2.0
}

fn speed_factor(speed: Speed) -> f32 {
    match speed {
        Speed::Slow => 0.6,
        Speed::Medium => 1.2,
        Speed::Fast => 2.4,
    }
}

/// How fast the animation runs, signed. Software effects use the speed
/// slider and Reverse; the RAM's own effects keep its three speeds.
fn rate(s: &Settings) -> f32 {
    if !s.effect.has_speed() {
        return 0.0;
    }
    if !s.runs_in_software() {
        return speed_factor(s.speed);
    }
    // Effects with a direction choice have no Reverse, so a saved one is ignored.
    let reverse = s.reverse && s.effect.directions().is_empty();
    let sign = if reverse { -1.0 } else { 1.0 };
    1.2 * f32::from(s.speed_pct) / 100.0 * sign
}

/// Colors for every LED at `seconds` since the effect started.
/// Pass normalized settings, so unsupported options are already removed.
pub fn render(settings: &Settings, seconds: f32) -> Frame {
    let mut frame = if settings.effect.is_custom() {
        render_custom(settings, seconds)
    } else {
        render_builtin(settings, seconds)
    };
    // Software effects are dimmed here; Static is dimmed by the RAM, and
    // the preview shows it the same.
    if settings.runs_in_software() || settings.effect == Effect::Static {
        for led in frame.iter_mut().flatten() {
            *led = led.dimmed(settings.brightness);
        }
    }
    frame
}

fn render_custom(settings: &Settings, seconds: f32) -> Frame {
    let p = seconds * rate(settings);
    let (colors, n) = palette_colors(settings, p);
    let pick = |k: f32| to_rgb(palette(&colors, n, k));
    let mut frame = [[Rgb(0, 0, 0); LED_COUNT]; STICKS_IN_FRAME];
    for (s, stick) in frame.iter_mut().enumerate() {
        for (i, led) in stick.iter_mut().enumerate() {
            *led = match settings.effect {
                Effect::Breathing => dim(pick(wave(p * 0.4)), wave(p * 1.5) * 0.95 + 0.05),
                Effect::Chase => {
                    let m = (p * 0.8).rem_euclid(6.0);
                    let position = if m < 3.0 { m } else { 6.0 - m };
                    dim(pick(position / 3.0), 1.0 - (s as f32 - position).abs() * 1.1)
                }
                Effect::Gradient => {
                    let x = i as f32 / 9.0 * PI + s as f32 * 0.6 - p;
                    pick(wave(x))
                }
                _ => Rgb(0, 0, 0),
            };
        }
    }
    frame
}

// Built-in effects, ported from the design canvas (via the window's preview).

type Color = [f32; 3];

/// An unlit LED. Black, since these are drawn on real LEDs.
const OFF: Color = [0.0, 0.0, 0.0];

fn color_of(c: Rgb) -> Color {
    [c.0 as f32, c.1 as f32, c.2 as f32]
}

fn to_rgb(c: Color) -> Rgb {
    let channel = |x: f32| x.round().clamp(0.0, 255.0) as u8;
    Rgb(channel(c[0]), channel(c[1]), channel(c[2]))
}

fn blend(a: Color, b: Color, k: f32) -> Color {
    [a[0] + (b[0] - a[0]) * k, a[1] + (b[1] - a[1]) * k, a[2] + (b[2] - a[2]) * k]
}

/// Dims toward an unlit LED.
fn scale(a: Color, k: f32) -> Color {
    blend(OFF, a, k.clamp(0.0, 1.0))
}

fn modulo(x: f32, m: f32) -> f32 {
    x.rem_euclid(m)
}

/// Interpolates linearly through the first `n` colors as `k` goes from 0 to 1.
/// With 2 colors it is exactly `blend(colors[0], colors[1], k)`.
fn palette(colors: &[Color; 4], n: usize, k: f32) -> Color {
    match n.min(colors.len()) {
        0 => OFF,
        1 => colors[0],
        n => {
            let x = k.clamp(0.0, 1.0) * (n - 1) as f32;
            let i = (x.floor() as usize).min(n - 2);
            blend(colors[i], colors[i + 1], x - i as f32)
        }
    }
}

/// Color `k` of the first `n`, wrapping.
fn nth(colors: &[Color; 4], n: usize, k: i64) -> Color {
    match n.min(colors.len()) {
        0 => OFF,
        n => colors[k.rem_euclid(n as i64) as usize],
    }
}

/// The colors to draw with, and how many of them are used. Random overrides
/// them; the RAM's own effects get colors 1 and 2, as the RAM does.
fn palette_colors(s: &Settings, p: f32) -> ([Color; 4], usize) {
    if s.random {
        return ([random_color(p * 40.0), random_color(p * 40.0 + 140.0), OFF, OFF], 2);
    }
    if s.runs_in_software() {
        return (s.colors().map(color_of), usize::from(s.color_count.min(MAX_COLORS)));
    }
    ([color_of(s.color1), color_of(s.color2), OFF, OFF], 2)
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

/// What one built-in LED needs besides its position.
struct Look {
    effect: Effect,
    p: f32,
    colors: [Color; 4],
    n: usize,
    /// Drawn in software, so the extra settings apply.
    soft: bool,
    vertical: bool,
}

fn render_builtin(s: &Settings, seconds: f32) -> Frame {
    let has_directions = !s.effect.directions().is_empty();
    let sign = if has_directions && matches!(s.direction, Direction::Left | Direction::Up) { -1.0 } else { 1.0 };
    let vertical = has_directions
        && matches!(s.direction, Direction::Up | Direction::Down | Direction::Vertical);
    let p = rate(s) * seconds * sign;
    let (colors, n) = palette_colors(s, p);
    let look = Look { effect: s.effect, p, colors, n, soft: s.runs_in_software(), vertical };
    let mut frame = [[Rgb(0, 0, 0); LED_COUNT]; STICKS_IN_FRAME];
    for (si, stick) in frame.iter_mut().enumerate() {
        for (i, led) in stick.iter_mut().enumerate() {
            *led = to_rgb(builtin_led(&look, si as f32, i as f32));
        }
    }
    frame
}

fn builtin_led(look: &Look, s: f32, i: f32) -> Color {
    let Look { effect, p, n, soft, vertical, .. } = *look;
    let colors = &look.colors;
    let a = colors[0];
    let pos = if vertical { s / 3.0 } else { i / 9.0 };
    let seed = modulo((i * 12.9 + s * 78.2).sin() * 43758.5, 1.0);
    match effect {
        Effect::ColorShift => palette(colors, n, ((p * 2.0).sin() + 1.0) / 2.0),
        Effect::ColorPulse => {
            // Each pulse takes the next color.
            let pulse = (p * 2.0 / PI).floor() as i64;
            scale(nth(colors, n, pulse), (p * 2.0).sin().abs())
        }
        Effect::RainbowWave => hsl(pos * 200.0 - p * 80.0),
        Effect::ColorWave => palette(colors, n, ((2.0 * PI * (pos - p * 0.25)).sin() + 1.0) / 2.0),
        Effect::Visor => {
            let c = (p.sin() + 1.0) / 2.0;
            scale(palette(colors, n, c), 1.0 - (pos - c).abs() * 4.0)
        }
        Effect::Rain => {
            let f = modulo(p * 0.5 + seed, 1.0);
            scale(palette(colors, n, seed), if f < 0.25 { 1.0 - f * 4.0 } else { 0.0 })
        }
        Effect::Marquee => {
            let v = i + s * 2.0 + (p * 3.0).floor();
            if modulo(v, 3.0) != 0.0 {
                OFF
            } else if soft {
                // Lit group `g` keeps its color as it moves.
                nth(colors, n, (v / 3.0).floor() as i64)
            } else {
                a
            }
        }
        Effect::Rainbow => hsl(p * 50.0),
        Effect::Sequential if soft => {
            // Pass `k` fills every LED with color `k % n`, over the color the
            // last pass left. With one color, it fills over black each pass.
            let step = (p * 6.0).floor() as i64;
            let (pass, front) = (step.div_euclid(40), step.rem_euclid(40));
            let order = (s * 10.0 + i) as i64;
            if order <= front {
                nth(colors, n, pass)
            } else if n <= 1 {
                OFF
            } else {
                nth(colors, n, pass - 1)
            }
        }
        Effect::Sequential => {
            let b = colors[1];
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
        // Static, plus custom effects (never reached: `render` draws them itself).
        _ => a,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::effects::ALL_EFFECTS;

    fn settings(effect: Effect) -> Settings {
        Settings {
            effect,
            color1: Rgb(255, 0, 0),
            color2: Rgb(0, 0, 255),
            ..Settings::default()
        }.normalized()
    }

    /// The same effect, run by the RAM rather than in software.
    fn on_ram(effect: Effect) -> Settings {
        Settings { synced: false, ..settings(effect) }
    }

    fn colors(n: u8) -> Settings {
        Settings {
            more_colors: [Rgb(0, 255, 0), Rgb(255, 255, 255)],
            color_count: n,
            ..settings(Effect::Gradient)
        }
    }

    #[test]
    fn breathing_keeps_all_sticks_in_sync() {
        let frame = render(&settings(Effect::Breathing), 1.3);
        assert!(frame.iter().flatten().all(|led| *led == frame[0][0]));
    }

    #[test]
    fn chase_moves_between_sticks() {
        let start = render(&settings(Effect::Chase), 0.0);
        assert_ne!(start[0][0], Rgb(0, 0, 0));
        assert_eq!(start[3][0], Rgb(0, 0, 0));
        let later = render(&settings(Effect::Chase), 5.0);
        assert_ne!(start, later);
    }

    #[test]
    fn gradient_changes_along_the_stick() {
        let frame = render(&settings(Effect::Gradient), 0.0);
        assert_ne!(frame[0][0], frame[0][5]);
    }

    #[test]
    fn built_in_effects_light_the_leds() {
        for effect in ALL_EFFECTS.into_iter().filter(|e| !e.is_custom()) {
            for s in [settings(effect), on_ram(effect)] {
                let lit = [0.0, 0.37, 1.0, 2.5].iter()
                    .any(|&t| render(&s, t).iter().flatten().any(|led| *led != Rgb(0, 0, 0)));
                assert!(lit, "{effect:?} stays black");
            }
        }
    }

    #[test]
    fn static_is_color1_everywhere() {
        let s = Settings { color1: Rgb(9, 8, 7), ..settings(Effect::Static) };
        assert!(render(&s, 3.0).iter().flatten().all(|&c| c == Rgb(9, 8, 7)));
    }

    #[test]
    fn rainbow_ignores_the_chosen_colors() {
        let a = settings(Effect::Rainbow);
        let b = Settings { color1: Rgb(1, 1, 1), color2: Rgb(2, 2, 2), ..a };
        assert_eq!(render(&a, 1.0), render(&b, 1.0));
    }

    #[test]
    fn direction_changes_the_wave() {
        let up = Settings { direction: Direction::Up, ..settings(Effect::RainbowWave) };
        let down = Settings { direction: Direction::Down, ..up };
        assert_ne!(render(&up, 1.0), render(&down, 1.0));
    }

    #[test]
    fn random_replaces_the_chosen_colors() {
        let plain = settings(Effect::ColorShift);
        let random = Settings { random: true, ..plain };
        assert_ne!(render(&plain, 1.0), render(&random, 1.0));
        let more = Settings { color_count: 4, ..random };
        assert_eq!(render(&random, 1.0), render(&more, 1.0), "random ignores the colour count");
    }

    #[test]
    fn marquee_mixes_lit_and_black_leds() {
        let s = Settings { color1: Rgb(200, 0, 0), ..settings(Effect::Marquee) };
        let frame = render(&s, 0.0);
        assert!(frame.iter().flatten().any(|&c| c == Rgb(200, 0, 0)));
        assert!(frame.iter().flatten().any(|&c| c == Rgb(0, 0, 0)));
    }

    #[test]
    fn every_effect_animates_without_panicking() {
        for effect in ALL_EFFECTS {
            for s in [settings(effect), on_ram(effect), Settings { color_count: 4, reverse: true, ..settings(effect) }] {
                for t in [0.0, 0.37, 10.0, 86_399.9] {
                    let _ = render(&s, t);
                }
            }
        }
        // Not normalized: out-of-range counts must not index past the colours.
        let _ = render(&Settings { color_count: 200, ..settings(Effect::ColorWave) }, 1.0);
    }

    #[test]
    fn palette_runs_through_the_first_n_colors() {
        let c = [[0.0, 0.0, 0.0], [100.0, 0.0, 0.0], [100.0, 100.0, 0.0], [0.0, 0.0, 200.0]];
        assert_eq!(palette(&c, 0, 0.5), OFF);
        assert_eq!(palette(&c, 1, 0.7), c[0]);
        assert_eq!(palette(&c, 2, 0.25), blend(c[0], c[1], 0.25));
        assert_eq!(palette(&c, 2, 1.0), c[1]);
        assert_eq!(palette(&c, 3, 0.5), c[1]);
        assert_eq!(palette(&c, 3, 0.75), [100.0, 50.0, 0.0]);
        assert_eq!(palette(&c, 4, 1.0), c[3]);
        assert_eq!(palette(&c, 4, 2.0), c[3], "k is clamped");
        assert_eq!(palette(&c, 4, -1.0), c[0]);
    }

    #[test]
    fn two_colors_draw_as_before() {
        // With the default 2 colours, software blends are colour 1 to colour 2 only.
        let two = colors(2);
        let other = Settings { more_colors: [Rgb(1, 1, 1), Rgb(2, 2, 2)], ..two };
        assert_eq!(render(&two, 1.7), render(&other, 1.7));
        assert_ne!(render(&two, 1.7), render(&colors(4), 1.7));
    }

    #[test]
    fn one_color_gradient_is_that_color() {
        let one = colors(1);
        assert!(render(&one, 2.0).iter().flatten().all(|&c| c == Rgb(255, 0, 0)));
    }

    #[test]
    fn speed_slider_scales_software_time() {
        let base = settings(Effect::ColorWave);
        let double = Settings { speed_pct: 200, ..base };
        assert_eq!(render(&double, 1.0), render(&base, 2.0));
        let quarter = Settings { speed_pct: 25, ..base };
        assert_eq!(render(&quarter, 4.0), render(&base, 1.0));
        // 100 % is the RAM's medium speed, whatever the Slow/Medium/Fast choice.
        let fast = Settings { speed: Speed::Fast, ..base };
        assert_eq!(render(&fast, 1.3), render(&base, 1.3));
    }

    #[test]
    fn reverse_flips_software_effects_without_directions() {
        let forward = settings(Effect::Gradient);
        let back = Settings { reverse: true, ..forward };
        assert_eq!(render(&back, 1.0), render(&forward, -1.0));
        assert_ne!(render(&back, 1.0), render(&forward, 1.0));
        // Effects with a direction use it instead.
        let rain = settings(Effect::Rain);
        assert_eq!(render(&Settings { reverse: true, ..rain }, 1.0), render(&rain, 1.0));
    }

    #[test]
    fn brightness_dims_software_effects_and_static() {
        for effect in [Effect::Gradient, Effect::ColorWave, Effect::Static] {
            let full = settings(effect);
            let half = Settings { brightness: 50, ..full };
            let expected = render(&full, 1.1).map(|stick| stick.map(|c| c.dimmed(50)));
            assert_eq!(render(&half, 1.1), expected, "{effect:?}");
        }
    }

    #[test]
    fn ram_run_preview_ignores_the_extra_settings() {
        for effect in [Effect::ColorWave, Effect::Marquee, Effect::Sequential, Effect::ColorPulse] {
            let plain = on_ram(effect);
            let extra = Settings {
                speed_pct: 300, brightness: 10, reverse: true, color_count: 4,
                more_colors: [Rgb(1, 2, 3), Rgb(4, 5, 6)], ..plain
            };
            assert_eq!(render(&extra, 1.3), render(&plain, 1.3), "{effect:?}");
            let fast = Settings { speed: Speed::Fast, ..plain };
            assert_ne!(render(&fast, 1.3), render(&plain, 1.3), "{effect:?} keeps the RAM's speeds");
        }
    }

    #[test]
    fn marquee_groups_take_turns_through_the_colors() {
        let s = Settings { color_count: 2, ..settings(Effect::Marquee) };
        let lit: Vec<Rgb> = render(&s, 0.0)[0].iter().copied().filter(|&c| c != Rgb(0, 0, 0)).collect();
        // Stick 0, time 0: LEDs 0, 3, 6, 9 are lit, groups 0 to 3.
        assert_eq!(lit, [Rgb(255, 0, 0), Rgb(0, 0, 255), Rgb(255, 0, 0), Rgb(0, 0, 255)]);
        let one = render(&settings(Effect::Marquee), 0.0);
        assert!(one.iter().flatten().all(|&c| c == Rgb(0, 0, 0) || c == Rgb(255, 0, 0)));
    }

    #[test]
    fn sequential_fills_each_color_over_the_last() {
        let s = Settings { color_count: 3, more_colors: [Rgb(0, 255, 0), Rgb(9, 9, 9)], ..settings(Effect::Sequential) };
        // 6 steps per unit of p, p = 1.2 x seconds, so step = floor(7.2 x seconds).
        let at = |step: f32| render(&s, (step + 0.5) / 7.2);
        let (red, blue, green) = (Rgb(255, 0, 0), Rgb(0, 0, 255), Rgb(0, 255, 0));
        // Pass 1, front at LED 5: colour 2 over colour 1.
        let frame = at(45.0);
        assert_eq!((frame[0][5], frame[0][6]), (blue, red));
        // Pass 2: colour 3 over colour 2.
        let frame = at(85.0);
        assert_eq!((frame[0][5], frame[0][6]), (green, blue));
        // Pass 3 wraps to colour 1 over colour 3.
        let frame = at(125.0);
        assert_eq!((frame[0][5], frame[0][6]), (red, green));
        // One colour fills over black.
        let one = Settings { color_count: 1, ..s };
        let frame = render(&one, 45.5 / 7.2);
        assert_eq!((frame[0][5], frame[0][6]), (red, Rgb(0, 0, 0)));
    }

    #[test]
    fn color_pulse_steps_through_every_color() {
        let s = Settings { color_count: 3, more_colors: [Rgb(0, 255, 0), Rgb(9, 9, 9)], ..settings(Effect::ColorPulse) };
        // Pulse k covers p from k x pi/2; p = 1.2 x seconds. Sample each pulse's peak.
        let peak = |k: f32| render(&s, (k + 0.5) * PI / 2.0 / 1.2)[0][0];
        assert_eq!([peak(0.0), peak(1.0), peak(2.0), peak(3.0)],
            [Rgb(255, 0, 0), Rgb(0, 0, 255), Rgb(0, 255, 0), Rgb(255, 0, 0)]);
    }
}
