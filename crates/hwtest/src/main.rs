use openicue_core::bus::{Bus, BusError, StickAddr};
use openicue_core::effects::{Effect, Rgb, Settings, ALL_EFFECTS};
use openicue_core::{pawnio::PawnIoBus, ram};
use std::process::exit;

/// NTSTATUS STATUS_ACCESS_DENIED, as `NtOpenFile` returns it.
const STATUS_ACCESS_DENIED: i32 = 0xC000_0022_u32 as i32;

const USAGE: &str = "usage: openicue-hwtest detect | info | one <58-5F> | effect <name>";

enum Command {
    Detect,
    Info,
    One(StickAddr),
    Effect(Effect),
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let command = parse(&args).unwrap_or_else(|message| {
        eprintln!("{message}");
        exit(2);
    });
    let mut bus = PawnIoBus::open().unwrap_or_else(|error| {
        match error {
            BusError::Os(STATUS_ACCESS_DENIED) => {
                eprintln!("PawnIO refused access. Run this from an admin terminal.")
            }
            other => eprintln!("Could not open PawnIO: {other:?}. Is PawnIO installed?"),
        }
        exit(1);
    });
    match command {
        Command::Detect => detect(&mut bus),
        Command::Info => info(&mut bus),
        Command::One(addr) => one_stick(&mut bus, addr),
        Command::Effect(chosen) => effect(&mut bus, chosen),
    }
}

/// Checks the arguments without touching the driver.
fn parse(args: &[&str]) -> Result<Command, String> {
    match args {
        ["detect"] => Ok(Command::Detect),
        ["info"] => Ok(Command::Info),
        ["one", address] => u8::from_str_radix(address, 16)
            .ok()
            .and_then(StickAddr::new)
            .map(Command::One)
            .ok_or_else(|| "address must be hex from 58 to 5F, such as 58".to_string()),
        ["effect", name] => Effect::from_name(name)
            .filter(|e| !e.is_custom())
            .map(Command::Effect)
            .ok_or_else(|| {
                let names: Vec<&str> =
                    ALL_EFFECTS.iter().filter(|e| !e.is_custom()).map(|e| e.name()).collect();
                format!("unknown effect. Choose from {names:?}")
            }),
        _ => Err(USAGE.to_string()),
    }
}

/// Read-only: reads the two ID registers and writes nothing.
fn detect(bus: &mut dyn Bus) {
    let sticks = ram::detect(bus);
    println!("Found {} sticks:", sticks.len());
    for stick in sticks {
        println!("  {:#04x}", stick.get());
    }
}

/// Reads the device info block of every detected stick. Changes no lighting.
fn info(bus: &mut dyn Bus) {
    for stick in ram::detect(bus) {
        match ram::read_info(bus, stick) {
            Ok(info) => println!(
                "{:#04x}: product {:#06x}, protocol {}",
                info.addr().get(), info.product_id(), info.protocol()
            ),
            Err(error) => println!("{:#04x}: {error:?}", stick.get()),
        }
    }
}

/// Writes one static red color to one detected stick.
fn one_stick(bus: &mut dyn Bus, addr: StickAddr) {
    if !ram::detect(bus).contains(&addr) {
        eprintln!("{:#04x} did not pass detection. Nothing written.", addr.get());
        exit(1);
    }
    let info = match ram::read_info(bus, addr) {
        Ok(info) => info,
        Err(error) => {
            eprintln!("{:#04x}: device info failed: {error:?}", addr.get());
            exit(1);
        }
    };
    let settings = Settings { effect: Effect::Static, color1: Rgb(255, 0, 0), ..Settings::default() };
    println!("{:?}", ram::apply_builtin(bus, &info, &settings));
}

/// Applies a built-in effect to every detected stick.
fn effect(bus: &mut dyn Bus, chosen: Effect) {
    let settings = Settings { effect: chosen, ..Settings::default() };
    for stick in ram::detect(bus) {
        let result = ram::read_info(bus, stick).and_then(|info| ram::apply_builtin(bus, &info, &settings));
        println!("{:#04x}: {result:?}", stick.get());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_commands_are_rejected_before_opening_the_driver() {
        assert!(parse(&["write"]).is_err());
        assert!(parse(&[]).is_err());
    }

    #[test]
    fn one_only_accepts_stick_addresses() {
        assert!(matches!(parse(&["one", "58"]), Ok(Command::One(a)) if a.get() == 0x58));
        assert!(parse(&["one", "57"]).is_err());
        assert!(parse(&["one", "60"]).is_err());
        assert!(parse(&["one", "zz"]).is_err());
    }

    #[test]
    fn effect_only_accepts_built_in_effects() {
        assert!(matches!(parse(&["effect", "rainbow"]), Ok(Command::Effect(Effect::Rainbow))));
        assert!(parse(&["effect", "breathing"]).is_err());
        assert!(parse(&["effect", "nope"]).is_err());
    }

    #[test]
    fn access_denied_matches_the_observed_code() {
        assert_eq!(STATUS_ACCESS_DENIED, -1073741790);
    }
}
