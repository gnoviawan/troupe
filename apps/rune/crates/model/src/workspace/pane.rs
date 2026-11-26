//! Pane-tree actions. Port of `workspace-store/slices/pane.ts` and
//! `slices/projects.ts`.

use super::tree::{
    activate_tab, close_tab_in_workspace, create_leaf, create_project_workspace, find_leaf,
    find_parent_split, find_split, leaves, neighbour_after_removal, remove_node, replace_node,
    resolve_active_pane_id, resolve_fullscreen_pane_id, resolve_split_sizes, sibling_focus,
    update_leaf,
};
use super::{
    Direction, Leaf, PaneNode, Position, ProjectWorkspace, Split, SplitOptions, State, Tab,
};
use crate::{Effect, Ids};

/// Park the visible tree under the current project and show `project_id`'s
/// parked tree, else `tree`, else a fresh one.
pub fn activate_project(
    state: &mut State,
    project_id: &str,
    tree: Option<ProjectWorkspace>,
    ids: &mut dyn Ids,
) {
    if state.active_project_id == project_id {
        // Re-activating the visible project only ever swaps in an explicit tree.
        if let Some(tree) = tree {
            state.tree = tree;
        }
        return;
    }
    let mut parked = state.parked_trees.clone();
    if !state.active_project_id.is_empty() {
        parked.insert(state.active_project_id.clone(), state.tree.clone());
    }
    let incoming = match parked.remove(project_id) {
        Some(found) => found,
        None => tree.unwrap_or_else(|| create_project_workspace(ids)),
    };
    state.active_project_id = project_id.to_owned();
    state.parked_trees = parked;
    state.tree = incoming;
}

#[allow(clippy::too_many_arguments)]
pub fn split_pane(
    state: &mut State,
    pane_id: &str,
    direction: Direction,
    tab: Tab,
    position: Position,
    options: &SplitOptions,
    ids: &mut dyn Ids,
    fx: &mut Vec<Effect>,
) {
    let root = &state.tree.root;
    let Some(target) = find_leaf(root, pane_id).cloned() else {
        return;
    };
    let custom = resolve_split_sizes(options.sizes.as_deref(), fx);
    let focus = options.focus != Some(false);
    let tab_id = tab.id().to_owned();
    let new_leaf = create_leaf(ids, vec![tab], Some(tab_id));
    let leading = position.is_leading();
    let next_active = if focus {
        new_leaf.id().to_owned()
    } else {
        state.tree.active_pane_id.clone()
    };
    // A focusing split leaves fullscreen; a background split keeps it.
    let next_fullscreen = if focus {
        None
    } else {
        state.tree.fullscreen_pane_id.clone()
    };

    if let Some(parent) = find_parent_split(root, pane_id).filter(|p| p.direction == direction) {
        let Some(target_index) = parent.children.iter().position(|c| c.id() == pane_id) else {
            return;
        };
        let insert = if leading {
            target_index
        } else {
            target_index + 1
        };
        let mut children = parent.children.clone();
        children.insert(insert, new_leaf);
        let sizes = match custom {
            Some(custom) => {
                // The target keeps its share of its former extent.
                let kept_ratio = custom[0] / 100.0;
                let target_size = parent
                    .sizes
                    .get(target_index)
                    .copied()
                    .unwrap_or(100.0 / parent.children.len() as f64);
                let kept = target_size * kept_ratio;
                let mut sizes = parent.sizes.clone();
                if target_index < sizes.len() {
                    sizes[target_index] = kept;
                } else {
                    sizes.resize(target_index, f64::NAN);
                    sizes.push(kept);
                }
                splice_insert(&mut sizes, insert, target_size - kept);
                sizes
            }
            None => grow_flat_group(&parent.sizes, parent.children.len(), insert),
        };
        let updated = PaneNode::Split(Split {
            children,
            sizes,
            ..parent.clone()
        });
        state.tree.root = replace_node(root, &parent.id.clone(), &updated);
        state.tree.active_pane_id = next_active;
        state.tree.fullscreen_pane_id = next_fullscreen;
        return;
    }

    let sizes = match custom {
        Some([kept, added]) if leading => vec![added, kept],
        Some([kept, added]) => vec![kept, added],
        None => vec![50.0, 50.0],
    };
    let target = PaneNode::Leaf(target);
    let split = PaneNode::Split(Split {
        id: ids.next_id(),
        direction,
        children: if leading {
            vec![new_leaf, target]
        } else {
            vec![target, new_leaf]
        },
        sizes,
    });
    state.tree.root = replace_node(root, pane_id, &split);
    state.tree.active_pane_id = next_active;
    state.tree.fullscreen_pane_id = next_fullscreen;
}

