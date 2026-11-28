//! Screen-edge indicator shown while an agent drives this computer.
//!
//! The desktop app draws a glow around every monitor plus a label naming the
//! agent ([`crate::comu_screen_overlay`]). This module holds the runtime-neutral
//! port and the bookkeeping, so `DesktopComuHost` never holds a Tauri handle
//! (`scripts/check-server-no-gui-libs.sh`).

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};

use parking_lot::Mutex;

/// Draws or removes the screen-edge indicator. Calls must not block: the
/// desktop implementation hands the work to the main thread.
pub trait ComuScreenIndicator: Send + Sync {
    /// Show the indicator on every monitor, or update its label.
    fn show(&self, label: &str);
    /// Fade the indicator out and close its windows.
    fn hide(&self);
    /// Native ids of the indicator's windows. The self-target guard skips them
    /// in its desktop hit-test: they are click-through and full-screen.
    fn window_ids(&self) -> Vec<i64>;
}

/// The label for the sessions driving the computer, by agent name.
/// `None` when none drives it.
#[must_use]
pub fn indicator_label(agent_names: &[String]) -> Option<String> {
    match agent_names {
        [] => None,
        [name] => Some(format!("{name} is using this computer")),
        many => Some(format!("{} agents are using this computer", many.len())),
    }
}

/// Which sessions drive the computer right now, and whether the user wants
/// the indicator. Owned by `DesktopComuHost`.
#[derive(Default)]
pub struct IndicatorState {
    port: OnceLock<Arc<dyn ComuScreenIndicator>>,
    /// Session id -> display name of the agent driving it.
    driving: Mutex<HashMap<String, String>>,
    /// The app policy's `screen_indicator_enabled` as of the last tool call.
    enabled: AtomicBool,
    /// Whether `show` was the last call made on the port.
    shown: AtomicBool,
}

impl IndicatorState {
    /// Install the desktop port. Only the first call takes effect.
    pub fn install(&self, port: Arc<dyn ComuScreenIndicator>) {
        if self.port.set(port).is_err() {
            log::warn!("[comu] screen indicator already installed; ignoring a second port");
        }
    }

    /// Native ids of the indicator windows (empty without a port).
    pub fn window_ids(&self) -> Vec<i64> {
        self.port.get().map(|port| port.window_ids()).unwrap_or_default()
    }

    /// `session_id` drove the computer for `agent_name`; `enabled` is the
    /// current policy value.
    pub fn session_driving(&self, session_id: &str, agent_name: &str, enabled: bool) {
        self.enabled.store(enabled, Ordering::SeqCst);
        self.driving
            .lock()
            .insert(session_id.to_string(), agent_name.to_string());
        self.refresh();
    }

    /// `session_id` stopped driving: turn end, Stop, or close.
    pub fn session_released(&self, session_id: &str) {
        let removed = self.driving.lock().remove(session_id).is_some();
        if removed {
            self.refresh();
        }
    }

    fn refresh(&self) {
        let Some(port) = self.port.get() else {
            return;
        };
        let label = if self.enabled.load(Ordering::SeqCst) {
            let mut names: Vec<String> = self.driving.lock().values().cloned().collect();
            names.sort();
            indicator_label(&names)
        } else {
            None
        };
        match label {
            Some(label) => {
                if !self.shown.swap(true, Ordering::SeqCst) {
                    log::info!("[comu] screen indicator shown");
                }
                port.show(&label);
            }
            None => {
                if self.shown.swap(false, Ordering::SeqCst) {
                    log::info!("[comu] screen indicator hidden");
                    port.hide();
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "indicator_tests.rs"]
mod tests;
