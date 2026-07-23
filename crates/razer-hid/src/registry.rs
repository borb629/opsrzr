//! Device registry: TOML-backed map of Razer USB PIDs to their protocol
//! parameters and capabilities.
//!
//! Capability data is ported from the OpenRazer daemon per-device classes
//! (`daemon/openrazer_daemon/hardware/mouse.py` / `keyboards.py`); transaction
//! ids and wait intervals from the OpenRazer kernel driver switch tables. Each
//! seeded `devices/*.toml` file carries its own `# source:` citation.

use std::path::Path;

use serde::{Deserialize, Serialize};

/// Default transaction id when a device omits it (older devices).
///
// source: OpenRazer razermouse_driver.c — 0xFF is the fallback transaction id.
pub const DEFAULT_TRANSACTION_ID: u8 = 0xFF;

/// Default send->receive wait in microseconds.
///
// source: Phase 0 timing table — mice/keyboards default 600 µs.
pub const DEFAULT_WAIT_US: u32 = 600;

/// Physical class of a device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DeviceType {
    Mouse,
    Keyboard,
}

/// Feature flags advertised by a device.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Capabilities {
    #[serde(default)]
    pub lighting: bool,
    #[serde(default)]
    pub dpi: bool,
    #[serde(default)]
    pub polling_rate: bool,
    #[serde(default)]
    pub battery: bool,
    /// Whether the device persists LED/DPI writes to onboard flash when the
    /// command carries the VARSTORE flag (arg0=0x01). Defaults to `false`:
    /// there is NO Synapse-style onboard profile-slot protocol, and per-device
    /// persistence support is not published by any consulted source, so opsrzr
    /// treats persistence as opt-in per device rather than assuming it. When
    /// `false`, writes use NOSTORE and profiles are reapplied app-side on
    /// connect instead.
    // source: plans/01-opsrzr-v1.md Phase 0 "Onboard profiles — GAP" note —
    // OpenRazer persists only via the VARSTORE flag on LED/DPI/device-mode
    // commands; no full onboard-slot command set exists.
    #[serde(default)]
    pub onboard: bool,
}

/// One named LED region (e.g. logo, scroll wheel, backlight).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LedRegion {
    /// Human-readable name.
    pub name: String,
    /// LED id byte used in lighting commands.
    ///
    // source: OpenRazer razercommon.h LED ids (scroll 0x01, logo 0x04,
    // backlight 0x05, ...).
    pub id: u8,
}

/// Hex string (`"0x0084"` or `"0084"`) deserialized into a `u16` PID / `u8` tid.
mod hex_num {
    use serde::{Deserialize, Deserializer};

    pub fn parse_u16<'de, D>(de: D) -> Result<u16, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = String::deserialize(de)?;
        let trimmed = raw.trim_start_matches("0x").trim_start_matches("0X");
        u16::from_str_radix(trimmed, 16).map_err(serde::de::Error::custom)
    }

    pub fn parse_u8<'de, D>(de: D) -> Result<u8, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = String::deserialize(de)?;
        let trimmed = raw.trim_start_matches("0x").trim_start_matches("0X");
        u8::from_str_radix(trimmed, 16).map_err(serde::de::Error::custom)
    }
}

fn default_transaction_id() -> u8 {
    DEFAULT_TRANSACTION_ID
}

fn default_wait_us() -> u32 {
    DEFAULT_WAIT_US
}

/// A single device definition parsed from a `devices/*.toml` file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceDef {
    /// Marketing name.
    pub name: String,
    /// USB product id (Razer VID is always 0x1532). Written as a hex string.
    #[serde(deserialize_with = "hex_num::parse_u16")]
    pub usb_pid: u16,
    /// Mouse or keyboard.
    pub device_type: DeviceType,
    /// Per-device transaction id byte, written as a hex string.
    #[serde(
        deserialize_with = "hex_num::parse_u8",
        default = "default_transaction_id"
    )]
    pub transaction_id: u8,
    /// Send->receive wait interval in microseconds.
    #[serde(default = "default_wait_us")]
    pub wait_us: u32,
    /// Feature flags.
    #[serde(default)]
    pub capabilities: Capabilities,
    /// Named LED regions.
    #[serde(default)]
    pub led_regions: Vec<LedRegion>,
    /// Matrix dimensions `[rows, cols]`, if the device is addressable-RGB.
    #[serde(default)]
    pub matrix_dims: Option<[u16; 2]>,
    /// Minimum DPI.
    #[serde(default)]
    pub dpi_min: Option<u16>,
    /// Maximum DPI.
    #[serde(default)]
    pub dpi_max: Option<u32>,
}

