//! The canvases' lifecycle transitions: mount open/close, pin verification, fallback fonts,
//! document loading, crash recovery, lease loss, and the `/ws` gateway. Persistence, autosave,
//! disk conflict resolution and document path helpers live in `persistence.rs`.

use std::collections::BTreeMap;

use serde_json::{Value, json};

use super::bridge::{Bridge, Called, Code, Event, Reply, host_stopped_waiting};
use super::persistence::same_doc;
use super::session::{
    Effect, LOAD_TIMEOUT_MS, LeaseLoss, OpenInfo, Phase, Remount, Session, UnloadReason, View, Waiter, closed,
    described_tag, event_name, gateway_reply, pin_refused,
};

/// Which host reply a [`Effect::Post`] waits for.
#[derive(Clone, Debug, PartialEq)]
pub enum Then {
    Open { generation: u64 },
    Close { mount_seq: u64, discard: bool },
    Visible { mount_seq: u64, visible: bool },
    ReadDocument { mount_seq: u64 },
    DocumentLoaded { mount_seq: u64 },
    Dirty { mount_seq: u64, notice: u64, retry: bool },
    Write { mount_seq: u64 },
    Disk { mount_seq: u64 },
    Resolve { mount_seq: u64 },
    Crashed { mount_seq: u64 },
}

/// A host failure: its code (`NETWORK_ERROR` when nothing answered) and message.
pub type Failure = (String, String);

/// What the app knows about the window, for the calls the session sends on its own.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HostFacts {
    /// This window's `clientInstanceId`, sent with `POST /canvas/open` for lease ownership.
    pub client_instance_id: String,
    /// `host.theme`'s `tokens`, or none to leave the editor's own theme.
    pub theme: Option<Value>,
    pub locale: String,
    /// `host.file` for a project's document.
    pub file: BTreeMap<String, Value>,
    /// Release builds refuse a bundle whose tag is not the pin.
    pub release: bool,
    /// Desktop runs the editor in process with native fonts; web fetches `fallbackFonts`.
    pub in_process: bool,
}

/// Every project's canvas in this window.
#[derive(Default)]
pub struct Canvases {
    pub sessions: BTreeMap<String, Session>,
    pub facts: HostFacts,
    /// `canvas.*` settings sent with each open (`CanvasOpenOptions`).
    pub options: Value,
    pub(super) effects: Vec<Effect>,
    generation: u64,
    /// Projects reported to the gateway as held here, with the mount.
    reported: BTreeMap<String, u64>,
}

impl Canvases {
    pub fn take_effects(&mut self) -> Vec<Effect> {
        self.sync_gateway();
        std::mem::take(&mut self.effects)
    }

    pub fn session(&self, project_id: &str) -> Option<&Session> {
        self.sessions.get(project_id)
    }

    pub fn has_unmount_confirm(&self) -> bool {
        self.sessions.values().any(|s| s.unsaved_close.is_some())
    }

    pub(super) fn toast(&mut self, error: bool, title: impl Into<String>, detail: Option<String>) {
        self.effects.push(Effect::Toast { error, title: title.into(), detail });
    }

    pub(super) fn post(&mut self, route: &'static str, body: Value, then: Then) {
        self.effects.push(Effect::Post { route, body, then });
    }

