//! The editor's effects on the host: an editor tab on screen opens its file (`GET /fs/read`), a
//! save writes it (`POST /fs/write` with the hash it was read at), and the unsaved-changes
//! dialog's Save writes before the tab closes. Drafts follow into the workspace record, which
//! the app already persists, so an unsaved buffer comes back after a reload.

use std::collections::BTreeSet;

use client::http::{HttpError, HttpRequest, Method};
use model::Level;
use model::editor::{Draft, Effect, Event, Read};
use model::persistence::{Cursor, OpenFile, ViewMode};
use model::workspace::Tab;
use serde_json::{Value, json};
use ui::overlay::ToastKind;

use crate::State;
use crate::host::Fetch;

/// How long the buffer rests before its draft is copied into the record, as `CodeEditor`'s
/// 300 ms change debounce.
const DRAFT_DEBOUNCE: f64 = 0.3;

#[derive(Default)]
pub struct EditorHost {
    /// Files whose tab closes once their save lands (the unsaved-changes dialog's Save).
    pub(crate) closing: BTreeSet<String>,
    /// Each file's buffer revision, when it last changed, and whether its draft was copied
    /// since.
    drafted: std::collections::BTreeMap<String, (u64, f64, bool)>,
}

impl State {
    /// Opens `path` in the active pane (the explorer, a file link). `.figcraft` documents open
    /// in the project's singleton canvas tab instead of the text editor.
    pub fn open_file(&mut self, path: &str) {
        if ui::canvas::is_canvas_doc_path(path) {
            self.open_canvas_for_active(path);
            return;
        }
        self.workspace.step(model::workspace::Event::AddEditorTab(path.to_owned(), None));
        self.sync_editors();
    }

    /// Mounts a file for every editor tab of the visible project and drops files whose tab
    /// closed; then runs what the files asked.
    pub(crate) fn sync_editors(&mut self) {
        let mut shown = BTreeSet::new();
        for leaf in model::workspace::leaves(self.workspace.root()) {
            for tab in &leaf.tabs {
                if let Tab::Editor { file_path, .. } = tab {
                    shown.insert(file_path.clone());
                }
            }
        }
        if self.workspace.record().is_none() {
            return;
        }
        for path in &shown {
            if let Some(kind) = ui::viewers::viewer_for_path(path) {
                self.viewers.open(path, kind);
                continue;
            }
            if self.editors.docs.docs.contains_key(path) {
                continue;
            }
            let draft = self.workspace.files.get(path).and_then(|f| {
                f.draft_content
                    .clone()
                    .filter(|_| f.is_dirty)
                    .map(|content| Draft { content, saved_at: f.last_modified })
            });
            if self.workspace.files.get(path).is_none() {
                self.workspace.files.open.push(OpenFile {
                    file_path: path.clone(),
                    cursor_position: Cursor { line: 1, col: 1 },
                    scroll_top: 0.0,
                    view_mode: if ui::editor::is_markdown(path) { ViewMode::Preview } else { ViewMode::Code },
                    is_dirty: false,
                    draft_content: None,
                    last_modified: 0.0,
                });
            }
            self.editors.open(path, draft);
            let preview = self.workspace.files.get(path).is_some_and(|f| f.view_mode != ViewMode::Code);
            self.editors.set_preview(path, preview);
        }
        self.editors.follow_find();
        let gone: Vec<String> = self.editors.docs.docs.keys().filter(|p| !shown.contains(*p)).cloned().collect();
        for path in gone {
            self.editors.close(&path);
            self.editor_host.drafted.remove(&path);
        }
        let gone: Vec<String> = self.viewers.files.keys().filter(|p| !shown.contains(*p)).cloned().collect();
        for path in gone {
            self.viewers.close(&path);
        }
        self.run_editor_effects();
        self.run_viewer_effects();
    }

    pub(crate) fn run_editor_effects(&mut self) {
        // An effect may ask for more (a tab waiting to close saves again).
        while !self.editors.effects.is_empty() {
            self.run_editor_effects_once();
        }
    }

