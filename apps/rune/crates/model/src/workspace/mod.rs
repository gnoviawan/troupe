//! The pane workspace: a tree of splits and leaves holding tabs, per project.
//! Port of `src/renderer/stores/workspace-store/`.
//!
//! The visible project's tree is [`State::tree`]; every other visited
//! project's tree is parked in [`State::parked_trees`]. [`Event`] mirrors the
//! store's actions one to one (same names, same argument order). A recorded
//! `{action, args}` is not its serde form: `tests/vectors.rs` (`call`)
//! rewrites it as `{action: args}`, `{action: arg}` or `"action"` first.

pub(crate) mod ids;
mod pane;
mod sync;
mod tabs;
pub(crate) mod tree;

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{Effect, Ids};

pub use ids::{commit_diff_tab_id, diff_tab_id, normalize_path};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    Horizontal,
    Vertical,
}

/// Where a dragged tab lands relative to the target pane.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Position {
    Left,
    Right,
    Top,
    Bottom,
    Center,
}

impl Position {
    fn is_leading(self) -> bool {
        matches!(self, Self::Left | Self::Top)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase"
)]
pub enum Tab {
    Terminal {
        id: String,
        terminal_id: String,
    },
    Editor {
        id: String,
        file_path: String,
    },
    Browser {
        id: String,
        browser_tab_id: String,
    },
    Git {
        id: String,
        cwd: String,
    },
    AgentChat {
        id: String,
        session_id: String,
        /// Stable mount identity that survives a session id swap.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        mount_key: Option<String>,
    },
    GitHistory {
        id: String,
        cwd: String,
    },
    GithubItem {
        id: String,
        cwd: String,
        repo: String,
        kind: String,
        number: u64,
        title: String,
    },
    Diff {
        id: String,
        cwd: String,
        path: String,
        staged: bool,
    },
    CommitDiff {
        id: String,
        cwd: String,
        commit: String,
        path: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        old_path: Option<String>,
    },
    Canvas {
        id: String,
        project_id: String,
        doc_path: String,
    },
    Automations {
        id: String,
        project_id: String,
    },
}

