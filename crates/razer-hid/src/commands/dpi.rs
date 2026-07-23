//! DPI commands (class `0x04`): direct X/Y DPI and up-to-5-stage DPI presets.
//!
//! Byte layouts are ported from OpenRazer `driver/razerchromacommon.c`
//! `razer_chroma_misc_set_dpi_xy()` / `_get_dpi_xy()` /
//! `_set_dpi_stages()` / `_get_dpi_stages()` (GPL-2.0).

use crate::transport::{Device, TransportError};
use crate::RazerReport;

/// DPI command class.
// source: razerchromacommon.c `get_razer_report(0x04, ...)`.
pub const CLASS: u8 = 0x04;

/// Set direct X/Y DPI.
// source: razerchromacommon.c `razer_chroma_misc_set_dpi_xy()` ->
// `get_razer_report(0x04, 0x05, 0x07)`.
pub const CMD_SET_XY: u8 = 0x05;
/// Get direct X/Y DPI.
// source: razerchromacommon.c `razer_chroma_misc_get_dpi_xy()` ->
// `get_razer_report(0x04, 0x85, 0x07)`.
pub const CMD_GET_XY: u8 = 0x85;
/// Set the DPI stage table.
// source: razerchromacommon.c `razer_chroma_misc_set_dpi_stages()` ->
// `get_razer_report(0x04, 0x06, 0x26)`.
pub const CMD_SET_STAGES: u8 = 0x06;
/// Get the DPI stage table.
// source: razerchromacommon.c `razer_chroma_misc_get_dpi_stages()` ->
// `get_razer_report(0x04, 0x86, 0x26)`.
pub const CMD_GET_STAGES: u8 = 0x86;

/// Data size (bytes) of the DPI-stages command payload: 3-byte header + up to
/// 5 stages of 7 bytes each.
// source: razerchromacommon.c `get_razer_report(0x04, 0x06/0x86, 0x26)` (0x26 = 38).
pub const STAGES_DATA_SIZE: usize = 0x26;
/// Maximum number of DPI stages a device can hold.
pub const MAX_STAGES: usize = 5;

/// Inclusive DPI clamp range shared by every DPI command.
// source: razerchromacommon.c `clamp(dpi_x, 100, 45000)` / `clamp(dpi_y, 100, 45000)`.
pub const DPI_MIN: u16 = 100;
pub const DPI_MAX: u16 = 45000;

/// Errors building a DPI-stages command.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum DpiError {
    #[error("dpi stages: {count} stages exceeds max {max}")]
    TooManyStages { count: usize, max: usize },
}

fn clamp_dpi(v: u16) -> u16 {
    v.clamp(DPI_MIN, DPI_MAX)
}

// source: razerchromacommon.c `razer_chroma_misc_set_dpi_xy()`:
// arguments[0]=varstore, [1]=dpi_x hi, [2]=dpi_x lo, [3]=dpi_y hi, [4]=dpi_y lo,
// [5..7]=0.
fn args_set_xy(varstore: u8, dpi_x: u16, dpi_y: u16) -> [u8; 7] {
    let x = clamp_dpi(dpi_x);
    let y = clamp_dpi(dpi_y);
    [
        varstore,
        (x >> 8) as u8,
        (x & 0xFF) as u8,
        (y >> 8) as u8,
        (y & 0xFF) as u8,
        0,
        0,
    ]
}

/// Build a set-DPI request. `dpi_x`/`dpi_y` are clamped to `100..=45000`.
pub fn build_set_dpi_xy(transaction_id: u8, varstore: u8, dpi_x: u16, dpi_y: u16) -> RazerReport {
    RazerReport::new(
        transaction_id,
        CLASS,
        CMD_SET_XY,
        &args_set_xy(varstore, dpi_x, dpi_y),
    )
}

fn args_get_xy(varstore: u8) -> [u8; 7] {
    [varstore, 0, 0, 0, 0, 0, 0]
}

/// Build a get-DPI request.
pub fn build_get_dpi_xy(transaction_id: u8, varstore: u8) -> RazerReport {
    RazerReport::new(transaction_id, CLASS, CMD_GET_XY, &args_get_xy(varstore))
}

/// Parse `(dpi_x, dpi_y)` out of a get-DPI response. Mirrors the request
/// layout: `arguments[1..3]` = X (big-endian), `arguments[3..5]` = Y
/// (big-endian).
pub fn parse_dpi_xy(report: &RazerReport) -> (u16, u16) {
    let x = u16::from_be_bytes([report.arguments[1], report.arguments[2]]);
    let y = u16::from_be_bytes([report.arguments[3], report.arguments[4]]);
    (x, y)
}

/// One DPI stage: index (`0`-based), X, Y.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DpiStage {
    pub index: u8,
    pub dpi_x: u16,
    pub dpi_y: u16,
}

