# openiCUE Core and Hardware Implementation Plan (Plan 1 of 3)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build and prove the RAM control core and background engine on real hardware, with no window or installer yet.

**Architecture:** A Cargo workspace with `openicue-core` (protocol, effects, settings, driver access), `openicue-engine` (shipped background process) and `openicue-hwtest` (developer-only test tool, never shipped). Hardware access goes through a `Bus` trait whose only address type is `StickAddr`, so tests use a fake bus and real code can't address other chips.

**Tech Stack:** Rust stable (1.99, `x86_64-pc-windows-msvc`), `windows-sys` 0.59, PawnIO 2.2.0, PawnIO modules 0.2.11.

**Spec:** `docs/superpowers/specs/2026-11-05-openicue-design.md`

**Roadmap:** Plan 2 builds the window to match the design. Plan 3 builds the custom installer. Both are written after this plan's hardware findings, because those findings decide their details (frame rate, driver memory label, what survives power-off).

## Global Constraints

- Allowed addresses: `0x58`–`0x5F` only, enforced by the `StickAddr` type. No general write function.
- Only product IDs `0x0100`, `0x0101`, `0x0300`, `0x0301` (10 LEDs) may be written to.
- Commit an effect or color buffer only after its CRC-8 matches.
- Driver access never changes the SMBus port. If the current port isn't 0, refuse.
- Hold `Global\Access_SMBUS.HTP.Method` around every bus transfer.
- Admin processes: no network code, no plugins, no command-line options beyond those listed.
- Settings: `C:\ProgramData\openiCUE\settings.txt`, Administrators and System only, strict parsing, 1024-byte limit, defaults on any error.
- Dependencies: `windows-sys` only. Commit `Cargo.lock`.
- No `Co-Authored-By` or AI attribution in commits.
- Hardware stages are run by the user in an admin terminal. Stop at anything unexpected. Don't skip ahead.
- Run `cargo` from PowerShell, because Git Bash puts a different `link.exe` first on its path.
- Known gap for Plan 3: the installer must create `C:\ProgramData\openiCUE` itself and remove any existing folder first, because files a non-admin placed there earlier could keep their own permissions.

---

### Task 1: Workspace and the address safety type

**Files:**
- Create: `.gitignore`, `Cargo.toml`, `.cargo/config.toml`, `crates/core/Cargo.toml`, `crates/core/src/lib.rs`, `crates/core/src/bus.rs`

**Interfaces:**
- Produces: `StickAddr::new(u8) -> Option<StickAddr>`, `StickAddr::all() -> [StickAddr; 8]`, `StickAddr::get(self) -> u8`, `trait Bus`, `enum BusError`

- [ ] **Step 1: Initialize the repository on `main`**

```bash
git init -b main
```

- [ ] **Step 2: Create the workspace files**

`.gitignore`:
```gitignore
/target
```

`Cargo.toml`:
```toml
[workspace]
resolver = "2"
members = ["crates/core"]

[profile.release]
opt-level = "z"
lto = true
codegen-units = 1
panic = "abort"
strip = true
```

