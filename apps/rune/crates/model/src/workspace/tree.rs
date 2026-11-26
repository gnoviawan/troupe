//! Pure pane-tree helpers. Port of `workspace-store/tree.ts`; the float
//! arithmetic keeps the TypeScript operation order so sizes match bit for bit.

use super::{Leaf, PaneNode, ProjectWorkspace, Split, Tab};
use crate::{Effect, Ids, Level, js_number};

pub fn find_pane<'a>(root: &'a PaneNode, id: &str) -> Option<&'a PaneNode> {
    if root.id() == id {
        return Some(root);
    }
    match root {
        PaneNode::Split(split) => split.children.iter().find_map(|child| find_pane(child, id)),
        PaneNode::Leaf(_) => None,
    }
}

pub fn find_leaf<'a>(root: &'a PaneNode, id: &str) -> Option<&'a Leaf> {
    match find_pane(root, id) {
        Some(PaneNode::Leaf(leaf)) => Some(leaf),
        _ => None,
    }
}

pub fn find_split<'a>(root: &'a PaneNode, id: &str) -> Option<&'a Split> {
    match find_pane(root, id) {
        Some(PaneNode::Split(split)) => Some(split),
        _ => None,
    }
}

pub fn find_parent_split<'a>(root: &'a PaneNode, child_id: &str) -> Option<&'a Split> {
    let PaneNode::Split(split) = root else {
        return None;
    };
    for child in &split.children {
        if child.id() == child_id {
            return Some(split);
        }
        if let Some(found) = find_parent_split(child, child_id) {
            return Some(found);
        }
    }
    None
}

pub fn leaves(root: &PaneNode) -> Vec<&Leaf> {
    match root {
        PaneNode::Leaf(leaf) => vec![leaf],
        PaneNode::Split(split) => split.children.iter().flat_map(leaves).collect(),
    }
}

pub fn find_pane_containing_tab<'a>(root: &'a PaneNode, tab_id: &str) -> Option<&'a Leaf> {
    match root {
        PaneNode::Leaf(leaf) => leaf
            .tabs
            .iter()
            .any(|tab| tab.id() == tab_id)
            .then_some(leaf),
        PaneNode::Split(split) => split
            .children
            .iter()
            .find_map(|child| find_pane_containing_tab(child, tab_id)),
    }
}

/// The id of the leaf holding `tab_id`, owned so the tree can change after.
pub fn pane_of_tab(root: &PaneNode, tab_id: &str) -> Option<String> {
    find_pane_containing_tab(root, tab_id).map(|leaf| leaf.id.clone())
}

pub fn create_leaf(ids: &mut dyn Ids, tabs: Vec<Tab>, active_tab_id: Option<String>) -> PaneNode {
    PaneNode::Leaf(Leaf {
        id: ids.next_id(),
        tabs,
        active_tab_id,
    })
}

pub fn replace_node(root: &PaneNode, target_id: &str, replacement: &PaneNode) -> PaneNode {
    if root.id() == target_id {
        return replacement.clone();
    }
    match root {
        PaneNode::Split(split) => PaneNode::Split(Split {
            children: split
                .children
                .iter()
                .map(|child| replace_node(child, target_id, replacement))
                .collect(),
            ..split.clone()
        }),
        PaneNode::Leaf(_) => root.clone(),
    }
}

/// Remove a node; a split left with one child becomes that child, and the
/// survivors' sizes scale back up to the split's former total.
pub fn remove_node(root: &PaneNode, target_id: &str) -> Option<PaneNode> {
    if root.id() == target_id {
        return None;
    }
    let PaneNode::Split(split) = root else {
        return Some(root.clone());
    };
    let surviving: Vec<(PaneNode, usize)> = split
        .children
        .iter()
        .enumerate()
        .filter_map(|(index, child)| remove_node(child, target_id).map(|node| (node, index)))
        .collect();
    match surviving.len() {
        0 => None,
        1 => surviving.into_iter().next().map(|(node, _)| node),
        _ => {
            let sizes: Vec<f64> = surviving
                .iter()
                .map(|(_, index)| size_at(&split.sizes, *index))
                .collect();
            let surviving_total = sum(&sizes, 0.0);
            let total_original = sum(&split.sizes, 0.0);
            Some(PaneNode::Split(Split {
                children: surviving.into_iter().map(|(node, _)| node).collect(),
                sizes: sizes
                    .iter()
                    .map(|size| (size / surviving_total) * total_original)
                    .collect(),
                ..split.clone()
            }))
        }
    }
}

