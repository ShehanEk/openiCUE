# openiCUE design

Small Windows app that changes the color and animation of Corsair RAM LEDs, uses as little memory as possible, and tells the user what each effect costs before they apply it.

- **UI design (source of truth for screens):** https://claude.ai/artifact/BujCxKxotsdSKvibDfEHj4
- **Target hardware:** 4 x Corsair `CMW16GX4M2C3200C16`, Gigabyte B550 AORUS ELITE AX V2, AMD SMBus controller `1022:790B` revision `0x61`
- **Backend and hosting:** none. No cloud services and no running cost.

## Locked decisions (2026-11-05)

- **Effects:** both built-in effects (run by the RAM) and custom effects (sent by a background process).
- **Cost display:** every effect shows its memory and CPU cost before it is applied. Custom effect figures are always measured, never estimated.
- **Control:** a window that closes fully. No tray icon.
- **Restart:** the last effect is restored at logon.
- **Build:** native Rust app, no web engine and no OpenRGB dependency.
- **Driver:** official signed PawnIO, run unmodified by our installer. The driver is not bundled as loose files or set up by our own code.
- **Installer:** custom dark `openiCUE-Setup.exe`, including uninstall.
- **Look:** minimal, near-black, one column, with the LEDs as the main color and orange only for ongoing resource use.
- **Security:** weighed in every decision. See Security rules.

## Programs

| Program | Runs | Job |
|---|---|---|
| `openiCUE-Setup.exe` | Once, as admin | Verifies and installs the app and PawnIO, creates the logon task |
| `openicue.exe` | When opened, as admin | Window for choosing and applying effects. Exits on close |
| `openicue-engine.exe --restore` | At logon, by scheduled task | Reapplies the last effect, then exits. Keeps running if the effect is custom |
| `openicue-engine.exe --run` | While a custom effect is active | Sends frames to the LEDs. No interface |

The engine is a separate exe (changed 2026-11-05), so a running custom effect doesn't load any window or graphics code. That keeps its memory low and its admin code small.

## App modules

1. **`smbus`:** calls PawnIO by device IO control using the signed `SmbusPIIX4` module, embedded in the exe. Holds `Access_SMBUS.HTP.Method` around bus access. Refuses any address outside `0x58`–`0x5F`.
2. **`ram`:** detects sticks and sends effect and color commands. Exposes named operations only, with no general write function.
3. **`effects`:** the built-in effect list and the three custom frame functions: Breathing, Chase and Gradient flow.
4. **`ui`:** native Windows window matching the design.
5. **`config`:** reads and writes settings and last measured costs. Creates nothing outside its own folder.

## Protocol starting point

Checked against current OpenRGB `Controllers/CorsairDRAMController` on 2026-11-05. It replaces the older fork's sequence.

- **Detection (read-only):** at each address `0x58`–`0x5F`, register `0x43` must read `0x1A`, `0x1B` or `0x1C`, and register `0x44` must read `0x01`, `0x03` or `0x04`. OpenRGB also scans `0x18`–`0x1F`. We don't, because other chips can use that range.
- **Device info:** `0x61`=`0`, `0x21`=`0`, read 32 bytes from `0x40`, compare CRC-8 with `0x42`. Product ID is bytes 3 and 2, protocol is byte 28. Only IDs `0x0100`, `0x0101`, `0x0300` and `0x0301` (10 LEDs) are accepted.
- **Effect write:** `0x0B`=`0`, `0x21`=`0`, 20 bytes to `0x20` (mode, speed, color mode, direction, color 1 RGB + brightness, color 2 RGB + brightness, 8 zeros), compare CRC-8 with `0x42`, `0x82`=`1`, then wait until bit `0x08` of `0x30` clears, up to 5 tries, 10 ms apart. No commit on a CRC mismatch.
- **Static colors:** effect write with mode `0x10`, then the same sequence with RGB + `0xFF` per LED and commit `0x82`=`2`.
- **Direct colors (custom effects, protocol 4 or higher):** one 32-byte block write to `0x31`: LED count, RGB per LED, CRC-8.

