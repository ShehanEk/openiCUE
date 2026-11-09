use crate::bus::{Bus, BusError, StickAddr};

pub const REG_ID_A: u8 = 0x43;
pub const REG_ID_B: u8 = 0x44;
pub const REG_DEVICE_INFO: u8 = 0x61;
pub const REG_BINARY_START: u8 = 0x21;
pub const REG_BINARY_READ: u8 = 0x40;
pub const REG_CHECKSUM: u8 = 0x42;

pub const LED_COUNT: usize = 10;
const SUPPORTED_PRODUCT_IDS: [u16; 4] = [0x0100, 0x0101, 0x0300, 0x0301];

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct StickInfo {
    addr: StickAddr,
    product_id: u16,
    protocol: u8,
}

impl StickInfo {
    pub fn addr(&self) -> StickAddr {
        self.addr
    }

    pub fn product_id(&self) -> u16 {
        self.product_id
    }

    pub fn protocol(&self) -> u8 {
        self.protocol
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum RamError {
    Bus(BusError),
    ChecksumMismatch,
    UnsupportedProduct(u16),
    ProtocolTooOld(u8),
    CustomEffect,
}

impl From<BusError> for RamError {
    fn from(error: BusError) -> Self {
        RamError::Bus(error)
    }
}

pub fn detect(bus: &mut dyn Bus) -> Vec<StickAddr> {
    StickAddr::all()
        .into_iter()
        .filter(|&addr| {
            let a = bus.read_byte(addr, REG_ID_A);
            let b = bus.read_byte(addr, REG_ID_B);
            matches!(a, Ok(0x1A..=0x1C)) && matches!(b, Ok(0x01 | 0x03 | 0x04))
        })
        .collect()
}

pub fn read_info(bus: &mut dyn Bus, addr: StickAddr) -> Result<StickInfo, RamError> {
    bus.write_byte(addr, REG_DEVICE_INFO, 0)?;
    bus.write_byte(addr, REG_BINARY_START, 0)?;
    let mut data = [0u8; 32];
    for byte in &mut data {
        *byte = bus.read_byte(addr, REG_BINARY_READ)?;
    }
    if bus.read_byte(addr, REG_CHECKSUM)? != crate::crc::crc8(&data) {
        return Err(RamError::ChecksumMismatch);
    }
    let product_id = u16::from_le_bytes([data[2], data[3]]);
    if !SUPPORTED_PRODUCT_IDS.contains(&product_id) {
        return Err(RamError::UnsupportedProduct(product_id));
    }
    Ok(StickInfo { addr, product_id, protocol: data[28] })
}

/// Every detected stick whose device info reads back correctly, in address order.
pub fn sticks(bus: &mut dyn Bus) -> Vec<StickInfo> {
    detect(bus).into_iter().filter_map(|addr| read_info(bus, addr).ok()).collect()
}

use crate::effects::{Rgb, Settings};

const REG_RESET_BUFFER: u8 = 0x0B;
const REG_BINARY_WRITE: u8 = 0x20;
const REG_STATUS: u8 = 0x30;
const REG_DIRECT_BLOCK: u8 = 0x31;
const REG_COMMIT: u8 = 0x82;
const COMMIT_EFFECT: u8 = 1;
const COMMIT_COLORS: u8 = 2;
const STATUS_BUSY: u8 = 0x08;
/// OpenRGB polls 5 times. These sticks stay silent longer than 50 ms after a
/// commit (hardware stage 2, 2026-11-06), so we allow up to 500 ms.
const READY_TRIES: u32 = 50;
const READY_DELAY_MS: u64 = 10;

/// Loads `data` into the stick's buffer and checks the stick's checksum of it.
/// Nothing takes effect until a commit.
fn stage(bus: &mut dyn Bus, addr: StickAddr, data: &[u8]) -> Result<(), RamError> {
    bus.write_byte(addr, REG_RESET_BUFFER, 0)?;
    bus.write_byte(addr, REG_BINARY_START, 0)?;
    for &byte in data {
        bus.write_byte(addr, REG_BINARY_WRITE, byte)?;
    }
    if bus.read_byte(addr, REG_CHECKSUM)? != crate::crc::crc8(data) {
        return Err(RamError::ChecksumMismatch);
    }
    Ok(())
}

/// As in OpenRGB's WaitReady: poll a bounded number of times, count a failed
/// read as busy, and carry on either way, since OpenRGB ignores the result.
/// If the stick is still silent, the next write fails and stops the
/// sequence. The checksum check in `stage` is what guards each commit.
fn wait_ready(bus: &mut dyn Bus, addr: StickAddr) {
    for _ in 0..READY_TRIES {
        if matches!(bus.read_byte(addr, REG_STATUS), Ok(status) if status & STATUS_BUSY == 0) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(READY_DELAY_MS));
    }
}

fn write_buffer(bus: &mut dyn Bus, addr: StickAddr, data: &[u8], commit: u8) -> Result<(), RamError> {
    stage(bus, addr, data)?;
    bus.write_byte(addr, REG_COMMIT, commit)?;
    wait_ready(bus, addr);
    Ok(())
}

const EFFECT_SETTLE_MS: u64 = 15;

/// Builds the 20 bytes OpenRGB's `DeviceUpdateMode` sends, including its mode defaults
/// (`direction = 0` is LEFT, `brightness = 0`, colors zeroed unless mode-specific).
fn effect_packet(mode: u8, settings: &Settings) -> [u8; 20] {
    let effect = settings.effect;
    // OpenRGB's MODE_COLORS_MODE_SPECIFIC modes are the ones with colors, except Static ("Custom").
    let specific = effect.color_count() > 0 && effect != crate::effects::Effect::Static;
    let off = Rgb(0, 0, 0);
    let (c1, c2) = match (specific && !settings.random, effect.color_count()) {
        (true, 2) => (settings.color1, settings.color2),
        (true, _) => (settings.color1, off),
        (false, _) => (off, off),
    };
    let brightness = if specific { 255 } else { 0 };
    let color_mode = if settings.random { 0 } else { 1 };
    // OpenRGB's "Custom" (static) mode never sets speed, so it sends the default 0.
    let speed = if effect.has_speed() { settings.speed.byte() } else { 0 };
    let direction = if effect.directions().is_empty() {
        crate::effects::Direction::Left.byte()
    } else {
        settings.direction.byte()
    };
    let (Rgb(r1, g1, b1), Rgb(r2, g2, b2)) = (c1, c2);
    let mut packet = [0u8; 20];
    packet[..12].copy_from_slice(&[
        mode, speed, color_mode, direction, r1, g1, b1, brightness, r2, g2, b2, brightness,
    ]);
    packet
}

pub fn apply_builtin(bus: &mut dyn Bus, stick: &StickInfo, settings: &Settings) -> Result<(), RamError> {
    apply_builtin_all(bus, std::slice::from_ref(stick), settings).remove(0)
}

/// Static's colour, dimmed by the brightness setting. At 100 % it is `color1` as is.
fn static_colors(settings: &Settings) -> [u8; LED_COUNT * 4] {
    let Rgb(r, g, b) = settings.color1.dimmed(settings.brightness);
    let mut colors = [0u8; LED_COUNT * 4];
    for led in colors.as_chunks_mut::<4>().0 {
        *led = [r, g, b, 255];
    }
    colors
}

/// Applies a built-in effect to every stick so their animations start
/// together: each stick's packet is staged and checksum-checked first, then
/// the commits go out back to back. Committing one stick at a time left each
/// about 130 ms behind the last (user report, 2026-11-08), because a stick
/// is silent for about 110 ms after a commit. Returns one result per stick.
pub fn apply_builtin_all(bus: &mut dyn Bus, sticks: &[StickInfo], settings: &Settings) -> Vec<Result<(), RamError>> {
    let settings = settings.normalized();
    let Some(mode) = settings.effect.mode_byte() else {
        return sticks.iter().map(|_| Err(RamError::CustomEffect)).collect();
    };
    let packet = effect_packet(mode, &settings);
    let mut results: Vec<Result<(), RamError>> = sticks.iter().map(|s| stage(bus, s.addr, &packet)).collect();
    for (stick, result) in sticks.iter().zip(results.iter_mut()) {
        if result.is_ok() {
            *result = bus.write_byte(stick.addr, REG_COMMIT, COMMIT_EFFECT).map_err(RamError::from);
        }
    }
    for (stick, result) in sticks.iter().zip(&results) {
        if result.is_ok() {
            wait_ready(bus, stick.addr);
        }
    }
    std::thread::sleep(std::time::Duration::from_millis(EFFECT_SETTLE_MS));
    if settings.effect == crate::effects::Effect::Static {
        let colors = static_colors(&settings);
        for (stick, result) in sticks.iter().zip(results.iter_mut()) {
            if result.is_ok() {
                *result = write_buffer(bus, stick.addr, &colors, COMMIT_COLORS);
            }
        }
    }
    results
}

pub fn write_direct(bus: &mut dyn Bus, stick: &StickInfo, leds: &[Rgb; LED_COUNT]) -> Result<(), RamError> {
    if stick.protocol < 4 {
        return Err(RamError::ProtocolTooOld(stick.protocol));
    }
    let mut packet = [0u8; 2 + LED_COUNT * 3];
    packet[0] = LED_COUNT as u8;
    for (slot, led) in packet[1..].as_chunks_mut::<3>().0.iter_mut().zip(leds) {
        *slot = [led.0, led.1, led.2];
    }
    packet[31] = crate::crc::crc8(&packet[..31]);
    bus.write_block(stick.addr, REG_DIRECT_BLOCK, &packet)?;
    Ok(())
}

/// Lights `sticks[lit]` white and every other stick black, so the user can
/// see which slot it is in. Direct writes only: no commit, nothing saved.
/// Tries every stick and returns the first error.
pub fn light_only(bus: &mut dyn Bus, sticks: &[StickInfo], lit: usize) -> Result<(), RamError> {
    let mut result = Ok(());
    for (index, stick) in sticks.iter().enumerate() {
        let color = if index == lit { Rgb(255, 255, 255) } else { Rgb(0, 0, 0) };
        let written = write_direct(bus, stick, &[color; LED_COUNT]);
        if result.is_ok() {
            result = written;
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fake::{info_block, FakeBus, Op};

    fn addr(value: u8) -> StickAddr {
        StickAddr::new(value).unwrap()
    }

    #[test]
    fn detect_only_reads_and_finds_matching_ids() {
        let mut bus = FakeBus::default();
        bus.regs.insert((0x58, REG_ID_A), 0x1C);
        bus.regs.insert((0x58, REG_ID_B), 0x03);
        bus.regs.insert((0x59, REG_ID_A), 0x1C);
        bus.regs.insert((0x59, REG_ID_B), 0x99);
        assert_eq!(detect(&mut bus), vec![addr(0x58)]);
        assert!(bus.ops.iter().all(|op| matches!(op, Op::Read(_, _))));
    }

    #[test]
    fn detect_ignores_a_wrong_id_a() {
        let mut bus = FakeBus::default();
        bus.regs.insert((0x58, REG_ID_A), 0x99);
        bus.regs.insert((0x58, REG_ID_B), 0x03);
        assert_eq!(detect(&mut bus), Vec::<StickAddr>::new());
    }

    #[test]
    fn detect_accepts_every_known_id_pair() {
        for a in [0x1A, 0x1B, 0x1C] {
            for b in [0x01, 0x03, 0x04] {
                let mut bus = FakeBus::default();
                bus.regs.insert((0x58, REG_ID_A), a);
                bus.regs.insert((0x58, REG_ID_B), b);
                assert_eq!(detect(&mut bus), vec![addr(0x58)], "id pair {a:#x}/{b:#x}");
            }
        }
    }

    #[test]
    fn read_info_parses_product_and_protocol() {
        let mut bus = FakeBus { info: info_block(0x0101, 4), ..Default::default() };
        let info = read_info(&mut bus, addr(0x58)).unwrap();
        assert_eq!(info.product_id, 0x0101);
        assert_eq!(info.protocol, 4);
        assert_eq!(bus.ops[0], Op::Write(0x58, REG_DEVICE_INFO, 0));
        assert_eq!(bus.ops[1], Op::Write(0x58, REG_BINARY_START, 0));
        assert_eq!(bus.ops.len(), 35);
        assert!(bus.ops[2..].iter().all(|op| matches!(op, Op::Read(_, _))));
    }

    #[test]
    fn read_info_exposes_values_through_getters() {
        let mut bus = FakeBus { info: info_block(0x0301, 4), ..Default::default() };
        let info = read_info(&mut bus, addr(0x59)).unwrap();
        assert_eq!(info.addr(), addr(0x59));
        assert_eq!(info.product_id(), 0x0301);
        assert_eq!(info.protocol(), 4);
    }

    #[test]
    fn read_info_rejects_a_bad_checksum() {
        let mut bus = FakeBus {
            info: info_block(0x0101, 4),
            corrupt_checksum: true,
            ..Default::default()
        };
        assert_eq!(read_info(&mut bus, addr(0x58)), Err(RamError::ChecksumMismatch));
    }

    #[test]
    fn sticks_keeps_only_detected_sticks_whose_info_reads() {
        let mut bus = FakeBus { info: info_block(0x0101, 4), ..Default::default() };
        for a in [0x58, 0x59] {
            bus.regs.insert((a, REG_ID_A), 0x1C);
            bus.regs.insert((a, REG_ID_B), 0x03);
        }
        let found = sticks(&mut bus);
        assert_eq!(found.iter().map(|s| s.addr()).collect::<Vec<_>>(), vec![addr(0x58), addr(0x59)]);
        assert!(found.iter().all(|s| s.product_id() == 0x0101 && s.protocol() == 4));

        let mut bad = FakeBus { info: info_block(0x0101, 4), corrupt_checksum: true, ..Default::default() };
        bad.regs.insert((0x58, REG_ID_A), 0x1C);
        bad.regs.insert((0x58, REG_ID_B), 0x03);
        assert!(sticks(&mut bad).is_empty(), "a stick whose info fails is left out");
        assert!(sticks(&mut FakeBus::default()).is_empty());
    }

    #[test]
    fn read_info_rejects_other_products() {
        let mut bus = FakeBus { info: info_block(0x0700, 4), ..Default::default() };
        assert_eq!(read_info(&mut bus, addr(0x58)), Err(RamError::UnsupportedProduct(0x0700)));
    }

    use crate::effects::{Effect, Rgb, Settings};

    fn stick(protocol: u8) -> StickInfo {
        StickInfo { addr: addr(0x58), product_id: 0x0101, protocol }
    }

    fn writes(bus: &FakeBus) -> Vec<(u8, u8)> {
        bus.ops.iter().filter_map(|op| match op {
            Op::Write(_, reg, value) => Some((*reg, *value)),
            _ => None,
        }).collect()
    }

    #[test]
    fn effect_write_sends_the_exact_sequence() {
        let mut bus = FakeBus::default();
        let settings = Settings {
            effect: Effect::ColorPulse, color1: Rgb(1, 2, 3), color2: Rgb(4, 5, 6),
            ..Settings::default()
        };
        apply_builtin(&mut bus, &stick(4), &settings).unwrap();
        let mut expected = vec![(0x0B, 0), (0x21, 0)];
        for byte in [0x01, 0x00, 0x01, 0x02, 1, 2, 3, 255, 4, 5, 6, 255, 0, 0, 0, 0, 0, 0, 0, 0] {
            expected.push((0x20, byte));
        }
        expected.push((0x82, 1));
        assert_eq!(writes(&bus), expected);
    }

    #[test]
    fn all_sticks_are_staged_first_then_committed_back_to_back() {
        let mut bus = FakeBus::default();
        let sticks = [stick(4), StickInfo { addr: addr(0x59), product_id: 0x0101, protocol: 4 }];
        let settings = Settings { effect: Effect::ColorPulse, ..Settings::default() };
        let results = apply_builtin_all(&mut bus, &sticks, &settings);
        assert_eq!(results, vec![Ok(()), Ok(())]);

        // Each stick gets exactly the same bytes as a single apply would send.
        let mut single = FakeBus::default();
        apply_builtin(&mut single, &stick(4), &settings).unwrap();
        let one: Vec<(u8, u8)> = writes(&single);
        for a in [0x58, 0x59] {
            let mine: Vec<(u8, u8)> = bus.ops.iter().filter_map(|op| match op {
                Op::Write(at, reg, value) if *at == a => Some((*reg, *value)),
                _ => None,
            }).collect();
            assert_eq!(mine, one, "stick {a:#x}");
        }

        // The two commits are adjacent: no bus traffic between them.
        let commit = |a: u8| bus.ops.iter().position(|op| *op == Op::Write(a, 0x82, 1)).unwrap();
        assert_eq!(commit(0x59), commit(0x58) + 1);
        // And both sticks were fully staged before the first commit.
        let last_stage = bus.ops.iter().rposition(|op| matches!(op, Op::Write(_, 0x20, _))).unwrap();
        assert!(last_stage < commit(0x58));
    }

    #[test]
    fn custom_effects_are_never_sent_to_any_stick() {
        let mut bus = FakeBus::default();
        let sticks = [stick(4), StickInfo { addr: addr(0x59), product_id: 0x0101, protocol: 4 }];
        let settings = Settings { effect: Effect::Breathing, ..Settings::default() };
        let results = apply_builtin_all(&mut bus, &sticks, &settings);
        assert_eq!(results, vec![Err(RamError::CustomEffect), Err(RamError::CustomEffect)]);
        assert!(bus.ops.is_empty());
    }

    #[test]
    fn bad_checksum_never_commits() {
        let mut bus = FakeBus { corrupt_checksum: true, ..Default::default() };
        let settings = Settings { effect: Effect::Rainbow, ..Settings::default() };
        assert_eq!(apply_builtin(&mut bus, &stick(4), &settings), Err(RamError::ChecksumMismatch));
        assert!(!writes(&bus).iter().any(|(reg, _)| *reg == 0x82));
    }

    #[test]
    fn unanswered_status_reads_count_as_busy() {
        let mut bus = FakeBus { nack_status_reads: 2, ..Default::default() };
        let settings = Settings { effect: Effect::Rainbow, ..Settings::default() };
        assert_eq!(apply_builtin(&mut bus, &stick(4), &settings), Ok(()));
        let polls = bus.ops.iter().filter(|op| matches!(op, Op::Read(_, 0x30))).count();
        assert_eq!(polls, 3);
    }

    #[test]
    fn unanswered_wait_gives_up_after_every_poll_and_continues() {
        let mut bus = FakeBus { nack_status_reads: u32::MAX, ..Default::default() };
        let settings = Settings { effect: Effect::Static, color1: Rgb(9, 8, 7), ..Settings::default() };
        assert_eq!(apply_builtin(&mut bus, &stick(4), &settings), Ok(()));
        let polls = bus.ops.iter().filter(|op| matches!(op, Op::Read(_, 0x30))).count();
        assert_eq!(polls, 2 * READY_TRIES as usize, "every poll after each of the two commits");
        assert_eq!(writes(&bus).last(), Some(&(0x82, 2)));
    }

    #[test]
    fn stuck_busy_wait_is_bounded() {
        let mut bus = FakeBus { stuck_busy: true, ..Default::default() };
        let settings = Settings { effect: Effect::Rainbow, ..Settings::default() };
        assert_eq!(apply_builtin(&mut bus, &stick(4), &settings), Ok(()));
        let polls = bus.ops.iter().filter(|op| matches!(op, Op::Read(_, 0x30))).count();
        assert_eq!(polls, READY_TRIES as usize);
    }

    #[test]
    fn static_also_writes_every_led() {
        let mut bus = FakeBus::default();
        let settings = Settings { effect: Effect::Static, color1: Rgb(9, 8, 7), ..Settings::default() };
        apply_builtin(&mut bus, &stick(4), &settings).unwrap();
        let all = writes(&bus);
        assert_eq!(all.iter().filter(|(reg, _)| *reg == 0x82).count(), 2);
        assert_eq!(all.last(), Some(&(0x82, 2)));
    }

    #[test]
    fn custom_effects_are_never_sent_as_a_mode() {
        let mut bus = FakeBus::default();
        let settings = Settings { effect: Effect::Chase, ..Settings::default() };
        assert!(apply_builtin(&mut bus, &stick(4), &settings).is_err());
        assert!(bus.ops.is_empty());
    }

    #[test]
    fn direct_write_is_one_block_with_crc() {
        let mut bus = FakeBus::default();
        write_direct(&mut bus, &stick(4), &[Rgb(1, 2, 3); LED_COUNT]).unwrap();
        match &bus.ops[..] {
            [Op::Block(0x58, 0x31, data)] => {
                assert_eq!(data.len(), 32);
                assert_eq!(data[0], 10);
                assert_eq!(&data[1..4], &[1, 2, 3]);
                assert_eq!(data[31], crate::crc::crc8(&data[..31]));
            }
            other => panic!("unexpected operations: {other:?}"),
        }
    }

    /// The writes OpenRGB's buffer path makes: reset, start, data bytes, commit.
    fn buffer_writes(data: &[u8], commit: u8) -> Vec<(u8, u8)> {
        let mut out = vec![(0x0B, 0), (0x21, 0)];
        out.extend(data.iter().map(|&byte| (0x20, byte)));
        out.push((0x82, commit));
        out
    }

    fn packet(head: [u8; 12]) -> Vec<u8> {
        let mut bytes = head.to_vec();
        bytes.extend([0u8; 8]);
        bytes
    }

    /// Every built-in effect's full 20-byte packet, as OpenRGB's `DeviceUpdateMode` sends it.
    /// Rules (RGBController_CorsairDRAM.cpp, with `mode()` defaults from RGBControllerInterface.h):
    /// - byte 2: `random = color_mode == MODE_COLORS_RANDOM` -> RANDOM_COLORS 0 / CUSTOM_COLORS 1.
    /// - byte 3: modes without a direction keep `direction = 0` (MODE_DIRECTION_LEFT) -> 0x02.
    /// - bytes 4-6, 8-10: zeroed, filled only for MODE_COLORS_MODE_SPECIFIC; color 2 only if
    ///   `colors.size() == 2`. So zero for RainbowWave, Rainbow, Static and random.
    /// - bytes 7, 11: `brightness`, BRIGHTNESS_DEFAULT 255 for modes with MODE_FLAG_HAS_BRIGHTNESS,
    ///   else the default 0 (RainbowWave, Rainbow, Static).
    /// - byte 1: Static ("Custom") never sets speed, so 0.
    #[test]
    fn every_effect_packet_matches_openrgb() {
        use crate::effects::{Direction as D, Speed};
        let z = 0u8;
        // Static has a second write, so it is pinned in its own test below.
        let cases: [(Effect, bool, D, [u8; 12]); 15] = [
            (Effect::ColorShift, false, D::Down, [0x00, 2, 1, 2, 1, 2, 3, 255, 4, 5, 6, 255]),
            (Effect::ColorShift, true, D::Down, [0x00, 2, 0, 2, z, z, z, 255, z, z, z, 255]),
            (Effect::ColorPulse, false, D::Down, [0x01, 2, 1, 2, 1, 2, 3, 255, 4, 5, 6, 255]),
            (Effect::ColorPulse, true, D::Down, [0x01, 2, 0, 2, z, z, z, 255, z, z, z, 255]),
            (Effect::RainbowWave, false, D::Right, [0x03, 2, 1, 3, z, z, z, 0, z, z, z, 0]),
            (Effect::ColorWave, false, D::Up, [0x04, 2, 1, 0, 1, 2, 3, 255, 4, 5, 6, 255]),
            (Effect::ColorWave, true, D::Left, [0x04, 2, 0, 2, z, z, z, 255, z, z, z, 255]),
            (Effect::Visor, false, D::Horizontal, [0x05, 2, 1, 3, 1, 2, 3, 255, 4, 5, 6, 255]),
            (Effect::Visor, true, D::Vertical, [0x05, 2, 0, 1, z, z, z, 255, z, z, z, 255]),
            (Effect::Rain, false, D::Down, [0x06, 2, 1, 1, 1, 2, 3, 255, 4, 5, 6, 255]),
            (Effect::Rain, true, D::Up, [0x06, 2, 0, 0, z, z, z, 255, z, z, z, 255]),
            (Effect::Marquee, false, D::Down, [0x07, 2, 1, 2, 1, 2, 3, 255, z, z, z, 255]),
            (Effect::Rainbow, false, D::Down, [0x08, 2, 1, 2, z, z, z, 0, z, z, z, 0]),
            (Effect::Sequential, false, D::Up, [0x09, 2, 1, 0, 1, 2, 3, 255, z, z, z, 255]),
            (Effect::Sequential, true, D::Down, [0x09, 2, 0, 1, z, z, z, 255, z, z, z, 255]),
        ];
        for (effect, random, direction, head) in cases {
            let mut bus = FakeBus::default();
            let settings = Settings {
                effect, random, direction, speed: Speed::Fast,
                color1: Rgb(1, 2, 3), color2: Rgb(4, 5, 6), synced: false,
                ..Settings::default()
            };
            apply_builtin(&mut bus, &stick(4), &settings).unwrap();
            assert_eq!(writes(&bus), buffer_writes(&packet(head), 1), "{effect:?} random={random}");
        }
    }

    #[test]
    fn random_is_dropped_where_openrgb_has_no_random_flag() {
        for effect in [Effect::Static, Effect::RainbowWave, Effect::Marquee, Effect::Rainbow] {
            let packet_for = |random| {
                let mut bus = FakeBus::default();
                let settings = Settings { effect, random, ..Settings::default() };
                apply_builtin(&mut bus, &stick(4), &settings).unwrap();
                writes(&bus)
            };
            assert_eq!(packet_for(true), packet_for(false), "{effect:?}");
        }
    }

    /// Static: effect packet (OpenRGB "Custom": speed, brightness and colors default to 0),
    /// then `SetColorsPerLED`: `r, g, b, 0xFF` per LED, committed with ID_COLOR_DATA (2).
    #[test]
    fn static_sends_the_exact_effect_then_colors_sequence() {
        let mut bus = FakeBus::default();
        let settings = Settings {
            effect: Effect::Static, color1: Rgb(9, 8, 7), color2: Rgb(4, 5, 6),
            speed: crate::effects::Speed::Fast, ..Settings::default()
        };
        apply_builtin(&mut bus, &stick(4), &settings).unwrap();
        let mut expected = buffer_writes(&packet([0x10, 0, 1, 2, 0, 0, 0, 0, 0, 0, 0, 0]), 1);
        let colors: Vec<u8> = [9u8, 8, 7, 0xFF].repeat(10);
        assert_eq!(colors.len(), 40);
        expected.extend(buffer_writes(&colors, 2));
        assert_eq!(writes(&bus), expected);
    }

    /// Brightness dims only Static's colour bytes; its effect packet is unchanged.
    #[test]
    fn static_at_half_brightness_halves_only_the_colors() {
        let mut bus = FakeBus::default();
        let settings = Settings {
            effect: Effect::Static, color1: Rgb(200, 100, 51), brightness: 50, ..Settings::default()
        };
        apply_builtin(&mut bus, &stick(4), &settings).unwrap();
        let mut expected = buffer_writes(&packet([0x10, 0, 1, 2, 0, 0, 0, 0, 0, 0, 0, 0]), 1);
        expected.extend(buffer_writes(&[100u8, 50, 26, 0xFF].repeat(10), 2));
        assert_eq!(writes(&bus), expected);
    }

    /// The extra settings never reach the RAM's own effects.
    #[test]
    fn extra_settings_leave_built_in_packets_alone() {
        for effect in [Effect::ColorShift, Effect::ColorWave, Effect::Marquee, Effect::Rainbow] {
            let packet_for = |settings: Settings| {
                let mut bus = FakeBus::default();
                apply_builtin(&mut bus, &stick(4), &settings).unwrap();
                writes(&bus)
            };
            let plain = Settings { effect, ..Settings::default() };
            let extra = Settings {
                more_colors: [Rgb(1, 2, 3), Rgb(4, 5, 6)], color_count: 4, speed_pct: 300,
                brightness: 10, reverse: true, ..plain
            };
            assert_eq!(packet_for(extra), packet_for(plain), "{effect:?}");
        }
    }

    /// Direct block: count 10, RGB per LED in order (Vengeance RGB Pro: `reverse` false),
    /// CRC-8 of bytes 0-30 last, one block write to COLOR_BUFFER_BLOCK_1 (0x31).
    #[test]
    fn direct_block_has_every_byte_in_order() {
        let mut leds = [Rgb(0, 0, 0); LED_COUNT];
        for (i, led) in leds.iter_mut().enumerate() {
            let base = (i * 3) as u8;
            *led = Rgb(base + 1, base + 2, base + 3);
        }
        let mut bus = FakeBus::default();
        write_direct(&mut bus, &stick(4), &leds).unwrap();
        let mut expected = vec![10u8];
        expected.extend(1..=30u8);
        expected.push(DIRECT_CRC);
        assert_eq!(bus.ops, vec![Op::Block(0x58, 0x31, expected)]);
    }

    /// CRC-8 (poly 0x07, init 0) of bytes 0-30, computed independently outside this crate.
    const DIRECT_CRC: u8 = 0x7C;

    /// The stick-order picker: one stick white, every other one black, with
    /// direct writes only, so nothing is saved to the sticks' memory.
    #[test]
    fn light_only_whitens_one_stick_and_blacks_out_the_rest() {
        let sticks: Vec<StickInfo> = [0x58, 0x59, 0x5A]
            .iter()
            .map(|&a| StickInfo { addr: addr(a), product_id: 0x0101, protocol: 4 })
            .collect();
        let mut bus = FakeBus::default();
        light_only(&mut bus, &sticks, 1).unwrap();
        let block = |value: u8| {
            let mut data = vec![10u8];
            data.extend([value; 30]);
            data.push(crate::crc::crc8(&data));
            data
        };
        assert_eq!(bus.ops, vec![
            Op::Block(0x58, 0x31, block(0)),
            Op::Block(0x59, 0x31, block(255)),
            Op::Block(0x5A, 0x31, block(0)),
        ]);
    }

    #[test]
    fn light_only_tries_every_stick_and_reports_a_failure() {
        let sticks = [stick(3), StickInfo { addr: addr(0x59), product_id: 0x0101, protocol: 4 }];
        let mut bus = FakeBus::default();
        assert_eq!(light_only(&mut bus, &sticks, 1), Err(RamError::ProtocolTooOld(3)));
        assert_eq!(bus.ops.len(), 1, "the second stick is still written");
    }

    #[test]
    fn direct_write_needs_protocol_4() {
        let mut bus = FakeBus::default();
        assert_eq!(
            write_direct(&mut bus, &stick(3), &[Rgb(0, 0, 0); LED_COUNT]),
            Err(RamError::ProtocolTooOld(3))
        );
        assert!(bus.ops.is_empty());
    }
}
