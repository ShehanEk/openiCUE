# Hardware findings

Results from running `openicue-hwtest` on the target PC: 4 x Corsair `CMW16GX4M2C3200C16`, Gigabyte B550 AORUS ELITE AX V2, AMD SMBus `1022:790B` rev `0x61`, PawnIO 2.2.0 (official signed edition).

## 2026-11-06: Stage 1, read-only detect

Command (admin terminal): `target\release\openicue-hwtest.exe detect`

```
Found 4 sticks:
  0x58
  0x59
  0x5a
  0x5b
```

- **Result:** pass. All 4 sticks pass the ID check at `0x58`–`0x5B`. No writes.
- **Note:** a first run in a non-admin terminal failed with `Os(-1073741790)` (`STATUS_ACCESS_DENIED`) when opening PawnIO. The driver only opens for administrators, as expected. The hwtest error message should say so plainly.

## 2026-11-06: Stage 1b, device info

Command (admin terminal): `target\release\openicue-hwtest.exe info`

```
0x58: product 0x0101, protocol 4
0x59: product 0x0101, protocol 4
0x5a: product 0x0101, protocol 4
0x5b: product 0x0101, protocol 4
```

- **Result:** pass. All 4 sticks are product `0x0101` (Vengeance RGB Pro, 10 LEDs) with valid info checksums.
- **Protocol 4:** direct block writes are supported, so custom effects are possible.
- **Lighting:** unchanged, as expected.

## 2026-11-06: Stage 2, one static color on one stick

Command (admin terminal): `target\release\openicue-hwtest.exe one 58`

- **Attempt 1:** `Err(Bus(Os(433)))`. 433 is PawnIO's `STATUS_NO_SUCH_DEVICE`: the stick didn't acknowledge a transfer.
- **Attempt 2 (with trace):** transfers 1-24 succeeded, including the checksum match and effect commit `0x82`=1. The next 5 status reads of `0x30` (50 ms) got no acknowledgement.
- **Attempt 3:** the wait was made advisory, as in OpenRGB, whose `SetEffect` and `SetColorsPerLED` ignore `WaitReady`. The next write, `0x0B`, also got no acknowledgement. The stick stays silent for more than 65 ms after a commit, probably while it saves to its own memory.
- **Attempt 4:** with the wait raised to 500 ms max, the user reports `Ok` and exactly one stick red. **Pass.**
- **Every attempt:** detect and info on `0x58` worked at the start, so the stick always recovered. A write that goes unacknowledged stops the sequence before any further commit.
- **Still to measure:** the exact silence time. Count the `reg 0x30` trace lines, each about 10 ms.

## 2026-11-06: Stage 3, all built-in effects on all sticks

Commands (admin terminal): `target\release\openicue-hwtest.exe effect <name>` for rainbow-wave, static, color-shift, color-pulse, color-wave, visor, rain, marquee, rainbow and sequential.

- **Result:** pass. Every effect returned `Ok(())` on all 4 sticks, and the user reports all of them look right.
- **Silence after a commit:** on every stick, the status read got no acknowledgement 11 times and an answer on the 12th, so about 110-120 ms.
- **Apply time:** each built-in effect costs about 130 ms per stick, and static costs twice that because it makes 2 commits. For 4 sticks that's about 0.5 s, or 1 s for static. The window should show a short "applying" state rather than freeze.
- **Status register:** it never answers while the stick is busy, so the busy bit can't be read. The 500 ms bounded wait in `ram::write_buffer` covers this.

## 2026-11-06: Stage 4, custom effect (Breathing) through the engine

Commands (admin terminal): `hwtest save breathing`, `Start-Process openicue-engine.exe --run`, `hwtest stop`.

- **Save:** `Saved breathing`. `icacls C:\ProgramData\openiCUE` shows only `NT AUTHORITY\SYSTEM:(OI)(CI)(F)` and `BUILTIN\Administrators:(OI)(CI)(F)`. Pass.
- **Stop:** the engine exits with code 0 when signalled. Pass.
- **Memory:** 1.4 MB private, 9.7 MB working set (shared DLLs included). Pass.
- **CPU: 28.8% of one core** at 30 frames per second (`FRAME_MS` = 33), measured over 10 s. Too high.
  - **Cause:** each frame is 4 block writes of 32 bytes. At SMBus speed each takes about 3 ms of bus time, and PawnIO's PIIX4 module waits for the bus inside the call (`microsleep_long` in `piix4_transaction`). That is about 120 writes/s x ~2.4 ms of CPU each.
  - **Not the cause:** the per-transfer port check is an ioctl with no bus traffic.
  - **What sets the cost:** CPU scales with bytes sent per second, so frame rate and skipping unchanged sticks are the levers. The bus clock is not something we change.
