//! The per-project workspace record (`workspace/<projectId>`, device scope): the open
//! editor files, the explorer's expanded folders and the pane layout. Port of
//! `hooks/editor-persistence/` (`types.ts`, `persist.ts`, the read half of `hydrate.ts`)
//! and of the editor store's open-file list (`stores/editor-store.ts`) as far as the
//! record needs it.
//!
//! The app reads the record when a project becomes visible ([`hydrate`]), keeps the
//! [`Files`] next to the workspace tree, and writes [`record`] after every change through
//! the host store, wrapped as the web client wraps every value ([`stored`]).

pub mod pane_tree;

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::workspace::{self, PaneNode, ProjectWorkspace, Tab, tree};
use crate::{Effect, Ids, Level};

pub use pane_tree::{PersistedNode, TabRef, TerminalInfo};

/// The device key of a project's record, under `device/<deviceId>/`.
#[must_use]
pub fn key(project_id: &str) -> String {
    format!("workspace/{project_id}")
}

/// The envelope version every web client write carries (`STORED_VALUE_VERSION`).
pub const STORED_VALUE_VERSION: u32 = 1;

/// A value as the web client writes it to the host store: `{ _version, data }`.
#[must_use]
pub fn stored(data: Value) -> Value {
    json!({ "_version": STORED_VALUE_VERSION, "data": data })
}

/// A stored value without its envelope; a value written without one passes through.
#[must_use]
pub fn unwrap_stored(raw: Value) -> Value {
    match raw {
        Value::Object(mut map) if map.contains_key("_version") => {
            map.remove("data").unwrap_or(Value::Null)
        }
        other => other,
    }
}

/// `value` with every whole float written as an integer, as `JSON.stringify` prints numbers:
/// a share of `50.0` goes over the wire as `50`.
#[must_use]
pub fn js_numbers(value: Value) -> Value {
    const MAX_SAFE: f64 = 9_007_199_254_740_991.0;
    match value {
        Value::Number(n) => match n.as_f64() {
            Some(f) if n.is_f64() && f.fract() == 0.0 && f.abs() <= MAX_SAFE => {
                Value::from(f as i64)
            }
            _ => Value::Number(n),
        },
        Value::Array(items) => Value::Array(items.into_iter().map(js_numbers).collect()),
        Value::Object(map) => Value::Object(
            map.into_iter()
                .map(|(key, value)| (key, js_numbers(value)))
                .collect(),
        ),
        other => other,
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cursor {
    pub line: u32,
    pub col: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ViewMode {
    #[default]
    Code,
    Preview,
    Markdown,
}

/// One open editor file as the record keeps it (`PersistedEditorFile`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenFile {
    pub file_path: String,
    pub cursor_position: Cursor,
    pub scroll_top: f64,
    pub view_mode: ViewMode,
    pub is_dirty: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub draft_content: Option<String>,
    pub last_modified: f64,
}

impl OpenFile {
    /// Unsaved: dirty, or holding a draft the file on disk has not taken.
    #[must_use]
    pub fn needs_attention(&self) -> bool {
        self.is_dirty
    }
}

/// `PersistedEditorState`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Record {
    pub open_files: Vec<OpenFile>,
    pub active_file_path: Option<String>,
    pub expanded_dirs: Vec<String>,
    pub active_tab_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pane_layout: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_pane_id: Option<String>,
}

/// The editor store's open files, in the order they were opened, and the active one.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Files {
    pub open: Vec<OpenFile>,
    pub active: Option<String>,
    /// The explorer's expanded folders, carried through unchanged.
    pub expanded_dirs: Vec<String>,
}

impl Files {
    #[must_use]
    pub fn get(&self, path: &str) -> Option<&OpenFile> {
        self.open.iter().find(|file| file.file_path == path)
    }

    /// `closeFile`: an active file hands the focus to the last one still open.
    pub fn close(&mut self, path: &str) {
        self.open.retain(|file| file.file_path != path);
        if self.active.as_deref() == Some(path) {
            self.active = self.open.last().map(|file| file.file_path.clone());
        }
    }

    /// `use-terminal-tab-sync`: the active file follows the active pane's active tab.
    pub fn follow(&mut self, workspace: &ProjectWorkspace) {
        let active = tree::find_leaf(&workspace.root, &workspace.active_pane_id).and_then(|leaf| {
            leaf.tabs
                .iter()
                .find(|tab| Some(tab.id()) == leaf.active_tab_id.as_deref())
        });
        self.active = match active {
            Some(Tab::Editor { file_path, .. }) => Some(file_path.clone()),
            _ => None,
        };
    }
}

