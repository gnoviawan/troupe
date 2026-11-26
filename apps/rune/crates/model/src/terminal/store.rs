//! Every terminal's session and command draft (`lib/command-lifecycle/session-store.ts`,
//! `draft-store.ts` and `editor-revision.ts`), keyed by terminal id.
//!
//! The host's lifecycle frames go through [`Store::apply`]; the alternate screen through
//! [`Store::apply_screen`]. A submit is a transaction: [`Store::begin_submit`] hands out a
//! [`Token`], the host's reply goes to [`Store::record_submit`] with it, and a token that went
//! stale (the session changed under it) is refused. A draft lives here, not in the editor, so it
//! survives the editor's unmount; an accepted submit or the PTY's exit drops it.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::session::{self, Event, Mode, Outcome, Phase, Prompt, Session};
use crate::Level;

/// A `session_lifecycle` frame's fields (`SessionLifecyclePayload`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
pub struct Lifecycle {
    pub kind: String,
    #[serde(default)]
    pub session_id: Option<String>,
    /// `None` when the frame has no `nonce` key, `Some(None)` for a `null` one: a command
    /// frame without the key is read with the session's own nonce.
    #[serde(default, deserialize_with = "present")]
    pub nonce: Option<Option<String>>,
    #[serde(default)]
    pub prompt_generation: Option<u64>,
    #[serde(default)]
    pub line_empty: Option<bool>,
    #[serde(default)]
    pub prompt_suppressed: Option<bool>,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub exit_code: Option<i32>,
}

fn present<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<Option<String>>, D::Error> {
    Option::<String>::deserialize(d).map(Some)
}

/// The host's answer to a submit or a completion (`SessionRpcResult`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RpcResult {
    /// The draft reached the shell.
    Accepted,
    /// Refused before anything was written.
    PreWrite,
    /// Some of it may have been written.
    Partial,
    NotStarted,
}

/// One submit in flight.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Token {
    pub session_id: String,
    pub nonce: String,
    pub epoch: u64,
    pub committed: bool,
}

/// A log line for the host log.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Log {
    pub level: Level,
    pub message: String,
}

/// A terminal's draft, and how many times it was dropped (`invalidationVersion`), so an editor
/// that read the draft before a drop does not write its stale text back.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Draft {
    pub text: String,
    pub version: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Store {
    pub sessions: BTreeMap<String, Session>,
    pub submissions: BTreeMap<String, Token>,
    pub drafts: BTreeMap<String, Draft>,
    /// Edits made in each terminal's editor, the revision a completion is asked for.
    pub revisions: BTreeMap<String, u64>,
    epoch: u64,
    /// What happened, for the host log; the caller drains it.
    #[serde(skip)]
    pub logs: Vec<Log>,
}

impl Store {
    /// The terminal's session; a terminal with no frames yet reads as a fresh PTY session.
    #[must_use]
    pub fn read(&self, id: &str) -> Session {
        self.sessions.get(id).cloned().unwrap_or_default()
    }

    #[must_use]
    pub fn draft(&self, id: &str) -> &str {
        self.drafts.get(id).map_or("", |d| d.text.as_str())
    }

    #[must_use]
    pub fn draft_version(&self, id: &str) -> u64 {
        self.drafts.get(id).map_or(0, |d| d.version)
    }

    /// The editor's text changed (`offer`): kept only while the editor is mounted.
    pub fn offer(&mut self, id: &str, mode: Mode, text: &str) {
        if mode == Mode::Editor {
            self.drafts.entry(id.to_owned()).or_default().text = text.to_owned();
        }
    }

    fn drop_draft(&mut self, id: &str) {
        let draft = self.drafts.entry(id.to_owned()).or_default();
        draft.text.clear();
        draft.version += 1;
    }

    #[must_use]
    pub fn revision(&self, id: &str) -> u64 {
        self.revisions.get(id).copied().unwrap_or(0)
    }

    pub fn bump_revision(&mut self, id: &str) -> u64 {
        let revision = self.revisions.entry(id.to_owned()).or_default();
        *revision += 1;
        *revision
    }

    fn log(&mut self, level: Level, message: String) {
        self.logs.push(Log { level, message });
    }

    fn write(&mut self, id: &str, next: Session) {
        if let Some(token) = self.submissions.get(id) {
            let stale = !next.alive
                || !next.trusted
                || next.mode == Mode::Raw
                || next.session_id.as_deref() != Some(token.session_id.as_str())
                || next.nonce.as_deref() != Some(token.nonce.as_str());
            if stale {
                self.submissions.remove(id);
            }
        }
        self.sessions.insert(id.to_owned(), next);
    }

    fn commit_submission(&mut self, id: &str, current: &Session) {
        let Some(token) = self.submissions.get_mut(id) else { return };
        if current.committed_submit
            || token.committed
            || Some(token.session_id.as_str()) != current.session_id.as_deref()
            || Some(token.nonce.as_str()) != current.nonce.as_deref()
        {
            return;
        }
        token.committed = true;
        self.drop_draft(id);
    }

