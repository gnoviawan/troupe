//! Tab openers. Port of `workspace-store/slices/tabs.ts`.

use super::pane::{add_tab_to_pane, close_tab, split_pane};
use super::tree::{
    activate_tab, close_tab_in_workspace, find_leaf, leaves, pane_of_tab, resolve_active_pane_id,
    update_leaf,
};
use super::{Direction, GithubItem, Leaf, Position, SplitOptions, State, Tab, ids};
use crate::{Effect, Ids, Level};

const AGENT_BROWSER_SPLIT_SIZES: [f64; 2] = [33.3, 66.7];

fn target_or_active(state: &State, target: Option<&str>) -> String {
    target.map_or_else(|| state.tree.active_pane_id.clone(), str::to_owned)
}

/// Activate `tab_id` in `leaf_id`, focus that pane (unless another is
/// fullscreen) and dismiss a launcher over it.
fn activate_in_place(state: &mut State, leaf_id: &str, tab_id: &str) {
    state.tree.root = activate_tab(&state.tree.root, leaf_id, tab_id);
    state.tree.active_pane_id =
        resolve_active_pane_id(state.tree.fullscreen_pane_id.as_deref(), leaf_id);
    if state.tree.agent_launcher_pane_id.as_deref() == Some(leaf_id) {
        state.tree.agent_launcher_pane_id = None;
    }
}

/// Activate the tab where it is open, else add `tab` to the target pane.
fn activate_or_add(state: &mut State, tab: Tab, target: Option<&str>) {
    let pane_id = target_or_active(state, target);
    match pane_of_tab(&state.tree.root, tab.id()) {
        Some(existing) => activate_in_place(state, &existing, tab.id()),
        None => add_tab_to_pane(state, &pane_id, tab, true),
    }
}

/// Like [`activate_or_add`], but focusing a tab in a pane other than the
/// fullscreen one leaves fullscreen, since only that pane renders.
fn focus_or_add(state: &mut State, tab: Tab, target: Option<&str>) {
    let pane_id = target_or_active(state, target);
    let Some(existing) = pane_of_tab(&state.tree.root, tab.id()) else {
        add_tab_to_pane(state, &pane_id, tab, true);
        return;
    };
    let leaves_fullscreen = state
        .tree
        .fullscreen_pane_id
        .as_ref()
        .is_some_and(|fullscreen| *fullscreen != existing);
    if leaves_fullscreen {
        state.tree.fullscreen_pane_id = None;
    }
    activate_in_place(state, &existing, tab.id());
}

pub fn add_terminal_tab(state: &mut State, terminal_id: &str, target: Option<&str>) {
    let tab = Tab::Terminal {
        id: ids::terminal(terminal_id),
        terminal_id: terminal_id.to_owned(),
    };
    activate_or_add(state, tab, target);
}

pub fn ensure_terminal_tab(
    state: &mut State,
    terminal_id: &str,
    target: Option<&str>,
    make_active: bool,
) {
    let id = ids::terminal(terminal_id);
    if pane_of_tab(&state.tree.root, &id).is_some() {
        return;
    }
    let pane_id = target_or_active(state, target);
    let tab = Tab::Terminal {
        id,
        terminal_id: terminal_id.to_owned(),
    };
    if make_active {
        add_tab_to_pane(state, &pane_id, tab, true);
        return;
    }
    if find_leaf(&state.tree.root, &pane_id).is_none() {
        return;
    }
    state.tree.root = update_leaf(&state.tree.root, &pane_id, &mut |leaf| Leaf {
        tabs: leaf.tabs.iter().cloned().chain([tab.clone()]).collect(),
        ..leaf.clone()
    });
}

pub fn add_editor_tab(state: &mut State, path: &str, target: Option<&str>) {
    let id = ids::editor(path);
    let pane_id = target_or_active(state, target);
    // Only the target pane is checked: the same file may be open in another.
    let in_target = find_leaf(&state.tree.root, &pane_id)
        .is_some_and(|leaf| leaf.tabs.iter().any(|t| t.id() == id));
    if in_target {
        activate_in_place(state, &pane_id, &id);
        return;
    }
    let tab = Tab::Editor {
        id,
        file_path: path.to_owned(),
    };
    add_tab_to_pane(state, &pane_id, tab, true);
}