`.cargo/config.toml` (static C runtime, so no runtime DLL is loaded from the exe's folder):
```toml
[target.x86_64-pc-windows-msvc]
rustflags = ["-C", "target-feature=+crt-static"]
```

`crates/core/Cargo.toml`:
```toml
[package]
name = "openicue-core"
version = "0.1.0"
edition = "2021"

[dependencies]
```

`crates/core/src/lib.rs`:
```rust
pub mod bus;
```

- [ ] **Step 3: Write the failing test and the type in `crates/core/src/bus.rs`**

```rust
/// A RAM lighting controller address. This is the only address type the
/// `Bus` trait accepts, so code can't reach any other chip on the SMBus.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StickAddr(u8);

impl StickAddr {
    pub fn new(_value: u8) -> Option<StickAddr> {
        None
    }
    pub fn all() -> [StickAddr; 8] {
        [StickAddr(0); 8]
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
```

- [ ] **Step 4: Run the test to verify it fails**

Run: `cargo test -p openicue-core`
Expected: FAIL in `accepts_only_controller_addresses`.

- [ ] **Step 5: Implement `new` and `all`**

```rust
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
```

- [ ] **Step 6: Run the test to verify it passes**

Run: `cargo test -p openicue-core`
Expected: PASS, 2 tests.

- [ ] **Step 7: Commit**

```bash
git add .gitignore Cargo.toml Cargo.lock .cargo crates docs
git commit -m "feat: add workspace and address safety type"
```

---

### Task 2: Detection and device info

**Files:**
- Create: `crates/core/src/crc.rs`, `crates/core/src/fake.rs`, `crates/core/src/ram.rs`
- Modify: `crates/core/src/lib.rs`

**Interfaces:**
- Consumes: `Bus`, `StickAddr`, `BusError`
- Produces: `crc8(&[u8]) -> u8`, `detect(&mut dyn Bus) -> Vec<StickAddr>`, `read_info(&mut dyn Bus, StickAddr) -> Result<StickInfo, RamError>`, `StickInfo { addr, product_id, protocol }`, `RamError`, register constants

- [ ] **Step 1: Add the CRC (`crates/core/src/crc.rs`) with its test**

```rust
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
```

- [ ] **Step 2: Add the fake bus (`crates/core/src/fake.rs`)**

```rust
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
```

Add to `crates/core/src/lib.rs`:
```rust
pub mod bus;
pub mod crc;
pub mod ram;

#[cfg(test)]
pub mod fake;
```

- [ ] **Step 3: Write the failing tests in `crates/core/src/ram.rs`**

```rust
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

pub fn detect(_bus: &mut dyn Bus) -> Vec<StickAddr> {
    Vec::new()
}

pub fn read_info(_bus: &mut dyn Bus, _addr: StickAddr) -> Result<StickInfo, RamError> {
    Err(RamError::NotReady)
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
    fn read_info_parses_product_and_protocol() {
        let mut bus = FakeBus { info: info_block(0x0101, 4), ..Default::default() };
        let info = read_info(&mut bus, addr(0x58)).unwrap();
        assert_eq!(info.product_id, 0x0101);
        assert_eq!(info.protocol, 4);
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
```

- [ ] **Step 4: Run the tests to verify they fail**

Run: `cargo test -p openicue-core ram`
Expected: FAIL on all four tests.

- [ ] **Step 5: Implement `detect` and `read_info`**

```rust
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
```

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cargo test -p openicue-core`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add crates
git commit -m "feat: add stick detection and device info"
```

---

### Task 3: Effects and settings

**Files:**
- Create: `crates/core/src/effects.rs`
- Modify: `crates/core/src/lib.rs` (add `pub mod effects;`)

**Interfaces:**
- Produces: `Rgb`, `Effect`, `Speed`, `Direction`, `Settings`, `Settings::normalized(self) -> Settings`, `Effect::{name, from_name, mode_byte, is_custom, color_count, allows_random, has_speed, directions}`, `Speed::{name, from_name, byte}`, `Direction::{name, from_name, byte}`, `Settings::default()`

- [ ] **Step 1: Write `crates/core/src/effects.rs` with tests**

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgb(pub u8, pub u8, pub u8);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effect {
    Static, ColorShift, ColorPulse, RainbowWave, ColorWave, Visor, Rain, Marquee, Rainbow, Sequential,
    Breathing, Chase, Gradient,
}

pub const ALL_EFFECTS: [Effect; 13] = [
    Effect::Static, Effect::ColorShift, Effect::ColorPulse, Effect::RainbowWave, Effect::ColorWave,
    Effect::Visor, Effect::Rain, Effect::Marquee, Effect::Rainbow, Effect::Sequential,
    Effect::Breathing, Effect::Chase, Effect::Gradient,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Speed { Slow, Medium, Fast }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction { Up, Down, Left, Right, Vertical, Horizontal }

impl Effect {
    pub fn name(self) -> &'static str {
        match self {
            Effect::Static => "static", Effect::ColorShift => "color-shift",
            Effect::ColorPulse => "color-pulse", Effect::RainbowWave => "rainbow-wave",
            Effect::ColorWave => "color-wave", Effect::Visor => "visor", Effect::Rain => "rain",
            Effect::Marquee => "marquee", Effect::Rainbow => "rainbow",
            Effect::Sequential => "sequential", Effect::Breathing => "breathing",
            Effect::Chase => "chase", Effect::Gradient => "gradient",
        }
    }
    pub fn from_name(name: &str) -> Option<Effect> {
        ALL_EFFECTS.into_iter().find(|e| e.name() == name)
    }
    /// The byte sent to the RAM. `None` for custom effects.
    pub fn mode_byte(self) -> Option<u8> {
        match self {
            Effect::ColorShift => Some(0x00), Effect::ColorPulse => Some(0x01),
            Effect::RainbowWave => Some(0x03), Effect::ColorWave => Some(0x04),
            Effect::Visor => Some(0x05), Effect::Rain => Some(0x06),
            Effect::Marquee => Some(0x07), Effect::Rainbow => Some(0x08),
            Effect::Sequential => Some(0x09), Effect::Static => Some(0x10),
            Effect::Breathing | Effect::Chase | Effect::Gradient => None,
        }
    }
    pub fn is_custom(self) -> bool {
        self.mode_byte().is_none()
    }
    pub fn color_count(self) -> u8 {
        match self {
            Effect::RainbowWave | Effect::Rainbow => 0,
            Effect::Static | Effect::Marquee | Effect::Sequential => 1,
            _ => 2,
        }
    }
    pub fn allows_random(self) -> bool {
        matches!(self, Effect::ColorShift | Effect::ColorPulse | Effect::ColorWave
            | Effect::Visor | Effect::Rain | Effect::Sequential)
    }
    pub fn has_speed(self) -> bool {
        self != Effect::Static
    }
    pub fn directions(self) -> &'static [Direction] {
        match self {
            Effect::RainbowWave | Effect::ColorWave =>
                &[Direction::Down, Direction::Up, Direction::Left, Direction::Right],
            Effect::Visor => &[Direction::Vertical, Direction::Horizontal],
            Effect::Rain | Effect::Sequential => &[Direction::Down, Direction::Up],
            _ => &[],
        }
    }
}

impl Speed {
    pub fn name(self) -> &'static str {
        match self { Speed::Slow => "slow", Speed::Medium => "medium", Speed::Fast => "fast" }
    }
    pub fn from_name(name: &str) -> Option<Speed> {
        [Speed::Slow, Speed::Medium, Speed::Fast].into_iter().find(|s| s.name() == name)
    }
    pub fn byte(self) -> u8 {
        match self { Speed::Slow => 0, Speed::Medium => 1, Speed::Fast => 2 }
    }
}

impl Direction {
    pub fn name(self) -> &'static str {
        match self {
            Direction::Up => "up", Direction::Down => "down", Direction::Left => "left",
            Direction::Right => "right", Direction::Vertical => "vertical",
            Direction::Horizontal => "horizontal",
        }
    }
    pub fn from_name(name: &str) -> Option<Direction> {
        [Direction::Up, Direction::Down, Direction::Left, Direction::Right,
         Direction::Vertical, Direction::Horizontal]
            .into_iter().find(|d| d.name() == name)
    }
    pub fn byte(self) -> u8 {
        match self {
            Direction::Up => 0, Direction::Down | Direction::Vertical => 1,
            Direction::Left => 2, Direction::Right | Direction::Horizontal => 3,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Settings {
    pub effect: Effect,
    pub color1: Rgb,
    pub color2: Rgb,
    pub random: bool,
    pub speed: Speed,
    pub direction: Direction,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            effect: Effect::RainbowWave,
            color1: Rgb(0x2F, 0x9B, 0xFF),
            color2: Rgb(0xFF, 0x7A, 0x3D),
            random: false,
            speed: Speed::Slow,
            direction: Direction::Down,
        }
    }
}