    /// `openCanvas`: mount the project's document, or show the mount it already has.
    pub fn open(&mut self, project_id: &str, doc_path: &str, show_tab: bool, now: f64) {
        if project_id.is_empty() || doc_path.is_empty() {
            return;
        }
        let show = |effects: &mut Vec<Effect>, path: &str| {
            if show_tab {
                effects.push(Effect::ShowTab { project_id: project_id.to_owned(), doc_path: path.to_owned() });
            }
        };
        if let Some(s) = self.sessions.get(project_id)
            && s.phase.mount_seq().is_some()
        {
            if same_doc(&s.doc_path, doc_path) {
                let path = s.doc_path.clone();
                show(&mut self.effects, &path);
                return;
            }
            log::info!("[canvas] project {project_id} opens another document; the open one closes first");
            if let Some(s) = self.sessions.get_mut(project_id) {
                s.reopen_after = Some((doc_path.to_owned(), show_tab));
            }
            self.unmount(project_id, false);
            return;
        }
        self.generation += 1;
        let generation = self.generation;
        let session = self.sessions.entry(project_id.to_owned()).or_insert_with(|| Session::new(project_id, doc_path, Phase::Opening));
        session.doc_path = doc_path.to_owned();
        session.phase = Phase::Opening;
        session.generation = generation;
        session.reopen_after = None;
        let mut body = json!({
            "docPath": doc_path,
            "projectId": project_id,
            "clientInstanceId": self.facts.client_instance_id,
        });
        if let (Some(body), Some(options)) = (body.as_object_mut(), self.options.as_object()) {
            body.extend(options.clone());
        }
        let _ = now;
        self.post("/canvas/open", body, Then::Open { generation });
        show(&mut self.effects, doc_path);
    }

    /// `unmountCanvas`: the host's AD-31 close; the session ends once the host unmounted.
    pub fn unmount(&mut self, project_id: &str, discard: bool) {
        let Some(session) = self.sessions.get_mut(project_id) else { return };
        let Some(mount_seq) = session.phase.host_mount_seq() else {
            self.drop_session(project_id);
            return;
        };
        session.closing = true;
        session.unsaved_close = None;
        self.post("/canvas/close", json!({"projectId": project_id, "mountSeq": mount_seq, "discard": discard}), Then::Close { mount_seq, discard });
    }

    /// The unmount confirm's "Keep editing".
    pub fn keep_editing(&mut self, project_id: &str) {
        if let Some(s) = self.sessions.get_mut(project_id) {
            s.unsaved_close = None;
            s.reopen_after = None;
        }
    }

    /// The user started or stopped looking at the canvas.
    pub fn set_visible(&mut self, project_id: &str, visible: bool) {
        let Some(mount_seq) = self.sessions.get(project_id).and_then(|s| s.phase.mount_seq()) else { return };
        self.post("/canvas/set-visible", json!({"projectId": project_id, "mountSeq": mount_seq, "visible": visible}), Then::Visible { mount_seq, visible });
        if visible {
            self.check_disk(project_id);
        }
    }

    pub fn dismiss_crash(&mut self, project_id: &str) {
        if let Some(s) = self.sessions.get_mut(project_id) {
            s.crash = None;
        }
    }

    /// `canvas_lease_lost`, or a `canvas_mount` the host refused.
    pub fn lease_lost(&mut self, project_id: &str, mount_seq: u64, loss: LeaseLoss) {
        let Some(s) = self.sessions.get(project_id) else { return };
        if s.phase.mount_seq() != Some(mount_seq) {
            return;
        }
        log::info!("[canvas] project {project_id} mountSeq={mount_seq} lost its lease: {loss:?}");
        let phase = match loss {
            LeaseLoss::Elsewhere => Phase::Elsewhere,
            LeaseLoss::Evicted => Phase::Unloaded { reason: UnloadReason::Evicted, message: None },
            LeaseLoss::Expired => Phase::Failed {
                code: "CANVAS_CLOSED".into(),
                message: "The canvas closed after this window was disconnected for too long. Open it again to continue from its last save.".into(),
                mount_seq: None,
            },
        };
        self.release(project_id, phase);
    }

    /// Answers the gateway's `canvas_mount` refusal.
    pub fn mount_refused(&mut self, project_id: &str, mount_seq: u64, held_elsewhere: bool) {
        self.reported.remove(project_id);
        self.lease_lost(project_id, mount_seq, if held_elsewhere { LeaseLoss::Elsewhere } else { LeaseLoss::Expired });
    }

