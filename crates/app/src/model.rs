//! What the window shows. Pure logic, no Windows calls.
use openicue_core::costs::{Cost, Costs};
use openicue_core::effects::{Direction, Effect, Rgb, Settings, Speed, ALL_EFFECTS};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    App,
    NoDriver,
    NoSticks,
}

pub const SPEEDS: [Speed; 3] = [Speed::Slow, Speed::Medium, Speed::Fast];

pub fn display_name(effect: Effect) -> &'static str {
    match effect {
        Effect::Static => "Static",
        Effect::ColorShift => "Color shift",
        Effect::ColorPulse => "Color pulse",
        Effect::RainbowWave => "Rainbow wave",
        Effect::ColorWave => "Color wave",
        Effect::Visor => "Visor",
        Effect::Rain => "Rain",
        Effect::Marquee => "Marquee",
        Effect::Rainbow => "Rainbow",
        Effect::Sequential => "Sequential",
        Effect::Breathing => "Breathing",
        Effect::Chase => "Chase",
        Effect::Gradient => "Gradient flow",
    }
}

pub fn speed_name(speed: Speed) -> &'static str {
    match speed {
        Speed::Slow => "Slow",
        Speed::Medium => "Medium",
        Speed::Fast => "Fast",
    }
}

pub fn direction_name(direction: Direction) -> &'static str {
    match direction {
        Direction::Up => "Up",
        Direction::Down => "Down",
        Direction::Left => "Left",
        Direction::Right => "Right",
        Direction::Vertical => "Vertical",
        Direction::Horizontal => "Horizontal",
    }
}

pub fn builtins() -> Vec<Effect> {
    ALL_EFFECTS.into_iter().filter(|e| !e.is_custom()).collect()
}

pub fn customs() -> Vec<Effect> {
    ALL_EFFECTS.into_iter().filter(|e| e.is_custom()).collect()
}

fn format_cost(cost: Cost) -> String {
    format!("{:.1} MB · {:.1}% CPU", cost.mb, cost.cpu)
}

pub struct Model {
    pub screen: Screen,
    pub sticks: usize,
    /// The choice on screen. Always normalized, so it can be applied as is.
    pub settings: Settings,
    /// The custom effect the engine is running, if any.
    pub running: Option<Effect>,
    pub costs: Costs,
    /// The latest sample while the selected custom effect runs.
    pub live: Option<Cost>,
    /// Overrides the status line until the next selection.
    pub message: String,
}

impl Model {
    pub fn new(settings: Settings) -> Model {
        Model {
            screen: Screen::App,
            sticks: 0,
            settings: settings.normalized(),
            running: None,
            costs: Costs::default(),
            live: None,
            message: String::new(),
        }
    }

    pub fn select(&mut self, effect: Effect) {
        self.settings.effect = effect;
        self.settings = self.settings.normalized();
        self.message.clear();
    }

    pub fn set_speed(&mut self, speed: Speed) {
        self.settings.speed = speed;
    }

    pub fn set_direction(&mut self, direction: Direction) {
        if self.directions().contains(&direction) {
            self.settings.direction = direction;
        }
    }

    pub fn toggle_random(&mut self) {
        if self.settings.effect.allows_random() {
            self.settings.random = !self.settings.random;
        }
    }

    /// `index` is 1 or 2. Anything else is ignored.
    pub fn set_color(&mut self, index: u8, color: Rgb) {
        match index {
            1 => self.settings.color1 = color,
            2 => self.settings.color2 = color,
            _ => {}
        }
    }

    pub fn show_colors(&self) -> bool {
        self.settings.effect.color_count() > 0
    }

    pub fn show_pickers(&self) -> bool {
        self.show_colors() && !self.settings.random
    }

    pub fn show_color2(&self) -> bool {
        self.settings.effect.color_count() > 1
    }

    pub fn show_random(&self) -> bool {
        self.settings.effect.allows_random()
    }

    pub fn show_speed(&self) -> bool {
        self.settings.effect.has_speed()
    }

