//! Canvas persistence, autosave, disk conflict resolution, agent `host.save`, and `.figcraft`
//! path helpers (`stores/canvas-store.ts`, `lib/canvas-doc.ts`).

use serde_json::{Value, json};

use super::bridge::{Code, Reply};
use super::flow::{Canvases, Failure, Then};
use super::session::{Choice, Effect, Phase, View, Waiter, gateway_reply};

/// The canvas document file extension (`CANVAS_DOC_EXTENSION`).
pub const CANVAS_DOC_EXTENSION: &str = ".figcraft";

/// What a project without a canvas document gets on its first open (`NEW_CANVAS_DOC_NAME`).
pub const NEW_CANVAS_DOC_NAME: &str = "design.figcraft";

/// Default autosave retry delay (ms) when `options.autosaveMs` is unset.
const DEFAULT_AUTOSAVE_MS: f64 = 1_500.0;

impl Canvases {
    /// The user's Save.
    pub fn save(&mut self, project_id: &str) {
        let Some(mount_seq) = self.ready_mount(project_id) else { return };
        if let Some(s) = self.sessions.get_mut(project_id) {
            s.user_save = true;
        }
        self.run_save(project_id, mount_seq);
    }

    /// Troupe saw the document change on disk (or the canvas was shown).
    pub fn check_disk(&mut self, project_id: &str) {
        let Some(mount_seq) = self.ready_mount(project_id) else { return };
        self.post("/canvas/disk-changed", json!({"projectId": project_id, "mountSeq": mount_seq}), Then::Disk { mount_seq });
    }

    pub fn resolve_conflict(&mut self, project_id: &str, choice: Choice) {
        let Some(mount_seq) = self.ready_mount(project_id) else { return };
        let Some(s) = self.sessions.get_mut(project_id) else { return };
        if s.conflict.is_none() || s.resolving {
            return;
        }
        s.resolving = true;
        if choice == Choice::SaveCopy {
            if let Some(reply) = s.send("host.snapshot", "{}", Waiter::CopySnapshot) {
                self.copy_snapshot(project_id, mount_seq, reply);
            }
            return;
        }
        self.post(
            "/canvas/resolve-conflict",
            json!({"projectId": project_id, "mountSeq": mount_seq, "choice": choice.wire()}),
            Then::Resolve { mount_seq },
        );
    }

    pub(super) fn run_save(&mut self, project_id: &str, mount_seq: u64) {
        let Some(s) = self.sessions.get_mut(project_id) else { return };
        if s.bridge_for(mount_seq).is_none() {
            return;
        }
        s.saving += 1;
        if let Some(reply) = s.send("host.snapshot", "{}", Waiter::Snapshot) {
            self.snapshot_taken(project_id, mount_seq, reply);
        }
    }

    pub(super) fn snapshot_taken(&mut self, project_id: &str, mount_seq: u64, reply: Reply) {
        match reply {
            Reply::Ok(snapshot) => self.post(
                "/canvas/write-snapshot",
                json!({"projectId": project_id, "mountSeq": mount_seq, "snapshot": snapshot}),
                Then::Write { mount_seq },
            ),
            Reply::Err { code, message } => self.save_failed(project_id, mount_seq, &format!("{} {message}", code.as_str())),
        }
    }

    fn save_failed(&mut self, project_id: &str, mount_seq: u64, message: &str) {
        log::warn!("[canvas] canvas save failed for project {project_id}: {message}");
        let mut toast_user = false;
        if let Some(s) = self.sessions.get_mut(project_id) {
            s.saving = s.saving.saturating_sub(1);
            if s.phase.mount_seq() == Some(mount_seq) {
                s.save_error = Some(message.to_owned());
                if s.user_save && s.conflict.is_none() {
                    toast_user = true;
                }
            }
            if s.saving == 0 {
                s.user_save = false;
            }
        }
        if toast_user {
            self.toast(true, "Could not save the canvas.", None);
        }
        self.resolve_saves(project_id, mount_seq, false);
    }

