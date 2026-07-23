//! Frontend-facing DTOs and the pure mapping functions that build them.
//!
//! Kept separate from `DeviceManager` so the mapping logic is unit-testable
//! without any hardware or hidapi state.

use razer_hid::{Capabilities, DeviceDef, DeviceType};
use serde::Serialize;

/// A battery reading, as reported to the frontend.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatteryDto {
    pub level_percent: u8,
    pub charging: bool,
}

/// Device capability flags, in the camelCase shape the frontend consumes.
///
/// `razer_hid::Capabilities` deserializes the snake_case `devices/*.toml`
/// files, so it deliberately has no `rename_all` — adding one there would
/// break TOML parsing. This app-side DTO carries the camelCase wire contract
/// instead (notably `pollingRate`, the gap flagged in the Phase 4 review).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilitiesDto {
    pub lighting: bool,
    pub dpi: bool,
    pub polling_rate: bool,
    pub battery: bool,
    pub onboard: bool,
}

/// Map a registry [`Capabilities`] into its camelCase frontend DTO. Pure.
pub fn capabilities_dto(caps: &Capabilities) -> CapabilitiesDto {
    CapabilitiesDto {
        lighting: caps.lighting,
        dpi: caps.dpi,
        polling_rate: caps.polling_rate,
        battery: caps.battery,
        onboard: caps.onboard,
    }
}

/// One row of the `list_devices` command result.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceInfoDto {
    pub name: String,
    pub usb_pid: u16,
    pub device_type: DeviceType,
    pub capabilities: CapabilitiesDto,
    pub connected: bool,
    /// Inclusive DPI range for the mouse page's slider, when the device is a
    /// DPI-capable mouse. `None` for keyboards / devices that don't publish it.
    pub dpi_min: Option<u16>,
    pub dpi_max: Option<u32>,
    /// Last known battery reading. Always `None` for devices whose registry
    /// capabilities don't advertise battery support (e.g. wired-only mice),
    /// even if a stale cached reading were somehow present.
    pub battery: Option<BatteryDto>,
}

/// A device's live-read settings, used to initialize the settings-UI controls
/// when a device is selected. Every field is best-effort: a read that the
/// device NACKs (or a capability the device lacks) surfaces as `null` rather
/// than failing the whole call.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceStateDto {
    pub dpi_x: Option<u16>,
    pub dpi_y: Option<u16>,
    pub polling_hz: Option<u16>,
    pub brightness: Option<u8>,
}

/// Build a [`DeviceInfoDto`] from a registry definition plus live manager
/// state. Pure function: no I/O, fully testable with hand-built `DeviceDef`s.
pub fn device_info_dto(
    def: &DeviceDef,
    connected: bool,
    battery: Option<BatteryDto>,
) -> DeviceInfoDto {
    DeviceInfoDto {
        name: def.name.clone(),
        usb_pid: def.usb_pid,
        device_type: def.device_type,
        capabilities: capabilities_dto(&def.capabilities),
        connected,
        dpi_min: def.dpi_min,
        dpi_max: def.dpi_max,
        battery: if def.capabilities.battery { battery } else { None },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use razer_hid::Registry;

    fn wired_mouse_def() -> DeviceDef {
        Registry::parse_def(
            r#"
            name = "Test Wired Mouse"
            usb_pid = "0x0084"
            device_type = "mouse"

            [capabilities]
            lighting = true
            dpi = true
            polling_rate = true
            battery = false
            "#,
        )
        .expect("parse")
    }

    fn wireless_mouse_def() -> DeviceDef {
        Registry::parse_def(
            r#"
            name = "Test Wireless Mouse"
            usb_pid = "0x0085"
            device_type = "mouse"

            [capabilities]
            lighting = true
            dpi = true
            polling_rate = true
            battery = true
            "#,
        )
        .expect("parse")
    }

    #[test]
    fn maps_name_pid_type_and_capabilities() {
        let def = wired_mouse_def();
        let dto = device_info_dto(&def, true, None);
        assert_eq!(dto.name, "Test Wired Mouse");
        assert_eq!(dto.usb_pid, 0x0084);
        assert_eq!(dto.device_type, DeviceType::Mouse);
        assert_eq!(dto.capabilities, capabilities_dto(&def.capabilities));
        assert!(dto.connected);
    }

    #[test]
    fn battery_is_none_when_capability_absent_even_if_reading_supplied() {
        let def = wired_mouse_def();
        let reading = Some(BatteryDto { level_percent: 42, charging: false });
        let dto = device_info_dto(&def, true, reading);
        assert_eq!(dto.battery, None, "wired device must never surface a battery reading");
    }

    #[test]
    fn battery_passes_through_when_capability_present() {
        let def = wireless_mouse_def();
        let reading = Some(BatteryDto { level_percent: 77, charging: true });
        let dto = device_info_dto(&def, true, reading);
        assert_eq!(dto.battery, reading);
    }

    #[test]
    fn battery_capable_but_no_reading_yet_is_none() {
        let def = wireless_mouse_def();
        let dto = device_info_dto(&def, true, None);
        assert_eq!(dto.battery, None);
    }

    #[test]
    fn disconnected_device_is_still_listed() {
        let def = wired_mouse_def();
        let dto = device_info_dto(&def, false, None);
        assert!(!dto.connected);
    }

    // The struct-equality tests above compare Rust values and would not have
    // caught `BatteryDto` missing `#[serde(rename_all = "camelCase")]` (it
    // still round-trips fine field-for-field in Rust; only the *wire* field
    // names were wrong). Assert the actual serialized JSON shape instead,
    // including the nested `battery` object the frontend destructures as
    // `{ levelPercent, charging }` (src/main.ts).
    #[test]
    fn json_shape_is_camel_case_including_nested_battery() {
        let def = wireless_mouse_def();
        let reading = Some(BatteryDto { level_percent: 77, charging: true });
        let dto = device_info_dto(&def, true, reading);

        let value = serde_json::to_value(&dto).expect("serialize DeviceInfoDto");

        assert_eq!(value["usbPid"], serde_json::json!(0x0085));
        assert_eq!(value["deviceType"], serde_json::json!("mouse"));
        assert_eq!(value["connected"], serde_json::json!(true));
        assert_eq!(value["battery"]["levelPercent"], serde_json::json!(77));
        assert_eq!(value["battery"]["charging"], serde_json::json!(true));

        // Capabilities must serialize camelCase too — `pollingRate` was the
        // gap flagged (but left unfixed) in the Phase 4 review; the frontend
        // destructures `capabilities.pollingRate`.
        assert_eq!(value["capabilities"]["pollingRate"], serde_json::json!(true));
        assert!(
            value["capabilities"].get("polling_rate").is_none(),
            "must not leak snake_case capabilities.polling_rate"
        );

        // Guard against the exact regression this test exists to catch.
        assert!(value.get("usb_pid").is_none(), "must not leak snake_case usb_pid");
        assert!(
            value["battery"].get("level_percent").is_none(),
            "must not leak snake_case battery.level_percent"
        );
    }

    #[test]
    fn json_shape_battery_is_null_when_not_battery_capable() {
        let def = wired_mouse_def();
        let dto = device_info_dto(&def, true, None);
        let value = serde_json::to_value(&dto).expect("serialize DeviceInfoDto");
        assert_eq!(value["battery"], serde_json::Value::Null);
    }
}