    pub fn directions(&self) -> &'static [Direction] {
        self.settings.effect.directions()
    }

    pub fn is_running_selected(&self) -> bool {
        self.running == Some(self.settings.effect)
    }

    pub fn cost_is_ongoing(&self) -> bool {
        self.settings.effect.is_custom()
    }

    pub fn cost_line(&self) -> String {
        let effect = self.settings.effect;
        if !effect.is_custom() {
            return "0 MB · 0% CPU".to_string();
        }
        if self.is_running_selected() {
            return self.live.map(format_cost).unwrap_or_else(|| "Measuring…".to_string());
        }
        self.costs.get(effect).map(format_cost).unwrap_or_else(|| "Not measured yet".to_string())
    }

    pub fn cost_body(&self) -> &'static str {
        let effect = self.settings.effect;
        if !effect.is_custom() {
            if self.running.is_some() {
                "After you close this window. Applying stops the background process."
            } else {
                "After you close this window. The RAM runs this itself."
            }
        } else if self.is_running_selected() {
            "Using this now, until you stop it or shut down."
        } else if self.costs.get(effect).is_some() {
            "Last run on this PC. Keeps running after you close this window."
        } else {
            "Keeps running after you close this window. Apply it to see the real cost."
        }
    }

    pub fn apply_label(&self) -> &'static str {
        if self.settings.effect.is_custom() && self.is_running_selected() { "Update" } else { "Apply" }
    }

    pub fn show_stop(&self) -> bool {
        self.running.is_some()
    }

    pub fn status_line(&self) -> String {
        if self.screen != Screen::App {
            return String::new();
        }
        if !self.message.is_empty() {
            return self.message.clone();
        }
        if self.running.is_some() {
            return "Background process running".to_string();
        }
        let noun = if self.sticks == 1 { "stick" } else { "sticks" };
        format!("{} {noun} · restores at logon", self.sticks)
    }

    pub fn error_title(&self) -> &'static str {
        match self.screen {
            Screen::NoDriver => "PawnIO driver not found",
            _ => "No RAM lighting found",
        }
    }

    pub fn error_body(&self) -> &'static str {
        match self.screen {
            Screen::NoDriver => "openiCUE needs this driver to reach the LEDs on your RAM. Run openiCUE Setup again to restore it, then check again.",
            _ => "No supported lighting controller answered, and nothing was written to your RAM. Close iCUE or other RGB apps, then check again.",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openicue_core::costs::{Cost, Costs};
    use openicue_core::effects::{Direction, Effect, Rgb, Settings, Speed};

    fn model(effect: Effect) -> Model {
        let mut m = Model::new(Settings::default());
        m.sticks = 4;
        m.select(effect);
        m
    }

    #[test]
    fn options_follow_the_effect() {
        let m = model(Effect::Static);
        assert!(m.show_colors() && m.show_pickers() && !m.show_color2());
        assert!(!m.show_random() && !m.show_speed() && m.directions().is_empty());

        let m = model(Effect::Rainbow);
        assert!(!m.show_colors() && m.show_speed());

        let mut m = model(Effect::ColorWave);
        assert!(m.show_color2() && m.show_random());
        assert_eq!(m.directions(), &[Direction::Down, Direction::Up, Direction::Left, Direction::Right]);
        m.toggle_random();
        assert!(m.show_colors() && !m.show_pickers(), "random hides the pickers");

        let m = model(Effect::Visor);
        assert_eq!(m.settings.direction, Direction::Vertical, "select keeps the direction valid");
    }

    #[test]
    fn setters_ignore_options_the_effect_lacks() {
        let mut m = model(Effect::Rainbow);
        m.toggle_random();
        assert!(!m.settings.random);
        m.set_direction(Direction::Vertical);
        assert_ne!(m.settings.direction, Direction::Vertical);
        m.set_color(3, Rgb(1, 2, 3));
        assert_eq!(m.settings.color1, Settings::default().color1);
        m.set_color(2, Rgb(1, 2, 3));
        m.set_speed(Speed::Fast);
        assert_eq!((m.settings.color2, m.settings.speed), (Rgb(1, 2, 3), Speed::Fast));
    }

    #[test]
    fn built_in_cost_is_zero_and_says_why() {
        let mut m = model(Effect::RainbowWave);
        assert_eq!(m.cost_line(), "0 MB · 0% CPU");
        assert!(!m.cost_is_ongoing());
        assert_eq!(m.cost_body(), "After you close this window. The RAM runs this itself.");
        m.running = Some(Effect::Chase);
        assert_eq!(m.cost_body(), "After you close this window. Applying stops the background process.");
    }

    #[test]
    fn custom_cost_comes_from_measurement() {
        let mut m = model(Effect::Chase);
        assert_eq!(m.cost_line(), "Not measured yet");
        assert_eq!(m.cost_body(), "Keeps running after you close this window. Apply it to see the real cost.");
        assert!(m.cost_is_ongoing());

        let mut costs = Costs::default();
        costs.set(Effect::Chase, Cost { mb: 1.4, cpu: 0.8 });
        m.costs = costs;
        assert_eq!(m.cost_line(), "1.4 MB · 0.8% CPU");
        assert_eq!(m.cost_body(), "Last run on this PC. Keeps running after you close this window.");

        m.running = Some(Effect::Chase);
        m.live = Some(Cost { mb: 1.5, cpu: 0.9 });
        assert_eq!(m.cost_line(), "1.5 MB · 0.9% CPU");
        assert_eq!(m.cost_body(), "Using this now, until you stop it or shut down.");
        assert_eq!(m.apply_label(), "Update");
        assert!(m.show_stop());

        m.live = None;
        assert_eq!(m.cost_line(), "Measuring…");
    }

    #[test]
    fn status_line_prefers_message_then_running() {
        let mut m = model(Effect::Static);
        assert_eq!(m.status_line(), "4 sticks · restores at logon");
        m.sticks = 1;
        assert_eq!(m.status_line(), "1 stick · restores at logon");
        m.running = Some(Effect::Gradient);
        assert_eq!(m.status_line(), "Background process running");
        m.message = "Applied. You can close this window".into();
        assert_eq!(m.status_line(), "Applied. You can close this window");
        m.select(Effect::Rain);
        assert_eq!(m.status_line(), "Background process running", "select clears the message");
        m.screen = Screen::NoDriver;
        assert_eq!(m.status_line(), "");
    }

    #[test]
    fn names_match_the_design() {
        let names: Vec<&str> = builtins().into_iter().map(display_name).collect();
        assert_eq!(names, ["Static", "Color shift", "Color pulse", "Rainbow wave", "Color wave",
            "Visor", "Rain", "Marquee", "Rainbow", "Sequential"]);
        let names: Vec<&str> = customs().into_iter().map(display_name).collect();
        assert_eq!(names, ["Breathing", "Chase", "Gradient flow"]);
        assert_eq!(speed_name(Speed::Medium), "Medium");
        assert_eq!(direction_name(Direction::Horizontal), "Horizontal");
    }

    #[test]
    fn error_screens_have_the_design_text() {
        let mut m = model(Effect::Static);
        m.screen = Screen::NoDriver;
        assert_eq!(m.error_title(), "PawnIO driver not found");
        m.screen = Screen::NoSticks;
        assert_eq!(m.error_title(), "No RAM lighting found");
        assert!(m.error_body().starts_with("No supported lighting controller answered"));
    }
}