impl Tab {
    #[must_use]
    pub fn id(&self) -> &str {
        match self {
            Self::Terminal { id, .. }
            | Self::Editor { id, .. }
            | Self::Browser { id, .. }
            | Self::Git { id, .. }
            | Self::AgentChat { id, .. }
            | Self::GitHistory { id, .. }
            | Self::GithubItem { id, .. }
            | Self::Diff { id, .. }
            | Self::CommitDiff { id, .. }
            | Self::Canvas { id, .. }
            | Self::Automations { id, .. } => id,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum PaneNode {
    Split(Split),
    Leaf(Leaf),
}

impl PaneNode {
    #[must_use]
    pub fn id(&self) -> &str {
        match self {
            Self::Split(split) => &split.id,
            Self::Leaf(leaf) => &leaf.id,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Split {
    pub id: String,
    pub direction: Direction,
    pub children: Vec<PaneNode>,
    /// Shares of the children, normally summing to 100.
    pub sizes: Vec<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Leaf {
    pub id: String,
    pub tabs: Vec<Tab>,
    pub active_tab_id: Option<String>,
}

/// One project's whole pane workspace.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectWorkspace {
    pub root: PaneNode,
    pub active_pane_id: String,
    pub fullscreen_pane_id: Option<String>,
    /// Pane showing the agent launcher overlay.
    pub agent_launcher_pane_id: Option<String>,
    /// The dedicated agent-browser pane; never persisted.
    pub agent_browser_pane_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct State {
    /// The project whose tree is [`State::tree`] (`""` before the first switch).
    pub active_project_id: String,
    pub parked_trees: BTreeMap<String, ProjectWorkspace>,
    #[serde(flatten)]
    pub tree: ProjectWorkspace,
}

impl State {
    /// A fresh single-leaf workspace with no project.
    pub fn new(ids: &mut dyn Ids) -> Self {
        Self {
            active_project_id: String::new(),
            parked_trees: BTreeMap::new(),
            tree: tree::create_project_workspace(ids),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub struct SplitOptions {
    /// `[target share, new leaf share]`; malformed values fall back to 50/50.
    pub sizes: Option<Vec<f64>>,
    /// Focus the new leaf (default true).
    pub focus: Option<bool>,
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub struct AddTabOptions {
    pub focus: Option<bool>,
}

/// A GitHub issue or PR tab's identity and its label before the detail loads.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct GithubItem {
    pub cwd: String,
    pub repo: String,
    pub kind: String,
    pub number: u64,
    pub title: String,
}

/// The store's actions, in its argument order. A recorded `{action, args}`
/// is this enum externally tagged by `action`.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Event {
    ActivateProject(String, #[serde(default)] Option<ProjectWorkspace>),
    DropProjectTree(String),
    ShowAgentLauncher(String),
    HideAgentLauncher,
    SplitPane(
        String,
        Direction,
        Tab,
        #[serde(default)] Option<Position>,
        #[serde(default)] Option<SplitOptions>,
    ),
    AddTabToPane(String, Tab, #[serde(default)] Option<AddTabOptions>),
    AddTabToParkedPane(String, String, Tab),
    MoveTabToPane(String, String, String),
    MoveTabToNewSplit(String, String, String, Position),
    CloseTab(String, String),
    SetActiveTab(String, String),
    SetActivePane(String),
    TogglePaneFullscreen(String),
    ClearFullscreenPane,
    UpdatePaneSizes(String, Vec<f64>, #[serde(default)] Option<String>),
    CollapsePane(String),
    ReorderTabsInPane(String, Vec<String>),
    /// The visible terminal ids, then every id the terminal store holds
    /// (a tab of a held terminal that has no PTY yet is kept).
    SyncTerminalTabs(Vec<String>, Vec<String>),
    ClearEditorTabs,
    ClearPane(String),
    ResetLayout,
    LoadProjectWorkspace(PaneNode, #[serde(default)] Option<String>),
    SyncEditorTabs(Vec<String>, #[serde(default)] Option<String>),
    RemapTerminalTabs(BTreeMap<String, String>),
    AddTerminalTab(String, #[serde(default)] Option<String>),
    EnsureTerminalTab(
        String,
        #[serde(default)] Option<String>,
        #[serde(default)] Option<bool>,
    ),
    AddEditorTab(String, #[serde(default)] Option<String>),
    AddBrowserTab(String, #[serde(default)] Option<String>),
    OpenAgentBrowserTab(String, #[serde(default)] Option<String>),
    AddAgentChatTab(String, #[serde(default)] Option<String>),
    InsertAgentChatTab(String),
    AddGitTab(String, #[serde(default)] Option<String>),
    AddGitHistoryTab(String, #[serde(default)] Option<String>),
    AddGithubItemTab(GithubItem, #[serde(default)] Option<String>),
    AddDiffTab(String, String, bool, #[serde(default)] Option<String>),
    AddCommitDiffTab(
        String,
        String,
        String,
        #[serde(default)] Option<String>,
        #[serde(default)] Option<String>,
    ),
    AddCanvasTab(String, String, #[serde(default)] Option<String>),
    AddAutomationsTab(String, #[serde(default)] Option<String>),
    RemapAgentChatSession(String, String, #[serde(default)] Option<String>),
    RemoveTab(String),
    RenameEditorTab(String, String),
}

/// The store's reads.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Query {
    HasProjectTree(String),
    GetProjectTree(String),
    GetActiveTab,
    GetActivePaneLeaf,
    /// `1` for the next tab, `-1` for the previous one.
    GetNextTabId(i8),
}

/// What an action or a read returns, when it returns anything.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(untagged)]
pub enum Reply {
    Bool(bool),
    Tab(Option<Tab>),
    Leaf(Option<PaneNode>),
    Tree(Option<ProjectWorkspace>),
    TabId(Option<String>),
}

/// What one [`step`] produced.
#[derive(Debug, Default, PartialEq)]
pub struct Out {
    pub effects: Vec<Effect>,
    pub reply: Option<Reply>,
}

/// Apply one action.
pub fn step(state: &mut State, event: Event, ids: &mut dyn Ids) -> Out {
    let mut out = Out::default();
    let fx = &mut out.effects;
    match event {
        Event::ActivateProject(project_id, tree) => {
            pane::activate_project(state, &project_id, tree, ids);
        }
        Event::DropProjectTree(project_id) => {
            state.parked_trees.remove(&project_id);
        }
        Event::ShowAgentLauncher(pane_id) => state.tree.agent_launcher_pane_id = Some(pane_id),
        Event::HideAgentLauncher => state.tree.agent_launcher_pane_id = None,
        Event::SplitPane(pane_id, direction, tab, position, options) => {
            let position = position.unwrap_or(Position::Right);
            pane::split_pane(
                state,
                &pane_id,
                direction,
                tab,
                position,
                &options.unwrap_or_default(),
                ids,
                fx,
            );
        }
        Event::AddTabToPane(pane_id, tab, options) => {
            let focus = options.and_then(|o| o.focus) != Some(false);
            pane::add_tab_to_pane(state, &pane_id, tab, focus);
        }
        Event::AddTabToParkedPane(project_id, pane_id, tab) => {
            let added = pane::add_tab_to_parked_pane(state, &project_id, &pane_id, tab);
            out.reply = Some(Reply::Bool(added));
        }
        Event::MoveTabToPane(tab_id, source, target) => {
            pane::move_tab_to_pane(state, &tab_id, &source, &target, ids);
        }
        Event::MoveTabToNewSplit(tab_id, source, target, position) => {
            pane::move_tab_to_new_split(state, &tab_id, &source, &target, position, ids);
        }
        Event::CloseTab(pane_id, tab_id) => {
            let removed = pane::close_tab(state, &pane_id, &tab_id, ids);
            out.reply = Some(Reply::Tab(removed));
        }
        Event::SetActiveTab(pane_id, tab_id) => pane::set_active_tab(state, &pane_id, &tab_id, fx),
        Event::SetActivePane(pane_id) => {
            state.tree.active_pane_id =
                tree::resolve_active_pane_id(state.tree.fullscreen_pane_id.as_deref(), &pane_id);
        }
        Event::TogglePaneFullscreen(pane_id) => pane::toggle_fullscreen(state, &pane_id),
        Event::ClearFullscreenPane => state.tree.fullscreen_pane_id = None,
        Event::UpdatePaneSizes(split_id, sizes, project_id) => {
            pane::update_pane_sizes(state, &split_id, sizes, project_id.as_deref());
        }
        Event::CollapsePane(pane_id) => pane::collapse_pane(state, &pane_id, ids),
        Event::ReorderTabsInPane(pane_id, order) => pane::reorder_tabs(state, &pane_id, &order),
        Event::SyncTerminalTabs(terminal_ids, held) => {
            sync::sync_terminal_tabs(state, &terminal_ids, &held, ids);
        }
        Event::ClearEditorTabs => sync::clear_editor_tabs(state),
        Event::ClearPane(pane_id) => sync::clear_pane(state, &pane_id),
        Event::ResetLayout => sync::reset_layout(state, ids),
        Event::LoadProjectWorkspace(root, active) => {
            sync::load_project_workspace(state, root, active.as_deref(), ids);
        }
        Event::SyncEditorTabs(paths, active) => {
            sync::sync_editor_tabs(state, &paths, active.as_deref(), ids);
        }
        Event::RemapTerminalTabs(map) => sync::remap_terminal_tabs(state, &map, ids),
        Event::AddTerminalTab(terminal_id, target) => {
            tabs::add_terminal_tab(state, &terminal_id, target.as_deref());
        }
        Event::EnsureTerminalTab(terminal_id, target, make_active) => {
            tabs::ensure_terminal_tab(
                state,
                &terminal_id,
                target.as_deref(),
                make_active == Some(true),
            );
        }
        Event::AddEditorTab(path, target) => tabs::add_editor_tab(state, &path, target.as_deref()),
        Event::AddBrowserTab(browser_tab_id, target) => {
            tabs::add_browser_tab(state, &browser_tab_id, target.as_deref());
        }
        Event::OpenAgentBrowserTab(browser_tab_id, _url) => {
            tabs::open_agent_browser_tab(state, &browser_tab_id, ids, fx);
        }
        Event::AddAgentChatTab(session_id, target) => {
            tabs::add_agent_chat_tab(state, &session_id, target.as_deref(), fx);
        }
        Event::InsertAgentChatTab(session_id) => tabs::insert_agent_chat_tab(state, &session_id),
        Event::AddGitTab(cwd, target) => tabs::add_git_tab(state, &cwd, target.as_deref(), false),
        Event::AddGitHistoryTab(cwd, target) => {
            tabs::add_git_tab(state, &cwd, target.as_deref(), true)
        }
        Event::AddGithubItemTab(item, target) => {
            tabs::add_github_item_tab(state, item, target.as_deref())
        }
        Event::AddDiffTab(cwd, path, staged, target) => {
            tabs::add_diff_tab(state, &cwd, &path, staged, target.as_deref());
        }
        Event::AddCommitDiffTab(cwd, hash, path, old_path, target) => {
            tabs::add_commit_diff_tab(
                state,
                &cwd,
                &hash,
                &path,
                old_path.as_deref(),
                target.as_deref(),
            );
        }
        Event::AddCanvasTab(project_id, doc_path, target) => {
            tabs::add_canvas_tab(state, &project_id, &doc_path, target.as_deref());
        }
        Event::AddAutomationsTab(project_id, target) => {
            tabs::add_automations_tab(state, &project_id, target.as_deref(), fx);
        }
        Event::RemapAgentChatSession(from, to, target) => {
            tabs::remap_agent_chat_session(state, &from, &to, target.as_deref(), fx);
        }
        Event::RemoveTab(tab_id) => tabs::remove_tab(state, &tab_id, ids),
        Event::RenameEditorTab(from, to) => tabs::rename_editor_tab(state, &from, &to, ids),
    }
    out
}

/// Answer one read; the state does not change.
#[must_use]
pub fn query(state: &State, query: Query) -> Reply {
    match query {
        Query::HasProjectTree(project_id) => Reply::Bool(
            state.active_project_id == project_id || state.parked_trees.contains_key(&project_id),
        ),
        Query::GetProjectTree(project_id) => {
            Reply::Tree(if state.active_project_id == project_id {
                Some(state.tree.clone())
            } else {
                state.parked_trees.get(&project_id).cloned()
            })
        }
        Query::GetActiveTab => Reply::Tab(active_leaf(state).and_then(|leaf| {
            leaf.tabs
                .iter()
                .find(|tab| Some(tab.id()) == leaf.active_tab_id.as_deref())
                .cloned()
        })),
        Query::GetActivePaneLeaf => Reply::Leaf(active_leaf(state).cloned().map(PaneNode::Leaf)),
        Query::GetNextTabId(direction) => Reply::TabId(next_tab_id(state, direction)),
    }
}

fn active_leaf(state: &State) -> Option<&Leaf> {
    tree::find_leaf(&state.tree.root, &state.tree.active_pane_id)
}

fn next_tab_id(state: &State, direction: i8) -> Option<String> {
    let leaf = active_leaf(state).filter(|leaf| !leaf.tabs.is_empty())?;
    let Some(active) = leaf.active_tab_id.as_deref() else {
        return Some(leaf.tabs[0].id().to_owned());
    };
    let Some(current) = leaf.tabs.iter().position(|tab| tab.id() == active) else {
        return Some(leaf.tabs[0].id().to_owned());
    };
    let len = leaf.tabs.len() as i64;
    let next = (current as i64 + i64::from(direction)).rem_euclid(len);
    Some(leaf.tabs[next as usize].id().to_owned())
}
