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
    NotReady,
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
            matches!(a, Ok(0x1A | 0x1B | 0x1C)) && matches!(b, Ok(0x01 | 0x03 | 0x04))
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

use crate::effects::{Rgb, Settings};

const REG_RESET_BUFFER: u8 = 0x0B;
const REG_BINARY_WRITE: u8 = 0x20;
const REG_STATUS: u8 = 0x30;
const REG_DIRECT_BLOCK: u8 = 0x31;
const REG_COMMIT: u8 = 0x82;
const COMMIT_EFFECT: u8 = 1;
const COMMIT_COLORS: u8 = 2;
const STATUS_BUSY: u8 = 0x08;
const READY_TRIES: u32 = 5;
const READY_DELAY_MS: u64 = 10;

fn write_buffer(bus: &mut dyn Bus, addr: StickAddr, data: &[u8], commit: u8) -> Result<(), RamError> {
    bus.write_byte(addr, REG_RESET_BUFFER, 0)?;
    bus.write_byte(addr, REG_BINARY_START, 0)?;
    for &byte in data {
        bus.write_byte(addr, REG_BINARY_WRITE, byte)?;
    }
    if bus.read_byte(addr, REG_CHECKSUM)? != crate::crc::crc8(data) {
        return Err(RamError::ChecksumMismatch);
    }
    bus.write_byte(addr, REG_COMMIT, commit)?;
    for _ in 0..READY_TRIES {
        if bus.read_byte(addr, REG_STATUS)? & STATUS_BUSY == 0 {
            return Ok(());
        }
        std::thread::sleep(std::time::Duration::from_millis(READY_DELAY_MS));
    }
    Err(RamError::NotReady)
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
    let direction = if effect.directions().is_empty() {
        crate::effects::Direction::Left.byte()
    } else {
        settings.direction.byte()
    };
    let (Rgb(r1, g1, b1), Rgb(r2, g2, b2)) = (c1, c2);
    let mut packet = [0u8; 20];
    packet[..12].copy_from_slice(&[
        mode, settings.speed.byte(), color_mode, direction, r1, g1, b1, brightness, r2, g2, b2, brightness,
    ]);
    packet
}

pub fn apply_builtin(bus: &mut dyn Bus, stick: &StickInfo, settings: &Settings) -> Result<(), RamError> {
    let settings = settings.normalized();
    let mode = settings.effect.mode_byte().ok_or(RamError::CustomEffect)?;
    write_buffer(bus, stick.addr, &effect_packet(mode, &settings), COMMIT_EFFECT)?;
    std::thread::sleep(std::time::Duration::from_millis(EFFECT_SETTLE_MS));
    if settings.effect == crate::effects::Effect::Static {
        let Rgb(r, g, b) = settings.color1;
        let mut colors = [0u8; LED_COUNT * 4];
        for led in colors.chunks_exact_mut(4) {
            led.copy_from_slice(&[r, g, b, 255]);
        }
        write_buffer(bus, stick.addr, &colors, COMMIT_COLORS)?;
    }
    Ok(())
}

pub fn write_direct(bus: &mut dyn Bus, stick: &StickInfo, leds: &[Rgb; LED_COUNT]) -> Result<(), RamError> {
    if stick.protocol < 4 {
        return Err(RamError::ProtocolTooOld(stick.protocol));
    }
    let mut packet = [0u8; 2 + LED_COUNT * 3];
    packet[0] = LED_COUNT as u8;
    for (slot, led) in packet[1..].chunks_exact_mut(3).zip(leds) {
        slot.copy_from_slice(&[led.0, led.1, led.2]);
    }
    packet[31] = crate::crc::crc8(&packet[..31]);
    bus.write_block(stick.addr, REG_DIRECT_BLOCK, &packet)?;
    Ok(())
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
    fn detect_ignores_a_wrong_id_b() {
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
    fn bad_checksum_never_commits() {
        let mut bus = FakeBus { corrupt_checksum: true, ..Default::default() };
        let settings = Settings { effect: Effect::Rainbow, ..Settings::default() };
        assert_eq!(apply_builtin(&mut bus, &stick(4), &settings), Err(RamError::ChecksumMismatch));
        assert!(!writes(&bus).iter().any(|(reg, _)| *reg == 0x82));
    }

    #[test]
    fn stuck_controller_times_out() {
        let mut bus = FakeBus { stuck_busy: true, ..Default::default() };
        let settings = Settings { effect: Effect::Rainbow, ..Settings::default() };
        assert_eq!(apply_builtin(&mut bus, &stick(4), &settings), Err(RamError::NotReady));
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
