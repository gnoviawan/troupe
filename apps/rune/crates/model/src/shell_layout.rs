//! The shell's layout record: rail and side-panel sizes and visibility, the SSH
//! section, the Git History detail width, and per project the side panel's last
//! view, the worktree pick and whether the rail hides the project's chats.
//! Port of `src/renderer/stores/shell-layout-store.ts` over
//! `lib/shell-layout-record.ts`.
//!
//! The record is the device key `shell-layout` in the host store. A hydration
//! gate guards it: nothing is written until the record was read, and a store
//! that could not read it (`unavailable`) keeps working in memory but never
//! writes, so defaults never overwrite a record that only failed to load.
//!
//! Not ported: the one-time carry-over from the legacy sources
//! (`lib/shell-layout-migration.ts` `readLegacyShellLayoutSources`). A device
//! with no record starts from the defaults, which is what the migration builds
//! when no legacy source exists.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::workspace::normalize_path;
use crate::{Effect, Level, js_number};

pub const VERSION: u32 = 3;

/// The persisted device key.
pub const KEY: &str = "shell-layout";

const SOURCE: &str = "shell-layout-store";

/// A size's range and its default.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bounds {
    pub min: f64,
    pub max: f64,
    pub default: f64,
}

pub const RAIL_WIDTH: Bounds = Bounds {
    min: 180.0,
    max: 360.0,
    default: 248.0,
};
pub const SIDE_PANEL_WIDTH: Bounds = Bounds {
    min: 220.0,
    max: 560.0,
    default: 340.0,
};
pub const SSH_SECTION_HEIGHT: Bounds = Bounds {
    min: 48.0,
    max: 400.0,
    default: 140.0,
};
pub const GIT_HISTORY_DETAIL_WIDTH: Bounds = Bounds {
    min: 320.0,
    max: 560.0,
    default: 400.0,
};

/// The rail's width when collapsed to icons (`COLLAPSED_WIDTH`).
pub const COLLAPSED_RAIL_WIDTH: f64 = 56.0;

/// The views the side panel remembers per project. SSH is a mode, never a view.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tab {
    #[default]
    Explorer,
    Changes,
    Github,
}

impl Tab {
    pub const ALL: [Tab; 3] = [Tab::Explorer, Tab::Changes, Tab::Github];

    fn parse(value: &Value) -> Option<Tab> {
        match value.as_str()? {
            "explorer" => Some(Tab::Explorer),
            "changes" => Some(Tab::Changes),
            "github" => Some(Tab::Github),
            _ => None,
        }
    }

    /// The tool strip's label and the panel header's title.
    pub fn label(self) -> &'static str {
        match self {
            Tab::Explorer => "Explorer",
            Tab::Changes => "Changes",
            Tab::Github => "GitHub",
        }
    }
}

/// `ShellLayoutRecord`: what the host stores.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Record {
    pub version: u32,
    pub rail_expanded: bool,
    pub rail_width: f64,
    pub side_panel_visible: bool,
    pub side_panel_width: f64,
    pub ssh_section_visible: bool,
    pub ssh_section_height: f64,
    pub git_history_detail_width: f64,
    pub last_column_tab_by_project: BTreeMap<String, Tab>,
    /// A normalized working-copy path (a key, not a cwd) per project.
    pub worktree_pick_by_project: BTreeMap<String, String>,
    /// Only `true` is stored.
    pub chats_hidden_by_project: BTreeMap<String, bool>,
}