/// Errors from loading device definitions.
#[derive(Debug, thiserror::Error)]
pub enum RegistryError {
    #[error("i/o error reading device definitions: {0}")]
    Io(#[from] std::io::Error),
    #[error("failed to parse device definition {path}: {source}")]
    Parse {
        path: String,
        #[source]
        source: toml::de::Error,
    },
}

/// A collection of loaded device definitions, keyed by lookup on PID.
#[derive(Debug, Clone, Default)]
pub struct Registry {
    devices: Vec<DeviceDef>,
}

impl Registry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Parse a single TOML document into a `DeviceDef`.
    pub fn parse_def(toml_src: &str) -> Result<DeviceDef, toml::de::Error> {
        toml::from_str(toml_src)
    }

    /// Load all `*.toml` files from a directory (non-recursive).
    ///
    /// Path resolution is the caller's responsibility: pass the repository's
    /// `devices/` directory (resolved relative to the executable or an env var
    /// by the app layer). Missing directory returns an empty registry.
    pub fn load_dir(dir: impl AsRef<Path>) -> Result<Self, RegistryError> {
        let dir = dir.as_ref();
        let mut devices = Vec::new();
        if !dir.exists() {
            return Ok(Self { devices });
        }
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("toml") {
                continue;
            }
            let src = std::fs::read_to_string(&path)?;
            let def: DeviceDef = toml::from_str(&src).map_err(|source| RegistryError::Parse {
                path: path.display().to_string(),
                source,
            })?;
            devices.push(def);
        }
        Ok(Self { devices })
    }

    /// Add a definition (used in tests and by callers embedding definitions).
    pub fn push(&mut self, def: DeviceDef) {
        self.devices.push(def);
    }

    /// All loaded definitions.
    pub fn devices(&self) -> &[DeviceDef] {
        &self.devices
    }

    /// Look up a device by USB PID.
    pub fn find_by_pid(&self, pid: u16) -> Option<&DeviceDef> {
        self.devices.iter().find(|d| d.usb_pid == pid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
name = "Razer DeathAdder V2"
usb_pid = "0x0084"
device_type = "mouse"
transaction_id = "0x3f"
wait_us = 600
matrix_dims = [1, 2]
dpi_min = 100
dpi_max = 20000

[capabilities]
lighting = true
dpi = true
polling_rate = true
battery = false

[[led_regions]]
name = "logo"
id = 0x04

[[led_regions]]
name = "scroll"
id = 0x01
"#;

    #[test]
    fn parse_sample_toml() {
        let def = Registry::parse_def(SAMPLE).expect("parse");
        assert_eq!(def.name, "Razer DeathAdder V2");
        assert_eq!(def.usb_pid, 0x0084);
        assert_eq!(def.device_type, DeviceType::Mouse);
        assert_eq!(def.transaction_id, 0x3F);
        assert_eq!(def.wait_us, 600);
        assert!(def.capabilities.lighting);
        assert!(def.capabilities.dpi);
        assert!(!def.capabilities.battery);
        assert_eq!(def.led_regions.len(), 2);
        assert_eq!(def.led_regions[0].name, "logo");
        assert_eq!(def.led_regions[0].id, 0x04);
        assert_eq!(def.matrix_dims, Some([1, 2]));
        assert_eq!(def.dpi_min, Some(100));
        assert_eq!(def.dpi_max, Some(20000));
    }

    #[test]
    fn defaults_apply_when_omitted() {
        let src = r#"
name = "Minimal"
usb_pid = "0x00AA"
device_type = "keyboard"
"#;
        let def = Registry::parse_def(src).expect("parse");
        assert_eq!(def.transaction_id, DEFAULT_TRANSACTION_ID);
        assert_eq!(def.wait_us, DEFAULT_WAIT_US);
        assert_eq!(def.capabilities, Capabilities::default());
        assert!(def.led_regions.is_empty());
        assert_eq!(def.matrix_dims, None);
    }

    #[test]
    fn registry_lookup_by_pid() {
        let mut reg = Registry::new();
        reg.push(Registry::parse_def(SAMPLE).expect("parse"));
        assert!(reg.find_by_pid(0x0084).is_some());
        assert!(reg.find_by_pid(0x9999).is_none());
    }
}