impl Settings {
    /// Removes options the chosen effect doesn't support.
    pub fn normalized(mut self) -> Settings {
        if !self.effect.allows_random() {
            self.random = false;
        }
        let directions = self.effect.directions();
        if !directions.is_empty() && !directions.contains(&self.direction) {
            self.direction = directions[0];
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_round_trip() {
        for effect in ALL_EFFECTS {
            assert_eq!(Effect::from_name(effect.name()), Some(effect));
        }
    }

    #[test]
    fn exactly_three_effects_are_custom() {
        assert_eq!(ALL_EFFECTS.iter().filter(|e| e.is_custom()).count(), 3);
    }

    #[test]
    fn normalized_removes_unsupported_options() {
        let settings = Settings {
            effect: Effect::Visor, direction: Direction::Up, ..Settings::default()
        }.normalized();
        assert_eq!(settings.direction, Direction::Vertical);

        let settings = Settings {
            effect: Effect::Rainbow, random: true, ..Settings::default()
        }.normalized();
        assert!(!settings.random);
    }
}
```

- [ ] **Step 2: Run the tests**

Run: `cargo test -p openicue-core effects`
Expected: PASS, 3 tests.

- [ ] **Step 3: Commit**

```bash
git add crates
git commit -m "feat: add effect and settings model"
```

---

### Task 4: Writing effects and colors

**Files:**
- Modify: `crates/core/src/ram.rs`

**Interfaces:**
- Consumes: `Settings`, `Rgb`, `StickInfo`, `crc8`
- Produces: `apply_builtin(&mut dyn Bus, &StickInfo, &Settings) -> Result<(), RamError>`, `write_direct(&mut dyn Bus, &StickInfo, &[Rgb; LED_COUNT]) -> Result<(), RamError>`

- [ ] **Step 1: Add failing tests inside `ram.rs`'s `tests` module**

```rust
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
        for byte in [0x01, 0x00, 0x01, 0x00, 1, 2, 3, 255, 4, 5, 6, 255, 0, 0, 0, 0, 0, 0, 0, 0] {
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
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p openicue-core ram`
Expected: FAIL to compile, `apply_builtin` not found.

- [ ] **Step 3: Implement in `ram.rs`, above the `tests` module**

```rust
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

fn effect_packet(mode: u8, settings: &Settings) -> [u8; 20] {
    let Rgb(r1, g1, b1) = settings.color1;
    let Rgb(r2, g2, b2) = settings.color2;
    let color_mode = if settings.random { 0 } else { 1 };
    let direction = if settings.effect.directions().is_empty() { 0 } else { settings.direction.byte() };
    let mut packet = [0u8; 20];
    packet[..12].copy_from_slice(&[
        mode, settings.speed.byte(), color_mode, direction, r1, g1, b1, 255, r2, g2, b2, 255,
    ]);
    packet
}

pub fn apply_builtin(bus: &mut dyn Bus, stick: &StickInfo, settings: &Settings) -> Result<(), RamError> {
    let settings = settings.normalized();
    let mode = settings.effect.mode_byte().ok_or(RamError::CustomEffect)?;
    write_buffer(bus, stick.addr, &effect_packet(mode, &settings), COMMIT_EFFECT)?;
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
```

Add `CustomEffect,` to the `RamError` enum.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p openicue-core`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates
git commit -m "feat: add effect and direct color writes"
```

---

### Task 5: Custom effect frames

**Files:**
- Create: `crates/core/src/frames.rs`
- Modify: `crates/core/src/lib.rs` (add `pub mod frames;`)

**Interfaces:**
- Consumes: `Settings`, `Rgb`, `Effect`, `LED_COUNT`
- Produces: `STICKS_IN_FRAME = 4`, `pub type Frame = [[Rgb; LED_COUNT]; STICKS_IN_FRAME]`, `render(&Settings, seconds: f32) -> Frame`

- [ ] **Step 1: Write `crates/core/src/frames.rs` with tests**

```rust
use crate::effects::{Effect, Rgb, Settings, Speed};
use crate::ram::LED_COUNT;

pub const STICKS_IN_FRAME: usize = 4;
pub type Frame = [[Rgb; LED_COUNT]; STICKS_IN_FRAME];

fn mix(a: Rgb, b: Rgb, k: f32) -> Rgb {
    let k = k.clamp(0.0, 1.0);
    let channel = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * k).round() as u8;
    Rgb(channel(a.0, b.0), channel(a.1, b.1), channel(a.2, b.2))
}

fn dim(color: Rgb, k: f32) -> Rgb {
    mix(Rgb(0, 0, 0), color, k)
}

fn wave(x: f32) -> f32 {
    (x.sin() + 1.0) / 2.0
}

/// Colors for every LED at `seconds` since the effect started.
pub fn render(settings: &Settings, seconds: f32) -> Frame {
    let speed = match settings.speed {
        Speed::Slow => 0.6,
        Speed::Medium => 1.2,
        Speed::Fast => 2.4,
    };
    let p = seconds * speed;
    let (a, b) = (settings.color1, settings.color2);
    let mut frame = [[Rgb(0, 0, 0); LED_COUNT]; STICKS_IN_FRAME];
    for (s, stick) in frame.iter_mut().enumerate() {
        for (i, led) in stick.iter_mut().enumerate() {
            *led = match settings.effect {
                Effect::Breathing => dim(mix(a, b, wave(p * 0.4)), wave(p * 1.5) * 0.95 + 0.05),
                Effect::Chase => {
                    let m = (p * 0.8).rem_euclid(6.0);
                    let position = if m < 3.0 { m } else { 6.0 - m };
                    dim(mix(a, b, position / 3.0), 1.0 - (s as f32 - position).abs() * 1.1)
                }
                Effect::Gradient => {
                    let x = i as f32 / 9.0 * std::f32::consts::PI + s as f32 * 0.6 - p;
                    mix(a, b, wave(x))
                }
                _ => Rgb(0, 0, 0),
            };
        }
    }
    frame
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(effect: Effect) -> Settings {
        Settings {
            effect,
            color1: Rgb(255, 0, 0),
            color2: Rgb(0, 0, 255),
            ..Settings::default()
        }
    }

    #[test]
    fn breathing_keeps_all_sticks_in_sync() {
        let frame = render(&settings(Effect::Breathing), 1.3);
        assert!(frame.iter().flatten().all(|led| *led == frame[0][0]));
    }

    #[test]
    fn chase_moves_between_sticks() {
        let start = render(&settings(Effect::Chase), 0.0);
        assert_ne!(start[0][0], Rgb(0, 0, 0));
        assert_eq!(start[3][0], Rgb(0, 0, 0));
        let later = render(&settings(Effect::Chase), 5.0);
        assert_ne!(start, later);
    }

    #[test]
    fn gradient_changes_along_the_stick() {
        let frame = render(&settings(Effect::Gradient), 0.0);
        assert_ne!(frame[0][0], frame[0][5]);
    }

    #[test]
    fn built_in_effects_render_black() {
        let frame = render(&settings(Effect::Rainbow), 1.0);
        assert!(frame.iter().flatten().all(|led| *led == Rgb(0, 0, 0)));
    }
}
```

- [ ] **Step 2: Run the tests**

Run: `cargo test -p openicue-core frames`
Expected: PASS, 4 tests.

- [ ] **Step 3: Commit**

```bash
git add crates
git commit -m "feat: add custom effect frames"
```

---

### Task 6: Strict settings parser

**Files:**
- Create: `crates/core/src/config.rs`
- Modify: `crates/core/src/lib.rs` (add `pub mod config;`)

**Interfaces:**
- Consumes: `Settings` and the effect name functions
- Produces: `parse(&str) -> Option<Settings>`, `serialize(&Settings) -> String`, `MAX_SETTINGS_BYTES: usize = 1024`

- [ ] **Step 1: Write `crates/core/src/config.rs` with tests**

```rust
use crate::effects::{Direction, Effect, Rgb, Settings, Speed};

pub const MAX_SETTINGS_BYTES: usize = 1024;

fn parse_color(text: &str) -> Option<Rgb> {
    if text.len() != 6 || !text.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let value = u32::from_str_radix(text, 16).ok()?;
    Some(Rgb((value >> 16) as u8, (value >> 8) as u8, value as u8))
}

fn next_value<'a>(lines: &mut std::str::Lines<'a>, key: &str) -> Option<&'a str> {
    lines.next()?.strip_prefix(key)?.strip_prefix('=')
}

/// Returns `None` for anything other than exactly the expected file.
pub fn parse(text: &str) -> Option<Settings> {
    if text.len() > MAX_SETTINGS_BYTES {
        return None;
    }
    let mut lines = text.lines();
    if next_value(&mut lines, "version")? != "1" {
        return None;
    }
    let effect = Effect::from_name(next_value(&mut lines, "effect")?)?;
    let color1 = parse_color(next_value(&mut lines, "color1")?)?;
    let color2 = parse_color(next_value(&mut lines, "color2")?)?;
    let random = match next_value(&mut lines, "random")? {
        "0" => false,
        "1" => true,
        _ => return None,
    };
    let speed = Speed::from_name(next_value(&mut lines, "speed")?)?;
    let direction = Direction::from_name(next_value(&mut lines, "direction")?)?;
    if lines.next().is_some() {
        return None;
    }
    Some(Settings { effect, color1, color2, random, speed, direction }.normalized())
}

pub fn serialize(settings: &Settings) -> String {
    let hex = |c: Rgb| format!("{:02X}{:02X}{:02X}", c.0, c.1, c.2);
    format!(
        "version=1\neffect={}\ncolor1={}\ncolor2={}\nrandom={}\nspeed={}\ndirection={}\n",
        settings.effect.name(),
        hex(settings.color1),
        hex(settings.color2),
        settings.random as u8,
        settings.speed.name(),
        settings.direction.name(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid() -> String {
        serialize(&Settings::default())
    }

    #[test]
    fn round_trips() {
        assert_eq!(parse(&valid()), Some(Settings::default()));
    }

    #[test]
    fn rejects_bad_input() {
        let oversized = format!("{}{}", valid(), " ".repeat(MAX_SETTINGS_BYTES));
        let cases = [
            String::new(),
            valid().replace("version=1", "version=2"),
            valid().replace("rainbow-wave", "../../evil"),
            valid().replace("2F9BFF", "2F9BFG"),
            valid().replace("2F9BFF", "2F9BF"),
            valid().replace("random=0", "random=2"),
            valid().replace("speed=slow", "speed=warp"),
            format!("{}extra=1\n", valid()),
            oversized,
        ];
        for case in cases {
            assert_eq!(parse(&case), None, "accepted: {case:?}");
        }
    }

    #[test]
    fn removes_unsupported_options() {
        let text = valid().replace("rainbow-wave", "rainbow").replace("random=0", "random=1");
        assert!(!parse(&text).unwrap().random);
    }
}
```

- [ ] **Step 2: Run the tests**

Run: `cargo test -p openicue-core config`
Expected: PASS, 3 tests.

- [ ] **Step 3: Commit**

```bash
git add crates
git commit -m "feat: add strict settings parser"
```

---

### Task 7: Real driver access and read-only hardware check

**Files:**
- Create: `crates/core/modules/SmbusPIIX4.bin`, `crates/core/modules/LICENSE-PawnIO-Modules.txt`, `crates/core/src/pawnio.rs`, `crates/hwtest/Cargo.toml`, `crates/hwtest/src/main.rs`, `docs/hardware-findings.md`
- Modify: `Cargo.toml`, `crates/core/Cargo.toml`, `crates/core/src/lib.rs`

**Interfaces:**
- Consumes: `Bus`, `StickAddr`, `BusError`, `detect`
- Produces: `PawnIoBus::open() -> Result<PawnIoBus, BusError>`, which implements `Bus`

- [ ] **Step 1: Ask the user's permission, then download the signed module**

Tell the user: "Downloading `release_0_2_11.zip`, 69,582 bytes, from `github.com/namazso/PawnIO.Modules`." Wait for a yes. Then download into a new, empty scratchpad folder:

```bash
gh release download 0.2.11 -R namazso/PawnIO.Modules -p release_0_2_11.zip
```

- [ ] **Step 2: Verify the hash, and stop if it differs**

Run in that folder: `(Get-FileHash release_0_2_11.zip -Algorithm SHA256).Hash`
Expected: `43608CB89BC84247FEF1368A139013F7D043E17DB6D6C8DFC9B46BF0905A81F4`

- [ ] **Step 3: Extract only the PIIX4 module**

Run in that folder, then list the result:

```powershell
Expand-Archive release_0_2_11.zip -DestinationPath extracted
```

```powershell
Get-ChildItem extracted -Recurse -Name
```

Copy the file named `SmbusPIIX4.bin` to `crates/core/modules/SmbusPIIX4.bin`. If there's no such file, stop and report the names the zip does contain. Save the module repo's LGPL text as the license file:

```bash
gh api -H "Accept: application/vnd.github.raw" repos/namazso/PawnIO.Modules/contents/COPYING > crates/core/modules/LICENSE-PawnIO-Modules.txt
```

- [ ] **Step 4: Add the dependency to `crates/core/Cargo.toml`**

```toml
[dependencies]
windows-sys = { version = "0.59", features = [
  "Win32_Foundation",
  "Win32_Security",
  "Win32_Security_Authorization",
  "Win32_System_IO",
  "Win32_System_Threading",
] }
```

- [ ] **Step 5: Write `crates/core/src/pawnio.rs`**

```rust
//! Talks to the PawnIO driver and its signed SMBus module.
use crate::bus::{Bus, BusError, StickAddr};
use std::ffi::c_void;
use std::ptr::{null, null_mut};
use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, HANDLE, WAIT_ABANDONED, WAIT_OBJECT_0};
use windows_sys::Win32::System::Threading::{CreateMutexW, ReleaseMutex, WaitForSingleObject};
use windows_sys::Win32::System::IO::DeviceIoControl;

static MODULE: &[u8] = include_bytes!("../modules/SmbusPIIX4.bin");

// CTL_CODE(41394, function, METHOD_BUFFERED, FILE_ANY_ACCESS), from pawnio_um.h.
const IOCTL_LOAD_BINARY: u32 = (41394 << 16) | (0x821 << 2);
const IOCTL_EXECUTE: u32 = (41394 << 16) | (0x841 << 2);

const READ: u64 = 1;
const WRITE: u64 = 0;
const PROTOCOL_BYTE_DATA: u64 = 2;
const PROTOCOL_BLOCK_DATA: u64 = 5;
const LOCK_TIMEOUT_MS: u32 = 100;
const ERROR_LOCK_TIMEOUT: i32 = -2;

const GENERIC_READ_WRITE: u32 = 0x8000_0000 | 0x4000_0000;
const FILE_SHARE_ALL: u32 = 1 | 2 | 4;

#[repr(C)]
struct UnicodeString { length: u16, maximum_length: u16, buffer: *const u16 }

#[repr(C)]
struct ObjectAttributes {
    length: u32,
    root_directory: HANDLE,
    object_name: *const UnicodeString,
    attributes: u32,
    security_descriptor: *const c_void,
    security_quality_of_service: *const c_void,
}

#[repr(C)]
struct IoStatusBlock { status: usize, information: usize }

#[link(name = "ntdll")]
extern "system" {
    fn NtOpenFile(handle: *mut HANDLE, access: u32, attributes: *const ObjectAttributes,
        io_status: *mut IoStatusBlock, share: u32, options: u32) -> i32;
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

pub struct PawnIoBus {
    device: HANDLE,
    smbus_lock: HANDLE,
}

impl PawnIoBus {
    pub fn open() -> Result<PawnIoBus, BusError> {
        let path: Vec<u16> = "\\Device\\PawnIO".encode_utf16().collect();
        let name = UnicodeString {
            length: (path.len() * 2) as u16,
            maximum_length: (path.len() * 2) as u16,
            buffer: path.as_ptr(),
        };
        let attributes = ObjectAttributes {
            length: std::mem::size_of::<ObjectAttributes>() as u32,
            root_directory: null_mut(),
            object_name: &name,
            attributes: 0,
            security_descriptor: null(),
            security_quality_of_service: null(),
        };
        let mut io_status = IoStatusBlock { status: 0, information: 0 };
        let mut device: HANDLE = null_mut();
        let status = unsafe {
            NtOpenFile(&mut device, GENERIC_READ_WRITE, &attributes, &mut io_status, FILE_SHARE_ALL, 0)
        };
        if status < 0 {
            return Err(BusError::Os(status));
        }
        let lock_name = wide("Global\\Access_SMBUS.HTP.Method");
        let smbus_lock = unsafe { CreateMutexW(null(), 0, lock_name.as_ptr()) };
        if smbus_lock.is_null() {
            unsafe { CloseHandle(device) };
            return Err(BusError::Os(unsafe { GetLastError() } as i32));
        }
        let mut bus = PawnIoBus { device, smbus_lock };
        bus.load_module()?;
        if bus.current_port()? != 0 {
            return Err(BusError::UnexpectedPort);
        }
        Ok(bus)
    }

    fn load_module(&mut self) -> Result<(), BusError> {
        let mut returned = 0u32;
        let ok = unsafe {
            DeviceIoControl(self.device, IOCTL_LOAD_BINARY, MODULE.as_ptr().cast(), MODULE.len() as u32,
                null_mut(), 0, &mut returned, null_mut())
        };
        if ok == 0 {
            return Err(BusError::Os(unsafe { GetLastError() } as i32));
        }
        Ok(())
    }

    fn call(&mut self, function: &str, input: &[u64], output: &mut [u64]) -> Result<(), BusError> {
        let mut buffer = vec![0u8; 32 + input.len() * 8];
        buffer[..function.len()].copy_from_slice(function.as_bytes());
        for (slot, value) in buffer[32..].chunks_exact_mut(8).zip(input) {
            slot.copy_from_slice(&value.to_le_bytes());
        }
        let mut returned = 0u32;
        let ok = unsafe {
            DeviceIoControl(self.device, IOCTL_EXECUTE, buffer.as_ptr().cast(), buffer.len() as u32,
                output.as_mut_ptr().cast(), (output.len() * 8) as u32, &mut returned, null_mut())
        };
        if ok == 0 {
            return Err(BusError::Os(unsafe { GetLastError() } as i32));
        }
        Ok(())
    }

    /// Runs one transfer while holding the lock shared with other hardware tools.
    fn transfer(&mut self, input: &[u64], output: &mut [u64]) -> Result<(), BusError> {
        let wait = unsafe { WaitForSingleObject(self.smbus_lock, LOCK_TIMEOUT_MS) };
        if wait != WAIT_OBJECT_0 && wait != WAIT_ABANDONED {
            return Err(BusError::Os(ERROR_LOCK_TIMEOUT));
        }
        let result = self.call("ioctl_smbus_xfer", input, output);
        unsafe { ReleaseMutex(self.smbus_lock) };
        result
    }

    /// Asks for the current port without changing it (-1 means query only).
    fn current_port(&mut self) -> Result<u64, BusError> {
        let mut output = [0u64; 1];
        self.call("ioctl_piix4_port_sel", &[u64::MAX], &mut output)?;
        Ok(output[0])
    }
}

impl Drop for PawnIoBus {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.smbus_lock);
            CloseHandle(self.device);
        }
    }
}