impl Default for Record {
    fn default() -> Self {
        Self {
            version: VERSION,
            rail_expanded: true,
            rail_width: RAIL_WIDTH.default,
            // The main screen opens without the side panel.
            side_panel_visible: false,
            side_panel_width: SIDE_PANEL_WIDTH.default,
            ssh_section_visible: true,
            ssh_section_height: SSH_SECTION_HEIGHT.default,
            git_history_detail_width: GIT_HISTORY_DETAIL_WIDTH.default,
            last_column_tab_by_project: BTreeMap::new(),
            worktree_pick_by_project: BTreeMap::new(),
            chats_hidden_by_project: BTreeMap::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    #[default]
    Idle,
    Hydrating,
    Ready,
    Unavailable,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct State {
    #[serde(flatten)]
    pub record: Record,
    pub status: Status,
    /// Projects removed before the record was read; dropped from it once read.
    #[serde(skip)]
    pending_removed: BTreeSet<String>,
}

impl State {
    /// The view the side panel shows for `project_id`.
    #[must_use]
    pub fn tab(&self, project_id: &str) -> Tab {
        self.record
            .last_column_tab_by_project
            .get(project_id)
            .copied()
            .unwrap_or_default()
    }
}

/// What the host's `store_read` of the record returned.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(untagged)]
pub enum ReadOutcome {
    Found { data: Value },
    Failed { code: String },
}

/// The store's actions by their TypeScript names. Setters take raw JSON
/// values because the store ignores or clamps what is not the right type.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Event {
    /// The record was asked for; mutations wait until it lands.
    StartHydrate,
    /// The read landed: `None` when the device has no record yet.
    Hydrate(Option<ReadOutcome>),
    SetRailExpanded(Value),
    SetRailWidth(Value),
    SetSidePanelVisible(Value),
    SetSidePanelWidth(Value),
    SetSshSectionVisible(Value),
    SetSshSectionHeight(Value),
    SetGitHistoryDetailWidth(Value),
    SetLastColumnTab(String, Value),
    SetWorktreePick(String, Option<Value>),
    SetProjectChatsHidden(String, Value),
    RemoveProject(String),
}

/// `clampToBounds`: rounds and clamps a finite number; anything else is the default.
#[must_use]
pub fn clamp(value: &Value, bounds: Bounds) -> f64 {
    match value.as_f64() {
        Some(v) if v.is_finite() => (v + 0.5).floor().clamp(bounds.min, bounds.max),
        _ => bounds.default,
    }
}

fn safe_key(key: &str) -> bool {
    !key.is_empty() && key != "__proto__"
}

fn sanitize_map<T>(
    value: Option<&Value>,
    accept: impl Fn(&Value) -> Option<T>,
) -> BTreeMap<String, T> {
    let Some(Value::Object(map)) = value else {
        return BTreeMap::new();
    };
    map.iter()
        .filter(|(key, _)| safe_key(key))
        .filter_map(|(key, entry)| Some((key.clone(), accept(entry)?)))
        .collect()
}

/// `sanitizeShellLayout`: a valid record from untrusted data.
#[must_use]
pub fn sanitize(raw: &Value) -> Record {
    let defaults = Record::default();
    let Value::Object(raw) = raw else {
        return defaults;
    };
    let flag =
        |key: &str, fallback: bool| raw.get(key).and_then(Value::as_bool).unwrap_or(fallback);
    let size = |key: &str, bounds: Bounds| clamp(raw.get(key).unwrap_or(&Value::Null), bounds);
    Record {
        version: VERSION,
        rail_expanded: flag("railExpanded", defaults.rail_expanded),
        rail_width: size("railWidth", RAIL_WIDTH),
        side_panel_visible: flag("sidePanelVisible", defaults.side_panel_visible),
        side_panel_width: size("sidePanelWidth", SIDE_PANEL_WIDTH),
        ssh_section_visible: flag("sshSectionVisible", defaults.ssh_section_visible),
        ssh_section_height: size("sshSectionHeight", SSH_SECTION_HEIGHT),
        git_history_detail_width: size("gitHistoryDetailWidth", GIT_HISTORY_DETAIL_WIDTH),
        last_column_tab_by_project: sanitize_map(raw.get("lastColumnTabByProject"), Tab::parse),
        worktree_pick_by_project: sanitize_map(raw.get("worktreePickByProject"), |v| {
            v.as_str().filter(|s| !s.is_empty()).map(str::to_owned)
        }),
        chats_hidden_by_project: sanitize_map(raw.get("chatsHiddenByProject"), |v| {
            (v == &Value::Bool(true)).then_some(true)
        }),
    }
}

/// `upgradeShellLayoutRecord`: a record older than version 3 remembers the
/// retired `chats` view as `explorer`.
fn upgrade(raw: &Value) -> Value {
    let Value::Object(map) = raw else {
        return raw.clone();
    };
    let version = map.get("version").and_then(Value::as_f64).unwrap_or(0.0);
    if version >= 3.0 {
        return raw.clone();
    }
    let Some(Value::Object(tabs)) = map.get("lastColumnTabByProject") else {
        return raw.clone();
    };
    let upgraded = tabs
        .iter()
        .map(|(id, tab)| {
            let tab = if tab == "chats" {
                Value::from("explorer")
            } else {
                tab.clone()
            };
            (id.clone(), tab)
        })
        .collect();
    let mut out = map.clone();
    out.insert("lastColumnTabByProject".into(), Value::Object(upgraded));
    Value::Object(out)
}

/// JavaScript truthiness, for setters that test their argument with `?:`.
fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|n| n != 0.0 && !n.is_nan()),
        Value::String(s) => !s.is_empty(),
        Value::Array(_) | Value::Object(_) => true,
    }
}

