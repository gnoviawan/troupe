//! When the command editor asks the host for completions (`lib/command-lifecycle/completion.ts`,
//! `CompletionScheduler`).
//!
//! Tab asks at once. Typing re-arms a 100 ms idle timer that asks on its own when automatic
//! refresh is on. One request is in flight at a time: an edit while it runs marks a replacement,
//! asked for once the reply lands. A reply counts only when the editor still shows what it was
//! asked for. A submit cancels the request in flight first.

use serde::{Deserialize, Serialize};

use super::session::{Mode, Phase};
use super::words::word_continues_past_cursor;

/// The host completion timeout (1500) plus its drain bound (2000) plus a 500 ms margin.
pub const CLIENT_TIMEOUT_MS: f64 = 1500.0 + 2000.0 + 500.0;
pub const IDLE_MS: f64 = 100.0;

/// What the editor showed when a completion was asked for (`CompletionContext`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Context {
    pub terminal_id: String,
    pub session_id: String,
    pub mode: Mode,
    pub phase: Phase,
    pub draft: String,
    pub buffer_revision: u64,
    /// UTF-16 code units, as the host counts them.
    pub cursor: u64,
    pub prompt_generation: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
}

impl Context {
    fn matches(&self, other: &Context) -> bool {
        self.session_id == other.session_id
            && self.mode == other.mode
            && self.draft == other.draft
            && self.buffer_revision == other.buffer_revision
            && self.cursor == other.cursor
            && self.prompt_generation == other.prompt_generation
            && self.cwd == other.cwd
    }
}

/// The `command_editor_request_completion` payload, without the terminal id.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Request {
    pub request_id: String,
    pub draft: String,
    pub buffer_revision: u64,
    pub cursor: u64,
    pub prompt_generation: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
}

/// A completer's answer and what it was asked for (`CompletionReplyFields`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Completion {
    pub request_id: String,
    pub buffer_revision: u64,
    pub cursor: u64,
    pub prompt_generation: u64,
    pub cwd: String,
    pub output: String,
}

/// The host's reply to a completion request (`CompletionRpcReply`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reply {
    pub result: super::store::RpcResult,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completion: Option<Completion>,
}

/// What the scheduler asks the app to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Effect {
    Request { terminal_id: String, request: Request },
    Cancel { terminal_id: String, request_id: String },
    /// A valid answer: show it.
    Show(Completion),
    Log { name: &'static str, detail: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct InFlight {
    request_id: String,
    context: Context,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Scheduler {
    in_flight: Option<InFlight>,
    latest: Option<Context>,
    replacement: Option<Context>,
    /// When the idle timer fires.
    idle_at: Option<f64>,
    /// When the request in flight times out, and which request it is.
    timeout_at: Option<(f64, String)>,
    requests: u64,
    pub auto_refresh: bool,
}

impl Default for Scheduler {
    fn default() -> Self {
        Scheduler { in_flight: None, latest: None, replacement: None, idle_at: None, timeout_at: None, requests: 0, auto_refresh: true }
    }
}

/// The scheduler's state as the vectors record it.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot<'a> {
    pub in_flight: Option<&'a str>,
    pub latest: Option<&'a Context>,
    pub replacement: Option<&'a Context>,
    pub idle_armed: bool,
    pub timeout_armed: bool,
}

/// JavaScript's `trim() === ''`.
fn blank(text: &str) -> bool {
    text.encode_utf16().all(|u| matches!(u, 0x09..=0x0d | 0x20 | 0xa0 | 0x1680 | 0x2000..=0x200a | 0x2028 | 0x2029 | 0x202f | 0x205f | 0x3000 | 0xfeff))
}

/// A UTF-16 offset as a char offset into `text`, clamped to its end.
fn char_offset(text: &str, units: u64) -> usize {
    let mut seen = 0u64;
    for (i, ch) in text.chars().enumerate() {
        if seen >= units {
            return i;
        }
        seen += ch.len_utf16() as u64;
    }
    text.chars().count()
}