**Effect settings:** Static and Marquee take 1 color. Sequential takes 1 color, plus random. Color shift, Color pulse, Color wave, Visor and Rain take 2 colors, plus random. Rainbow wave and Rainbow take none. Directions: Rainbow wave and Color wave use up, down, left and right. Visor uses vertical and horizontal. Rain and Sequential use up and down.

## Data flow

- **Built-in effect:** stop any engine, write the effect, save settings, nothing keeps running.
- **Custom effect:** save settings, start the engine. The window reads the engine's memory and CPU from Windows every 2 seconds while open, and saves the last figures per effect.
- **Stop:** the window signals the engine, which exits cleanly.
- **Bus access:** only one openiCUE process writes at a time, and the app and engine both use the shared bus lock.

## Installer flow

1. Welcome screen explains that openiCUE and the PawnIO driver will be installed, and that an admin prompt comes next.
2. Check every file before changing anything.
3. Install the app to `C:\Program Files\openiCUE`. The location is fixed.
4. Run PawnIO's official setup, or skip it if PawnIO is already installed.
5. Create the logon task and uninstall entry, then show Finish.

Uninstall removes the app, logon task and settings. PawnIO stays by default, with a checkbox to remove it, because other apps may use it.

## Security rules

- **RAM writes:** hardcoded addresses, ID check first, no general write function. PawnIO's module accepts any address, so this limit is our only protection.
- **Setup:** verify PawnIO's Authenticode signature and pinned SHA-256 before running it. On any failure, stop and change nothing. Never install the Unrestricted edition.
- **Setup loading:** static linking and restricted DLL search, because Setup runs as admin from a folder such as Downloads.
- **Install location:** `Program Files` only, because the admin logon task runs this file.
- **Settings:** stored in `C:\ProgramData\openiCUE`, writable by Administrators and System only. Parsed strictly, with effect names limited to a known list and values range-checked. Bad settings fall back to defaults and never reach the bus.
- **Admin processes:** no network code, no plugins, and no command-line options beyond the listed modes. No web pages opened from admin windows.
- **Engine signal:** stop signal limited to admin processes.
- **Logon task:** quoted full path, highest privileges, removed on uninstall.
- **Dependencies:** few Rust crates, committed `Cargo.lock`, `cargo audit` before release.

## Testing

**Automated, no hardware:**
- `smbus` rejects every address outside `0x58`–`0x5F`.
- `ram` produces the exact byte sequences above against a fake bus, and polling times out.
- `config` rejects malformed, oversized and out-of-range input.
- Frame functions stay within range for all inputs.

**On your PC, in this order, stopping on anything unexpected:**
1. Read-only: confirm the module loads and all 4 sticks pass the ID check. No writes.
2. One static color on one stick.
3. All built-in effects on all sticks.
4. Custom effects: check for flicker and measure frame rate, memory and CPU.
5. Restart and full power-off: check what persists and that restore works.
6. Check how much memory PawnIO uses and whether it stays loaded.

**Installer, on Windows Sandbox or a virtual machine first:** clean install, PawnIO already present, tampered PawnIO file, uninstall with and without removing PawnIO. Windows Sandbox can't test the RAM.

## Open questions

- Does PawnIO's license allow including its setup unchanged in ours? If not, Setup downloads the official file during install and checks it the same way. Its silent switches are listed in winget as `-install -silent`, to be tested.
- Does the random-colors flag work on this RAM? OpenRGB advertises it but its code path never sets it.
- Does the driver stay loaded, and how much memory does it use? If it stays, the "0 MB" label must clearly mean openiCUE itself.
- Do built-in effects survive restart and power-off?
- What frame rate can custom effects reach without flicker? Block writes should beat the earlier 15 frames per second estimate.
- How closely can native drawing match the design's glow and rounded shapes?
- Code signing before any public release, since an unsigned Setup shows "Unknown publisher". Not needed for personal use. Cost to be priced.
- License review of any OpenRGB-derived protocol code before distributing the app.
