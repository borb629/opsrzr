//! Polling-rate commands (class `0x00`): the classic 3-rate command and the
//! HyperPolling v2 8-rate command.
//!
//! Byte layouts ported from OpenRazer `driver/razerchromacommon.c`
//! `razer_chroma_misc_set_polling_rate()` / `_get_polling_rate()` /
//! `_set_polling_rate2()` / `_get_polling_rate2()` (GPL-2.0).

use crate::transport::{Device, TransportError};
use crate::RazerReport;

/// Polling-rate command class.
// source: razerchromacommon.c `get_razer_report(0x00, ...)`.
pub const CLASS: u8 = 0x00;

/// Classic set-polling-rate command id.
// source: razerchromacommon.c `razer_chroma_misc_set_polling_rate()` ->
// `get_razer_report(0x00, 0x05, 0x01)`.
pub const CMD_SET: u8 = 0x05;
/// Classic get-polling-rate command id.
// source: razerchromacommon.c `razer_chroma_misc_get_polling_rate()` ->
// `get_razer_report(0x00, 0x85, 0x01)`.
pub const CMD_GET: u8 = 0x85;
/// HyperPolling v2 set-polling-rate command id.
// source: razerchromacommon.c `razer_chroma_misc_set_polling_rate2()` ->
// `get_razer_report(0x00, 0x40, 0x02)`.
pub const CMD_SET_V2: u8 = 0x40;
/// HyperPolling v2 get-polling-rate command id.
// source: razerchromacommon.c `razer_chroma_misc_get_polling_rate2()` ->
// `get_razer_report(0x00, 0xC0, 0x01)`.
pub const CMD_GET_V2: u8 = 0xC0;

/// Map a polling rate in Hz to the classic (3-rate) wire code. Unknown rates
/// fall back to 500 Hz, matching upstream's `default:` case.
// source: razerchromacommon.c `razer_chroma_misc_set_polling_rate()` switch.
pub fn code_v1(hz: u16) -> u8 {
    match hz {
        1000 => 0x01,
        500 => 0x02,
        125 => 0x08,
        _ => 0x02,
    }
}

/// Reverse of [`code_v1`]: map a wire code back to Hz, if recognized.
pub fn hz_from_code_v1(code: u8) -> Option<u16> {
    match code {
        0x01 => Some(1000),
        0x02 => Some(500),
        0x08 => Some(125),
        _ => None,
    }
}

/// Map a polling rate in Hz to the HyperPolling v2 (8-rate) wire code.
/// Unknown rates fall back to 500 Hz, matching upstream's `default:` case.
// source: razerchromacommon.c `razer_chroma_misc_set_polling_rate2()` switch.
pub fn code_v2(hz: u16) -> u8 {
    match hz {
        8000 => 0x01,
        4000 => 0x02,
        2000 => 0x04,
        1000 => 0x08,
        500 => 0x10,
        250 => 0x20,
        125 => 0x40,
        _ => 0x10,
    }
}

/// Reverse of [`code_v2`]: map a wire code back to Hz, if recognized.
pub fn hz_from_code_v2(code: u8) -> Option<u16> {
    match code {
        0x01 => Some(8000),
        0x02 => Some(4000),
        0x04 => Some(2000),
        0x08 => Some(1000),
        0x10 => Some(500),
        0x20 => Some(250),
        0x40 => Some(125),
        _ => None,
    }
}

/// Build a classic set-polling-rate request.
pub fn build_set_polling_rate(transaction_id: u8, hz: u16) -> RazerReport {
    RazerReport::new(transaction_id, CLASS, CMD_SET, &[code_v1(hz)])
}

/// Build a classic get-polling-rate request.
pub fn build_get_polling_rate(transaction_id: u8) -> RazerReport {
    RazerReport::new(transaction_id, CLASS, CMD_GET, &[0u8])
}

/// Parse the wire code out of a classic get-polling-rate response.
pub fn parse_polling_rate(report: &RazerReport) -> u8 {
    report.arguments[0]
}

/// Build a HyperPolling v2 set-polling-rate request. `argument` is an
/// opaque per-device selector byte (upstream: `report.arguments[0]`) used by
/// some devices to pick a wireless/wired polling profile; pass `0x00` when
/// the device doesn't distinguish.
// source: razerchromacommon.c `razer_chroma_misc_set_polling_rate2()`:
// arguments[0]=argument, [1]=rate code.
pub fn build_set_polling_rate_v2(transaction_id: u8, hz: u16, argument: u8) -> RazerReport {
    RazerReport::new(
        transaction_id,
        CLASS,
        CMD_SET_V2,
        &[argument, code_v2(hz)],
    )
}

