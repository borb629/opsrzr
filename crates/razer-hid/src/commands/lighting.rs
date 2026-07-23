//! Lighting commands: extended-matrix (modern devices, class `0x0F`) and the
//! classic/standard fallback (older devices, class `0x03`).
//!
//! Byte layouts are ported from OpenRazer `driver/razerchromacommon.c`
//! (GPL-2.0). Every command constant below cites the upstream function it
//! mirrors.

use crate::report::MAX_ARGS;
use crate::transport::{Device, TransportError};
use crate::RazerReport;

/// An RGB colour triplet.
pub type Rgb = [u8; 3];

/// Errors from building a lighting command whose payload is caller-supplied
/// (custom-frame rows) and therefore needs bounds-checking before it can be
/// turned into a `RazerReport`.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum LightingError {
    #[error("custom frame row: expected {expected} RGB bytes ({cols} cols * 3), got {got}")]
    RowLengthMismatch {
        cols: u16,
        expected: usize,
        got: usize,
    },
    #[error("custom frame row: {data_size} byte payload exceeds max argument size {max}")]
    RowTooLong { data_size: usize, max: usize },
}

// ---------------------------------------------------------------------
// Extended matrix (modern devices), class 0x0F.
// ---------------------------------------------------------------------

/// Extended-matrix command class.
// source: razerchromacommon.c `get_razer_report(0x0F, ...)` calls throughout
// razer_chroma_extended_matrix_effect_base() / _brightness() / _set_custom_frame2().
pub const EXT_CLASS: u8 = 0x0F;

/// Extended-matrix effect command id (arg0=varstore, arg1=led_id, arg2=effect).
// source: razerchromacommon.c `razer_chroma_extended_matrix_effect_base()` ->
// `get_razer_report(0x0F, 0x02, arg_size)`.
pub const EXT_CMD_EFFECT: u8 = 0x02;

/// Extended-matrix custom-frame row upload command id.
// source: razerchromacommon.c `razer_chroma_extended_matrix_set_custom_frame2()`
// -> `get_razer_report(0x0F, 0x03, data_length)`.
pub const EXT_CMD_CUSTOM_FRAME_ROW: u8 = 0x03;

/// Extended-matrix set-brightness command id.
// source: razerchromacommon.c `razer_chroma_extended_matrix_brightness()` ->
// `get_razer_report(0x0F, 0x04, 0x03)`.
pub const EXT_CMD_BRIGHTNESS_SET: u8 = 0x04;

/// Extended-matrix get-brightness command id.
// source: razerchromacommon.c `razer_chroma_extended_matrix_get_brightness()`
// -> `get_razer_report(0x0F, 0x84, 0x03)`.
pub const EXT_CMD_BRIGHTNESS_GET: u8 = 0x84;

/// Extended-matrix effect ids (arg2 of the effect command).
// source: razerchromacommon.c `razer_chroma_extended_matrix_effect_*()` family.
pub mod ext_effect {
    pub const NONE: u8 = 0x00;
    pub const STATIC: u8 = 0x01;
    pub const BREATHING: u8 = 0x02;
    pub const SPECTRUM: u8 = 0x03;
    pub const WAVE: u8 = 0x04;
    pub const REACTIVE: u8 = 0x05;
    pub const STARLIGHT: u8 = 0x07;
    pub const CUSTOM: u8 = 0x08;
    pub const WHEEL: u8 = 0x0A;
}

/// Common LED ids used by the classic/standard lighting commands (also used
/// as `led_id` for extended-matrix commands on single/dual-zone devices).
// source: OpenRazer driver/razercommon.h LED ids.
pub mod led_id {
    pub const SCROLL: u8 = 0x01;
    pub const LOGO: u8 = 0x04;
    pub const BACKLIGHT: u8 = 0x05;
}

fn clamp(v: u8, lo: u8, hi: u8) -> u8 {
    v.clamp(lo, hi)
}

// -- extended matrix: effect argument builders --------------------------

// source: razerchromacommon.c `razer_chroma_extended_matrix_effect_none()` ->
// effect_base(0x06, varstore, led_id, 0x00).
fn args_ext_none(varstore: u8, led_id: u8) -> [u8; 6] {
    [varstore, led_id, ext_effect::NONE, 0, 0, 0]
}

/// Build an extended-matrix "no effect" request.
pub fn build_ext_effect_none(transaction_id: u8, varstore: u8, led_id: u8) -> RazerReport {
    RazerReport::new(
        transaction_id,
        EXT_CLASS,
        EXT_CMD_EFFECT,
        &args_ext_none(varstore, led_id),
    )
}

// source: razerchromacommon.c `razer_chroma_extended_matrix_effect_static()` ->
// effect_base(0x09, ...); arguments[5]=0x01 (single colour), [6..9]=rgb.
fn args_ext_static(varstore: u8, led_id: u8, rgb: Rgb) -> [u8; 9] {
    [
        varstore,
        led_id,
        ext_effect::STATIC,
        0,
        0,
        0x01,
        rgb[0],
        rgb[1],
        rgb[2],
    ]
}

/// Build an extended-matrix static-colour effect request.
pub fn build_ext_effect_static(
    transaction_id: u8,
    varstore: u8,
    led_id: u8,
    rgb: Rgb,
) -> RazerReport {
    RazerReport::new(
        transaction_id,
        EXT_CLASS,
        EXT_CMD_EFFECT,
        &args_ext_static(varstore, led_id, rgb),
    )
}

// source: razerchromacommon.c `razer_chroma_extended_matrix_effect_breathing_random()`
// -> effect_base(0x06, ...), no extra bytes set.
fn args_ext_breathing_random(varstore: u8, led_id: u8) -> [u8; 6] {
    [varstore, led_id, ext_effect::BREATHING, 0, 0, 0]
}

/// Build an extended-matrix random-colour breathing effect request.
pub fn build_ext_effect_breathing_random(
    transaction_id: u8,
    varstore: u8,
    led_id: u8,
) -> RazerReport {
    RazerReport::new(
        transaction_id,
        EXT_CLASS,
        EXT_CMD_EFFECT,
        &args_ext_breathing_random(varstore, led_id),
    )
}

