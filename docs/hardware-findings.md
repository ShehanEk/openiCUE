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