    pub(super) fn written(&mut self, project_id: &str, mount_seq: u64, result: Result<Value, Failure>) {
        if !self.live(project_id, mount_seq) {
            return;
        }
        let written = match result {
            Err((code, _)) if code == "CANVAS_CLOSED" => return self.host_dropped(project_id, mount_seq),
            Err((code, message)) => return self.save_failed(project_id, mount_seq, &format!("{code} {message}")),
            Ok(v) => v,
        };
        let view = written.get("view").cloned().and_then(|v| serde_json::from_value::<View>(v).ok()).unwrap_or_default();
        let mark = written.get("markSaved").and_then(Value::as_str).map(str::to_owned);
        if mark.is_none() {
            if view.conflict.is_some() {
                if let Some(s) = self.sessions.get_mut(project_id) {
                    s.saving = s.saving.saturating_sub(1);
                    if s.saving == 0 {
                        s.user_save = false;
                    }
                    s.apply_view(&view);
                }
                self.resolve_saves(project_id, mount_seq, false);
                return;
            }
            if let Some(err) = view.save_error.clone() {
                if let Some(s) = self.sessions.get_mut(project_id) {
                    s.apply_view(&view);
                }
                return self.save_failed(project_id, mount_seq, &err);
            }
        }
        let Some(s) = self.sessions.get_mut(project_id) else { return };
        s.saving = s.saving.saturating_sub(1);
        if s.saving == 0 {
            s.user_save = false;
        }
        s.apply_view(&view);
        if let Some(version) = mark {
            s.send("host.markSaved", &format!("{{\"contentVersion\":{version}}}"), Waiter::MarkSaved);
        }
        self.resolve_saves(project_id, mount_seq, true);
    }

    pub(super) fn dirty_changed(&mut self, project_id: &str, mount_seq: u64, dirty: bool, version: String) {
        let Some(s) = self.sessions.get_mut(project_id) else { return };
        if !matches!(s.phase, Phase::Ready { .. }) {
            return;
        }
        s.dirty = dirty;
        s.content_version = Some(version.clone());
        self.forward_dirty(project_id, mount_seq, dirty, version, true);
    }

    pub(super) fn forward_dirty(&mut self, project_id: &str, mount_seq: u64, dirty: bool, version: String, retry: bool) {
        let Some(s) = self.sessions.get_mut(project_id) else { return };
        s.dirty_notice += 1;
        let notice = s.dirty_notice;
        s.dirty_retry = None;
        self.post(
            "/canvas/dirty-changed",
            json!({"projectId": project_id, "mountSeq": mount_seq, "dirty": dirty, "contentVersion": version}),
            Then::Dirty { mount_seq, notice, retry },
        );
    }

    pub(super) fn dirty_settled(&mut self, project_id: &str, mount_seq: u64, notice: u64, retry: bool, result: Result<Value, Failure>, now: f64) {
        if !self.live(project_id, mount_seq) {
            return;
        }
        let outcome = match result {
            Err((code, _)) if code == "CANVAS_CLOSED" => return self.host_dropped(project_id, mount_seq),
            Err((code, message)) => {
                log::warn!("[canvas] canvas dirty notification failed for project {project_id}: {code}");
                let delay = self.options.get("autosaveMs").and_then(Value::as_f64).unwrap_or(DEFAULT_AUTOSAVE_MS);
                if let Some(s) = self.sessions.get_mut(project_id) {
                    s.save_error = Some(format!("Autosave could not reach Troupe: {message}"));
                    if retry && s.dirty_notice == notice {
                        s.dirty_retry = Some((notice, now + delay));
                    }
                }
                return;
            }
            Ok(v) => v,
        };
        let kind = outcome.get("kind").and_then(Value::as_str).unwrap_or_default();
        if kind == "superseded" {
            return;
        }
        let view = outcome.get("view").cloned().and_then(|v| serde_json::from_value::<View>(v).ok()).unwrap_or_default();
        let Some(s) = self.sessions.get_mut(project_id) else { return };
        s.apply_view(&view);
        if kind == "saveDue" && matches!(s.phase, Phase::Ready { .. }) {
            self.run_save(project_id, mount_seq);
        }
    }