impl Bus for PawnIoBus {
    fn read_byte(&mut self, addr: StickAddr, reg: u8) -> Result<u8, BusError> {
        let mut output = [0u64; 1];
        self.transfer(&[addr.get() as u64, READ, reg as u64, PROTOCOL_BYTE_DATA], &mut output)?;
        Ok(output[0] as u8)
    }

    fn write_byte(&mut self, addr: StickAddr, reg: u8, value: u8) -> Result<(), BusError> {
        self.transfer(&[addr.get() as u64, WRITE, reg as u64, PROTOCOL_BYTE_DATA, value as u64], &mut [])
    }

    fn write_block(&mut self, addr: StickAddr, reg: u8, data: &[u8]) -> Result<(), BusError> {
        if data.is_empty() || data.len() > 32 {
            return Err(BusError::BlockTooLong);
        }
        let mut bytes = [0u8; 40];
        bytes[0] = data.len() as u8;
        bytes[1..=data.len()].copy_from_slice(data);
        let mut input = [addr.get() as u64, WRITE, reg as u64, PROTOCOL_BLOCK_DATA, 0, 0, 0, 0, 0];
        for (slot, chunk) in input[4..].iter_mut().zip(bytes.chunks_exact(8)) {
            *slot = u64::from_le_bytes(chunk.try_into().unwrap());
        }
        self.transfer(&input, &mut [])
    }
}
```

In `crates/core/src/bus.rs`, add `UnexpectedPort,` to `BusError`. In `lib.rs`, add `pub mod pawnio;`.

- [ ] **Step 6: Add the test tool**

Add `"crates/hwtest"` to `members` in the root `Cargo.toml`.

`crates/hwtest/Cargo.toml`:
```toml
[package]
name = "openicue-hwtest"
version = "0.1.0"
edition = "2021"

