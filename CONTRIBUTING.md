# Contributing to opsrzr

Thanks for your interest in contributing!

## Building from source

Prerequisites:

- [Node.js](https://nodejs.org/) (LTS) and npm
- [Rust](https://www.rust-lang.org/tools/install) (stable, MSVC toolchain)
- The [Tauri Windows prerequisites](https://tauri.app/start/prerequisites/)
  (WebView2 is preinstalled on Windows 11; Visual Studio Build Tools with the
  "Desktop development with C++" workload provides the MSVC linker)

Build steps:

```
npm install        # frontend + Tauri CLI dependencies
cargo build        # builds the Rust workspace (razer-hid + app shell)
npm run tauri dev  # runs the app in development mode
```

## Adding support for a new device

Device support in opsrzr is data-driven: each supported device is described
by a TOML file in `devices/` (schema lands in Phase 2 — see
`devices/README.md`). Adding a device usually does **not** require writing
Rust code.

To add a device:

1. Find your device's USB product ID (PID). Vendor ID is always `0x1532`
   (Razer). On Windows, Device Manager → device → Details → Hardware IDs
   shows `VID_1532&PID_xxxx`.
2. Look up the device's capabilities in the OpenRazer daemon sources —
   [`daemon/openrazer_daemon/hardware/mouse.py`](https://github.com/openrazer/openrazer/blob/master/daemon/openrazer_daemon/hardware/mouse.py)
   and
   [`keyboards.py`](https://github.com/openrazer/openrazer/blob/master/daemon/openrazer_daemon/hardware/keyboards.py).
   Each device class lists `USB_PID`, supported `METHODS`, `MATRIX_DIMS`,
   and `DPI_MAX`.
3. Find the device's transaction ID and response wait time in the OpenRazer
   driver switch tables (`driver/razermouse_driver.c`,
   `driver/razerkbd_driver.c`). Common groupings: `0xFF` for older devices,
   `0x3F` for the Lancehead/Basilisk/DeathAdder V2 era, `0x1F` for most
   modern wireless devices.
4. Create `devices/<device-name>.toml` following the schema and existing
   files as examples.
5. Test on real hardware if you own the device, and say so in your PR. If
   you cannot test, mark the definition as untested.

Every protocol fact (command bytes, transaction IDs, timings) must trace to
an OpenRazer source file or a captured USB trace — cite the source in a
comment. Do not invent command IDs.

## Code contributions

- Rust code lives in `crates/razer-hid` (protocol, no Tauri dependencies)
  and `src-tauri` (app shell). Frontend TypeScript lives in `src/`.
- Device I/O must never panic: return `Result`, no `unwrap()` on I/O paths.
- License: GPL-2.0-only. By contributing you agree to license your work
  under the same terms.