// source: razerchromacommon.c `razer_chroma_misc_set_dpi_stages()`:
// arguments[0]=varstore, [1]=active_stage, [2]=count, then per stage:
// index, x hi, x lo, y hi, y lo, 0, 0 (7 bytes each), up to 5 stages.
fn args_set_stages(varstore: u8, active_stage: u8, stages: &[DpiStage]) -> [u8; STAGES_DATA_SIZE] {
    let mut args = [0u8; STAGES_DATA_SIZE];
    args[0] = varstore;
    args[1] = active_stage;
    args[2] = stages.len() as u8;
    let mut offset = 3;
    for stage in stages {
        let x = clamp_dpi(stage.dpi_x);
        let y = clamp_dpi(stage.dpi_y);
        args[offset] = stage.index;
        args[offset + 1] = (x >> 8) as u8;
        args[offset + 2] = (x & 0xFF) as u8;
        args[offset + 3] = (y >> 8) as u8;
        args[offset + 4] = (y & 0xFF) as u8;
        args[offset + 5] = 0;
        args[offset + 6] = 0;
        offset += 7;
    }
    args
}

/// Build a set-DPI-stages request. `stages` must hold at most
/// [`MAX_STAGES`] entries.
pub fn build_set_dpi_stages(
    transaction_id: u8,
    varstore: u8,
    active_stage: u8,
    stages: &[DpiStage],
) -> Result<RazerReport, DpiError> {
    if stages.len() > MAX_STAGES {
        return Err(DpiError::TooManyStages {
            count: stages.len(),
            max: MAX_STAGES,
        });
    }
    Ok(RazerReport::new(
        transaction_id,
        CLASS,
        CMD_SET_STAGES,
        &args_set_stages(varstore, active_stage, stages),
    ))
}

fn args_get_stages(varstore: u8) -> [u8; STAGES_DATA_SIZE] {
    let mut args = [0u8; STAGES_DATA_SIZE];
    args[0] = varstore;
    args
}

/// Build a get-DPI-stages request.
pub fn build_get_dpi_stages(transaction_id: u8, varstore: u8) -> RazerReport {
    RazerReport::new(
        transaction_id,
        CLASS,
        CMD_GET_STAGES,
        &args_get_stages(varstore),
    )
}

/// Parse `(active_stage, stages)` out of a get-DPI-stages response.
pub fn parse_dpi_stages(report: &RazerReport) -> (u8, Vec<DpiStage>) {
    let active_stage = report.arguments[1];
    let count = (report.arguments[2] as usize).min(MAX_STAGES);
    let mut stages = Vec::with_capacity(count);
    let mut offset = 3;
    for _ in 0..count {
        let index = report.arguments[offset];
        let x = u16::from_be_bytes([report.arguments[offset + 1], report.arguments[offset + 2]]);
        let y = u16::from_be_bytes([report.arguments[offset + 3], report.arguments[offset + 4]]);
        stages.push(DpiStage {
            index,
            dpi_x: x,
            dpi_y: y,
        });
        offset += 7;
    }
    (active_stage, stages)
}

impl Device {
    /// Set direct X/Y DPI. `dpi_x`/`dpi_y` are clamped to `100..=45000`.
    pub fn set_dpi(&self, varstore: u8, dpi_x: u16, dpi_y: u16) -> Result<(), TransportError> {
        self.send_payload(CLASS, CMD_SET_XY, &args_set_xy(varstore, dpi_x, dpi_y))?;
        Ok(())
    }

    /// Read back direct X/Y DPI.
    pub fn get_dpi(&self, varstore: u8) -> Result<(u16, u16), TransportError> {
        let response = self.send_payload(CLASS, CMD_GET_XY, &args_get_xy(varstore))?;
        Ok(parse_dpi_xy(&response))
    }

    /// Set the DPI stage table (at most [`MAX_STAGES`] entries).
    pub fn set_dpi_stages(
        &self,
        varstore: u8,
        active_stage: u8,
        stages: &[DpiStage],
    ) -> Result<(), DpiStagesError> {
        if stages.len() > MAX_STAGES {
            return Err(DpiStagesError::TooManyStages {
                count: stages.len(),
                max: MAX_STAGES,
            });
        }
        self.send_payload(
            CLASS,
            CMD_SET_STAGES,
            &args_set_stages(varstore, active_stage, stages),
        )?;
        Ok(())
    }

    /// Read back the DPI stage table.
    pub fn get_dpi_stages(&self, varstore: u8) -> Result<(u8, Vec<DpiStage>), TransportError> {
        let response = self.send_payload(CLASS, CMD_GET_STAGES, &args_get_stages(varstore))?;
        Ok(parse_dpi_stages(&response))
    }
}

