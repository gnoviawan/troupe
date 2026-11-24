//! The web client's type ramp as Rune text styles. Sizes and line heights are the computed
//! values Chromium reports (Tailwind's `text-*` with `leading-*`); every sans style carries the
//! stylistic sets `body` turns on for Inter.

use rune::TextStyle;

/// Inter at `size`/`line_height` and `weight`, with Troupe's Inter features.
pub const fn sans(size: f32, weight: f32, line_height: f32) -> TextStyle {
    TextStyle::new(size, weight)
        .lh(line_height)
        .features(theme::INTER_FEATURES)
}

/// Paper Mono at `size`/`line_height`.
pub const fn mono(size: f32, weight: f32, line_height: f32) -> TextStyle {
    TextStyle::new(size, weight).lh(line_height).mono()
}

/// `text-2xs`, `text-3xs` and `text-4xs`: `--font-size-micro`, 12 px on a desktop pointer. They
/// set no line height, so they inherit the root's 1.5.
pub const MICRO: f32 = 12.0;
/// `text-xs` (12/16).
pub const XS: f32 = 12.0;
/// `text-sm` (14/20).
pub const SM: f32 = 14.0;
/// `text-base` (16/24).
pub const BASE: f32 = 16.0;

pub const TEXT_XS: TextStyle = sans(XS, 400.0, 16.0);
pub const TEXT_SM: TextStyle = sans(SM, 400.0, 20.0);
pub const TEXT_BASE: TextStyle = sans(BASE, 400.0, 24.0);
pub const MONO_XS: TextStyle = mono(XS, 400.0, 16.0);
