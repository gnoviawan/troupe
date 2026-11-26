//! The pane tree as the workspace record stores it. Port of
//! `hooks/editor-persistence/pane-tree.ts`: [`serialize`] writes the tree,
//! [`deserialize`] reads it back tolerating corrupt or legacy entries, and
//! [`reconcile_terminal_tabs`] keeps the tabs the device can still show.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::workspace::{Direction, Leaf, PaneNode, Split, Tab, ids, tree};
use crate::{Effect, Ids, Level};

const DESERIALIZE: &str = "useEditorPersistence.deserializePaneTree";

/// One tab as the record keeps it: enough to rebuild the tab, never its body.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase"
)]
pub enum TabRef {
    Editor {
        file_path: String,
    },
    Terminal {
        terminal_id: String,
    },
    Browser {
        browser_tab_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        url: Option<String>,
    },
    Git {
        id: String,
        cwd: String,
    },
    AgentChat {
        id: String,
        session_id: String,
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
    Automations {
        id: String,
        project_id: String,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum PersistedNode {
    Split(PersistedSplit),
    Leaf(PersistedLeaf),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PersistedSplit {
    pub id: String,
    pub direction: Direction,
    pub children: Vec<PersistedNode>,
    pub sizes: Vec<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersistedLeaf {
    pub id: String,
    pub tabs: Vec<TabRef>,
    pub active_tab_id: Option<String>,
}

/// A live terminal of the project, or one the terminal layout remembers.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalInfo {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub shell: String,
    #[serde(default)]
    pub cwd: Option<String>,
}

/// The record's view of a tab; a canvas tab is never stored.
fn tab_ref(tab: &Tab) -> Option<TabRef> {
    Some(match tab.clone() {
        Tab::Editor { file_path, .. } => TabRef::Editor { file_path },
        Tab::Terminal { terminal_id, .. } => TabRef::Terminal { terminal_id },
        // The url lives in the browser session store, which Rune does not have yet.
        Tab::Browser { browser_tab_id, .. } => TabRef::Browser {
            browser_tab_id,
            url: None,
        },
        Tab::Git { id, cwd } => TabRef::Git { id, cwd },
        Tab::AgentChat { id, session_id, .. } => TabRef::AgentChat { id, session_id },
        Tab::GitHistory { id, cwd } => TabRef::GitHistory { id, cwd },
        Tab::GithubItem {
            id,
            cwd,
            repo,
            kind,
            number,
            title,
        } => TabRef::GithubItem {
            id,
            cwd,
            repo,
            kind,
            number,
            title,
        },
        Tab::Diff {
            id,
            cwd,
            path,
            staged,
        } => TabRef::Diff {
            id,
            cwd,
            path,
            staged,
        },
        Tab::CommitDiff {
            id,
            cwd,
            commit,
            path,
            old_path,
        } => TabRef::CommitDiff {
            id,
            cwd,
            commit,
            path,
            old_path,
        },
        Tab::Automations { id, project_id } => TabRef::Automations { id, project_id },
        Tab::Canvas { .. } => return None,
    })
}

#[must_use]
pub fn serialize(node: &PaneNode) -> PersistedNode {
    match node {
        PaneNode::Leaf(leaf) => PersistedNode::Leaf(PersistedLeaf {
            id: leaf.id.clone(),
            tabs: leaf.tabs.iter().filter_map(tab_ref).collect(),
            active_tab_id: leaf.active_tab_id.clone(),
        }),
        PaneNode::Split(split) => PersistedNode::Split(PersistedSplit {
            id: split.id.clone(),
            direction: split.direction,
            children: split.children.iter().map(serialize).collect(),
            sizes: split.sizes.clone(),
        }),
    }
}

/// `describeKind` in `lib/workspace-tab-kinds.ts`: the kind as safe log text.
fn describe_kind(kind: Option<&Value>) -> String {
    const MAX: usize = 40;
    match kind {
        None => "undefined".to_owned(),
        Some(Value::Null) => "null".to_owned(),
        Some(Value::String(text)) => {
            let clean: String = text
                .chars()
                .map(|c| if (c as u32) < 0x20 || c == '\u{7f}' { '?' } else { c })
                .collect();
            let units: Vec<u16> = clean.encode_utf16().collect();
            if units.len() > MAX {
                format!("{}…", String::from_utf16_lossy(&units[..MAX]))
            } else {
                clean
            }
        }
        Some(Value::Bool(_)) => "<boolean>".to_owned(),
        Some(Value::Number(_)) => "<number>".to_owned(),
        Some(Value::Array(_) | Value::Object(_)) => "<object>".to_owned(),
    }
}

fn unknown_kind(tab: &Value, source: &str, action: &str, fx: &mut Vec<Effect>) {
    fx.push(Effect::log(
        Level::Warn,
        source,
        format!(
            "Unrecognised workspace tab kind \"{}\" — {action}",
            describe_kind(tab.get("type"))
        ),
    ));
}

fn warn(message: &str, fx: &mut Vec<Effect>) {
    fx.push(Effect::log(Level::Warn, DESERIALIZE, message.to_owned()));
}

fn text<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}

fn owned(value: &Value, key: &str) -> Option<String> {
    text(value, key).map(str::to_owned)
}

/// `tab-<uuid>`: the only browser labels the host registry builds tabs under.
fn is_user_browser_tab_id(id: &str) -> bool {
    let Some(rest) = id.strip_prefix("tab-") else {
        return false;
    };
    let groups: Vec<&str> = rest.split('-').collect();
    groups.len() == 5
        && groups.iter().zip([8, 4, 4, 4, 12]).all(|(group, len)| {
            group.len() == len
                && group
                    .chars()
                    .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
        })
}

fn restore_tab(tab: &Value, ids_source: &mut dyn Ids, fx: &mut Vec<Effect>) -> Option<Tab> {
    match text(tab, "type") {
        Some("editor") => {
            let file_path = owned(tab, "filePath")?;
            Some(Tab::Editor {
                id: ids::editor(&file_path),
                file_path,
            })
        }
        Some("browser") => {
            let saved = owned(tab, "browserTabId")?;
            let browser_tab_id = if is_user_browser_tab_id(&saved) {
                saved
            } else {
                format!("tab-{}", ids_source.next_id().to_lowercase())
            };
            Some(Tab::Browser {
                id: ids::browser(&browser_tab_id),
                browser_tab_id,
            })
        }
        Some("git") => Some(Tab::Git {
            id: owned(tab, "id")?,
            cwd: owned(tab, "cwd")?,
        }),
        Some("agent-chat") => match text(tab, "sessionId") {
            Some(session) if !session.starts_with("launch-") => Some(Tab::AgentChat {
                id: owned(tab, "id")?,
                session_id: session.to_owned(),
                mount_key: None,
            }),
            Some(session) => {
                warn(
                    &format!(
                        "Dropped failed-launch placeholder chat tab (session {session}) during workspace restore"
                    ),
                    fx,
                );
                None
            }
            None => {
                warn(
                    "Dropped agent-chat tab with a missing/invalid sessionId during workspace restore",
                    fx,
                );
                None
            }
        },
        Some("git-history") => Some(Tab::GitHistory {
            id: owned(tab, "id")?,
            cwd: owned(tab, "cwd")?,
        }),
        Some("automations") => {
            let Some(project_id) = owned(tab, "projectId") else {
                warn(
                    "Dropped automations tab with a missing projectId during workspace restore",
                    fx,
                );
                return None;
            };
            Some(Tab::Automations {
                id: ids::automations(&project_id),
                project_id,
            })
        }
        Some("diff") => {
            let (Some(cwd), Some(path), Some(staged)) = (
                owned(tab, "cwd"),
                owned(tab, "path"),
                tab.get("staged").and_then(Value::as_bool),
            ) else {
                warn(
                    "Dropped malformed diff tab (cwd/path/staged invalid) during workspace restore",
                    fx,
                );
                return None;
            };
            Some(Tab::Diff {
                id: owned(tab, "id").unwrap_or_else(|| ids::diff_tab_id(&cwd, &path, staged)),
                cwd,
                path,
                staged,
            })
        }
        Some("commit-diff") => {
            let (Some(cwd), Some(commit), Some(path)) =
                (owned(tab, "cwd"), owned(tab, "commit"), owned(tab, "path"))
            else {
                warn(
                    "Dropped malformed commit diff tab (cwd/commit/path invalid) during workspace restore",
                    fx,
                );
                return None;
            };
            Some(Tab::CommitDiff {
                id: owned(tab, "id")
                    .unwrap_or_else(|| ids::commit_diff_tab_id(&cwd, &commit, &path)),
                cwd,
                commit,
                path,
                old_path: owned(tab, "oldPath"),
            })
        }
        Some("github-item") => restore_github_item(tab, fx),
        Some("terminal") => {
            let terminal_id = owned(tab, "terminalId")?;
            Some(Tab::Terminal {
                id: ids::terminal(&terminal_id),
                terminal_id,
            })
        }
        _ => {
            unknown_kind(tab, DESERIALIZE, "dropped", fx);
            None
        }
    }
}

fn restore_github_item(tab: &Value, fx: &mut Vec<Effect>) -> Option<Tab> {
    const MAX_SAFE: f64 = 9_007_199_254_740_991.0;
    let cwd = owned(tab, "cwd");
    let repo = text(tab, "repo").filter(|repo| matches!(*repo, "origin" | "upstream"));
    let kind = text(tab, "kind").filter(|kind| matches!(*kind, "issue" | "pr"));
    let number = tab
        .get("number")
        .and_then(Value::as_f64)
        .filter(|n| n.fract() == 0.0 && *n > 0.0 && *n <= MAX_SAFE);
    let (Some(cwd), Some(repo), Some(kind), Some(number)) = (cwd, repo, kind, number) else {
        warn(
            "Dropped malformed GitHub item tab (cwd/repo/kind/number invalid) during workspace restore",
            fx,
        );
        return None;
    };
    let number = number as u64;
    Some(Tab::GithubItem {
        id: ids::github_item(&cwd, repo, number),
        cwd,
        repo: repo.to_owned(),
        kind: kind.to_owned(),
        number,
        title: owned(tab, "title").unwrap_or_default(),
    })
}

/// A stored tree as the workspace's tree. Corrupt tabs are dropped with one warn each; a
/// legacy leaf (`editorFilePaths`) becomes editor tabs.
pub fn deserialize(persisted: &Value, ids_source: &mut dyn Ids, fx: &mut Vec<Effect>) -> PaneNode {
    let id = owned(persisted, "id").unwrap_or_default();
    if text(persisted, "type") != Some("split") {
        let mut tabs: Vec<Tab> = persisted
            .get("tabs")
            .and_then(Value::as_array)
            .map(|tabs| {
                tabs.iter()
                    .filter_map(|tab| restore_tab(tab, ids_source, fx))
                    .collect()
            })
            .unwrap_or_default();
        if tabs.is_empty()
            && let Some(paths) = persisted.get("editorFilePaths").and_then(Value::as_array)
        {
            tabs.extend(paths.iter().filter_map(Value::as_str).map(|path| Tab::Editor {
                id: ids::editor(path),
                file_path: path.to_owned(),
            }));
        }
        let active_tab_id = match text(persisted, "activeTabId") {
            Some(active) if !active.is_empty() && tabs.iter().any(|tab| tab.id() == active) => {
                Some(active.to_owned())
            }
            _ => tabs.first().map(|tab| tab.id().to_owned()),
        };
        return PaneNode::Leaf(Leaf {
            id,
            tabs,
            active_tab_id,
        });
    }
    let direction = match text(persisted, "direction") {
        Some("vertical") => Direction::Vertical,
        _ => Direction::Horizontal,
    };
    PaneNode::Split(Split {
        id,
        direction,
        children: persisted
            .get("children")
            .and_then(Value::as_array)
            .map(|children| {
                children
                    .iter()
                    .map(|child| deserialize(child, ids_source, fx))
                    .collect()
            })
            .unwrap_or_default(),
        sizes: persisted
            .get("sizes")
            .and_then(Value::as_array)
            .map(|sizes| sizes.iter().map(|s| s.as_f64().unwrap_or(f64::NAN)).collect())
            .unwrap_or_default(),
    })
}

/// Hands persisted terminal ids to live ones: the same id, else the first unused live
/// terminal with the remembered name, shell and cwd, then name and shell, then name.
struct TerminalMatcher<'a> {
    live: &'a [TerminalInfo],
    remembered: &'a [TerminalInfo],
    unused: Vec<&'a TerminalInfo>,
}

impl<'a> TerminalMatcher<'a> {
    fn new(live: &'a [TerminalInfo], remembered: &'a [TerminalInfo]) -> Self {
        Self {
            live,
            remembered,
            unused: live.iter().collect(),
        }
    }

    fn take(&mut self, matches: impl Fn(&TerminalInfo) -> bool) -> Option<String> {
        let index = self.unused.iter().position(|t| matches(t))?;
        Some(self.unused.remove(index).id.clone())
    }

    fn match_id(&mut self, persisted_id: &str) -> Option<String> {
        if let Some(direct) = self.live.iter().find(|t| t.id == persisted_id) {
            if let Some(index) = self.unused.iter().position(|t| t.id == persisted_id) {
                self.unused.remove(index);
            }
            return Some(direct.id.clone());
        }
        let wanted = self.remembered.iter().find(|t| t.id == persisted_id)?;
        self.take(|t| t.name == wanted.name && t.shell == wanted.shell && t.cwd == wanted.cwd)
            .or_else(|| self.take(|t| t.name == wanted.name && t.shell == wanted.shell))
            .or_else(|| self.take(|t| t.name == wanted.name))
    }
}

/// Keep the tabs this device can still show: editors whose file is open, terminals mapped to
/// a live terminal (or every terminal tab when none is live but the layout remembers some),
/// never a canvas. The result is renormalized.
pub fn reconcile_terminal_tabs(
    root: &PaneNode,
    open_file_paths: &BTreeSet<String>,
    live: &[TerminalInfo],
    layout: Option<&[TerminalInfo]>,
    ids_source: &mut dyn Ids,
) -> PaneNode {
    let remembered = layout.unwrap_or_default();
    let keep_persisted = live.is_empty() && !remembered.is_empty();
    let mut matcher = TerminalMatcher::new(live, remembered);
    let visited = visit(root, open_file_paths, keep_persisted, &mut matcher);
    sanitize(&visited).unwrap_or_else(|| tree::create_leaf(ids_source, Vec::new(), None))
}

fn visit(
    node: &PaneNode,
    open_file_paths: &BTreeSet<String>,
    keep_persisted: bool,
    matcher: &mut TerminalMatcher<'_>,
) -> PaneNode {
    let leaf = match node {
        PaneNode::Leaf(leaf) => leaf,
        PaneNode::Split(split) => {
            return PaneNode::Split(Split {
                children: split
                    .children
                    .iter()
                    .map(|child| visit(child, open_file_paths, keep_persisted, matcher))
                    .collect(),
                ..split.clone()
            });
        }
    };
    let mut remapped: Vec<(String, String)> = Vec::new();
    let mut tabs = Vec::new();
    for tab in &leaf.tabs {
        match tab {
            Tab::Editor { file_path, .. } => {
                if open_file_paths.contains(file_path) {
                    tabs.push(tab.clone());
                }
            }
            Tab::Canvas { .. } => {}
            Tab::Terminal { id, terminal_id } => {
                if keep_persisted {
                    tabs.push(tab.clone());
                } else if let Some(mapped) = matcher.match_id(terminal_id) {
                    let mapped_tab_id = ids::terminal(&mapped);
                    remapped.push((id.clone(), mapped_tab_id.clone()));
                    tabs.push(Tab::Terminal {
                        id: mapped_tab_id,
                        terminal_id: mapped,
                    });
                }
            }
            _ => tabs.push(tab.clone()),
        }
    }
    let mut active_tab_id = leaf.active_tab_id.clone();
    if let Some(active) = active_tab_id.as_deref().filter(|id| !id.is_empty()) {
        if let Some((_, mapped)) = remapped.iter().find(|(from, _)| from == active) {
            active_tab_id = Some(mapped.clone());
        }
        let current = active_tab_id.as_deref().unwrap_or_default();
        if !tabs.iter().any(|tab: &Tab| tab.id() == current) {
            active_tab_id = tabs.first().map(|tab| tab.id().to_owned());
        }
    }
    PaneNode::Leaf(Leaf {
        id: leaf.id.clone(),
        tabs,
        active_tab_id,
    })
}

/// `sanitizePaneNode`: drop nothing but renormalize each split's sizes to sum to 100, an
/// invalid share counting as 1. Only a split with no children vanishes.
fn sanitize(node: &PaneNode) -> Option<PaneNode> {
    let PaneNode::Split(split) = node else {
        return Some(node.clone());
    };
    let surviving: Vec<(PaneNode, usize)> = split
        .children
        .iter()
        .enumerate()
        .filter_map(|(index, child)| sanitize(child).map(|child| (child, index)))
        .collect();
    match surviving.len() {
        0 => None,
        1 => surviving.into_iter().next().map(|(child, _)| child),
        _ => {
            let valid: Vec<f64> = surviving
                .iter()
                .map(|(_, index)| match split.sizes.get(*index) {
                    Some(size) if size.is_finite() && *size > 0.0 => *size,
                    _ => 1.0,
                })
                .collect();
            let total = valid.iter().fold(0.0, |sum, value| sum + value);
            Some(PaneNode::Split(Split {
                children: surviving.into_iter().map(|(child, _)| child).collect(),
                sizes: valid.iter().map(|value| (value / total) * 100.0).collect(),
                ..split.clone()
            }))
        }
    }
}
