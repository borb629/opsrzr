//! Tray icon: menu ("Open opsrzr" / separator / "Quit"), left-click-to-show,
//! and the tooltip the battery-poll loop updates.
//!
//! Tauri v2 APIs only (`tauri::tray::TrayIconBuilder`, `tauri::menu::*`) —
//! the v1 `SystemTray`/`CustomMenuItem` APIs do not exist in this version.

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager};

use crate::dto::BatteryDto;

/// Id used to look the tray icon back up later (e.g. to update its tooltip
/// from the hotplug/battery-poll loops) via `AppHandle::tray_by_id`.
pub const TRAY_ID: &str = "main";

/// One currently-open device's info as needed to render the tray tooltip.
/// Built by `DeviceManager::tooltip_devices` from live open-device + registry
/// + battery-cache state.
#[derive(Debug, Clone, PartialEq)]
pub struct TooltipDevice {
    pub name: String,
    pub battery: Option<BatteryDto>,
}

/// Pure formatting: build the tray tooltip text for the currently-open
/// devices. A battery-less (wired) device shows its name alone; a
/// battery-capable device shows `name: NN%`, with a `(charging)` suffix
/// while charging. Multiple devices are joined with `, `. Zero devices
/// falls back to the bare app name.
///
/// Shared by the hotplug (connect/disconnect) and battery-poll loops so both
/// always render the same tooltip for the same state — see
/// `update_tray_tooltip` below, the single call site that invokes this.
pub fn format_tooltip(devices: &[TooltipDevice]) -> String {
    if devices.is_empty() {
        return "opsrzr".to_string();
    }
    let parts: Vec<String> = devices
        .iter()
        .map(|d| match d.battery {
            Some(BatteryDto { level_percent, charging }) => {
                let suffix = if charging { " (charging)" } else { "" };
                format!("{}: {level_percent}%{suffix}", d.name)
            }
            None => d.name.clone(),
        })
        .collect();
    format!("opsrzr — {}", parts.join(", "))
}

/// Update the tray tooltip from the current set of open devices. Never
/// panics: a missing tray icon or a failed `set_tooltip` call is logged and
/// skipped, never fatal.
pub fn update_tray_tooltip(app: &AppHandle, devices: &[TooltipDevice]) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return;
    };
    let tooltip = format_tooltip(devices);
    println!("opsrzr: tray tooltip updated: {tooltip}");
    if let Err(err) = tray.set_tooltip(Some(tooltip)) {
        eprintln!("opsrzr: failed to update tray tooltip: {err}");
    }
}

/// Build and register the tray icon + menu during `.setup(...)`.
pub fn build_tray(app: &tauri::App) -> tauri::Result<()> {
    let open_item = MenuItem::with_id(app, "open", "Open opsrzr", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open_item, &separator, &quit_item])?;

    let icon = app
        .default_window_icon()
        .expect("bundle icon configured in tauri.conf.json")
        .clone();

    TrayIconBuilder::with_id(TRAY_ID)
        .icon(icon)
        .tooltip("opsrzr")
        .menu(&menu)
        // Right-click still opens the menu natively; left-click is wired
        // below to show the window instead of also popping the menu.
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => show_main_window(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main_window(tray.app_handle());
            }
        })
        .build(app)?;

    Ok(())
}

fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    } else {
        eprintln!("opsrzr: main window not found when handling a tray event");
    }
}

#[cfg(test)]
mod format_tooltip_tests {
    use super::*;

    fn wired(name: &str) -> TooltipDevice {
        TooltipDevice { name: name.to_string(), battery: None }
    }

    fn battery_device(name: &str, level_percent: u8, charging: bool) -> TooltipDevice {
        TooltipDevice {
            name: name.to_string(),
            battery: Some(BatteryDto { level_percent, charging }),
        }
    }

    #[test]
    fn zero_devices_falls_back_to_bare_app_name() {
        assert_eq!(format_tooltip(&[]), "opsrzr");
    }

    #[test]
    fn one_wired_device_shows_name_alone() {
        let devices = [wired("Razer DeathAdder V2 Mini")];
        assert_eq!(format_tooltip(&devices), "opsrzr — Razer DeathAdder V2 Mini");
    }

    #[test]
    fn one_battery_device_shows_name_and_percent() {
        let devices = [battery_device("Razer Mamba Wireless", 77, false)];
        assert_eq!(format_tooltip(&devices), "opsrzr — Razer Mamba Wireless: 77%");
    }

    #[test]
    fn charging_battery_device_appends_charging_suffix() {
        let devices = [battery_device("Razer Mamba Wireless", 42, true)];
        assert_eq!(
            format_tooltip(&devices),
            "opsrzr — Razer Mamba Wireless: 42% (charging)"
        );
    }

    #[test]
    fn mixed_wired_and_battery_devices_join_with_comma() {
        let devices = [
            wired("Razer DeathAdder V2 Mini"),
            battery_device("Razer Mamba Wireless", 90, false),
        ];
        assert_eq!(
            format_tooltip(&devices),
            "opsrzr — Razer DeathAdder V2 Mini, Razer Mamba Wireless: 90%"
        );
    }

    #[test]
    fn multiple_wired_devices_join_with_comma() {
        let devices = [wired("Razer DeathAdder V2 Mini"), wired("Razer Basilisk V3")];
        assert_eq!(
            format_tooltip(&devices),
            "opsrzr — Razer DeathAdder V2 Mini, Razer Basilisk V3"
        );
    }
}