    /// One agent call the host forwarded (`canvas_call`); `deadline` on the `now` clock.
    #[allow(clippy::too_many_arguments)]
    pub fn gateway_call(&mut self, call_id: u64, project_id: &str, mount_seq: u64, method: &str, params: &str, deadline: Option<f64>, now: f64) {
        let held = self.sessions.get(project_id).and_then(|s| s.phase.mount_seq());
        let reply = if held != Some(mount_seq) {
            Some(closed())
        } else if deadline.is_some_and(|d| now >= d) {
            Some(host_stopped_waiting())
        } else if method == "host.save" {
            self.host_save(project_id, call_id, mount_seq)
        } else if method == "host.remounted" {
            Some(self.host_remounted(project_id, params, now))
        } else {
            let session = self.sessions.get_mut(project_id).expect("held above");
            match session.bridge.as_mut().map(|b| b.call(method, params, deadline)) {
                None => Some(Reply::failure(Code::Closed, "no canvas is mounted")),
                Some(Called::Now(reply)) => Some(reply),
                Some(Called::Waiting(ticket)) => {
                    session.waiting.insert(ticket, Waiter::Gateway { call_id, mount_seq });
                    None
                }
            }
        };
        if let Some(reply) = reply {
            self.effects.push(Effect::Ws { kind: "canvas_reply", payload: gateway_reply(call_id, mount_seq, &reply) });
        }
        self.flush(project_id);
    }

    /// Messages from a project's editor, at `now`.
    pub fn editor_messages(&mut self, project_id: &str, messages: &[Value], now: f64) {
        let Some(s) = self.sessions.get_mut(project_id) else { return };
        let Some(bridge) = s.bridge.as_mut() else { return };
        for m in messages {
            bridge.receive(m, now);
        }
        self.settle(project_id, now);
    }

    /// The load watchdog (#1490) and dirty-notification retries. Returns the next due time (ms).
    pub fn tick(&mut self, now: f64) -> Option<f64> {
        let late: Vec<(String, u64)> = self
            .sessions
            .values()
            .filter_map(|s| match &s.phase {
                Phase::Loading { info, since } if now - since >= LOAD_TIMEOUT_MS => Some((s.project_id.clone(), info.mount_seq)),
                _ => None,
            })
            .collect();
        for (project_id, mount_seq) in late {
            log::warn!("[canvas] editor did not report ready for project {project_id} mountSeq={mount_seq} within {LOAD_TIMEOUT_MS}ms");
            self.fail(&project_id, mount_seq, "EDITOR_LOAD_TIMEOUT", "The canvas editor did not finish loading. Open the canvas again to retry.");
        }
        let retries: Vec<(String, u64, bool, String)> = self
            .sessions
            .values_mut()
            .filter_map(|s| {
                let (notice, due) = s.dirty_retry?;
                if now < due {
                    return None;
                }
                s.dirty_retry = None;
                if s.dirty_notice != notice {
                    return None;
                }
                let Phase::Ready { info } = &s.phase else { return None };
                let version = s.content_version.clone()?;
                Some((s.project_id.clone(), info.mount_seq, s.dirty, version))
            })
            .collect();
        for (project_id, mount_seq, dirty, version) in retries {
            self.forward_dirty(&project_id, mount_seq, dirty, version, false);
        }
        self.sessions
            .values()
            .flat_map(|s| {
                let load = match &s.phase {
                    Phase::Loading { since, .. } => Some(since + LOAD_TIMEOUT_MS),
                    _ => None,
                };
                let retry = s.dirty_retry.map(|(_, due)| due);
                [load, retry]
            })
            .flatten()
            .reduce(f64::min)
    }

