//! Windows HID feature-report transport over hidapi 2.6.x.
//!
//! Framing follows razer-ctl `librazer/src/device.rs` and the OpenRazer
//! `razer_send_payload` retry loop (razermouse_driver.c:116-174), adapted to
//! hidapi's report-ID convention (docs.rs/hidapi `HidDevice`).

use std::ffi::CStr;
use std::time::Duration;

use hidapi::{HidApi, HidError};

use crate::report::{status, RazerReport, REPORT_SIZE};

/// Razer USB vendor id.
///
// source: OpenRazer razercommon.h — `#define RAZER_VENDOR_ID 0x1532`.
pub const RAZER_VID: u16 = 0x1532;

/// hidapi buffers carry a leading report-ID byte, so the wire buffer is 91 bytes.
///
// source: docs.rs/hidapi `HidDevice::send_feature_report` — first byte is the
// report id; razer-ctl uses `vec![0x00; 1 + size_of::<Packet>()]` (report id 0).
const HID_BUF_SIZE: usize = REPORT_SIZE + 1;

/// Max busy-retries before giving up.
///
// source: OpenRazer razermouse_driver.c:116-174 `razer_send_payload` retries
// up to 5 times with ~10 ms sleeps when the device reports status 0x01 (busy).
const MAX_BUSY_RETRIES: u32 = 5;
const BUSY_RETRY_SLEEP: Duration = Duration::from_millis(10);

/// Transport-layer errors. Every device-I/O path returns this — no panics.
#[derive(Debug, thiserror::Error)]
pub enum TransportError {
    #[error("hidapi error: {0}")]
    Hid(#[from] HidError),
    #[error("no Razer vendor-collection interface found for pid {pid:#06x}")]
    NoInterface { pid: u16 },
    #[error("no Razer device found for pid {pid:#06x}")]
    DeviceNotFound { pid: u16 },
    #[error("short feature report: expected {expected} bytes, got {got}")]
    ShortRead { expected: usize, got: usize },
    #[error(
        "device reported command failed: status {status:#04x} (class {class:#04x}, id {id:#04x})"
    )]
    CommandFailed { status: u8, class: u8, id: u8 },
    #[error("device stayed busy after {0} retries")]
    BusyExhausted(u32),
    #[error(
        "response echo mismatch: sent class/id {sent_class:#04x}/{sent_id:#04x}, \
         got {got_class:#04x}/{got_id:#04x}"
    )]
    EchoMismatch {
        sent_class: u8,
        sent_id: u8,
        got_class: u8,
        got_id: u8,
    },
}

/// An opened Razer device ready for feature-report I/O.
pub struct Device {
    handle: hidapi::HidDevice,
    transaction_id: u8,
    wait: Duration,
    pid: u16,
}

impl Device {
    /// Open the first Razer device matching `pid`, using the given transaction
    /// id and wait interval (both come from the device registry).
    ///
    /// Enumerates VID 0x1532, prefers the vendor-defined collection
    /// (`usage_page` in the 0xFF00 range), and falls back to probing candidate
    /// interfaces with a zero feature report — the first that accepts it wins.
    // source: razer-ctl librazer/src/device.rs interface-probing pattern;
    // Phase 0 anti-pattern note (never open the 0x01 keyboard/mouse collection).
    pub fn open(
        api: &HidApi,
        pid: u16,
        transaction_id: u8,
        wait_us: u32,
    ) -> Result<Self, TransportError> {
        let wait = Duration::from_micros(wait_us as u64);

        // Collect candidate interface paths for this PID.
        let mut vendor_paths: Vec<&CStr> = Vec::new();
        let mut other_paths: Vec<&CStr> = Vec::new();
        for info in api.device_list() {
            if info.vendor_id() != RAZER_VID || info.product_id() != pid {
                continue;
            }
            // Vendor-defined usage pages live in the 0xFF00..=0xFFFF range.
            if (0xFF00..=0xFFFF).contains(&info.usage_page()) {
                vendor_paths.push(info.path());
            } else {
                other_paths.push(info.path());
            }
        }

        if vendor_paths.is_empty() && other_paths.is_empty() {
            return Err(TransportError::DeviceNotFound { pid });
        }

        // Try vendor-collection interfaces first, then probe the rest.
        for path in vendor_paths.iter().chain(other_paths.iter()) {
            let handle = match api.open_path(path) {
                Ok(h) => h,
                Err(_) => continue,
            };
            // Probe: a zero-length-ish feature report the device should accept.
            // source: razer-ctl device.rs probes with send_feature_report(&[0,0]).
            if handle.send_feature_report(&[0x00, 0x00]).is_ok() {
                return Ok(Self {
                    handle,
                    transaction_id,
                    wait,
                    pid,
                });
            }
        }

        Err(TransportError::NoInterface { pid })
    }

