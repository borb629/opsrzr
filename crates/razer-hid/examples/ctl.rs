//! `ctl` — a hand-rolled CLI test harness for the `razer-hid` feature-command
//! layer. Runnable as:
//!
//! ```text
//! cargo run -p razer-hid --example ctl -- <subcommand> [args...]
//! ```
//!
//! Subcommands:
//! - `list`                                    — enumerate attached Razer HID devices.
//! - `serial <pid>`                             — read the serial number.
//! - `firmware <pid>`                           — read the firmware version.
//! - `battery <pid>`                            — read battery level + charging status.
//! - `color <pid> <r> <g> <b> [led_id]`         — set a static colour (extended-matrix path).
//! - `brightness <pid> <0-255> [led_id]`        — set LED brightness (extended-matrix path).
//! - `dpi <pid> <x> <y>`                        — set direct X/Y DPI.
//! - `getdpi <pid>`                             — read direct X/Y DPI.
//! - `polling <pid> <hz>`                       — set the classic polling rate.
//! - `getpolling <pid>`                         — read the classic polling rate.
//!
//! `<pid>` accepts `0x008c`, `008C`, or `8c`. Every subcommand looks the PID
//! up in the `devices/` registry to source `transaction_id`/`wait_us`; if the
//! PID isn't attached (or isn't in the registry), the tool prints a clear
//! message and exits without panicking — no `unwrap`/`expect` on any
//! device-I/O path.

use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

use hidapi::HidApi;
use razer_hid::commands::lighting::led_id;
use razer_hid::commands::{NOSTORE, VARSTORE};
use razer_hid::{Device, DeviceDef, Registry, TransportError, RAZER_VID};

fn devices_dir() -> PathBuf {
    // crates/razer-hid/examples/ctl.rs -> repo_root/devices
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("devices")
}

fn parse_pid(raw: &str) -> Result<u16, String> {
    let trimmed = raw.trim_start_matches("0x").trim_start_matches("0X");
    u16::from_str_radix(trimmed, 16).map_err(|e| format!("invalid PID {raw:?}: {e}"))
}

fn load_registry() -> Registry {
    match Registry::load_dir(devices_dir()) {
        Ok(reg) => reg,
        Err(e) => {
            eprintln!("warning: failed to load device registry: {e}");
            Registry::new()
        }
    }
}

fn find_def<'a>(registry: &'a Registry, pid: u16) -> Option<&'a DeviceDef> {
    registry.find_by_pid(pid)
}

/// Open a device by PID, using registry parameters when available and
/// falling back to protocol defaults otherwise. Prints a clear message and
/// returns `None` (never panics) when the device isn't attached.
fn open_device(api: &HidApi, registry: &Registry, pid: u16) -> Option<Device> {
    let (transaction_id, wait_us, name) = match find_def(registry, pid) {
        Some(def) => (def.transaction_id, def.wait_us, def.name.clone()),
        None => {
            println!(
                "note: PID {pid:#06x} is not in the devices/ registry; using protocol defaults \
                 (transaction_id=0xFF, wait_us=600)."
            );
            (0xFF, 600, format!("PID {pid:#06x}"))
        }
    };

    match Device::open(api, pid, transaction_id, wait_us) {
        Ok(dev) => Some(dev),
        Err(TransportError::DeviceNotFound { .. }) | Err(TransportError::NoInterface { .. }) => {
            println!("No {name} found. Is it plugged in?");
            None
        }
        Err(e) => {
            println!("Failed to open {name}: {e}");
            None
        }
    }
}

