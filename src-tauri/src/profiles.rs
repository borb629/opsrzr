//! Profiles: the app-side domain model for a device's desired settings, the
//! pure planner that turns those settings into an ordered list of device
//! writes, and the JSON-file store under `%APPDATA%/<identifier>/`.
//!
//! There is deliberately no Synapse-style onboard profile-slot protocol (see
//! the Phase 0 gap note in `plans/01-opsrzr-v1.md`): v1 profiles are app-side
//! JSON that we reapply on connect, optionally backed by per-command VARSTORE
//! persistence where a device's registry entry marks it supported
//! (`capabilities.onboard`).
//!
//! The model/planner/sanitizer are all pure and unit-tested here without any
//! hardware or filesystem state; `ProfileStore` is the thin I/O layer, and the
//! executor that actually drives a [`razer_hid::Device`] lives in
//! `device_manager.rs`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use razer_hid::commands::lighting::{led_id, Rgb};
use razer_hid::commands::{NOSTORE, VARSTORE};
use razer_hid::DeviceDef;
use serde::{Deserialize, Serialize};

/// A lighting effect the UI can select. The extended-matrix (class 0x0F) path
/// is used for all of these; `Off` maps to the "no effect" command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Effect {
    Static,
    Breathing,
    Spectrum,
    Wave,
    Reactive,
    /// Effect off. Wire value is `"none"` to match the UI's selector option.
    #[serde(rename = "none")]
    Off,
}

/// Default wave direction (`0x00..=0x02`); left-to-right.
pub const WAVE_DEFAULT_DIRECTION: u8 = 0x01;
/// Default reactive speed (`0x01..=0x04`); medium.
pub const REACTIVE_DEFAULT_SPEED: u8 = 0x02;

/// Desired lighting state for a device's primary LED region.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LightingSettings {
    pub effect: Effect,
    /// RGB used by colour-bearing effects (static/breathing/reactive); ignored
    /// by spectrum/wave/off but always carried so the UI round-trips cleanly.
    pub color: Rgb,
    /// Brightness `0..=255` (the UI shows it as a percentage).
    pub brightness: u8,
}

/// Desired single-DPI state (X/Y). DPI stages are applied separately and are
/// not persisted in a profile in v1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DpiSettings {
    pub x: u16,
    pub y: u16,
}

/// A device's full desired settings. Every field is optional so a partial
/// update (e.g. only lighting from the lighting page) reuses the same planner.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceSettings {
    #[serde(default)]
    pub lighting: Option<LightingSettings>,
    #[serde(default)]
    pub dpi: Option<DpiSettings>,
    #[serde(default)]
    pub polling_hz: Option<u16>,
}

/// A named profile: per-device settings keyed by USB PID.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Profile {
    pub name: String,
    #[serde(default)]
    pub devices: HashMap<u16, DeviceSettings>,
}

/// One concrete device write the planner emits. Pure data: the executor in
/// `device_manager.rs` maps each variant to a `razer_hid::Device` call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlannedWrite {
    Lighting {
        led_id: u8,
        varstore: u8,
        effect: Effect,
        color: Rgb,
    },
    Brightness {
        led_id: u8,
        varstore: u8,
        value: u8,
    },
    Dpi {
        varstore: u8,
        x: u16,
        y: u16,
    },
    PollingRate {
        hz: u16,
    },
}

/// The LED id a device's lighting commands should target: the first declared
/// LED region, or the logo LED (`0x04`) as a sensible default.
pub fn led_id_for(def: &DeviceDef) -> u8 {
    def.led_regions.first().map(|r| r.id).unwrap_or(led_id::LOGO)
}

/// The varstore flag to use for a device: VARSTORE (persist to onboard flash)
/// when the registry marks the device `onboard`-capable, else NOSTORE
/// (volatile — profiles reapply it app-side on connect).
pub fn varstore_for(def: &DeviceDef) -> u8 {
    if def.capabilities.onboard {
        VARSTORE
    } else {
        NOSTORE
    }
}

