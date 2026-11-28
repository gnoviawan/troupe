//! The explorer's effects on the host and the workspace: folders list through `GET /fs/ls`,
//! creates, renames and deletes post to `/fs/write`, `/fs/mkdir`, `/fs/rename` and
//! `/fs/delete`; a file opens in an editor tab, a rename follows open tabs and a delete closes
//! them; a file dragged onto a pane opens there (`usePaneDnd`'s file drop).

use std::cell::RefCell;
use std::collections::BTreeSet;

use client::http::{HttpError, HttpRequest, Method};
use model::Level;
use model::explorer::{Effect, Entry, Event, Kind, Op};
use model::workspace::{Direction, Position, Tab};
use rune::{Cx, Phase};
use serde_json::{Value, json};
use ui::explorer::{FileDrag, Hooks};
use ui::overlay::ToastKind;
use ui::workspace::{DropTarget, Geometry, TabDrag};

use crate::State;
use crate::host::Fetch;

#[derive(Default)]
pub struct ExplorerHost {
    /// The panes' rects from the last frame, for a file drag to test (the workspace view keeps
    /// its own copy private).
    geometry: RefCell<Geometry>,
}

pub(crate) fn hooks() -> Hooks<State> {
    Hooks { file_drag, open_terminal }
}

pub(crate) fn get(s: &State) -> &ui::explorer::Explorer {
    &s.explorer
}

pub(crate) fn lens(s: &mut State) -> &mut ui::explorer::Explorer {
    &mut s.explorer
}

impl State {
    /// Follows the visible project's folder, then runs what the explorer asked.
    pub(crate) fn sync_explorer(&mut self) {
        let root = self.active_project().and_then(|p| p.path.clone());
        if self.explorer.tree.root != root {
            self.explorer.step(Event::Root(root));
        }
        self.run_explorer_effects();
    }

    pub(crate) fn run_explorer_effects(&mut self) {
        for effect in std::mem::take(&mut self.explorer.effects) {
            match effect {
                Effect::List(dir) => {
                    let url = format!("/fs/ls?path={}", ui::composer::encode_query(&dir));
                    match &mut self.host {
                        Some(host) => host.fetch(url, Fetch::ExplorerList(dir)),
                        None => self.explorer.step(Event::Listed { dir, result: Err("No server".into()) }),
                    }
                }
                Effect::Run(op) => self.run_op(op),
                Effect::Open(path) => self.open_file(&path),
                Effect::Renamed { from, to } => self.follow_rename(&from, &to),
                Effect::Deleted { path: gone } => {
                    for path in self.open_editor_paths() {
                        if under(&path, &gone).is_some() {
                            self.workspace.step(model::workspace::Event::RemoveTab(model::workspace::editor_tab_id(&path)));
                        }
                    }
                    self.canvas_file_changed(&gone);
                }
                Effect::Reveal(_) => {}
                Effect::Toast(message) => {
                    self.toasts.push(self.now, ToastKind::Error, message, None);
                }
                Effect::Log { level, message } => match level {
                    Level::Error => log::error!("[explorer] {message}"),
                    Level::Warn => log::warn!("[explorer] {message}"),
                    Level::Info => log::info!("[explorer] {message}"),
                },
            }
        }
        self.run_editor_effects();
    }

    /// `renameOpenFile` for every open file at or under `from`: the buffer with its unsaved
    /// edits, its record (draft and dirty flag) and its tab move to the new path, so the tab
    /// keeps the edits instead of reading the renamed file from disk.
    fn follow_rename(&mut self, from: &str, to: &str) {
        let mut paths: BTreeSet<String> = self.open_editor_paths().into_iter().collect();
        paths.extend(self.editors.docs.docs.keys().cloned());
        paths.extend(self.workspace.files.open.iter().map(|f| f.file_path.clone()));
        for path in paths {
            let Some(rest) = under(&path, from) else { continue };
            let moved = format!("{to}{rest}");
            log::info!("[explorer] open file follows the rename: {path} -> {moved}");
            self.editors.rename(&path, &moved);
            self.workspace.files.open.retain(|f| f.file_path != moved);
            if let Some(file) = self.workspace.files.open.iter_mut().find(|f| f.file_path == path) {
                file.file_path.clone_from(&moved);
            }
            if self.editor_host.closing.remove(&path) {
                self.editor_host.closing.insert(moved.clone());
            }
            self.workspace.step(model::workspace::Event::RenameEditorTab(path, moved));
        }
        self.workspace.revision += 1;
    }

