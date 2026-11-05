/// A RAM lighting controller address. This is the only address type the
/// `Bus` trait accepts, so code can't reach any other chip on the SMBus.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StickAddr(u8);

impl StickAddr {
    pub fn new(value: u8) -> Option<StickAddr> {
        (0x58..=0x5F).contains(&value).then_some(StickAddr(value))
    }
    pub fn all() -> [StickAddr; 8] {
        std::array::from_fn(|i| StickAddr(0x58 + i as u8))
    }
    pub fn get(self) -> u8 {
        self.0
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum BusError {
    /// Windows or driver error code.
    Os(i32),
    /// The block is too long for one SMBus transfer.
    BlockTooLong,
}

pub trait Bus {
    fn read_byte(&mut self, addr: StickAddr, reg: u8) -> Result<u8, BusError>;
    fn write_byte(&mut self, addr: StickAddr, reg: u8, value: u8) -> Result<(), BusError>;
    fn write_block(&mut self, addr: StickAddr, reg: u8, data: &[u8]) -> Result<(), BusError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_only_controller_addresses() {
        for value in 0u8..=255 {
            let expected = (0x58..=0x5F).contains(&value);
            assert_eq!(StickAddr::new(value).is_some(), expected, "address {value:#x}");
        }
    }

    #[test]
    fn all_lists_the_eight_addresses() {
        let values: Vec<u8> = StickAddr::all().iter().map(|a| a.get()).collect();
        assert_eq!(values, vec![0x58, 0x59, 0x5A, 0x5B, 0x5C, 0x5D, 0x5E, 0x5F]);
    }
}
