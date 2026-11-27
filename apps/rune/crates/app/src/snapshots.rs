//! Workspace snapshots (`pages/WorkspaceSnapshots.tsx`, `hooks/use-snapshots.ts`): the page's
//! own state, the create, restore and delete dialogs, and the host side of `model::snapshots`.
//! The list lives in this device's `snapshots/<projectId>` key. A restore closes the visible
//! project's terminals and spawns one per saved terminal, in its folder and shell.

use std::collections::BTreeMap;

use client::CallId;
use model::snapshots::{self, Effect, Event, Op, Read, Snapshot};
use model::workspace::{Event as WorkspaceEvent, Tab};
use rune::TextEdit;
use serde_json::{Value, json};
use troupe_protocol::ws::{WsError, WsRequestType};
use ui::overlay::ToastKind;

use crate::sync::{Pending, stored_value};
use crate::{Route, State};

/// The overlay keys of the three dialogs.
pub const CREATE: &str = "create-snapshot";
pub const RESTORE: &str = "restore-snapshot";
pub const DELETE: &str = "delete-snapshot";

/// The lines of each terminal a snapshot keeps (`min(1000, DEFAULT_SCROLLBACK_LIMIT / 10)`).
const SCROLLBACK_LINES: usize = 1000;

/// What a host reply answers.
#[derive(Clone, Debug, PartialEq)]
pub enum Call {
    /// A list read; `load` answers `loadSnapshots`, else the running mutation.
    Read { project_id: String, load: bool },
    Write(String),
    /// `getSnapshot` for a confirmed restore.
    Restore { project_id: String, id: String },
}

#[derive(Default)]
pub struct SnapshotsUi {
    pub model: snapshots::State,
    /// The project the page last loaded.
    pub loaded: Option<String>,
    pub name: TextEdit,
    pub description: TextEdit,
    pub creating: bool,
    /// The snapshot the restore confirm names, and whether its restore runs.
    pub restore: Option<Snapshot>,
    pub restoring: bool,
    pub delete: Option<Snapshot>,
    pub deleting: bool,
    /// The phone card whose name is a field, and the field.
    pub renaming: Option<String>,
    pub rename: TextEdit,
    /// The desktop card under the pointer: it shows its Rename and Delete.
    pub hovered: Option<String>,
    /// Terminals a restore spawned, by spawn call: the saved name each tab takes and the saved
    /// lines it shows before the shell's output (`pendingScrollback`).
    pub names: BTreeMap<CallId, (String, Vec<String>)>,
}

fn read_of(result: Result<Option<Value>, WsError>, key: &str) -> Read {
    match result {
        Ok(reply) => {
            let revision = reply.as_ref().and_then(|r| r.get("revision")).and_then(Value::as_u64).unwrap_or(0);
            match stored_value(reply) {
                Some(list) => Read::Found(list),
                None if revision == 0 => Read::Missing { error: format!("Key not found: {key}") },
                None => Read::Empty,
            }
        }
        Err(e) => Read::Failed { code: e.code, error: e.message },
    }
}

impl State {
    /// The page's list: the visible project's snapshots, newest first.
    pub(crate) fn snapshot_list(&self) -> Vec<&Snapshot> {
        let Some(project) = self.active_project() else { return Vec::new() };
        self.snapshots.model.of(&project.id).collect()
    }

    /// `useSnapshotLoader`: read the list again when the page shows another project.
    pub(crate) fn load_snapshots(&mut self) {
        let Some(project) = self.active_project().map(|p| p.id.clone()) else { return };
        if self.snapshots.loaded.as_deref() == Some(project.as_str()) {
            return;
        }
        self.snapshots.loaded = Some(project.clone());
        self.snapshots_step(Event::Load(project));
    }

    /// Opens the snapshots page (`navigate('/snapshots')`).
    pub(crate) fn open_snapshots(&mut self) {
        self.overlays.close(crate::DRAWER);
        self.route = Route::Snapshots;
        self.snapshots.loaded = None;
        self.load_snapshots();
    }

    pub(crate) fn snapshots_step(&mut self, event: Event) {
        for effect in self.snapshots.model.step(event) {
            self.snapshot_effect(effect);
        }
    }