pub fn add_browser_tab(state: &mut State, browser_tab_id: &str, target: Option<&str>) {
    let tab = Tab::Browser {
        id: ids::browser(browser_tab_id),
        browser_tab_id: browser_tab_id.to_owned(),
    };
    activate_or_add(state, tab, target);
}

/// Only the fullscreen pane renders, so a tab placed elsewhere would never
/// mount; leave fullscreen unless `hosting` is the fullscreen pane.
fn ensure_visible(state: &mut State, hosting: &str) {
    if state
        .tree
        .fullscreen_pane_id
        .as_deref()
        .is_some_and(|fullscreen| fullscreen != hosting)
    {
        state.tree.fullscreen_pane_id = None;
    }
}

/// Open an agent's browser tab in the dedicated right-hand pane, splitting
/// the active pane on first use without moving focus off the chat.
pub fn open_agent_browser_tab(
    state: &mut State,
    browser_tab_id: &str,
    ids: &mut dyn Ids,
    fx: &mut Vec<Effect>,
) {
    let id = ids::browser(browser_tab_id);
    if let Some(existing) = pane_of_tab(&state.tree.root, &id) {
        ensure_visible(state, &existing);
        add_browser_tab(state, browser_tab_id, None);
        return;
    }
    let tab = Tab::Browser {
        id: id.clone(),
        browser_tab_id: browser_tab_id.to_owned(),
    };
    let dedicated = state
        .tree
        .agent_browser_pane_id
        .as_deref()
        .and_then(|pane_id| find_leaf(&state.tree.root, pane_id))
        .map(|leaf| leaf.id.clone());
    if let Some(dedicated) = dedicated {
        ensure_visible(state, &dedicated);
        add_tab_to_pane(state, &dedicated, tab, false);
        return;
    }
    let split_target = if find_leaf(&state.tree.root, &state.tree.active_pane_id).is_some() {
        Some(state.tree.active_pane_id.clone())
    } else {
        leaves(&state.tree.root).first().map(|leaf| leaf.id.clone())
    };
    if let Some(split_target) = split_target {
        let options = SplitOptions {
            sizes: Some(AGENT_BROWSER_SPLIT_SIZES.to_vec()),
            focus: Some(false),
        };
        split_pane(
            state,
            &split_target,
            Direction::Horizontal,
            tab,
            Position::Right,
            &options,
            ids,
            fx,
        );
    }
    if let Some(new_leaf) = pane_of_tab(&state.tree.root, &id) {
        ensure_visible(state, &new_leaf);
        state.tree.agent_browser_pane_id = Some(new_leaf);
        return;
    }
    fx.push(Effect::log(
        Level::Error,
        "workspace-store:openAgentBrowserTab",
        format!(
            "[workspace] agent browser tab could not be placed: no leaf pane accepted the split (tabId={browser_tab_id})"
        ),
    ));
}

fn chat_tab(session_id: &str) -> Tab {
    let id = ids::agent_chat(session_id);
    Tab::AgentChat {
        mount_key: Some(id.clone()),
        id,
        session_id: session_id.to_owned(),
    }
}

pub fn add_agent_chat_tab(
    state: &mut State,
    session_id: &str,
    target: Option<&str>,
    fx: &mut Vec<Effect>,
) {
    let id = ids::agent_chat(session_id);
    let pane_id = target_or_active(state, target);
    fx.push(Effect::NavigateToChat {
        session_id: session_id.to_owned(),
        replace_if_on: None,
    });
    let Some(existing) = find_leaf_holding(state, &id) else {
        add_tab_to_pane(state, &pane_id, chat_tab(session_id), true);
        return;
    };
    // Already active, focused and uncovered: nothing would change.
    let noop = existing.active_tab_id.as_deref() == Some(id.as_str())
        && state.tree.active_pane_id
            == resolve_active_pane_id(state.tree.fullscreen_pane_id.as_deref(), &existing.id)
        && state.tree.agent_launcher_pane_id.as_deref() != Some(existing.id.as_str());
    if !noop {
        activate_in_place(state, &existing.id, &id);
    }
}

fn find_leaf_holding(state: &State, tab_id: &str) -> Option<Leaf> {
    super::tree::find_pane_containing_tab(&state.tree.root, tab_id).cloned()
}

