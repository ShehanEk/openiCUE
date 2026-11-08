//! What each Setup screen says and offers. Pure, so it is tested.
pub use openicue_core::effects::Rgb;
use openicue_core::order::StickOrder;

/// CSS `hsl(hue, saturation, lightness)`, with saturation and lightness
/// from 0 to 1. Copied from the window's `stick_color`, not shared.
pub fn hsl(hue: f32, saturation: f32, lightness: f32) -> Rgb {
    let hue = hue.rem_euclid(360.0) / 60.0;
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    Welcome,
    Progress,
    Done,
    Failed,
    Uninstall,
    Removed,
    /// After installing: "Which slot is lit?", once per stick.
    Arrange,
}

/// A stick's color in the picker: `hsl(200 + 40 * stick, 90%, 58%)`.
pub fn stick_color(stick: usize) -> Rgb {
    hsl(200.0 + 40.0 * stick as f32, 0.9, 0.58)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Failure {
    /// The bundled PawnIO setup failed its hash or signature check.
    PawnIoCheck,
    /// Anything else, with a sentence for the user.
    Other(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepState {
    Done,
    Active,
    Waiting,
}

pub const STEPS: [&str; 4] = [
    "Checking files",
    "Installing openiCUE",
    "Installing PawnIO driver",
    "Setting up restore at logon",
];

pub struct Model {
    pub screen: Screen,
    /// The step in progress, 0 to 3; 4 once all are done.
    pub step: usize,
    pub failure: Option<Failure>,
    pub remove_pawnio: bool,
    /// How many sticks the picker places.
    pub sticks: usize,
    /// In the picker: `picks[k]` is the slot chosen for stick `k`.
    pub picks: Vec<usize>,
}

impl Model {
    pub fn installing() -> Model {
        Model { screen: Screen::Welcome, step: 0, failure: None, remove_pawnio: false, sticks: 0, picks: Vec::new() }
    }

    pub fn uninstalling() -> Model {
        Model { screen: Screen::Uninstall, ..Model::installing() }
    }

    pub fn window_title(&self) -> &'static str {
        match self.screen {
            Screen::Uninstall | Screen::Removed => "openiCUE",
            _ => "openiCUE Setup",
        }
    }

    pub fn heading(&self) -> &'static str {
        match self.screen {
            Screen::Welcome => "Install openiCUE",
            Screen::Progress => "Installing",
            Screen::Done => "openiCUE is ready",
            Screen::Failed => "Setup stopped",
            Screen::Uninstall => "Uninstall openiCUE",
            Screen::Removed => "openiCUE is removed",
            Screen::Arrange if self.arrange_done() => "Stick order saved",
            Screen::Arrange => "Which slot is lit?",
        }
    }

    /// The paragraph under the heading, where the screen has one.
    pub fn body(&self) -> Option<String> {
        match self.screen {
            Screen::Done => Some("openiCUE and the PawnIO driver are installed. Your effect will come back each time you log on.".into()),
            Screen::Failed => Some(match &self.failure {
                Some(Failure::Other(reason)) => reason.clone(),
                _ => "The PawnIO file failed its security check, so it was not run and nothing was installed. Don't retry this copy of Setup. Download openiCUE Setup again from its official source.".into(),
            }),
            Screen::Uninstall => Some("Removes the app, the logon task and your settings.".into()),
            Screen::Removed => Some("The app, the logon task and your settings are gone. A leftover folder is removed when you next restart.".into()),
            Screen::Arrange if self.arrange_done() => Some("Effects that move between sticks now travel in this order. Numbers show which stick sits in each slot.".into()),
            Screen::Arrange => Some("One stick on your PC is glowing white. Pick its slot, counting left to right as you look at your RAM.".into()),
            _ => None,
        }
    }

    pub fn primary(&self) -> Option<&'static str> {
        match self.screen {
            Screen::Welcome => Some("Install"),
            Screen::Done => Some("Open openiCUE"),
            Screen::Failed | Screen::Removed => Some("Close"),
            Screen::Uninstall => Some("Uninstall"),
            Screen::Arrange if self.arrange_done() => Some("Continue"),
            Screen::Progress | Screen::Arrange => None,
        }
    }

    pub fn secondary(&self) -> Option<&'static str> {
        match self.screen {
            Screen::Welcome | Screen::Uninstall => Some("Cancel"),
            Screen::Done => Some("Close"),
            Screen::Arrange if self.arrange_done() => Some("Start over"),
            Screen::Arrange => Some("Skip"),
            _ => None,
        }
    }

    pub fn hint(&self) -> &'static str {
        match self.screen {
            Screen::Progress => "This usually takes under a minute.",
            Screen::Arrange => "You can change this later in openiCUE.",
            _ => "",
        }
    }

    /// Opens the picker for `sticks` sticks, with nothing placed yet.
    pub fn start_arrange(&mut self, sticks: usize) {
        self.screen = Screen::Arrange;
        self.sticks = sticks;
        self.picks.clear();
    }

    /// Places the lit stick in `slot`. Taken or unknown slots are ignored.
    /// When one stick is left, it fills the last free slot. Returns true if
    /// anything changed.
    pub fn pick_slot(&mut self, slot: usize) -> bool {
        if self.arrange_done() || slot >= self.sticks || self.picks.contains(&slot) {
            return false;
        }
        self.picks.push(slot);
        if self.picks.len() + 1 == self.sticks {
            if let Some(last) = (0..self.sticks).find(|s| !self.picks.contains(s)) {
                self.picks.push(last);
            }
        }
        true
    }

    pub fn arrange_done(&self) -> bool {
        self.sticks > 0 && self.picks.len() == self.sticks
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
            format!("All {} sticks placed", self.sticks)
        } else {
            format!("Stick {} of {}", self.picks.len() + 1, self.sticks)
        }
    }

    pub fn step_state(&self, i: usize) -> StepState {
        if i < self.step {
            StepState::Done
        } else if i == self.step {
            StepState::Active
        } else {
            StepState::Waiting
        }
    }

    /// How many of the 10 strip LEDs glow, as in the design.
    pub fn lit_leds(&self) -> usize {
        match self.screen {
            Screen::Welcome | Screen::Done => 10,
            // Steps 0..4 light 0, 2, 5, 7, 10 LEDs, as the design's progress sweep.
            Screen::Progress => self.step * 10 / STEPS.len(),
            _ => 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn picker(n: usize) -> Model {
        let mut m = Model::installing();
        m.start_arrange(n);
        m
    }

    #[test]
    fn picker_asks_for_each_stick_with_skip() {
        let m = picker(4);
        assert_eq!(m.screen, Screen::Arrange);
        assert_eq!(m.window_title(), "openiCUE Setup");
        assert_eq!(m.arrange_progress(), "Stick 1 of 4");
        assert_eq!(m.heading(), "Which slot is lit?");
        assert_eq!(m.body().unwrap(), "One stick on your PC is glowing white. Pick its slot, counting left to right as you look at your RAM.");
        assert_eq!((m.primary(), m.secondary()), (None, Some("Skip")));
        assert_eq!(m.hint(), "You can change this later in openiCUE.");
        assert_eq!(m.lit_stick(), Some(0));
        assert_eq!(m.lit_leds(), 0);
        assert!(m.arrange_result().is_none());
    }

    #[test]
    fn picks_ignore_taken_and_unknown_slots_and_fill_the_last() {
        let mut m = picker(4);
        assert!(m.pick_slot(0));
        assert_eq!((m.lit_stick(), m.arrange_progress().as_str()), (Some(1), "Stick 2 of 4"));
        assert!(!m.pick_slot(0), "a taken slot is ignored");
        assert!(!m.pick_slot(4), "no such slot");
        assert_eq!((m.slot_stick(0), m.slot_stick(1)), (Some(0), None));
        assert!(m.pick_slot(2));
        assert!(m.pick_slot(1), "the third pick also places the last stick");
        assert_eq!(m.picks, vec![0, 2, 1, 3]);
        assert!(m.arrange_done());
        assert_eq!(m.lit_stick(), None);
        assert!(!m.pick_slot(3), "nothing left to pick");
        assert_eq!(m.arrange_result(), StickOrder::from_picks(&[0, 2, 1, 3]));
        assert_eq!((0..4).map(|s| m.slot_stick(s)).collect::<Vec<_>>(), [Some(0), Some(2), Some(1), Some(3)]);
    }

    #[test]
    fn done_picker_offers_start_over_and_continue() {
        let mut m = picker(2);
        assert!(m.pick_slot(1), "with two sticks one pick places both");
        assert_eq!(m.picks, vec![1, 0]);
        assert_eq!(m.arrange_progress(), "All 2 sticks placed");
        assert_eq!(m.heading(), "Stick order saved");
        assert_eq!(m.body().unwrap(), "Effects that move between sticks now travel in this order. Numbers show which stick sits in each slot.");
        assert_eq!((m.primary(), m.secondary()), (Some("Continue"), Some("Start over")));
        assert_eq!(m.hint(), "You can change this later in openiCUE.");
        m.start_over();
        assert!(m.picks.is_empty());
        assert_eq!((m.lit_stick(), m.secondary()), (Some(0), Some("Skip")));
    }

    #[test]
    fn stick_colors_follow_the_design_hues() {
        assert_eq!(stick_color(0), Rgb(52, 180, 244));
        assert_eq!(stick_color(1), Rgb(52, 52, 244));
        assert_eq!(stick_color(4), Rgb(244, 52, 52), "360 wraps to red");
    }

    #[test]
    fn hsl_matches_css() {
        assert_eq!(hsl(0.0, 0.9, 0.58), Rgb(244, 52, 52));
        assert_eq!(hsl(200.0, 0.9, 0.58), Rgb(52, 180, 244));
        assert_eq!(hsl(240.0, 0.9, 0.58), Rgb(52, 52, 244));
        assert_eq!(hsl(360.0, 0.9, 0.58), hsl(0.0, 0.9, 0.58), "360 wraps to red");
        assert_eq!(hsl(-40.0, 0.9, 0.58), hsl(320.0, 0.9, 0.58), "negative hues wrap");
    }

    #[test]
    fn welcome_matches_the_design() {
        let m = Model::installing();
        assert_eq!(m.window_title(), "openiCUE Setup");
        assert_eq!(m.heading(), "Install openiCUE");
        assert_eq!((m.primary(), m.secondary()), (Some("Install"), Some("Cancel")));
        assert_eq!(m.lit_leds(), 10);
        assert_eq!(m.hint(), "");
    }

    #[test]
    fn progress_marks_steps_and_lights_leds() {
        let mut m = Model::installing();
        m.screen = Screen::Progress;
        m.step = 2;
        assert_eq!(m.heading(), "Installing");
        assert_eq!(m.step_state(1), StepState::Done);
        assert_eq!(m.step_state(2), StepState::Active);
        assert_eq!(m.step_state(3), StepState::Waiting);
        assert_eq!(m.lit_leds(), 5);
        assert_eq!((m.primary(), m.secondary()), (None, None));
        assert_eq!(m.hint(), "This usually takes under a minute.");
    }

    #[test]
    fn done_and_failed_have_the_design_text() {
        let mut m = Model::installing();
        m.screen = Screen::Done;
        assert_eq!(m.heading(), "openiCUE is ready");
        assert_eq!((m.primary(), m.secondary()), (Some("Open openiCUE"), Some("Close")));
        m.screen = Screen::Failed;
        m.failure = Some(Failure::PawnIoCheck);
        assert_eq!(m.heading(), "Setup stopped");
        assert!(m.body().unwrap().starts_with("The PawnIO file failed its security check"));
        assert_eq!((m.primary(), m.secondary()), (Some("Close"), None));
        m.failure = Some(Failure::Other("Close openiCUE, then run Setup again.".into()));
        assert_eq!(m.body().unwrap(), "Close openiCUE, then run Setup again.");
        assert_eq!(m.lit_leds(), 0);
    }

    #[test]
    fn uninstall_screens() {
        let mut m = Model::uninstalling();
        assert_eq!(m.window_title(), "openiCUE");
        assert_eq!(m.heading(), "Uninstall openiCUE");
        assert_eq!((m.primary(), m.secondary()), (Some("Uninstall"), Some("Cancel")));
        assert!(!m.remove_pawnio);
        m.screen = Screen::Removed;
        assert_eq!(m.heading(), "openiCUE is removed");
        assert_eq!((m.primary(), m.secondary()), (Some("Close"), None));
    }
}