fn log(level: Level, source: &str, message: String) -> Effect {
    Effect::Log {
        level,
        source: source.to_owned(),
        message,
    }
}

fn persist(state: &State, effects: &mut Vec<Effect>) {
    if state.status == Status::Ready {
        effects.push(Effect::WriteShellLayout {
            record: state.record.clone(),
            immediate: false,
        });
    }
}

/// `mutate`: applies `update` once the record was read (or could not be), then persists.
fn mutate(state: &mut State, effects: &mut Vec<Effect>, update: impl FnOnce(&mut Record)) {
    if !matches!(state.status, Status::Ready | Status::Unavailable) {
        return;
    }
    update(&mut state.record);
    persist(state, effects);
}

fn drop_project(record: &mut Record, project_id: &str) {
    record.last_column_tab_by_project.remove(project_id);
    record.worktree_pick_by_project.remove(project_id);
    record.chats_hidden_by_project.remove(project_id);
}

/// `flushPendingRemovals`: true when it saved the result.
fn flush_pending(state: &mut State, effects: &mut Vec<Effect>) -> bool {
    if state.pending_removed.is_empty() {
        return false;
    }
    for id in std::mem::take(&mut state.pending_removed) {
        drop_project(&mut state.record, &id);
    }
    persist(state, effects);
    true
}

fn hydrate(state: &mut State, outcome: Option<ReadOutcome>, effects: &mut Vec<Effect>) {
    match outcome {
        Some(ReadOutcome::Found { data }) => {
            let version = data.get("version").and_then(Value::as_f64);
            if let Some(v) = version.filter(|v| *v > f64::from(VERSION)) {
                state.status = Status::Unavailable;
                flush_pending(state, effects);
                effects.push(log(
                    Level::Warn,
                    SOURCE,
                    format!(
                        "[shell] unsupported shell-layout version {}; layout is not saved",
                        js_number(v)
                    ),
                ));
                return;
            }
            state.record = sanitize(&upgrade(&data));
            state.status = Status::Ready;
            let upgraded = version.is_none_or(|v| v < f64::from(VERSION));
            if !flush_pending(state, effects) && upgraded {
                persist(state, effects);
            }
            effects.push(log(
                Level::Info,
                SOURCE,
                "[shell] shell layout hydrated".into(),
            ));
        }
        Some(ReadOutcome::Failed { code }) => {
            state.status = Status::Unavailable;
            flush_pending(state, effects);
            effects.push(log(
                Level::Warn,
                SOURCE,
                format!("[shell] shell layout read failed ({code}); not saved"),
            ));
        }
        None => {
            let mut record = Record::default();
            for id in std::mem::take(&mut state.pending_removed) {
                drop_project(&mut record, &id);
            }
            state.record = record;
            state.status = Status::Ready;
            effects.push(Effect::WriteShellLayout {
                record: state.record.clone(),
                immediate: true,
            });
            effects.push(log(
                Level::Info,
                "shell-layout-migration",
                "[shell-migrate] migrated (flags 0, sizes 0, picks 0)".into(),
            ));
        }
    }
}