    /// One host reply.
    pub fn replied(&mut self, project_id: &str, then: Then, result: Result<Value, Failure>, now: f64) {
        match then {
            Then::Open { generation } => self.opened(project_id, generation, result, now),
            Then::Close { mount_seq, discard } => self.closed(project_id, mount_seq, discard, result, now),
            Then::Visible { mount_seq, visible } => {
                if visible && matches!(&result, Err((code, _)) if code == "CANVAS_CLOSED") && self.live(project_id, mount_seq) {
                    log::info!("[canvas] the host no longer holds the canvas of project {project_id}");
                    self.release(project_id, Phase::Unloaded { reason: UnloadReason::Evicted, message: None });
                }
            }
            Then::ReadDocument { mount_seq } => self.document_read(project_id, mount_seq, result),
            Then::DocumentLoaded { mount_seq } => self.document_loaded(project_id, mount_seq, result),
            Then::Dirty { mount_seq, notice, retry } => self.dirty_settled(project_id, mount_seq, notice, retry, result, now),
            Then::Write { mount_seq } => self.written(project_id, mount_seq, result),
            Then::Disk { mount_seq } => self.disk_checked(project_id, mount_seq, result),
            Then::Resolve { mount_seq } => self.resolved(project_id, mount_seq, result),
            Then::Crashed { mount_seq } => match result.ok().and_then(|v| serde_json::from_value::<Remount>(v).ok()) {
                Some(remount) => {
                    self.apply_remount(project_id, remount, now);
                }
                None if self.live(project_id, mount_seq) => self.fail(project_id, mount_seq, "EDITOR_CRASHED", "The canvas editor stopped working."),
                None => {}
            },
        }
        self.flush(project_id);
    }

    fn opened(&mut self, project_id: &str, generation: u64, result: Result<Value, Failure>, now: f64) {
        let info = result.and_then(|v| serde_json::from_value::<OpenInfo>(v).map_err(|e| ("INVALID_PAYLOAD".into(), e.to_string())));
        if let Ok(info) = &info {
            for eviction in info.evicted.clone() {
                log::info!("[canvas] canvas for project {} was unloaded: {}", eviction.project_id, eviction.reason);
                self.release(&eviction.project_id, Phase::Unloaded { reason: UnloadReason::Evicted, message: None });
            }
        }
        let current = self.sessions.get(project_id).is_some_and(|s| s.generation == generation && s.phase == Phase::Opening);
        if !current {
            log::info!("[canvas] dropped a superseded canvas open for project {project_id}");
            if let Ok(info) = info {
                self.post("/canvas/close", json!({"projectId": project_id, "mountSeq": info.mount_seq, "discard": false}), Then::Close { mount_seq: info.mount_seq, discard: false });
            }
            return;
        }
        match info {
            Err((code, message)) => {
                log::warn!("[canvas] canvas open failed for project {project_id}: {code} {message}");
                self.toast(true, "Could not open the canvas", Some(message.clone()));
                if code == "EVICTION_BLOCKED" {
                    self.release(project_id, Phase::Unloaded { reason: UnloadReason::Blocked, message: Some(message) });
                } else {
                    self.sessions.remove(project_id);
                }
            }
            Ok(info) => {
                let Some(s) = self.sessions.get_mut(project_id) else { return };
                s.clean();
                s.bridge = Some(Bridge::new(info.mount_seq));
                s.waiting.clear();
                s.phase = Phase::Loading { info: info.clone(), since: now };
                log::info!("[canvas] canvas open project={project_id} mountSeq={}", info.mount_seq);
                self.effects.push(Effect::MountEditor { project_id: project_id.to_owned(), info });
            }
        }
    }

    fn closed(&mut self, project_id: &str, mount_seq: u64, discard: bool, result: Result<Value, Failure>, now: f64) {
        let Some(s) = self.sessions.get_mut(project_id) else { return };
        if s.phase.host_mount_seq() != Some(mount_seq) {
            return;
        }
        match result {
            Ok(outcome) => {
                let discarded = outcome.get("discarded").and_then(Value::as_bool).unwrap_or(discard);
                let reopen = s.reopen_after.take();
                log::info!("[canvas] canvas unmounted for project {project_id} discarded={discarded}");
                self.drop_session(project_id);
                if let Some((doc_path, show_tab)) = reopen {
                    self.open(project_id, &doc_path, show_tab, now);
                }
            }
            Err((code, message)) => {
                log::warn!("[canvas] canvas unmount refused for project {project_id}: {code} {message}");
                s.closing = false;
                s.reopen_after = None;
                if code == "SAVE_FAILED" {
                    s.unsaved_close = Some(message);
                } else {
                    self.toast(true, "Could not close the canvas", Some(message));
                }
            }
        }
    }