pub fn update_leaf(
    root: &PaneNode,
    leaf_id: &str,
    updater: &mut dyn FnMut(&Leaf) -> Leaf,
) -> PaneNode {
    match root {
        PaneNode::Leaf(leaf) if leaf.id == leaf_id => PaneNode::Leaf(updater(leaf)),
        PaneNode::Leaf(_) => root.clone(),
        PaneNode::Split(split) => PaneNode::Split(Split {
            children: split
                .children
                .iter()
                .map(|child| update_leaf(child, leaf_id, updater))
                .collect(),
            ..split.clone()
        }),
    }
}

/// Make `tab_id` the active tab of `leaf_id`.
pub fn activate_tab(root: &PaneNode, leaf_id: &str, tab_id: &str) -> PaneNode {
    update_leaf(root, leaf_id, &mut |leaf| Leaf {
        active_tab_id: Some(tab_id.to_owned()),
        ..leaf.clone()
    })
}

/// A missing entry reads as `undefined` in TypeScript, which is NaN in sums.
fn size_at(sizes: &[f64], index: usize) -> f64 {
    sizes.get(index).copied().unwrap_or(f64::NAN)
}

fn sum(values: &[f64], start: f64) -> f64 {
    values.iter().fold(start, |total, value| total + value)
}

/// Custom split sizes normalized to sum to 100, or `None` (use 50/50) when
/// absent or malformed; a malformed tuple is logged.
pub fn resolve_split_sizes(sizes: Option<&[f64]>, fx: &mut Vec<Effect>) -> Option<[f64; 2]> {
    let sizes = sizes?;
    if sizes.len() != 2 || !sizes.iter().all(|size| size.is_finite() && *size > 0.0) {
        let listed: Vec<String> = sizes.iter().map(|size| js_number(*size)).collect();
        fx.push(Effect::log(
            Level::Warn,
            "workspace-store:splitPane",
            format!(
                "[workspace] invalid splitPane sizes ignored, falling back to [50, 50] (sizes=[{}])",
                listed.join(", ")
            ),
        ));
        return None;
    }
    let total = sizes[0] + sizes[1];
    Some([(sizes[0] / total) * 100.0, (sizes[1] / total) * 100.0])
}

pub fn resolve_fullscreen_pane_id(root: &PaneNode, fullscreen: Option<&str>) -> Option<String> {
    let id = fullscreen.filter(|id| !id.is_empty())?;
    find_leaf(root, id).map(|_| id.to_owned())
}

pub fn resolve_active_pane_id(fullscreen: Option<&str>, requested: &str) -> String {
    match fullscreen {
        Some(id) if !id.is_empty() && id != requested => id.to_owned(),
        _ => requested.to_owned(),
    }
}

/// Merge nested same-direction splits into one flat group.
pub fn flatten_same_direction(root: &PaneNode) -> PaneNode {
    let PaneNode::Split(split) = root else {
        return root.clone();
    };
    let mut children = Vec::new();
    let mut sizes = Vec::new();
    for (index, child) in split.children.iter().enumerate() {
        let child = flatten_same_direction(child);
        let parent_size = split.sizes.get(index).copied().unwrap_or(1.0);
        match child {
            PaneNode::Split(inner) if inner.direction == split.direction => {
                let child_total = sum(&inner.sizes, 0.0);
                for (j, grandchild) in inner.children.into_iter().enumerate() {
                    children.push(grandchild);
                    sizes.push(parent_size * (size_at(&inner.sizes, j) / child_total));
                }
            }
            other => {
                children.push(other);
                sizes.push(parent_size);
            }
        }
    }
    // The TypeScript reduce starts at 1, not 0; kept for parity.
    let total = sum(&sizes, 1.0);
    PaneNode::Split(Split {
        children,
        sizes: sizes.iter().map(|size| (size / total) * 100.0).collect(),
        ..split.clone()
    })
}

/// Flatten, drop empty splits, unwrap single-child splits and renormalize.
pub fn normalize_pane_tree(root: &PaneNode, ids: &mut dyn Ids) -> PaneNode {
    collapse(root).unwrap_or_else(|| create_leaf(ids, Vec::new(), None))
}

