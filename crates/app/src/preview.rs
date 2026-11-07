//! Preview colors for the 4 x 10 LED grid: the same frames the engine draws.
use openicue_core::effects::Settings;
use openicue_core::frames::{self, Frame};

pub fn colors(settings: &Settings, seconds: f32) -> Frame {
    frames::render(&settings.normalized(), seconds)
}

#[cfg(test)]
mod tests {
    use super::*;
    use openicue_core::effects::ALL_EFFECTS;

    #[test]
    fn matches_the_engine_exactly() {
        for effect in ALL_EFFECTS {
            let s = Settings { effect, ..Settings::default() }.normalized();
            assert_eq!(colors(&s, 2.5), frames::render(&s, 2.5), "{effect:?}");
        }
    }
}