/// Applies one event; returns the effects to run in order.
pub fn step(state: &mut State, event: Event) -> Vec<Effect> {
    let mut effects = Vec::new();
    let fx = &mut effects;
    match event {
        Event::StartHydrate => {
            if state.status == Status::Idle {
                state.status = Status::Hydrating;
            }
        }
        Event::Hydrate(outcome) => {
            if matches!(state.status, Status::Idle | Status::Hydrating) {
                hydrate(state, outcome, fx);
            }
        }
        Event::SetRailExpanded(v) => {
            if let Some(v) = v.as_bool() {
                mutate(state, fx, |r| r.rail_expanded = v);
            }
        }
        Event::SetRailWidth(v) => mutate(state, fx, |r| r.rail_width = clamp(&v, RAIL_WIDTH)),
        Event::SetSidePanelVisible(v) => {
            if let Some(v) = v.as_bool() {
                mutate(state, fx, |r| r.side_panel_visible = v);
            }
        }
        Event::SetSidePanelWidth(v) => mutate(state, fx, |r| {
            r.side_panel_width = clamp(&v, SIDE_PANEL_WIDTH)
        }),
        Event::SetSshSectionVisible(v) => {
            if let Some(v) = v.as_bool() {
                mutate(state, fx, |r| r.ssh_section_visible = v);
            }
        }
        Event::SetSshSectionHeight(v) => {
            mutate(state, fx, |r| {
                r.ssh_section_height = clamp(&v, SSH_SECTION_HEIGHT)
            });
        }
        Event::SetGitHistoryDetailWidth(v) => {
            mutate(state, fx, |r| {
                r.git_history_detail_width = clamp(&v, GIT_HISTORY_DETAIL_WIDTH)
            });
        }
        Event::SetLastColumnTab(project_id, tab) => {
            if let Some(tab) = Tab::parse(&tab).filter(|_| safe_key(&project_id)) {
                mutate(state, fx, |r| {
                    r.last_column_tab_by_project.insert(project_id, tab);
                });
            }
        }
        Event::SetWorktreePick(project_id, path) => {
            if !safe_key(&project_id) {
                return effects;
            }
            match path {
                None | Some(Value::Null) => mutate(state, fx, |r| {
                    r.worktree_pick_by_project.remove(&project_id);
                }),
                Some(path) => {
                    let key = path.as_str().map(normalize_path).unwrap_or_default();
                    if !key.is_empty() {
                        mutate(state, fx, |r| {
                            r.worktree_pick_by_project.insert(project_id, key);
                        });
                    }
                }
            }
        }
        Event::SetProjectChatsHidden(project_id, hidden) => {
            if safe_key(&project_id) {
                mutate(state, fx, |r| {
                    if truthy(&hidden) {
                        r.chats_hidden_by_project.insert(project_id, true);
                    } else {
                        r.chats_hidden_by_project.remove(&project_id);
                    }
                });
            }
        }
        Event::RemoveProject(project_id) => {
            if !safe_key(&project_id) {
                return effects;
            }
            let r = &state.record;
            if matches!(state.status, Status::Idle | Status::Hydrating) {
                state.pending_removed.insert(project_id.clone());
            } else if r.last_column_tab_by_project.contains_key(&project_id)
                || r.worktree_pick_by_project.contains_key(&project_id)
                || r.chats_hidden_by_project.contains_key(&project_id)
            {
                mutate(state, fx, |r| drop_project(r, &project_id));
            }
            fx.push(Effect::ForgetProjectChats {
                project_id: project_id.clone(),
            });
            fx.push(Effect::DeleteStoreKey {
                key: format!("workspace/{project_id}"),
            });
        }
    }
    effects
}
