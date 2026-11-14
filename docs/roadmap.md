# Roadmap

## Status (2026-11-08)

- **Plan 1 (core and hardware):** complete. Verified on the target PC; see `hardware-findings.md`.
- **Plan 2 (window):** complete. Stage 8 passed on 2026-11-08: Breathing costs 1.5 MB · 0.8% CPU.
- **Plan 3 (installer):** complete. Sandbox (stage 9) and real install (stage 10) passed on 2026-11-08.
- **2026-11-09:** final whole-project review fixes and the logo icon (logo C) shipped in `7186ce7`, and the user reinstalled. All plans done.

## Locked decisions

- **2026-11-06:** custom effects run at 10 frames per second. The cost is about 13% of one CPU core, all of it the driver waiting on the SMBus (about 3.4 ms per stick write).
- **2026-11-06:** effect bytes match OpenRGB exactly, and status polls are advisory with a 500 ms maximum. The sticks don't answer for about 110 ms after each commit.
- **2026-11-07:** `--restore` does nothing for built-in effects, because the sticks keep them through restart and power-off and every rewrite saves to their own memory.
- **2026-11-07:** the spec's "no general write function" means: only `ram` calls `Bus`, by rule. `StickAddr` and `StickInfo` keep every write limited to `0x58`–`0x5F`, and only to sticks that passed detect and info.

## Plan 2 (window) inputs

1. **Apply:** write to the RAM only on an explicit Apply, never while dragging a color. Every commit saves to the stick's own memory.
2. **One writer:** the window stops the engine, then holds `claim_engine()` while it writes, so two openiCUE writers never interleave.
3. **Applying state:** applying a built-in effect takes about 0.5 s for 4 sticks, and about 1 s for Static, so show a short "applying" state.
4. **Exit code 3:** split it so the window can tell "engine already running" from "not admin / lock failed".
5. **Driver labels and errors:**
   - **Label:** PawnIO stays loaded after first use, so the "0 MB" label must mean openiCUE itself.
   - **Errors:** explain lock or name squatting by another app as "another program is using the RAM bus".
   - **Detect:** `detect` hides bus errors, so "no sticks" can also mean a lock timeout.
6. **PawnIO handle:** open it synchronously (`SYNCHRONIZE` plus `FILE_SYNCHRONOUS_IO_NONALERT`), check the bytes-returned count, then re-run hardware stage 1.

## Plan 3 (installer) inputs

1. **Settings folder:** if `C:\ProgramData\openiCUE` already exists and fails the admin-only check, remove it without following links and recreate it.
2. **What to ship:** leave `openicue-hwtest` out.
3. **Logon task:** it only matters for custom effects (see the locked decisions).
4. **Install folder:** it must be writable by admins only. The admin window starts `openicue-engine.exe` from its own folder without a UAC prompt, so a swapped file there would run as admin.

## Locked decisions (Plan 2)

- **2026-11-07:** the window's effect settings sit under a divider with a "<effect> settings" label, a user change from the original design. The design canvas is updated to match.
- **2026-11-07:** CPU % is shown as a share of all logical processors, like Task Manager.
- **2026-11-08:** built-in effects get a "Keep sticks in sync" toggle (not Static). Off: the RAM runs them at 0% CPU and the sticks drift apart, because no hardware sync exists. On: the engine draws them in software, in sync, for about 1.5 MB · 0.8% CPU. Settings file is now version 2 (`synced=`); version 1 still loads.
- **2026-11-08:** effects drawn in software follow a saved physical stick order (`order.txt`). The user sets it in Setup and can change it under Settings → Stick order. Nothing can detect slot positions automatically.

## Locked decisions (hardware support)

- **2026-11-14:** Vengeance RGB RS DDR4 (`0x0400`/`0x0401`) is supported with every effect. It gets the same effect packet as the Pro; Static's colour buffer and direct packets hold 6 LEDs. Frames stay 10 LEDs per stick, and an RS stick shows frame LEDs 0, 2, 4, 5, 7 and 9, so a wave lines up with a 10-LED stick beside it. The RS-only hwtest commands are removed; `idblock` stays.