/// Turn a device's desired [`DeviceSettings`] into the ordered list of writes
/// to send, gated by what the device's registry entry actually supports. Pure:
/// no I/O, fully unit-tested below with hand-built [`DeviceDef`]s.
///
/// Settings for a capability the device lacks (e.g. DPI on a keyboard) are
/// silently dropped rather than erroring, so one profile can safely cover a
/// mixed set of devices.
pub fn plan_device_settings(def: &DeviceDef, settings: &DeviceSettings) -> Vec<PlannedWrite> {
    let mut plan = Vec::new();
    let led = led_id_for(def);
    let varstore = varstore_for(def);

    if def.capabilities.lighting {
        if let Some(l) = settings.lighting {
            plan.push(PlannedWrite::Lighting {
                led_id: led,
                varstore,
                effect: l.effect,
                color: l.color,
            });
            plan.push(PlannedWrite::Brightness {
                led_id: led,
                varstore,
                value: l.brightness,
            });
        }
    }
    if def.capabilities.dpi {
        if let Some(d) = settings.dpi {
            plan.push(PlannedWrite::Dpi {
                varstore,
                x: d.x,
                y: d.y,
            });
        }
    }
    if def.capabilities.polling_rate {
        if let Some(hz) = settings.polling_hz {
            plan.push(PlannedWrite::PollingRate { hz });
        }
    }
    plan
}

/// Validate a user-supplied profile name so it maps to exactly one safe
/// filename (no path traversal, no separators). Returns the trimmed name on
/// success.
pub fn sanitize_name(name: &str) -> Option<String> {
    let trimmed = name.trim();
    if trimmed.is_empty() || trimmed.len() > 64 {
        return None;
    }
    let ok = trimmed
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == ' ' || c == '-' || c == '_');
    ok.then(|| trimmed.to_string())
}

/// JSON-file profile store rooted at the app config directory.
///
/// Layout: `<config>/profiles/<name>.json` for each profile, plus
/// `<config>/active-profile.txt` holding the last-applied profile name (so it
/// can be reapplied on next launch). All methods are non-panicking; corrupt or
/// unreadable files surface as `Err`/skipped rather than crashing the app.
pub struct ProfileStore {
    profiles_dir: PathBuf,
    active_path: PathBuf,
}

impl ProfileStore {
    /// Build a store under `config_dir` (typically `app.path().app_config_dir()`).
    pub fn new(config_dir: impl AsRef<Path>) -> Self {
        let root = config_dir.as_ref();
        Self {
            profiles_dir: root.join("profiles"),
            active_path: root.join("active-profile.txt"),
        }
    }

    fn path_for(&self, name: &str) -> Option<PathBuf> {
        let safe = sanitize_name(name)?;
        Some(self.profiles_dir.join(format!("{safe}.json")))
    }

    /// Names of all valid profiles on disk, sorted. A file that fails to parse
    /// is logged and skipped, never fatal.
    pub fn list(&self) -> Vec<String> {
        let mut names = Vec::new();
        let Ok(entries) = std::fs::read_dir(&self.profiles_dir) else {
            return names; // dir doesn't exist yet -> no profiles
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            match std::fs::read_to_string(&path) {
                Ok(src) => match serde_json::from_str::<Profile>(&src) {
                    Ok(_) => names.push(stem.to_string()),
                    Err(err) => {
                        eprintln!("opsrzr: skipping corrupt profile {}: {err}", path.display())
                    }
                },
                Err(err) => {
                    eprintln!("opsrzr: skipping unreadable profile {}: {err}", path.display())
                }
            }
        }
        names.sort();
        names
    }

    /// Load a single profile by name.
    pub fn load(&self, name: &str) -> Result<Profile, String> {
        let path = self
            .path_for(name)
            .ok_or_else(|| format!("invalid profile name: {name:?}"))?;
        let src = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
        serde_json::from_str(&src).map_err(|e| format!("corrupt profile {name:?}: {e}"))
    }

    /// Write a profile to disk (pretty-printed), creating the directory on
    /// demand. The profile's own `name` is used for the filename.
    pub fn save(&self, profile: &Profile) -> Result<(), String> {
        let path = self
            .path_for(&profile.name)
            .ok_or_else(|| format!("invalid profile name: {:?}", profile.name))?;
        std::fs::create_dir_all(&self.profiles_dir).map_err(|e| e.to_string())?;
        let json = serde_json::to_string_pretty(profile).map_err(|e| e.to_string())?;
        std::fs::write(&path, json).map_err(|e| e.to_string())
    }

