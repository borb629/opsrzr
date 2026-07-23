<!-- Badges assume the repo lives at github.com/atv57/opsrzr — update the owner
     if you fork or host it elsewhere. -->

<div align="center">
  <img src="public/logo.png" alt="opsrzr logo" width="84" height="84" />

  <h1>opsrzr</h1>

  <p><strong>A lightweight, open-source Razer Synapse alternative for Windows.</strong></p>

  <p>
    <a href="LICENSE"><img src="https://img.shields.io/badge/License-GPLv2-blue.svg" alt="License: GPL v2" /></a>
    <a href="https://github.com/atv57/opsrzr/actions/workflows/release.yml"><img src="https://github.com/atv57/opsrzr/actions/workflows/release.yml/badge.svg" alt="Release" /></a>
    <img src="https://img.shields.io/badge/platform-Windows-0078D6.svg" alt="Platform: Windows" />
  </p>
</div>

opsrzr lives in your system tray, uses tens of megabytes instead of hundreds,
and installs no background service. It talks to your Razer mouse or keyboard
directly over USB HID, so there is no daemon, no account, and no telemetry.

## Features

- **RGB lighting** — static colour, breathing, spectrum, wave, reactive, and
  off, with a brightness slider.
- **DPI** — a slider + numeric entry clamped to your device's range, plus a
  DPI-stages editor (2 to 5 stages) that writes to the device as you edit, so
  the physical DPI button cycles your current presets.
- **Polling rate** — 125 / 500 / 1000 Hz.
- **Battery** — level and charging state in the tray tooltip and the app, with
  a low-battery notification (for wireless devices).
- **Profiles** — named settings saved as JSON under
  `%APPDATA%\dev.opsrzr.app\profiles`, reapplied automatically when a device
  connects. Settings persist to onboard memory (VARSTORE) where the device
  supports it.
- **Daemonless** — a single tray app, no elevated service. Hides to tray on
  close; quits only from the tray menu. Optional launch-at-login.

Idle working set measured at ~40 MB (debug build; release builds are lower) —
versus the hundreds of megabytes an Electron-based utility typically uses.

## Interface &amp; design

opsrzr is deliberately not a gaming-RGB dashboard. The UI is a calm *soft
instrument panel*: a stable three-zone desktop layout (device rail · settings
canvas · summary), grouped settings instead of a field of floating cards, and
colour used only for status, selection, and real actions. It renders through the
Windows-native WebView2 with the system variable font, in dark mode.

- **Capability-gated** — each device shows only the controls it actually
  supports (battery for wireless, DPI and stages for mice, lighting and polling
  where present); nothing is fabricated.
- **Accessible** — WCAG 2.2 AA contrast, always-visible keyboard focus, honours
  `prefers-reduced-motion` and `prefers-contrast`, and never signals state by
  colour alone.

The full design contract lives in [DESIGN.md](DESIGN.md), with machine-readable
tokens in [`docs/design-tokens.json`](docs/design-tokens.json).

## Supported devices

Device support is data-driven — each device is a TOML file in
[`devices/`](devices/), so adding one is usually a data contribution, not a
code change (see [CONTRIBUTING.md](CONTRIBUTING.md)).

| Device | USB PID | Type | Lighting | DPI range | Polling | Battery |
| --- | --- | --- | :---: | --- | :---: | :---: |
| Razer DeathAdder V2 | `0x0084` | Mouse | ✓ | 100–20000 | ✓ | — |
| Razer DeathAdder V2 Mini | `0x008C` | Mouse | ✓ | 100–8500 | ✓ | — |
| Razer Basilisk V3 | `0x0099` | Mouse | ✓ | 100–26000 | ✓ | — |
| Razer BlackWidow V3 | `0x024E` | Keyboard | ✓ | — | ✓ | — |

Don't see your device? [Adding it](CONTRIBUTING.md#adding-support-for-a-new-device)
is straightforward if you can find its USB PID.

## Installing

Download the latest `opsrzr_*-setup.exe` from the
[Releases page](https://github.com/atv57/opsrzr/releases) and run it.

> **SmartScreen note:** releases are unsigned (code-signing certificates cost
> money), so Windows SmartScreen may show a warning on first run. Choose
> **More info → Run anyway**. If you'd rather not trust a prebuilt binary,
> build from source below.

WebView2 (used to render the UI) is preinstalled on Windows 11; on older
Windows the installer downloads it automatically.

## Building from source

Prerequisites: [Node.js](https://nodejs.org/) (LTS),
[Rust](https://www.rust-lang.org/tools/install) (stable MSVC toolchain), and
the [Tauri Windows prerequisites](https://tauri.app/start/prerequisites/)
(Visual Studio Build Tools with the "Desktop development with C++" workload).

```
npm install        # frontend + Tauri CLI dependencies
npm run tauri dev  # run in development
npm run tauri build  # produce an installer under src-tauri/target/release/bundle
```

Run the tests and the on-device CLI harness:

```
cargo test --workspace
cargo run -p razer-hid --example ctl -- list
```

## Architecture

- **[`crates/razer-hid`](crates/razer-hid)** — a standalone Rust library (no
  Tauri dependencies) implementing the Razer USB HID protocol: the 90-byte
  report codec with CRC, the Windows feature-report transport over hidapi, the
  feature commands (lighting / DPI / polling / battery / info), and the
  per-device registry.
- **[`src-tauri`](src-tauri)** — the Tauri 2 app: tray, hotplug polling,
  device manager, settings commands, and profile storage.
- **[`src`](src)** — the vanilla-TypeScript settings UI.
- **[`devices/`](devices)** — per-device TOML capability definitions.

## Credits

- **[OpenRazer](https://github.com/openrazer/openrazer)** (GPL-2.0) — the
  definitive open-source implementation of the Razer USB protocol. opsrzr's
  protocol implementation and device capability data are ported from
  OpenRazer's driver and daemon sources with per-constant attribution, which
  is why opsrzr is GPL-2.0.
- **[razer-ctl](https://github.com/tdakhran/razer-ctl)** — prior art for Rust
  protocol framing over hidapi.

## License

GPL-2.0-only. See [LICENSE](LICENSE).