// source: razerchromacommon.c `razer_chroma_extended_matrix_effect_breathing_single()`
// -> effect_base(0x09, ...); arguments[3]=0x01 (type=single), [5]=0x01 (num colours),
// [6..9]=rgb.
fn args_ext_breathing_single(varstore: u8, led_id: u8, rgb: Rgb) -> [u8; 9] {
    [
        varstore,
        led_id,
        ext_effect::BREATHING,
        0x01,
        0,
        0x01,
        rgb[0],
        rgb[1],
        rgb[2],
    ]
}

/// Build an extended-matrix single-colour breathing effect request.
pub fn build_ext_effect_breathing_single(
    transaction_id: u8,
    varstore: u8,
    led_id: u8,
    rgb: Rgb,
) -> RazerReport {
    RazerReport::new(
        transaction_id,
        EXT_CLASS,
        EXT_CMD_EFFECT,
        &args_ext_breathing_single(varstore, led_id, rgb),
    )
}

// source: razerchromacommon.c `razer_chroma_extended_matrix_effect_breathing_dual()`
// -> effect_base(0x0C, ...); arguments[3]=0x02 (type=dual), [5]=0x02 (num colours),
// [6..9]=rgb1, [9..12]=rgb2.
fn args_ext_breathing_dual(varstore: u8, led_id: u8, rgb1: Rgb, rgb2: Rgb) -> [u8; 12] {
    [
        varstore,
        led_id,
        ext_effect::BREATHING,
        0x02,
        0,
        0x02,
        rgb1[0],
        rgb1[1],
        rgb1[2],
        rgb2[0],
        rgb2[1],
        rgb2[2],
    ]
}

/// Build an extended-matrix dual-colour breathing effect request.
pub fn build_ext_effect_breathing_dual(
    transaction_id: u8,
    varstore: u8,
    led_id: u8,
    rgb1: Rgb,
    rgb2: Rgb,
) -> RazerReport {
    RazerReport::new(
        transaction_id,
        EXT_CLASS,
        EXT_CMD_EFFECT,
        &args_ext_breathing_dual(varstore, led_id, rgb1, rgb2),
    )
}

// source: razerchromacommon.c `razer_chroma_extended_matrix_effect_spectrum()` ->
// effect_base(0x06, ...), no extra bytes set.
fn args_ext_spectrum(varstore: u8, led_id: u8) -> [u8; 6] {
    [varstore, led_id, ext_effect::SPECTRUM, 0, 0, 0]
}

/// Build an extended-matrix spectrum-cycling effect request.
pub fn build_ext_effect_spectrum(transaction_id: u8, varstore: u8, led_id: u8) -> RazerReport {
    RazerReport::new(
        transaction_id,
        EXT_CLASS,
        EXT_CMD_EFFECT,
        &args_ext_spectrum(varstore, led_id),
    )
}

// source: razerchromacommon.c `razer_chroma_extended_matrix_effect_wave()` ->
// effect_base(0x06, ...); direction clamped 0x00..=0x02; arguments[3]=direction,
// [4]=0x28 (fixed speed byte).
fn args_ext_wave(varstore: u8, led_id: u8, direction: u8) -> [u8; 6] {
    [
        varstore,
        led_id,
        ext_effect::WAVE,
        clamp(direction, 0x00, 0x02),
        0x28,
        0,
    ]
}

/// Build an extended-matrix wave effect request. `direction` is clamped to
/// `0x00..=0x02`.
pub fn build_ext_effect_wave(
    transaction_id: u8,
    varstore: u8,
    led_id: u8,
    direction: u8,
) -> RazerReport {
    RazerReport::new(
        transaction_id,
        EXT_CLASS,
        EXT_CMD_EFFECT,
        &args_ext_wave(varstore, led_id, direction),
    )
}

// source: razerchromacommon.c `razer_chroma_extended_matrix_effect_reactive()` ->
// effect_base(0x09, ...); speed clamped 0x01..=0x04; arguments[4]=speed,
// [5]=0x01 (num colours), [6..9]=rgb.
fn args_ext_reactive(varstore: u8, led_id: u8, speed: u8, rgb: Rgb) -> [u8; 9] {
    [
        varstore,
        led_id,
        ext_effect::REACTIVE,
        0,
        clamp(speed, 0x01, 0x04),
        0x01,
        rgb[0],
        rgb[1],
        rgb[2],
    ]
}

/// Build an extended-matrix reactive effect request. `speed` is clamped to
/// `0x01..=0x04`.
pub fn build_ext_effect_reactive(
    transaction_id: u8,
    varstore: u8,
    led_id: u8,
    speed: u8,
    rgb: Rgb,
) -> RazerReport {
    RazerReport::new(
        transaction_id,
        EXT_CLASS,
        EXT_CMD_EFFECT,
        &args_ext_reactive(varstore, led_id, speed, rgb),
    )
}

// source: razerchromacommon.c `razer_chroma_extended_matrix_effect_starlight_random()`
// -> effect_base(0x06, ...); speed clamped 0x01..=0x03; arguments[4]=speed.
fn args_ext_starlight_random(varstore: u8, led_id: u8, speed: u8) -> [u8; 6] {
    [
        varstore,
        led_id,
        ext_effect::STARLIGHT,
        0,
        clamp(speed, 0x01, 0x03),
        0,
    ]
}

/// Build an extended-matrix random-colour starlight effect request. `speed`
/// is clamped to `0x01..=0x03`.
pub fn build_ext_effect_starlight_random(
    transaction_id: u8,
    varstore: u8,
    led_id: u8,
    speed: u8,
) -> RazerReport {
    RazerReport::new(
        transaction_id,
        EXT_CLASS,
        EXT_CMD_EFFECT,
        &args_ext_starlight_random(varstore, led_id, speed),
    )
}

// source: razerchromacommon.c `razer_chroma_extended_matrix_effect_starlight_single()`
// -> effect_base(0x09, ...); arguments[4]=speed, [5]=0x01 (num colours),
// [6..9]=rgb1.
fn args_ext_starlight_single(varstore: u8, led_id: u8, speed: u8, rgb: Rgb) -> [u8; 9] {
    [
        varstore,
        led_id,
        ext_effect::STARLIGHT,
        0,
        clamp(speed, 0x01, 0x03),
        0x01,
        rgb[0],
        rgb[1],
        rgb[2],
    ]
}

