//! Background battery-poll loop (~60s): reads level + charging for every
//! currently-open, battery-capable device, emits `battery-updated`, updates
//! the tray tooltip, and fires a once-per-discharge-cycle low-battery
//! notification.
//!
//! On a wired-only setup (no battery-capable device open) the loop simply
//! finds nothing to do each tick and idles harmlessly. A failed read (the
//! attached DA V2 Mini is known to occasionally NACK with status 0xfc/0x05)
//! is logged and skipped for that cycle — never treated as fatal or as the
//! device having disappeared (hotplug polling owns that).

use std::thread;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_notification::NotificationExt;

use crate::device_manager::DeviceManager;
use crate::dto::BatteryDto;
use crate::tray::update_tray_tooltip;

const POLL_INTERVAL: Duration = Duration::from_secs(60);
const LOW_BATTERY_THRESHOLD: u8 = 15;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct BatteryUpdatedPayload {
    pid: u16,
    level_percent: u8,
    charging: bool,
}

/// Spawn the background battery-poll thread.
pub fn spawn_battery_loop(app: AppHandle) {
    thread::spawn(move || loop {
        thread::sleep(POLL_INTERVAL);
        poll_once(&app);
    });
}

fn poll_once(app: &AppHandle) {
    let mgr = app.state::<DeviceManager>();

    let pids = mgr.battery_capable_open_pids();

    for pid in pids {
        // Each `read_battery` call locks only that pid's own device mutex
        // for the blocking HID round-trip — never the whole manager — so a
        // slow or NACKing device here never stalls `list_devices` /
        // `get_battery` commands, or reads for other pids.
        let outcome = match mgr.read_battery(pid) {
            Ok(Some(reading)) => {
                let notify =
                    mgr.should_notify_low_battery(pid, reading.level_percent, LOW_BATTERY_THRESHOLD);
                Some((reading, notify))
            }
            Ok(None) => None,
            Err(err) => {
                eprintln!(
                    "opsrzr: battery read failed for pid {pid:#06x}, skipping this cycle: {err}"
                );
                None
            }
        };

        let Some((BatteryDto { level_percent, charging }, notify)) = outcome else {
            continue;
        };

        if let Err(err) = app.emit(
            "battery-updated",
            BatteryUpdatedPayload { pid, level_percent, charging },
        ) {
            eprintln!("opsrzr: failed to emit battery-updated for pid {pid:#06x}: {err}");
        }

        // Shared with the hotplug loop via `tray::format_tooltip` so both
        // stay consistent (see tray.rs doc comment).
        update_tray_tooltip(app, &mgr.tooltip_devices());

        if notify && !charging {
            notify_low_battery(app, level_percent);
        }
    }
}

fn notify_low_battery(app: &AppHandle, level_percent: u8) {
    if let Err(err) = app
        .notification()
        .builder()
        .title("opsrzr")
        .body(format!("Battery low: {level_percent}%"))
        .show()
    {
        eprintln!("opsrzr: failed to show low-battery notification: {err}");
    }
}