    fn run_editor_effects_once(&mut self) {
        for effect in std::mem::take(&mut self.editors.effects) {
            match effect {
                Effect::Read { path } => {
                    let url = format!("/fs/read?path={}", ui::composer::encode_query(&path));
                    match &mut self.host {
                        Some(host) => host.fetch(url, Fetch::FileRead(path)),
                        None => self.editors.step(Event::Loaded { path, result: Err(("NO_SERVER".into(), "No server".into())) }),
                    }
                }
                Effect::Write { path, content, expected } => {
                    let mut body = json!({ "path": path, "content": content });
                    if let Some(hash) = expected {
                        body["expected"] = json!({ "sha256": hash });
                    }
                    let command_id = Some(self.workspace.ids().next_id());
                    let request = HttpRequest { method: Method::Post, path: "/fs/write".into(), body: Some(body), command_id };
                    if let Some(host) = &mut self.host {
                        host.request(request, Fetch::FileWrite { path, content });
                    }
                }
                Effect::ReadBack { path, content } => {
                    let url = format!("/fs/read?path={}", ui::composer::encode_query(&path));
                    match &mut self.host {
                        Some(host) => host.fetch(url, Fetch::FileReadBack { path, content }),
                        None => self.editors.step(Event::ReadBack { path, content, disk: None }),
                    }
                }
                Effect::Saved { path, clean } => {
                    let name = path.rsplit(['/', '\\']).next().unwrap_or(&path).to_owned();
                    self.toasts.push(self.now, ToastKind::Success, format!("{name} saved"), None);
                    if !clean {
                        // Edits landed while the write was in flight: a tab waiting to close
                        // saves again, and closes only once the disk holds them.
                        if self.editor_host.closing.contains(&path) {
                            self.editors.save(&path);
                        }
                        continue;
                    }
                    if let Some(file) = self.workspace.files.open.iter_mut().find(|f| f.file_path == path) {
                        file.is_dirty = false;
                        file.draft_content = None;
                        self.workspace.revision += 1;
                    }
                    if self.editor_host.closing.remove(&path) {
                        self.workspace.saved(&path);
                    }
                }
                Effect::SaveFailed { path, message } => {
                    self.toast_error("Save failed", &message);
                    self.editor_host.closing.remove(&path);
                }
                Effect::ExternalChange { path } => {
                    let name = path.rsplit(['/', '\\']).next().unwrap_or(&path).to_owned();
                    self.toasts.push(self.now, ToastKind::Error, "File changed externally", Some(&name));
                    self.editor_host.closing.remove(&path);
                }
                Effect::Log { level, message } => match level {
                    Level::Error => log::error!("[editor] {message}"),
                    Level::Warn => log::warn!("[editor] {message}"),
                    Level::Info => log::info!("[editor] {message}"),
                },
            }
        }
    }

    /// An answer to a read or write the editor asked for.
    pub(crate) fn editor_fetched(&mut self, tag: Fetch, result: Result<Value, HttpError>) {
        match tag {
            Fetch::FileRead(path) => {
                let result = result.map_err(refusal).and_then(|v| {
                    let content = v
                        .get("content")
                        .and_then(Value::as_str)
                        .ok_or_else(|| ("UNREADABLE".to_owned(), "the read had no text content".to_owned()))?;
                    Ok(Read {
                        content: content.to_owned(),
                        modified_at: v.get("modifiedAt").and_then(Value::as_f64).unwrap_or(0.0),
                        content_hash: v.get("contentHash").and_then(Value::as_str).map(str::to_owned),
                    })
                });
                self.editors.step(Event::Loaded { path, result });
            }
            Fetch::FileWrite { path, content } => {
                let result = result.map(|_| ()).map_err(refusal);
                self.editors.step(Event::Written { path, content, result });
            }
            Fetch::FileReadBack { path, content } => {
                let disk = result.ok().and_then(|v| v.get("content").and_then(Value::as_str).map(str::to_owned));
                self.editors.step(Event::ReadBack { path, content, disk });
            }
            _ => {}
        }
        self.run_editor_effects();
    }

