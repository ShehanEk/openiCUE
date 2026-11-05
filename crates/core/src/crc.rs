/// CRC-8, polynomial 0x07, initial value 0, as CRCPP's CRC_8 used by OpenRGB.
pub fn crc8(data: &[u8]) -> u8 {
    let mut crc = 0u8;
    for byte in data {
        crc ^= byte;
        for _ in 0..8 {
            crc = if crc & 0x80 != 0 { (crc << 1) ^ 0x07 } else { crc << 1 };
        }
    }
    crc
}

#[cfg(test)]
mod tests {
    use super::crc8;

    #[test]
    fn matches_the_standard_check_value() {
        assert_eq!(crc8(b"123456789"), 0xF4);
    }
}
