//! Live driver for a Rune window: newline JSON over a Unix socket named by `RUNE_DRIVE_SOCK`.
//! A compositor decorator reads the element tree each frame and injects input through the
//! window's own `Runtime`, so tests and agents drive the real app without a Rune patch.
//!
//! Commands, one JSON object per line, one reply per line (`{"ok": ..., "us": ...}`):
//! `tree`, `a11y`, `find {key|text}`, `wait {key|text, timeout_ms}`, `click {key|x,y}`,
//! `type {text}`, `key {name, mods}`, `snap {path}`.
//!
//! Everything is behind the `tooling` feature and Unix; elsewhere `install` is the identity.

#![forbid(unsafe_code)]

#[cfg(all(unix, feature = "tooling"))]
mod driver;
#[cfg(all(unix, feature = "tooling"))]
mod protocol;

#[cfg(all(unix, feature = "tooling"))]
pub use protocol::{Command, respond};

use rune::App;
use rune::compose::Compositor;

/// Drives `app` when `RUNE_DRIVE_SOCK` is set; returns it unchanged otherwise.
pub fn install<S: 'static>(app: App<S>) -> App<S> {
    #[cfg(all(unix, feature = "tooling"))]
    if let Some(path) = std::env::var_os("RUNE_DRIVE_SOCK") {
        return driver::install(app, path.into(), driver::UiOnly);
    }
    app
}

/// `install` for an app with its own compositor: the driver wraps `compositor`, which still
/// draws every frame.
pub fn install_with<S: 'static>(app: App<S>, compositor: impl Compositor<S>) -> App<S> {
    #[cfg(all(unix, feature = "tooling"))]
    if let Some(path) = std::env::var_os("RUNE_DRIVE_SOCK") {
        return driver::install(app, path.into(), compositor);
    }
    app.compositor(compositor)
}