    /// The unsaved-changes dialog's Save: write the buffer with the hash it was read at, and
    /// close the tab once the disk holds every edit (a write in flight lands first, then the
    /// newer text is written). A tab whose file never mounted mounts it first, so its draft is
    /// written with the same check; with no draft there is nothing to write.
    pub(crate) fn save_and_close(&mut self, path: String) {
        self.editor_host.closing.insert(path.clone());
        if !self.editors.docs.docs.contains_key(&path) {
            self.sync_editors();
        }
        if self.editors.docs.docs.contains_key(&path) {
            self.editors.save(&path);
        } else if self.workspace.files.get(&path).is_none_or(|f| !f.is_dirty || f.draft_content.is_none()) {
            self.editor_host.closing.remove(&path);
            self.workspace.saved(&path);
        } else {
            self.editor_host.closing.remove(&path);
            log::warn!("[editor] save and close: {path} has a draft but no open file to save it with");
            self.toast_error("Save failed", "Open the file's tab and save it there");
        }
        self.run_editor_effects();
    }

    /// Copies every file's draft into the workspace record now, without the debounce: before
    /// the record is saved and its project is left.
    pub(crate) fn flush_drafts(&mut self) {
        self.copy_drafts(self.now, true);
    }

    /// Copies each file's dirty state now, and its draft once the buffer rests, into the
    /// workspace record.
    pub(crate) fn sync_drafts(&mut self, now: f64) {
        self.copy_drafts(now, false);
    }

    /// Copies drafts that rested [`DRAFT_DEBOUNCE`], or every one when `forced`.
    fn copy_drafts(&mut self, now: f64, forced: bool) {
        let mut changed = false;
        for (path, doc) in &self.editors.docs.docs {
            let revision = doc.editor.revision();
            let entry = self.editor_host.drafted.entry(path.clone()).or_insert((revision, now, true));
            if entry.0 != revision {
                *entry = (revision, now, false);
            }
            let Some(file) = self.workspace.files.open.iter_mut().find(|f| &f.file_path == path) else {
                continue;
            };
            let preview = self.editors.previewing(path);
            if ui::editor::is_markdown(path) && (file.view_mode != ViewMode::Code) != preview {
                file.view_mode = if preview { ViewMode::Preview } else { ViewMode::Code };
                changed = true;
            }
            let attention = doc.needs_attention();
            let (line, col) = doc.editor.line_col();
            let cursor = Cursor { line: line as u32, col: col as u32 };
            if file.is_dirty != attention || file.cursor_position != cursor {
                file.is_dirty = attention;
                file.cursor_position = cursor;
                changed = true;
            }
            if !attention && file.draft_content.is_some() {
                file.draft_content = None;
                changed = true;
            }
            if attention && (forced || (!entry.2 && now - entry.1 >= DRAFT_DEBOUNCE)) {
                entry.2 = true;
                let draft = doc.draft();
                let content = draft.as_ref().map(|d| d.content.clone());
                if file.draft_content != content {
                    file.draft_content = content;
                    file.last_modified = draft.map_or(doc.modified_at, |d| d.saved_at);
                    changed = true;
                }
            }
        }
        if changed {
            self.workspace.revision += 1;
        }
    }

    /// When the next draft copy is due.
    pub(crate) fn next_draft_sync(&self) -> Option<f64> {
        self.editors
            .docs
            .docs
            .keys()
            .filter_map(|path| self.editor_host.drafted.get(path))
            .filter(|(_, _, copied)| !copied)
            .map(|(_, at, _)| at + DRAFT_DEBOUNCE)
            .reduce(f64::min)
    }
}

/// An HTTP failure as the host's code and message.
fn refusal(e: HttpError) -> (String, String) {
    match e {
        HttpError::Refused { code, message, .. } => (code, message),
        HttpError::Network(m) => ("NETWORK_ERROR".to_owned(), m),
        HttpError::InProgress => ("COMMAND_IN_PROGRESS".to_owned(), String::new()),
    }
}

/// The app's input method handler: compositions go to the focused command editor, else to the
/// focused file editor.
pub(crate) fn on_ime(s: &mut State, ime: &rune::Ime, tx: &mut rune::Tx) -> bool {
    if let Some(id) = tx.focused().and_then(crate::command_editor::field_terminal) {
        let id = id.to_owned();
        return s.command_editor_ime(&id, ime);
    }
    if s.canvas_ime(ime, tx) {
        return true;
    }
    s.editors.ime(ime)
}

/// Milliseconds since the epoch: the clock file times and drafts are compared on.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn epoch_ms() -> f64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0.0, |d| d.as_secs_f64() * 1000.0)
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn epoch_ms() -> f64 {
    js_sys::Date::now()
}