[dependencies]
openicue-core = { path = "../core" }
```

`crates/hwtest/src/main.rs`:
```rust
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
```

- [ ] **Step 7: Build and run all tests**

Run: `cargo test --workspace`, then `cargo build --release -p openicue-hwtest`
Expected: tests PASS, build succeeds.

- [ ] **Step 8: Commit**

```bash
git add Cargo.toml Cargo.lock crates
git commit -m "feat: add PawnIO bus and read-only hardware check"
```

- [ ] **Step 9: HARDWARE STAGE 1, user runs it (read-only)**

Ask the user to:
1. Install PawnIO 2.2.0 from `pawnio.eu`, choosing the official signed edition. The file's SHA-256 should be `1F519A22E47187F70A1379A48CA604981C4FCF694F4E65B734AAA74A9FBA3032`.
2. Close iCUE and other RGB or hardware monitoring apps.
3. Open an admin terminal in the project folder and run:

```bash
target\release\openicue-hwtest.exe detect
```

Expected: `Found 4 sticks` with addresses in `0x58`–`0x5F`. This writes nothing to the RAM. If it finds fewer than 4, or errors, stop and diagnose. Don't widen the address range.

- [ ] **Step 10: Record the result**

Create `docs/hardware-findings.md`, with a dated entry containing the exact output, then commit:

```bash
git add docs/hardware-findings.md
git commit -m "docs: record read-only hardware check"
```

---

### Task 8: First writes (hardware stages 2 and 3)

**Files:**
- Modify: `crates/hwtest/src/main.rs`, `docs/hardware-findings.md`

**Interfaces:**
- Consumes: `read_info`, `apply_builtin`, `write_direct`, `Settings`, `Effect`, `ALL_EFFECTS`, `StickAddr::new`

- [ ] **Step 1: Add commands to `main.rs`**

Replace the `match` and add the functions:

```rust
use openicue_core::bus::StickAddr;
use openicue_core::effects::{Effect, Rgb, Settings, ALL_EFFECTS};

    match args.as_slice() {
        ["detect"] => detect(&mut bus),
        ["info"] => info(&mut bus),
        ["one", address] => one_stick(&mut bus, address),
        ["effect", name] => effect(&mut bus, name),
        _ => eprintln!("usage: openicue-hwtest detect | info | one <58-5F> | effect <name>"),
    }