    /// `event.ready` on a loading mount: check the pin, then load fallback fonts and document.
    fn complete_mount(&mut self, project_id: &str) {
        let Some(s) = self.sessions.get_mut(project_id) else { return };
        if let Some(reply) = s.send("host.describe", "{}", Waiter::Describe) {
            self.described(project_id, reply);
        }
    }

    fn described(&mut self, project_id: &str, reply: Reply) {
        let (release, in_process) = (self.facts.release, self.facts.in_process);
        let Some(s) = self.sessions.get_mut(project_id) else { return };
        let Phase::Loading { info, .. } = &s.phase else { return };
        let (mount_seq, pin, fonts) = (info.mount_seq, info.pin_tag.clone(), info.fallback_fonts.clone());
        let tag = described_tag(&reply);
        if pin_refused(tag.as_deref(), &pin, release) {
            log::warn!("[canvas] refused the canvas mount for project {project_id}: bundle tag {tag:?} does not match the pinned {pin}");
            let message = format!("The canvas bundle ({}) does not match this build ({pin}).", tag.as_deref().unwrap_or("unknown"));
            self.fail(project_id, mount_seq, "BUNDLE_PIN_MISMATCH", &message);
            return;
        }
        if tag.as_deref() != Some(pin.as_str()) {
            log::warn!("[canvas] bundle tag {tag:?} differs from the pin {pin} (debug build: kept)");
        }
        if !in_process && !fonts.is_empty() {
            let params = json!({"urls": fonts}).to_string();
            if let Some(font_reply) = s.send("host.fallbackFonts", &params, Waiter::FallbackFonts) {
                self.fallback_fonts_done(project_id, mount_seq, font_reply);
            }
            return;
        }
        self.read_document(project_id, mount_seq);
    }

    fn fallback_fonts_done(&mut self, project_id: &str, mount_seq: u64, reply: Reply) {
        if let Reply::Err { code, message } = reply {
            log::info!("[canvas] fallback fonts not loaded for project {project_id}: {} {message}", code.as_str());
        }
        if self.live(project_id, mount_seq) {
            self.read_document(project_id, mount_seq);
        }
    }

    fn read_document(&mut self, project_id: &str, mount_seq: u64) {
        self.post("/canvas/read-document", json!({"projectId": project_id, "mountSeq": mount_seq}), Then::ReadDocument { mount_seq });
    }

    fn document_read(&mut self, project_id: &str, mount_seq: u64, result: Result<Value, Failure>) {
        if !self.live(project_id, mount_seq) {
            return;
        }
        let bytes = match result {
            Err((code, message)) => return self.load_failed(project_id, mount_seq, &format!("{code} {message}")),
            Ok(v) => v.get("bytes").and_then(Value::as_str).map(str::to_owned),
        };
        let Some(bytes) = bytes else {
            self.post("/canvas/document-loaded", json!({"projectId": project_id, "mountSeq": mount_seq, "ack": null}), Then::DocumentLoaded { mount_seq });
            return;
        };
        let params = json!({"bytes": bytes}).to_string();
        let Some(s) = self.sessions.get_mut(project_id) else { return };
        if let Some(reply) = s.send("host.openDocument", &params, Waiter::OpenDocument { reload: false }) {
            self.document_opened(project_id, mount_seq, reply, false);
        }
    }

    pub(super) fn document_opened(&mut self, project_id: &str, mount_seq: u64, reply: Reply, reload: bool) {
        match reply {
            Reply::Ok(ack) => self.post(
                "/canvas/document-loaded",
                json!({"projectId": project_id, "mountSeq": mount_seq, "ack": ack}),
                Then::DocumentLoaded { mount_seq },
            ),
            Reply::Err { code, message } if reload => self.reload_not_taken(project_id, mount_seq, &format!("{} {message}", code.as_str())),
            Reply::Err { code, message } => self.load_failed(project_id, mount_seq, &format!("{} {message}", code.as_str())),
        }
    }

