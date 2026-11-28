//! Desktop windows for the Computer Use screen-edge indicator.
//!
//! One transparent, click-through, always-on-top window per monitor shows a
//! glow along the screen edges and a label naming the agent
//! (`comu-overlay.html`). The windows have no capability and no IPC: Rust
//! pushes state with `eval`, and `capabilities/default.json` only matches
//! `main`.
//!
//! Lives outside `crate::comu` because it holds an `AppHandle`; the Comu host
//! only sees the [`ComuScreenIndicator`] port (`scripts/check-server-no-gui-libs.sh`).

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use serde_json::json;
use tauri::{AppHandle, Manager, Monitor, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

use crate::comu::indicator::ComuScreenIndicator;

const LABEL_PREFIX: &str = "comu-overlay-";
const PAGE: &str = "comu-overlay.html";
/// Matches the exit transition in `src/renderer/comu-overlay/overlay.css`.
const EXIT_SETTLE: Duration = Duration::from_millis(220);

#[derive(Clone)]
struct OverlayWindow {
    window: WebviewWindow,
    native_id: Option<i64>,
}

#[derive(Default)]
struct Inner {
    windows: Mutex<Vec<OverlayWindow>>,
    /// Bumped by every show and hide; stale work bails out.
    generation: AtomicU64,
    /// Serializes window creation, updates and teardown.
    work: tokio::sync::Mutex<()>,
    wayland_logged: AtomicBool,
}

/// The desktop [`ComuScreenIndicator`].
pub struct ScreenOverlay {
    app: AppHandle,
    inner: Arc<Inner>,
}

impl ScreenOverlay {
    #[must_use]
    pub fn new(app: AppHandle) -> Arc<Self> {
        Arc::new(Self {
            app,
            inner: Arc::default(),
        })
    }
}

impl ComuScreenIndicator for ScreenOverlay {
    fn show(&self, label: &str) {
        let generation = self.inner.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let inner = Arc::clone(&self.inner);
        let app = self.app.clone();
        let state = json!({ "on": true, "label": label });
        tauri::async_runtime::spawn(async move {
            let _work = inner.work.lock().await;
            if inner.generation.load(Ordering::SeqCst) != generation {
                return;
            }
            let existing = inner.windows.lock().clone();
            if existing.is_empty() {
                open_windows(&app, &inner, &state).await;
            } else {
                for overlay in &existing {
                    push_state(&overlay.window, &state);
                }
            }
        });
    }

    fn hide(&self) {
        let generation = self.inner.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let inner = Arc::clone(&self.inner);
        tauri::async_runtime::spawn(async move {
            {
                let _work = inner.work.lock().await;
                if inner.generation.load(Ordering::SeqCst) != generation {
                    return;
                }
                let state = json!({ "on": false });
                for overlay in inner.windows.lock().iter() {
                    push_state(&overlay.window, &state);
                }
            }
            // Let the fade finish, unless a show takes over meanwhile.
            tokio::time::sleep(EXIT_SETTLE).await;
            let _work = inner.work.lock().await;
            if inner.generation.load(Ordering::SeqCst) != generation {
                return;
            }
            let closing = std::mem::take(&mut *inner.windows.lock());
            for overlay in closing {
                if let Err(error) = overlay.window.destroy() {
                    log::warn!(
                        "[comu-overlay] closing {} failed: {error}",
                        overlay.window.label()
                    );
                }
            }
        });
    }

    fn window_ids(&self) -> Vec<i64> {
        self.inner
            .windows
            .lock()
            .iter()
            .filter_map(|overlay| overlay.native_id)
            .collect()
    }
}

async fn open_windows(app: &AppHandle, inner: &Inner, state: &serde_json::Value) {
    if crate::window_visibility::is_wayland_session() {
        if !inner.wayland_logged.swap(true, Ordering::SeqCst) {
            log::info!(
                "[comu-overlay] Wayland session: the screen-edge indicator is not shown (no window positioning)"
            );
        }
        return;
    }
    let monitors = match app.available_monitors() {
        Ok(monitors) if !monitors.is_empty() => monitors,
        Ok(_) => {
            log::warn!("[comu-overlay] no monitors reported; indicator not shown");
            return;
        }
        Err(error) => {
            log::warn!("[comu-overlay] listing monitors failed: {error}");
            return;
        }
    };
    let init_script = format!("window.__comuOverlayState = {state};");
    let mut opened = Vec::with_capacity(monitors.len());
    for (index, monitor) in monitors.iter().enumerate() {
        match open_window(app, index, monitor, &init_script).await {
            Ok(overlay) => opened.push(overlay),
            Err(error) => log::warn!("[comu-overlay] window for monitor {index} failed: {error}"),
        }
    }
    log::info!(
        "[comu-overlay] opened {} of {} monitor windows (native ids {:?})",
        opened.len(),
        monitors.len(),
        opened.iter().map(|o| o.native_id).collect::<Vec<_>>()
    );
    *inner.windows.lock() = opened;
}

async fn open_window(
    app: &AppHandle,
    index: usize,
    monitor: &Monitor,
    init_script: &str,
) -> tauri::Result<OverlayWindow> {
    let label = format!("{LABEL_PREFIX}{index}");
    // A window left over from an interrupted teardown would make the build fail.
    if let Some(stale) = app.get_webview_window(&label) {
        let _ = stale.destroy();
    }
    let window = WebviewWindowBuilder::new(app, &label, WebviewUrl::App(PAGE.into()))
        .title("Troupe Computer Use indicator")
        .initialization_script(init_script)
        .visible(false)
        .transparent(true)
        .background_color(tauri::window::Color(0, 0, 0, 0))
        .decorations(false)
        .shadow(false)
        .resizable(false)
        .always_on_top(true)
        .visible_on_all_workspaces(true)
        .skip_taskbar(true)
        .focused(false)
        .focusable(false)
        .content_protected(true)
        .build()?;
    place_on_monitor(&window, monitor)?;
    window.set_ignore_cursor_events(true)?;
    let native_id = present(&window).await;
    Ok(OverlayWindow { window, native_id })
}

/// Cover `monitor` exactly. macOS positions in points per monitor; the other
/// platforms take the monitor's physical rectangle as is.
fn place_on_monitor(window: &WebviewWindow, monitor: &Monitor) -> tauri::Result<()> {
    let position = *monitor.position();
    let size = *monitor.size();
    if cfg!(target_os = "macos") {
        let scale = monitor.scale_factor();
        window.set_position(position.to_logical::<f64>(scale))?;
        window.set_size(size.to_logical::<f64>(scale))?;
    } else {
        window.set_position(position)?;
        window.set_size(size)?;
    }
    Ok(())
}

fn push_state(window: &WebviewWindow, state: &serde_json::Value) {
    let script = format!("window.__comuOverlay && window.__comuOverlay({state});");
    if let Err(error) = window.eval(script) {
        log::warn!("[comu-overlay] updating {} failed: {error}", window.label());
    }
}

/// Show the window without activating Troupe, and return the id the Comu
/// engine knows it by (`WindowOwnerInfo::window_id`).
#[cfg(target_os = "macos")]
async fn present(window: &WebviewWindow) -> Option<i64> {
    // Above the menu bar (`NSMainMenuWindowLevel` is 24), on every Space and
    // beside full-screen apps, and out of the window cycle.
    const STATUS_WINDOW_LEVEL: isize = 25;
    const BEHAVIOR: usize = (1 << 0) // canJoinAllSpaces
        | (1 << 4) // stationary
        | (1 << 6) // ignoresCycle
        | (1 << 8); // fullScreenAuxiliary
    let ns_window = match window.ns_window() {
        Ok(pointer) => pointer as usize,
        Err(error) => {
            log::warn!("[comu-overlay] no NSWindow: {error}");
            return None;
        }
    };
    let (tx, rx) = tokio::sync::oneshot::channel();
    let scheduled = window.run_on_main_thread(move || {
        use objc2::msg_send;
        use objc2::runtime::AnyObject;
        let ns_window = ns_window as *mut AnyObject;
        // SAFETY: `ns_window` is the live NSWindow of this webview window,
        // used on the main thread. Every selector exists on NSWindow.
        let number: isize = unsafe {
            let _: () = msg_send![ns_window, setLevel: STATUS_WINDOW_LEVEL];
            let _: () = msg_send![ns_window, setCollectionBehavior: BEHAVIOR];
            let _: () = msg_send![ns_window, orderFrontRegardless];
            msg_send![ns_window, windowNumber]
        };
        let _ = tx.send(number);
    });
    if let Err(error) = scheduled {
        log::warn!("[comu-overlay] presenting the window failed: {error}");
        return None;
    }
    rx.await.ok().map(|number| number as i64)
}

#[cfg(not(target_os = "macos"))]
async fn present(window: &WebviewWindow) -> Option<i64> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    if let Err(error) = window.show() {
        log::warn!("[comu-overlay] showing {} failed: {error}", window.label());
    }
    let handle = match window.window_handle() {
        Ok(handle) => handle.as_raw(),
        Err(error) => {
            log::warn!("[comu-overlay] no native handle: {error}");
            return None;
        }
    };
    match handle {
        RawWindowHandle::Win32(handle) => Some(handle.hwnd.get() as i64),
        RawWindowHandle::Xlib(handle) => i64::try_from(handle.window).ok(),
        RawWindowHandle::Xcb(handle) => Some(i64::from(handle.window.get())),
        _ => None,
    }
}
