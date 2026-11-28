//! FigCraft's editor in the Rune desktop app (plan R11, the egui slot of R0's FigCraft spike).
//!
//! The editor is FigCraft's own egui UI (`figcraft_ui_egui::FigApp`, the kiosk shell the web
//! bundle shows in Troupe) at the fork tag `src-tauri` pins. [`Handle`] mounts one editor and
//! carries the host's control messages to it in figcraft-web's `postMessage` shape (`engine`), so
//! the host side of the bridge is the same for the desktop editor and the web client's iframe.
//! [`CanvasCompositor`] draws each visible editor in its `El::effect` slot (`slot`).

#![cfg(not(target_arch = "wasm32"))]

mod blit;
mod engine;
mod host;
pub mod input;
mod slot;

#[cfg(test)]
mod engine_tests;
#[cfg(test)]
mod pin_tests;

pub use egui;
pub use engine::Engine;
pub use slot::{CanvasCompositor, Handle, PlatformAction, slot_key};

/// The figcraft tag this build pins, read from the file `scripts/canvas/check-figcraft-pin.mjs`
/// keeps equal to `src-tauri`'s dependency; `host.describe` reports it.
pub const PIN_TAG: &str = pin_tag(include_str!("../../../../../canvas/figcraft.pin"));

/// The figcraft workspace version at that tag, which the web bundle's `host.describe` reports as
/// its engine version.
pub const ENGINE_VERSION: &str = "0.15.0";

const fn pin_tag(pin: &str) -> &str {
    let bytes = pin.as_bytes();
    let prefix = b"tag=";
    let mut i = 0;
    while i < prefix.len() {
        if bytes.len() <= i || bytes[i] != prefix[i] {
            panic!("canvas/figcraft.pin must start with tag=");
        }
        i += 1;
    }
    let mut end = prefix.len();
    while end < bytes.len() && bytes[end] != b'\n' && bytes[end] != b'\r' {
        end += 1;
    }
    let (_, rest) = bytes.split_at(prefix.len());
    let (tag, _) = rest.split_at(end - prefix.len());
    match std::str::from_utf8(tag) {
        Ok(tag) => tag,
        Err(_) => panic!("canvas/figcraft.pin is not UTF-8"),
    }
}