/// `persistState`: the record of the visible project.
#[must_use]
pub fn record(workspace: &ProjectWorkspace, files: &Files) -> Record {
    let active_tab_id = tree::find_leaf(&workspace.root, &workspace.active_pane_id)
        .and_then(|leaf| leaf.active_tab_id.clone());
    Record {
        open_files: files.open.clone(),
        active_file_path: files.active.clone(),
        expanded_dirs: files.expanded_dirs.clone(),
        active_tab_id,
        pane_layout: Some(
            serde_json::to_value(pane_tree::serialize(&workspace.root)).unwrap_or(Value::Null),
        ),
        active_pane_id: Some(workspace.active_pane_id.clone()),
    }
}

/// `filterExpandedDirsByRoot`: the folders under the project's root, with `/` separators.
#[must_use]
pub fn filter_expanded_dirs(dirs: &[String], root: Option<&str>) -> Vec<String> {
    let Some(root) = root.filter(|root| !root.is_empty()) else {
        return Vec::new();
    };
    let root = root.replace('\\', "/");
    dirs.iter()
        .map(|dir| dir.replace('\\', "/"))
        .filter(|dir| *dir == root || dir.starts_with(&format!("{root}/")))
        .collect()
}

/// What a project shows once its record is read.
#[derive(Clone, Debug, PartialEq)]
pub struct Hydrated {
    pub tree: ProjectWorkspace,
    pub files: Files,
}

/// The read half of `hydrateProject`: `raw` is the stored value (`None` when the device has
/// no record for the project). Every open file is taken as readable; the web client drops
/// the ones it can no longer read from disk.
pub fn hydrate(
    raw: Option<Value>,
    root_path: Option<&str>,
    live_terminals: &[TerminalInfo],
    terminal_layout: Option<&[TerminalInfo]>,
    ids: &mut dyn Ids,
    fx: &mut Vec<Effect>,
) -> Hydrated {
    let mut fresh = || Hydrated {
        tree: tree::create_project_workspace(ids),
        files: Files::default(),
    };
    let Some(raw) = raw.map(unwrap_stored).filter(|v| !v.is_null()) else {
        return fresh();
    };
    let record: Record = match serde_json::from_value(raw.clone()) {
        Ok(record) => record,
        Err(error) => {
            fx.push(Effect::log(
                Level::Warn,
                "useEditorPersistence.hydrateProject",
                format!("unreadable workspace record ignored: {error}"),
            ));
            return fresh();
        }
    };
    let files = Files {
        open: record.open_files.clone(),
        active: record.active_file_path.clone(),
        expanded_dirs: filter_expanded_dirs(&record.expanded_dirs, root_path),
    };
    let tree = match &record.pane_layout {
        Some(layout) => {
            let restored = pane_tree::deserialize(layout, ids, fx);
            let open: BTreeSet<String> = files.open.iter().map(|f| f.file_path.clone()).collect();
            let clean = pane_tree::reconcile_terminal_tabs(
                &restored,
                &open,
                live_terminals,
                terminal_layout,
                ids,
            );
            build_project_workspace(&clean, record.active_pane_id.as_deref(), ids)
        }
        None => legacy_editor_workspace(&files, record.active_tab_id.as_deref(), ids),
    };
    Hydrated { tree, files }
}

/// `buildProjectWorkspace`: normalize the tree and keep the active pane when it is a leaf.
pub fn build_project_workspace(
    root: &PaneNode,
    active_pane_id: Option<&str>,
    ids: &mut dyn Ids,
) -> ProjectWorkspace {
    let root = tree::normalize_pane_tree(root, ids);
    let active_pane_id = tree::resolve_leaf_id(&root, active_pane_id);
    ProjectWorkspace {
        root,
        active_pane_id,
        fullscreen_pane_id: None,
        agent_launcher_pane_id: None,
        agent_browser_pane_id: None,
    }
}

/// A record from before the pane layout: one leaf with the open files as tabs.
fn legacy_editor_workspace(
    files: &Files,
    active_tab_id: Option<&str>,
    ids: &mut dyn Ids,
) -> ProjectWorkspace {
    let mut fresh = tree::create_project_workspace(ids);
    let mut seen = BTreeSet::new();
    let tabs: Vec<Tab> = files
        .open
        .iter()
        .filter(|file| seen.insert(file.file_path.clone()))
        .map(|file| Tab::Editor {
            id: workspace::ids::editor(&file.file_path),
            file_path: file.file_path.clone(),
        })
        .collect();
    let active = match active_tab_id {
        Some(id) if !id.is_empty() && tabs.iter().any(|tab| tab.id() == id) => Some(id.to_owned()),
        _ => tabs.last().map(|tab| tab.id().to_owned()),
    };
    if let PaneNode::Leaf(leaf) = &mut fresh.root {
        leaf.tabs = tabs;
        leaf.active_tab_id = active;
    }
    fresh
}

#[cfg(test)]
mod tests;
