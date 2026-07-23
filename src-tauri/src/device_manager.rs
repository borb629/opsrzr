//! Owns the hidapi context, the embedded device registry, and the set of
//! currently-open device handles.
//!
//! Managed as Tauri `State<'_, DeviceManager>` directly (no outer
//! `Mutex<DeviceManager>` — see below for why).
//!
//! ## Locking
//!
//! Every mutable field is behind its own `Mutex` (`api`, `open`,
//! `battery_cache`, `low_battery_notified`); `registry` is immutable after
//! construction and needs no lock. This lets `DeviceManager` be `Send +
//! Sync` on its own, so Tauri commands (`list_devices`/`get_battery`) never
//! block behind a single app-wide lock held across blocking HID I/O — they
//! only ever contend with the specific per-field lock they themselves touch,
//! held for a HashMap read/write, never for a device round-trip.
//!
//! **Lock-ordering invariant: at most one of this struct's internal
//! `Mutex`es is ever held at a time.** Every method acquires one, does a
//! HashMap-only operation (or opens/reads a device while holding *only* the
//! per-device lock — see below), and drops it before acquiring the next.
//! There is never a nested `lock()` call from inside a held guard, so no
//! lock-ordering cycle is possible.
//!
//! Per-device I/O (`Device::open`, `get_battery_level`, etc.) never runs
//! while any of the four struct-level locks above is held: `open` handles
//! are wrapped `Arc<Mutex<Device>>`, so a caller clones the `Arc` out of the
//! `open` map (dropping the map lock immediately), then locks *that specific
//! device's* mutex to do the blocking I/O. Two different pids can be read
//! concurrently; the same pid's device is naturally serialized by its own
//! mutex, which is exactly the exclusion we need (the underlying hidapi
//! `HidDevice` uses non-thread-safe interior mutability internally).

use std::collections::{BTreeSet, HashMap};
use std::sync::{Arc, Mutex, MutexGuard};

use hidapi::HidApi;
use razer_hid::commands::dpi::DpiStage;
use razer_hid::commands::lighting::Rgb;
use razer_hid::{Device, DeviceDef, Registry};

use crate::dto::{device_info_dto, BatteryDto, DeviceInfoDto, DeviceStateDto};
use crate::hotplug::{diff_hotplug, HotplugDiff};
use crate::profiles::{
    led_id_for, plan_device_settings, varstore_for, DeviceSettings, DpiSettings, Effect,
    LightingSettings, PlannedWrite, REACTIVE_DEFAULT_SPEED, WAVE_DEFAULT_DIRECTION,
};
use crate::registry_embed::embedded_registry;
use crate::tray::TooltipDevice;