    /// Delete a profile. Missing file is treated as success (idempotent).
    pub fn delete(&self, name: &str) -> Result<(), String> {
        let path = self
            .path_for(name)
            .ok_or_else(|| format!("invalid profile name: {name:?}"))?;
        match std::fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(err) => Err(err.to_string()),
        }
    }

    /// The last-applied profile name, if one is recorded and still valid.
    pub fn get_active(&self) -> Option<String> {
        let raw = std::fs::read_to_string(&self.active_path).ok()?;
        sanitize_name(&raw)
    }

    /// Record (or clear, with `None`) the last-applied profile name.
    pub fn set_active(&self, name: Option<&str>) -> Result<(), String> {
        match name {
            Some(n) => {
                let safe =
                    sanitize_name(n).ok_or_else(|| format!("invalid profile name: {n:?}"))?;
                if let Some(parent) = self.active_path.parent() {
                    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                }
                std::fs::write(&self.active_path, safe).map_err(|e| e.to_string())
            }
            None => match std::fs::remove_file(&self.active_path) {
                Ok(()) => Ok(()),
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(err) => Err(err.to_string()),
            },
        }
    }
}

/// Tauri-managed profile state: the on-disk [`ProfileStore`] plus the
/// in-memory "active profile" name (mirrors the on-disk `active-profile.txt`
/// pointer so apply-on-connect doesn't hit the disk every hotplug tick).
pub struct ProfileState {
    pub store: ProfileStore,
    active: std::sync::Mutex<Option<String>>,
}

impl ProfileState {
    /// Build from a store, seeding the active name from disk.
    pub fn new(store: ProfileStore) -> Self {
        let active = store.get_active();
        Self {
            store,
            active: std::sync::Mutex::new(active),
        }
    }

    fn lock_active(&self) -> std::sync::MutexGuard<'_, Option<String>> {
        match self.active.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    /// The currently active profile name, if any.
    pub fn active(&self) -> Option<String> {
        self.lock_active().clone()
    }