/// Build a HyperPolling v2 get-polling-rate request.
pub fn build_get_polling_rate_v2(transaction_id: u8) -> RazerReport {
    RazerReport::new(transaction_id, CLASS, CMD_GET_V2, &[0u8])
}

/// Parse the wire code out of a HyperPolling v2 get-polling-rate response.
pub fn parse_polling_rate_v2(report: &RazerReport) -> u8 {
    report.arguments[0]
}

impl Device {
    /// Set the classic (3-rate) polling rate. Unrecognized `hz` values fall
    /// back to 500 Hz, matching upstream.
    pub fn set_polling_rate(&self, hz: u16) -> Result<(), TransportError> {
        self.send_payload(CLASS, CMD_SET, &[code_v1(hz)])?;
        Ok(())
    }

    /// Read back the classic polling rate in Hz, if the wire code is
    /// recognized.
    pub fn get_polling_rate(&self) -> Result<Option<u16>, TransportError> {
        let response = self.send_payload(CLASS, CMD_GET, &[0u8])?;
        Ok(hz_from_code_v1(parse_polling_rate(&response)))
    }

    /// Set the HyperPolling v2 (8-rate) polling rate.
    pub fn set_polling_rate_v2(&self, hz: u16, argument: u8) -> Result<(), TransportError> {
        self.send_payload(CLASS, CMD_SET_V2, &[argument, code_v2(hz)])?;
        Ok(())
    }

    /// Read back the HyperPolling v2 polling rate in Hz, if the wire code is
    /// recognized.
    pub fn get_polling_rate_v2(&self) -> Result<Option<u16>, TransportError> {
        let response = self.send_payload(CLASS, CMD_GET_V2, &[0u8])?;
        Ok(hz_from_code_v2(parse_polling_rate_v2(&response)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_polling_rate_bytes() {
        let r = build_set_polling_rate(0x3F, 1000);
        assert_eq!(r.command_class, CLASS);
        assert_eq!(r.command_id, CMD_SET);
        assert_eq!(r.data_size, 1);
        assert_eq!(r.arguments[0], 0x01);
        assert!(r.crc_valid());

        assert_eq!(build_set_polling_rate(0x3F, 500).arguments[0], 0x02);
        assert_eq!(build_set_polling_rate(0x3F, 125).arguments[0], 0x08);
        // Unknown rate falls back to 500 Hz's code, matching upstream default.
        assert_eq!(build_set_polling_rate(0x3F, 2222).arguments[0], 0x02);
    }

    #[test]
    fn get_polling_rate_bytes() {
        let r = build_get_polling_rate(0x3F);
        assert_eq!(r.command_id, CMD_GET);
        assert_eq!(r.data_size, 1);
        assert_eq!(r.arguments[0], 0);
    }

    #[test]
    fn parse_polling_rate_round_trip() {
        let mut report = build_get_polling_rate(0x3F);
        report.arguments[0] = 0x08;
        assert_eq!(parse_polling_rate(&report), 0x08);
        assert_eq!(hz_from_code_v1(0x08), Some(125));
        assert_eq!(hz_from_code_v1(0xEE), None);
    }

    #[test]
    fn set_polling_rate_v2_bytes() {
        let r = build_set_polling_rate_v2(0x3F, 8000, 0x00);
        assert_eq!(r.command_class, CLASS);
        assert_eq!(r.command_id, CMD_SET_V2);
        assert_eq!(r.data_size, 2);
        assert_eq!(&r.arguments[..2], &[0x00, 0x01]);

        assert_eq!(build_set_polling_rate_v2(0x3F, 125, 0x01).arguments[1], 0x40);
        // Unknown rate falls back to 500 Hz's v2 code.
        assert_eq!(build_set_polling_rate_v2(0x3F, 3, 0x00).arguments[1], 0x10);
    }

    #[test]
    fn get_polling_rate_v2_bytes() {
        let r = build_get_polling_rate_v2(0x3F);
        assert_eq!(r.command_id, CMD_GET_V2);
        assert_eq!(r.data_size, 1);
    }

    #[test]
    fn parse_polling_rate_v2_round_trip() {
        let mut report = build_get_polling_rate_v2(0x3F);
        report.arguments[0] = 0x04;
        assert_eq!(parse_polling_rate_v2(&report), 0x04);
        assert_eq!(hz_from_code_v2(0x04), Some(2000));
    }
}
