//! Device info / mode commands (class `0x00`): serial number, firmware
//! version, and driver/normal device mode.
//!
//! Byte layouts ported from OpenRazer `driver/razerchromacommon.c`
//! `razer_chroma_standard_get_serial()` / `_get_firmware_version()` /
//! `_set_device_mode()` / `_get_device_mode()` (GPL-2.0).

use crate::transport::{Device, TransportError};
use crate::RazerReport;

/// Device-info command class.
// source: razerchromacommon.c `get_razer_report(0x00, ...)`.
pub const CLASS: u8 = 0x00;

/// Get serial number (22 ASCII bytes).
// source: razerchromacommon.c `razer_chroma_standard_get_serial()` ->
// `get_razer_report(0x00, 0x82, 0x16)` (0x16 = 22).
pub const CMD_GET_SERIAL: u8 = 0x82;
/// Serial number length in bytes.
pub const SERIAL_LEN: usize = 0x16;

/// Get firmware version (response `arguments[0].arguments[1]` = major.minor).
// source: razerchromacommon.c `razer_chroma_standard_get_firmware_version()`
// -> `get_razer_report(0x00, 0x81, 0x02)`.
pub const CMD_GET_FIRMWARE: u8 = 0x81;

/// Set device mode (`arg0` = mode, `arg1` = param, always `0x00` upstream).
// source: razerchromacommon.c `razer_chroma_standard_set_device_mode()` ->
// `get_razer_report(0x00, 0x04, 0x02)`.
pub const CMD_SET_MODE: u8 = 0x04;
/// Get device mode.
// source: razerchromacommon.c `razer_chroma_standard_get_device_mode()` ->
// `get_razer_report(0x00, 0x84, 0x02)`.
pub const CMD_GET_MODE: u8 = 0x84;

/// Device-mode values.
// source: razerchromacommon.c razer_chroma_standard_set_device_mode() — 0x00 normal, 0x03 driver
pub mod mode {
    pub const NORMAL: u8 = 0x00;
    pub const DRIVER: u8 = 0x03;
}

/// Build a get-serial-number request.
pub fn build_get_serial(transaction_id: u8) -> RazerReport {
    RazerReport::new(transaction_id, CLASS, CMD_GET_SERIAL, &[0u8; SERIAL_LEN])
}

/// Parse the serial number out of a get-serial response: the first
/// `SERIAL_LEN` argument bytes as ASCII, trimmed of trailing NUL padding.
pub fn parse_serial(report: &RazerReport) -> String {
    let raw = &report.arguments[..SERIAL_LEN];
    let end = raw.iter().position(|&b| b == 0).unwrap_or(raw.len());
    String::from_utf8_lossy(&raw[..end]).into_owned()
}

/// Build a get-firmware-version request.
pub fn build_get_firmware_version(transaction_id: u8) -> RazerReport {
    RazerReport::new(transaction_id, CLASS, CMD_GET_FIRMWARE, &[0u8, 0u8])
}

/// Parse `(major, minor)` out of a get-firmware-version response.
pub fn parse_firmware_version(report: &RazerReport) -> (u8, u8) {
    (report.arguments[0], report.arguments[1])
}

/// Build a set-device-mode request. Upstream coerces any `mode` other than
/// [`mode::NORMAL`] / [`mode::DRIVER`] to [`mode::NORMAL`], and always sends
/// `param = 0x00`; we replicate that exactly rather than erroring, to match
/// device behavior byte-for-byte.
// source: razerchromacommon.c `razer_chroma_standard_set_device_mode()`:
// `if (mode != 0x00 && mode != 0x03) mode = 0x00; param = 0x00;`
pub fn build_set_device_mode(transaction_id: u8, requested_mode: u8) -> RazerReport {
    let coerced_mode = if requested_mode == mode::NORMAL || requested_mode == mode::DRIVER {
        requested_mode
    } else {
        mode::NORMAL
    };
    RazerReport::new(
        transaction_id,
        CLASS,
        CMD_SET_MODE,
        &[coerced_mode, 0x00],
    )
}

/// Build a get-device-mode request.
pub fn build_get_device_mode(transaction_id: u8) -> RazerReport {
    RazerReport::new(transaction_id, CLASS, CMD_GET_MODE, &[0u8, 0u8])
}