/// Sizes of a flat group of `count` children after inserting an equal share at `insert`.
fn grow_flat_group(sizes: &[f64], count: usize, insert: usize) -> Vec<f64> {
    let count = count as f64;
    let new_size = 100.0 / (count + 1.0);
    let scale = count / (count + 1.0);
    let mut sizes: Vec<f64> = sizes.iter().map(|size| size * scale).collect();
    splice_insert(&mut sizes, insert, new_size);
    sizes
}

/// `Array.prototype.splice(index, 0, value)`: an index past the end appends.
fn splice_insert(sizes: &mut Vec<f64>, index: usize, value: f64) {
    sizes.insert(index.min(sizes.len()), value);
}

fn clear_launcher_on(state: &State, pane_id: &str) -> Option<String> {
    state
        .tree
        .agent_launcher_pane_id
        .clone()
        .filter(|launcher| launcher != pane_id)
}

pub fn add_tab_to_pane(state: &mut State, pane_id: &str, tab: Tab, focus: bool) {
    let Some(pane) = find_leaf(&state.tree.root, pane_id) else {
        return;
    };
    // Opening a tab dismisses the agent launcher over that pane.
    let launcher = clear_launcher_on(state, pane_id);
    if pane.tabs.iter().any(|existing| existing.id() == tab.id()) {
        state.tree.root = activate_tab(&state.tree.root, pane_id, tab.id());
        state.tree.agent_launcher_pane_id = launcher;
        return;
    }
    state.tree.root = update_leaf(&state.tree.root, pane_id, &mut |leaf| Leaf {
        id: leaf.id.clone(),
        tabs: leaf.tabs.iter().cloned().chain([tab.clone()]).collect(),
        active_tab_id: Some(tab.id().to_owned()),
    });
    if focus {
        state.tree.active_pane_id =
            resolve_active_pane_id(state.tree.fullscreen_pane_id.as_deref(), pane_id);
    }
    state.tree.agent_launcher_pane_id = launcher;
}

pub fn add_tab_to_parked_pane(
    state: &mut State,
    project_id: &str,
    pane_id: &str,
    tab: Tab,
) -> bool {
    if project_id == state.active_project_id {
        return false;
    }
    let Some(parked) = state.parked_trees.get_mut(project_id) else {
        return false;
    };
    if find_leaf(&parked.root, pane_id).is_none() {
        return false;
    }
    parked.root = update_leaf(&parked.root, pane_id, &mut |leaf| Leaf {
        id: leaf.id.clone(),
        tabs: if leaf.tabs.iter().any(|t| t.id() == tab.id()) {
            leaf.tabs.clone()
        } else {
            leaf.tabs.iter().cloned().chain([tab.clone()]).collect()
        },
        active_tab_id: Some(tab.id().to_owned()),
    });
    true
}

/// Remove `tab_id` from `source`, picking the neighbour as the active tab.
fn take_tab(root: &PaneNode, source: &str, tab_id: &str) -> PaneNode {
    update_leaf(root, source, &mut |leaf| {
        let index = leaf.tabs.iter().position(|t| t.id() == tab_id).unwrap_or(0);
        let tabs: Vec<Tab> = leaf
            .tabs
            .iter()
            .filter(|t| t.id() != tab_id)
            .cloned()
            .collect();
        let active_tab_id = if leaf.active_tab_id.as_deref() == Some(tab_id) {
            neighbour_after_removal(&tabs, index)
        } else {
            leaf.active_tab_id.clone()
        };
        Leaf {
            id: leaf.id.clone(),
            tabs,
            active_tab_id,
        }
    })
}