/// Build an extended-matrix single-colour starlight effect request. `speed`
/// is clamped to `0x01..=0x03`.
pub fn build_ext_effect_starlight_single(
    transaction_id: u8,
    varstore: u8,
    led_id: u8,
    speed: u8,
    rgb: Rgb,
) -> RazerReport {
    RazerReport::new(
        transaction_id,
        EXT_CLASS,
        EXT_CMD_EFFECT,
        &args_ext_starlight_single(varstore, led_id, speed, rgb),
    )
}

// source: razerchromacommon.c `razer_chroma_extended_matrix_effect_starlight_dual()`
// -> effect_base(0x0C, ...); arguments[4]=speed, [5]=0x02 (num colours),
// [6..9]=rgb1, [9..12]=rgb2.
fn args_ext_starlight_dual(varstore: u8, led_id: u8, speed: u8, rgb1: Rgb, rgb2: Rgb) -> [u8; 12] {
    [
        varstore,
        led_id,
        ext_effect::STARLIGHT,
        0,
        clamp(speed, 0x01, 0x03),
        0x02,
        rgb1[0],
        rgb1[1],
        rgb1[2],
        rgb2[0],
        rgb2[1],
        rgb2[2],
    ]
}

/// Build an extended-matrix dual-colour starlight effect request. `speed` is
/// clamped to `0x01..=0x03`.
pub fn build_ext_effect_starlight_dual(
    transaction_id: u8,
    varstore: u8,
    led_id: u8,
    speed: u8,
    rgb1: Rgb,
    rgb2: Rgb,
) -> RazerReport {
    RazerReport::new(
        transaction_id,
        EXT_CLASS,
        EXT_CMD_EFFECT,
        &args_ext_starlight_dual(varstore, led_id, speed, rgb1, rgb2),
    )
}

// source: razerchromacommon.c `razer_chroma_extended_matrix_effect_custom_frame()`
// -> effect_base(0x0C, 0x00, 0x00, 0x08) — varstore and led_id are hardcoded 0
// upstream (custom-draw applies to the whole matrix, not one LED).
fn args_ext_custom_draw() -> [u8; 12] {
    [0, 0, ext_effect::CUSTOM, 0, 0, 0, 0, 0, 0, 0, 0, 0]
}

/// Build the extended-matrix "activate custom draw" effect request (arms the
/// device to display frames previously uploaded via
/// [`build_ext_custom_frame_row`]).
pub fn build_ext_effect_custom_draw(transaction_id: u8) -> RazerReport {
    RazerReport::new(
        transaction_id,
        EXT_CLASS,
        EXT_CMD_EFFECT,
        &args_ext_custom_draw(),
    )
}

// source: razerchromacommon.c `razer_chroma_extended_matrix_effect_wheel()` ->
// effect_base(0x06, ...); direction clamped 0x01..=0x02; arguments[3]=direction,
// [4]=0x28 (fixed speed byte).
fn args_ext_wheel(varstore: u8, led_id: u8, direction: u8) -> [u8; 6] {
    [
        varstore,
        led_id,
        ext_effect::WHEEL,
        clamp(direction, 0x01, 0x02),
        0x28,
        0,
    ]
}

/// Build an extended-matrix "wheel" effect request. `direction` is clamped
/// to `0x01..=0x02`.
pub fn build_ext_effect_wheel(
    transaction_id: u8,
    varstore: u8,
    led_id: u8,
    direction: u8,
) -> RazerReport {
    RazerReport::new(
        transaction_id,
        EXT_CLASS,
        EXT_CMD_EFFECT,
        &args_ext_wheel(varstore, led_id, direction),
    )
}

// -- extended matrix: brightness -----------------------------------------

fn args_ext_brightness_set(varstore: u8, led_id: u8, brightness: u8) -> [u8; 3] {
    [varstore, led_id, brightness]
}

/// Build an extended-matrix set-brightness request (`0..=255`).
pub fn build_ext_brightness_set(
    transaction_id: u8,
    varstore: u8,
    led_id: u8,
    brightness: u8,
) -> RazerReport {
    RazerReport::new(
        transaction_id,
        EXT_CLASS,
        EXT_CMD_BRIGHTNESS_SET,
        &args_ext_brightness_set(varstore, led_id, brightness),
    )
}

fn args_ext_brightness_get(varstore: u8, led_id: u8) -> [u8; 3] {
    [varstore, led_id, 0]
}

/// Build an extended-matrix get-brightness request.
pub fn build_ext_brightness_get(transaction_id: u8, varstore: u8, led_id: u8) -> RazerReport {
    RazerReport::new(
        transaction_id,
        EXT_CLASS,
        EXT_CMD_BRIGHTNESS_GET,
        &args_ext_brightness_get(varstore, led_id),
    )
}

/// Parse the brightness value (`0..=255`) out of a get-brightness response.
pub fn parse_ext_brightness(report: &RazerReport) -> u8 {
    report.arguments[2]
}

// -- extended matrix: custom-frame row upload ----------------------------

/// Build an extended-matrix custom-frame row-upload request: `row_index`
/// spans `start_col..=stop_col`, with `rgb` holding exactly
/// `(stop_col + 1 - start_col) * 3` bytes (one RGB triplet per column).
///
// source: razerchromacommon.c `razer_chroma_extended_matrix_set_custom_frame2()`:
// arguments[2]=row_index, [3]=start_col, [4]=stop_col, [5..]=rgb data;
// data_size = rgb.len() + 5. arguments[0..2] are left 0 (unused upstream).
pub fn build_ext_custom_frame_row(
    transaction_id: u8,
    row_index: u8,
    start_col: u8,
    stop_col: u8,
    rgb: &[u8],
) -> Result<RazerReport, LightingError> {
    let cols = (stop_col as u16 + 1).saturating_sub(start_col as u16);
    let expected = cols as usize * 3;
    if rgb.len() != expected {
        return Err(LightingError::RowLengthMismatch {
            cols,
            expected,
            got: rgb.len(),
        });
    }
    let data_size = rgb.len() + 5;
    if data_size > MAX_ARGS {
        return Err(LightingError::RowTooLong {
            data_size,
            max: MAX_ARGS,
        });
    }
    let mut args = vec![0u8; data_size];
    args[2] = row_index;
    args[3] = start_col;
    args[4] = stop_col;
    args[5..].copy_from_slice(rgb);
    Ok(RazerReport::new(
        transaction_id,
        EXT_CLASS,
        EXT_CMD_CUSTOM_FRAME_ROW,
        &args,
    ))
}

// ---------------------------------------------------------------------
// Standard/classic (older devices), class 0x03.
// ---------------------------------------------------------------------