    /// A newer editor prompt after a submitted command accepts it and drops its draft.
    fn settle_prompt_submission(&mut self, id: &str, current: &Session, next: &Session) {
        let submitted = current.phase == Phase::Submitting || (current.phase == Phase::Executing && current.committed_submit);
        if submitted && next.mode == Mode::Editor && next.prompt_generation != current.prompt_generation {
            self.commit_submission(id, current);
            self.drop_draft(id);
        }
    }

    /// The editor gave the terminal back to the PTY (`leaveEditor`).
    pub fn leave_editor(&mut self, id: &str) {
        let current = self.read(id);
        if current.mode != Mode::Editor || current.phase != Phase::AtPrompt {
            return;
        }
        self.write(id, Session { mode: Mode::Pty, completion_pending: false, ..current });
        self.log(Level::Info, "editor left: mode=PTY".into());
    }

    /// Starts a submit from a mounted editor; None when the session cannot take one.
    pub fn begin_submit(&mut self, id: &str) -> Option<Token> {
        let current = self.read(id);
        let pending = self.submissions.get(id).is_some_and(|t| !t.committed);
        if !current.registered || !current.alive || !current.trusted || current.mode != Mode::Editor || current.phase != Phase::AtPrompt || pending {
            return None;
        }
        let (Some(session_id), Some(nonce)) = (current.session_id.clone(), current.nonce.clone()) else { return None };
        if session_id.is_empty() || nonce.is_empty() {
            return None;
        }
        self.epoch += 1;
        let token = Token { session_id, nonce, epoch: self.epoch, committed: false };
        self.submissions.insert(id.to_owned(), token.clone());
        self.write(id, Session { mode: Mode::Pty, phase: Phase::Submitting, completion_pending: false, ..current });
        self.log(Level::Info, "submit begun: phase=Submitting".into());
        Some(token)
    }

    /// Whether the submit `epoch` started is still the one in flight (`isSubmitCurrent`).
    #[must_use]
    pub fn is_submit_current(&self, id: &str, epoch: u64) -> bool {
        self.submissions.get(id).is_some_and(|t| t.epoch == epoch && !t.committed)
    }

    /// The host answered the submit `epoch` started. True when the answer applied.
    pub fn record_submit(&mut self, id: &str, result: RpcResult, epoch: u64) -> bool {
        let current = self.read(id);
        if self.submissions.get(id).is_none_or(|t| t.epoch != epoch) {
            self.log(Level::Info, "submit reply ignored: stale transaction".into());
            return false;
        }
        let Some(token) = self.submissions.remove(id) else { return false };
        self.log(Level::Info, format!("submit reply: result={}", result_name(result)));
        match result {
            RpcResult::PreWrite | RpcResult::NotStarted => {
                if current.phase != Phase::Submitting {
                    return false;
                }
                self.write(id, Session { mode: Mode::Editor, phase: Phase::AtPrompt, committed_submit: false, ..current });
                true
            }
            RpcResult::Partial => {
                let mut next = Session { committed_submit: false, ..current };
                session::reduce(&mut next, &Event::Desync);
                self.write(id, next);
                !token.committed
            }
            RpcResult::Accepted => {
                if token.committed {
                    return false;
                }
                self.write(id, Session { committed_submit: true, ..current });
                true
            }
        }
    }

    /// The terminal closed: forget it.
    pub fn forget(&mut self, id: &str) {
        self.submissions.remove(id);
        self.sessions.remove(id);
        self.drop_draft(id);
        self.revisions.remove(id);
    }

    /// The PTY exited: the session ends and its draft goes.
    pub fn process_exited(&mut self, id: &str) {
        let mut next = self.read(id);
        session::reduce(&mut next, &Event::ProcessExit);
        self.revisions.remove(id);
        self.drop_draft(id);
        self.write(id, next);
    }

    fn fallback(&mut self, id: &str, level: Level, message: &str, require_init_shell: bool) {
        let mut next = self.read(id);
        session::reduce(&mut next, &Event::Desync);
        if require_init_shell {
            next.registered = false;
        }
        self.write(id, next);
        self.log(level, message.into());
    }

    /// A local desync: the session falls back to `raw` and waits for a host `InitShell`.
    pub fn invalidate(&mut self, id: &str, level: Level) {
        self.fallback(id, level, "session history discarded: mode=raw", true);
    }

