//! Hotplug polling: hidapi has no callback/event API, so a background thread
//! periodically re-enumerates the bus and diffs the set of present Razer PIDs
//! against the set we currently hold open handles for.
//!
//! The diff itself ([`diff_hotplug`]) is a pure function, unit-tested below
//! without any hardware or hidapi state. [`spawn_hotplug_thread`] is the
//! impure wrapper that drives it against real state every ~2s.

use std::collections::BTreeSet;
use std::thread;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::device_manager::DeviceManager;
use crate::tray::update_tray_tooltip;

const POLL_INTERVAL: Duration = Duration::from_secs(2);

/// Result of comparing the currently-present Razer PIDs against the
/// previously-open set.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HotplugDiff {
    pub connected: Vec<u16>,
    pub disconnected: Vec<u16>,
}

/// Pure set-diff. `present` is every registry-known PID currently enumerated
/// on the bus; `open` is every PID we currently hold a device handle for.
/// `BTreeSet` keeps output ordering deterministic for stable events/tests.
pub fn diff_hotplug(present: &BTreeSet<u16>, open: &BTreeSet<u16>) -> HotplugDiff {
    HotplugDiff {
        connected: present.difference(open).copied().collect(),
        disconnected: open.difference(present).copied().collect(),
    }
}

#[derive(Clone, Serialize)]
struct DevicePidPayload {
    pid: u16,
}

/// Spawn the background hotplug-polling thread. Never panics: a poisoned
/// device-manager lock is recovered internally by `DeviceManager` (not
/// propagated), and a failed scan or event emit is logged and simply skipped
/// for that cycle — the thread keeps running either way so an unplug/replug
/// is always caught on the next tick.
///
/// On any change (connect or disconnect) the tray tooltip is refreshed from
/// the manager's current open-device set, so a battery-less (wired) device
/// shows up in the tooltip immediately instead of only ever being covered by
/// the battery-poll loop (which skips battery-less devices entirely).
pub fn spawn_hotplug_thread(app: AppHandle) {
    thread::spawn(move || loop {
        thread::sleep(POLL_INTERVAL);

        let mgr = app.state::<DeviceManager>();
        let diff = mgr.rescan();

        if !diff.connected.is_empty() || !diff.disconnected.is_empty() {
            update_tray_tooltip(&app, &mgr.tooltip_devices());
        }

        // Apply-on-connect: reapply the active profile to freshly-connected
        // devices before announcing them, so the UI's first `list_devices`
        // reflects the applied state.
        if !diff.connected.is_empty() {
            crate::apply_active_profile(&app, &diff.connected);
        }

        for pid in diff.connected {
            if let Err(err) = app.emit("device-connected", DevicePidPayload { pid }) {
                eprintln!("opsrzr: failed to emit device-connected for pid {pid:#06x}: {err}");
            }
        }
        for pid in diff.disconnected {
            if let Err(err) = app.emit("device-disconnected", DevicePidPayload { pid }) {
                eprintln!("opsrzr: failed to emit device-disconnected for pid {pid:#06x}: {err}");
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(pids: &[u16]) -> BTreeSet<u16> {
        pids.iter().copied().collect()
    }

    #[test]
    fn no_change_when_sets_equal() {
        let diff = diff_hotplug(&set(&[1, 2]), &set(&[1, 2]));
        assert!(diff.connected.is_empty());
        assert!(diff.disconnected.is_empty());
    }

    #[test]
    fn detects_new_connection() {
        let diff = diff_hotplug(&set(&[1, 2, 3]), &set(&[1, 2]));
        assert_eq!(diff.connected, vec![3]);
        assert!(diff.disconnected.is_empty());
    }

    #[test]
    fn detects_disconnection() {
        let diff = diff_hotplug(&set(&[1]), &set(&[1, 2]));
        assert!(diff.connected.is_empty());
        assert_eq!(diff.disconnected, vec![2]);
    }

    #[test]
    fn detects_simultaneous_connect_and_disconnect() {
        let diff = diff_hotplug(&set(&[1, 3]), &set(&[1, 2]));
        assert_eq!(diff.connected, vec![3]);
        assert_eq!(diff.disconnected, vec![2]);
    }

    #[test]
    fn empty_sets_produce_empty_diff() {
        let diff = diff_hotplug(&BTreeSet::new(), &BTreeSet::new());
        assert!(diff.connected.is_empty());
        assert!(diff.disconnected.is_empty());
    }

    #[test]
    fn ordering_is_deterministic_ascending() {
        let diff = diff_hotplug(&set(&[5, 1, 3]), &set(&[]));
        assert_eq!(diff.connected, vec![1, 3, 5]);
    }
}
