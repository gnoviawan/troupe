//! One figcraft mount per project (`stores/canvas-store.ts`, `lib/canvas-gateway.ts`), as a
//! machine: the app hands it the host's replies, the editor's messages and the gateway's calls,
//! and carries out the [`Effect`]s it queues.
//!
//! A session's [`Phase`] is the mount state machine: `Opening` (the `/canvas/open` route in
//! flight) → `Loading` (editor mounted, bridge attached; on `event.ready` the pin is checked and
//! the document loaded) → `Ready`, or `Failed`, `Elsewhere` (another window took the lease) or
//! `Unloaded` (the host evicted it to stay under `canvas.max_mounted`). Persistence is the host's
//! (spine AD-23): the session holds only the projection `{dirty, contentVersion, savedVersion,
//! saving, saveError, conflict}`. It forwards `event.dirtyChanged` to the host, and runs
//! `host.snapshot` → `/canvas/write-snapshot` → `host.markSaved` when the host says a save is due
//! or the user saves. Agent calls the host forwards (`canvas_call`) run through the mount's
//! bridge; `host.save` and `host.remounted` are answered here.

use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::{Value, json};

use super::bridge::{Bridge, Called, Code, Event, Reply, Ticket};
pub use super::flow::Then;

/// How long the editor may take to report ready (`CANVAS_LOAD_TIMEOUT_MS`).
pub const LOAD_TIMEOUT_MS: f64 = 30_000.0;

/// `CanvasOpenInfo`, the `/canvas/open` reply.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OpenInfo {
    pub canvas_id: String,
    #[serde(default)]
    pub base_url: String,
    pub canvas_token: String,
    pub pin_tag: String,
    #[serde(default)]
    pub doc_key: String,
    pub mount_seq: u64,
    #[serde(default)]
    pub evicted: Vec<Eviction>,
    #[serde(default)]
    pub fallback_fonts: Vec<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Eviction {
    pub project_id: String,
    pub reason: String,
}

/// `CanvasConflict`: the file changed on disk while the editor held unsaved work.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Conflict {
    pub disk_hash: Option<String>,
    pub version: Option<String>,
}

/// `CanvasRemount`: a crashed mount replaced by a fresh one from the last save.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Remount {
    pub previous_mount_seq: u64,
    pub mount_seq: u64,
    pub reason: String,
    pub lost: Option<Lost>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Lost {
    pub count: u64,
    pub since_ms: Option<f64>,
}

/// `CanvasPersistenceView`.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct View {
    pub dirty: bool,
    pub content_version: Option<String>,
    pub saved_version: Option<String>,
    pub save_error: Option<String>,
    pub conflict: Option<Conflict>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnloadReason {
    Evicted,
    Blocked,
}

/// Why a window no longer holds a mount (`CanvasLeaseLoss`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LeaseLoss {
    Elsewhere,
    Evicted,
    Expired,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Phase {
    Opening,
    Loading { info: OpenInfo, since: f64 },
    Ready { info: OpenInfo },
    /// `mount_seq`: the host mount the failure left behind.
    Failed { code: String, message: String, mount_seq: Option<u64> },
    Elsewhere,
    Unloaded { reason: UnloadReason, message: Option<String> },
}

impl Phase {
    pub fn mount_seq(&self) -> Option<u64> {
        match self {
            Phase::Loading { info, .. } | Phase::Ready { info } => Some(info.mount_seq),
            _ => None,
        }
    }

    /// The host mount this window holds, a failed one included.
    pub fn host_mount_seq(&self) -> Option<u64> {
        match self {
            Phase::Failed { mount_seq, .. } => *mount_seq,
            _ => self.mount_seq(),
        }
    }
}

/// The user's answer to a conflict.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Choice {
    KeepMine,
    TakeDisk,
    SaveCopy,
}

impl Choice {
    pub fn wire(self) -> &'static str {
        match self {
            Choice::KeepMine => "keepMine",
            Choice::TakeDisk => "takeDisk",
            Choice::SaveCopy => "saveCopy",
        }
    }
}

/// Who waits for a bridge reply.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Waiter {
    Gateway { call_id: u64, mount_seq: u64 },
    Describe,
    FallbackFonts,
    OpenDocument { reload: bool },
    Snapshot,
    CopySnapshot,
    MarkSaved,
    /// A fire-and-forget host call (`host.file`, `host.theme`, `host.locale`).
    Fire(&'static str),
}

