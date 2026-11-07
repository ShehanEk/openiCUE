//! Background engine: `openicue-engine.exe --restore | --run`.
//!
//! Exit codes, readable through `Start-Process -Wait -PassThru`:
//! - 0: built-in effect applied (`--run`) or already kept by the sticks
//!   (`--restore`), or an effect drawn in software (custom, or a built-in
//!   kept in sync) stopped on request
//! - 2: missing, unknown or extra arguments
//! - 3: another engine is already running
//! - 4: could not open the stop event
//! - 5: could not open PawnIO
//! - 6: no sticks found
//! - 7: applying a built-in effect failed on at least one stick
//! - 8: a direct write failed several times in a row while animating
//! - 9: couldn't create the engine lock (usually not run as admin)
#![windows_subsystem = "windows"]
use openicue_core::bus::Bus;
use openicue_core::effects::{Rgb, Settings};
use openicue_core::pawnio::PawnIoBus;
use openicue_core::ram::{self, StickInfo, LED_COUNT};
use openicue_core::{frames, system};

/// 10 frames per second (user choice, 2026-11-06). Each stick write keeps the
/// CPU busy for about 3.4 ms while the SMBus sends it, so CPU is about
/// writes per second x 3.4 ms: ~14% of a core here, ~20% at 15 fps.
const FRAME_MS: u32 = 100;
/// Animation time restarts each day, so `f32` seconds keep their precision.
const ANIMATION_PERIOD_SECONDS: f64 = 86_400.0;
/// A stick may fail this many direct writes in a row before the engine gives up.
const MAX_FAILED_WRITES: u32 = 3;

fn main() {
    let mut args = std::env::args().skip(1);
    let mode = args.next();
    if args.next().is_some() {
        std::process::exit(2);
    }
    let code = match mode.as_deref() {
        Some("--restore") => run(true),
        Some("--run") => run(false),
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

fn run(restore: bool) -> i32 {
    // Open the stop event before claiming, so a window that sees the claim
    // always finds the event to signal. An engine that loses the claim
    // never waits on it, so holding it briefly is harmless.
    let Ok(stop) = system::open_stop_event() else { return 4 };
    let _claim = match system::claim_engine() {
        Ok(Some(claim)) => claim,
        Ok(None) => return 3,
        Err(_) => return 9,
    };
    // The sticks keep built-in effects through restarts and power-off
    // (hardware stage 5), and every rewrite saves to their own memory,
    // so a logon restore only has work to do for effects drawn in software:
    // custom effects and built-ins kept in sync.
    if restore && !system::load_settings().runs_in_software() {
        return 0;
    }
    let Ok(mut bus) = PawnIoBus::open() else { return 5 };
    let found = sticks(&mut bus);
    if found.is_empty() {
        return 6;
    }
    let settings = system::load_settings();
    if settings.runs_in_software() {
        animate(&mut bus, &found, &settings, &stop)
    } else {
        // Try every stick, even after one fails.
        // All sticks at once, so their animations start in sync.
        let failed = ram::apply_builtin_all(&mut bus, &found, &settings).iter().filter(|r| r.is_err()).count();
        if failed > 0 { 7 } else { 0 }
    }
}

/// Counts one stick's failed writes in a row. Returns true when it's time to give up.
fn note_write(failures_in_a_row: &mut u32, ok: bool) -> bool {
    *failures_in_a_row = if ok { 0 } else { *failures_in_a_row + 1 };
    *failures_in_a_row >= MAX_FAILED_WRITES
}

/// Draws custom effects, and built-ins kept in sync, from one clock.
/// Only the first `frames::STICKS_IN_FRAME` (4) detected sticks animate,
/// because a frame holds colors for 4 sticks; the `zip` drops the rest.
fn animate(bus: &mut dyn Bus, found: &[StickInfo], settings: &Settings, stop: &system::StopEvent) -> i32 {
    let start = std::time::Instant::now();
    let mut failures = vec![0u32; found.len()];
    // The colors each stick last accepted. A stick whose colors haven't
    // changed is skipped, since every write costs CPU time.
    let mut shown: Vec<Option<[Rgb; LED_COUNT]>> = vec![None; found.len()];
    loop {
        let frame_start = std::time::Instant::now();
        let seconds = (start.elapsed().as_secs_f64() % ANIMATION_PERIOD_SECONDS) as f32;
        let frame = frames::render(settings, seconds);
        let sticks = found.iter().zip(frame.iter()).zip(failures.iter_mut()).zip(shown.iter_mut());
        for (((stick, leds), count), last) in sticks {
            if !needs_write(last, leds) {
                continue;
            }
            let ok = ram::write_direct(bus, stick, leds).is_ok();
            *last = ok.then_some(*leds);
            if note_write(count, ok) {
                return 8;
            }
        }
        if stop.wait_ms(remaining_ms(frame_start.elapsed())) {
            return 0;
        }
    }
}

/// True unless the stick already shows exactly these colors.
fn needs_write(last: &Option<[Rgb; LED_COUNT]>, leds: &[Rgb; LED_COUNT]) -> bool {
    last.as_ref() != Some(leds)
}

/// Time left in this frame, so write time doesn't slow the frame rate.
fn remaining_ms(spent: std::time::Duration) -> u32 {
    FRAME_MS.saturating_sub(spent.as_millis().min(u32::MAX as u128) as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gives_up_after_three_failed_writes_in_a_row() {
        let mut count = 0;
        assert!(!note_write(&mut count, false));
        assert!(!note_write(&mut count, false));
        assert!(note_write(&mut count, false));
    }

    #[test]
    fn unchanged_sticks_are_skipped_and_failed_ones_retried() {
        let red = [Rgb(255, 0, 0); LED_COUNT];
        let blue = [Rgb(0, 0, 255); LED_COUNT];
        assert!(needs_write(&None, &red), "first frame, or after a failed write");
        assert!(!needs_write(&Some(red), &red));
        assert!(needs_write(&Some(red), &blue));
    }

    #[test]
    fn frame_pacing_subtracts_write_time() {
        use std::time::Duration;
        assert_eq!(remaining_ms(Duration::from_millis(0)), FRAME_MS);
        assert_eq!(remaining_ms(Duration::from_millis(16)), FRAME_MS - 16);
        assert_eq!(remaining_ms(Duration::from_secs(5)), 0);
    }

    #[test]
    fn a_success_resets_the_count() {
        let mut count = 0;
        assert!(!note_write(&mut count, false));
        assert!(!note_write(&mut count, false));
        assert!(!note_write(&mut count, true));
        assert_eq!(count, 0);
        assert!(!note_write(&mut count, false));
        assert!(!note_write(&mut count, false));
    }
}
