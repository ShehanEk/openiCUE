//! What the window shows. Pure logic, no Windows calls.
use openicue_core::costs::{Cost, Costs};
use openicue_core::effects::{Direction, Effect, Rgb, Settings, Speed, ALL_EFFECTS, BRIGHTNESS_RANGE, SPEED_PCT_RANGE};
use openicue_core::order::StickOrder;
pub use openicue_core::ram::{Family, InfoSummary};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    App,
    NoDriver,
    NoSticks,
    Settings,
    /// "Which slot is lit?": the user places each stick in its slot.
    Arrange,
    /// "Report an unsupported stick", from the notice under the preview.
    Report,
}

pub const REPORT_TITLE: &str = "Report an unsupported stick";
pub const REPORT_BODY: &str = "openiCUE found a Corsair stick it doesn't support yet, so it hasn't changed its lighting. This report helps add support for it. On GitHub, add your RAM's model from the label on the stick.";
pub const REPORT_PRIVACY: &str = "Your sticks' serial numbers are left out. This report only includes the vendor, product ID, firmware and protocol openiCUE needs to add support.";
pub const REPORT_FOOTER: &str = "Copies the report and opens github.com. Paste it into the form. Needs a GitHub account.";
/// The longest link Explorer passes on whole.
pub const MAX_LINK: usize = 259;
/// Where reports go: the project's issue form for unsupported RAM.
const ISSUE_FORM: &str = "https://github.com/ShehanEk/openiCUE/issues/new?template=unsupported-ram.yml";

/// Percent-encodes everything but RFC 3986's unreserved characters.
fn url_encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len() * 3);
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
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

/// The speed and brightness sliders.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slider {
    Speed,
    Brightness,
}

/// Both sliders move in steps of 5 percent.
pub const SLIDER_STEP: u16 = 5;

impl Slider {
    pub fn range(self) -> (u16, u16) {
        match self {
            Slider::Speed => SPEED_PCT_RANGE,
            Slider::Brightness => (BRIGHTNESS_RANGE.0.into(), BRIGHTNESS_RANGE.1.into()),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Slider::Speed => "Speed",
            Slider::Brightness => "Brightness",
        }
    }
}

/// Rounds to the nearest step, then keeps it in range.
fn snap(value: f32, min: u16, max: u16) -> u16 {
    let step = f32::from(SLIDER_STEP);
    let value = if value.is_finite() { (value / step).round() * step } else { f32::from(min) };
    value.clamp(f32::from(min), f32::from(max)) as u16
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
    /// Each found stick's model line, in address order; `None` if its info
    /// didn't read back.
    pub families: Vec<Option<Family>>,
    /// Whether the preview shows the sticks' models (a click toggles it).
    pub details_open: bool,
    /// How far the last open or close has run, from 0 to 1.
    pub details_progress: f32,
    /// Corsair sticks that answered but aren't supported yet: (address, info).
    pub unsupported: Vec<(u8, InfoSummary)>,
    /// "Windows 11, build 26200", for the report.
    pub os: String,
}

/// The preview's open and close: each row takes `DETAILS_ROW_MS`, and each
/// starts `DETAILS_STAGGER_MS` after the one before it.
pub const DETAILS_ROW_MS: f32 = 450.0;
pub const DETAILS_STAGGER_MS: f32 = 50.0;
pub const DETAILS_TOTAL_MS: f32 = DETAILS_ROW_MS + 3.0 * DETAILS_STAGGER_MS;
/// A preview row's height with the details open.
pub const PREVIEW_ROW_OPEN: f32 = 52.0;

/// Fast at first, gentle at the end, like the design's cubic-bezier(.2,.8,.2,1).
fn ease_out(t: f32) -> f32 {
    1.0 - (1.0 - t.clamp(0.0, 1.0)).powi(3)
}

