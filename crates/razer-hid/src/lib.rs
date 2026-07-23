//! `razer-hid` — Razer USB HID protocol library for opsrzr.
//!
//! This crate contains:
//! - [`report`]: the 90-byte `razer_report` codec (pack/unpack + CRC).
//! - [`transport`]: the Windows hidapi feature-report transport (a [`Device`]
//!   handle with send/receive + busy-retry).
//! - [`registry`]: the per-device TOML registry (PID -> protocol params +
//!   capabilities).
//! - [`commands`]: feature-command builders (lighting, DPI, polling rate,
//!   battery/power, device info) layered on top of `report` and `transport`.
//!
//! # Attribution
//! Protocol facts (byte layouts, CRC, command classes/ids, transaction ids and
//! per-device capabilities) are ported from OpenRazer (GPL-2.0) with citations
//! at each definition; the Rust framing pattern follows razer-ctl. This crate is
//! licensed GPL-2.0-only to match.

pub mod commands;
pub mod registry;
pub mod report;
pub mod transport;

pub use registry::{Capabilities, DeviceDef, DeviceType, LedRegion, Registry, RegistryError};
pub use report::{status, RazerReport, MAX_ARGS, REPORT_SIZE};
pub use transport::{Device, TransportError, RAZER_VID};
