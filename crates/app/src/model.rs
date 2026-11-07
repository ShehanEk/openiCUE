//! What the window shows. Pure logic, no Windows calls.
use openicue_core::costs::{Cost, Costs};
use openicue_core::effects::{Direction, Effect, Rgb, Settings, Speed, ALL_EFFECTS};
use openicue_core::order::StickOrder;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    App,
    NoDriver,
    NoSticks,
    Settings,
    /// "Which slot is lit?": the user places each stick in its slot.
    Arrange,
}

pub const SETTINGS_TITLE: &str = "Settings";
pub const ORDER_TITLE: &str = "Stick order";
pub const ORDER_NOTE: &str = "The order effects move through when openiCUE draws them, such as Chase, Sequential and synced effects.";

/// A stick's color in the order views: `hsl(200 + 40 * stick, 90%, 58%)`.
pub fn stick_color(stick: usize) -> Rgb {
    let hue = (200.0 + 40.0 * stick as f32).rem_euclid(360.0) / 60.0;
    let (saturation, lightness) = (0.9f32, 0.58f32);
    let chroma = (1.0 - (2.0 * lightness - 1.0).abs()) * saturation;
    let x = chroma * (1.0 - (hue.rem_euclid(2.0) - 1.0).abs());
    let (r, g, b) = match hue as u32 {
        0 => (chroma, x, 0.0),
        1 => (x, chroma, 0.0),
        2 => (0.0, chroma, x),
        3 => (0.0, x, chroma),
        4 => (x, 0.0, chroma),
        _ => (chroma, 0.0, x),
    };
    let m = lightness - chroma / 2.0;
    let channel = |v: f32| ((v + m) * 255.0).round().clamp(0.0, 255.0) as u8;
    Rgb(channel(r), channel(g), channel(b))
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
    /// The effect the engine is drawing in software, if any.
    pub running: Option<Effect>,
    pub costs: Costs,
    /// The latest sample while the selected effect runs in the background.
    pub live: Option<Cost>,
    /// Overrides the status line until the next selection.
    pub message: String,
    /// Slot to stick, as saved. Identity when none is saved or it doesn't fit.
    pub order: StickOrder,
    /// While arranging: `picks[k]` is the slot chosen for stick `k`.
    pub picks: Vec<usize>,
    /// How many sticks are being arranged.
    pub arranging: usize,
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
            order: StickOrder::identity(0),
            picks: Vec::new(),
            arranging: 0,
        }
    }

    pub fn open_settings(&mut self) {
        if self.screen == Screen::App {
            self.screen = Screen::Settings;
            self.message.clear();
        }
    }

    /// Settings goes back to the app; the picker goes back to Settings, unsaved.
    pub fn back(&mut self) {
        match self.screen {
            Screen::Settings => self.screen = Screen::App,
            Screen::Arrange => {
                self.screen = Screen::Settings;
                self.picks.clear();
            }
            _ => return,
        }
        self.message.clear();
    }

    /// Opens the picker for `sticks` sticks, with nothing placed yet.
    pub fn start_arrange(&mut self, sticks: usize) {
        self.screen = Screen::Arrange;
        self.arranging = sticks;
        self.picks.clear();
        self.message.clear();
    }

    /// Places the lit stick in `slot`. Taken or unknown slots are ignored.
    /// When one stick is left, it fills the last free slot. Returns true if
    /// anything changed.
    pub fn pick_slot(&mut self, slot: usize) -> bool {
        if self.arrange_done() || slot >= self.arranging || self.picks.contains(&slot) {
            return false;
        }
        self.picks.push(slot);
        if self.picks.len() + 1 == self.arranging {
            if let Some(last) = (0..self.arranging).find(|s| !self.picks.contains(s)) {
                self.picks.push(last);
            }
        }
        true
    }

    pub fn arrange_done(&self) -> bool {
        self.arranging > 0 && self.picks.len() == self.arranging
    }

    pub fn start_over(&mut self) {
        self.picks.clear();
    }

    /// The stick to light white: the first one not placed yet.
    pub fn lit_stick(&self) -> Option<usize> {
        (!self.arrange_done()).then_some(self.picks.len())
    }

    /// The stick placed in `slot` so far, if any.
    pub fn slot_stick(&self, slot: usize) -> Option<usize> {
        self.picks.iter().position(|&s| s == slot)
    }

    pub fn arrange_result(&self) -> Option<StickOrder> {
        if self.arrange_done() { StickOrder::from_picks(&self.picks) } else { None }
    }

    pub fn arrange_progress(&self) -> String {
        if self.arrange_done() {
            format!("All {} sticks placed", self.arranging)
        } else {
            format!("Stick {} of {}", self.picks.len() + 1, self.arranging)
        }
    }

    pub fn arrange_heading(&self) -> &'static str {
        if self.arrange_done() { "Stick order saved" } else { "Which slot is lit?" }
    }

    pub fn arrange_body(&self) -> &'static str {
        if self.arrange_done() {
            "Effects that move between sticks now travel in this order. Numbers show which stick sits in each slot."
        } else {
            "One stick on your PC is glowing white. Pick its slot, counting left to right as you look at your RAM."
        }
    }

    pub fn arrange_secondary(&self) -> &'static str {
        if self.arrange_done() { "Start over" } else { "Cancel" }
    }

    /// The saved order, left to right: each slot's stick number (from 1) and color.
    pub fn order_slots(&self) -> Vec<(usize, Rgb)> {
        (0..self.order.len())
            .filter_map(|p| self.order.stick_at(p))
            .map(|stick| (stick + 1, stick_color(stick)))
            .collect()
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

    pub fn toggle_sync(&mut self) {
        if self.settings.effect.allows_sync() {
            self.settings.synced = !self.settings.synced;
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

    pub fn show_sync(&self) -> bool {
        self.settings.effect.allows_sync()
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
        self.settings.runs_in_software()
    }

    pub fn cost_line(&self) -> String {
        let effect = self.settings.effect;
        if !self.settings.runs_in_software() {
            return "0 MB · 0% CPU".to_string();
        }
        if self.is_running_selected() {
            return self.live.map(format_cost).unwrap_or_else(|| "Measuring…".to_string());
        }
        self.costs.get(effect).map(format_cost).unwrap_or_else(|| "Not measured yet".to_string())
    }

    pub fn cost_body(&self) -> &'static str {
        let effect = self.settings.effect;
        if !self.settings.runs_in_software() {
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
        if self.settings.runs_in_software() && self.is_running_selected() { "Update" } else { "Apply" }
    }

    pub fn show_stop(&self) -> bool {
        self.running.is_some()
    }

    pub fn status_line(&self) -> String {
        // Settings and the picker show only messages, such as errors.
        if matches!(self.screen, Screen::Settings | Screen::Arrange) {
            return self.message.clone();
        }
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
    use openicue_core::order::StickOrder;

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
    fn sync_toggle_shows_only_for_animated_built_ins() {
        for (effect, shown) in [(Effect::ColorWave, true), (Effect::Static, false), (Effect::Breathing, false)] {
            let mut m = model(effect);
            assert_eq!(m.show_sync(), shown, "{effect:?}");
            m.toggle_sync();
            assert_eq!(m.settings.synced, shown, "{effect:?}");
        }
        let mut m = model(Effect::ColorWave);
        m.toggle_sync();
        m.toggle_sync();
        assert!(!m.settings.synced);
        m.toggle_sync();
        m.select(Effect::Static);
        assert!(!m.settings.synced, "select drops sync where it isn't allowed");
    }

    #[test]
    fn synced_built_in_costs_like_a_custom_effect() {
        let mut m = model(Effect::ColorWave);
        m.toggle_sync();
        assert!(m.cost_is_ongoing());
        assert_eq!(m.cost_line(), "Not measured yet");
        assert_eq!(m.cost_body(), "Keeps running after you close this window. Apply it to see the real cost.");
        assert_eq!(m.apply_label(), "Apply");

        m.running = Some(Effect::ColorWave);
        m.live = Some(Cost { mb: 1.2, cpu: 0.7 });
        assert_eq!(m.cost_line(), "1.2 MB · 0.7% CPU");
        assert_eq!(m.cost_body(), "Using this now, until you stop it or shut down.");
        assert_eq!(m.apply_label(), "Update");

        m.toggle_sync();
        assert!(!m.cost_is_ongoing());
        assert_eq!(m.cost_line(), "0 MB · 0% CPU");
        assert_eq!(m.cost_body(), "After you close this window. Applying stops the background process.");
        assert_eq!(m.apply_label(), "Apply");
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
    fn settings_opens_from_the_app_and_back_returns() {
        let mut m = model(Effect::Static);
        m.message = "Applied".into();
        m.open_settings();
        assert_eq!(m.screen, Screen::Settings);
        assert_eq!(m.status_line(), "", "the app's status line stays on the app screen");
        m.message = "Couldn't reach the RAM".into();
        assert_eq!(m.status_line(), "Couldn't reach the RAM", "errors still show");
        m.back();
        assert_eq!((m.screen, m.message.as_str()), (Screen::App, ""));
        m.screen = Screen::NoSticks;
        m.open_settings();
        assert_eq!(m.screen, Screen::NoSticks, "only from the app screen");
    }

    #[test]
    fn settings_text_matches_the_design() {
        assert_eq!(SETTINGS_TITLE, "Settings");
        assert_eq!(ORDER_TITLE, "Stick order");
        assert_eq!(ORDER_NOTE, "The order effects move through when openiCUE draws them, such as Chase, Sequential and synced effects.");
        let mut m = model(Effect::Static);
        m.order = StickOrder::from_picks(&[0, 2, 1, 3]).unwrap();
        assert_eq!(m.order_slots(), vec![(1, stick_color(0)), (3, stick_color(2)), (2, stick_color(1)), (4, stick_color(3))]);
    }

    #[test]
    fn stick_colors_follow_the_design_hues() {
        // hsl(200, 90%, 58%), then 40 degrees further per stick.
        assert_eq!(stick_color(0), Rgb(52, 180, 244));
        assert_eq!(stick_color(1), Rgb(52, 52, 244));
        assert_eq!(stick_color(4), Rgb(244, 52, 52), "360 wraps to red");
    }

    fn arranging(n: usize) -> Model {
        let mut m = model(Effect::Static);
        m.open_settings();
        m.start_arrange(n);
        m
    }

    #[test]
    fn picker_asks_for_each_stick_in_turn() {
        let mut m = arranging(4);
        assert_eq!(m.screen, Screen::Arrange);
        assert_eq!(m.lit_stick(), Some(0));
        assert_eq!(m.arrange_progress(), "Stick 1 of 4");
        assert_eq!(m.arrange_heading(), "Which slot is lit?");
        assert_eq!(m.arrange_body(), "One stick on your PC is glowing white. Pick its slot, counting left to right as you look at your RAM.");
        assert_eq!(m.arrange_secondary(), "Cancel");
        assert!(m.pick_slot(0));
        assert_eq!((m.lit_stick(), m.arrange_progress().as_str()), (Some(1), "Stick 2 of 4"));
        assert!(!m.pick_slot(0), "a taken slot is ignored");
        assert!(!m.pick_slot(4), "no such slot");
        assert_eq!(m.picks, vec![0]);
        assert_eq!(m.slot_stick(0), Some(0));
        assert_eq!(m.slot_stick(1), None);
        assert!(m.arrange_result().is_none());
    }

    #[test]
    fn the_last_stick_fills_itself_in() {
        let mut m = arranging(4);
        assert!(m.pick_slot(0) && m.pick_slot(2) && m.pick_slot(1));
        assert_eq!(m.picks, vec![0, 2, 1, 3]);
        assert!(m.arrange_done());
        assert_eq!(m.lit_stick(), None);
        assert!(!m.pick_slot(3), "nothing left to pick");
        assert_eq!(m.arrange_progress(), "All 4 sticks placed");
        assert_eq!(m.arrange_heading(), "Stick order saved");
        assert_eq!(m.arrange_body(), "Effects that move between sticks now travel in this order. Numbers show which stick sits in each slot.");
        assert_eq!(m.arrange_secondary(), "Start over");
        assert_eq!(m.arrange_result(), StickOrder::from_picks(&[0, 2, 1, 3]));
        assert_eq!((0..4).map(|s| m.slot_stick(s)).collect::<Vec<_>>(), [Some(0), Some(2), Some(1), Some(3)]);

        let mut two = arranging(2);
        assert!(two.pick_slot(1));
        assert_eq!(two.arrange_result(), StickOrder::from_picks(&[1, 0]), "two sticks need one pick");
    }

    #[test]
    fn start_over_and_cancel_clear_the_picks() {
        let mut m = arranging(3);
        m.pick_slot(2);
        m.pick_slot(0);
        m.start_over();
        assert_eq!((m.picks.len(), m.lit_stick()), (0, Some(0)));
        m.pick_slot(1);
        m.back();
        assert_eq!(m.screen, Screen::Settings, "back from the picker is cancel");
        assert!(m.picks.is_empty());
        m.start_arrange(3);
        assert!(m.picks.is_empty(), "start clears picks");
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