pub struct Session {
    pub project_id: String,
    pub doc_path: String,
    pub phase: Phase,
    pub dirty: bool,
    pub content_version: Option<String>,
    pub saved_version: Option<String>,
    pub saving: u32,
    pub save_error: Option<String>,
    pub closing: bool,
    pub conflict: Option<Conflict>,
    pub resolving: bool,
    pub crash: Option<Remount>,
    /// The close the host refused because the save failed: the unmount confirm asks.
    pub unsaved_close: Option<String>,
    pub(super) bridge: Option<Bridge>,
    pub(super) waiting: BTreeMap<Ticket, Waiter>,
    /// Agent `host.save` calls waiting for the save they started: `(callId, mountSeq)`.
    pub(super) agent_saves: Vec<(u64, u64)>,
    pub(super) generation: u64,
    pub(super) dirty_notice: u64,
    /// Retry a failed `/canvas/dirty-changed` once at `(notice, due_ms)`.
    pub(super) dirty_retry: Option<(u64, f64)>,
    /// Whether an explicit user Save is among the saves in flight (toasts on failure).
    pub(super) user_save: bool,
    /// Opening a different doc in the same project closes the current one first, then opens this.
    pub(super) reopen_after: Option<(String, bool)>,
}

impl Session {
    pub(super) fn new(project_id: &str, doc_path: &str, phase: Phase) -> Session {
        Session {
            project_id: project_id.to_owned(),
            doc_path: doc_path.to_owned(),
            phase,
            dirty: false,
            content_version: None,
            saved_version: None,
            saving: 0,
            save_error: None,
            closing: false,
            conflict: None,
            resolving: false,
            crash: None,
            unsaved_close: None,
            bridge: None,
            waiting: BTreeMap::new(),
            agent_saves: Vec::new(),
            generation: 0,
            dirty_notice: 0,
            dirty_retry: None,
            user_save: false,
            reopen_after: None,
        }
    }

    /// Back to the projection of a fresh mount.
    pub(super) fn clean(&mut self) {
        self.dirty = false;
        self.content_version = None;
        self.saved_version = None;
        self.saving = 0;
        self.save_error = None;
        self.closing = false;
        self.conflict = None;
        self.resolving = false;
        self.crash = None;
        self.dirty_retry = None;
        self.user_save = false;
    }

    pub(super) fn apply_view(&mut self, view: &View) {
        self.dirty = view.dirty;
        self.content_version.clone_from(&view.content_version);
        self.saved_version.clone_from(&view.saved_version);
        self.save_error.clone_from(&view.save_error);
        self.conflict.clone_from(&view.conflict);
    }

    /// The live bridge for `mount_seq`.
    pub(super) fn bridge_for(&mut self, mount_seq: u64) -> Option<&mut Bridge> {
        (self.phase.mount_seq() == Some(mount_seq)).then_some(())?;
        self.bridge.as_mut().filter(|b| b.mount_seq() == mount_seq)
    }

    pub(super) fn send(&mut self, method: &str, params: &str, waiter: Waiter) -> Option<Reply> {
        let bridge = self.bridge.as_mut()?;
        match bridge.call(method, params, None) {
            Called::Now(reply) => Some(reply),
            Called::Waiting(ticket) => {
                self.waiting.insert(ticket, waiter);
                None
            }
        }
    }
}

/// What the app carries out for the canvases.
#[derive(Clone, Debug, PartialEq)]
pub enum Effect {
    /// `POST /canvas/<route>` with `body`; its reply goes to [`super::Canvases::replied`].
    Post { route: &'static str, body: Value, then: Then },
    /// A `/ws` request the gateway leg sends: `canvas_mount`, `canvas_unmount` or `canvas_reply`.
    Ws { kind: &'static str, payload: Value },
    /// Mount an editor for the project's new mount; drop the one it had.
    MountEditor { project_id: String, info: OpenInfo },
    DropEditor { project_id: String },
    /// Envelopes for the project's editor.
    ToEditor { project_id: String, messages: Vec<Value> },
    ShowTab { project_id: String, doc_path: String },
    Toast { error: bool, title: String, detail: Option<String> },
}

/// The gateway's reply for `call_id` at `mount_seq`.
pub(super) fn gateway_reply(call_id: u64, mount_seq: u64, reply: &Reply) -> Value {
    match reply {
        Reply::Ok(result) => json!({"callId": call_id, "mountSeq": mount_seq, "ok": true, "result": result}),
        Reply::Err { code, message } => {
            json!({"callId": call_id, "mountSeq": mount_seq, "ok": false, "error": {"code": code.as_str(), "message": message}})
        }
    }
}

/// `pinVerdict` (AD-22): a bundle whose `host.describe` tag differs from the pin is refused in
/// release builds and only logged in debug ones.
pub fn pin_refused(described: Option<&str>, pin_tag: &str, release: bool) -> bool {
    described != Some(pin_tag) && release
}

pub(super) fn described_tag(reply: &Reply) -> Option<String> {
    let Reply::Ok(text) = reply else { return None };
    serde_json::from_str::<Value>(text).ok()?.get("tag")?.as_str().map(str::to_owned)
}

pub(super) fn closed() -> Reply {
    Reply::failure(Code::Closed, "the canvas this call was sent to is no longer mounted")
}

pub(super) fn event_name(event: &Event) -> &'static str {
    match event {
        Event::Ready => "ready",
        Event::DirtyChanged { .. } => "dirtyChanged",
        Event::Crashed { .. } => "crashed",
    }
}
