//! Tauri commands exposed to the frontend: device enumeration + battery reads
//! (Phase 4), the lighting/DPI/polling write paths + live-state reads, and the
//! profile CRUD/apply surface (Phase 5).
//!
//! Every command returns `Result<_, String>`. The I/O commands are `async`
//! (matching `get_battery`) and take only owned args plus the `State`
//! extractor, so none capture a borrow across the call.

use std::collections::HashMap;

use razer_hid::commands::dpi::DpiStage;
use tauri::State;
use tauri_plugin_autostart::ManagerExt;

use crate::device_manager::DeviceManager;
use crate::dto::{BatteryDto, DeviceInfoDto, DeviceStateDto};
use crate::profiles::{DeviceSettings, Effect, Profile, ProfileState};

/// List every registry-known device with live connected state and last
/// known (cached) battery reading.
#[tauri::command]
pub fn list_devices(state: State<'_, DeviceManager>) -> Result<Vec<DeviceInfoDto>, String> {
    Ok(state.list_devices())
}

/// Live battery read for `pid`. `Ok(None)` if the device is unknown,
/// not battery-capable, or not currently open.
#[tauri::command]
pub async fn get_battery(
    pid: u16,
    state: State<'_, DeviceManager>,
) -> Result<Option<BatteryDto>, String> {
    state.read_battery(pid)
}

/// Best-effort read of a device's current DPI / polling / brightness, to
/// initialize the settings-UI controls when a device is selected.
#[tauri::command]
pub async fn read_device_state(
    pid: u16,
    state: State<'_, DeviceManager>,
) -> Result<DeviceStateDto, String> {
    state.read_device_state(pid)
}

/// Read direct X/Y DPI for `pid`, as `[x, y]`.
#[tauri::command]
pub async fn get_dpi(pid: u16, state: State<'_, DeviceManager>) -> Result<[u16; 2], String> {
    let (x, y) = state.get_dpi(pid)?;
    Ok([x, y])
}

/// Read the classic polling rate (Hz) for `pid`, if the wire code is known.
#[tauri::command]
pub async fn get_polling_rate(
    pid: u16,
    state: State<'_, DeviceManager>,
) -> Result<Option<u16>, String> {
    state.get_polling_rate(pid)
}

// ---- Lighting / DPI / polling write paths ----

/// Set lighting (effect + colour + brightness) for `pid`.
#[tauri::command]
pub async fn set_lighting(
    pid: u16,
    effect: Effect,
    r: u8,
    g: u8,
    b: u8,
    brightness: u8,
    state: State<'_, DeviceManager>,
) -> Result<(), String> {
    state.set_lighting(pid, effect, [r, g, b], brightness)
}

/// Set direct X/Y DPI for `pid`.
#[tauri::command]
pub async fn set_dpi(
    pid: u16,
    x: u16,
    y: u16,
    state: State<'_, DeviceManager>,
) -> Result<(), String> {
    state.set_dpi(pid, x, y)
}

/// Set the DPI stage table for `pid`. `stages` is a list of `[x, y]` pairs
/// (max 5); each stage's index is its position in the list. `active` is the
/// 0-based active-stage index.
#[tauri::command]
pub async fn set_dpi_stages(
    pid: u16,
    stages: Vec<[u16; 2]>,
    active: u8,
    state: State<'_, DeviceManager>,
) -> Result<(), String> {
    let stages: Vec<DpiStage> = stages
        .into_iter()
        .enumerate()
        .map(|(i, [x, y])| DpiStage { index: i as u8, dpi_x: x, dpi_y: y })
        .collect();
    state.set_dpi_stages(pid, &stages, active)
}

/// Set the classic polling rate (Hz) for `pid`.
#[tauri::command]
pub async fn set_polling_rate(
    pid: u16,
    hz: u16,
    state: State<'_, DeviceManager>,
) -> Result<(), String> {
    state.set_polling_rate(pid, hz)
}

// ---- Profiles ----

/// Names of all saved profiles, sorted.
#[tauri::command]
pub fn list_profiles(profiles: State<'_, ProfileState>) -> Result<Vec<String>, String> {
    Ok(profiles.store.list())
}

/// The currently active (last-applied) profile name, if any.
#[tauri::command]
pub fn get_active_profile(profiles: State<'_, ProfileState>) -> Result<Option<String>, String> {
    Ok(profiles.active())
}

/// Save `devices` as a named profile (the UI's current working state). Does
/// not change the active profile.
#[tauri::command]
pub async fn save_profile(
    name: String,
    devices: HashMap<u16, DeviceSettings>,
    profiles: State<'_, ProfileState>,
) -> Result<(), String> {
    let profile = Profile { name, devices };
    profiles.store.save(&profile)
}

/// Apply a saved profile to all currently-connected devices and mark it
/// active (persisted, reapplied on next launch and on device connect). Returns
/// the loaded profile so the UI can sync its controls.
#[tauri::command]
pub async fn apply_profile(
    name: String,
    profiles: State<'_, ProfileState>,
    devices: State<'_, DeviceManager>,
) -> Result<Profile, String> {
    let profile = profiles.store.load(&name)?;
    for pid in devices.open_pids() {
        if let Some(settings) = profile.devices.get(&pid) {
            if let Err(err) = devices.apply_settings(pid, settings) {
                eprintln!("opsrzr: apply_profile {name:?} failed for pid {pid:#06x}: {err}");
            }
        }
    }
    profiles.set_active(Some(name))?;
    Ok(profile)
}

/// Delete a saved profile. If it was the active profile, the active pointer is
/// also cleared.
#[tauri::command]
pub async fn delete_profile(
    name: String,
    profiles: State<'_, ProfileState>,
) -> Result<(), String> {
    profiles.store.delete(&name)?;
    if profiles.active().as_deref() == Some(name.as_str()) {
        profiles.set_active(None)?;
    }
    Ok(())
}

// ---- Autostart ----

/// Enable/disable launch-at-login.
#[tauri::command]
pub async fn set_autostart(app: tauri::AppHandle, enabled: bool) -> Result<(), String> {
    let autolaunch = app.autolaunch();
    if enabled {
        autolaunch.enable().map_err(|e| e.to_string())
    } else {
        autolaunch.disable().map_err(|e| e.to_string())
    }
}

/// Whether launch-at-login is currently enabled.
#[tauri::command]
pub fn autostart_enabled(app: tauri::AppHandle) -> Result<bool, String> {
    app.autolaunch().is_enabled().map_err(|e| e.to_string())
}
