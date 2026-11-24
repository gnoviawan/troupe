//! The project list as this device sees it: the host's `projects` watch
//! merged with the device's own fields, the active project and the sidebar
//! groups. Port of `src/renderer/stores/project-store.ts` (its synchronous
//! actions) and `project-registry-feed.ts`, over [`prefs`](crate::prefs).
//!
//! Renames, recolours, moves and deletes are host commands: `step` returns
//! them as effects and the next watch frame carries the result.

use serde::{Deserialize, Serialize};

use crate::prefs::{self, ClientFields, Worktree};
use crate::{Effect, Ids};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: String,
    pub name: String,
    pub color: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Set by this device's selection, not by the host.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_active: Option<bool>,
    /// The host's default project.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_default: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_archived: Option<bool>,
    #[serde(flatten)]
    pub client: ClientFields,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Group {
    pub id: String,
    pub name: String,
    pub project_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_collapsed: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

/// One project in a `projects` watch frame.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSummary {
    pub id: String,
    pub name: String,
    pub color: String,
    pub path: Option<String>,
    #[serde(default)]
    pub is_archived: bool,
    #[serde(default)]
    pub is_default: bool,
}

/// A `projects` watch frame.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectList {
    pub projects: Vec<ProjectSummary>,
    pub default_project_id: Option<String>,
}

/// The host fields an update changes.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct HostPatch {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

/// `updateProject`'s argument: host fields and device fields together.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectUpdates {
    pub name: Option<String>,
    pub color: Option<String>,
    pub path: Option<String>,
    #[serde(flatten)]
    pub client: ClientFields,
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupUpdates {
    pub name: Option<String>,
    pub project_ids: Option<Vec<String>>,
    pub is_collapsed: Option<bool>,
    pub color: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct State {
    pub projects: Vec<Project>,
    pub groups: Vec<Group>,
    /// This device's choice (`""` before the first list).
    pub active_project_id: String,
    /// The first watch frame has landed.
    pub is_loaded: bool,
    pub is_worktree_operation_locked: bool,
}

/// The project store and the device prefs it reads and writes.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Model {
    pub store: State,
    pub prefs: prefs::State,
}

/// The store's actions and the prefs and feed calls, by their TypeScript names.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Event {
    /// A `projects` watch frame, and the device's saved selection (read only
    /// for the first frame).
    ApplyRegistryList(ProjectList, Option<String>),
    SetProjects(
        Vec<Project>,
        #[serde(default)] Option<String>,
        #[serde(default)] Option<Vec<Group>>,
    ),
    SelectProject(String),
    UpdateProject(String, ProjectUpdates),
    ReorderProjects(Vec<String>),
    AddWorktree(String, Worktree),
    RemoveWorktree(String, String),
    SetWorktreeOperationLock(bool),
    AddGroup(String),
    RemoveGroup(String, bool),
    RenameGroup(String, String),
    ToggleGroupCollapse(String),
    MoveProjectToGroup(String, Option<String>, #[serde(default)] Option<usize>),
    ReorderGroups(Vec<String>),
    ReorderProjectInGroup(String, Vec<String>),
    UpdateGroup(String, GroupUpdates),
    /// The `project-prefs` record was read (`None`: there is none yet).
    LoadProjectPrefs(Option<prefs::Record>),
    RememberProjectPrefs(String, ClientFields),
    PersistProjectPrefs,
    /// A `WritePrefs` effect failed; carries its `signature`.
    PrefsWriteFailed(String),
    ForgetProjectPrefs(String),
}

#[derive(Debug, Default, PartialEq)]
pub struct Out {
    pub effects: Vec<Effect>,
    /// A new group's id, the active project after a list, or a load's result.
    pub reply: Option<serde_json::Value>,
}