    fn snapshot_effect(&mut self, effect: Effect) {
        match effect {
            Effect::Read { key, project_id, load } => {
                let key = self.device_key(&key);
                self.call(WsRequestType::StoreRead, json!({ "key": key }), Pending::Snapshots(Call::Read { project_id, load }));
            }
            Effect::WriteSnapshots { key, mut record } => {
                let project = key.trim_start_matches("snapshots/").to_owned();
                record["updatedAt"] = json!(snapshots::iso(crate::editor_host::epoch_ms()));
                self.write(&key, record, Pending::Snapshots(Call::Write(project)));
            }
            Effect::Log { level, source, message } => match level {
                model::Level::Info => log::info!("[{source}] {message}"),
                model::Level::Warn => log::warn!("[{source}] {message}"),
                model::Level::Error => log::error!("[{source}] {message}"),
            },
            Effect::Toast { message, .. } => {
                self.toasts.push(self.now, ToastKind::Warning, message, None);
            }
            Effect::Settled { op, id, error } => self.snapshot_settled(op, &id, error),
        }
    }

    fn snapshot_settled(&mut self, op: Op, id: &str, error: Option<String>) {
        let ui = &mut self.snapshots;
        match (op, error) {
            (Op::Create, None) => {
                ui.creating = false;
                self.close(CREATE);
            }
            (Op::Create, Some(e)) => {
                ui.creating = false;
                log::error!("[snapshots] failed to create the snapshot: {e}");
            }
            (Op::Rename, Some(e)) => {
                log::warn!("[snapshots] rename of {id} failed: {e}");
                self.toast_error("Could not rename the snapshot. Try again.", "");
            }
            (Op::Rename, None) => {}
            (Op::Delete, None) => {
                ui.deleting = false;
                self.close(DELETE);
            }
            (Op::Delete, Some(e)) => {
                ui.deleting = false;
                log::error!("[snapshots] failed to delete the snapshot: {e}");
            }
        }
    }

    pub(crate) fn snapshots_replied(&mut self, call: Call, result: Result<Option<Value>, WsError>) {
        match call {
            Call::Read { project_id, load } => {
                let read = read_of(result, &snapshots::key(&project_id));
                let event = if load { Event::Loaded { project_id, read } } else { Event::Read { project_id, read } };
                self.snapshots_step(event);
            }
            Call::Write(project_id) => {
                let result = result.map(|_| ()).map_err(|e| e.message);
                self.snapshots_step(Event::Written { project_id, result });
            }
            Call::Restore { project_id, id } => {
                let saved = match result {
                    Ok(reply) => stored_value(reply).and_then(|list| snapshots::saved(&list, &id)),
                    Err(e) => {
                        log::warn!("[snapshots] reading snapshot {id} for its restore failed: {}", e.code);
                        None
                    }
                };
                match saved {
                    Some(saved) => self.restore_from(&project_id, &saved),
                    None => {
                        log::error!("[snapshots] failed to restore snapshot {id}: Snapshot not found");
                        self.snapshots.restoring = false;
                    }
                }
            }
        }
    }

    pub(crate) fn open_create_snapshot(&mut self) {
        let ui = &mut self.snapshots;
        ui.name = TextEdit::new("");
        ui.description = TextEdit::new("");
        ui.creating = false;
        self.overlays.open(CREATE);
    }

    /// The Create button and Return in the dialog (`handleCreate`).
    pub(crate) fn create_snapshot(&mut self) {
        let name = self.snapshots.name.text.trim().to_owned();
        if name.is_empty() || self.snapshots.creating {
            return;
        }
        let Some(project) = self.active_project().map(|p| p.id.clone()) else {
            log::error!("[snapshots] failed to create the snapshot: No active project");
            return;
        };
        let description = Some(self.snapshots.description.text.trim().to_owned());
        let terminals = self.snapshot_terminals();
        let active = self.active_terminal();
        let now = crate::editor_host::epoch_ms();
        let id = format!("{now:.0}-{}", random_suffix());
        log::info!("[snapshots] creating a snapshot of {} terminals for {project}", terminals.len());
        self.snapshots.creating = true;
        self.snapshots_step(Event::Create {
            id,
            created_at: snapshots::iso(now),
            name,
            description,
            project_id: project,
            terminals,
            active_terminal_id: active,
        });
    }