pub fn family_name(family: Option<Family>) -> &'static str {
    match family {
        Some(Family::Pro) => "Vengeance RGB Pro",
        Some(Family::ProSl) => "Vengeance RGB Pro SL",
        Some(Family::Rs) => "Vengeance RGB RS",
        None => "Corsair RGB stick",
    }
}

pub fn family_leds(family: Option<Family>) -> usize {
    if family == Some(Family::Rs) { 6 } else { 10 }
}

impl Model {
    pub fn new(settings: Settings) -> Model {
        Model {
            families: Vec::new(),
            details_open: false,
            details_progress: 1.0,
            unsupported: Vec::new(),
            os: String::new(),
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

    /// Opens or closes the stick details, from wherever the last move got to.
    pub fn toggle_details(&mut self) {
        // Reversing midway starts where the other direction shows the same
        // amount, so nothing jumps: solve the ease for it.
        let amount = self.details_amount();
        self.details_open = !self.details_open;
        let eased = if self.details_open { 1.0 - (1.0 - amount).cbrt() } else { 1.0 - amount.cbrt() };
        self.details_progress = eased * DETAILS_ROW_MS / DETAILS_TOTAL_MS;
    }

    /// How open row `row` is (0 closed, 1 open). Opening runs top to bottom,
    /// closing bottom to top.
    pub fn details_row(&self, row: usize) -> f32 {
        let order = if self.details_open { row } else { 3usize.saturating_sub(row) };
        let elapsed = self.details_progress.clamp(0.0, 1.0) * DETAILS_TOTAL_MS - order as f32 * DETAILS_STAGGER_MS;
        let k = ease_out(elapsed / DETAILS_ROW_MS);
        if self.details_open { k } else { 1.0 - k }
    }

    /// How open the preview is overall, for its height. Like the design, the
    /// height moves over one row's time, without the stagger.
    pub fn details_amount(&self) -> f32 {
        let k = ease_out(self.details_progress.clamp(0.0, 1.0) * DETAILS_TOTAL_MS / DETAILS_ROW_MS);
        if self.details_open { k } else { 1.0 - k }
    }

    /// Row `row`'s height in the preview: a 16 px LED bar closed, a 52 px
    /// stick open. Each row grows with its own stick, as in the design.
    pub fn preview_row_h(&self, row: usize) -> f32 {
        16.0 + (PREVIEW_ROW_OPEN - 16.0) * self.details_row(row)
    }

    /// The space between preview rows: 8 px closed, 10 px open.
    pub fn preview_gap(&self) -> f32 {
        8.0 + 2.0 * self.details_amount()
    }

    /// The whole preview's height: 4 rows and the 3 gaps between them.
    pub fn preview_height(&self) -> f32 {
        (0..4).map(|row| self.preview_row_h(row)).sum::<f32>() + 3.0 * self.preview_gap()
    }

    /// How visible row `row`'s label is. As in the design, labels fade in over
    /// 300 ms, 150 ms after their stick starts; closing, they fade out first.
    pub fn details_label(&self, row: usize) -> f32 {
        let ms = self.details_progress.clamp(0.0, 1.0) * DETAILS_TOTAL_MS;
        if self.details_open {
            ease_out((ms - row as f32 * DETAILS_STAGGER_MS - 150.0) / 300.0)
        } else {
            1.0 - ease_out((ms - 3usize.saturating_sub(row) as f32 * 30.0) / 300.0)
        }
    }

    /// The model line of the stick in physical slot `slot`.
    pub fn slot_family(&self, slot: usize) -> Option<Family> {
        self.order.stick_at(slot).and_then(|s| self.families.get(s).copied().flatten())
    }

    pub fn open_settings(&mut self) {
        if self.screen == Screen::App {
            self.screen = Screen::Settings;
            self.message.clear();
        }
    }

    /// Settings goes back to the app; the picker goes back to Settings, unsaved.
    /// The notice under the preview, when some sticks aren't supported.
    pub fn unsupported_notice(&self) -> Option<String> {
        match self.unsupported.len() {
            0 => None,
            1 => Some("1 stick isn't supported yet, so openiCUE leaves it alone.".to_string()),
            n => Some(format!("{n} sticks aren't supported yet, so openiCUE leaves them alone.")),
        }
    }

    pub fn open_report(&mut self) {
        if self.screen == Screen::App && !self.unsupported.is_empty() {
            self.screen = Screen::Report;
            self.message.clear();
        }
    }

    /// The report, exactly as it is copied or sent. It holds no serial numbers.
    pub fn report_text(&self) -> String {
        let mut lines = vec![
            format!("openiCUE {}", env!("CARGO_PKG_VERSION")),
            self.os.clone(),
            "Chipset: AMD (SMBus PIIX4)".to_string(),
            format!("Supported sticks found: {}", self.sticks),
        ];
        for (addr, s) in &self.unsupported {
            let vendor = if s.vendor == 0x1B1C { " (Corsair)" } else { "" };
            let (major, minor, build) = s.firmware;
            lines.push(String::new());
            lines.push(format!("Unsupported stick at 0x{addr:02X}"));
            lines.push(format!("  Vendor     0x{:04X}{vendor}", s.vendor));
            lines.push(format!("  Product    0x{:04X}", s.product_id));
            lines.push(format!("  Firmware   {major}.{minor}.{build}"));
            lines.push(format!("  Protocol   {}", s.protocol));
        }
        lines.join("\n")
    }

    /// A new issue on the project's unsupported-RAM form, with the title filled
    /// in. The report itself goes on the clipboard: Explorer, which opens the
    /// link as the signed-in user, cuts links at 259 characters (user test,
    /// 2026-11-15), and the report alone needs about 400.
    pub fn report_url(&self) -> String {
        let mut products: Vec<String> = self.unsupported.iter().map(|(_, s)| format!("0x{:04X}", s.product_id)).collect();
        products.sort();
        products.dedup();
        let url = format!("{ISSUE_FORM}&title={}", url_encode(&format!("Unsupported RAM: product {}", products.join(", "))));
        if url.len() <= MAX_LINK { url } else { format!("{ISSUE_FORM}&title={}", url_encode("Unsupported RAM")) }
    }

    pub fn back(&mut self) {
        match self.screen {
            Screen::Settings | Screen::Report => self.screen = Screen::App,
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

    /// Picking another effect starts it in sync where it can be (2026-11-09),
    /// with its own number of colours. Picking the same one again keeps both.
    pub fn select(&mut self, effect: Effect) {
        if effect != self.settings.effect {
            self.settings.effect = effect;
            self.settings.synced = effect.allows_sync();
            self.settings.color_count = effect.color_count();
        }
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

    /// `index` is 1 to 4. Anything else is ignored.
    pub fn set_color(&mut self, index: u8, color: Rgb) {
        match index {
            1 => self.settings.color1 = color,
            2 => self.settings.color2 = color,
            3 => self.settings.more_colors[0] = color,
            4 => self.settings.more_colors[1] = color,
            _ => {}
        }
    }

    /// Colour `index`, from 1 to 4.
    pub fn color(&self, index: u8) -> Option<Rgb> {
        let i = usize::from(index).checked_sub(1)?;
        self.settings.colors().get(i).copied()
    }

    /// Software-drawn effects, and Static, get the extra settings: more
    /// colours, a speed slider, brightness and Reverse. The RAM's own
    /// animations keep only the RAM's options.
    pub fn soft_opts(&self) -> bool {
        self.settings.runs_in_software() || self.settings.effect == Effect::Static
    }

    /// The fewest and most colours the user can pick: Static 1, effects with colours 1 to 4.
    pub fn color_limits(&self) -> (u8, u8) {
        self.settings.effect.color_limits()
    }

    /// The colour swatches and + and −, hidden while Random is on.
    pub fn show_soft_colors(&self) -> bool {
        self.color_limits().1 > 0 && !self.settings.random
    }

    pub fn can_add_color(&self) -> bool {
        self.show_soft_colors() && self.settings.color_count < self.color_limits().1
    }

    pub fn can_remove_color(&self) -> bool {
        self.show_soft_colors() && self.settings.color_count > self.color_limits().0.max(1)
    }

    pub fn add_color(&mut self) {
        if self.can_add_color() {
            self.settings.color_count += 1;
        }
    }

    /// Drops the last colour. Its value is kept for when it's added back.
    pub fn remove_color(&mut self) {
        if self.can_remove_color() {
            self.settings.color_count -= 1;
        }
    }

    pub fn show_speed_slider(&self) -> bool {
        self.settings.effect.has_speed()
    }

    pub fn set_speed_pct(&mut self, pct: f32) {
        self.settings.speed_pct = snap(pct, SPEED_PCT_RANGE.0, SPEED_PCT_RANGE.1);
    }

    pub fn set_brightness(&mut self, pct: f32) {
        let (low, high) = BRIGHTNESS_RANGE;
        self.settings.brightness = snap(pct, low.into(), high.into()) as u8;
    }

    /// Reverse, for software effects that move but have no direction choice.
    pub fn show_reverse(&self) -> bool {
        self.settings.runs_in_software() && self.settings.effect.has_speed() && self.directions().is_empty()
    }

    pub fn toggle_reverse(&mut self) {
        if self.show_reverse() {
            self.settings.reverse = !self.settings.reverse;
        }
    }

    /// The direction group, among the software options.
    pub fn show_soft_direction(&self) -> bool {
        self.settings.runs_in_software() && !self.directions().is_empty()
    }

    /// "1.25×": the speed as a multiple, with at most one trailing zero dropped.
    pub fn speed_text(&self) -> String {
        let text = format!("{:.2}", f32::from(self.settings.speed_pct) / 100.0);
        format!("{}×", text.strip_suffix('0').unwrap_or(&text))
    }

    pub fn brightness_text(&self) -> String {
        format!("{}%", self.settings.brightness)
    }

    pub fn slider_value(&self, slider: Slider) -> u16 {
        match slider {
            Slider::Speed => self.settings.speed_pct,
            Slider::Brightness => self.settings.brightness.into(),
        }
    }

    pub fn set_slider(&mut self, slider: Slider, value: f32) {
        match slider {
            Slider::Speed => self.set_speed_pct(value),
            Slider::Brightness => self.set_brightness(value),
        }
    }

    /// One step left (`-1`) or right (`1`), for the arrow keys.
    pub fn nudge(&mut self, slider: Slider, steps: i32) {
        let value = f32::from(self.slider_value(slider)) + (steps * i32::from(SLIDER_STEP)) as f32;
        self.set_slider(slider, value);
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
                "The RAM runs this itself. Applying stops the background process."
            } else if effect == Effect::Static {
                "The RAM runs this itself, so nothing keeps running."
            } else {
                "The RAM runs this itself, but the sticks drift out of sync."
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
        // Settings, the picker and the report show only messages, such as errors.
        if matches!(self.screen, Screen::Settings | Screen::Arrange | Screen::Report) {
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
        m.toggle_sync();
        assert_eq!(m.cost_line(), "0 MB · 0% CPU");
        assert!(!m.cost_is_ongoing());
        assert_eq!(m.cost_body(), "The RAM runs this itself, but the sticks drift out of sync.");
        m.running = Some(Effect::Chase);
        assert_eq!(m.cost_body(), "The RAM runs this itself. Applying stops the background process.");

        let mut m = model(Effect::Static);
        assert_eq!(m.cost_line(), "0 MB · 0% CPU");
        assert_eq!(m.cost_body(), "The RAM runs this itself, so nothing keeps running.");
        m.running = Some(Effect::Chase);
        assert_eq!(m.cost_body(), "The RAM runs this itself. Applying stops the background process.");
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
            assert_eq!(m.settings.synced, shown, "{effect:?} starts in sync where it can");
            m.toggle_sync();
            assert!(!m.settings.synced, "{effect:?}");
        }
        let mut m = model(Effect::ColorWave);
        m.toggle_sync();
        m.toggle_sync();
        assert!(m.settings.synced);
        m.select(Effect::Static);
        assert!(!m.settings.synced, "select drops sync where it isn't allowed");
    }

    #[test]
    fn select_turns_sync_on_and_keeps_an_untick_on_the_same_tile() {
        let mut m = model(Effect::ColorWave);
        m.toggle_sync();
        m.select(Effect::ColorWave);
        assert!(!m.settings.synced, "the same tile keeps the user's choice");
        m.select(Effect::Rain);
        assert!(m.settings.synced, "another effect starts in sync");
        for effect in openicue_core::effects::ALL_EFFECTS {
            assert_eq!(model(effect).settings.synced, effect.allows_sync(), "{effect:?}");
        }
        // Saved settings keep the user's choice.
        let saved = Settings { effect: Effect::Visor, synced: false, ..Settings::default() };
        assert!(!Model::new(saved).settings.synced);
    }

    #[test]
    fn extra_options_show_for_software_effects_and_static() {
        for effect in openicue_core::effects::ALL_EFFECTS {
            let mut m = model(effect);
            assert!(m.soft_opts(), "{effect:?} is in sync or Static");
            if effect.allows_sync() {
                m.toggle_sync();
                assert!(!m.soft_opts(), "{effect:?} on the RAM keeps the RAM's options");
            }
        }
        let m = model(Effect::Static);
        assert!(m.show_soft_colors() && !m.can_add_color() && !m.can_remove_color());
        assert!(!m.show_speed_slider() && !m.show_reverse() && !m.show_soft_direction());
    }

    #[test]
    fn colours_add_and_remove_within_the_limits() {
        let mut m = model(Effect::ColorWave);
        assert_eq!((m.settings.color_count, m.color_limits()), (2, (1, 4)));
        m.add_color();
        m.add_color();
        assert_eq!(m.settings.color_count, 4);
        assert!(!m.can_add_color());
        m.add_color();
        assert_eq!(m.settings.color_count, 4, "4 at most");
        for _ in 0..5 {
            m.remove_color();
        }
        assert_eq!(m.settings.color_count, 1, "1 at least");
        assert!(!m.can_remove_color() && m.can_add_color());

        let mut m = model(Effect::Marquee);
        assert_eq!(m.settings.color_count, 1, "a one-colour effect starts with one");
        m.add_color();
        assert_eq!(m.settings.color_count, 2);
        m.select(Effect::Chase);
        assert_eq!(m.settings.color_count, 2, "each effect starts with its own count");

        let mut m = model(Effect::Rainbow);
        assert_eq!((m.settings.color_count, m.color_limits()), (0, (0, 0)));
        assert!(!m.show_soft_colors());
        m.add_color();
        assert_eq!(m.settings.color_count, 0);

        let mut m = model(Effect::ColorShift);
        m.toggle_random();
        assert!(!m.show_soft_colors() && !m.can_add_color() && !m.can_remove_color(), "random hides them");
        m.add_color();
        assert_eq!(m.settings.color_count, 2);
    }

    #[test]
    fn colours_three_and_four_are_kept_and_set() {
        let mut m = model(Effect::Gradient);
        assert_eq!(m.color(3), Some(Rgb(0x7C, 0xFF, 0xB2)));
        assert_eq!(m.color(4), Some(Rgb(0xC7, 0x7D, 0xFF)));
        m.set_color(3, Rgb(1, 2, 3));
        m.set_color(4, Rgb(4, 5, 6));
        assert_eq!(m.settings.more_colors, [Rgb(1, 2, 3), Rgb(4, 5, 6)]);
        assert_eq!((m.color(0), m.color(5)), (None, None));
        m.set_color(5, Rgb(9, 9, 9));
        assert_eq!(m.settings.colors()[..2], [Settings::default().color1, Settings::default().color2]);
    }

    #[test]
    fn sliders_snap_to_steps_and_stay_in_range() {
        let mut m = model(Effect::Chase);
        m.set_speed_pct(127.4);
        assert_eq!(m.settings.speed_pct, 125);
        m.set_speed_pct(1.0);
        assert_eq!(m.settings.speed_pct, 25);
        m.set_speed_pct(9_999.0);
        assert_eq!(m.settings.speed_pct, 300);
        m.set_speed_pct(f32::NAN);
        assert_eq!(m.settings.speed_pct, 25);
        m.set_brightness(82.6);
        assert_eq!(m.settings.brightness, 85);
        m.set_brightness(0.0);
        assert_eq!(m.settings.brightness, 10);
        m.set_brightness(400.0);
        assert_eq!(m.settings.brightness, 100);

        m.nudge(Slider::Brightness, -1);
        assert_eq!(m.slider_value(Slider::Brightness), 95);
        m.set_speed_pct(300.0);
        m.nudge(Slider::Speed, 1);
        assert_eq!(m.slider_value(Slider::Speed), 300, "already at the top");
        m.nudge(Slider::Speed, -1);
        assert_eq!(m.slider_value(Slider::Speed), 295);
        m.set_slider(Slider::Speed, 100.0);
        assert_eq!(m.settings.speed_pct, 100);
        assert_eq!((Slider::Speed.range(), Slider::Brightness.range()), ((25, 300), (10, 100)));
        assert_eq!((Slider::Speed.label(), Slider::Brightness.label()), ("Speed", "Brightness"));
    }

    #[test]
    fn slider_values_read_like_the_design() {
        let mut m = model(Effect::Chase);
        for (pct, text) in [(100, "1.0×"), (125, "1.25×"), (150, "1.5×"), (25, "0.25×"), (300, "3.0×"), (205, "2.05×")] {
            m.set_speed_pct(pct as f32);
            assert_eq!(m.speed_text(), text);
        }
        m.set_brightness(80.0);
        assert_eq!(m.brightness_text(), "80%");
    }

    #[test]
    fn reverse_and_direction_follow_the_effect() {
        let mut m = model(Effect::Chase);
        assert!(m.show_reverse() && !m.show_soft_direction());
        m.toggle_reverse();
        assert!(m.settings.reverse);
        let mut m = model(Effect::ColorShift);
        assert!(m.show_reverse(), "a synced built-in that moves");
        m.toggle_sync();
        assert!(!m.show_reverse());
        m.toggle_reverse();
        assert!(!m.settings.reverse, "ignored where it isn't shown");
        let m = model(Effect::Rain);
        assert!(!m.show_reverse() && m.show_soft_direction());
        let m = model(Effect::Static);
        assert!(!m.show_reverse());
    }

    #[test]
    fn synced_built_in_costs_like_a_custom_effect() {
        let mut m = model(Effect::ColorWave);
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
        assert_eq!(m.cost_body(), "The RAM runs this itself. Applying stops the background process.");
        assert_eq!(m.apply_label(), "Apply");
    }

    fn with_unsupported() -> Model {
        let mut m = Model::new(Settings::default());
        m.sticks = 3;
        m.os = "Windows 11, build 26200".into();
        m.unsupported = vec![(0x5B, InfoSummary { vendor: 0x1B1C, product_id: 0x0201, firmware: (0, 6, 36), protocol: 4 })];
        m
    }

    #[test]
    fn report_matches_the_design_and_has_no_serial() {
        let m = with_unsupported();
        let expected = format!(
            "openiCUE {}\nWindows 11, build 26200\nChipset: AMD (SMBus PIIX4)\nSupported sticks found: 3\n\n\
             Unsupported stick at 0x5B\n  Vendor     0x1B1C (Corsair)\n  Product    0x0201\n  Firmware   0.6.36\n  Protocol   4",
            env!("CARGO_PKG_VERSION")
        );
        assert_eq!(m.report_text(), expected);
        assert_eq!(m.unsupported_notice().unwrap(), "1 stick isn't supported yet, so openiCUE leaves it alone.");
        let mut two = with_unsupported();
        two.unsupported.push(two.unsupported[0]);
        assert_eq!(two.unsupported_notice().unwrap(), "2 sticks aren't supported yet, so openiCUE leaves them alone.");
        assert_eq!(Model::new(Settings::default()).unsupported_notice(), None);
    }

    #[test]
    fn report_url_fits_explorer_and_is_safe() {
        let url = with_unsupported().report_url();
        assert_eq!(url, "https://github.com/ShehanEk/openiCUE/issues/new?template=unsupported-ram.yml&title=Unsupported%20RAM%3A%20product%200x0201");
        // Only unreserved characters and escapes after the fixed prefix: no
        // stray `&`, `#` or spaces that could change the link.
        let query = &url[url.find("&title=").unwrap()..];
        assert!(query.chars().all(|c| c.is_ascii_alphanumeric() || "-._~%&=".contains(c)), "{query}");
        // Even with many different unknown sticks, the link stays whole.
        let mut many = with_unsupported();
        many.unsupported = (0..8u16).map(|i| (0x58 + i as u8, InfoSummary { product_id: 0x0200 + i, ..many.unsupported[0].1 })).collect();
        assert!(many.report_url().len() <= MAX_LINK, "{}", many.report_url().len());
    }

    #[test]
    fn report_screen_opens_only_with_something_to_report() {
        let mut m = Model::new(Settings::default());
        m.open_report();
        assert_eq!(m.screen, Screen::App);
        let mut m = with_unsupported();
        m.open_report();
        assert_eq!(m.screen, Screen::Report);
        assert_eq!(m.status_line(), "");
        m.back();
        assert_eq!(m.screen, Screen::App);
    }

    #[test]
    fn details_open_top_down_and_close_bottom_up() {
        let mut m = Model::new(Settings::default());
        assert_eq!((m.details_amount(), m.details_row(0)), (0.0, 0.0), "closed at start");
        m.toggle_details();
        assert_eq!(m.details_progress, 0.0);
        m.details_progress = 0.3;
        assert!(m.details_row(0) > m.details_row(3), "opening: top row leads");
        m.details_progress = 1.0;
        assert!((0..4).all(|r| m.details_row(r) == 1.0) && m.details_amount() == 1.0);
        m.toggle_details();
        m.details_progress = 0.3;
        assert!(m.details_row(3) < m.details_row(0), "closing: bottom row leads");
        m.details_progress = 1.0;
        assert!((0..4).all(|r| m.details_row(r) == 0.0));
    }

    #[test]
    fn reversing_midway_does_not_jump() {
        let mut m = Model::new(Settings::default());
        m.toggle_details();
        m.details_progress = 0.4;
        let before = m.details_amount();
        m.toggle_details();
        assert!((m.details_amount() - before).abs() < 0.25, "{before} -> {}", m.details_amount());
    }

    #[test]
    fn slots_show_their_sticks_family_in_saved_order() {
        let mut m = Model::new(Settings::default());
        m.families = vec![Some(Family::Rs), Some(Family::ProSl), None];
        m.order = StickOrder::identity(3);
        assert_eq!(m.slot_family(0), Some(Family::Rs));
        assert_eq!(m.slot_family(1), Some(Family::ProSl));
        assert_eq!(family_name(m.slot_family(2)), "Corsair RGB stick");
        assert_eq!(m.slot_family(3), None, "no stick in slot 4");
        assert_eq!(family_leds(Some(Family::Rs)), 6);
        assert_eq!(family_leds(None), 10);
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
