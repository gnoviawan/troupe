//! Screen-edge indicator shown while an agent drives this computer.
//!
//! The desktop app draws a glow around every monitor plus a label naming the
//! agent ([`crate::comu_screen_overlay`]). This module holds the runtime-neutral
//! port and the bookkeeping, so `DesktopComuHost` never holds a Tauri handle
//! (`scripts/check-server-no-gui-libs.sh`).

use std::collections::HashMap;
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
    /// Held across each decision and its port call, so overlapping session
    /// changes reach the port in the order they were decided.
    inner: Mutex<Inner>,
}

#[derive(Default)]
struct Inner {
    /// Session id -> display name of the agent driving it.
    driving: HashMap<String, String>,
    /// The app policy's `screen_indicator_enabled` as of the last acting call.
    enabled: bool,
    /// Whether `show` was the last call made on the port.
    shown: bool,
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
        self.port
            .get()
            .map(|port| port.window_ids())
            .unwrap_or_default()
    }

    /// `session_id` drove the computer for `agent_name`; `enabled` is the
    /// current policy value.
    pub fn session_driving(&self, session_id: &str, agent_name: &str, enabled: bool) {
        let mut inner = self.inner.lock();
        inner.enabled = enabled;
        inner
            .driving
            .insert(session_id.to_string(), agent_name.to_string());
        self.refresh(&mut inner);
    }

    /// `session_id` stopped driving: turn end, Stop, or close.
    pub fn session_released(&self, session_id: &str) {
        let mut inner = self.inner.lock();
        if inner.driving.remove(session_id).is_some() {
            self.refresh(&mut inner);
        }
    }

    /// Apply the current sessions to the port. The port only schedules work,
    /// so calling it under the lock does not block.
    fn refresh(&self, inner: &mut Inner) {
        let Some(port) = self.port.get() else {
            return;
        };
        let label = if inner.enabled {
            let mut names: Vec<String> = inner.driving.values().cloned().collect();
            names.sort();
            indicator_label(&names)
        } else {
            None
        };
        match label {
            Some(label) => {
                if !inner.shown {
                    inner.shown = true;
                    log::info!("[comu] screen indicator shown");
                }
                port.show(&label);
            }
            None => {
                if inner.shown {
                    inner.shown = false;
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