fn cmd_list(api: &HidApi, registry: &Registry) {
    use std::collections::BTreeSet;

    // A physical device exposes several HID interfaces (one per usage page);
    // de-dup by PID so each device is only reported once.
    let mut pids: BTreeSet<u16> = BTreeSet::new();
    for info in api.device_list() {
        if info.vendor_id() == RAZER_VID {
            pids.insert(info.product_id());
        }
    }

    if pids.is_empty() {
        println!("No Razer devices found (VID {RAZER_VID:#06x}).");
        return;
    }

    for pid in pids {
        match find_def(registry, pid) {
            Some(def) => println!("{pid:#06x}  {}", def.name),
            None => println!("{pid:#06x}  (unknown device — not in devices/ registry)"),
        }
    }
}

fn cmd_serial(api: &HidApi, registry: &Registry, pid: u16) {
    let Some(device) = open_device(api, registry, pid) else {
        return;
    };
    match device.get_serial() {
        Ok(serial) => println!("serial: {serial}"),
        Err(e) => println!("failed to read serial: {e}"),
    }
}

fn cmd_firmware(api: &HidApi, registry: &Registry, pid: u16) {
    let Some(device) = open_device(api, registry, pid) else {
        return;
    };
    match device.get_firmware_version() {
        Ok((major, minor)) => println!("firmware: v{major}.{minor}"),
        Err(e) => println!("failed to read firmware version: {e}"),
    }
}

fn cmd_battery(api: &HidApi, registry: &Registry, pid: u16) {
    let Some(device) = open_device(api, registry, pid) else {
        return;
    };
    match device.get_battery_level() {
        Ok(level) => println!("battery: {level}%"),
        Err(e) => println!("failed to read battery level: {e}"),
    }
    match device.get_charging_status() {
        Ok(charging) => println!("charging: {charging}"),
        Err(e) => println!("failed to read charging status: {e}"),
    }
}

fn cmd_color(api: &HidApi, registry: &Registry, pid: u16, rgb: [u8; 3], led: u8) {
    let Some(device) = open_device(api, registry, pid) else {
        return;
    };
    match device.set_static_color(NOSTORE, led, rgb) {
        Ok(()) => println!("set static colour {rgb:02x?} on led {led:#04x}"),
        Err(e) => println!("failed to set colour: {e}"),
    }
}

fn cmd_brightness(api: &HidApi, registry: &Registry, pid: u16, value: u8, led: u8) {
    let Some(device) = open_device(api, registry, pid) else {
        return;
    };
    match device.set_brightness(NOSTORE, led, value) {
        Ok(()) => println!("set brightness {value} on led {led:#04x}"),
        Err(e) => println!("failed to set brightness: {e}"),
    }
}

fn cmd_dpi(api: &HidApi, registry: &Registry, pid: u16, x: u16, y: u16) {
    let Some(device) = open_device(api, registry, pid) else {
        return;
    };
    match device.set_dpi(VARSTORE, x, y) {
        Ok(()) => println!("set dpi {x}x{y}"),
        Err(e) => println!("failed to set dpi: {e}"),
    }
}

fn cmd_getdpi(api: &HidApi, registry: &Registry, pid: u16) {
    let Some(device) = open_device(api, registry, pid) else {
        return;
    };
    match device.get_dpi(VARSTORE) {
        Ok((x, y)) => println!("dpi: {x}x{y}"),
        Err(e) => println!("failed to read dpi: {e}"),
    }
}

fn cmd_getpolling(api: &HidApi, registry: &Registry, pid: u16) {
    let Some(device) = open_device(api, registry, pid) else {
        return;
    };
    match device.get_polling_rate() {
        Ok(Some(hz)) => println!("polling: {hz} Hz"),
        Ok(None) => println!("polling: unknown wire code"),
        Err(e) => println!("failed to read polling rate: {e}"),
    }
}

fn cmd_polling(api: &HidApi, registry: &Registry, pid: u16, hz: u16) {
    let Some(device) = open_device(api, registry, pid) else {
        return;
    };
    match device.set_polling_rate(hz) {
        Ok(()) => println!("set polling rate {hz} Hz"),
        Err(e) => println!("failed to set polling rate: {e}"),
    }
}