fn collapse(node: &PaneNode) -> Option<PaneNode> {
    if let PaneNode::Leaf(_) = node {
        return Some(node.clone());
    }
    let PaneNode::Split(flattened) = flatten_same_direction(node) else {
        return None;
    };
    let surviving: Vec<(PaneNode, usize)> = flattened
        .children
        .iter()
        .enumerate()
        .filter_map(|(index, child)| collapse(child).map(|node| (node, index)))
        .collect();
    match surviving.len() {
        0 => None,
        1 => surviving.into_iter().next().map(|(node, _)| node),
        _ => {
            let valid: Vec<f64> = surviving
                .iter()
                .map(|(_, index)| match flattened.sizes.get(*index) {
                    Some(size) if size.is_finite() && *size > 0.0 => *size,
                    _ => 1.0,
                })
                .collect();
            let total = sum(&valid, 0.0);
            Some(PaneNode::Split(Split {
                id: flattened.id,
                direction: flattened.direction,
                children: surviving.into_iter().map(|(node, _)| node).collect(),
                sizes: valid.iter().map(|size| (size / total) * 100.0).collect(),
            }))
        }
    }
}

pub fn create_project_workspace(ids: &mut dyn Ids) -> ProjectWorkspace {
    let leaf = create_leaf(ids, Vec::new(), None);
    ProjectWorkspace {
        active_pane_id: leaf.id().to_owned(),
        root: leaf,
        fullscreen_pane_id: None,
        agent_launcher_pane_id: None,
        agent_browser_pane_id: None,
    }
}

/// The first leaf's id when `requested` is not a leaf of `root`.
pub fn resolve_leaf_id(root: &PaneNode, requested: Option<&str>) -> String {
    let leaves = leaves(root);
    match requested {
        Some(id) if !id.is_empty() && leaves.iter().any(|leaf| leaf.id == id) => id.to_owned(),
        _ => leaves
            .first()
            .map_or_else(|| root.id().to_owned(), |leaf| leaf.id.clone()),
    }
}

/// The tab after removing `index` from a leaf whose active tab was removed.
pub fn neighbour_after_removal(remaining: &[Tab], index: usize) -> Option<String> {
    if remaining.is_empty() {
        return None;
    }
    Some(remaining[index.min(remaining.len() - 1)].id().to_owned())
}

/// The last tab, for a leaf whose active tab disappeared.
pub fn last_tab_id(tabs: &[Tab]) -> Option<String> {
    tabs.last().map(|tab| tab.id().to_owned())
}

/// The first leaf of the sibling of `pane_id`, else `fallback`.
pub fn sibling_focus(root: &PaneNode, pane_id: &str, fallback: &str) -> String {
    find_parent_split(root, pane_id)
        .and_then(|parent| parent.children.iter().find(|child| child.id() != pane_id))
        .and_then(|sibling| leaves(sibling).first().map(|leaf| leaf.id.clone()))
        .unwrap_or_else(|| fallback.to_owned())
}

/// Close `tab_id` in `pane_id`; an emptied pane collapses into its sibling
/// (focus moves there) unless it is the only pane. `None` when the tab is not
/// in that pane.
pub fn close_tab_in_workspace(
    workspace: &ProjectWorkspace,
    pane_id: &str,
    tab_id: &str,
    ids: &mut dyn Ids,
) -> Option<(ProjectWorkspace, Tab)> {
    let mut removed = None;
    let mut root = update_leaf(&workspace.root, pane_id, &mut |leaf| {
        let Some(index) = leaf.tabs.iter().position(|tab| tab.id() == tab_id) else {
            return leaf.clone();
        };
        removed = Some(leaf.tabs[index].clone());
        let tabs: Vec<Tab> = leaf
            .tabs
            .iter()
            .filter(|tab| tab.id() != tab_id)
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
    });
    let removed = removed?;
    let emptied = find_leaf(&root, pane_id).is_some_and(|leaf| leaf.tabs.is_empty());
    if emptied && leaves(&root).len() > 1 {
        let active_pane_id = sibling_focus(&root, pane_id, &workspace.active_pane_id);
        root = remove_node(&root, pane_id).unwrap_or_else(|| create_leaf(ids, Vec::new(), None));
        let fullscreen_pane_id =
            resolve_fullscreen_pane_id(&root, workspace.fullscreen_pane_id.as_deref());
        return Some((
            ProjectWorkspace {
                root,
                active_pane_id,
                fullscreen_pane_id,
                ..workspace.clone()
            },
            removed,
        ));
    }
    Some((
        ProjectWorkspace {
            root,
            ..workspace.clone()
        },
        removed,
    ))
}
