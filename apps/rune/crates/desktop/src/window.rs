//! The window: 1200x800 by default, 800x600 at least, hidden until its first frame is ready,
//! the overlay title bar with the traffic lights at (22, 24) on macOS, and the frame saved on
//! close and quit and restored at the next launch (`window.json`, physical pixels). Wayland
//! hides window placement, so nothing is saved there.

use std::path::Path;
use std::sync::Mutex;

use rune::{Chrome, WindowHandle, WindowOptions};
use serde::{Deserialize, Serialize};

pub const DEFAULT_SIZE: (f32, f32) = (1200.0, 800.0);
pub const MIN_SIZE: (f32, f32) = (800.0, 600.0);
/// The Tauri app's `trafficLightPosition` (`tauri.conf.json`).
pub const TRAFFIC_LIGHTS: (f32, f32) = (22.0, 24.0);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Frame {
    /// The frame's top-left.
    pub position: (i32, i32),
    /// The content size in physical pixels.
    pub size: (u32, u32),
    pub maximized: bool,
}

impl Frame {
    /// A frame worth restoring: positive, and not absurdly far off any display.
    fn sane(&self) -> bool {
        let (w, h) = self.size;
        let (x, y) = self.position;
        let on_desktop = x.abs() < 100_000 && y.abs() < 100_000;
        (100..=30_000).contains(&w) && (100..=30_000).contains(&h) && on_desktop
    }

    /// The frame to restore from the window's placement. A maximized, minimized or
    /// full-screen window reports that frame, not the one it returns to: the system's record
    /// of that (Windows) wins, else the frame saved before (`previous`) with the new state.
    pub fn of(placement: &rune::WindowPlacement, previous: Option<Frame>) -> Option<Frame> {
        let maximized = placement.maximized;
        if let Some((position, size)) = placement.restore {
            return Some(Frame { position, size, maximized });
        }
        if maximized || placement.minimized || placement.full_screen {
            return previous.map(|f| Frame { maximized, ..f });
        }
        Some(Frame { position: placement.position, size: placement.content_size, maximized })
    }
}

/// Whether a window at `placement` shows too little of itself on its display to be grabbed:
/// a frame saved on a monitor that is gone. The display is the one winit reports the window
/// on, or the primary one when it overlaps none.
pub fn off_screen(placement: &rune::WindowPlacement) -> bool {
    const VISIBLE: i64 = 64;
    let Some(((dx, dy), (dw, dh))) = placement.display else { return false };
    let (x, y) = placement.position;
    let (w, h) = placement.content_size;
    let overlap = |a: i32, len_a: u32, b: i32, len_b: u32| {
        let start = i64::from(a.max(b));
        let end = (i64::from(a) + i64::from(len_a)).min(i64::from(b) + i64::from(len_b));
        end - start
    };
    overlap(x, w, dx, dw) < VISIBLE || overlap(y, h, dy, dh) < VISIBLE
}

/// Centres a window whose restored frame lands off every display. Call once it exists.
pub fn keep_on_screen(window: &WindowHandle) {
    if let Some(placement) = window.placement().filter(off_screen) {
        log::info!("[window] saved frame at {:?} is off screen; centring", placement.position);
        window.center();
    }
}

pub fn load(path: &Path) -> Option<Frame> {
    let text = std::fs::read_to_string(path).ok()?;
    let frame: Frame = serde_json::from_str(&text).ok()?;
    if frame.sane() {
        Some(frame)
    } else {
        log::warn!("[window] ignoring an unusable saved frame {frame:?}");
        None
    }
}

/// The options the window opens with: hidden, at its minimum, and where `saved` was.
pub fn options(saved: Option<&Frame>) -> WindowOptions {
    WindowOptions {
        visible: false,
        min_size: Some(MIN_SIZE),
        position: saved.map(|f| f.position),
        physical_size: saved.map(|f| f.size),
        ..WindowOptions::default()
    }
}

pub fn chrome() -> Chrome {
    Chrome { full_size_content: true, traffic_lights: Some(TRAFFIC_LIGHTS), drag_regions: true }
}

/// The window, once it exists: the close and quit hooks read its placement from here.
static WINDOW: Mutex<Option<WindowHandle>> = Mutex::new(None);
static FRAME_FILE: Mutex<Option<std::path::PathBuf>> = Mutex::new(None);

pub fn track(path: &Path) {
    *FRAME_FILE.lock().unwrap_or_else(|e| e.into_inner()) = Some(path.to_path_buf());
}

pub fn created(window: &WindowHandle) {
    *WINDOW.lock().unwrap_or_else(|e| e.into_inner()) = Some(window.clone());
}

pub fn handle() -> Option<WindowHandle> {
    WINDOW.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

/// Saves where the window is. Call on the UI thread (the close and quit hooks).
pub fn save() {
    let Some(path) = FRAME_FILE.lock().unwrap_or_else(|e| e.into_inner()).clone() else { return };
    let Some(placement) = handle().and_then(|w| w.placement()) else {
        log::debug!("[window] no placement to save");
        return;
    };
    let Some(frame) = Frame::of(&placement, load(&path)) else { return };
    match serde_json::to_vec_pretty(&frame).map_err(std::io::Error::other).and_then(|json| {
        crate::launcher::record::write_private(&path, &json)
    }) {
        Ok(()) => log::info!("[window] saved frame {frame:?}"),
        Err(e) => log::warn!("[window] cannot save the frame: {e}"),
    }
}

#[cfg(test)]
#[path = "window_tests.rs"]
mod tests;