/// Put a chat tab back in the active pane without focusing it.
pub fn insert_agent_chat_tab(state: &mut State, session_id: &str) {
    let tab = chat_tab(session_id);
    if pane_of_tab(&state.tree.root, tab.id()).is_some() {
        return;
    }
    let pane_id = state.tree.active_pane_id.clone();
    if find_leaf(&state.tree.root, &pane_id).is_none() {
        return;
    }
    state.tree.root = update_leaf(&state.tree.root, &pane_id, &mut |leaf| Leaf {
        id: leaf.id.clone(),
        tabs: leaf.tabs.iter().cloned().chain([tab.clone()]).collect(),
        active_tab_id: leaf
            .active_tab_id
            .clone()
            .or_else(|| Some(tab.id().to_owned())),
    });
}

/// The Git Changes tab (or with `history`, the Git History tab) of `cwd`.
pub fn add_git_tab(state: &mut State, cwd: &str, target: Option<&str>, history: bool) {
    let tab = if history {
        Tab::GitHistory {
            id: ids::git_history(cwd),
            cwd: cwd.to_owned(),
        }
    } else {
        Tab::Git {
            id: ids::git(cwd),
            cwd: cwd.to_owned(),
        }
    };
    activate_or_add(state, tab, target);
}

pub fn add_github_item_tab(state: &mut State, item: GithubItem, target: Option<&str>) {
    let tab = Tab::GithubItem {
        id: ids::github_item(&item.cwd, &item.repo, item.number),
        cwd: item.cwd,
        repo: item.repo,
        kind: item.kind,
        number: item.number,
        title: item.title,
    };
    focus_or_add(state, tab, target);
}

pub fn add_diff_tab(state: &mut State, cwd: &str, path: &str, staged: bool, target: Option<&str>) {
    let tab = Tab::Diff {
        id: ids::diff_tab_id(cwd, path, staged),
        cwd: cwd.to_owned(),
        path: ids::repo_relative(path),
        staged,
    };
    focus_or_add(state, tab, target);
}

pub fn add_commit_diff_tab(
    state: &mut State,
    cwd: &str,
    hash: &str,
    path: &str,
    old_path: Option<&str>,
    target: Option<&str>,
) {
    let tab = Tab::CommitDiff {
        id: ids::commit_diff_tab_id(cwd, hash, path),
        cwd: cwd.to_owned(),
        commit: hash.to_owned(),
        path: ids::repo_relative(path),
        old_path: old_path
            .filter(|old| !old.is_empty())
            .map(ids::repo_relative),
    };
    focus_or_add(state, tab, target);
}

/// The project's one canvas tab; a different doc re-binds it in place.
pub fn add_canvas_tab(state: &mut State, project_id: &str, doc_path: &str, target: Option<&str>) {
    let id = ids::canvas(project_id);
    let Some(existing) = pane_of_tab(&state.tree.root, &id) else {
        let pane_id = target_or_active(state, target);
        let tab = Tab::Canvas {
            id,
            project_id: project_id.to_owned(),
            doc_path: doc_path.to_owned(),
        };
        add_tab_to_pane(state, &pane_id, tab, true);
        return;
    };
    state.tree.root = update_leaf(&state.tree.root, &existing, &mut |leaf| Leaf {
        id: leaf.id.clone(),
        active_tab_id: Some(id.clone()),
        tabs: leaf
            .tabs
            .iter()
            .map(|tab| match tab {
                Tab::Canvas {
                    id: tab_id,
                    project_id,
                    ..
                } if *tab_id == id => Tab::Canvas {
                    id: tab_id.clone(),
                    project_id: project_id.clone(),
                    doc_path: doc_path.to_owned(),
                },
                other => other.clone(),
            })
            .collect(),
    });
    activate_in_place(state, &existing, &id);
}

pub fn add_automations_tab(
    state: &mut State,
    project_id: &str,
    target: Option<&str>,
    fx: &mut Vec<Effect>,
) {
    // Mid-switch the project store already names the next project; its tab
    // must not land in the tree being left.
    if project_id != state.active_project_id {
        let on_screen = if state.active_project_id.is_empty() {
            "no project"
        } else {
            &state.active_project_id
        };
        fx.push(Effect::log(
            Level::Warn,
            "workspace-store.addAutomationsTab",
            format!("ignored an Automations tab for {project_id} while {on_screen} is on screen"),
        ));
        return;
    }
    let tab = Tab::Automations {
        id: ids::automations(project_id),
        project_id: project_id.to_owned(),
    };
    focus_or_add(state, tab, target);
}