fn drop_if_empty(root: PaneNode, pane_id: &str, ids: &mut dyn Ids) -> PaneNode {
    if find_leaf(&root, pane_id).is_some_and(|leaf| leaf.tabs.is_empty()) {
        return remove_node(&root, pane_id).unwrap_or_else(|| create_leaf(ids, Vec::new(), None));
    }
    root
}

pub fn move_tab_to_pane(
    state: &mut State,
    tab_id: &str,
    source: &str,
    target: &str,
    ids: &mut dyn Ids,
) {
    if source == target {
        return;
    }
    let Some(tab) = find_leaf(&state.tree.root, source)
        .and_then(|leaf| leaf.tabs.iter().find(|t| t.id() == tab_id))
        .cloned()
    else {
        return;
    };
    let root = take_tab(&state.tree.root, source, tab_id);
    let root = update_leaf(&root, target, &mut |leaf| Leaf {
        id: leaf.id.clone(),
        tabs: if leaf.tabs.iter().any(|t| t.id() == tab_id) {
            leaf.tabs.clone()
        } else {
            leaf.tabs.iter().cloned().chain([tab.clone()]).collect()
        },
        active_tab_id: Some(tab_id.to_owned()),
    });
    let root = drop_if_empty(root, source, ids);
    state.tree.fullscreen_pane_id =
        resolve_fullscreen_pane_id(&root, state.tree.fullscreen_pane_id.as_deref());
    state.tree.root = root;
    state.tree.active_pane_id = target.to_owned();
}

pub fn move_tab_to_new_split(
    state: &mut State,
    tab_id: &str,
    source: &str,
    target: &str,
    position: Position,
    ids: &mut dyn Ids,
) {
    if position == Position::Center {
        move_tab_to_pane(state, tab_id, source, target, ids);
        return;
    }
    let Some(tab) = find_leaf(&state.tree.root, source)
        .and_then(|leaf| leaf.tabs.iter().find(|t| t.id() == tab_id))
        .cloned()
    else {
        return;
    };
    let root = take_tab(&state.tree.root, source, tab_id);
    let mut root = drop_if_empty(root, source, ids);

    let Some(target_leaf) = find_leaf(&root, target).cloned() else {
        // The target was the source pane that just collapsed.
        let leaf = create_leaf(ids, vec![tab], Some(tab_id.to_owned()));
        state.tree.active_pane_id = leaf.id().to_owned();
        state.tree.root = leaf;
        state.tree.fullscreen_pane_id = None;
        return;
    };
    let direction = match position {
        Position::Left | Position::Right => Direction::Horizontal,
        _ => Direction::Vertical,
    };
    let new_leaf = create_leaf(ids, vec![tab.clone()], Some(tab_id.to_owned()));
    let new_leaf_id = new_leaf.id().to_owned();

    if let Some(parent) = find_parent_split(&root, target).filter(|p| p.direction == direction) {
        let Some(target_index) = parent.children.iter().position(|c| c.id() == target) else {
            let leaf = create_leaf(ids, vec![tab], Some(tab_id.to_owned()));
            state.tree.active_pane_id = leaf.id().to_owned();
            state.tree.root = leaf;
            state.tree.fullscreen_pane_id = None;
            return;
        };
        let insert = if position.is_leading() {
            target_index
        } else {
            target_index + 1
        };
        let sizes = grow_flat_group(&parent.sizes, parent.children.len(), insert);
        let mut children = parent.children.clone();
        children.insert(insert, new_leaf);
        let updated = PaneNode::Split(Split {
            children,
            sizes,
            ..parent.clone()
        });
        root = replace_node(&root, &parent.id.clone(), &updated);
    } else {
        let target_node = PaneNode::Leaf(target_leaf);
        let split = PaneNode::Split(Split {
            id: ids.next_id(),
            direction,
            children: if position.is_leading() {
                vec![new_leaf, target_node]
            } else {
                vec![target_node, new_leaf]
            },
            sizes: vec![50.0, 50.0],
        });
        root = replace_node(&root, target, &split);
    }
    state.tree.fullscreen_pane_id =
        resolve_fullscreen_pane_id(&root, state.tree.fullscreen_pane_id.as_deref());
    state.tree.root = root;
    state.tree.active_pane_id = new_leaf_id;
}