pub fn step(model: &mut Model, event: Event, ids: &mut dyn Ids) -> Out {
    let mut out = Out::default();
    let fx = &mut out.effects;
    let store = &mut model.store;
    match event {
        Event::ApplyRegistryList(list, saved) => {
            let active = apply_registry_list(model, list, saved.as_deref(), fx);
            out.reply = Some(active.into());
        }
        Event::SetProjects(projects, active, groups) => {
            set_projects(store, projects, active, groups)
        }
        Event::SelectProject(id) => {
            let changed = store.active_project_id != id;
            for project in &mut store.projects {
                project.is_active = Some(project.id == id);
            }
            store.active_project_id = id.clone();
            if changed {
                fx.push(Effect::SaveActiveProject { project_id: id });
                fx.push(Effect::ClearChatRoute);
            }
        }
        Event::UpdateProject(id, updates) => update_project(store, &id, updates, fx),
        Event::ReorderProjects(order) => {
            let (archived, active): (Vec<Project>, Vec<Project>) = store
                .projects
                .drain(..)
                .partition(|p| p.is_archived == Some(true));
            let mut projects: Vec<Project> = order
                .iter()
                .filter_map(|id| active.iter().rev().find(|p| p.id == *id).cloned())
                .collect();
            // Projects missing from the order (grouped ones) keep their place after.
            projects.extend(active.into_iter().filter(|p| !order.contains(&p.id)));
            projects.extend(archived);
            store.projects = projects;
        }
        Event::AddWorktree(project_id, worktree) => {
            if let Some(project) = store.projects.iter_mut().find(|p| p.id == project_id) {
                project
                    .client
                    .worktrees
                    .get_or_insert_with(Vec::new)
                    .push(worktree);
            }
        }
        Event::RemoveWorktree(project_id, worktree_id) => {
            if let Some(project) = store.projects.iter_mut().find(|p| p.id == project_id) {
                let worktrees = project.client.worktrees.get_or_insert_with(Vec::new);
                worktrees.retain(|w| w.id != worktree_id);
            }
        }
        Event::SetWorktreeOperationLock(locked) => store.is_worktree_operation_locked = locked,
        Event::AddGroup(name) => {
            let id = ids.next_id();
            store.groups.push(Group {
                id: id.clone(),
                name,
                project_ids: Vec::new(),
                is_collapsed: Some(false),
                color: None,
            });
            out.reply = Some(id.into());
        }
        Event::RemoveGroup(id, delete_projects) => {
            let Some(index) = store.groups.iter().position(|g| g.id == id) else {
                return out;
            };
            let removed = store.groups.remove(index);
            // Each delete asks the host; a busy project waits for the user.
            if delete_projects {
                fx.extend(
                    removed
                        .project_ids
                        .into_iter()
                        .map(|project_id| Effect::DeleteProject { project_id }),
                );
            }
        }
        Event::RenameGroup(id, name) => edit_group(store, &id, |g| g.name = name.clone()),
        Event::ToggleGroupCollapse(id) => {
            edit_group(store, &id, |g| {
                g.is_collapsed = Some(g.is_collapsed != Some(true))
            });
        }
        Event::MoveProjectToGroup(project_id, target, index) => {
            for group in &mut store.groups {
                group.project_ids.retain(|id| *id != project_id);
            }
            if let Some(target) = target {
                edit_group(store, &target, |g| {
                    let at = index.map_or(g.project_ids.len(), |i| i.min(g.project_ids.len()));
                    g.project_ids.insert(at, project_id.clone());
                    g.is_collapsed = Some(false);
                });
            }
        }
        Event::ReorderGroups(order) => {
            let mut groups: Vec<Group> = order
                .iter()
                .filter_map(|id| store.groups.iter().rev().find(|g| g.id == *id).cloned())
                .collect();
            groups.extend(
                store
                    .groups
                    .iter()
                    .filter(|g| !order.contains(&g.id))
                    .cloned(),
            );
            store.groups = groups;
        }
        Event::ReorderProjectInGroup(id, project_ids) => {
            edit_group(store, &id, |g| g.project_ids = project_ids.clone());
        }
        Event::UpdateGroup(id, updates) => edit_group(store, &id, |g| {
            if let Some(name) = &updates.name {
                g.name = name.clone();
            }
            if let Some(project_ids) = &updates.project_ids {
                g.project_ids = project_ids.clone();
            }
            if updates.is_collapsed.is_some() {
                g.is_collapsed = updates.is_collapsed;
            }
            if updates.color.is_some() {
                g.color = updates.color.clone();
            }
        }),
        Event::LoadProjectPrefs(record) => {
            model.prefs.load(record);
            out.reply = Some(true.into());
        }
        Event::RememberProjectPrefs(id, fields) => model.prefs.remember(&id, fields),
        Event::PersistProjectPrefs => {
            if let Some((record, signature)) = model
                .prefs
                .persist(&model.store.projects, &model.store.groups)
            {
                fx.push(Effect::WritePrefs { record, signature });
            }
        }
        Event::PrefsWriteFailed(signature) => model.prefs.write_failed(&signature),
        Event::ForgetProjectPrefs(id) => model.prefs.forget(&id),
    }
    out
}

fn edit_group(store: &mut State, id: &str, mut edit: impl FnMut(&mut Group)) {
    store
        .groups
        .iter_mut()
        .filter(|g| g.id == id)
        .for_each(&mut edit);
}

fn set_projects(
    store: &mut State,
    projects: Vec<Project>,
    active: Option<String>,
    groups: Option<Vec<Group>>,
) {
    store.active_project_id =
        active.unwrap_or_else(|| projects.first().map(|p| p.id.clone()).unwrap_or_default());
    store.projects = projects;
    store.groups = groups.unwrap_or_default();
    store.is_loaded = true;
}

