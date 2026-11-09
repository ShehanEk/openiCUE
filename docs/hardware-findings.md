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
- **Second measurement:** at 15 fps with frame pacing (`1181488`), CPU is 19.5-20.3% of one core. All of it is kernel time ("Driver" 20.3%, our code 0%).
- **Model that fits both runs:** each 32-byte block write costs about 3.4 ms of CPU. PIIX4 at the default 93.75 kHz needs (10 + 9 x 34) bit times, about 3.4 ms, and the module waits it out with `microsleep_long`.
  - 15 fps x 4 sticks x 3.4 ms = 20.4%, which matches.
  - The first run's loop slept 33 ms after about 14 ms of writes, so it actually ran at about 21 fps: 21 x 4 x 3.4 ms = 29%, which also matches.
  - So there is no fixed overhead. **CPU ≈ stick writes per second x 3.4 ms.**
- **Levers:** fewer frames per second, or skipping sticks that haven't changed. Not used: raising the SMBus clock, which risks the RAM's SPD and other devices on the bus.
- **At 10 fps (`4261e91`, the user's choice):** 13% of one core, which matches the model (10 x 4 x 3.4 ms ≈ 13.6%). Private memory 1.4 MB.

## 2026-11-07: Stage 5, restore

- **Restore:** `hwtest save rainbow-wave`, then `engine --restore`, exits 0 and applies the effect.
- **After a restart:** in a fresh admin Command Prompt, `start /wait "" target\release\openicue-engine.exe --restore` exits 0. Pass.
- **Non-admin terminal:** the engine exits 3 and never reaches the driver. That's correct: it fails closed. Exit code 3 covers both "another engine running" and "lock creation failed", so it should be split.
- **Built-in effect after a restart, and after a full power-off with 30 s unplugged:** Rainbow wave both times, before any restore ran. **The sticks keep built-in effects themselves.**
- **What this means for restore:** `--restore` is only needed for custom effects. For a built-in effect it just rewrites the same settings, costing about 0.5 s and a save to each stick's own memory at every logon. Plan 3 should skip the rewrite when the saved effect is built-in (open decision).

## 2026-11-07: Stage 6, driver

- **Status:** `Win32_SystemDriver` shows PawnIO as `StartMode: Manual`, `State: Running`. It's still loaded after our process exited, so the driver stays loaded until reboot once something has opened it.
- **Size:** `PawnIO.sys` is 133 KB on disk.
- **Non-paged pool use:** not measured (needs RAMMap or poolmon). Since the driver stays loaded, the app's "0 MB" label for built-in effects must clearly mean openiCUE's own processes, not the driver.

## 2026-11-07: Dependency scan

`cargo audit` (cargo-audit 0.22.2, 1,422 advisories) scanned the 13 crates in `Cargo.lock`: no vulnerabilities, exit code 0.

## 2026-11-07: Stage 7, synchronous PawnIO handle and reply-size check

Commit `6b68d14`: PawnIO is now opened with `SYNCHRONIZE` and `FILE_SYNCHRONOUS_IO_NONALERT`, and every reply must be exactly the requested size.

- **`hwtest info`:** all 4 sticks report product `0x0101`, protocol 4, with no `ShortReply`. Pass.
- **`hwtest effect rainbow-wave`:** `Ok(())` on all 4 sticks. The `reg 0x30` busy lines are the same as before. Pass.
- **Engine `--restore` from a non-admin Command Prompt:** exit code `9` ("couldn't create the engine lock"). Before this change it was 3. Pass.

## 2026-11-08: Stage 8, the window on the RAM

The window (`target\release\openicue.exe`, commit `4bfd399`) ran through the 6 checks: built-in apply, Static with a picked color, a custom effect with live cost, close and reopen while running, Stop, and switching from a custom effect to a built-in one.

- **Result:** the user reported no failures.
- **Breathing's live cost in the window:** **1.5 MB · 0.8% CPU**. That's a share of all logical processors, like Task Manager, and matches the stage 4 model (about 13% of one core on this CPU).

## 2026-11-08: Built-in effects don't stay in sync across sticks

- **Observed:** after Color pulse is applied, the 4 sticks pulse at different moments. Applying a different effect first (Rainbow) doesn't help.
- **Tried (`23fb9e9`):** staged the packet on every stick, then sent the 4 commits back to back, about 1 ms apart. The sticks are still out of phase immediately, so each stick evidently runs the animation on its own timer.
- **Research (2026-11-08):**
  - No public sync, broadcast or timer-reset command exists for Vengeance RGB Pro. OpenRGB's register list has none.
  - Users report the same drift under iCUE's hardware playback.
  - iCUE's synced lighting most likely streams colours from software (direct mode), as openiCUE's custom effects do.
- **Conclusion:** built-in effects run in sync only by chance. In-sync lighting needs software-driven effects, which cost about 1.5 MB and 0.8% CPU.
- **Software sync test (`076b466`, `hwtest sync color-pulse`):** Color pulse drawn in software from one clock with direct writes at 10 fps keeps all 4 sticks in sync and looks right (user, 2026-11-08). This is the way to offer synced built-in effects.
- **All built-in effects in software (2026-11-08):** `hwtest sync` for color-shift, rainbow-wave, color-wave, visor, rain, marquee, rainbow and sequential. The user reports all of them OK and in sync.
- **"Keep sticks in sync" toggle (`eb56a80`, `be50901`, user test 2026-11-08):** all checks passed.
  - **On:** Color pulse runs in software with all sticks in sync, the cost is shown live, and the Stop button appears.
  - **Off:** the RAM runs Color pulse itself at 0 MB · 0% CPU, and the background process stops.
  - **Static:** shows no toggle.

## 2026-11-08: Stick order (Settings → Stick order → Change)

On the user's board, address order is physical slots 1, 3, 2, 4. Software can't learn slot positions, so the user picks the slot of each lit stick once.

- **Build:** `437caf2`, `2e7ac7b`.
- **User test:** all checks passed.
  - The picker lit one stick white at a time.
  - Save stored the order and restored the effect.
  - Synced Sequential and Chase now run slot 1 → 4 in order.

## 2026-11-08: Stage 9, Setup in Windows Sandbox

Builds: `openiCUE-Setup.exe` (4,591,616 bytes, SHA-256 `9A4337DF…`, commit `36d6499`) and a `tamper-test` copy.

- **Results:** the user reports every check passed:
  - the tampered copy stops with the PawnIO security message and installs nothing;
  - a clean install creates the Start menu entry, the Installed apps entry and the PawnIO service;
  - a reinstall skips PawnIO;
  - uninstall keeps PawnIO with the box off and removes it with the box on.
- **PawnIO switches:** the silent switches `-install -silent` and `-uninstall -silent` work.
- **Before the build:** an engine started from the window at 9:08 survived the window's Stop. The admin `hwtest stop` ended it. Re-check the window's Stop at stage 10.

## 2026-11-08: Stage 10, real install

- **Install:** the user installed `openiCUE-Setup.exe` (`9A4337DF…`) and opened openiCUE. The user reports "all ok".
- **Stop and restore (confirmed):** the window's Stop works, and restore at logon works: Breathing comes back after a restart, with no window and no UAC prompt. Stage 10 passed.

## 2026-11-09: Final fixes and logo, reinstalled

Commit `7186ce7`:
- **Fixes:** the engine start wait is now 5 s; Setup restores the effect after install; the uninstall entry is written early; clippy is clean; shared helpers.
- **Logo:** logo C ("glowing stick") is the icon for the app and Setup.

The user reinstalled with the new Setup. The icon shows, and the effect came back right after install.