/// Errors from [`Device::set_dpi_stages`]: either the stage count is invalid,
/// or the underlying transport failed.
#[derive(Debug, thiserror::Error)]
pub enum DpiStagesError {
    #[error("dpi stages: {count} stages exceeds max {max}")]
    TooManyStages { count: usize, max: usize },
    #[error(transparent)]
    Transport(#[from] TransportError),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_dpi_xy_bytes_and_clamp() {
        let r = build_set_dpi_xy(0x3F, 0x01, 1600, 1600);
        assert_eq!(r.command_class, CLASS);
        assert_eq!(r.command_id, CMD_SET_XY);
        assert_eq!(r.data_size, 7);
        // 1600 = 0x0640
        assert_eq!(&r.arguments[..7], &[0x01, 0x06, 0x40, 0x06, 0x40, 0, 0]);
        assert!(r.crc_valid());

        // Below-min clamps up to 100 = 0x0064.
        let low = build_set_dpi_xy(0x3F, 0x01, 1, 1);
        assert_eq!(&low.arguments[..5], &[0x01, 0x00, 0x64, 0x00, 0x64]);

        // Above-max clamps down to 45000 = 0xAFC8.
        let high = build_set_dpi_xy(0x3F, 0x01, 60000, 60000);
        assert_eq!(&high.arguments[..5], &[0x01, 0xAF, 0xC8, 0xAF, 0xC8]);
    }

    #[test]
    fn get_dpi_xy_bytes() {
        let r = build_get_dpi_xy(0x3F, 0x01);
        assert_eq!(r.command_id, CMD_GET_XY);
        assert_eq!(r.data_size, 7);
        assert_eq!(&r.arguments[..7], &[0x01, 0, 0, 0, 0, 0, 0]);
    }

    #[test]
    fn parse_dpi_xy_round_trip() {
        let mut report = build_get_dpi_xy(0x3F, 0x01);
        report.arguments[1] = 0x06;
        report.arguments[2] = 0x40;
        report.arguments[3] = 0x03;
        report.arguments[4] = 0x20;
        assert_eq!(parse_dpi_xy(&report), (1600, 800));
    }

    #[test]
    fn set_dpi_stages_bytes() {
        let stages = [
            DpiStage {
                index: 0,
                dpi_x: 800,
                dpi_y: 800,
            },
            DpiStage {
                index: 1,
                dpi_x: 1600,
                dpi_y: 1600,
            },
        ];
        let r = build_set_dpi_stages(0x3F, 0x01, 0x01, &stages).expect("valid stage count");
        assert_eq!(r.command_class, CLASS);
        assert_eq!(r.command_id, CMD_SET_STAGES);
        assert_eq!(r.data_size, STAGES_DATA_SIZE as u8);
        // header: varstore, active_stage, count
        assert_eq!(&r.arguments[..3], &[0x01, 0x01, 0x02]);
        // stage 0: index 0, 800=0x0320, 800=0x0320, reserved 0,0
        assert_eq!(
            &r.arguments[3..10],
            &[0x00, 0x03, 0x20, 0x03, 0x20, 0x00, 0x00]
        );
        // stage 1: index 1, 1600=0x0640
        assert_eq!(
            &r.arguments[10..17],
            &[0x01, 0x06, 0x40, 0x06, 0x40, 0x00, 0x00]
        );
    }

    #[test]
    fn set_dpi_stages_rejects_too_many() {
        let stages = [DpiStage {
            index: 0,
            dpi_x: 800,
            dpi_y: 800,
        }; 6];
        let err = build_set_dpi_stages(0x3F, 0x01, 0, &stages).unwrap_err();
        assert_eq!(err, DpiError::TooManyStages { count: 6, max: 5 });
    }

    #[test]
    fn get_dpi_stages_bytes() {
        let r = build_get_dpi_stages(0x3F, 0x01);
        assert_eq!(r.command_id, CMD_GET_STAGES);
        assert_eq!(r.data_size, STAGES_DATA_SIZE as u8);
        assert_eq!(r.arguments[0], 0x01);
    }

    #[test]
    fn parse_dpi_stages_round_trip() {
        let mut report = build_get_dpi_stages(0x3F, 0x01);
        report.arguments[1] = 0x02; // active stage
        report.arguments[2] = 0x02; // count
        report.arguments[3..10].copy_from_slice(&[0x00, 0x03, 0x20, 0x03, 0x20, 0x00, 0x00]);
        report.arguments[10..17].copy_from_slice(&[0x01, 0x06, 0x40, 0x06, 0x40, 0x00, 0x00]);
        let (active, stages) = parse_dpi_stages(&report);
        assert_eq!(active, 0x02);
        assert_eq!(stages.len(), 2);
        assert_eq!(
            stages[0],
            DpiStage {
                index: 0,
                dpi_x: 800,
                dpi_y: 800
            }
        );
        assert_eq!(
            stages[1],
            DpiStage {
                index: 1,
                dpi_x: 1600,
                dpi_y: 1600
            }
        );
    }
}
