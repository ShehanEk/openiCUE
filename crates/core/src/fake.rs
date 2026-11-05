use crate::bus::{Bus, BusError, StickAddr};
use crate::crc::crc8;
use std::collections::HashMap;

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum Op {
    Read(u8, u8),
    Write(u8, u8, u8),
    Block(u8, u8, Vec<u8>),
}

/// Records every operation and answers like a stick.
#[derive(Default)]
pub struct FakeBus {
    pub ops: Vec<Op>,
    pub regs: HashMap<(u8, u8), u8>,
    pub info: Vec<u8>,
    pub info_pos: usize,
    pub buffer: Vec<u8>,
    pub corrupt_checksum: bool,
    pub stuck_busy: bool,
}

impl Bus for FakeBus {
    fn read_byte(&mut self, addr: StickAddr, reg: u8) -> Result<u8, BusError> {
        self.ops.push(Op::Read(addr.get(), reg));
        let value = match reg {
            0x40 => {
                let value = self.info.get(self.info_pos).copied().unwrap_or(0);
                self.info_pos += 1;
                value
            }
            0x42 => {
                let source = if self.buffer.is_empty() { &self.info } else { &self.buffer };
                crc8(source) ^ self.corrupt_checksum as u8
            }
            0x30 => if self.stuck_busy { 0x08 } else { 0x00 },
            _ => match self.regs.get(&(addr.get(), reg)) {
                Some(value) => *value,
                None => return Err(BusError::Os(-1)),
            },
        };
        Ok(value)
    }

    fn write_byte(&mut self, addr: StickAddr, reg: u8, value: u8) -> Result<(), BusError> {
        self.ops.push(Op::Write(addr.get(), reg, value));
        match reg {
            0x20 => self.buffer.push(value),
            0x0B => self.buffer.clear(),
            0x61 => self.info_pos = 0,
            _ => {}
        }
        Ok(())
    }

    fn write_block(&mut self, addr: StickAddr, reg: u8, data: &[u8]) -> Result<(), BusError> {
        self.ops.push(Op::Block(addr.get(), reg, data.to_vec()));
        Ok(())
    }
}

/// A 32-byte device info block with the given product ID and protocol.
pub fn info_block(product_id: u16, protocol: u8) -> Vec<u8> {
    let mut info = vec![0u8; 32];
    info[2] = (product_id & 0xFF) as u8;
    info[3] = (product_id >> 8) as u8;
    info[28] = protocol;
    info
}