fn usage() -> &'static str {
    "usage: ctl <list | serial <pid> | firmware <pid> | battery <pid> | \
color <pid> <r> <g> <b> [led_id] | brightness <pid> <0-255> [led_id] | \
dpi <pid> <x> <y> | getdpi <pid> | polling <pid> <hz> | getpolling <pid>>"
}

fn run() -> Result<(), String> {
    let args: Vec<String> = env::args().skip(1).collect();
    let Some(cmd) = args.first() else {
        println!("{}", usage());
        return Ok(());
    };

    let api = HidApi::new().map_err(|e| format!("failed to initialize hidapi: {e}"))?;
    let registry = load_registry();

    match cmd.as_str() {
        "list" => cmd_list(&api, &registry),
        "serial" => {
            let pid = parse_pid(args.get(1).ok_or("serial requires <pid>")?)?;
            cmd_serial(&api, &registry, pid);
        }
        "firmware" => {
            let pid = parse_pid(args.get(1).ok_or("firmware requires <pid>")?)?;
            cmd_firmware(&api, &registry, pid);
        }
        "battery" => {
            let pid = parse_pid(args.get(1).ok_or("battery requires <pid>")?)?;
            cmd_battery(&api, &registry, pid);
        }
        "color" => {
            let pid = parse_pid(args.get(1).ok_or("color requires <pid>")?)?;
            let r: u8 = args
                .get(2)
                .ok_or("color requires <r>")?
                .parse()
                .map_err(|e| format!("invalid r: {e}"))?;
            let g: u8 = args
                .get(3)
                .ok_or("color requires <g>")?
                .parse()
                .map_err(|e| format!("invalid g: {e}"))?;
            let b: u8 = args
                .get(4)
                .ok_or("color requires <b>")?
                .parse()
                .map_err(|e| format!("invalid b: {e}"))?;
            let led = match args.get(5) {
                Some(raw) => parse_pid(raw)? as u8,
                None => led_id::LOGO,
            };
            cmd_color(&api, &registry, pid, [r, g, b], led);
        }
        "brightness" => {
            let pid = parse_pid(args.get(1).ok_or("brightness requires <pid>")?)?;
            let value: u8 = args
                .get(2)
                .ok_or("brightness requires <0-255>")?
                .parse()
                .map_err(|e| format!("invalid brightness: {e}"))?;
            let led = match args.get(3) {
                Some(raw) => parse_pid(raw)? as u8,
                None => led_id::LOGO,
            };
            cmd_brightness(&api, &registry, pid, value, led);
        }
        "dpi" => {
            let pid = parse_pid(args.get(1).ok_or("dpi requires <pid>")?)?;
            let x: u16 = args
                .get(2)
                .ok_or("dpi requires <x>")?
                .parse()
                .map_err(|e| format!("invalid x: {e}"))?;
            let y: u16 = args
                .get(3)
                .ok_or("dpi requires <y>")?
                .parse()
                .map_err(|e| format!("invalid y: {e}"))?;
            cmd_dpi(&api, &registry, pid, x, y);
        }
        "getdpi" => {
            let pid = parse_pid(args.get(1).ok_or("getdpi requires <pid>")?)?;
            cmd_getdpi(&api, &registry, pid);
        }
        "getpolling" => {
            let pid = parse_pid(args.get(1).ok_or("getpolling requires <pid>")?)?;
            cmd_getpolling(&api, &registry, pid);
        }
        "polling" => {
            let pid = parse_pid(args.get(1).ok_or("polling requires <pid>")?)?;
            let hz: u16 = args
                .get(2)
                .ok_or("polling requires <hz>")?
                .parse()
                .map_err(|e| format!("invalid hz: {e}"))?;
            cmd_polling(&api, &registry, pid, hz);
        }
        other => {
            println!("unknown subcommand {other:?}");
            println!("{}", usage());
        }
    }

    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            eprintln!("{}", usage());
            ExitCode::FAILURE
        }
    }
}