/// Execute a planned list of device writes against an open device, in order.
/// Any transport error stops the sequence and is returned as a string. Kept a
/// free function so it takes only a `&Device` (not `&self`), making the
/// with-device locking boundary obvious.
fn execute_writes(device: &Device, writes: &[PlannedWrite]) -> Result<(), String> {
    for write in writes {
        let result = match *write {
            PlannedWrite::Lighting { led_id, varstore, effect, color } => match effect {
                Effect::Static => device.set_static_color(varstore, led_id, color),
                Effect::Breathing => device.set_effect_breathing_single(varstore, led_id, color),
                Effect::Spectrum => device.set_effect_spectrum(varstore, led_id),
                Effect::Wave => device.set_effect_wave(varstore, led_id, WAVE_DEFAULT_DIRECTION),
                Effect::Reactive => {
                    device.set_effect_reactive(varstore, led_id, REACTIVE_DEFAULT_SPEED, color)
                }
                Effect::Off => device.set_effect_none(varstore, led_id),
            },
            PlannedWrite::Brightness { led_id, varstore, value } => {
                device.set_brightness(varstore, led_id, value)
            }
            PlannedWrite::Dpi { varstore, x, y } => device.set_dpi(varstore, x, y),
            PlannedWrite::PollingRate { hz } => device.set_polling_rate(hz),
        };
        result.map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub struct DeviceManager {
    api: Mutex<HidApi>,
    registry: Registry,
    open: Mutex<HashMap<u16, Arc<Mutex<Device>>>>,
    battery_cache: Mutex<HashMap<u16, BatteryDto>>,
    /// Whether a low-battery notification has already fired for the current
    /// discharge cycle (reset once the level recovers above the threshold),
    /// keyed by pid.
    low_battery_notified: Mutex<HashMap<u16, bool>>,
}

impl DeviceManager {
    /// Initialize the hidapi context and load the embedded registry. This is
    /// a one-time startup step (not a per-cycle device-I/O path); a failure
    /// here means the OS HID subsystem itself is unavailable.
    pub fn new() -> Result<Self, String> {
        let api = HidApi::new().map_err(|e| format!("hidapi context init failed: {e}"))?;
        Ok(Self {
            api: Mutex::new(api),
            registry: embedded_registry(),
            open: Mutex::new(HashMap::new()),
            battery_cache: Mutex::new(HashMap::new()),
            low_battery_notified: Mutex::new(HashMap::new()),
        })
    }

    fn lock_api(&self) -> MutexGuard<'_, HidApi> {
        match self.api.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    fn lock_open(&self) -> MutexGuard<'_, HashMap<u16, Arc<Mutex<Device>>>> {
        match self.open.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    fn lock_battery_cache(&self) -> MutexGuard<'_, HashMap<u16, BatteryDto>> {
        match self.battery_cache.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    fn lock_notified(&self) -> MutexGuard<'_, HashMap<u16, bool>> {
        match self.low_battery_notified.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    /// Re-enumerate the HID bus, open newly-seen registry devices, and drop
    /// handles for devices that disappeared. Never panics: any hidapi or
    /// open error is logged and the previous state is preserved for that
    /// device, to be retried on the next cycle.
    ///
    /// Enumeration (`refresh_devices`/`device_list`) and each `Device::open`
    /// call happen while holding only the `api` lock — never the `open` map
    /// lock, and never for longer than that single call. `list_devices` /
    /// `get_battery` (which don't touch `api` at all) are never blocked by a
    /// slow or NACKing device being opened here.
    pub fn rescan(&self) -> HotplugDiff {
        let present: BTreeSet<u16> = {
            let mut api = self.lock_api();
            if let Err(err) = api.refresh_devices() {
                eprintln!("opsrzr: hidapi refresh_devices failed, skipping cycle: {err}");
                return HotplugDiff::default();
            }
            api.device_list()
                .filter(|info| {
                    info.vendor_id() == razer_hid::RAZER_VID
                        && self.registry.find_by_pid(info.product_id()).is_some()
                })
                .map(|info| info.product_id())
                .collect()
        };

        let open_pids: BTreeSet<u16> = self.lock_open().keys().copied().collect();
        let diff = diff_hotplug(&present, &open_pids);

        for pid in &diff.connected {
            let Some(def) = self.registry.find_by_pid(*pid) else {
                continue;
            };
            let opened = {
                let api = self.lock_api();
                Device::open(&api, *pid, def.transaction_id, def.wait_us)
            };
            match opened {
                Ok(device) => {
                    self.lock_open().insert(*pid, Arc::new(Mutex::new(device)));
                }
                Err(err) => {
                    eprintln!("opsrzr: failed to open pid {pid:#06x}, will retry next scan: {err}");
                }
            }
        }
        for pid in &diff.disconnected {
            self.lock_open().remove(pid);
            self.lock_battery_cache().remove(pid);
            self.lock_notified().remove(pid);
        }

        diff
    }

    /// All registry devices, annotated with live connected state and last
    /// known (cached) battery reading.
    pub fn list_devices(&self) -> Vec<DeviceInfoDto> {
        let open_pids: std::collections::HashSet<u16> =
            self.lock_open().keys().copied().collect();
        let battery_snapshot = self.lock_battery_cache().clone();

        self.registry
            .devices()
            .iter()
            .map(|def| {
                let connected = open_pids.contains(&def.usb_pid);
                let battery = battery_snapshot.get(&def.usb_pid).copied();
                device_info_dto(def, connected, battery)
            })
            .collect()
    }

    /// Live battery read for `pid`. Returns `Ok(None)` if the device is
    /// unknown, not battery-capable, or not currently open. A transient read
    /// failure (the DA V2 Mini is known to occasionally NACK with status
    /// 0xfc/0x05) is surfaced as `Err` for the caller to log/skip — this
    /// method itself never panics.
    ///
    /// The blocking read happens while holding only that pid's own device
    /// mutex — never the `open` map lock, so other pids (and
    /// `list_devices`/hotplug rescans) are never blocked behind it.
    pub fn read_battery(&self, pid: u16) -> Result<Option<BatteryDto>, String> {
        let Some(def) = self.registry.find_by_pid(pid) else {
            return Ok(None);
        };
        if !def.capabilities.battery {
            return Ok(None);
        }
        let device_arc = self.lock_open().get(&pid).cloned();
        let Some(device_arc) = device_arc else {
            return Ok(None);
        };

        let (level_percent, charging) = {
            let device = match device_arc.lock() {
                Ok(guard) => guard,
                Err(poisoned) => poisoned.into_inner(),
            };
            let level_percent = device.get_battery_level().map_err(|e| e.to_string())?;
            let charging = device.get_charging_status().map_err(|e| e.to_string())?;
            (level_percent, charging)
        };

        let dto = BatteryDto { level_percent, charging };
        self.lock_battery_cache().insert(pid, dto);
        Ok(Some(dto))
    }

    /// PIDs that are currently open and whose registry capabilities say
    /// `battery = true` — the set the background battery-poll loop drives.
    pub fn battery_capable_open_pids(&self) -> Vec<u16> {
        let pids: Vec<u16> = self.lock_open().keys().copied().collect();
        pids.into_iter()
            .filter(|pid| {
                self.registry
                    .find_by_pid(*pid)
                    .map(|d| d.capabilities.battery)
                    .unwrap_or(false)
            })
            .collect()
    }

    /// Currently-open devices' name + cached battery reading, in ascending
    /// pid order (deterministic), as needed to render the tray tooltip. Fed
    /// straight into `tray::format_tooltip` by both the hotplug and
    /// battery-poll loops so they always agree on the tooltip text.
    pub fn tooltip_devices(&self) -> Vec<TooltipDevice> {
        let mut open_pids: Vec<u16> = self.lock_open().keys().copied().collect();
        open_pids.sort_unstable();
        let battery_snapshot = self.lock_battery_cache().clone();

        open_pids
            .into_iter()
            .filter_map(|pid| {
                self.registry.find_by_pid(pid).map(|def| TooltipDevice {
                    name: def.name.clone(),
                    battery: battery_snapshot.get(&pid).copied(),
                })
            })
            .collect()
    }

    /// PIDs currently open (ascending), regardless of capability. Used to
    /// apply the active profile to everything on startup.
    pub fn open_pids(&self) -> Vec<u16> {
        let mut pids: Vec<u16> = self.lock_open().keys().copied().collect();
        pids.sort_unstable();
        pids
    }

    // ---- Phase 5: write paths + reads for the settings UI ----

    /// Run `f` against the open [`Device`] for `pid`, holding only that
    /// device's own mutex (never a manager-level lock) for its duration —
    /// consistent with the locking invariant documented at the top of this
    /// file. Errors if the device isn't currently open.
    fn with_device<T>(
        &self,
        pid: u16,
        f: impl FnOnce(&Device) -> Result<T, String>,
    ) -> Result<T, String> {
        let device_arc = self.lock_open().get(&pid).cloned();
        let Some(device_arc) = device_arc else {
            return Err(format!("device {pid:#06x} is not connected"));
        };
        let device = match device_arc.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        f(&device)
    }

    fn def_for(&self, pid: u16) -> Result<&DeviceDef, String> {
        self.registry
            .find_by_pid(pid)
            .ok_or_else(|| format!("unknown device pid {pid:#06x}"))
    }

    /// Apply a (possibly partial) [`DeviceSettings`] to `pid` via the pure
    /// planner. Settings for a capability the device lacks are dropped by the
    /// planner, so this is safe to call for any device.
    pub fn apply_settings(&self, pid: u16, settings: &DeviceSettings) -> Result<(), String> {
        let plan = plan_device_settings(self.def_for(pid)?, settings);
        self.with_device(pid, |device| execute_writes(device, &plan))
    }

    /// Set lighting (effect + colour + brightness) for `pid`.
    pub fn set_lighting(
        &self,
        pid: u16,
        effect: Effect,
        color: Rgb,
        brightness: u8,
    ) -> Result<(), String> {
        self.apply_settings(
            pid,
            &DeviceSettings {
                lighting: Some(LightingSettings { effect, color, brightness }),
                ..DeviceSettings::default()
            },
        )
    }

    /// Set direct X/Y DPI for `pid`.
    pub fn set_dpi(&self, pid: u16, x: u16, y: u16) -> Result<(), String> {
        self.apply_settings(
            pid,
            &DeviceSettings { dpi: Some(DpiSettings { x, y }), ..DeviceSettings::default() },
        )
    }

    /// Set the DPI stage table for `pid` (up to 5 stages; `active` is the
    /// 0-based active stage index).
    pub fn set_dpi_stages(
        &self,
        pid: u16,
        stages: &[DpiStage],
        active: u8,
    ) -> Result<(), String> {
        let varstore = varstore_for(self.def_for(pid)?);
        self.with_device(pid, |device| {
            device
                .set_dpi_stages(varstore, active, stages)
                .map_err(|e| e.to_string())
        })
    }

    /// Set the classic polling rate (Hz) for `pid`.
    pub fn set_polling_rate(&self, pid: u16, hz: u16) -> Result<(), String> {
        self.apply_settings(
            pid,
            &DeviceSettings { polling_hz: Some(hz), ..DeviceSettings::default() },
        )
    }

    /// Read direct X/Y DPI for `pid`.
    pub fn get_dpi(&self, pid: u16) -> Result<(u16, u16), String> {
        let varstore = varstore_for(self.def_for(pid)?);
        self.with_device(pid, |device| device.get_dpi(varstore).map_err(|e| e.to_string()))
    }

    /// Read the classic polling rate (Hz) for `pid`, if the wire code is known.
    pub fn get_polling_rate(&self, pid: u16) -> Result<Option<u16>, String> {
        self.with_device(pid, |device| {
            device.get_polling_rate().map_err(|e| e.to_string())
        })
    }

    /// Best-effort read of a device's current DPI / polling / brightness, for
    /// initializing the settings UI. Each sub-read is independent: a NACK on
    /// one leaves that field `None` rather than failing the whole call. Only
    /// the "device not connected" case is a hard error.
    pub fn read_device_state(&self, pid: u16) -> Result<DeviceStateDto, String> {
        let def = self.def_for(pid)?;
        let varstore = varstore_for(def);
        let led = led_id_for(def);
        let caps = def.capabilities.clone();
        self.with_device(pid, |device| {
            let (dpi_x, dpi_y) = if caps.dpi {
                match device.get_dpi(varstore) {
                    Ok((x, y)) => (Some(x), Some(y)),
                    Err(err) => {
                        eprintln!("opsrzr: get_dpi failed for pid {pid:#06x}: {err}");
                        (None, None)
                    }
                }
            } else {
                (None, None)
            };
            let polling_hz = if caps.polling_rate {
                device.get_polling_rate().ok().flatten()
            } else {
                None
            };
            let brightness = if caps.lighting {
                device.get_brightness(varstore, led).ok()
            } else {
                None
            };
            Ok(DeviceStateDto { dpi_x, dpi_y, polling_hz, brightness })
        })
    }

    /// Once-per-discharge-cycle low-battery gate: returns `true` the first
    /// time `level_percent` drops to/below `threshold` for `pid`, `false` on
    /// every subsequent call until the level recovers above the threshold.
    pub fn should_notify_low_battery(&self, pid: u16, level_percent: u8, threshold: u8) -> bool {
        let mut notified = self.lock_notified();
        if level_percent > threshold {
            notified.insert(pid, false);
            return false;
        }
        let already_notified = notified.get(&pid).copied().unwrap_or(false);
        if already_notified {
            false
        } else {
            notified.insert(pid, true);
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // `should_notify_low_battery` is pure state-machine logic over its own
    // HashMap and needs no hidapi/hardware, so it's covered directly here;
    // `rescan`/`read_battery` need a live HidApi and are exercised by the
    // manual runtime verification instead.
    fn manager_for_notify_tests() -> DeviceManager {
        DeviceManager {
            api: Mutex::new(HidApi::new().expect("hidapi init available on CI/dev machine")),
            registry: Registry::new(),
            open: Mutex::new(HashMap::new()),
            battery_cache: Mutex::new(HashMap::new()),
            low_battery_notified: Mutex::new(HashMap::new()),
        }
    }

    #[test]
    fn notifies_once_when_crossing_threshold() {
        let mgr = manager_for_notify_tests();
        assert!(mgr.should_notify_low_battery(1, 14, 15));
        assert!(!mgr.should_notify_low_battery(1, 14, 15), "must not repeat within same cycle");
        assert!(!mgr.should_notify_low_battery(1, 10, 15), "still same discharge cycle");
    }

    #[test]
    fn does_not_notify_above_threshold() {
        let mgr = manager_for_notify_tests();
        assert!(!mgr.should_notify_low_battery(1, 80, 15));
    }

    #[test]
    fn renotifies_after_recovering_above_threshold() {
        let mgr = manager_for_notify_tests();
        assert!(mgr.should_notify_low_battery(1, 10, 15));
        // Recharged above threshold: resets the gate.
        assert!(!mgr.should_notify_low_battery(1, 90, 15));
        // Discharged again: fires once more.
        assert!(mgr.should_notify_low_battery(1, 12, 15));
    }

    #[test]
    fn tracks_independent_devices_separately() {
        let mgr = manager_for_notify_tests();
        assert!(mgr.should_notify_low_battery(1, 5, 15));
        assert!(mgr.should_notify_low_battery(2, 5, 15));
        assert!(!mgr.should_notify_low_battery(1, 5, 15));
    }

    #[test]
    fn tooltip_devices_empty_when_nothing_open() {
        let mgr = manager_for_notify_tests();
        assert!(mgr.tooltip_devices().is_empty());
    }
}