/// Classic/standard command class.
// source: razerchromacommon.c `get_razer_report(0x03, ...)` calls throughout
// razer_chroma_standard_set_led_*() / _matrix_effect_*() / _matrix_set_custom_frame().
pub const STD_CLASS: u8 = 0x03;

/// Classic single-LED "on/off" state command id.
// source: razerchromacommon.c `razer_chroma_standard_set_led_state()` ->
// `get_razer_report(0x03, 0x00, 0x03)`.
pub const STD_CMD_LED_STATE: u8 = 0x00;

/// Classic single-LED static RGB command id.
// source: razerchromacommon.c `razer_chroma_standard_set_led_rgb()` ->
// `get_razer_report(0x03, 0x01, 0x05)`.
pub const STD_CMD_LED_RGB: u8 = 0x01;

/// Classic single-LED effect command id.
// source: razerchromacommon.c `razer_chroma_standard_set_led_effect()` ->
// `get_razer_report(0x03, 0x02, 0x03)`.
pub const STD_CMD_LED_EFFECT: u8 = 0x02;

/// Classic single-LED brightness command id.
// source: razerchromacommon.c `razer_chroma_standard_set_led_brightness()` ->
// `get_razer_report(0x03, 0x03, 0x03)`.
pub const STD_CMD_LED_BRIGHTNESS: u8 = 0x03;

/// Classic whole-matrix effect command id.
// source: razerchromacommon.c `razer_chroma_standard_matrix_effect_base()` ->
// `get_razer_report(0x03, 0x0A, arg_size)`.
pub const STD_CMD_MATRIX_EFFECT: u8 = 0x0A;

/// Classic whole-matrix custom-frame row upload command id.
// source: razerchromacommon.c `razer_chroma_standard_matrix_set_custom_frame()`
// -> `get_razer_report(0x03, 0x0B, 0x46)`.
pub const STD_CMD_MATRIX_CUSTOM_FRAME_ROW: u8 = 0x0B;

/// Classic whole-matrix effect ids (arg0 of the matrix-effect command).
// source: razerchromacommon.c razer_chroma_standard_matrix_effect_*() — class 0x03, cmd 0x0A effect ids
pub mod std_effect {
    pub const OFF: u8 = 0x00;
    pub const WAVE: u8 = 0x01;
    pub const REACTIVE: u8 = 0x02;
    pub const BREATHING: u8 = 0x03;
    pub const SPECTRUM: u8 = 0x04;
    pub const CUSTOM: u8 = 0x05;
    pub const STATIC: u8 = 0x06;
    pub const STARLIGHT: u8 = 0x19;
}

// -- classic single-LED commands -----------------------------------------

fn args_std_led_state(varstore: u8, led_id: u8, state: u8) -> [u8; 3] {
    [varstore, led_id, clamp(state, 0x00, 0x01)]
}

/// Build a classic single-LED on/off request. `state` is clamped to
/// `0x00..=0x01`.
pub fn build_std_led_state(
    transaction_id: u8,
    varstore: u8,
    led_id: u8,
    state: u8,
) -> RazerReport {
    RazerReport::new(
        transaction_id,
        STD_CLASS,
        STD_CMD_LED_STATE,
        &args_std_led_state(varstore, led_id, state),
    )
}

fn args_std_led_rgb(varstore: u8, led_id: u8, rgb: Rgb) -> [u8; 5] {
    [varstore, led_id, rgb[0], rgb[1], rgb[2]]
}

/// Build a classic single-LED static-colour request.
pub fn build_std_led_rgb(transaction_id: u8, varstore: u8, led_id: u8, rgb: Rgb) -> RazerReport {
    RazerReport::new(
        transaction_id,
        STD_CLASS,
        STD_CMD_LED_RGB,
        &args_std_led_rgb(varstore, led_id, rgb),
    )
}

fn args_std_led_effect(varstore: u8, led_id: u8, effect: u8) -> [u8; 3] {
    [varstore, led_id, clamp(effect, 0x00, 0x05)]
}

/// Build a classic single-LED effect request. `effect` is clamped to
/// `0x00..=0x05`.
pub fn build_std_led_effect(
    transaction_id: u8,
    varstore: u8,
    led_id: u8,
    effect: u8,
) -> RazerReport {
    RazerReport::new(
        transaction_id,
        STD_CLASS,
        STD_CMD_LED_EFFECT,
        &args_std_led_effect(varstore, led_id, effect),
    )
}

fn args_std_led_brightness(varstore: u8, led_id: u8, brightness: u8) -> [u8; 3] {
    [varstore, led_id, brightness]
}

/// Build a classic single-LED brightness request.
pub fn build_std_led_brightness(
    transaction_id: u8,
    varstore: u8,
    led_id: u8,
    brightness: u8,
) -> RazerReport {
    RazerReport::new(
        transaction_id,
        STD_CLASS,
        STD_CMD_LED_BRIGHTNESS,
        &args_std_led_brightness(varstore, led_id, brightness),
    )
}

// -- classic whole-matrix effects -----------------------------------------

// source: razerchromacommon.c `razer_chroma_standard_matrix_effect_none()` ->
// effect_base(0x01, MATRIX_EFFECT_OFF).
fn args_std_matrix_none() -> [u8; 1] {
    [std_effect::OFF]
}

/// Build a classic whole-matrix "off" request.
pub fn build_std_matrix_effect_none(transaction_id: u8) -> RazerReport {
    RazerReport::new(
        transaction_id,
        STD_CLASS,
        STD_CMD_MATRIX_EFFECT,
        &args_std_matrix_none(),
    )
}

// source: razerchromacommon.c `razer_chroma_standard_matrix_effect_wave()` ->
// effect_base(0x02, MATRIX_EFFECT_WAVE); direction clamped 0x01..=0x02.
fn args_std_matrix_wave(direction: u8) -> [u8; 2] {
    [std_effect::WAVE, clamp(direction, 0x01, 0x02)]
}

/// Build a classic whole-matrix wave request. `direction` is clamped to
/// `0x01..=0x02`.
pub fn build_std_matrix_effect_wave(transaction_id: u8, direction: u8) -> RazerReport {
    RazerReport::new(
        transaction_id,
        STD_CLASS,
        STD_CMD_MATRIX_EFFECT,
        &args_std_matrix_wave(direction),
    )
}