    /// One `session_lifecycle` frame for terminal `id`.
    pub fn apply(&mut self, id: &str, event: &Lifecycle) {
        let current = self.read(id);
        if event.kind == "InitShell" {
            let next = init_shell(current, event);
            self.write(id, next);
            return;
        }
        let Some(session_id) = event.session_id.clone().filter(|s| current.registered && Some(s) == current.session_id.as_ref()) else {
            return;
        };
        match event.kind.as_str() {
            "Desync" => {
                if current.mode != Mode::Raw && event.nonce.as_ref() == Some(&current.nonce) {
                    self.fallback(id, Level::Info, "session desync: mode=raw", false);
                }
            }
            "PromptReady" => {
                let next = reduce_prompt(&session_id, &current, event);
                self.settle_prompt_submission(id, &current, &next);
                self.write(id, next);
            }
            "CommandStarted" => {
                let next = started(&session_id, &current, event);
                if current.phase == Phase::Submitting && next.phase == Phase::Executing {
                    self.commit_submission(id, &current);
                }
                self.write(id, next);
            }
            "CommandFinished" => {
                let nonce = event.nonce.clone().unwrap_or_else(|| current.nonce.clone());
                let mut next = current;
                let finished = Event::CommandFinished { session_id: Some(session_id), nonce, exit_code: event.exit_code };
                session::reduce(&mut next, &finished);
                self.write(id, next);
            }
            _ => {}
        }
    }

    /// The terminal entered or left the alternate screen.
    pub fn apply_screen(&mut self, id: &str, alternate_screen: bool) {
        let current = self.read(id);
        let mut next = current.clone();
        session::apply_screen(&mut next, alternate_screen);
        if next.mode == current.mode && next.alternate_screen == current.alternate_screen {
            return;
        }
        if next.mode == Mode::Editor && current.mode != Mode::Editor {
            // A prompt held during the alternate screen mounts once the leave is parsed. The same
            // generation means the screen was entered over a mounted prompt.
            self.settle_prompt_submission(id, &current, &next);
            let source = if next.prompt_generation == current.prompt_generation { "prompt kept across alternate screen" } else { "held prompt after alternate screen" };
            let generation = next.prompt_generation.map_or_else(|| "null".to_owned(), |g| g.to_string());
            self.log(Level::Info, format!("editor mounted: {source} generation={generation}"));
        }
        self.write(id, next);
    }
}

fn result_name(result: RpcResult) -> &'static str {
    match result {
        RpcResult::Accepted => "Accepted",
        RpcResult::PreWrite => "PreWrite",
        RpcResult::Partial => "Partial",
        RpcResult::NotStarted => "NotStarted",
    }
}

fn init_shell(current: Session, event: &Lifecycle) -> Session {
    // A missing key is `undefined` in the web client, which no stored value equals.
    let changed = event.session_id.is_none() || event.session_id != current.session_id || event.nonce.as_ref() != Some(&current.nonce);
    let base = if current.mode == Mode::Exited || changed { Session::default() } else { current };
    let nonce = event.nonce.clone().flatten();
    Session { registered: event.session_id.is_some(), session_id: event.session_id.clone(), nonce, ..base }
}

fn reduce_prompt(session_id: &str, current: &Session, event: &Lifecycle) -> Session {
    let (Some(generation), Some(cwd)) = (event.prompt_generation, event.cwd.clone()) else { return current.clone() };
    let mut input = current.clone();
    if current.phase == Phase::Submitting && current.trusted && current.mode == Mode::Pty {
        input.committed_submit = false;
    }
    let frame = Prompt {
        nonce: event.nonce.clone().flatten(),
        prompt_generation: Some(generation),
        line_empty: Some(event.line_empty.unwrap_or(false)),
        prompt_suppressed: Some(event.prompt_suppressed.unwrap_or(false)),
        cwd: Some(cwd.clone()),
    };
    if session::reduce(&mut input, &Event::PromptReady { session_id: Some(session_id.to_owned()), frame }) == Outcome::Kept {
        return current.clone();
    }
    input.cwd = Some(cwd);
    // The Submitting override only lets the prompt mount. A prompt held for the alternate screen
    // keeps the accepted submit until the leave mounts it.
    if input.mode == current.mode {
        input.committed_submit = current.committed_submit;
    }
    input
}

fn started(session_id: &str, current: &Session, event: &Lifecycle) -> Session {
    // A host replay can announce a command another client started. It demotes the editor
    // without treating this client's draft as submitted; the reducer keeps an authenticated
    // CommandStarted at an Editor prompt as a no-op, so the demotion is here.
    let explicit = event.nonce.clone().flatten();
    let authentic = session::authenticated(current, Some(session_id), explicit.as_deref());
    if current.mode == Mode::Editor && current.phase == Phase::AtPrompt && authentic {
        return Session { mode: Mode::Pty, phase: Phase::Executing, completion_pending: false, ..current.clone() };
    }
    let nonce = event.nonce.clone().unwrap_or_else(|| current.nonce.clone());
    let mut next = current.clone();
    session::reduce(&mut next, &Event::CommandStarted { session_id: Some(session_id.to_owned()), nonce });
    next
}

/// Whether input may go to the PTY (`canWriteToPty`): never while the editor is mounted, and
/// only Ctrl+C while a submit is in flight.
#[must_use]
pub fn can_write_to_pty(mode: Mode, phase: Phase, data: &str) -> bool {
    match mode {
        Mode::Editor => false,
        Mode::Pty if phase == Phase::Submitting => data == "\u{3}",
        _ => true,
    }
}
