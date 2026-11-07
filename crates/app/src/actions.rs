//! Calls that reach the hardware or the engine. The window logic stays in `model`.
use openicue_core::pawnio::PawnIoBus;
use openicue_core::ram;

pub struct Hardware {
    // Read by Apply and Stop in Task 8.
    #[allow(dead_code)]
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