    pub(super) fn disk_checked(&mut self, project_id: &str, mount_seq: u64, result: Result<Value, Failure>) {
        if !self.live(project_id, mount_seq) {
            return;
        }
        let Ok(outcome) = result else { return };
        let view = outcome.get("view").cloned().and_then(|v| serde_json::from_value::<View>(v).ok()).unwrap_or_default();
        if outcome.get("kind").and_then(Value::as_str) == Some("reload")
            && let Some(bytes) = outcome.get("bytes").and_then(Value::as_str)
        {
            let clean_at = view.content_version.clone();
            let Some(s) = self.sessions.get(project_id) else { return };
            if s.dirty || s.content_version != clean_at {
                return self.reload_not_taken(project_id, mount_seq, "the editor changed since it was judged clean");
            }
            return self.reload(project_id, bytes);
        }
        self.show_conflict(project_id, &view);
    }

    fn reload(&mut self, project_id: &str, bytes: &str) {
        let Some(mount_seq) = self.ready_mount(project_id) else { return };
        let params = json!({"bytes": bytes}).to_string();
        let Some(s) = self.sessions.get_mut(project_id) else { return };
        if let Some(reply) = s.send("host.openDocument", &params, Waiter::OpenDocument { reload: true }) {
            self.document_opened(project_id, mount_seq, reply, true);
        }
    }

    pub(super) fn reload_not_taken(&mut self, project_id: &str, mount_seq: u64, why: &str) {
        log::warn!("[canvas] canvas reload not applied for project {project_id}: {why}");
        self.post("/canvas/disk-changed", json!({"projectId": project_id, "mountSeq": mount_seq, "reloadFailed": true}), Then::Disk { mount_seq });
    }

    fn show_conflict(&mut self, project_id: &str, view: &View) {
        let Some(s) = self.sessions.get_mut(project_id) else { return };
        let had = s.conflict.is_some();
        s.apply_view(view);
        if view.conflict.is_some() && !had {
            let name = file_name(&s.doc_path).to_owned();
            self.toast(false, format!("{name} changed on disk"), Some("The canvas has unsaved changes. Choose which version to keep.".into()));
        }
    }

    pub(super) fn copy_snapshot(&mut self, project_id: &str, mount_seq: u64, reply: Reply) {
        match reply {
            Reply::Ok(snapshot) => self.post(
                "/canvas/resolve-conflict",
                json!({"projectId": project_id, "mountSeq": mount_seq, "choice": Choice::SaveCopy.wire(), "snapshot": snapshot}),
                Then::Resolve { mount_seq },
            ),
            Reply::Err { message, .. } => self.resolve_failed(project_id, &message),
        }
    }

    fn resolve_failed(&mut self, project_id: &str, message: &str) {
        log::warn!("[canvas] canvas conflict resolution failed for project {project_id}: {message}");
        if let Some(s) = self.sessions.get_mut(project_id) {
            s.resolving = false;
        }
        self.toast(true, "Could not resolve the canvas conflict", Some(message.to_owned()));
    }

    pub(super) fn resolved(&mut self, project_id: &str, mount_seq: u64, result: Result<Value, Failure>) {
        if !self.live(project_id, mount_seq) {
            return;
        }
        let outcome = match result {
            Err((_, message)) => return self.resolve_failed(project_id, &message),
            Ok(v) => v,
        };
        let view = outcome.get("view").cloned().and_then(|v| serde_json::from_value::<View>(v).ok()).unwrap_or_default();
        if let Some(s) = self.sessions.get_mut(project_id) {
            s.resolving = false;
            s.apply_view(&view);
        }
        if let Some(copy) = outcome.get("copyPath").and_then(Value::as_str) {
            self.toast(false, format!("Saved your version as {}", file_name(copy)), None);
        }
        match outcome.get("reload").and_then(Value::as_str) {
            Some(bytes) => self.reload(project_id, bytes),
            None => self.run_save(project_id, mount_seq),
        }
    }