    /// The PID this handle was opened for.
    pub fn pid(&self) -> u16 {
        self.pid
    }

    /// Send a fully-built report as a 91-byte feature report (leading 0x00
    /// report id). Does not read a response.
    // source: docs.rs/hidapi report-ID semantics; razer-ctl device.rs send().
    pub fn send_report(&self, report: &RazerReport) -> Result<(), TransportError> {
        let mut buf = [0u8; HID_BUF_SIZE];
        buf[0] = 0x00; // report id
        buf[1..].copy_from_slice(&report.to_bytes());
        self.handle.send_feature_report(&buf)?;
        Ok(())
    }

    /// Read a response report. Sleeps the device's wait interval first, then
    /// issues a feature-report read and parses bytes `[1..91]`.
    // source: OpenRazer razer_send_payload waits before reading; hidapi
    // get_feature_report expects buf[0] = report id and returns bytes+1.
    pub fn get_report(&self) -> Result<RazerReport, TransportError> {
        std::thread::sleep(self.wait);
        let mut buf = [0u8; HID_BUF_SIZE];
        buf[0] = 0x00; // report id
        let read = self.handle.get_feature_report(&mut buf)?;
        // hidapi returns byte count including the report-id byte.
        if read < HID_BUF_SIZE {
            return Err(TransportError::ShortRead {
                expected: HID_BUF_SIZE,
                got: read,
            });
        }
        let mut payload = [0u8; REPORT_SIZE];
        payload.copy_from_slice(&buf[1..HID_BUF_SIZE]);
        Ok(RazerReport::from_bytes(&payload))
    }

    /// Build a command from `command_class`/`command_id`/`args`, send it, read
    /// the response, and validate it. Retries on status 0x01 (busy).
    ///
    /// Returns the validated response report on success.
    // source: OpenRazer razermouse_driver.c:116-174 `razer_send_payload` —
    // retry <=5x with 10 ms sleeps on busy; require echoed class/id + status 0x02.
    pub fn send_payload(
        &self,
        command_class: u8,
        command_id: u8,
        args: &[u8],
    ) -> Result<RazerReport, TransportError> {
        let request = RazerReport::new(self.transaction_id, command_class, command_id, args);

        for _ in 0..=MAX_BUSY_RETRIES {
            self.send_report(&request)?;
            let response = self.get_report()?;

            match response.status {
                status::BUSY => {
                    std::thread::sleep(BUSY_RETRY_SLEEP);
                    continue;
                }
                status::SUCCESSFUL => {
                    // Validate echoed command_class / command_id.
                    if response.command_class != command_class
                        || response.command_id != command_id
                    {
                        return Err(TransportError::EchoMismatch {
                            sent_class: command_class,
                            sent_id: command_id,
                            got_class: response.command_class,
                            got_id: response.command_id,
                        });
                    }
                    return Ok(response);
                }
                other => {
                    return Err(TransportError::CommandFailed {
                        status: other,
                        class: command_class,
                        id: command_id,
                    });
                }
            }
        }

        Err(TransportError::BusyExhausted(MAX_BUSY_RETRIES))
    }
}
