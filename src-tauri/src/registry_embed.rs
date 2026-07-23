//! Embeds the repository's `devices/*.toml` registry definitions into the
//! compiled binary at build time, so the installed app never depends on a
//! `devices/` folder existing next to the executable.
//!
//! `razer_hid::Registry::load_dir` remains available separately (unused by
//! default here) for a dev-time override that reads loose TOML files.

use razer_hid::Registry;

/// Build the registry from the definitions embedded at compile time.
///
/// Any single source failing to parse is logged and skipped rather than
/// panicking the app at startup — the remaining embedded devices still load.
/// `embedded_registry_parses_all_sources` below guards against this actually
/// happening for the checked-in `devices/*.toml` files.
pub fn embedded_registry() -> Registry {
    const SOURCES: &[&str] = &[
        include_str!("../../devices/basilisk-v3.toml"),
        include_str!("../../devices/blackwidow-v3.toml"),
        include_str!("../../devices/deathadder-v2-mini.toml"),
        include_str!("../../devices/deathadder-v2.toml"),
    ];

    let mut registry = Registry::new();
    for src in SOURCES {
        match Registry::parse_def(src) {
            Ok(def) => registry.push(def),
            Err(err) => {
                eprintln!("opsrzr: embedded device definition failed to parse: {err}");
            }
        }
    }
    registry
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_registry_parses_all_sources() {
        let reg = embedded_registry();
        assert_eq!(reg.devices().len(), 4, "one of the devices/*.toml sources failed to parse");
    }

    #[test]
    fn embedded_registry_includes_deathadder_v2_mini() {
        let reg = embedded_registry();
        let def = reg
            .find_by_pid(0x008C)
            .expect("DeathAdder V2 Mini (dev machine's attached device) must be embedded");
        assert_eq!(def.name, "Razer DeathAdder V2 Mini");
        assert!(!def.capabilities.battery, "wired device must not advertise battery");
    }

    #[test]
    fn embedded_registry_has_no_duplicate_pids() {
        let reg = embedded_registry();
        let mut pids: Vec<u16> = reg.devices().iter().map(|d| d.usb_pid).collect();
        let before = pids.len();
        pids.sort_unstable();
        pids.dedup();
        assert_eq!(pids.len(), before, "duplicate usb_pid across embedded devices/*.toml");
    }
}