fn info(bus: &mut dyn Bus) {
    for stick in ram::detect(bus) {
        match ram::read_info(bus, stick) {
            Ok(info) => println!("{:#04x}: product {:#06x}, protocol {}", stick.get(), info.product_id, info.protocol),
            Err(error) => println!("{:#04x}: {error:?}", stick.get()),
        }
    }
}

/// Writes one static red color to one detected stick.
fn one_stick(bus: &mut dyn Bus, address: &str) {
    let value = u8::from_str_radix(address, 16).expect("address must be hex, such as 58");
    let addr = StickAddr::new(value).expect("address must be 58 to 5F");
    assert!(ram::detect(bus).contains(&addr), "that address did not pass detection");
    let info = ram::read_info(bus, addr).expect("device info failed");
    let settings = Settings { effect: Effect::Static, color1: Rgb(255, 0, 0), ..Settings::default() };
    println!("{:?}", ram::apply_builtin(bus, &info, &settings));
}

/// Applies a built-in effect to every detected stick.
fn effect(bus: &mut dyn Bus, name: &str) {
    let chosen = Effect::from_name(name).unwrap_or_else(|| {
        let names: Vec<&str> = ALL_EFFECTS.iter().map(|e| e.name()).collect();
        panic!("unknown effect. Choose from {names:?}");
    });
    let settings = Settings { effect: chosen, ..Settings::default() };
    for stick in ram::detect(bus) {
        let result = ram::read_info(bus, stick).and_then(|info| ram::apply_builtin(bus, &info, &settings));
        println!("{:#04x}: {result:?}", stick.get());
    }
}
```

- [ ] **Step 2: Build**

Run: `cargo build --release -p openicue-hwtest`
Expected: success.

- [ ] **Step 3: HARDWARE STAGE 1b, user runs**

```bash
target\release\openicue-hwtest.exe info
```

Expected: 4 lines, each with a supported product ID and a protocol number. Any other result, stop. Record whether the protocol is 4 or higher, because custom effects need that. This is the first command that writes to the sticks: two register writes that request the info block, using OpenRGB's sequence. It changes no lighting.

- [ ] **Step 4: HARDWARE STAGE 2, user runs, after confirming stage 1b**

Use the first address from the `detect` output. For example:

```bash
target\release\openicue-hwtest.exe one 58
```

Expected: `Ok(())` and exactly one stick turns red. The PC stays stable. If anything else happens, stop.

- [ ] **Step 5: HARDWARE STAGE 3, user runs each effect**

```bash
target\release\openicue-hwtest.exe effect rainbow-wave
```

Repeat with `static`, `color-shift`, `color-pulse`, `color-wave`, `visor`, `rain`, `marquee`, `rainbow` and `sequential`. Expected: each shows `Ok(())` for 4 sticks and the effect visibly runs. Record any that look wrong.

- [ ] **Step 6: Record and commit**

Add the results to `docs/hardware-findings.md`, then:

```bash
git add crates docs/hardware-findings.md
git commit -m "feat: add hardware write commands and record stages 2 and 3"
```

---

### Task 9: Secure settings storage and background engine

**Files:**
- Create: `crates/core/src/system.rs`, `crates/engine/Cargo.toml`, `crates/engine/src/main.rs`
- Modify: `Cargo.toml`, `crates/core/src/lib.rs`

**Interfaces:**
- Consumes: `PawnIoBus`, `detect`, `read_info`, `apply_builtin`, `write_direct`, `render`, `parse`, `serialize`, `Settings`
- Produces: `AdminDescriptor`, `secure_data_dir() -> Result<PathBuf, u32>`, `load_settings() -> Settings`, `save_settings(&Settings) -> Result<(), u32>`, `open_stop_event() -> Result<StopEvent, u32>`, `StopEvent::{signal, wait_ms}`, `claim_engine() -> Result<Option<EngineClaim>, u32>`. Binary: `openicue-engine.exe --restore | --run`

- [ ] **Step 1: Write `crates/core/src/system.rs`**

```rust
//! Windows objects that only Administrators and System can touch.
use crate::config;
use crate::effects::Settings;
use std::path::PathBuf;
use std::ptr::{null, null_mut};
use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, LocalFree, ERROR_ALREADY_EXISTS, HANDLE, WAIT_OBJECT_0,
};
use windows_sys::Win32::Security::Authorization::{
    ConvertStringSecurityDescriptorToSecurityDescriptorW, SetNamedSecurityInfoW,
    SDDL_REVISION_1, SE_FILE_OBJECT,
};
use windows_sys::Win32::Security::{
    GetSecurityDescriptorDacl, ACL, DACL_SECURITY_INFORMATION, PROTECTED_DACL_SECURITY_INFORMATION,
    PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES,
};
use windows_sys::Win32::System::Threading::{CreateEventW, CreateMutexW, SetEvent, WaitForSingleObject};

