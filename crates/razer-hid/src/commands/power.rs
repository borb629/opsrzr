//! Battery / power commands (class `0x07`).
//!
//! Byte layouts ported from OpenRazer `driver/razerchromacommon.c`
//! `razer_chroma_misc_get_battery_level()` / `_get_charging_status()` /
//! `_set_low_battery_threshold()` / `_get_low_battery_threshold()` /
//! `_set_idle_time()` / `_get_idle_time()` (GPL-2.0).

use crate::transport::{Device, TransportError};
use crate::RazerReport;

/// Battery/power command class.
// source: razerchromacommon.c `get_razer_report(0x07, ...)`.
pub const CLASS: u8 = 0x07;

/// Get battery level (response `arguments[1]` = `0..=255`, scale by `100/255`).
// source: razerchromacommon.c `razer_chroma_misc_get_battery_level()` ->
// `get_razer_report(0x07, 0x80, 0x02)`.
pub const CMD_GET_LEVEL: u8 = 0x80;
/// Get charging status (response `arguments[1]` = `0`/`1`).
// source: razerchromacommon.c `razer_chroma_misc_get_charging_status()` ->
// `get_razer_report(0x07, 0x84, 0x02)`.
pub const CMD_GET_CHARGING: u8 = 0x84;
/// Set low-battery threshold (`0x0C..=0x3F`, roughly 5-25%).
// source: razerchromacommon.c `razer_chroma_misc_set_low_battery_threshold()`
// -> `get_razer_report(0x07, 0x01, 0x01)`.
pub const CMD_SET_LOW_BATTERY_THRESHOLD: u8 = 0x01;
/// Get low-battery threshold.
// source: razerchromacommon.c `razer_chroma_misc_get_low_battery_threshold()`
// -> `get_razer_report(0x07, 0x81, 0x01)`.
pub const CMD_GET_LOW_BATTERY_THRESHOLD: u8 = 0x81;
/// Set idle time in seconds (BE u16, clamped `60..=900`).
// source: razerchromacommon.c `razer_chroma_misc_set_idle_time()` ->
// `get_razer_report(0x07, 0x03, 0x02)`.
pub const CMD_SET_IDLE_TIME: u8 = 0x03;
/// Get idle time.
// source: razerchromacommon.c `razer_chroma_misc_get_idle_time()` ->
// `get_razer_report(0x07, 0x83, 0x02)`.
pub const CMD_GET_IDLE_TIME: u8 = 0x83;

/// Inclusive clamp range for the low-battery threshold (~5-25%).
// source: plans/01-opsrzr-v1.md Phase 0 command table — threshold range 0x0C–0x3F (≈5–25%), verified against OpenRazer razerchromacommon.c razer_chroma_misc_set_low_battery_threshold()
pub const LOW_BATTERY_THRESHOLD_MIN: u8 = 0x0C;
pub const LOW_BATTERY_THRESHOLD_MAX: u8 = 0x3F;

/// Inclusive clamp range for the idle timeout, in seconds.
// source: plans/01-opsrzr-v1.md Phase 0 command table — idle time 60–900 s BE u16, verified against OpenRazer razerchromacommon.c razer_chroma_misc_set_idle_time()
pub const IDLE_TIME_MIN: u16 = 60;
pub const IDLE_TIME_MAX: u16 = 900;

/// Build a get-battery-level request.
pub fn build_get_battery_level(transaction_id: u8) -> RazerReport {
    RazerReport::new(transaction_id, CLASS, CMD_GET_LEVEL, &[0u8, 0u8])
}

/// Parse the battery level (percent, `0..=100`) out of a get-battery-level
/// response. `arguments[1]` is `0..=255`; scaled by `100/255`.
// source: OpenRazer daemon `hardware/mouse.py` battery property scales by
// `* 100 / 255`.
pub fn parse_battery_level(report: &RazerReport) -> u8 {
    ((report.arguments[1] as u32 * 100) / 255) as u8
}

/// Build a get-charging-status request.
pub fn build_get_charging_status(transaction_id: u8) -> RazerReport {
    RazerReport::new(transaction_id, CLASS, CMD_GET_CHARGING, &[0u8, 0u8])
}

/// Parse the charging flag out of a get-charging-status response.
pub fn parse_charging_status(report: &RazerReport) -> bool {
    report.arguments[1] != 0
}

fn clamp_threshold(v: u8) -> u8 {
    v.clamp(LOW_BATTERY_THRESHOLD_MIN, LOW_BATTERY_THRESHOLD_MAX)
}

/// Build a set-low-battery-threshold request. `threshold` is clamped to
/// `0x0C..=0x3F`.
pub fn build_set_low_battery_threshold(transaction_id: u8, threshold: u8) -> RazerReport {
    RazerReport::new(
        transaction_id,
        CLASS,
        CMD_SET_LOW_BATTERY_THRESHOLD,
        &[clamp_threshold(threshold)],
    )
}

/// Build a get-low-battery-threshold request.
pub fn build_get_low_battery_threshold(transaction_id: u8) -> RazerReport {
    RazerReport::new(
        transaction_id,
        CLASS,
        CMD_GET_LOW_BATTERY_THRESHOLD,
        &[0u8],
    )
}

/// Parse the threshold byte out of a get-low-battery-threshold response.
pub fn parse_low_battery_threshold(report: &RazerReport) -> u8 {
    report.arguments[0]
}

fn clamp_idle(v: u16) -> u16 {
    v.clamp(IDLE_TIME_MIN, IDLE_TIME_MAX)
}

