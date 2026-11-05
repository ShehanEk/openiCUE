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
    pub addr: StickAddr,
    pub product_id: u16,
    pub protocol: u8,
}

#[derive(Debug, PartialEq, Eq)]
pub enum RamError {
    Bus(BusError),
    ChecksumMismatch,
    UnsupportedProduct(u16),
    ProtocolTooOld(u8),
    NotReady,
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
}