    /// The visible project's terminals as a snapshot keeps them (`PersistedTerminal`).
    fn snapshot_terminals(&self) -> Vec<Value> {
        let Some(project) = self.active_project().map(|p| p.id.clone()) else { return Vec::new() };
        if self.workspace.project() != Some(project.as_str()) {
            return Vec::new();
        }
        model::workspace::leaves(self.workspace.root())
            .into_iter()
            .flat_map(|leaf| leaf.tabs.iter())
            .filter_map(|tab| match tab {
                Tab::Terminal { terminal_id, .. } => Some(terminal_id.clone()),
                _ => None,
            })
            .map(|id| {
                let session = self.terms.sessions.get(&id);
                let mut terminal = json!({
                    "id": id,
                    "name": self.terms.names.get(&id).cloned().unwrap_or_else(|| "Terminal".to_owned()),
                    "shell": session.map(|s| s.shell.clone()).unwrap_or_default(),
                });
                if let Some(cwd) = session.map(|s| s.cwd.clone()).filter(|c| !c.is_empty()) {
                    terminal["cwd"] = json!(cwd);
                }
                terminal["scrollback"] = json!(self.scrollback(&id));
                terminal
            })
            .collect()
    }

    /// The last [`SCROLLBACK_LINES`] lines a terminal shows, right-trimmed
    /// (`translateToString(true)`).
    fn scrollback(&self, id: &str) -> Vec<String> {
        let Some(view) = self.terms.views.get(id) else { return Vec::new() };
        let Ok(term) = view.cells.lock() else { return Vec::new() };
        let lines = term.lines();
        let from = lines.len().saturating_sub(SCROLLBACK_LINES);
        lines.iter().skip(from).map(|line| troupe_term::text::line_text(line, 0, line.cells.len(), true)).collect()
    }

    fn active_terminal(&self) -> Option<String> {
        match model::workspace::query(&self.workspace.store, model::workspace::Query::GetActiveTab) {
            model::workspace::Reply::Tab(Some(Tab::Terminal { terminal_id, .. })) => Some(terminal_id),
            _ => None,
        }
    }

    /// Whether the visible project runs any terminal (the restore warning).
    pub(crate) fn has_running_terminals(&self) -> bool {
        self.active_project().is_some_and(|p| self.terms.sessions.of_project(&p.id).next().is_some())
    }

    pub(crate) fn ask_restore(&mut self, snapshot: Snapshot) {
        self.snapshots.restore = Some(snapshot);
        self.overlays.open(RESTORE);
    }

    pub(crate) fn ask_delete(&mut self, snapshot: Snapshot) {
        self.snapshots.delete = Some(snapshot);
        self.overlays.open(DELETE);
    }

    /// Cancel, ✕, Escape or the scrim on the restore or delete confirm: refused while it runs.
    pub(crate) fn close_snapshot_confirm(&mut self, key: &str) {
        let busy = match key {
            RESTORE => self.snapshots.restoring,
            DELETE => self.snapshots.deleting,
            _ => false,
        };
        if !busy {
            self.close(key);
        }
    }

    /// The confirm's Restore (`handleRestore`): read the picked snapshot's terminals.
    pub(crate) fn confirm_restore(&mut self) {
        if self.snapshots.restoring {
            return;
        }
        let Some(snapshot) = self.snapshots.restore.clone() else { return };
        // A restore spawns into the shown project; a snapshot of another one restores nothing.
        let Some(project) = self.active_project().map(|p| p.id.clone()).filter(|p| *p == snapshot.project_id) else {
            log::error!("[snapshots] failed to restore snapshot {}: its project is not the one shown", snapshot.id);
            self.close(RESTORE);
            return;
        };
        self.snapshots.restoring = true;
        let key = self.device_key(&snapshots::key(&snapshot.project_id));
        let call = Call::Restore { project_id: project, id: snapshot.id };
        self.call(WsRequestType::StoreRead, json!({ "key": key }), Pending::Snapshots(call));
    }

