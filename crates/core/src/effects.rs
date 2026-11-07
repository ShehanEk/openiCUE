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
    /// Built-ins that animate can be drawn in software from one clock, so the
    /// sticks stay in sync. Custom effects always are; Static doesn't animate.
    pub fn allows_sync(self) -> bool {
        !self.is_custom() && self != Effect::Static
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
    /// Draw a built-in effect in software, like a custom effect, so the sticks stay in sync.
    pub synced: bool,
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
            synced: false,
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
        if !self.effect.allows_sync() {
            self.synced = false;
        }
        self
    }

    /// True when the background engine draws the effect, rather than the RAM.
    pub fn runs_in_software(&self) -> bool {
        self.effect.is_custom() || self.synced
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

    /// Pinned to OpenRGB's CorsairDRAMController.h `CORSAIR_DRAM_MODE_*`.
    #[test]
    fn mode_bytes_match_openrgb() {
        let expected = [
            (Effect::Static, Some(0x10)), (Effect::ColorShift, Some(0x00)),
            (Effect::ColorPulse, Some(0x01)), (Effect::RainbowWave, Some(0x03)),
            (Effect::ColorWave, Some(0x04)), (Effect::Visor, Some(0x05)),
            (Effect::Rain, Some(0x06)), (Effect::Marquee, Some(0x07)),
            (Effect::Rainbow, Some(0x08)), (Effect::Sequential, Some(0x09)),
            (Effect::Breathing, None), (Effect::Chase, None), (Effect::Gradient, None),
        ];
        assert_eq!(expected.len(), ALL_EFFECTS.len());
        for (effect, byte) in expected {
            assert_eq!(effect.mode_byte(), byte, "{effect:?}");
        }
    }

    /// Pinned to `CORSAIR_DRAM_SPEED_*`.
    #[test]
    fn speed_bytes_match_openrgb() {
        assert_eq!(Speed::Slow.byte(), 0x00);
        assert_eq!(Speed::Medium.byte(), 0x01);
        assert_eq!(Speed::Fast.byte(), 0x02);
    }

    /// Pinned to `CORSAIR_DRAM_DIRECTION_*`.
    #[test]
    fn direction_bytes_match_openrgb() {
        assert_eq!(Direction::Up.byte(), 0x00);
        assert_eq!(Direction::Down.byte(), 0x01);
        assert_eq!(Direction::Left.byte(), 0x02);
        assert_eq!(Direction::Right.byte(), 0x03);
        assert_eq!(Direction::Vertical.byte(), 0x01);
        assert_eq!(Direction::Horizontal.byte(), 0x03);
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

        let synced = |effect| Settings { effect, synced: true, ..Settings::default() }.normalized().synced;
        assert!(!synced(Effect::Static));
        assert!(!synced(Effect::Chase));
        assert!(synced(Effect::ColorWave));
    }

    #[test]
    fn every_animated_built_in_allows_sync() {
        assert_eq!(ALL_EFFECTS.iter().filter(|e| e.allows_sync()).count(), 9);
        assert!(!Effect::Static.allows_sync() && !Effect::Breathing.allows_sync());
    }

    #[test]
    fn custom_or_synced_effects_run_in_software() {
        let s = |effect, synced| Settings { effect, synced, ..Settings::default() };
        assert!(s(Effect::Chase, false).runs_in_software());
        assert!(s(Effect::ColorWave, true).runs_in_software());
        assert!(!s(Effect::ColorWave, false).runs_in_software());
        assert!(!s(Effect::Static, false).runs_in_software());
    }
}
