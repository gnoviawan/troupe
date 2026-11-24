//! The pane workspace (`components/workspace/`): split panes with their tab bars, the `+`
//! menu, closing tabs (asking first when an editor holds unsaved changes), resizing splits,
//! dragging tabs onto a pane's drop zone or another tab bar, and the drop zone itself.
//!
//! [`Workspace`] is the state the app keeps: the model's pane tree and open files plus the
//! gesture in flight. Its methods are the user's actions; each one runs the model's store
//! (`model::workspace::step`) and marks the record changed. The app reads and writes the
//! record (`model::persistence`) and drains [`Workspace::requests`] for what needs the server.
//! [`view`] draws it for an app state `S` reached through a lens.

mod dnd;
mod view;

#[cfg(test)]
mod dnd_tests;

use std::collections::BTreeMap;

use model::persistence::{self, Files, Hydrated, Record};
use model::workspace::{self as store, Event, PaneNode, Position, ProjectWorkspace, Tab};
use model::{Effect, Ids};

pub use dnd::{DropTarget, Geometry, Side, zone_at, zone_rect};
pub use view::{Slots, resized, view};

/// What the workspace asks of the app, in order.
#[derive(Clone, Debug, PartialEq)]
pub enum Request {
    /// The `+` menu's "New chat" in this pane.
    NewChat { pane: String },
    /// The `+` menu's "New terminal" in this pane.
    NewTerminal { pane: String },
    /// Write `content` to `path` (the unsaved-changes dialog's Save), then close its tab.
    SaveAndClose { path: String, content: String },
}

/// A tab under the pointer, from press to release.
#[derive(Clone, Debug, PartialEq)]
pub struct TabDrag {
    pub tab: String,
    pub source: String,
    /// The pointer in window points.
    pub point: [f32; 2],
    /// Past the 4 pt slop: the tab is being dragged, not pressed.
    pub moving: bool,
    pub target: Option<DropTarget>,
}

/// A split's shares while its handle is dragged; committed on release.
#[derive(Clone, Debug, PartialEq)]
pub struct Sizing {
    pub split: String,
    /// The shares when the press landed.
    pub start: Vec<f64>,
    pub sizes: Vec<f64>,
}

/// Version 4 UUIDs from a splitmix64 stream, for the pane ids the store mints.
pub struct Uuids(u64);

impl Uuids {
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }

    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }
}

impl Ids for Uuids {
    fn next_id(&mut self) -> String {
        let (a, b) = (self.next(), self.next());
        let variant = 0x8000 | (b >> 48) & 0x3fff;
        format!(
            "{:08x}-{:04x}-4{:03x}-{:04x}-{:012x}",
            a >> 32,
            (a >> 16) & 0xffff,
            a & 0xfff,
            variant,
            b & 0xffff_ffff_ffff
        )
    }
}

pub struct Workspace {
    pub store: store::State,
    pub files: Files,
    /// Open files of the projects whose trees are parked.
    parked_files: BTreeMap<String, Files>,
    /// The visible project's record has been read and its tree is the store's.
    pub ready: bool,
    pub drag: Option<TabDrag>,
    pub sizing: Option<Sizing>,
    /// The pane whose `+` menu is open.
    pub menu: Option<String>,
    /// The file whose unsaved-changes dialog is open.
    pub confirm_close: Option<String>,
    pub requests: Vec<Request>,
    /// Log lines the store produced, for the app to write.
    pub logs: Vec<Effect>,
    /// Bumped by every change the record could see.
    pub revision: u64,
    ids: Box<dyn Ids>,
}

impl Workspace {
    pub fn new(mut ids: Box<dyn Ids>) -> Self {
        Self {
            store: store::State::new(ids.as_mut()),
            files: Files::default(),
            parked_files: BTreeMap::new(),
            ready: false,
            drag: None,
            sizing: None,
            menu: None,
            confirm_close: None,
            requests: Vec::new(),
            logs: Vec::new(),
            revision: 0,
            ids,
        }
    }

    /// The tree on screen.
    #[must_use]
    pub fn tree(&self) -> &ProjectWorkspace {
        &self.store.tree
    }

    #[must_use]
    pub fn project(&self) -> Option<&str> {
        Some(self.store.active_project_id.as_str()).filter(|id| !id.is_empty())
    }

    /// Whether `project_id` has a parked tree, so it shows without reading its record.
    #[must_use]
    pub fn has_tree(&self, project_id: &str) -> bool {
        self.store.parked_trees.contains_key(project_id)
    }

    pub fn ids(&mut self) -> &mut dyn Ids {
        self.ids.as_mut()
    }

    /// Run one store action and follow it with the open-file list.
    pub fn step(&mut self, event: Event) -> store::Out {
        let out = store::step(&mut self.store, event, self.ids.as_mut());
        self.logs.extend(out.effects.iter().cloned());
        self.files.follow(&self.store.tree);
        self.revision += 1;
        out
    }

    /// Show `project_id`: its parked tree, else the tree its record holds (`hydrated`).
    pub fn activate(&mut self, project_id: &str, hydrated: Option<Hydrated>) {
        if let Some(leaving) = self.project().map(str::to_owned)
            && leaving != project_id
        {
            let files = std::mem::take(&mut self.files);
            self.parked_files.insert(leaving, files);
        }
        let (tree, files) = match hydrated {
            Some(Hydrated { tree, files, .. }) => (Some(tree), files),
            None => (
                None,
                self.parked_files.remove(project_id).unwrap_or_default(),
            ),
        };
        self.files = files;
        self.drag = None;
        self.sizing = None;
        self.menu = None;
        self.confirm_close = None;
        self.step(Event::ActivateProject(project_id.to_owned(), tree));
        self.step(Event::HideAgentLauncher);
        self.ready = true;
    }