/// Device fields apply at once; name, colour and root go to the host.
fn update_project(store: &mut State, id: &str, updates: ProjectUpdates, fx: &mut Vec<Effect>) {
    let client = updates.client;
    if client != ClientFields::default() {
        for project in store.projects.iter_mut().filter(|p| p.id == id) {
            merge(&mut project.client, &client);
        }
    }
    let current = store.projects.iter().find(|p| p.id == id);
    let differs =
        |new: &Option<String>, old: Option<&String>| new.clone().filter(|new| Some(new) != old);
    let root = updates
        .path
        .as_deref()
        .map(crate::js_trim)
        .filter(|root| !root.is_empty())
        .map(str::to_owned);
    let patch = HostPatch {
        name: differs(&updates.name, current.map(|p| &p.name)),
        color: differs(&updates.color, current.map(|p| &p.color)),
        path: differs(&root, current.and_then(|p| p.path.as_ref())),
    };
    if patch != HostPatch::default() {
        fx.push(Effect::UpdateProjectOnHost {
            project_id: id.to_owned(),
            patch,
        });
    }
}

/// `{ ...into, ...from }` over the fields `from` defines.
fn merge(into: &mut ClientFields, from: &ClientFields) {
    let from = from.clone();
    into.git_branch = from.git_branch.or(into.git_branch.take());
    into.default_shell = from.default_shell.or(into.default_shell.take());
    into.env_vars = from.env_vars.or(into.env_vars.take());
    into.worktrees = from.worktrees.or(into.worktrees.take());
    into.symlink_dirs = from.symlink_dirs.or(into.symlink_dirs.take());
    into.is_git_repo = from.is_git_repo.or(into.is_git_repo.take());
    into.icon = from.icon.or(into.icon.take());
}

/// Replace the list with a watch frame, carrying each project's device fields
/// over (from the store, else the prefs) in the order the device last used.
/// Returns the active project afterwards.
fn apply_registry_list(
    model: &mut Model,
    list: ProjectList,
    saved: Option<&str>,
    fx: &mut Vec<Effect>,
) -> String {
    let store = &model.store;
    let order: Vec<String> = if store.is_loaded {
        store.projects.iter().map(|p| p.id.clone()).collect()
    } else {
        model.prefs.layout.order.clone()
    };
    let mut projects: Vec<Project> = list
        .projects
        .into_iter()
        .map(|summary| {
            let carried = match store.projects.iter().find(|p| p.id == summary.id) {
                Some(live) => Some(ClientFields::of(live)),
                None => model.prefs.get(&summary.id).cloned(),
            };
            Project {
                id: summary.id,
                name: summary.name,
                color: summary.color,
                path: summary.path,
                is_active: None,
                is_default: Some(summary.is_default),
                is_archived: Some(summary.is_archived),
                client: carried.unwrap_or_default(),
            }
        })
        .collect();
    let rank = |id: &str| order.iter().rposition(|o| o == id).unwrap_or(order.len());
    projects.sort_by_key(|p| rank(&p.id));

    // A saved selection archived elsewhere is not a place to land.
    let saved_usable = saved.filter(|saved| {
        projects
            .iter()
            .any(|p| p.id == *saved && p.is_archived != Some(true))
    });
    let preferred = if store.is_loaded {
        Some(store.active_project_id.as_str())
    } else {
        saved_usable
    };
    let active = pick_active_project_id(&projects, preferred, list.default_project_id.as_deref());
    // Groups made before the first list stay; the saved ones fill in the rest.
    let groups = if store.is_loaded {
        store.groups.clone()
    } else {
        let saved_groups = model
            .prefs
            .layout
            .groups
            .iter()
            .filter(|g| !store.groups.iter().any(|own| own.id == g.id));
        saved_groups.chain(store.groups.iter()).cloned().collect()
    };
    let was_loaded = store.is_loaded;
    let previous = store.active_project_id.clone();
    set_projects(
        &mut model.store,
        projects,
        Some(active.clone()),
        Some(groups),
    );
    // The project this device was on is gone: leave its chat route.
    if was_loaded && active != previous {
        fx.push(Effect::ClearChatRoute);
    }
    active
}

/// `preferred` when listed, else the host default, else the first project
/// that is not archived, else the first, else `""`.
#[must_use]
pub fn pick_active_project_id(
    projects: &[Project],
    preferred: Option<&str>,
    default: Option<&str>,
) -> String {
    fn listed<'a>(projects: &[Project], id: Option<&'a str>) -> Option<&'a str> {
        id.filter(|id| projects.iter().any(|p| p.id == *id))
    }
    listed(projects, preferred)
        .or_else(|| listed(projects, default))
        .map(str::to_owned)
        .or_else(|| {
            projects
                .iter()
                .find(|p| p.is_archived != Some(true))
                .or(projects.first())
                .map(|p| p.id.clone())
        })
        .unwrap_or_default()
}
