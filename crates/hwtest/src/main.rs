use openicue_core::{bus::Bus, pawnio::PawnIoBus, ram};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let mut bus = PawnIoBus::open().expect("could not open PawnIO. Is this an admin terminal?");
    match args.as_slice() {
        ["detect"] => detect(&mut bus),
        _ => eprintln!("usage: openicue-hwtest detect"),
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
