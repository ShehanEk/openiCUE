# Roadmap

## Status (2026-11-07)

Plan 1 (core and hardware) is complete. Everything was verified on the target PC; see `hardware-findings.md`. 58 tests pass and `cargo audit` is clean. Plan 2 (window) and Plan 3 (installer) are next.

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