// source: razerchromacommon.c `razer_chroma_standard_matrix_effect_spectrum()`
// -> effect_base(0x01, MATRIX_EFFECT_SPECTRUM).
fn args_std_matrix_spectrum() -> [u8; 1] {
    [std_effect::SPECTRUM]
}

/// Build a classic whole-matrix spectrum-cycling request.
pub fn build_std_matrix_effect_spectrum(transaction_id: u8) -> RazerReport {
    RazerReport::new(
        transaction_id,
        STD_CLASS,
        STD_CMD_MATRIX_EFFECT,
        &args_std_matrix_spectrum(),
    )
}

// source: razerchromacommon.c `razer_chroma_standard_matrix_effect_static()`
// -> effect_base(0x04, MATRIX_EFFECT_STATIC); arguments[1..4]=rgb.
fn args_std_matrix_static(rgb: Rgb) -> [u8; 4] {
    [std_effect::STATIC, rgb[0], rgb[1], rgb[2]]
}

/// Build a classic whole-matrix static-colour request.
pub fn build_std_matrix_effect_static(transaction_id: u8, rgb: Rgb) -> RazerReport {
    RazerReport::new(
        transaction_id,
        STD_CLASS,
        STD_CMD_MATRIX_EFFECT,
        &args_std_matrix_static(rgb),
    )
}

// source: razerchromacommon.c `razer_chroma_standard_matrix_effect_reactive()`
// -> effect_base(0x05, MATRIX_EFFECT_REACTIVE); speed clamped 0x01..=0x04;
// arguments[1]=speed, [2..5]=rgb.
fn args_std_matrix_reactive(speed: u8, rgb: Rgb) -> [u8; 5] {
    [
        std_effect::REACTIVE,
        clamp(speed, 0x01, 0x04),
        rgb[0],
        rgb[1],
        rgb[2],
    ]
}

/// Build a classic whole-matrix reactive request. `speed` is clamped to
/// `0x01..=0x04`.
pub fn build_std_matrix_effect_reactive(transaction_id: u8, speed: u8, rgb: Rgb) -> RazerReport {
    RazerReport::new(
        transaction_id,
        STD_CLASS,
        STD_CMD_MATRIX_EFFECT,
        &args_std_matrix_reactive(speed, rgb),
    )
}

// source: razerchromacommon.c `razer_chroma_standard_matrix_effect_breathing_single()`
// -> effect_base(0x08, MATRIX_EFFECT_BREATHING); arguments[1]=0x01 (single
// colour), [2..5]=rgb; arg_size 0x08 leaves [5..8] zero-padded.
fn args_std_matrix_breathing_single(rgb: Rgb) -> [u8; 8] {
    [
        std_effect::BREATHING,
        0x01,
        rgb[0],
        rgb[1],
        rgb[2],
        0,
        0,
        0,
    ]
}

/// Build a classic whole-matrix single-colour breathing request.
pub fn build_std_matrix_effect_breathing_single(transaction_id: u8, rgb: Rgb) -> RazerReport {
    RazerReport::new(
        transaction_id,
        STD_CLASS,
        STD_CMD_MATRIX_EFFECT,
        &args_std_matrix_breathing_single(rgb),
    )
}

// source: razerchromacommon.c `razer_chroma_standard_matrix_effect_custom_frame()`
// -> effect_base(0x02, MATRIX_EFFECT_CUSTOMFRAME); arguments[1]=variable_storage.
fn args_std_matrix_custom(varstore: u8) -> [u8; 2] {
    [std_effect::CUSTOM, varstore]
}

/// Build a classic whole-matrix "activate custom frame" request.
pub fn build_std_matrix_effect_custom(transaction_id: u8, varstore: u8) -> RazerReport {
    RazerReport::new(
        transaction_id,
        STD_CLASS,
        STD_CMD_MATRIX_EFFECT,
        &args_std_matrix_custom(varstore),
    )
}

/// Build a classic whole-matrix single-colour starlight request. `speed` is
/// clamped to `0x01..=0x03`.
///
/// **Upstream quirk, faithfully reproduced:** OpenRazer passes `arg_size =
/// 0x01` to `razer_chroma_standard_matrix_effect_base()` for this effect even
/// though it writes bytes through `arguments[8]` — the kernel driver always
/// transmits the full fixed-size argument buffer regardless of the `data_size`
/// metadata field, so the extra bytes still reach the device; only the
/// `data_size` byte itself (and therefore the CRC) differs from what the
/// written-byte count would suggest. We replicate this exactly rather than
/// "fixing" it, since the goal is protocol fidelity, not a corrected protocol.
// source: razerchromacommon.c `razer_chroma_standard_matrix_effect_starlight_single()`:
// `razer_chroma_standard_matrix_effect_base(0x01, MATRIX_EFFECT_STARLIGHT)`,
// then arguments[1]=0x01 (one colour), [2]=speed, [3..6]=rgb1, [6..9]=0,0,0.
pub fn build_std_matrix_effect_starlight_single(
    transaction_id: u8,
    speed: u8,
    rgb: Rgb,
) -> RazerReport {
    let mut arguments = [0u8; MAX_ARGS];
    arguments[0] = std_effect::STARLIGHT;
    arguments[1] = 0x01;
    arguments[2] = clamp(speed, 0x01, 0x03);
    arguments[3] = rgb[0];
    arguments[4] = rgb[1];
    arguments[5] = rgb[2];
    build_raw(transaction_id, STD_CLASS, STD_CMD_MATRIX_EFFECT, 0x01, arguments)
}

/// Build a classic whole-matrix dual-colour starlight request. `speed` is
/// clamped to `0x01..=0x03`. See [`build_std_matrix_effect_starlight_single`]
/// for the upstream `data_size` quirk this replicates.
// source: razerchromacommon.c `razer_chroma_standard_matrix_effect_starlight_dual()`:
// `razer_chroma_standard_matrix_effect_base(0x01, MATRIX_EFFECT_STARLIGHT)`,
// then arguments[1]=0x02 (two colours), [2]=speed, [3..6]=rgb1, [6..9]=rgb2.
pub fn build_std_matrix_effect_starlight_dual(
    transaction_id: u8,
    speed: u8,
    rgb1: Rgb,
    rgb2: Rgb,
) -> RazerReport {
    let mut arguments = [0u8; MAX_ARGS];
    arguments[0] = std_effect::STARLIGHT;
    arguments[1] = 0x02;
    arguments[2] = clamp(speed, 0x01, 0x03);
    arguments[3] = rgb1[0];
    arguments[4] = rgb1[1];
    arguments[5] = rgb1[2];
    arguments[6] = rgb2[0];
    arguments[7] = rgb2[1];
    arguments[8] = rgb2[2];
    build_raw(transaction_id, STD_CLASS, STD_CMD_MATRIX_EFFECT, 0x01, arguments)
}

