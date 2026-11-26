//! Terminal and editor sync, reset and remap. Port of
//! `workspace-store/slices/sync.ts`.

use std::collections::BTreeMap;

use super::tree::{
    create_leaf, find_leaf, last_tab_id, leaves, normalize_pane_tree, resolve_fullscreen_pane_id,
    resolve_leaf_id, update_leaf,
};
use super::{Leaf, PaneNode, Split, State, Tab, ids};
use crate::Ids;

/// Keep the leaf's active tab if it survived, else the last tab.
fn keep_or_last(active: Option<&String>, tabs: &[Tab]) -> Option<String> {
    match active {
        Some(id) if !tabs.iter().any(|tab| tab.id() == id) => last_tab_id(tabs),
        other => other.cloned(),
    }
}

fn set_normalized(state: &mut State, root: &PaneNode, ids: &mut dyn Ids) {
    let root = normalize_pane_tree(root, ids);
    state.tree.fullscreen_pane_id =
        resolve_fullscreen_pane_id(&root, state.tree.fullscreen_pane_id.as_deref());
    state.tree.root = root;
}

/// Drop tabs of terminals that are gone and add tabs for new ones to the
/// active pane. A tab whose terminal the terminal store still `held` is kept
/// even when it is not listed: its PTY is still starting.
pub fn sync_terminal_tabs(
    state: &mut State,
    terminal_ids: &[String],
    held: &[String],
    ids: &mut dyn Ids,
) {
    let listed: Vec<String> = terminal_ids.iter().map(|id| ids::terminal(id)).collect();
    let keeps = |tab: &Tab| match tab {
        Tab::Terminal { id, terminal_id } => {
            listed.contains(id) || (!terminal_id.is_empty() && held.contains(terminal_id))
        }
        _ => true,
    };
    let mut root = state.tree.root.clone();
    let mut changed = false;
    let leaf_ids: Vec<String> = leaves(&state.tree.root)
        .iter()
        .map(|leaf| leaf.id.clone())
        .collect();
    for leaf_id in leaf_ids {
        let orphaned =
            find_leaf(&root, &leaf_id).is_some_and(|leaf| leaf.tabs.iter().any(|tab| !keeps(tab)));
        if !orphaned {
            continue;
        }
        changed = true;
        root = update_leaf(&root, &leaf_id, &mut |leaf| {
            let tabs: Vec<Tab> = leaf.tabs.iter().filter(|tab| keeps(tab)).cloned().collect();
            Leaf {
                id: leaf.id.clone(),
                active_tab_id: keep_or_last(leaf.active_tab_id.as_ref(), &tabs),
                tabs,
            }
        });
    }
    // Built once, before the loop: a repeated id adds a second tab, as in TypeScript.
    let present: Vec<String> = leaves(&root)
        .iter()
        .flat_map(|leaf| leaf.tabs.iter())
        .filter(|tab| matches!(tab, Tab::Terminal { .. }))
        .map(|tab| tab.id().to_owned())
        .collect();
    let active_pane = state.tree.active_pane_id.clone();
    for terminal_id in terminal_ids {
        let id = ids::terminal(terminal_id);
        if present.contains(&id) {
            continue;
        }
        changed = true;
        root = update_leaf(&root, &active_pane, &mut |leaf| Leaf {
            id: leaf.id.clone(),
            tabs: leaf
                .tabs
                .iter()
                .cloned()
                .chain([Tab::Terminal {
                    id: id.clone(),
                    terminal_id: terminal_id.clone(),
                }])
                .collect(),
            active_tab_id: Some(id.clone()),
        });
    }
    if changed {
        set_normalized(state, &root, ids);
    }
}

pub fn clear_editor_tabs(state: &mut State) {
    let leaf_ids: Vec<String> = leaves(&state.tree.root)
        .iter()
        .map(|leaf| leaf.id.clone())
        .collect();
    let mut root = state.tree.root.clone();
    for leaf_id in leaf_ids {
        let has_editors =
            find_leaf(&root, &leaf_id).is_some_and(|leaf| leaf.tabs.iter().any(is_editor));
        if !has_editors {
            continue;
        }
        root = update_leaf(&root, &leaf_id, &mut |leaf| {
            let tabs: Vec<Tab> = leaf
                .tabs
                .iter()
                .filter(|tab| !is_editor(tab))
                .cloned()
                .collect();
            Leaf {
                id: leaf.id.clone(),
                active_tab_id: keep_or_last(leaf.active_tab_id.as_ref(), &tabs),
                tabs,
            }
        });
    }
    state.tree.root = root;
}

fn is_editor(tab: &Tab) -> bool {
    matches!(tab, Tab::Editor { .. })
}

pub fn clear_pane(state: &mut State, pane_id: &str) {
    if !find_leaf(&state.tree.root, pane_id).is_some_and(|leaf| !leaf.tabs.is_empty()) {
        return;
    }
    state.tree.root = update_leaf(&state.tree.root, pane_id, &mut |leaf| Leaf {
        id: leaf.id.clone(),
        tabs: Vec::new(),
        active_tab_id: None,
    });
}

pub fn reset_layout(state: &mut State, ids: &mut dyn Ids) {
    let leaf = create_leaf(ids, Vec::new(), None);
    state.tree.active_pane_id = leaf.id().to_owned();
    state.tree.root = leaf;
    state.tree.fullscreen_pane_id = None;
}

