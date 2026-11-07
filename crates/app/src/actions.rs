//! Calls that reach the hardware or the engine. The window logic stays in `model`.
use openicue_core::control;
use openicue_core::effects::Settings;
use openicue_core::pawnio::PawnIoBus;
use openicue_core::ram;
use openicue_core::system;
use std::time::Duration;

const STOP_TIMEOUT: Duration = Duration::from_secs(2);
const BUSY: &str = "Couldn't reach the RAM. Close other RGB apps and try again.";

pub struct Hardware {
    pub bus: PawnIoBus,
}

pub enum Startup {
    NoDriver,
    NoSticks,
    Ready(Hardware, usize),
}

/// Opens PawnIO and finds the sticks. Detection only reads ID registers, so
/// it's safe while an engine is animating.
pub fn startup() -> Startup {
    let Ok(mut bus) = PawnIoBus::open() else { return Startup::NoDriver };
    let count = ram::detect(&mut bus).len();
    if count == 0 { Startup::NoSticks } else { Startup::Ready(Hardware { bus }, count) }
}

fn stop_first() -> Result<(), String> {
    control::stop_engine(STOP_TIMEOUT).map_err(|_| "The background process didn't stop. Try again.".to_string())
}

impl Hardware {
    /// Stops any engine, then writes the effect to every stick while holding
    /// the engine claim, so no other openiCUE process writes at the same time.
    pub fn apply_builtin(&mut self, settings: &Settings) -> Result<(), String> {
        stop_first()?;
        let Ok(Some(_claim)) = system::claim_engine() else { return Err(BUSY.to_string()) };
        let sticks = ram::detect(&mut self.bus);
        if sticks.is_empty() {
            return Err(BUSY.to_string());
        }
        let infos: Vec<ram::StickInfo> = sticks.iter().filter_map(|&addr| ram::read_info(&mut self.bus, addr).ok()).collect();
        // All sticks at once, so their animations start in sync.
        let applied = ram::apply_builtin_all(&mut self.bus, &infos, settings).iter().filter(|r| r.is_ok()).count();
        let failed = sticks.len() - applied;
        if failed > 0 {
            return Err(format!("{failed} of {} sticks didn't take it. Close other RGB apps and try again.", sticks.len()));
        }
        system::save_settings(settings).map_err(|_| "Applied, but couldn't save it for next time.".to_string())
    }
}

/// Held while the stick-order picker is open: the engine claim, so no other
/// openiCUE process writes to the RAM meanwhile, and the sticks being placed.
pub struct Arranging {
    _claim: system::EngineClaim,
    sticks: Vec<ram::StickInfo>,
}

impl Arranging {
    pub fn count(&self) -> usize {
        self.sticks.len()
    }
}

impl Hardware {
    /// Stops any engine, takes the claim and lights the first stick white.
    /// Needs at least 2 sticks, since one has nothing to order.
    pub fn begin_arrange(&mut self) -> Result<Arranging, String> {
        stop_first()?;
        let Ok(Some(claim)) = system::claim_engine() else { return Err(BUSY.to_string()) };
        let sticks: Vec<ram::StickInfo> = ram::detect(&mut self.bus)
            .into_iter()
            .filter_map(|addr| ram::read_info(&mut self.bus, addr).ok())
            .collect();
        if sticks.len() < 2 {
            return Err("Stick order needs at least 2 sticks that answer. Close other RGB apps and try again.".to_string());
        }
        let arranging = Arranging { _claim: claim, sticks };
        self.light(&arranging, 0)?;
        Ok(arranging)
    }

    /// Lights `stick` (address order) white and the others black.
    pub fn light(&mut self, arranging: &Arranging, stick: usize) -> Result<(), String> {
        ram::light_only(&mut self.bus, &arranging.sticks, stick).map_err(|_| BUSY.to_string())
    }
}

/// Puts the lights back after the picker by re-applying the saved effect,
/// as Apply would. Call it only after the `Arranging` claim is dropped.
pub fn restore_saved(hardware: Option<&mut Hardware>) -> Result<(), String> {
    let settings = system::load_settings();
    if settings.runs_in_software() {
        return apply_custom(&settings);
    }
    match hardware {
        Some(hardware) => hardware.apply_builtin(&settings),
        None => Err("Couldn't reach the RAM. Check again.".to_string()),
    }
}

/// How long a fresh engine gets to claim its lock before we call it failed.
const START_TIMEOUT: Duration = Duration::from_secs(1);
const START_POLL: Duration = Duration::from_millis(50);

/// For effects drawn in software (custom, or built-ins kept in sync): saves the
/// effect, then starts a fresh engine and waits until it is really
/// running, since the process can exit at once (codes 3, 5, 6, 9).
pub fn apply_custom(settings: &Settings) -> Result<(), String> {
    const NOT_STARTED: &str = "The background process didn't start. Try again.";
    stop_first()?;
    system::save_settings(settings).map_err(|_| "Couldn't save the effect.".to_string())?;
    control::start_engine().map_err(|_| NOT_STARTED.to_string())?;
    let deadline = std::time::Instant::now() + START_TIMEOUT;
    while !control::engine_running() {
        if std::time::Instant::now() >= deadline {
            return Err(NOT_STARTED.to_string());
        }
        std::thread::sleep(START_POLL);
    }
    Ok(())
}

pub fn stop() -> Result<(), String> {
    stop_first()
}