/// Construct a `RazerReport` with an explicit `data_size` independent of the
/// populated argument bytes, for the rare upstream commands (see
/// `build_std_matrix_effect_starlight_*`) whose `data_size` metadata
/// genuinely does not match the number of meaningful argument bytes written.
fn build_raw(
    transaction_id: u8,
    command_class: u8,
    command_id: u8,
    data_size: u8,
    arguments: [u8; MAX_ARGS],
) -> RazerReport {
    let mut report = RazerReport {
        transaction_id,
        command_class,
        command_id,
        data_size,
        arguments,
        ..RazerReport::default()
    };
    report.crc = report.compute_crc();
    report
}

/// Build a classic whole-matrix custom-frame row-upload request. `rgb` must
/// hold exactly `(stop_col + 1 - start_col) * 3` bytes.
///
// source: razerchromacommon.c `razer_chroma_standard_matrix_set_custom_frame()`:
// `get_razer_report(0x03, 0x0B, 0x46)`; arguments[0]=0xFF (frame id),
// [1]=row_index, [2]=start_col, [3]=stop_col, [4..]=rgb data.
pub fn build_std_matrix_custom_frame_row(
    transaction_id: u8,
    row_index: u8,
    start_col: u8,
    stop_col: u8,
    rgb: &[u8],
) -> Result<RazerReport, LightingError> {
    let cols = (stop_col as u16 + 1).saturating_sub(start_col as u16);
    let expected = cols as usize * 3;
    if rgb.len() != expected {
        return Err(LightingError::RowLengthMismatch {
            cols,
            expected,
            got: rgb.len(),
        });
    }
    if rgb.len() + 4 > MAX_ARGS {
        return Err(LightingError::RowTooLong {
            data_size: rgb.len() + 4,
            max: MAX_ARGS,
        });
    }
    let mut arguments = [0u8; MAX_ARGS];
    arguments[0] = 0xFF;
    arguments[1] = row_index;
    arguments[2] = start_col;
    arguments[3] = stop_col;
    arguments[4..4 + rgb.len()].copy_from_slice(rgb);
    // data_size is a fixed 0x46 upstream regardless of the actual row length.
    Ok(build_raw(
        transaction_id,
        STD_CLASS,
        STD_CMD_MATRIX_CUSTOM_FRAME_ROW,
        0x46,
        arguments,
    ))
}

// ---------------------------------------------------------------------
// Device convenience methods.
// ---------------------------------------------------------------------

impl Device {
    /// Set a static colour via the extended-matrix path (modern devices).
    pub fn set_static_color(
        &self,
        varstore: u8,
        led_id: u8,
        rgb: Rgb,
    ) -> Result<(), TransportError> {
        self.send_payload(EXT_CLASS, EXT_CMD_EFFECT, &args_ext_static(varstore, led_id, rgb))?;
        Ok(())
    }

    /// Turn the LED effect off (extended-matrix path).
    pub fn set_effect_none(&self, varstore: u8, led_id: u8) -> Result<(), TransportError> {
        self.send_payload(EXT_CLASS, EXT_CMD_EFFECT, &args_ext_none(varstore, led_id))?;
        Ok(())
    }

    /// Set a single-colour breathing effect (extended-matrix path).
    pub fn set_effect_breathing_single(
        &self,
        varstore: u8,
        led_id: u8,
        rgb: Rgb,
    ) -> Result<(), TransportError> {
        self.send_payload(
            EXT_CLASS,
            EXT_CMD_EFFECT,
            &args_ext_breathing_single(varstore, led_id, rgb),
        )?;
        Ok(())
    }

    /// Set the spectrum-cycling effect (extended-matrix path).
    pub fn set_effect_spectrum(&self, varstore: u8, led_id: u8) -> Result<(), TransportError> {
        self.send_payload(EXT_CLASS, EXT_CMD_EFFECT, &args_ext_spectrum(varstore, led_id))?;
        Ok(())
    }

    /// Set the wave effect (extended-matrix path). `direction` is clamped to
    /// `0x00..=0x02`.
    pub fn set_effect_wave(
        &self,
        varstore: u8,
        led_id: u8,
        direction: u8,
    ) -> Result<(), TransportError> {
        self.send_payload(
            EXT_CLASS,
            EXT_CMD_EFFECT,
            &args_ext_wave(varstore, led_id, direction),
        )?;
        Ok(())
    }

    /// Set the reactive effect (extended-matrix path). `speed` is clamped to
    /// `0x01..=0x04`.
    pub fn set_effect_reactive(
        &self,
        varstore: u8,
        led_id: u8,
        speed: u8,
        rgb: Rgb,
    ) -> Result<(), TransportError> {
        self.send_payload(
            EXT_CLASS,
            EXT_CMD_EFFECT,
            &args_ext_reactive(varstore, led_id, speed, rgb),
        )?;
        Ok(())
    }

    /// Set LED brightness (`0..=255`) via the extended-matrix path.
    pub fn set_brightness(
        &self,
        varstore: u8,
        led_id: u8,
        brightness: u8,
    ) -> Result<(), TransportError> {
        self.send_payload(
            EXT_CLASS,
            EXT_CMD_BRIGHTNESS_SET,
            &args_ext_brightness_set(varstore, led_id, brightness),
        )?;
        Ok(())
    }