    fn open_editor_paths(&self) -> Vec<String> {
        model::workspace::leaves(self.workspace.root())
            .into_iter()
            .flat_map(|leaf| leaf.tabs.iter())
            .filter_map(|tab| match tab {
                Tab::Editor { file_path, .. } => Some(file_path.clone()),
                _ => None,
            })
            .collect()
    }

    fn run_op(&mut self, op: Op) {
        // A new file is written only where nothing is: the host refuses to replace one.
        let (path, body) = match &op {
            Op::Create { path, kind: Kind::File } => ("/fs/write", json!({ "path": path, "content": "", "expected": "absent" })),
            Op::Create { path, kind: Kind::Folder } => ("/fs/mkdir", json!({ "path": path })),
            Op::Rename { from, to } => ("/fs/rename", json!({ "from": from, "to": to })),
            Op::Copy { from, to } => ("/fs/copy", json!({ "from": from, "to": to })),
            Op::Delete { entry } => ("/fs/delete", json!({ "path": entry.path, "recursive": entry.dir })),
        };
        log::info!("[explorer] {path} {op:?}");
        let command_id = Some(self.workspace.ids().next_id());
        let request = HttpRequest { method: Method::Post, path: path.into(), body: Some(body), command_id };
        match &mut self.host {
            Some(host) => host.request(request, Fetch::ExplorerOp(op)),
            None => self.explorer.step(Event::Done { op, result: Err("No server".into()) }),
        }
    }

    pub(crate) fn explorer_fetched(&mut self, tag: Fetch, result: Result<Value, HttpError>) {
        match tag {
            Fetch::ExplorerList(dir) => {
                let result = result
                    .map_err(|e| e.to_string())
                    .and_then(|v| Entry::parse_listing(&v).ok_or_else(|| "the listing was not a list".to_owned()));
                self.explorer.step(Event::Listed { dir, result });
            }
            Fetch::ExplorerOp(op) => {
                let result = result.map(|_| ()).map_err(|e| match e {
                    HttpError::Refused { message, .. } => message,
                    other => other.to_string(),
                });
                self.explorer.step(Event::Done { op, result });
            }
            _ => return,
        }
        self.run_explorer_effects();
    }

    /// Records the panes' rects for a file drag, as the workspace view does for a tab drag.
    pub(crate) fn measure_panes(&self, cx: &Cx<State>) {
        let mut g = Geometry::default();
        for leaf in model::workspace::leaves(self.workspace.root()) {
            if let Some(rect) = cx.layout(&format!("pane-content-{}", leaf.id)) {
                g.contents.push((leaf.id.clone(), rect));
            }
        }
        *self.explorer_host.geometry.borrow_mut() = g;
    }
}

/// `path` at or under `dir`: what follows `dir` in it.
fn under<'a>(path: &'a str, dir: &str) -> Option<&'a str> {
    let rest = path.strip_prefix(dir)?;
    (rest.is_empty() || rest.starts_with(['/', '\\'])).then_some(rest)
}

/// A file row's drag: the panes' drop zones light up as a tab drag's do, and the release opens
/// the file in the pane, or in a new split at its edge.
fn file_drag(s: &mut State, drag: &FileDrag, phase: Phase, cancelled: bool) {
    let fullscreen = s.workspace.tree().fullscreen_pane_id.clone();
    let target = s.explorer_host.geometry.borrow().target(drag.point, fullscreen.as_deref());
    if phase != Phase::Up {
        s.workspace.drag = Some(TabDrag { tab: String::new(), source: String::new(), point: drag.point, moving: true, target });
        return;
    }
    s.workspace.drag = None;
    if cancelled {
        return;
    }
    let Some(DropTarget::Zone { pane, position }) = target else { return };
    let path = drag.path.clone();
    log::info!("[explorer] dropped {path} on pane {pane} ({position:?})");
    if position == Position::Center {
        s.workspace.step(model::workspace::Event::AddEditorTab(path, Some(pane)));
    } else {
        let direction = if matches!(position, Position::Left | Position::Right) { Direction::Horizontal } else { Direction::Vertical };
        let tab = Tab::Editor { id: model::workspace::editor_tab_id(&path), file_path: path };
        s.workspace.step(model::workspace::Event::SplitPane(pane, direction, tab, Some(position), None));
    }
    s.sync_editors();
}

fn open_terminal(s: &mut State, dir: &str) {
    s.spawn_terminal_in(None, Some(dir.to_owned()));
}