use std::os::windows::fs::MetadataExt;

const ADMIN_ONLY: &str = "D:P(A;OICI;GA;;;SY)(A;OICI;GA;;;BA)";
const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
const ERROR_REPARSE_POINT_FOUND: u32 = 4390;

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

fn last_error() -> u32 {
    unsafe { GetLastError() }
}

pub struct AdminDescriptor(PSECURITY_DESCRIPTOR);

impl AdminDescriptor {
    pub fn new() -> Result<AdminDescriptor, u32> {
        let text = wide(ADMIN_ONLY);
        let mut descriptor: PSECURITY_DESCRIPTOR = null_mut();
        let ok = unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                text.as_ptr(), SDDL_REVISION_1, &mut descriptor, null_mut())
        };
        if ok == 0 { Err(last_error()) } else { Ok(AdminDescriptor(descriptor)) }
    }

    fn attributes(&self) -> SECURITY_ATTRIBUTES {
        SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: self.0,
            bInheritHandle: 0,
        }
    }
}

impl Drop for AdminDescriptor {
    fn drop(&mut self) {
        unsafe { LocalFree(self.0) };
    }
}

/// Creates the data folder if needed and always resets its permissions,
/// because a non-admin could have created the folder first.
pub fn secure_data_dir() -> Result<PathBuf, u32> {
    let base = std::env::var_os("ProgramData").ok_or(0u32)?;
    let dir = PathBuf::from(base).join("openiCUE");
    std::fs::create_dir_all(&dir).map_err(|e| e.raw_os_error().unwrap_or(0) as u32)?;
    // Never follow a link or junction a non-admin planted here, or the
    // permission reset below would apply to the link's target.
    let metadata = std::fs::symlink_metadata(&dir).map_err(|e| e.raw_os_error().unwrap_or(0) as u32)?;
    if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(ERROR_REPARSE_POINT_FOUND);
    }
    let descriptor = AdminDescriptor::new()?;
    let path = wide(dir.to_str().ok_or(0u32)?);
    let mut present = 0;
    let mut acl: *mut ACL = null_mut();
    let mut defaulted = 0;
    let ok = unsafe { GetSecurityDescriptorDacl(descriptor.0, &mut present, &mut acl, &mut defaulted) };
    if ok == 0 {
        return Err(last_error());
    }
    let status = unsafe {
        SetNamedSecurityInfoW(
            path.as_ptr() as *mut u16, SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
            null_mut(), null_mut(), acl, null())
    };
    if status != 0 { Err(status) } else { Ok(dir) }
}

/// Any problem gives the default settings, and nothing from a bad file is used.
pub fn load_settings() -> Settings {
    let read = || -> Option<Settings> {
        let path = secure_data_dir().ok()?.join("settings.txt");
        if std::fs::metadata(&path).ok()?.len() > config::MAX_SETTINGS_BYTES as u64 {
            return None;
        }
        config::parse(&std::fs::read_to_string(path).ok()?)
    };
    read().unwrap_or_default()
}

pub fn save_settings(settings: &Settings) -> Result<(), u32> {
    let path = secure_data_dir()?.join("settings.txt");
    std::fs::write(path, config::serialize(settings)).map_err(|e| e.raw_os_error().unwrap_or(0) as u32)
}

pub struct StopEvent(HANDLE);