pub fn close_tab(state: &mut State, pane_id: &str, tab_id: &str, ids: &mut dyn Ids) -> Option<Tab> {
    let (workspace, removed) = close_tab_in_workspace(&state.tree, pane_id, tab_id, ids)?;
    state.tree.root = workspace.root;
    state.tree.active_pane_id = workspace.active_pane_id;
    state.tree.fullscreen_pane_id = workspace.fullscreen_pane_id;
    Some(removed)
}

pub fn set_active_tab(state: &mut State, pane_id: &str, tab_id: &str, fx: &mut Vec<Effect>) {
    let chat = find_leaf(&state.tree.root, pane_id)
        .and_then(|leaf| leaf.tabs.iter().find(|t| t.id() == tab_id))
        .and_then(|tab| match tab {
            Tab::AgentChat { session_id, .. } => Some(session_id.clone()),
            _ => None,
        });
    state.tree.agent_launcher_pane_id = clear_launcher_on(state, pane_id);
    state.tree.root = activate_tab(&state.tree.root, pane_id, tab_id);
    state.tree.active_pane_id =
        resolve_active_pane_id(state.tree.fullscreen_pane_id.as_deref(), pane_id);
    if let Some(session_id) = chat {
        fx.push(Effect::NavigateToChat {
            session_id,
            replace_if_on: None,
        });
    }
}

pub fn toggle_fullscreen(state: &mut State, pane_id: &str) {
    if find_leaf(&state.tree.root, pane_id).is_none() {
        return;
    }
    let is_fullscreen = state.tree.fullscreen_pane_id.as_deref() == Some(pane_id);
    // With one leaf, entering fullscreen is a no-op.
    if leaves(&state.tree.root).len() <= 1 && !is_fullscreen {
        return;
    }
    state.tree.active_pane_id = pane_id.to_owned();
    state.tree.fullscreen_pane_id = if is_fullscreen {
        None
    } else {
        Some(pane_id.to_owned())
    };
}

/// Resize a split in the visible tree, or in `project_id`'s parked tree.
pub fn update_pane_sizes(
    state: &mut State,
    split_id: &str,
    sizes: Vec<f64>,
    project_id: Option<&str>,
) {
    let parked_id = project_id.filter(|id| *id != state.active_project_id);
    let root = match parked_id {
        Some(id) => match state.parked_trees.get(id) {
            Some(parked) => &parked.root,
            // A named project that is neither active nor parked has no tree.
            None => return,
        },
        None => &state.tree.root,
    };
    let Some(node) = find_split(root, split_id) else {
        return;
    };
    let unchanged = node.sizes.len() == sizes.len()
        && node
            .sizes
            .iter()
            .zip(&sizes)
            .all(|(old, new)| (old - new).abs() < 0.01);
    if unchanged {
        return;
    }
    let updated = PaneNode::Split(Split {
        sizes,
        ..node.clone()
    });
    let new_root = replace_node(root, split_id, &updated);
    match parked_id.and_then(|id| state.parked_trees.get_mut(id)) {
        Some(parked) => parked.root = new_root,
        None => state.tree.root = new_root,
    }
}

pub fn collapse_pane(state: &mut State, pane_id: &str, ids: &mut dyn Ids) {
    if leaves(&state.tree.root).len() <= 1 {
        return;
    }
    let active = sibling_focus(&state.tree.root, pane_id, &state.tree.active_pane_id);
    let root = remove_node(&state.tree.root, pane_id)
        .unwrap_or_else(|| create_leaf(ids, Vec::new(), None));
    state.tree.fullscreen_pane_id =
        resolve_fullscreen_pane_id(&root, state.tree.fullscreen_pane_id.as_deref());
    state.tree.root = root;
    state.tree.active_pane_id = active;
}

pub fn reorder_tabs(state: &mut State, pane_id: &str, order: &[String]) {
    state.tree.root = update_leaf(&state.tree.root, pane_id, &mut |leaf| {
        let mut tabs: Vec<Tab> = order
            .iter()
            .filter_map(|id| leaf.tabs.iter().rev().find(|t| t.id() == id).cloned())
            .collect();
        tabs.extend(
            leaf.tabs
                .iter()
                .filter(|t| !order.iter().any(|id| id == t.id()))
                .cloned(),
        );
        Leaf {
            tabs,
            ..leaf.clone()
        }
    });
}
