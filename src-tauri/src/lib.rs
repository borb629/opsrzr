mod battery;
mod commands;
mod device_manager;
mod dto;
mod hotplug;
mod profiles;
mod registry_embed;
mod tray;

use tauri::{AppHandle, Manager};
use tauri_plugin_autostart::MacosLauncher;

use device_manager::DeviceManager;
use profiles::{ProfileState, ProfileStore};

/// Apply the active profile's settings to the given PIDs, if a profile is
/// active and it has an entry for that device. Used both at startup (for
/// already-connected devices) and by the hotplug loop (apply-on-connect).
///
/// Never fatal: a missing/corrupt profile or a failed device write is logged
/// and skipped so a bad profile can't wedge hotplug handling.
pub(crate) fn apply_active_profile(app: &AppHandle, pids: &[u16]) {
    if pids.is_empty() {
        return;
    }
    let profiles = app.state::<ProfileState>();
    let Some(active) = profiles.active() else {
        return;
    };
    let profile = match profiles.store.load(&active) {
        Ok(p) => p,
        Err(err) => {
            eprintln!("opsrzr: active profile {active:?} could not be loaded: {err}");
            return;
        }
    };
    let mgr = app.state::<DeviceManager>();
    for &pid in pids {
        if let Some(settings) = profile.devices.get(&pid) {
            if let Err(err) = mgr.apply_settings(pid, settings) {
                eprintln!(
                    "opsrzr: applying profile {active:?} to pid {pid:#06x} failed: {err}"
                );
            }
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // Hard requirement: single-instance must be registered first in the
        // plugin chain.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_autostart::init(MacosLauncher::LaunchAgent, None))
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .manage(DeviceManager::new().expect("hidapi context init failed at startup"))
        .invoke_handler(tauri::generate_handler![
            commands::list_devices,
            commands::get_battery,
            commands::read_device_state,
            commands::get_dpi,
            commands::get_polling_rate,
            commands::set_lighting,
            commands::set_dpi,
            commands::set_dpi_stages,
            commands::set_polling_rate,
            commands::list_profiles,
            commands::get_active_profile,
            commands::save_profile,
            commands::apply_profile,
            commands::delete_profile,
            commands::set_autostart,
            commands::autostart_enabled,
        ])
        .setup(|app| {
            tray::build_tray(app)?;

            // Profile store lives under %APPDATA%/<identifier>/. If the config
            // dir can't be resolved we still run (profiles simply unavailable).
            match app.path().app_config_dir() {
                Ok(dir) => {
                    app.manage(ProfileState::new(ProfileStore::new(dir)));
                }
                Err(err) => {
                    eprintln!("opsrzr: could not resolve app config dir, profiles disabled: {err}");
                    // Fall back to a store rooted at the current dir so the
                    // ProfileState managed type still exists for commands.
                    app.manage(ProfileState::new(ProfileStore::new(".")));
                }
            }

            // Populate initial device state synchronously so a wired device
            // already plugged in at startup shows up in the tray tooltip
            // right away, instead of waiting for the first hotplug poll tick.
            let mgr = app.state::<DeviceManager>();
            mgr.rescan();
            tray::update_tray_tooltip(app.handle(), &mgr.tooltip_devices());

            // Reapply the last-active profile to whatever is already plugged in.
            let open = mgr.open_pids();
            apply_active_profile(app.handle(), &open);

            hotplug::spawn_hotplug_thread(app.handle().clone());
            battery::spawn_battery_loop(app.handle().clone());
            Ok(())
        })
        .on_window_event(|window, event| {
            // Hide-to-tray: closing the window never exits the app; only
            // the tray's Quit menu item does.
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let _ = window.hide();
                api.prevent_close();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