    pub(super) fn host_save(&mut self, project_id: &str, call_id: u64, mount_seq: u64) -> Option<Reply> {
        let s = self.sessions.get_mut(project_id)?;
        if !matches!(s.phase, Phase::Ready { .. }) {
            return Some(Reply::failure(Code::Closed, "the canvas document is not loaded"));
        }
        s.agent_saves.push((call_id, mount_seq));
        self.run_save(project_id, mount_seq);
        None
    }

    /// Answers the agent saves waiting on this mount once a save settles.
    fn resolve_saves(&mut self, project_id: &str, mount_seq: u64, saved: bool) {
        let _ = mount_seq;
        let path = self.facts.file.get(project_id).and_then(|f| f.get("path")).and_then(Value::as_str).map(str::to_owned);
        let Some(s) = self.sessions.get_mut(project_id) else { return };
        let path = path.unwrap_or_else(|| file_name(&s.doc_path).to_owned());
        let error = s.save_error.clone();
        for (call_id, m) in std::mem::take(&mut s.agent_saves) {
            let reply = if saved {
                Reply::Ok(json!({"path": path}).to_string())
            } else {
                Reply::failure(Code::EngineError, error.clone().unwrap_or_else(|| "the canvas could not be saved".into()))
            };
            self.effects.push(Effect::Ws { kind: "canvas_reply", payload: gateway_reply(call_id, m, &reply) });
        }
    }
}

/// Whether `path` names a `.figcraft` document (`isCanvasDocPath`).
pub fn is_canvas_doc_path(path: &str) -> bool {
    path.to_ascii_lowercase().ends_with(CANVAS_DOC_EXTENSION)
}

/// The project's alphabetically-first `.figcraft` file name (`pickCanvasDoc`).
pub fn pick_canvas_doc<'a>(names: impl IntoIterator<Item = (&'a str, bool)>) -> Option<&'a str> {
    names.into_iter().filter(|(name, is_dir)| !is_dir && is_canvas_doc_path(name)).map(|(name, _)| name).min()
}

fn plain_path(path: &str) -> String {
    let p = path.replace('\\', "/");
    if let Some(rest) = p.strip_prefix("//?/UNC/").or_else(|| p.strip_prefix("//?/unc/")) {
        format!("//{rest}")
    } else if let Some(rest) = p.strip_prefix("//?/") {
        rest.to_owned()
    } else {
        p
    }
}

fn comparable_path(path: &str) -> String {
    let p = plain_path(path);
    let windows = (p.len() >= 2 && p.as_bytes()[0].is_ascii_alphabetic() && p.as_bytes()[1] == b':') || p.starts_with("//");
    if windows { p.to_lowercase() } else { p }
}

/// Whether two spellings name the same document (`sameCanvasDoc`).
pub fn same_doc(a: &str, b: &str) -> bool {
    comparable_path(a) == comparable_path(b)
}

/// `host.file` payload for `doc_path` under `project` (`canvasDocFile`): never an absolute path.
pub fn canvas_doc_file(doc_path: &str, project_name: Option<&str>, project_root: Option<&str>) -> Value {
    let doc = plain_path(doc_path);
    let file = file_name(&doc);
    let relative = match project_root {
        Some(root) if !root.is_empty() => {
            let prefix = format!("{}/", plain_path(root).trim_end_matches('/'));
            if comparable_path(&doc).starts_with(&comparable_path(&prefix)) {
                &doc[prefix.len()..]
            } else {
                file
            }
        }
        _ => file,
    };
    let name = if is_canvas_doc_path(file) { &file[..file.len() - CANVAS_DOC_EXTENSION.len()] } else { file };
    json!({
        "name": name,
        "path": relative,
        "location": project_name,
    })
}

fn file_name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}