    /// The record of the visible project.
    #[must_use]
    pub fn record(&self) -> Option<Record> {
        self.ready
            .then(|| persistence::record(&self.store.tree, &self.files))
    }

    /// Press on a tab: it becomes its pane's active tab and the pane the active pane.
    pub fn select(&mut self, pane: &str, tab: &str) {
        self.step(Event::SetActiveTab(pane.to_owned(), tab.to_owned()));
        self.step(Event::SetActivePane(pane.to_owned()));
    }

    pub fn focus_pane(&mut self, pane: &str) {
        if self.store.tree.active_pane_id != pane {
            self.step(Event::SetActivePane(pane.to_owned()));
        }
    }

    fn tab(&self, tab_id: &str) -> Option<Tab> {
        store::find_pane_containing_tab(&self.store.tree.root, tab_id).and_then(|leaf| {
            leaf.tabs.iter().find(|tab| tab.id() == tab_id).cloned()
        })
    }

    /// The tab bar's close: an editor with unsaved changes asks first.
    pub fn close(&mut self, tab_id: &str) {
        let Some(tab) = self.tab(tab_id) else {
            return;
        };
        if let Tab::Editor { file_path, .. } = &tab
            && self.files.get(file_path).is_some_and(|f| f.needs_attention())
        {
            self.confirm_close = Some(file_path.clone());
            return;
        }
        self.close_now(&tab);
    }

    fn close_now(&mut self, tab: &Tab) {
        if let Tab::Editor { file_path, .. } = tab {
            self.files.close(file_path);
        }
        self.step(Event::RemoveTab(tab.id().to_owned()));
    }

    fn close_editor(&mut self, path: &str) {
        let id = store::editor_tab_id(path);
        match self.tab(&id) {
            Some(tab) => self.close_now(&tab),
            None => self.files.close(path),
        }
    }

    /// The unsaved-changes dialog's Discard.
    pub fn discard_and_close(&mut self) {
        if let Some(path) = self.confirm_close.take() {
            self.close_editor(&path);
        }
    }

    /// The unsaved-changes dialog's Save: the app writes the draft, then [`Self::saved`].
    pub fn save_and_close(&mut self) {
        let Some(path) = self.confirm_close.take() else {
            return;
        };
        let content = self
            .files
            .get(&path)
            .and_then(|f| f.draft_content.clone())
            .unwrap_or_default();
        self.requests.push(Request::SaveAndClose { path, content });
    }

    /// The draft of `path` reached the disk: the file is clean and its tab closes.
    pub fn saved(&mut self, path: &str) {
        self.close_editor(path);
    }

    pub fn cancel_close(&mut self) {
        self.confirm_close = None;
    }

    pub fn new_tab(&mut self, request: Request) {
        self.menu = None;
        self.requests.push(request);
    }

    /// Commit a split handle's drag.
    pub fn resize(&mut self, split: &str, sizes: Vec<f64>) {
        self.step(Event::UpdatePaneSizes(split.to_owned(), sizes, None));
    }

    pub fn toggle_fullscreen(&mut self, pane: &str) {
        self.step(Event::TogglePaneFullscreen(pane.to_owned()));
    }

    /// Release a dragged tab over `target`: a zone splits or joins the pane, its own tab bar
    /// reorders it. `WorkspaceTabBar` lands only chat drags on another pane's bar, so a tab
    /// dropped there stays put.
    pub fn drop_tab(&mut self, tab: &str, source: &str, target: &DropTarget) {
        match target {
            DropTarget::Zone { pane, position } => {
                if *position == Position::Center {
                    self.step(Event::MoveTabToPane(tab.into(), source.into(), pane.clone()));
                } else {
                    self.step(Event::MoveTabToNewSplit(
                        tab.into(),
                        source.into(),
                        pane.clone(),
                        *position,
                    ));
                }
            }
            DropTarget::Bar {
                pane,
                at: Some((over, side)),
            } if pane == source => self.reorder(source, tab, over, *side),
            DropTarget::Bar { .. } => {}
        }
    }

    /// `handleTabReorder`: move `tab` before or after `over` in `pane`.
    fn reorder(&mut self, pane: &str, tab: &str, over: &str, side: Side) {
        if tab == over {
            return;
        }
        let Some(leaf) = store::find_leaf(&self.store.tree.root, pane) else {
            return;
        };
        let mut order: Vec<String> = leaf.tabs.iter().map(|t| t.id().to_owned()).collect();
        let (Some(from), Some(to)) = (
            order.iter().position(|id| id == tab),
            order.iter().position(|id| id == over),
        ) else {
            return;
        };
        let moved = order.remove(from);
        let mut insert = if from < to { to - 1 } else { to };
        if side == Side::After {
            insert += 1;
        }
        order.insert(insert, moved);
        self.step(Event::ReorderTabsInPane(pane.to_owned(), order));
    }

    /// The leaves on screen: the fullscreen one alone, else every leaf.
    #[must_use]
    pub fn leaf_count(&self) -> usize {
        store::leaves(&self.store.tree.root).len()
    }

    #[must_use]
    pub fn root(&self) -> &PaneNode {
        &self.store.tree.root
    }
}
