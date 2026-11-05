use crate::effects::{Effect, Rgb, Settings, Speed};
use crate::ram::LED_COUNT;

pub const STICKS_IN_FRAME: usize = 4;
pub type Frame = [[Rgb; LED_COUNT]; STICKS_IN_FRAME];

fn mix(a: Rgb, b: Rgb, k: f32) -> Rgb {
    let k = k.clamp(0.0, 1.0);
    let channel = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * k).round() as u8;
    Rgb(channel(a.0, b.0), channel(a.1, b.1), channel(a.2, b.2))
}

fn dim(color: Rgb, k: f32) -> Rgb {
    mix(Rgb(0, 0, 0), color, k)
}

fn wave(x: f32) -> f32 {
    (x.sin() + 1.0) / 2.0
}

/// Colors for every LED at `seconds` since the effect started.
pub fn render(settings: &Settings, seconds: f32) -> Frame {
    let speed = match settings.speed {
        Speed::Slow => 0.6,
        Speed::Medium => 1.2,
        Speed::Fast => 2.4,
    };
    let p = seconds * speed;
    let (a, b) = (settings.color1, settings.color2);
    let mut frame = [[Rgb(0, 0, 0); LED_COUNT]; STICKS_IN_FRAME];
    for (s, stick) in frame.iter_mut().enumerate() {
        for (i, led) in stick.iter_mut().enumerate() {
            *led = match settings.effect {
                Effect::Breathing => dim(mix(a, b, wave(p * 0.4)), wave(p * 1.5) * 0.95 + 0.05),
                Effect::Chase => {
                    let m = (p * 0.8).rem_euclid(6.0);
                    let position = if m < 3.0 { m } else { 6.0 - m };
                    dim(mix(a, b, position / 3.0), 1.0 - (s as f32 - position).abs() * 1.1)
                }
                Effect::Gradient => {
                    let x = i as f32 / 9.0 * std::f32::consts::PI + s as f32 * 0.6 - p;
                    mix(a, b, wave(x))
                }
                _ => Rgb(0, 0, 0),
            };
        }
    }
    frame
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(effect: Effect) -> Settings {
        Settings {
            effect,
            color1: Rgb(255, 0, 0),
            color2: Rgb(0, 0, 255),
            ..Settings::default()
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
    fn built_in_effects_render_black() {
        let frame = render(&settings(Effect::Rainbow), 1.0);
        assert!(frame.iter().flatten().all(|led| *led == Rgb(0, 0, 0)));
    }
}