pub fn open_stop_event() -> Result<StopEvent, u32> {
    let descriptor = AdminDescriptor::new()?;
    let attributes = descriptor.attributes();
    let name = wide("Global\\openiCUE.Engine.Stop");
    // Manual reset is off, so one signal stops one engine.
    let handle = unsafe { CreateEventW(&attributes, 0, 0, name.as_ptr()) };
    if handle.is_null() { Err(last_error()) } else { Ok(StopEvent(handle)) }
}

impl StopEvent {
    pub fn signal(&self) {
        unsafe { SetEvent(self.0) };
    }
    /// Waits up to `ms`. Returns true if a stop was requested.
    pub fn wait_ms(&self, ms: u32) -> bool {
        unsafe { WaitForSingleObject(self.0, ms) == WAIT_OBJECT_0 }
    }
}

impl Drop for StopEvent {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.0) };
    }
}

/// Held for the engine's lifetime, so only one engine runs.
pub struct EngineClaim(HANDLE);

pub fn claim_engine() -> Result<Option<EngineClaim>, u32> {
    let descriptor = AdminDescriptor::new()?;
    let attributes = descriptor.attributes();
    let name = wide("Global\\openiCUE.Engine");
    let handle = unsafe { CreateMutexW(&attributes, 0, name.as_ptr()) };
    if handle.is_null() {
        return Err(last_error());
    }
    if last_error() == ERROR_ALREADY_EXISTS {
        unsafe { CloseHandle(handle) };
        return Ok(None);
    }
    Ok(Some(EngineClaim(handle)))
}

impl Drop for EngineClaim {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.0) };
    }
}
```

If `windows-sys` 0.59 declares the `SetNamedSecurityInfoW` path as `*const u16`, drop the `as *mut u16` cast. Add `pub mod system;` to `lib.rs`.

- [ ] **Step 2: Write the engine**

Add `"crates/engine"` to `members` in the root `Cargo.toml`.

`crates/engine/Cargo.toml`:
```toml
[package]
name = "openicue-engine"
version = "0.1.0"
edition = "2021"

[dependencies]
openicue-core = { path = "../core" }
```

`crates/engine/src/main.rs`:
```rust
#![windows_subsystem = "windows"]
use openicue_core::bus::Bus;
use openicue_core::effects::Settings;
use openicue_core::pawnio::PawnIoBus;
use openicue_core::ram::{self, StickInfo};
use openicue_core::{frames, system};

const FRAME_MS: u32 = 33;

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
        let frame = frames::render(settings, start.elapsed().as_secs_f32());
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
```

- [ ] **Step 3: Add test commands to `crates/hwtest/src/main.rs`**

Replace `fn main` with this, because `save` and `stop` don't need the driver, and add the `save` function and import:

```rust
use openicue_core::system;

fn open_bus() -> PawnIoBus {
    PawnIoBus::open().expect("could not open PawnIO. Is this an admin terminal?")
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    match args.as_slice() {
        ["detect"] => detect(&mut open_bus()),
        ["info"] => info(&mut open_bus()),
        ["one", address] => one_stick(&mut open_bus(), address),
        ["effect", name] => effect(&mut open_bus(), name),
        ["save", name] => save(name),
        ["stop"] => system::open_stop_event().expect("event failed").signal(),
        _ => eprintln!("usage: openicue-hwtest detect | info | one <58-5F> | effect <name> | save <name> | stop"),
    }
}

fn save(name: &str) {
    let effect = Effect::from_name(name).expect("unknown effect");
    system::save_settings(&Settings { effect, ..Settings::default() }.normalized()).expect("save failed");
}
```

- [ ] **Step 4: Build and test**

Run: `cargo test --workspace`, then `cargo build --release --workspace`
Expected: PASS and success, with no compiler warnings about the temporary import.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml Cargo.lock crates
git commit -m "feat: add secure settings and background engine"
```

- [ ] **Step 6: HARDWARE STAGE 4, user runs, in an admin terminal**

Only if stage 1b showed protocol 4 or higher. Otherwise stop and discuss, because custom effects can't use direct mode.

```bash
target\release\openicue-hwtest.exe save breathing
```

```bash
target\release\openicue-engine.exe --run
```

Expected: all 4 sticks breathe in sync with no flicker. In a second admin terminal, check the cost:

```bash
Get-Process openicue-engine | Select-Object @{n='MB';e={[math]::Round($_.WorkingSet64/1MB,1)}},CPU
```

Then stop it:

```bash
target\release\openicue-hwtest.exe stop
```

Expected: engine exits. Repeat with `chase` and `gradient`. Record memory, CPU and any flicker for each. If frames flicker, try raising `FRAME_MS` before anything else.

- [ ] **Step 7: Confirm the folder permissions**

```bash
icacls C:\ProgramData\openiCUE
```

Expected: only `NT AUTHORITY\SYSTEM` and `BUILTIN\Administrators`, no inherited entries. Stop if a normal user has access.

- [ ] **Step 8: HARDWARE STAGE 5, user runs**

1. `openicue-hwtest save rainbow-wave`, then `openicue-engine --restore`. Expected: effect applies and the process exits.
2. Restart Windows. Record the LEDs before running anything.
3. Shut down fully, unplug power for 30 seconds, and start up. Record the LEDs again.
4. Run `openicue-engine --restore`. Expected: effect applies.

- [ ] **Step 9: HARDWARE STAGE 6, measure the driver**

With nothing of ours running:

```bash
Get-CimInstance Win32_SystemDriver -Filter "Name='PawnIO'" | Select-Object Name,State,StartMode
```

Record whether it stays loaded, and its non-paged memory from Task Manager or Sysinternals RAMMap if available.

- [ ] **Step 10: Record, scan and commit**

Write all results to `docs/hardware-findings.md`, including the answers to the spec's open questions on driver memory, persistence, frame rate and random colors. Then, with the user's OK to install the scanner:

```bash
cargo install cargo-audit --locked
```

```bash
cargo audit
```

Expected: no vulnerabilities.

```bash
git add docs/hardware-findings.md
git commit -m "docs: record engine and driver findings"
```

---

## After this plan

Report the findings to the user, then write Plan 2 (window, matching the design) and Plan 3 (custom installer, including tampered-file tests and DLL-loading protections). The "0 MB" label wording in Plan 2 depends on stage 6.