    /// `restoreFromSnapshot`: close the project's terminal tabs (their PTYs die with them), spawn
    /// the saved ones, then leave for the workspace.
    fn restore_from(&mut self, project_id: &str, saved: &Value) {
        let name = saved["name"].as_str().unwrap_or_default().to_owned();
        if self.workspace.project() == Some(project_id) {
            let tabs: Vec<String> = model::workspace::leaves(self.workspace.root())
                .into_iter()
                .flat_map(|leaf| leaf.tabs.iter())
                .filter_map(|tab| matches!(tab, Tab::Terminal { .. }).then(|| tab.id().to_owned()))
                .collect();
            for tab in tabs {
                self.workspace.step(WorkspaceEvent::RemoveTab(tab));
            }
        }
        let terminals = saved["terminals"].as_array().cloned().unwrap_or_default();
        let active = saved["activeTerminalId"].as_str().map(str::to_owned);
        // The active terminal spawns last, so its tab is the one left selected.
        let (last, rest): (Vec<&Value>, Vec<&Value>) =
            terminals.iter().partition(|t| active.is_some() && t["id"].as_str() == active.as_deref());
        log::info!("[snapshots] restoring \"{name}\" with {} terminals into {project_id}", terminals.len());
        for terminal in rest.into_iter().chain(last) {
            let cwd = terminal["cwd"].as_str().map(str::to_owned);
            let shell = terminal["shell"].as_str().filter(|s| !s.is_empty()).map(str::to_owned);
            if let Some(call) = self.spawn_terminal_with(None, cwd, shell) {
                let tab_name = terminal["name"].as_str().unwrap_or("Terminal").to_owned();
                let lines = terminal["scrollback"]
                    .as_array()
                    .map(|l| l.iter().filter_map(Value::as_str).map(str::to_owned).collect())
                    .unwrap_or_default();
                self.snapshots.names.insert(call, (tab_name, lines));
            }
        }
        self.snapshots.restoring = false;
        self.snapshots.restore = None;
        self.close(RESTORE);
        self.route = Route::Workspace;
    }

    /// A spawn a restore asked for landed as terminal `id`: it takes the saved name.
    pub(crate) fn snapshot_terminal_named(&mut self, call: CallId, id: &str) {
        if let Some((name, lines)) = self.snapshots.names.remove(&call) {
            self.terms.names.insert(id.to_owned(), name);
            // `restoreScrollback`: the saved lines, then the shell's own output after them.
            if !lines.is_empty()
                && let Some(view) = self.terms.views.get(id)
                && let Ok(mut term) = view.cells.lock()
            {
                term.advance(format!("{}\r\n", lines.join("\r\n")).as_bytes());
            }
        }
    }

    /// A dialog closed: drop what it held.
    pub(crate) fn snapshot_dialog_closed(&mut self, key: &str) {
        let ui = &mut self.snapshots;
        match key {
            CREATE => ui.creating = false,
            RESTORE => ui.restore = None,
            _ => ui.delete = None,
        }
    }

    /// Whether the top overlay is a restore or delete confirm whose action runs: Escape and a
    /// back press leave it open.
    pub(crate) fn snapshot_confirm_busy(&self) -> bool {
        match self.overlays.top() {
            Some(RESTORE) => self.snapshots.restoring,
            Some(DELETE) => self.snapshots.deleting,
            _ => false,
        }
    }

    pub(crate) fn confirm_delete(&mut self) {
        if self.snapshots.deleting {
            return;
        }
        let Some(snapshot) = self.snapshots.delete.clone() else { return };
        self.snapshots.deleting = true;
        self.snapshots_step(Event::Delete(snapshot.id));
    }

    pub(crate) fn start_snapshot_rename(&mut self, snapshot: &Snapshot) {
        self.snapshots.renaming = Some(snapshot.id.clone());
        self.snapshots.rename = TextEdit::new(&snapshot.name);
    }

    /// Return or leaving the field (`confirmRename`): an empty name renames nothing.
    pub(crate) fn confirm_snapshot_rename(&mut self) {
        let Some(id) = self.snapshots.renaming.take() else { return };
        let name = self.snapshots.rename.text.trim().to_owned();
        self.snapshots.rename = TextEdit::new("");
        if !name.is_empty() {
            self.snapshots_step(Event::Rename { id, name });
        }
    }
}

/// Nine base-36 digits, as `Math.random().toString(36).substring(2, 11)` gives.
fn random_suffix() -> String {
    let mut n = crate::editor_host::epoch_ms().to_bits() ^ rand_bits();
    (0..9)
        .map(|_| {
            let digit = (n % 36) as u32;
            n /= 36;
            char::from_digit(digit, 36).unwrap_or('0')
        })
        .collect()
}

#[cfg(not(target_arch = "wasm32"))]
fn rand_bits() -> u64 {
    std::collections::hash_map::RandomState::new().hash_one(std::process::id())
}

#[cfg(target_arch = "wasm32")]
fn rand_bits() -> u64 {
    (js_sys::Math::random() * 9_007_199_254_740_992.0) as u64
}

#[cfg(not(target_arch = "wasm32"))]
use std::hash::BuildHasher;