pub fn load_project_workspace(
    state: &mut State,
    root: PaneNode,
    active: Option<&str>,
    ids: &mut dyn Ids,
) {
    let root = normalize_pane_tree(&root, ids);
    state.tree.active_pane_id = resolve_leaf_id(&root, active);
    state.tree.fullscreen_pane_id =
        resolve_fullscreen_pane_id(&root, state.tree.fullscreen_pane_id.as_deref());
    state.tree.root = root;
}

/// Put exactly these editor tabs, deduplicated, in the active pane, after its
/// terminal tabs and its other tabs.
pub fn sync_editor_tabs(
    state: &mut State,
    paths: &[String],
    active: Option<&str>,
    ids: &mut dyn Ids,
) {
    let mut root = state.tree.root.clone();
    let leaf_ids: Vec<String> = leaves(&state.tree.root)
        .iter()
        .map(|leaf| leaf.id.clone())
        .collect();
    for leaf_id in leaf_ids {
        let has_editors =
            find_leaf(&root, &leaf_id).is_some_and(|leaf| leaf.tabs.iter().any(is_editor));
        if !has_editors {
            continue;
        }
        root = update_leaf(&root, &leaf_id, &mut |leaf| {
            let active_is_editor = leaf
                .active_tab_id
                .as_ref()
                .and_then(|id| leaf.tabs.iter().find(|tab| tab.id() == id))
                .is_some_and(is_editor);
            Leaf {
                id: leaf.id.clone(),
                tabs: leaf
                    .tabs
                    .iter()
                    .filter(|tab| !is_editor(tab))
                    .cloned()
                    .collect(),
                active_tab_id: if active_is_editor {
                    None
                } else {
                    leaf.active_tab_id.clone()
                },
            }
        });
    }
    let mut editors: Vec<Tab> = Vec::new();
    for path in paths {
        let id = ids::editor(path);
        if !editors.iter().any(|tab| tab.id() == id) {
            editors.push(Tab::Editor {
                id,
                file_path: path.clone(),
            });
        }
    }
    let active_pane = state.tree.active_pane_id.clone();
    root = update_leaf(&root, &active_pane, &mut |leaf| {
        let terminals = leaf
            .tabs
            .iter()
            .filter(|tab| matches!(tab, Tab::Terminal { .. }));
        let others = leaf
            .tabs
            .iter()
            .filter(|tab| !matches!(tab, Tab::Terminal { .. }) && !is_editor(tab));
        let tabs: Vec<Tab> = terminals
            .chain(others)
            .cloned()
            .chain(editors.iter().cloned())
            .collect();
        let restored = active.filter(|id| !id.is_empty() && tabs.iter().any(|tab| tab.id() == *id));
        Leaf {
            id: leaf.id.clone(),
            active_tab_id: restored.map(str::to_owned).or_else(|| last_tab_id(&tabs)),
            tabs,
        }
    });
    set_normalized(state, &root, ids);
}

/// Re-key terminal tabs from old terminal ids to new ones.
pub fn remap_terminal_tabs(state: &mut State, map: &BTreeMap<String, String>, ids: &mut dyn Ids) {
    let mapped: BTreeMap<&str, &str> = map
        .iter()
        .filter(|(old, new)| !old.is_empty() && !new.is_empty())
        .map(|(old, new)| (old.as_str(), new.as_str()))
        .collect();
    if mapped.is_empty() {
        return;
    }
    let root = normalize_pane_tree(&remap_node(&state.tree.root, &mapped), ids);
    state.tree.active_pane_id = resolve_leaf_id(&root, Some(&state.tree.active_pane_id));
    state.tree.fullscreen_pane_id =
        resolve_fullscreen_pane_id(&root, state.tree.fullscreen_pane_id.as_deref());
    state.tree.root = root;
}

fn remap_node(node: &PaneNode, mapped: &BTreeMap<&str, &str>) -> PaneNode {
    let leaf = match node {
        PaneNode::Split(split) => {
            return PaneNode::Split(Split {
                children: split
                    .children
                    .iter()
                    .map(|child| remap_node(child, mapped))
                    .collect(),
                ..split.clone()
            });
        }
        PaneNode::Leaf(leaf) => leaf,
    };
    let tabs: Vec<Tab> = leaf
        .tabs
        .iter()
        .filter_map(|tab| {
            let Tab::Terminal { terminal_id, .. } = tab else {
                return Some(tab.clone());
            };
            let Some(new_terminal) = mapped.get(terminal_id.as_str()) else {
                return Some(tab.clone());
            };
            let new_id = ids::terminal(new_terminal);
            // The destination tab is already in this leaf: drop the source.
            let present = leaf.tabs.iter().any(|existing| {
                matches!(existing, Tab::Terminal { id, terminal_id } if *id == new_id && terminal_id == new_terminal)
            });
            (!present).then(|| Tab::Terminal {
                id: new_id,
                terminal_id: (*new_terminal).to_owned(),
            })
        })
        .collect();
    let mut active = leaf.active_tab_id.clone();
    if let Some(current) = &active {
        let remapped = mapped
            .iter()
            .find(|(old, _)| ids::terminal(old) == *current)
            .map(|(_, new)| ids::terminal(new));
        if let Some(remapped) = remapped {
            active = Some(remapped);
        }
    }
    PaneNode::Leaf(Leaf {
        id: leaf.id.clone(),
        active_tab_id: keep_or_last(active.as_ref(), &tabs),
        tabs,
    })
}