/// Parse `(mode, param)` out of a get-device-mode response.
pub fn parse_device_mode(report: &RazerReport) -> (u8, u8) {
    (report.arguments[0], report.arguments[1])
}

impl Device {
    /// Read the device serial number.
    pub fn get_serial(&self) -> Result<String, TransportError> {
        let response = self.send_payload(CLASS, CMD_GET_SERIAL, &[0u8; SERIAL_LEN])?;
        Ok(parse_serial(&response))
    }

    /// Read the firmware version as `(major, minor)`.
    pub fn get_firmware_version(&self) -> Result<(u8, u8), TransportError> {
        let response = self.send_payload(CLASS, CMD_GET_FIRMWARE, &[0u8, 0u8])?;
        Ok(parse_firmware_version(&response))
    }

    /// Set the device mode ([`mode::NORMAL`] or [`mode::DRIVER`]; any other
    /// value is coerced to [`mode::NORMAL`], matching upstream).
    pub fn set_device_mode(&self, requested_mode: u8) -> Result<(), TransportError> {
        let coerced_mode = if requested_mode == mode::NORMAL || requested_mode == mode::DRIVER {
            requested_mode
        } else {
            mode::NORMAL
        };
        self.send_payload(CLASS, CMD_SET_MODE, &[coerced_mode, 0x00])?;
        Ok(())
    }

    /// Read `(mode, param)`.
    pub fn get_device_mode(&self) -> Result<(u8, u8), TransportError> {
        let response = self.send_payload(CLASS, CMD_GET_MODE, &[0u8, 0u8])?;
        Ok(parse_device_mode(&response))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_serial_bytes() {
        let r = build_get_serial(0x3F);
        assert_eq!(r.command_class, CLASS);
        assert_eq!(r.command_id, CMD_GET_SERIAL);
        assert_eq!(r.data_size, SERIAL_LEN as u8);
        assert!(r.arguments[..SERIAL_LEN].iter().all(|&b| b == 0));
        assert!(r.crc_valid());
    }

    #[test]
    fn parse_serial_trims_nul_padding() {
        let mut report = build_get_serial(0x3F);
        let serial = b"PM1234567890AB";
        report.arguments[..serial.len()].copy_from_slice(serial);
        assert_eq!(parse_serial(&report), "PM1234567890AB");
    }

    #[test]
    fn parse_serial_full_length_no_nul() {
        let mut report = build_get_serial(0x3F);
        let serial = [b'A'; SERIAL_LEN];
        report.arguments[..SERIAL_LEN].copy_from_slice(&serial);
        assert_eq!(parse_serial(&report), "A".repeat(SERIAL_LEN));
    }

    #[test]
    fn get_firmware_version_bytes_and_parse() {
        let mut r = build_get_firmware_version(0x3F);
        assert_eq!(r.command_id, CMD_GET_FIRMWARE);
        assert_eq!(r.data_size, 2);
        r.arguments[0] = 1;
        r.arguments[1] = 5;
        assert_eq!(parse_firmware_version(&r), (1, 5));
    }

    #[test]
    fn set_device_mode_bytes() {
        let normal = build_set_device_mode(0x3F, mode::NORMAL);
        assert_eq!(normal.command_class, CLASS);
        assert_eq!(normal.command_id, CMD_SET_MODE);
        assert_eq!(normal.data_size, 2);
        assert_eq!(&normal.arguments[..2], &[0x00, 0x00]);

        let driver = build_set_device_mode(0x3F, mode::DRIVER);
        assert_eq!(&driver.arguments[..2], &[0x03, 0x00]);

        // Invalid mode coerces to NORMAL, matching upstream.
        let invalid = build_set_device_mode(0x3F, 0x42);
        assert_eq!(&invalid.arguments[..2], &[0x00, 0x00]);
    }

    #[test]
    fn get_device_mode_bytes_and_parse() {
        let mut r = build_get_device_mode(0x3F);
        assert_eq!(r.command_id, CMD_GET_MODE);
        assert_eq!(r.data_size, 2);
        r.arguments[0] = mode::DRIVER;
        r.arguments[1] = 0x00;
        assert_eq!(parse_device_mode(&r), (0x03, 0x00));
    }
}