    /// Set (or clear) the active profile, persisting the pointer to disk.
    pub fn set_active(&self, name: Option<String>) -> Result<(), String> {
        self.store.set_active(name.as_deref())?;
        *self.lock_active() = name;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use razer_hid::Registry;

    fn lighting_mouse(onboard: bool) -> DeviceDef {
        let src = format!(
            r#"
            name = "Test Mouse"
            usb_pid = "0x008C"
            device_type = "mouse"
            dpi_min = 100
            dpi_max = 8500

            [capabilities]
            lighting = true
            dpi = true
            polling_rate = true
            battery = false
            onboard = {onboard}

            [[led_regions]]
            name = "logo"
            id = 0x04
            "#
        );
        Registry::parse_def(&src).expect("parse")
    }

    fn keyboard_no_dpi() -> DeviceDef {
        Registry::parse_def(
            r#"
            name = "Test Keyboard"
            usb_pid = "0x024E"
            device_type = "keyboard"

            [capabilities]
            lighting = true
            dpi = false
            polling_rate = true
            battery = false
            "#,
        )
        .expect("parse")
    }

    fn full_settings() -> DeviceSettings {
        DeviceSettings {
            lighting: Some(LightingSettings {
                effect: Effect::Static,
                color: [10, 20, 30],
                brightness: 200,
            }),
            dpi: Some(DpiSettings { x: 1600, y: 1600 }),
            polling_hz: Some(1000),
        }
    }

    #[test]
    fn plans_lighting_dpi_and_polling_for_full_capability_mouse() {
        let def = lighting_mouse(false);
        let plan = plan_device_settings(&def, &full_settings());
        assert_eq!(
            plan,
            vec![
                PlannedWrite::Lighting {
                    led_id: 0x04,
                    varstore: NOSTORE,
                    effect: Effect::Static,
                    color: [10, 20, 30],
                },
                PlannedWrite::Brightness {
                    led_id: 0x04,
                    varstore: NOSTORE,
                    value: 200,
                },
                PlannedWrite::Dpi {
                    varstore: NOSTORE,
                    x: 1600,
                    y: 1600,
                },
                PlannedWrite::PollingRate { hz: 1000 },
            ]
        );
    }

    #[test]
    fn onboard_device_uses_varstore() {
        let def = lighting_mouse(true);
        let plan = plan_device_settings(&def, &full_settings());
        // Every persisting write carries VARSTORE (polling has no varstore arg).
        for w in &plan {
            match w {
                PlannedWrite::Lighting { varstore, .. }
                | PlannedWrite::Brightness { varstore, .. }
                | PlannedWrite::Dpi { varstore, .. } => assert_eq!(*varstore, VARSTORE),
                PlannedWrite::PollingRate { .. } => {}
            }
        }
    }

    #[test]
    fn drops_dpi_for_device_without_dpi_capability() {
        let def = keyboard_no_dpi();
        let plan = plan_device_settings(&def, &full_settings());
        assert!(
            !plan.iter().any(|w| matches!(w, PlannedWrite::Dpi { .. })),
            "keyboard has no DPI capability; DPI write must be dropped"
        );
        // But lighting + polling still planned.
        assert!(plan.iter().any(|w| matches!(w, PlannedWrite::Lighting { .. })));
        assert!(plan.iter().any(|w| matches!(w, PlannedWrite::PollingRate { .. })));
    }

    #[test]
    fn led_id_falls_back_to_logo_without_regions() {
        let def = Registry::parse_def(
            r#"
            name = "No Regions"
            usb_pid = "0x0001"
            device_type = "mouse"
            [capabilities]
            lighting = true
            "#,
        )
        .expect("parse");
        assert_eq!(led_id_for(&def), led_id::LOGO);
    }

    #[test]
    fn empty_settings_plans_nothing() {
        let def = lighting_mouse(false);
        assert!(plan_device_settings(&def, &DeviceSettings::default()).is_empty());
    }

    #[test]
    fn effect_off_serializes_as_none() {
        let json = serde_json::to_string(&Effect::Off).expect("serialize");
        assert_eq!(json, "\"none\"");
        let back: Effect = serde_json::from_str("\"none\"").expect("deserialize");
        assert_eq!(back, Effect::Off);
    }

    #[test]
    fn profile_json_round_trips() {
        let mut devices = HashMap::new();
        devices.insert(0x008Cu16, full_settings());
        let profile = Profile {
            name: "Gaming".to_string(),
            devices,
        };
        let json = serde_json::to_string_pretty(&profile).expect("serialize");
        let back: Profile = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(profile, back);
        // PID map key survives as the numeric key it was inserted under.
        assert!(back.devices.contains_key(&0x008C));
    }

    #[test]
    fn sanitize_name_accepts_plain_names_and_rejects_traversal() {
        assert_eq!(sanitize_name("My Profile-1"), Some("My Profile-1".to_string()));
        assert_eq!(sanitize_name("  spaced  "), Some("spaced".to_string()));
        assert_eq!(sanitize_name("../evil"), None);
        assert_eq!(sanitize_name("a/b"), None);
        assert_eq!(sanitize_name("a\\b"), None);
        assert_eq!(sanitize_name(""), None);
        assert_eq!(sanitize_name("   "), None);
        assert_eq!(sanitize_name(&"x".repeat(65)), None);
    }

    #[test]
    fn store_save_load_list_delete_round_trip() {
        let dir = std::env::temp_dir().join(format!("opsrzr-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = ProfileStore::new(&dir);

        assert!(store.list().is_empty());

        let mut devices = HashMap::new();
        devices.insert(0x008Cu16, full_settings());
        let profile = Profile {
            name: "Work".to_string(),
            devices,
        };
        store.save(&profile).expect("save");

        assert_eq!(store.list(), vec!["Work".to_string()]);
        assert_eq!(store.load("Work").expect("load"), profile);

        store.set_active(Some("Work")).expect("set active");
        assert_eq!(store.get_active(), Some("Work".to_string()));

        store.delete("Work").expect("delete");
        assert!(store.list().is_empty());
        // Idempotent delete.
        store.delete("Work").expect("delete missing is ok");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn store_skips_corrupt_profile_in_list() {
        let dir = std::env::temp_dir().join(format!("opsrzr-corrupt-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("profiles")).expect("mkdir");
        std::fs::write(dir.join("profiles").join("bad.json"), "{ not json").expect("write");
        let store = ProfileStore::new(&dir);
        assert!(store.list().is_empty(), "corrupt file must be skipped, not listed");
        assert!(store.load("bad").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
