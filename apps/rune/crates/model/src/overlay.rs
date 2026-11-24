//! The stack of open overlays (sheets, dialogs, the mobile drawer), so back
//! and Esc close the topmost one instead of leaving the app. Port of the
//! store in `src/renderer/stores/overlay-stack-store.ts`.
//!
//! Each overlay's close belongs to its owner (dirty guards, confirm flows),
//! so the model holds ids only: closing the topmost returns a
//! [`Effect::CloseOverlay`] and the owner unregisters when it has closed.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::Effect;

/// Overlay-stack id of the settings modal.
pub const SETTINGS_MODAL_OVERLAY_ID: &str = "settings-modal";
/// Prefix of each new-project modal's overlay-stack id.
pub const NEW_PROJECT_MODAL_OVERLAY_PREFIX: &str = "new-project-modal:";

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct State {
    /// Open overlays, oldest first; the last is topmost.
    pub stack: Vec<String>,
    /// The mobile web shell is showing; it gates mobile-only registrations.
    pub mobile_shell: bool,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Event {
    SetMobileShell(bool),
    /// Mark an overlay open and topmost; a repeat is a no-op.
    RegisterOverlay(String),
    UnregisterOverlay(String),
    /// Ask the topmost overlay's owner to close it.
    CloseTopmostOverlay,
}

/// The effects of a step, and whether an overlay was asked to close.
#[derive(Debug, Default, PartialEq)]
pub struct Out {
    pub effects: Vec<Effect>,
    pub closed: Option<bool>,
}

pub fn step(state: &mut State, event: Event) -> Out {
    let mut out = Out::default();
    match event {
        Event::SetMobileShell(active) => state.mobile_shell = active,
        Event::RegisterOverlay(id) => {
            if !state.stack.contains(&id) {
                state.stack.push(id);
            }
        }
        Event::UnregisterOverlay(id) => state.stack.retain(|open| *open != id),
        Event::CloseTopmostOverlay => {
            let top = state.stack.last().cloned();
            out.closed = Some(top.is_some());
            if let Some(id) = top {
                out.effects.push(Effect::CloseOverlay { id });
            }
        }
    }
    out
}

/// How many overlay sentinels a `history.state` value stands for: its
/// positive integer `termulOverlayDepth` (1 when absent) when
/// `termulOverlay` is `true`, else 0.
#[must_use]
pub fn sentinel_depth(history_state: &Value) -> u64 {
    let Some(record) = history_state.as_object() else {
        return 0;
    };
    if record.get("termulOverlay") != Some(&Value::Bool(true)) {
        return 0;
    }
    match record.get("termulOverlayDepth").and_then(Value::as_f64) {
        Some(depth) if depth.fract() == 0.0 && depth > 0.0 => depth as u64,
        _ => 1,
    }
}
