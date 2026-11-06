//! Background engine: `openicue-engine.exe --restore | --run`.
//!
//! Exit codes, readable through `Start-Process -Wait -PassThru`:
//! - 0: built-in effect applied, or a custom effect stopped on request
//! - 2: missing, unknown or extra arguments
//! - 3: another engine is already running, or the claim failed
//! - 4: could not open the stop event
//! - 5: could not open PawnIO
//! - 6: no sticks found
//! - 7: applying a built-in effect failed on at least one stick
//! - 8: a direct write failed while animating a custom effect
#![windows_subsystem = "windows"]
use openicue_core::bus::Bus;
use openicue_core::effects::Settings;
use openicue_core::pawnio::PawnIoBus;
use openicue_core::ram::{self, StickInfo};
use openicue_core::{frames, system};

const FRAME_MS: u32 = 33;
/// Animation time restarts each day, so `f32` seconds keep their precision.
const ANIMATION_PERIOD_SECONDS: f64 = 86_400.0;

fn main() {
    let mut args = std::env::args().skip(1);
    let mode = args.next();
    if args.next().is_some() {
        std::process::exit(2);
    }
    let code = match mode.as_deref() {
        Some("--restore") | Some("--run") => run(),
        _ => 2,
    };
    std::process::exit(code);
}

fn sticks(bus: &mut dyn Bus) -> Vec<StickInfo> {
    ram::detect(bus)
        .into_iter()
        .filter_map(|addr| ram::read_info(bus, addr).ok())
        .collect()
}

fn run() -> i32 {
    let Ok(Some(_claim)) = system::claim_engine() else { return 3 };
    let Ok(stop) = system::open_stop_event() else { return 4 };
    let Ok(mut bus) = PawnIoBus::open() else { return 5 };
    let found = sticks(&mut bus);
    if found.is_empty() {
        return 6;
    }
    let settings = system::load_settings();
    if settings.effect.is_custom() {
        animate(&mut bus, &found, &settings, &stop)
    } else {
        let failed = found.iter().any(|s| ram::apply_builtin(&mut bus, s, &settings).is_err());
        if failed { 7 } else { 0 }
    }
}

fn animate(bus: &mut dyn Bus, found: &[StickInfo], settings: &Settings, stop: &system::StopEvent) -> i32 {
    let start = std::time::Instant::now();
    loop {
        let seconds = (start.elapsed().as_secs_f64() % ANIMATION_PERIOD_SECONDS) as f32;
        let frame = frames::render(settings, seconds);
        for (stick, leds) in found.iter().zip(frame.iter()) {
            if ram::write_direct(bus, stick, leds).is_err() {
                return 8;
            }
        }
        if stop.wait_ms(FRAME_MS) {
            return 0;
        }
    }
}
