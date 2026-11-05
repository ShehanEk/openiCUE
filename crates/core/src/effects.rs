#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgb(pub u8, pub u8, pub u8);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effect {
    Static, ColorShift, ColorPulse, RainbowWave, ColorWave, Visor, Rain, Marquee, Rainbow, Sequential,
    Breathing, Chase, Gradient,
}

pub const ALL_EFFECTS: [Effect; 13] = [
    Effect::Static, Effect::ColorShift, Effect::ColorPulse, Effect::RainbowWave, Effect::ColorWave,
    Effect::Visor, Effect::Rain, Effect::Marquee, Effect::Rainbow, Effect::Sequential,
    Effect::Breathing, Effect::Chase, Effect::Gradient,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Speed { Slow, Medium, Fast }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction { Up, Down, Left, Right, Vertical, Horizontal }

impl Effect {
    pub fn name(self) -> &'static str {
        match self {
            Effect::Static => "static", Effect::ColorShift => "color-shift",
            Effect::ColorPulse => "color-pulse", Effect::RainbowWave => "rainbow-wave",
            Effect::ColorWave => "color-wave", Effect::Visor => "visor", Effect::Rain => "rain",
            Effect::Marquee => "marquee", Effect::Rainbow => "rainbow",
            Effect::Sequential => "sequential", Effect::Breathing => "breathing",
            Effect::Chase => "chase", Effect::Gradient => "gradient",
        }
    }
    pub fn from_name(name: &str) -> Option<Effect> {
        ALL_EFFECTS.into_iter().find(|e| e.name() == name)
    }
    /// The byte sent to the RAM. `None` for custom effects.
    pub fn mode_byte(self) -> Option<u8> {
        match self {
            Effect::ColorShift => Some(0x00), Effect::ColorPulse => Some(0x01),
            Effect::RainbowWave => Some(0x03), Effect::ColorWave => Some(0x04),
            Effect::Visor => Some(0x05), Effect::Rain => Some(0x06),
            Effect::Marquee => Some(0x07), Effect::Rainbow => Some(0x08),
            Effect::Sequential => Some(0x09), Effect::Static => Some(0x10),
            Effect::Breathing | Effect::Chase | Effect::Gradient => None,
        }
    }
    pub fn is_custom(self) -> bool {
        self.mode_byte().is_none()
    }
    pub fn color_count(self) -> u8 {
        match self {
            Effect::RainbowWave | Effect::Rainbow => 0,
            Effect::Static | Effect::Marquee | Effect::Sequential => 1,
            _ => 2,
        }
    }
    pub fn allows_random(self) -> bool {
        matches!(self, Effect::ColorShift | Effect::ColorPulse | Effect::ColorWave
            | Effect::Visor | Effect::Rain | Effect::Sequential)
    }
    pub fn has_speed(self) -> bool {
        self != Effect::Static
    }
    pub fn directions(self) -> &'static [Direction] {
        match self {
            Effect::RainbowWave | Effect::ColorWave =>
                &[Direction::Down, Direction::Up, Direction::Left, Direction::Right],
            Effect::Visor => &[Direction::Vertical, Direction::Horizontal],
            Effect::Rain | Effect::Sequential => &[Direction::Down, Direction::Up],
            _ => &[],
        }
    }
}

impl Speed {
    pub fn name(self) -> &'static str {
        match self { Speed::Slow => "slow", Speed::Medium => "medium", Speed::Fast => "fast" }
    }
    pub fn from_name(name: &str) -> Option<Speed> {
        [Speed::Slow, Speed::Medium, Speed::Fast].into_iter().find(|s| s.name() == name)
    }
    pub fn byte(self) -> u8 {
        match self { Speed::Slow => 0, Speed::Medium => 1, Speed::Fast => 2 }
    }
}

impl Direction {
    pub fn name(self) -> &'static str {
        match self {
            Direction::Up => "up", Direction::Down => "down", Direction::Left => "left",
            Direction::Right => "right", Direction::Vertical => "vertical",
            Direction::Horizontal => "horizontal",
        }
    }
    pub fn from_name(name: &str) -> Option<Direction> {
        [Direction::Up, Direction::Down, Direction::Left, Direction::Right,
         Direction::Vertical, Direction::Horizontal]
            .into_iter().find(|d| d.name() == name)
    }
    pub fn byte(self) -> u8 {
        match self {
            Direction::Up => 0, Direction::Down | Direction::Vertical => 1,
            Direction::Left => 2, Direction::Right | Direction::Horizontal => 3,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Settings {
    pub effect: Effect,
    pub color1: Rgb,
    pub color2: Rgb,
    pub random: bool,
    pub speed: Speed,
    pub direction: Direction,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            effect: Effect::RainbowWave,
            color1: Rgb(0x2F, 0x9B, 0xFF),
            color2: Rgb(0xFF, 0x7A, 0x3D),
            random: false,
            speed: Speed::Slow,
            direction: Direction::Down,
        }
    }
}

impl Settings {
    /// Removes options the chosen effect doesn't support.
    pub fn normalized(mut self) -> Settings {
        if !self.effect.allows_random() {
            self.random = false;
        }
        let directions = self.effect.directions();
        if !directions.is_empty() && !directions.contains(&self.direction) {
            self.direction = directions[0];
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_round_trip() {
        for effect in ALL_EFFECTS {
            assert_eq!(Effect::from_name(effect.name()), Some(effect));
        }
    }

    #[test]
    fn exactly_three_effects_are_custom() {
        assert_eq!(ALL_EFFECTS.iter().filter(|e| e.is_custom()).count(), 3);
    }

    #[test]
    fn normalized_removes_unsupported_options() {
        let settings = Settings {
            effect: Effect::Visor, direction: Direction::Up, ..Settings::default()
        }.normalized();
        assert_eq!(settings.direction, Direction::Vertical);

        let settings = Settings {
            effect: Effect::Rainbow, random: true, ..Settings::default()
        }.normalized();
        assert!(!settings.random);
    }
}
