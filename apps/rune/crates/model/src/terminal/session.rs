//! A terminal's shell session as the command editor sees it (`lib/command-lifecycle/transition.ts`
//! and `screen-mode.ts`): which surface takes input, the shell's phase, and the prompt the
//! editor was mounted for.
//!
//! The host injects shell integration and reports `InitShell`, `PromptReady`, `CommandStarted`
//! and `CommandFinished` frames for each PTY. Only an authenticated prompt the host suppressed,
//! on an empty line, mounts the editor; anything the reducer cannot trust falls back to `raw`,
//! where every key goes to the PTY.

use serde::{Deserialize, Serialize};

/// Which surface takes input. `Editor` is mounted only for a trusted suppressed prompt.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Mode {
    Editor,
    #[default]
    #[serde(rename = "PTY")]
    Pty,
    #[serde(rename = "raw")]
    Raw,
    #[serde(rename = "exited")]
    Exited,
}

/// The shell's phase. A pending completion stays inside `AtPrompt`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Phase {
    #[default]
    Unknown,
    AtPrompt,
    Submitting,
    Executing,
    Terminated,
}

/// One terminal's session identity, shell lifecycle and editor mount (`SessionSnapshot`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub session_id: Option<String>,
    pub nonce: Option<String>,
    pub registered: bool,
    pub alive: bool,
    pub trusted: bool,
    pub mode: Mode,
    pub phase: Phase,
    pub prompt_generation: Option<u64>,
    pub last_exit_code: Option<i32>,
    pub committed_submit: bool,
    pub completion_pending: bool,
    pub alternate_screen: bool,
    /// A mountable prompt that arrived while the alternate screen was still on. `PromptReady`
    /// can win the race against the terminal parsing the screen's exit, so the leave mounts it.
    pub held_prompt_generation: Option<u64>,
    /// The shell's folder at its last prompt.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
}

impl Default for Session {
    fn default() -> Self {
        Session {
            session_id: None,
            nonce: None,
            registered: false,
            alive: true,
            trusted: false,
            mode: Mode::Pty,
            phase: Phase::Unknown,
            prompt_generation: None,
            last_exit_code: None,
            committed_submit: false,
            completion_pending: false,
            alternate_screen: false,
            held_prompt_generation: None,
            cwd: None,
        }
    }
}

/// A `PromptReady` frame's fields.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
pub struct Prompt {
    #[serde(default)]
    pub nonce: Option<String>,
    #[serde(default)]
    pub prompt_generation: Option<u64>,
    #[serde(default)]
    pub line_empty: Option<bool>,
    #[serde(default)]
    pub prompt_suppressed: Option<bool>,
    #[serde(default)]
    pub cwd: Option<String>,
}

/// What the reducer reacts to: the host's lifecycle frames and local terminal signals.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(tag = "type")]
pub enum Event {
    PromptReady {
        #[serde(default, rename = "sessionId")]
        session_id: Option<String>,
        frame: Prompt,
    },
    CommandStarted {
        #[serde(default, rename = "sessionId")]
        session_id: Option<String>,
        #[serde(default)]
        nonce: Option<String>,
    },
    CommandFinished {
        #[serde(default, rename = "sessionId")]
        session_id: Option<String>,
        #[serde(default)]
        nonce: Option<String>,
        #[serde(default)]
        exit_code: Option<i32>,
    },
    /// Shell integration that is not Troupe's own; it changes nothing.
    Osc133,
    IntegrationLost,
    ProcessExit,
    CompletionPending,
    Desync,
    AlternateScreenEnter,
    AlternateScreenLeave,
    Elapsed,
}

/// What one event did (`SessionReduction`). `Kept` is the reducer handing its input back
/// untouched, which the store reads as "this prompt changed nothing".
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    Kept,
    Changed,
    /// The session fell back to `raw`; the detail is for the boundary log.
    Raw(&'static str),
}

fn fallback_raw(s: &mut Session, detail: &'static str) -> Outcome {
    s.mode = Mode::Raw;
    s.phase = Phase::Unknown;
    s.trusted = false;
    s.completion_pending = false;
    s.held_prompt_generation = None;
    Outcome::Raw(detail)
}

fn submitting_transaction(s: &Session) -> bool {
    s.phase == Phase::Submitting && s.trusted && s.mode == Mode::Pty
}

fn newer_prompt(s: &Session, generation: u64) -> bool {
    s.prompt_generation.is_none_or(|g| generation > g)
}

/// Every editor condition except the alternate screen, which may clear later.
fn mountable(s: &Session, frame: &Prompt, generation: u64) -> bool {
    frame.prompt_suppressed == Some(true)
        && frame.line_empty == Some(true)
        && newer_prompt(s, generation)
        && !(s.committed_submit && s.phase != Phase::Executing)
}

fn open_editor(s: &mut Session, generation: u64) {
    s.mode = Mode::Editor;
    s.phase = Phase::AtPrompt;
    s.trusted = true;
    s.committed_submit = false;
    s.completion_pending = false;
    s.prompt_generation = Some(generation);
    s.held_prompt_generation = None;
}

