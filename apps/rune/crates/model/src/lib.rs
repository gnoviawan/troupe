//! Troupe's client state for the Rune shell, with no UI and no I/O.
//!
//! Each store is a state type and a `step` function that applies one event
//! and returns the [`Effect`]s the caller carries out (send a host command,
//! move the route, write the device record, log). The TypeScript stores in
//! `src/renderer/stores/` are the reference: `tests/vectors.rs` replays what
//! `tools/parity/vectors/` recorded from them. `README.md` says which state
//! stays on the client and which the server already owns.

pub mod acp;
pub mod automations;
pub mod chats;
pub mod editor;
pub mod explorer;
pub mod git;
pub mod overlay;
pub mod persistence;
pub mod prefs;
pub mod projects;
pub mod settings;
pub mod shell_layout;
pub mod shortcuts;
pub mod terminal;
pub mod workspace;

use serde::{Deserialize, Serialize};

/// Where new ids come from. The app passes a random source; a replay passes
/// the ids the TypeScript store drew.
pub trait Ids {
    fn next_id(&mut self) -> String;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Info,
    Warn,
    Error,
}

/// What a step asks the world to do. The caller runs these in order.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Effect {
    /// A boundary log line for the host log.
    Log {
        level: Level,
        source: String,
        message: String,
    },
    /// Show the chat's route. With `replace_if_on`, replace the history entry
    /// when the current route is that chat's.
    NavigateToChat {
        session_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        replace_if_on: Option<String>,
    },
    /// Leave any chat route.
    ClearChatRoute,
    /// Save this device's active project.
    SaveActiveProject { project_id: String },
    /// Send `update_project` with the host fields that changed.
    UpdateProjectOnHost {
        project_id: String,
        patch: projects::HostPatch,
    },
    /// Start the delete flow of one project (canvases first, then the host).
    DeleteProject { project_id: String },
    /// Write the device's `project-prefs` record. On failure, send
    /// `prefsWriteFailed` with `signature` so the next persist retries.
    WritePrefs {
        record: prefs::Record,
        #[serde(skip_serializing)]
        signature: String,
    },
    /// Run the owner's close of an overlay.
    CloseOverlay { id: String },
    /// Write the device's `shell-layout` record: now when `immediate`, else
    /// through the debounced writer.
    WriteShellLayout {
        record: shell_layout::Record,
        immediate: bool,
    },
    /// Forget the chats retained for a project that no longer exists.
    ForgetProjectChats { project_id: String },
    /// Delete one device key from the host store.
    DeleteStoreKey { key: String },
}

impl Effect {
    fn log(level: Level, source: &str, message: String) -> Self {
        Self::Log {
            level,
            source: source.to_owned(),
            message,
        }
    }
}

/// `String.prototype.trim`: JavaScript's whitespace includes U+FEFF and
/// excludes U+0085, unlike Rust's `str::trim`.
fn js_trim(text: &str) -> &str {
    text.trim_matches(|c: char| c == '\u{feff}' || (c.is_whitespace() && c != '\u{85}'))
}

/// A number the way JavaScript prints it in a template string, for log text.
fn js_number(value: f64) -> String {
    if value.fract() == 0.0 && value.abs() < 1e21 {
        format!("{value:.0}")
    } else {
        format!("{value}")
    }
}