    fn document_loaded(&mut self, project_id: &str, mount_seq: u64, result: Result<Value, Failure>) {
        if !self.live(project_id, mount_seq) {
            return;
        }
        let view = match result.and_then(|v| serde_json::from_value::<View>(v).map_err(|e| ("INVALID_PAYLOAD".into(), e.to_string()))) {
            Ok(view) => view,
            Err((code, message)) => return self.load_failed(project_id, mount_seq, &format!("{code} {message}")),
        };
        let Some(s) = self.sessions.get_mut(project_id) else { return };
        s.apply_view(&view);
        if let Phase::Loading { info, .. } = &s.phase {
            log::info!("[canvas] canvas ready for project {project_id} mountSeq={mount_seq} pin={} saved={}", info.pin_tag, view.saved_version.as_deref().unwrap_or("new"));
            s.phase = Phase::Ready { info: info.clone() };
        }
        self.push_host_facts(project_id);
    }

    /// `host.theme`, `host.locale` and `host.file` after every load (or when theme changes).
    pub fn push_host_facts(&mut self, project_id: &str) {
        let theme = self.facts.theme.clone();
        let locale = self.facts.locale.clone();
        let file = self.facts.file.get(project_id).cloned();
        let Some(s) = self.sessions.get_mut(project_id) else { return };
        if !matches!(s.phase, Phase::Ready { .. }) {
            return;
        }
        if let Some(tokens) = theme {
            s.send("host.theme", &json!({"tokens": tokens}).to_string(), Waiter::Fire("host.theme"));
        }
        if !locale.is_empty() {
            s.send("host.locale", &json!({"bcp47": locale}).to_string(), Waiter::Fire("host.locale"));
        }
        if let Some(file) = file {
            s.send("host.file", &file.to_string(), Waiter::Fire("host.file"));
        }
        self.flush(project_id);
    }

    /// Pushes the current `facts.theme` to every ready canvas without reloading.
    pub fn push_theme_all(&mut self) {
        let Some(tokens) = self.facts.theme.clone() else { return };
        let body = json!({"tokens": tokens}).to_string();
        let ids: Vec<String> = self
            .sessions
            .iter()
            .filter(|(_, s)| matches!(s.phase, Phase::Ready { .. })).map(|(id, _)| id.clone())
            .collect();
        for id in ids {
            if let Some(s) = self.sessions.get_mut(&id) {
                s.send("host.theme", &body, Waiter::Fire("host.theme"));
            }
            self.flush(&id);
        }
    }

    fn load_failed(&mut self, project_id: &str, mount_seq: u64, message: &str) {
        log::warn!("[canvas] canvas document load failed for project {project_id}: {message}");
        self.fail(project_id, mount_seq, "DOCUMENT_LOAD_FAILED", &format!("Could not load the canvas: {message}"));
    }

    fn host_remounted(&mut self, project_id: &str, params: &str, now: f64) -> Reply {
        match serde_json::from_str::<Remount>(params) {
            Err(_) => Reply::failure(Code::EngineError, "malformed host.remounted notice"),
            Ok(remount) => {
                if self.apply_remount(project_id, remount, now) {
                    Reply::Ok("{}".into())
                } else {
                    Reply::failure(Code::Closed, "this window does not hold that mount")
                }
            }
        }
    }