/// Build a set-idle-time request. `seconds` is clamped to `60..=900`.
pub fn build_set_idle_time(transaction_id: u8, seconds: u16) -> RazerReport {
    let secs = clamp_idle(seconds);
    RazerReport::new(
        transaction_id,
        CLASS,
        CMD_SET_IDLE_TIME,
        &[(secs >> 8) as u8, (secs & 0xFF) as u8],
    )
}

/// Build a get-idle-time request.
pub fn build_get_idle_time(transaction_id: u8) -> RazerReport {
    RazerReport::new(transaction_id, CLASS, CMD_GET_IDLE_TIME, &[0u8, 0u8])
}

/// Parse the idle time (seconds) out of a get-idle-time response.
pub fn parse_idle_time(report: &RazerReport) -> u16 {
    u16::from_be_bytes([report.arguments[0], report.arguments[1]])
}

impl Device {
    /// Read the battery level as a percentage (`0..=100`).
    pub fn get_battery_level(&self) -> Result<u8, TransportError> {
        let response = self.send_payload(CLASS, CMD_GET_LEVEL, &[0u8, 0u8])?;
        Ok(parse_battery_level(&response))
    }

    /// Read whether the device is currently charging.
    pub fn get_charging_status(&self) -> Result<bool, TransportError> {
        let response = self.send_payload(CLASS, CMD_GET_CHARGING, &[0u8, 0u8])?;
        Ok(parse_charging_status(&response))
    }

    /// Set the low-battery warning threshold. `threshold` is clamped to
    /// `0x0C..=0x3F`.
    pub fn set_low_battery_threshold(&self, threshold: u8) -> Result<(), TransportError> {
        self.send_payload(
            CLASS,
            CMD_SET_LOW_BATTERY_THRESHOLD,
            &[clamp_threshold(threshold)],
        )?;
        Ok(())
    }

    /// Read the low-battery warning threshold.
    pub fn get_low_battery_threshold(&self) -> Result<u8, TransportError> {
        let response = self.send_payload(CLASS, CMD_GET_LOW_BATTERY_THRESHOLD, &[0u8])?;
        Ok(parse_low_battery_threshold(&response))
    }

    /// Set the idle timeout in seconds. Clamped to `60..=900`.
    pub fn set_idle_time(&self, seconds: u16) -> Result<(), TransportError> {
        let secs = clamp_idle(seconds);
        self.send_payload(
            CLASS,
            CMD_SET_IDLE_TIME,
            &[(secs >> 8) as u8, (secs & 0xFF) as u8],
        )?;
        Ok(())
    }

    /// Read the idle timeout in seconds.
    pub fn get_idle_time(&self) -> Result<u16, TransportError> {
        let response = self.send_payload(CLASS, CMD_GET_IDLE_TIME, &[0u8, 0u8])?;
        Ok(parse_idle_time(&response))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_battery_level_bytes_and_parse() {
        let r = build_get_battery_level(0x3F);
        assert_eq!(r.command_class, CLASS);
        assert_eq!(r.command_id, CMD_GET_LEVEL);
        assert_eq!(r.data_size, 2);
        assert!(r.crc_valid());

        let mut response = r;
        response.arguments[1] = 255;
        assert_eq!(parse_battery_level(&response), 100);
        response.arguments[1] = 0;
        assert_eq!(parse_battery_level(&response), 0);
        response.arguments[1] = 128;
        assert_eq!(parse_battery_level(&response), 50); // 128*100/255 = 50 (integer)
    }

    #[test]
    fn get_charging_status_bytes_and_parse() {
        let mut r = build_get_charging_status(0x3F);
        assert_eq!(r.command_id, CMD_GET_CHARGING);
        assert_eq!(r.data_size, 2);
        r.arguments[1] = 1;
        assert!(parse_charging_status(&r));
        r.arguments[1] = 0;
        assert!(!parse_charging_status(&r));
    }

    #[test]
    fn low_battery_threshold_bytes_and_clamp() {
        let r = build_set_low_battery_threshold(0x3F, 0x20);
        assert_eq!(r.command_id, CMD_SET_LOW_BATTERY_THRESHOLD);
        assert_eq!(r.data_size, 1);
        assert_eq!(r.arguments[0], 0x20);

        assert_eq!(build_set_low_battery_threshold(0x3F, 0x00).arguments[0], 0x0C);
        assert_eq!(build_set_low_battery_threshold(0x3F, 0xFF).arguments[0], 0x3F);

        let get = build_get_low_battery_threshold(0x3F);
        assert_eq!(get.command_id, CMD_GET_LOW_BATTERY_THRESHOLD);
        assert_eq!(get.data_size, 1);
    }

    #[test]
    fn idle_time_bytes_and_clamp() {
        let r = build_set_idle_time(0x3F, 300);
        assert_eq!(r.command_id, CMD_SET_IDLE_TIME);
        assert_eq!(r.data_size, 2);
        assert_eq!(&r.arguments[..2], &[0x01, 0x2C]); // 300 = 0x012C

        // Below-min clamps to 60 = 0x003C.
        assert_eq!(&build_set_idle_time(0x3F, 0).arguments[..2], &[0x00, 0x3C]);
        // Above-max clamps to 900 = 0x0384.
        assert_eq!(
            &build_set_idle_time(0x3F, 5000).arguments[..2],
            &[0x03, 0x84]
        );

        let mut get = build_get_idle_time(0x3F);
        assert_eq!(get.command_id, CMD_GET_IDLE_TIME);
        get.arguments[0] = 0x01;
        get.arguments[1] = 0x2C;
        assert_eq!(parse_idle_time(&get), 300);
    }
}