impl Scheduler {
    #[must_use]
    pub fn snapshot(&self) -> Snapshot<'_> {
        Snapshot {
            in_flight: self.in_flight.as_ref().map(|f| f.request_id.as_str()),
            latest: self.latest.as_ref(),
            replacement: self.replacement.as_ref(),
            idle_armed: self.idle_at.is_some(),
            timeout_armed: self.timeout_at.is_some(),
        }
    }

    /// The next time a timer fires.
    #[must_use]
    pub fn deadline(&self) -> Option<f64> {
        match (self.idle_at, &self.timeout_at) {
            (Some(a), Some((b, _))) => Some(a.min(*b)),
            (a, b) => a.or(b.as_ref().map(|(t, _)| *t)),
        }
    }

    /// Tab: ask now, unless a request is in flight. With `showing` the list is already open and
    /// only the context is kept.
    pub fn request(&mut self, context: Context, showing: bool, now: f64, out: &mut Vec<Effect>) {
        self.latest = Some(context.clone());
        if showing {
            return;
        }
        if let Some(active) = &self.in_flight {
            out.push(Effect::Log { name: "completion_tab_ignored", detail: format!("requestId={}", active.request_id) });
            return;
        }
        self.start(context, false, now, out);
    }

    /// The editor's text or caret changed.
    pub fn update(&mut self, context: Context, now: f64, out: &mut Vec<Effect>) {
        self.latest = Some(context.clone());
        if let Some(active) = &self.in_flight {
            if !active.context.matches(&context) {
                out.push(Effect::Log { name: "completion_invalidated", detail: format!("requestId={}", active.request_id) });
                if self.auto_refresh {
                    self.replacement = Some(context);
                }
            }
            return;
        }
        if self.auto_refresh {
            self.idle_at = Some(now + IDLE_MS);
        }
    }

    /// A submit is about to go: cancel the request in flight. Returns its id.
    pub fn prepare_submit(&mut self, out: &mut Vec<Effect>) -> Option<String> {
        self.idle_at = None;
        self.replacement = None;
        self.timeout_at = None;
        let active = self.in_flight.take()?;
        out.push(Effect::Log { name: "completion_cancelled", detail: format!("requestId={} reason=submit", active.request_id) });
        out.push(Effect::Cancel { terminal_id: active.context.terminal_id, request_id: active.request_id.clone() });
        Some(active.request_id)
    }

    /// The session left the prompt, or the editor unmounted: drop everything.
    pub fn invalidate(&mut self, out: &mut Vec<Effect>) {
        self.idle_at = None;
        self.timeout_at = None;
        self.latest = None;
        self.replacement = None;
        if let Some(active) = self.in_flight.take() {
            out.push(Effect::Cancel { terminal_id: active.context.terminal_id, request_id: active.request_id });
        }
    }

    /// Fires the timers due by `now`, earliest first, each at its own time, as the web
    /// client's clock would have fired them.
    pub fn tick(&mut self, now: f64, out: &mut Vec<Effect>) {
        loop {
            let idle = self.idle_at.filter(|&at| at <= now);
            let timeout = self.timeout_at.as_ref().map(|(at, _)| *at).filter(|&at| at <= now);
            match (idle, timeout) {
                (Some(i), Some(t)) if t < i => self.fire_timeout(out),
                (Some(_), _) => self.fire_idle(out),
                (None, Some(_)) => self.fire_timeout(out),
                (None, None) => return,
            }
        }
    }

    fn fire_idle(&mut self, out: &mut Vec<Effect>) {
        let Some(at) = self.idle_at.take() else { return };
        if self.in_flight.is_none()
            && let Some(latest) = self.latest.clone()
        {
            self.start(latest, true, at, out);
        }
    }

    fn fire_timeout(&mut self, out: &mut Vec<Effect>) {
        let Some((at, request_id)) = self.timeout_at.take() else { return };
        if self.in_flight.as_ref().is_some_and(|f| f.request_id == request_id) {
            let active = self.in_flight.take().expect("checked");
            out.push(Effect::Log { name: "completion_timeout", detail: "result=Partial".into() });
            out.push(Effect::Cancel { terminal_id: active.context.terminal_id, request_id });
            self.start_replacement(at, out);
        }
    }

    /// The host answered `request_id`.
    pub fn reply(&mut self, request_id: &str, reply: &Reply, now: f64, out: &mut Vec<Effect>) {
        if self.in_flight.as_ref().is_none_or(|f| f.request_id != request_id) {
            return;
        }
        self.timeout_at = None;
        let active = self.in_flight.take().expect("checked");
        let current = self.latest.as_ref().is_some_and(|latest| active.context.matches(latest));
        if let Some(completion) = &reply.completion
            && current
            && completion.request_id == active.request_id
            && completion.buffer_revision == active.context.buffer_revision
            && completion.cursor == active.context.cursor
            && completion.prompt_generation == active.context.prompt_generation
            && Some(&completion.cwd) == active.context.cwd.as_ref()
        {
            out.push(Effect::Show(completion.clone()));
        }
        if reply.result == super::store::RpcResult::NotStarted {
            self.replacement = None;
            return;
        }
        self.start_replacement(now, out);
    }

    fn start_replacement(&mut self, now: f64, out: &mut Vec<Effect>) {
        if let Some(next) = self.replacement.take() {
            let context = self.latest.clone().unwrap_or(next);
            self.start(context, true, now, out);
        }
    }

    fn start(&mut self, context: Context, automatic: bool, now: f64, out: &mut Vec<Effect>) {
        self.idle_at = None;
        if automatic && !self.auto_refresh {
            return;
        }
        let skipped = |reason: &str| Effect::Log { name: "completion_skipped", detail: format!("reason={reason}") };
        let Some(cwd) = context.cwd.clone().filter(|c| !c.is_empty()) else {
            out.push(skipped("missing_cwd"));
            return;
        };
        if automatic && blank(&context.draft) {
            out.push(skipped("blank_draft"));
            return;
        }
        let cursor = usize::try_from(context.cursor).unwrap_or(usize::MAX);
        if word_continues_past_cursor(&context.draft, cursor) {
            out.push(skipped("inside_word"));
            return;
        }
        self.requests += 1;
        let request_id = format!("completion-{}", self.requests);
        out.push(Effect::Log {
            name: "completion_requested",
            detail: format!("requestId={request_id} revision={} cursor={}", context.buffer_revision, context.cursor),
        });
        self.timeout_at = Some((now + CLIENT_TIMEOUT_MS, request_id.clone()));
        let request = Request {
            request_id: request_id.clone(),
            draft: context.draft.clone(),
            buffer_revision: context.buffer_revision,
            cursor: context.cursor,
            prompt_generation: context.prompt_generation,
            cwd: Some(cwd),
        };
        out.push(Effect::Request { terminal_id: context.terminal_id.clone(), request });
        self.in_flight = Some(InFlight { request_id, context });
    }
}

/// The caret's char offset in `draft` as the UTF-16 offset the host counts, and back.
#[must_use]
pub fn utf16_cursor(draft: &str, chars: usize) -> u64 {
    draft.chars().take(chars).map(|c| c.len_utf16() as u64).sum()
}

#[must_use]
pub fn char_cursor(draft: &str, units: u64) -> usize {
    char_offset(draft, units)
}