    /// The host replaced `previous_mount_seq` with a fresh mount loading the last save.
    pub fn apply_remount(&mut self, project_id: &str, remount: Remount, now: f64) -> bool {
        let Some(s) = self.sessions.get_mut(project_id) else { return false };
        let info = match &s.phase {
            Phase::Loading { info, .. } | Phase::Ready { info } if info.mount_seq == remount.previous_mount_seq => info.clone(),
            _ => return false,
        };
        log::warn!("[canvas] canvas remounted for project {project_id} mountSeq {}->{}: {}", remount.previous_mount_seq, remount.mount_seq, remount.reason);
        let info = OpenInfo { mount_seq: remount.mount_seq, ..info };
        if let Some(b) = s.bridge.as_mut() {
            b.dispose();
        }
        s.clean();
        s.waiting.clear();
        s.crash = Some(remount);
        s.bridge = Some(Bridge::new(info.mount_seq));
        s.phase = Phase::Loading { info: info.clone(), since: now };
        self.effects.push(Effect::MountEditor { project_id: project_id.to_owned(), info });
        true
    }

    fn crashed(&mut self, project_id: &str, mount_seq: u64, reason: &str) {
        log::warn!("[canvas] canvas editor crashed for project {project_id}: {reason}");
        let recovering = self.sessions.get(project_id).is_some_and(|s| matches!(s.phase, Phase::Loading { .. }) && s.crash.as_ref().is_some_and(|c| c.mount_seq == mount_seq));
        if recovering {
            self.fail(project_id, mount_seq, "EDITOR_CRASHED", "The canvas editor stopped working.");
            return;
        }
        self.post("/canvas/crashed", json!({"projectId": project_id, "mountSeq": mount_seq, "reason": reason}), Then::Crashed { mount_seq });
    }

    /// Moves a project's bridge traffic: answers and events in, envelopes out.
    fn settle(&mut self, project_id: &str, now: f64) {
        loop {
            let Some(s) = self.sessions.get_mut(project_id) else { return };
            let Some(bridge) = s.bridge.as_mut() else { return };
            let mount_seq = bridge.mount_seq();
            let answered = bridge.take_answered();
            let events = bridge.take_events();
            if answered.is_empty() && events.is_empty() {
                break;
            }
            for (ticket, reply) in answered {
                let waiter = self.sessions.get_mut(project_id).and_then(|s| s.waiting.remove(&ticket));
                match waiter {
                    Some(Waiter::Gateway { call_id, mount_seq }) => {
                        self.effects.push(Effect::Ws { kind: "canvas_reply", payload: gateway_reply(call_id, mount_seq, &reply) });
                    }
                    Some(Waiter::Describe) => self.described(project_id, reply),
                    Some(Waiter::FallbackFonts) => self.fallback_fonts_done(project_id, mount_seq, reply),
                    Some(Waiter::OpenDocument { reload }) => self.document_opened(project_id, mount_seq, reply, reload),
                    Some(Waiter::Snapshot) => self.snapshot_taken(project_id, mount_seq, reply),
                    Some(Waiter::CopySnapshot) => self.copy_snapshot(project_id, mount_seq, reply),
                    Some(Waiter::MarkSaved) | Some(Waiter::Fire(_)) => {
                        if let Reply::Err { code, .. } = &reply {
                            log::warn!("[canvas] a host call failed for project {project_id}: {}", code.as_str());
                        }
                    }
                    None => {}
                }
            }
            for event in events {
                log::debug!("[canvas] event.{} project={project_id}", event_name(&event));
                match event {
                    Event::Ready => {
                        let phase = self.sessions.get(project_id).map(|s| s.phase.clone());
                        match phase {
                            Some(Phase::Loading { .. }) => self.complete_mount(project_id),
                            Some(Phase::Ready { info }) => {
                                log::warn!("[canvas] canvas frame reloaded for project {project_id}");
                                if let Some(s) = self.sessions.get_mut(project_id) {
                                    s.phase = Phase::Loading { info, since: now };
                                }
                                self.complete_mount(project_id);
                            }
                            _ => {}
                        }
                    }
                    Event::DirtyChanged { dirty, content_version } => self.dirty_changed(project_id, mount_seq, dirty, content_version),
                    Event::Crashed { reason } => self.crashed(project_id, mount_seq, &reason),
                }
            }
        }
        self.flush(project_id);
    }

