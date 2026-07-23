//! Feature-command builders layered on top of the `razer_report` codec.
//!
//! Every submodule holds pure functions that build [`crate::RazerReport`]
//! request values, parsers for the matching response values, and — where a
//! send+parse round trip is meaningful — thin convenience methods added onto
//! [`crate::Device`]. Every command byte/constant carries a `// source:`
//! citation naming the OpenRazer (GPL-2.0) function or file it was ported
//! from; see each submodule for details.
//!
//! Argument-byte helper functions (named `args_*`) are shared between the
//! pure `RazerReport` builders (used directly in unit tests, asserting exact
//! request bytes) and the `Device` convenience methods, so the two never
//! drift apart.

pub mod dpi;
pub mod info;
pub mod lighting;
pub mod polling;
pub mod power;

/// Persist the setting to the device's onboard flash.
// source: OpenRazer driver/razerchromacommon.h `#define VARSTORE 0x01`
pub const VARSTORE: u8 = 0x01;

/// Do not persist; setting is volatile until the next write.
// source: OpenRazer driver/razerchromacommon.h `#define NOSTORE 0x00`
pub const NOSTORE: u8 = 0x00;