/// Swap a launch placeholder's chat tab to the real session id in place.
pub fn remap_agent_chat_session(
    state: &mut State,
    from: &str,
    to: &str,
    target: Option<&str>,
    fx: &mut Vec<Effect>,
) {
    if from == to {
        add_agent_chat_tab(state, to, target, fx);
        return;
    }
    let from_id = ids::agent_chat(from);
    let to_id = ids::agent_chat(to);
    let Some(pane_id) = pane_of_tab(&state.tree.root, &from_id) else {
        add_agent_chat_tab(state, to, target, fx);
        return;
    };
    state.tree.root = update_leaf(&state.tree.root, &pane_id, &mut |leaf| Leaf {
        id: leaf.id.clone(),
        // Drop a destination tab first so the remapped source, which keeps
        // the stable mount key, is the one survivor.
        tabs: leaf
            .tabs
            .iter()
            .filter(|tab| !(matches!(tab, Tab::AgentChat { .. }) && tab.id() == to_id))
            .map(|tab| match tab {
                Tab::AgentChat { id, mount_key, .. } if *id == from_id => Tab::AgentChat {
                    id: to_id.clone(),
                    session_id: to.to_owned(),
                    mount_key: Some(mount_key.clone().unwrap_or_else(|| id.clone())),
                },
                other => other.clone(),
            })
            .collect(),
        active_tab_id: if leaf.active_tab_id.as_deref() == Some(from_id.as_str()) {
            Some(to_id.clone())
        } else {
            leaf.active_tab_id.clone()
        },
    });
    state.tree.active_pane_id =
        resolve_active_pane_id(state.tree.fullscreen_pane_id.as_deref(), &pane_id);
    if state.tree.agent_launcher_pane_id.as_deref() == Some(pane_id.as_str()) {
        state.tree.agent_launcher_pane_id = None;
    }
    // The placeholder's route becomes the session's: replace, so Back does
    // not land on a launch id that no longer exists.
    fx.push(Effect::NavigateToChat {
        session_id: to.to_owned(),
        replace_if_on: Some(from.to_owned()),
    });
}

/// Re-key an editor tab to the path its file was renamed to.
pub fn rename_editor_tab(state: &mut State, from: &str, to: &str, ids_source: &mut dyn Ids) {
    let from_id = ids::editor(from);
    let to_id = ids::editor(to);
    let Some(pane_id) = pane_of_tab(&state.tree.root, &from_id) else {
        return;
    };
    if from_id == to_id {
        return;
    }
    // A tab open at the destination shows the file this rename overwrote.
    if pane_of_tab(&state.tree.root, &to_id).is_some() {
        remove_tab(state, &to_id, ids_source);
    }
    state.tree.root = update_leaf(&state.tree.root, &pane_id, &mut |leaf| Leaf {
        id: leaf.id.clone(),
        tabs: leaf
            .tabs
            .iter()
            .map(|tab| match tab {
                Tab::Editor { id, .. } if *id == from_id => Tab::Editor {
                    id: to_id.clone(),
                    file_path: to.to_owned(),
                },
                other => other.clone(),
            })
            .collect(),
        active_tab_id: if leaf.active_tab_id.as_deref() == Some(from_id.as_str()) {
            Some(to_id.clone())
        } else {
            leaf.active_tab_id.clone()
        },
    });
}

/// Close a tab wherever it is: the visible tree first, then parked trees.
pub fn remove_tab(state: &mut State, tab_id: &str, ids: &mut dyn Ids) {
    if let Some(pane_id) = pane_of_tab(&state.tree.root, tab_id) {
        close_tab(state, &pane_id, tab_id, ids);
        return;
    }
    let found = state.parked_trees.iter().find_map(|(project_id, parked)| {
        let pane_id = pane_of_tab(&parked.root, tab_id)?;
        Some((project_id.clone(), pane_id))
    });
    let Some((project_id, pane_id)) = found else {
        return;
    };
    let Some(parked) = state.parked_trees.get(&project_id) else {
        return;
    };
    if let Some((workspace, _)) = close_tab_in_workspace(parked, &pane_id, tab_id, ids) {
        state.parked_trees.insert(project_id, workspace);
    }
}