    /// Queues the project's bridge envelopes for its editor.
    pub(super) fn flush(&mut self, project_id: &str) {
        let Some(bridge) = self.sessions.get_mut(project_id).and_then(|s| s.bridge.as_mut()) else { return };
        let messages = bridge.take_outbox();
        if !messages.is_empty() {
            self.effects.push(Effect::ToEditor { project_id: project_id.to_owned(), messages });
        }
    }

    /// Reports every loading or ready mount to the gateway as held here.
    fn sync_gateway(&mut self) {
        let held: BTreeMap<String, u64> = self.sessions.iter().filter_map(|(p, s)| s.phase.mount_seq().map(|m| (p.clone(), m))).collect();
        for (project_id, mount_seq) in &self.reported {
            if held.get(project_id) != Some(mount_seq) && !held.contains_key(project_id) {
                self.effects.push(Effect::Ws { kind: "canvas_unmount", payload: json!({"projectId": project_id, "mountSeq": mount_seq}) });
            }
        }
        for (project_id, mount_seq) in &held {
            if self.reported.get(project_id) != Some(mount_seq) {
                self.effects.push(Effect::Ws { kind: "canvas_mount", payload: json!({"projectId": project_id, "mountSeq": mount_seq}) });
            }
        }
        self.reported = held;
    }

    /// A new connection holds nothing until told again.
    pub fn reconnected(&mut self) {
        self.reported.clear();
    }

    pub(super) fn ready_mount(&self, project_id: &str) -> Option<u64> {
        match &self.sessions.get(project_id)?.phase {
            Phase::Ready { info } => Some(info.mount_seq),
            _ => None,
        }
    }

    pub(super) fn live(&self, project_id: &str, mount_seq: u64) -> bool {
        self.sessions.get(project_id).is_some_and(|s| s.phase.mount_seq() == Some(mount_seq) && s.bridge.as_ref().is_some_and(|b| b.mount_seq() == mount_seq))
    }

    pub(super) fn fail(&mut self, project_id: &str, mount_seq: u64, code: &str, message: &str) {
        self.release(project_id, Phase::Failed { code: code.into(), message: message.into(), mount_seq: Some(mount_seq) });
    }

    /// The host answered `CANVAS_CLOSED` for a mount this window still showed.
    pub(super) fn host_dropped(&mut self, project_id: &str, mount_seq: u64) {
        if !self.live(project_id, mount_seq) {
            return;
        }
        log::warn!("[canvas] the host no longer holds the canvas of project {project_id}");
        self.release(
            project_id,
            Phase::Failed {
                code: "CANVAS_CLOSED".into(),
                message: "Troupe closed this canvas, so changes made here since were not saved. Open it again to continue from its last save.".into(),
                mount_seq: None,
            },
        );
    }

    /// Ends this window's hold on the project's editor and shows `phase`.
    pub(super) fn release(&mut self, project_id: &str, phase: Phase) {
        let Some(s) = self.sessions.get_mut(project_id) else { return };
        let keep_failed_mount = matches!(phase, Phase::Failed { .. });
        if let Some(b) = s.bridge.as_mut() {
            b.dispose();
        }
        let answered = s.bridge.as_mut().map(Bridge::take_answered).unwrap_or_default();
        for (ticket, reply) in answered {
            if let Some(Waiter::Gateway { call_id, mount_seq }) = s.waiting.remove(&ticket) {
                self.effects.push(Effect::Ws { kind: "canvas_reply", payload: gateway_reply(call_id, mount_seq, &reply) });
            }
        }
        let Some(s) = self.sessions.get_mut(project_id) else { return };
        s.bridge = None;
        s.waiting.clear();
        if !keep_failed_mount {
            s.clean();
        }
        s.phase = phase;
        self.effects.push(Effect::DropEditor { project_id: project_id.to_owned() });
    }

    fn drop_session(&mut self, project_id: &str) {
        self.release(project_id, Phase::Opening);
        self.sessions.remove(project_id);
    }
}