    /// Read back LED brightness via the extended-matrix path.
    pub fn get_brightness(&self, varstore: u8, led_id: u8) -> Result<u8, TransportError> {
        let response = self.send_payload(
            EXT_CLASS,
            EXT_CMD_BRIGHTNESS_GET,
            &args_ext_brightness_get(varstore, led_id),
        )?;
        Ok(parse_ext_brightness(&response))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ext_effect_none_bytes() {
        let r = build_ext_effect_none(0x1F, VARSTORE_TEST, 0x04);
        assert_eq!(r.command_class, 0x0F);
        assert_eq!(r.command_id, 0x02);
        assert_eq!(r.data_size, 6);
        assert_eq!(&r.arguments[..6], &[0x01, 0x04, 0x00, 0, 0, 0]);
        assert!(r.crc_valid());
    }

    const VARSTORE_TEST: u8 = crate::commands::VARSTORE;

    #[test]
    fn ext_effect_static_bytes() {
        let r = build_ext_effect_static(0x1F, crate::commands::NOSTORE, 0x04, [0xFF, 0x00, 0x80]);
        assert_eq!(r.command_class, 0x0F);
        assert_eq!(r.command_id, 0x02);
        assert_eq!(r.data_size, 9);
        assert_eq!(
            &r.arguments[..9],
            &[0x00, 0x04, 0x01, 0x00, 0x00, 0x01, 0xFF, 0x00, 0x80]
        );
        assert!(r.crc_valid());
    }

    #[test]
    fn ext_effect_breathing_single_bytes() {
        let r = build_ext_effect_breathing_single(0xFF, VARSTORE_TEST, 0x01, [0x10, 0x20, 0x30]);
        assert_eq!(r.data_size, 9);
        assert_eq!(
            &r.arguments[..9],
            &[0x01, 0x01, 0x02, 0x01, 0x00, 0x01, 0x10, 0x20, 0x30]
        );
    }

    #[test]
    fn ext_effect_breathing_dual_bytes() {
        let r = build_ext_effect_breathing_dual(
            0xFF,
            VARSTORE_TEST,
            0x01,
            [0x01, 0x02, 0x03],
            [0x04, 0x05, 0x06],
        );
        assert_eq!(r.data_size, 12);
        assert_eq!(
            &r.arguments[..12],
            &[
                0x01, 0x01, 0x02, 0x02, 0x00, 0x02, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06
            ]
        );
    }

    #[test]
    fn ext_effect_breathing_random_bytes() {
        let r = build_ext_effect_breathing_random(0xFF, VARSTORE_TEST, 0x01);
        assert_eq!(r.data_size, 6);
        assert_eq!(&r.arguments[..6], &[0x01, 0x01, 0x02, 0, 0, 0]);
    }

    #[test]
    fn ext_effect_spectrum_bytes() {
        let r = build_ext_effect_spectrum(0xFF, VARSTORE_TEST, 0x04);
        assert_eq!(r.data_size, 6);
        assert_eq!(&r.arguments[..6], &[0x01, 0x04, 0x03, 0, 0, 0]);
    }

    #[test]
    fn ext_effect_wave_bytes_and_clamp() {
        let r = build_ext_effect_wave(0xFF, VARSTORE_TEST, 0x01, 0x01);
        assert_eq!(r.data_size, 6);
        assert_eq!(&r.arguments[..6], &[0x01, 0x01, 0x04, 0x01, 0x28, 0]);

        // Out-of-range direction clamps to 0x02.
        let clamped = build_ext_effect_wave(0xFF, VARSTORE_TEST, 0x01, 0xFF);
        assert_eq!(clamped.arguments[3], 0x02);
    }

    #[test]
    fn ext_effect_reactive_bytes_and_clamp() {
        let r = build_ext_effect_reactive(0xFF, VARSTORE_TEST, 0x01, 0x02, [1, 2, 3]);
        assert_eq!(r.data_size, 9);
        assert_eq!(
            &r.arguments[..9],
            &[0x01, 0x01, 0x05, 0x00, 0x02, 0x01, 1, 2, 3]
        );
        let clamped = build_ext_effect_reactive(0xFF, VARSTORE_TEST, 0x01, 0xFF, [0, 0, 0]);
        assert_eq!(clamped.arguments[4], 0x04);
    }

    #[test]
    fn ext_effect_starlight_bytes() {
        let single = build_ext_effect_starlight_single(0xFF, VARSTORE_TEST, 0x01, 0x02, [9, 9, 9]);
        assert_eq!(single.data_size, 9);
        assert_eq!(
            &single.arguments[..9],
            &[0x01, 0x01, 0x07, 0x00, 0x02, 0x01, 9, 9, 9]
        );

        let random = build_ext_effect_starlight_random(0xFF, VARSTORE_TEST, 0x01, 0x03);
        assert_eq!(random.data_size, 6);
        assert_eq!(&random.arguments[..6], &[0x01, 0x01, 0x07, 0, 0x03, 0]);

        let dual = build_ext_effect_starlight_dual(0xFF, VARSTORE_TEST, 0x01, 0x01, [1, 2, 3], [4, 5, 6]);
        assert_eq!(dual.data_size, 12);
        assert_eq!(
            &dual.arguments[..12],
            &[0x01, 0x01, 0x07, 0, 0x01, 0x02, 1, 2, 3, 4, 5, 6]
        );
    }

    #[test]
    fn ext_effect_custom_draw_bytes() {
        let r = build_ext_effect_custom_draw(0xFF);
        assert_eq!(r.data_size, 12);
        assert_eq!(&r.arguments[..12], &[0, 0, 0x08, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    }

    #[test]
    fn ext_effect_wheel_bytes_and_clamp() {
        let r = build_ext_effect_wheel(0xFF, VARSTORE_TEST, 0x01, 0x02);
        assert_eq!(r.data_size, 6);
        assert_eq!(&r.arguments[..6], &[0x01, 0x01, 0x0A, 0x02, 0x28, 0]);
        let clamped = build_ext_effect_wheel(0xFF, VARSTORE_TEST, 0x01, 0x00);
        assert_eq!(clamped.arguments[3], 0x01);
    }

    #[test]
    fn ext_brightness_set_and_get_bytes() {
        let set = build_ext_brightness_set(0xFF, VARSTORE_TEST, 0x04, 200);
        assert_eq!(set.command_class, 0x0F);
        assert_eq!(set.command_id, 0x04);
        assert_eq!(set.data_size, 3);
        assert_eq!(&set.arguments[..3], &[0x01, 0x04, 200]);

        let get = build_ext_brightness_get(0xFF, VARSTORE_TEST, 0x04);
        assert_eq!(get.command_id, 0x84);
        assert_eq!(get.data_size, 3);
        assert_eq!(&get.arguments[..3], &[0x01, 0x04, 0]);
    }

    #[test]
    fn parse_brightness_reads_arg2() {
        let mut report = build_ext_brightness_get(0xFF, VARSTORE_TEST, 0x04);
        report.arguments[2] = 128;
        assert_eq!(parse_ext_brightness(&report), 128);
    }

    #[test]
    fn ext_custom_frame_row_bytes() {
        let rgb = [0xAAu8, 0xBB, 0xCC, 0x11, 0x22, 0x33]; // 2 columns
        let r = build_ext_custom_frame_row(0xFF, 0x00, 0x00, 0x01, &rgb).expect("valid row");
        assert_eq!(r.command_class, 0x0F);
        assert_eq!(r.command_id, 0x03);
        assert_eq!(r.data_size, 11); // 6 rgb bytes + 5
        assert_eq!(
            &r.arguments[..11],
            &[0, 0, 0x00, 0x00, 0x01, 0xAA, 0xBB, 0xCC, 0x11, 0x22, 0x33]
        );
    }

    #[test]
    fn ext_custom_frame_row_rejects_mismatched_length() {
        let err = build_ext_custom_frame_row(0xFF, 0x00, 0x00, 0x01, &[0x00; 3]).unwrap_err();
        assert_eq!(
            err,
            LightingError::RowLengthMismatch {
                cols: 2,
                expected: 6,
                got: 3,
            }
        );
    }

    #[test]
    fn std_led_state_rgb_effect_brightness_bytes() {
        let state = build_std_led_state(0x3F, VARSTORE_TEST, led_id::SCROLL, 0x01);
        assert_eq!(state.command_class, 0x03);
        assert_eq!(state.command_id, 0x00);
        assert_eq!(state.data_size, 3);
        assert_eq!(&state.arguments[..3], &[0x01, 0x01, 0x01]);
        // Out-of-range state clamps to 0x01.
        let clamped_state = build_std_led_state(0x3F, VARSTORE_TEST, led_id::SCROLL, 0xFF);
        assert_eq!(clamped_state.arguments[2], 0x01);

        let rgb = build_std_led_rgb(0x3F, VARSTORE_TEST, led_id::LOGO, [1, 2, 3]);
        assert_eq!(rgb.command_id, 0x01);
        assert_eq!(rgb.data_size, 5);
        assert_eq!(&rgb.arguments[..5], &[0x01, 0x04, 1, 2, 3]);

        let effect = build_std_led_effect(0x3F, VARSTORE_TEST, led_id::BACKLIGHT, 0x02);
        assert_eq!(effect.command_id, 0x02);
        assert_eq!(effect.data_size, 3);
        assert_eq!(&effect.arguments[..3], &[0x01, 0x05, 0x02]);

        let brightness = build_std_led_brightness(0x3F, VARSTORE_TEST, led_id::LOGO, 128);
        assert_eq!(brightness.command_id, 0x03);
        assert_eq!(&brightness.arguments[..3], &[0x01, 0x04, 128]);
    }

    #[test]
    fn std_matrix_effect_bytes() {
        let none = build_std_matrix_effect_none(0x3F);
        assert_eq!(none.command_class, 0x03);
        assert_eq!(none.command_id, 0x0A);
        assert_eq!(none.data_size, 1);
        assert_eq!(&none.arguments[..1], &[0x00]);

        let wave = build_std_matrix_effect_wave(0x3F, 0x01);
        assert_eq!(wave.data_size, 2);
        assert_eq!(&wave.arguments[..2], &[0x01, 0x01]);

        let spectrum = build_std_matrix_effect_spectrum(0x3F);
        assert_eq!(spectrum.data_size, 1);
        assert_eq!(&spectrum.arguments[..1], &[0x04]);

        let static_ = build_std_matrix_effect_static(0x3F, [10, 20, 30]);
        assert_eq!(static_.data_size, 4);
        assert_eq!(&static_.arguments[..4], &[0x06, 10, 20, 30]);

        let reactive = build_std_matrix_effect_reactive(0x3F, 0x02, [1, 2, 3]);
        assert_eq!(reactive.data_size, 5);
        assert_eq!(&reactive.arguments[..5], &[0x02, 0x02, 1, 2, 3]);

        let breathing = build_std_matrix_effect_breathing_single(0x3F, [4, 5, 6]);
        assert_eq!(breathing.data_size, 8);
        assert_eq!(&breathing.arguments[..8], &[0x03, 0x01, 4, 5, 6, 0, 0, 0]);

        let custom = build_std_matrix_effect_custom(0x3F, VARSTORE_TEST);
        assert_eq!(custom.data_size, 2);
        assert_eq!(&custom.arguments[..2], &[0x05, 0x01]);
    }

    #[test]
    fn std_matrix_effect_starlight_replicates_upstream_data_size_quirk() {
        let single = build_std_matrix_effect_starlight_single(0x3F, 0x02, [1, 2, 3]);
        assert_eq!(single.command_class, 0x03);
        assert_eq!(single.command_id, 0x0A);
        // Upstream passes arg_size=0x01 despite writing through arguments[8].
        assert_eq!(single.data_size, 0x01);
        assert_eq!(
            &single.arguments[..9],
            &[0x19, 0x01, 0x02, 1, 2, 3, 0, 0, 0]
        );
        assert!(single.crc_valid());

        let dual = build_std_matrix_effect_starlight_dual(0x3F, 0x01, [1, 2, 3], [4, 5, 6]);
        assert_eq!(dual.data_size, 0x01);
        assert_eq!(&dual.arguments[..9], &[0x19, 0x02, 0x01, 1, 2, 3, 4, 5, 6]);
        assert!(dual.crc_valid());
    }

    #[test]
    fn std_matrix_custom_frame_row_bytes() {
        let rgb = [1u8, 2, 3, 4, 5, 6];
        let r = build_std_matrix_custom_frame_row(0x3F, 0x00, 0x00, 0x01, &rgb).expect("valid");
        assert_eq!(r.command_class, 0x03);
        assert_eq!(r.command_id, 0x0B);
        assert_eq!(r.data_size, 0x46);
        assert_eq!(
            &r.arguments[..10],
            &[0xFF, 0x00, 0x00, 0x01, 1, 2, 3, 4, 5, 6]
        );
    }

    #[test]
    fn std_matrix_custom_frame_row_rejects_mismatched_length() {
        let err = build_std_matrix_custom_frame_row(0x3F, 0x00, 0x00, 0x01, &[0u8; 3]).unwrap_err();
        assert_eq!(
            err,
            LightingError::RowLengthMismatch {
                cols: 2,
                expected: 6,
                got: 3,
            }
        );
    }
}