/// A live registered session, the event's session id, and an explicit matching nonce.
#[must_use]
pub fn authenticated(s: &Session, session_id: Option<&str>, nonce: Option<&str>) -> bool {
    s.alive
        && s.registered
        && session_id.is_some()
        && session_id == s.session_id.as_deref()
        && nonce.is_some()
        && nonce == s.nonce.as_deref()
}

fn reduce_prompt(s: &mut Session, frame: &Prompt) -> Outcome {
    let Some(generation) = frame.prompt_generation else { return Outcome::Kept };
    if mountable(s, frame, generation) {
        if s.alternate_screen {
            s.held_prompt_generation = Some(generation);
        } else {
            open_editor(s, generation);
        }
        return Outcome::Changed;
    }
    let shown = frame.prompt_suppressed != Some(true) || frame.line_empty != Some(true);
    if submitting_transaction(s) && newer_prompt(s, generation) && shown {
        return fallback_raw(s, "mode=raw reason=unsuppressed_prompt");
    }
    // A newer prompt that cannot mount supersedes a held one.
    if s.held_prompt_generation.is_some_and(|held| generation > held) {
        s.held_prompt_generation = None;
        return Outcome::Changed;
    }
    Outcome::Kept
}

fn reduce_started(s: &mut Session) {
    if s.committed_submit || submitting_transaction(s) {
        s.mode = Mode::Pty;
        s.phase = Phase::Executing;
        s.committed_submit = true;
        s.completion_pending = false;
    }
    s.held_prompt_generation = None;
}

fn reduce_finished(s: &mut Session, exit_code: Option<i32>) {
    if s.mode == Mode::Editor && !s.committed_submit {
        return;
    }
    if s.committed_submit {
        s.phase = Phase::Executing;
        s.completion_pending = false;
    }
    if exit_code.is_some() {
        s.last_exit_code = exit_code;
    }
}

/// A trusted editor mounted for a prompt and not yet handed to a command.
fn mounted_prompt(s: &Session) -> Option<u64> {
    (s.mode == Mode::Editor && s.phase == Phase::AtPrompt && s.trusted && !s.committed_submit)
        .then_some(s.prompt_generation)
        .flatten()
}

/// The alternate screen always takes the PTY. The mounted prompt is held, because
/// `PromptReady` can mount the editor before the terminal parses a fast `CSI ? 1049 h` ...
/// `CSI ? 1049 l` that came before it; the leave mounts it again unless a command starts or a
/// newer prompt replaces it first.
fn enter_alternate_screen(s: &mut Session) {
    if let Some(generation) = mounted_prompt(s) {
        s.held_prompt_generation = Some(generation);
    }
    s.alternate_screen = true;
    s.mode = Mode::Pty;
    s.completion_pending = false;
}

/// Leaving the alternate screen mounts the editor only for a held prompt that was already
/// mountable apart from the screen.
fn leave_alternate_screen(s: &mut Session) {
    let held = s.held_prompt_generation.take();
    s.alternate_screen = false;
    let Some(held) = held else { return };
    let remount = Some(held) == s.prompt_generation && s.phase == Phase::AtPrompt;
    if remount || newer_prompt(s, held) {
        open_editor(s, held);
    }
}

/// Applies one event (`reduceSession`). Only an authenticated suppressed prompt mounts the
/// editor.
pub fn reduce(s: &mut Session, event: &Event) -> Outcome {
    if !s.alive || s.mode == Mode::Exited {
        return Outcome::Kept;
    }
    let before = s.clone();
    match event {
        Event::PromptReady { session_id, frame } => {
            if authenticated(s, session_id.as_deref(), frame.nonce.as_deref()) {
                return reduce_prompt(s, frame);
            }
        }
        Event::CommandStarted { session_id, nonce } => {
            if authenticated(s, session_id.as_deref(), nonce.as_deref()) {
                reduce_started(s);
            }
        }
        Event::CommandFinished { session_id, nonce, exit_code } => {
            if authenticated(s, session_id.as_deref(), nonce.as_deref()) {
                reduce_finished(s, *exit_code);
            }
        }
        Event::ProcessExit => {
            s.mode = Mode::Exited;
            s.alive = false;
            s.phase = Phase::Terminated;
            s.trusted = false;
            s.completion_pending = false;
            s.held_prompt_generation = None;
        }
        Event::Desync => return fallback_raw(s, "mode=raw reason=desync"),
        Event::IntegrationLost => return fallback_raw(s, "mode=raw reason=integration_lost"),
        Event::AlternateScreenEnter => enter_alternate_screen(s),
        Event::AlternateScreenLeave => leave_alternate_screen(s),
        Event::CompletionPending => {
            let open = s.trusted && s.mode == Mode::Editor && s.phase == Phase::AtPrompt;
            if open && !s.committed_submit && !s.alternate_screen {
                s.completion_pending = true;
            }
        }
        Event::Osc133 | Event::Elapsed => {}
    }
    if *s == before { Outcome::Kept } else { Outcome::Changed }
}

/// Follows the terminal's alternate screen (`applyScreenMode`): entering or leaving it, when it
/// changed.
pub fn apply_screen(s: &mut Session, alternate_screen: bool) -> Outcome {
    match (alternate_screen, s.alternate_screen) {
        (true, false) => reduce(s, &Event::AlternateScreenEnter),
        (false, true) => reduce(s, &Event::AlternateScreenLeave),
        _ => Outcome::Kept,
    }
}
