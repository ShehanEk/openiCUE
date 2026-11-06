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
