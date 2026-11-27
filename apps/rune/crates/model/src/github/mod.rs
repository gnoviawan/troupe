//! The GitHub cache of `stores/github-store.ts`: per working copy its `gh` context and list
//! filter, and per key the last list, item detail, checks, check log and repository meta the
//! host returned. A resource keeps its last good data while a refresh is in flight or after
//! one fails, so a rate-limited refresh never blanks what the user reads.
//!
//! Each action is a task: `step` starts it and returns [`Effect::Call`]s, the app sends each
//! call to its `/github/*` route and answers with [`Event::Reply`]. A task without a parent
//! ends with [`Effect::Settled`], carrying the write's `IpcResult` (`None` for a read).

mod types;

use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

pub use types::*;

/// `\u0000`: the separator of the store's keys.
const SEP: &str = "\u{0}";

/// Failure codes that mean the cached context is stale: `gh` vanished, logged out, or the
/// remote changed (`CONTEXT_ERROR_CODES`).
const CONTEXT_ERROR_CODES: [&str; 3] = ["GH_CLI_MISSING", "GH_UNAUTHENTICATED", "GH_NOT_GITHUB_REPO"];

/// The code a reply gets when its data is not the shape the route promises.
pub const BAD_REPLY: &str = "BAD_REPLY";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    #[default]
    Idle,
    Loading,
    Error,
}

/// `GithubResource<T>`: one host read.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Resource<T> {
    pub data: Option<T>,
    pub status: Status,
    pub error_code: Option<String>,
    pub error: Option<String>,
}

impl<T> Default for Resource<T> {
    fn default() -> Self {
        Resource { data: None, status: Status::Idle, error_code: None, error: None }
    }
}

/// The `{ success: false }` half of an `IpcResult`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Failure {
    pub code: String,
    pub error: String,
}

/// An `IpcResult`: the route's data, or its failure.
pub type Reply = Result<Value, Failure>;

/// The `IpcResult` JSON the TypeScript store returns from a write.
#[must_use]
pub fn ipc_json(reply: &Reply) -> Value {
    match reply {
        Ok(data) => json!({ "success": true, "data": data }),
        Err(f) => json!({ "success": false, "code": f.code, "error": f.error }),
    }
}

/// The store's data, as the recorder snapshots it.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct State {
    pub contexts: BTreeMap<String, Resource<Context>>,
    pub lists: BTreeMap<String, Resource<ListResult>>,
    pub items: BTreeMap<String, Resource<ItemDetail>>,
    pub checks: BTreeMap<String, Resource<ChecksResult>>,
    pub logs: BTreeMap<String, Resource<CheckLog>>,
    pub repo_meta: BTreeMap<String, Resource<RepoMeta>>,
    /// The list filter per working copy; absent means [`ListFilter::default`].
    pub filters: BTreeMap<String, ListFilter>,
    #[serde(skip)]
    book: Book,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Slice {
    Contexts,
    Lists,
    Items,
    Checks,
    Logs,
    RepoMeta,
}

impl Slice {
    fn name(self) -> &'static str {
        match self {
            Slice::Contexts => "contexts",
            Slice::Lists => "lists",
            Slice::Items => "items",
            Slice::Checks => "checks",
            Slice::Logs => "logs",
            Slice::RepoMeta => "repoMeta",
        }
    }
}

/// Request tokens and running tasks: what the store keeps outside its state.
#[derive(Clone, Debug, Default)]
struct Book {
    /// Latest request token per slice and key (`requestVersions`).
    versions: HashMap<(Slice, String), u64>,
    tasks: BTreeMap<TaskId, Task>,
    next_task: TaskId,
}

pub type TaskId = u64;

#[derive(Clone, Debug)]
enum Task {
    /// `track`: one read, waiting on its reply.
    Track { slice: Slice, key: String, token: u64, recheck: Option<String>, parent: Option<TaskId> },
    /// A write, then the re-reads it waits on.
    Write { write: Write, result: Option<Reply>, waiting: u32 },
}

#[derive(Clone, Debug)]
enum Write {
    Merge(ItemRef),
    Comment(ItemRef, CommentTarget),
    CreateLabel(String, RepoSlot),
    /// `editItem` and `setItemState`: both re-read the list and the item.
    Edit(ItemRef),
}

/// The store's actions, and the host's answers to the calls they made.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Event {
    LoadContext(String),
    SetFilter(String, FilterPatch),
    LoadList(String, #[serde(default)] Option<LoadOptions>),
    LoadItem(ItemRef, #[serde(default)] Option<LoadOptions>),
    LoadChecks(ItemRef, #[serde(default)] Option<LoadOptions>),
    LoadCheckLog(String, RepoSlot, u64),
    Merge(ItemRef, MergeAction),
    Comment(ItemRef, CommentTarget, String),
    LoadRepoMeta(String, RepoSlot, #[serde(default)] Option<LoadOptions>),
    CreateLabel(String, RepoSlot, NewLabel),
    EditItem(ItemRef, ItemEdit),
    SetItemState(ItemRef, ItemStateAction),
    /// The host answered a task's [`Call`].
    #[serde(skip)]
    Reply(TaskId, Reply),
}

/// One `githubApi` call: its method, its route and the JSON request both carry.
#[derive(Clone, Debug, PartialEq)]
pub struct Call {
    pub op: &'static str,
    pub path: &'static str,
    pub request: Value,
}

impl Call {
    /// The `/github/*` route `webServerApi` posts a `githubApi` method to.
    #[must_use]
    pub fn path_of(op: &str) -> Option<&'static str> {
        Some(match op {
            "getContext" => "/github/context",
            "listItems" => "/github/items",
            "getItem" => "/github/item",
            "getChecks" => "/github/checks",
            "getCheckLog" => "/github/check-log",
            "merge" => "/github/merge",
            "comment" => "/github/comment",
            "getRepoMeta" => "/github/repo-meta",
            "createLabel" => "/github/create-label",
            "editItem" => "/github/edit-item",
            "setItemState" => "/github/item-state",
            _ => return None,
        })
    }

    /// The `githubApi` argument: `getContext` takes the bare cwd, every other method its
    /// request.
    #[must_use]
    pub fn arg(&self) -> Value {
        if self.op == "getContext" { self.request["cwd"].clone() } else { self.request.clone() }
    }

    /// The request body a `githubApi` argument posts (the inverse of [`Call::arg`]).
    #[must_use]
    pub fn body_of(op: &str, arg: Value) -> Value {
        if op == "getContext" { json!({ "cwd": arg }) } else { arg }
    }

    fn new(op: &'static str, request: Value) -> Call {
        Call { op, path: Call::path_of(op).unwrap_or_default(), request }
    }
}

/// What a step asks the app to do, in order.
#[derive(Clone, Debug, PartialEq)]
pub enum Effect {
    /// POST `call.request` to `call.path` and answer with [`Event::Reply`] for `task`.
    Call { task: TaskId, call: Call },
    /// A reply the model could not read.
    Log { message: String },
    /// A task with no parent ended: a write with its `IpcResult`, a read with `None`.
    Settled { task: TaskId, outcome: Option<Reply> },
}

/// What one step did: the task it started, if any, and the effects.
#[derive(Debug, Default)]
pub struct Step {
    pub task: Option<TaskId>,
    pub effects: Vec<Effect>,
}

fn with_refresh(mut request: Value, options: Option<LoadOptions>) -> Value {
    if let Some(refresh) = options.and_then(|o| o.refresh) {
        request["refresh"] = json!(refresh);
    }
    request
}

fn refresh() -> Option<LoadOptions> {
    Some(LoadOptions { refresh: Some(true) })
}

/// `githubListKey`.
#[must_use]
pub fn list_key(cwd: &str, filter: &ListFilter) -> String {
    let assignee = match &filter.assignee {
        AssigneeFilter::User { login } => format!("user:{login}"),
        AssigneeFilter::Anyone => "anyone".into(),
        AssigneeFilter::Me => "me".into(),
        AssigneeFilter::Unassigned => "unassigned".into(),
    };
    [
        cwd.to_owned(),
        filter.repo.as_str().to_owned(),
        word(&filter.kind),
        word(&filter.state),
        crate::js_trim(&filter.search).to_owned(),
        word(&filter.label_mode),
        serde_json::to_string(&filter.labels).unwrap_or_default(),
        assignee,
        if filter.review_requested { "review".into() } else { String::new() },
        word(&filter.sort),
    ]
    .join(SEP)
}

/// The serde name of a unit enum value (`"open"`).
fn word<T: Serialize>(value: &T) -> String {
    serde_json::to_value(value).ok().and_then(|v| v.as_str().map(str::to_owned)).unwrap_or_default()
}

/// `githubRepoMetaKey`.
#[must_use]
pub fn repo_meta_key(cwd: &str, repo: RepoSlot) -> String {
    [cwd, repo.as_str()].join(SEP)
}

/// `githubItemKey`.
#[must_use]
pub fn item_key(cwd: &str, repo: RepoSlot, number: u64) -> String {
    [cwd, repo.as_str(), &number.to_string()].join(SEP)
}

/// `githubLogKey`.
#[must_use]
pub fn log_key(cwd: &str, repo: RepoSlot, job_id: u64) -> String {
    [cwd, repo.as_str(), "job", &job_id.to_string()].join(SEP)
}

impl State {
    /// `selectGithubFilter`.
    #[must_use]
    pub fn filter(&self, cwd: &str) -> ListFilter {
        self.filters.get(cwd).cloned().unwrap_or_default()
    }

    #[must_use]
    pub fn context(&self, cwd: &str) -> Resource<Context> {
        self.contexts.get(cwd).cloned().unwrap_or_default()
    }

    /// The list resource the working copy's filter names.
    #[must_use]
    pub fn list(&self, cwd: &str) -> Resource<ListResult> {
        self.lists.get(&list_key(cwd, &self.filter(cwd))).cloned().unwrap_or_default()
    }

    #[must_use]
    pub fn item(&self, at: &ItemRef) -> Option<&Resource<ItemDetail>> {
        self.items.get(&item_key(&at.cwd, at.repo, at.number))
    }

    #[must_use]
    pub fn item_checks(&self, at: &ItemRef) -> Option<&Resource<ChecksResult>> {
        self.checks.get(&item_key(&at.cwd, at.repo, at.number))
    }

    #[must_use]
    pub fn log(&self, cwd: &str, repo: RepoSlot, job_id: u64) -> Option<&Resource<CheckLog>> {
        self.logs.get(&log_key(cwd, repo, job_id))
    }

    #[must_use]
    pub fn meta(&self, cwd: &str, repo: RepoSlot) -> Option<&Resource<RepoMeta>> {
        self.repo_meta.get(&repo_meta_key(cwd, repo))
    }

    /// Whether a task is still waiting on the host.
    #[must_use]
    pub fn running(&self, task: TaskId) -> bool {
        self.book.tasks.contains_key(&task)
    }
}

/// Applies one event.
pub fn step(state: &mut State, event: Event) -> Step {
    let mut cx = Cx { state, effects: Vec::new() };
    let task = match event {
        Event::LoadContext(cwd) => Some(cx.load_context(&cwd, None)),
        Event::SetFilter(cwd, patch) => {
            let mut filter = cx.state.filter(&cwd);
            filter.apply(patch);
            cx.state.filters.insert(cwd.clone(), filter);
            cx.load_list(&cwd, None, None);
            None
        }
        Event::LoadList(cwd, options) => Some(cx.load_list(&cwd, options, None)),
        Event::LoadItem(at, options) => Some(cx.load_item(&at, options, None)),
        Event::LoadChecks(at, options) => Some(cx.load_checks(&at, options, None)),
        Event::LoadCheckLog(cwd, repo, job_id) => {
            let request = json!({ "cwd": cwd, "repo": repo, "jobId": job_id });
            let call = Call::new("getCheckLog", request);
            Some(cx.track(Slice::Logs, log_key(&cwd, repo, job_id), call, None, None))
        }
        Event::LoadRepoMeta(cwd, repo, options) => Some(cx.load_meta(&cwd, repo, options, None)),
        Event::Merge(at, action) => {
            let request = json!({ "cwd": at.cwd, "repo": at.repo, "number": at.number, "action": action });
            Some(cx.write(Write::Merge(at), Call::new("merge", request)))
        }
        Event::Comment(at, target, body) => {
            let request = json!({
                "cwd": at.cwd, "repo": at.repo, "kind": at.kind, "number": at.number,
                "target": target, "body": body,
            });
            Some(cx.write(Write::Comment(at, target), Call::new("comment", request)))
        }
        Event::CreateLabel(cwd, repo, label) => {
            let request = json!({
                "cwd": cwd, "repo": repo,
                "name": label.name, "color": label.color, "description": label.description,
            });
            let call = Call::new("createLabel", request);
            Some(cx.write(Write::CreateLabel(cwd, repo), call))
        }
        Event::EditItem(at, edit) => {
            let mut request = json!({ "cwd": at.cwd, "repo": at.repo, "kind": at.kind, "number": at.number });
            if let (Some(fields), Value::Object(edit)) = (request.as_object_mut(), json!(edit)) {
                fields.extend(edit);
            }
            Some(cx.write(Write::Edit(at), Call::new("editItem", request)))
        }
        Event::SetItemState(at, action) => {
            let request = json!({
                "cwd": at.cwd, "repo": at.repo, "kind": at.kind, "number": at.number, "action": action,
            });
            Some(cx.write(Write::Edit(at), Call::new("setItemState", request)))
        }
        Event::Reply(task, reply) => {
            cx.reply(task, reply);
            None
        }
    };
    Step { task, effects: cx.effects }
}

struct Cx<'a> {
    state: &'a mut State,
    effects: Vec<Effect>,
}

/// Runs `$body` with `$map` bound to the slice's map, whatever its data type.
macro_rules! with_slice {
    ($state:expr, $slice:expr, |$map:ident| $body:expr) => {
        match $slice {
            Slice::Contexts => { let $map = &mut $state.contexts; $body }
            Slice::Lists => { let $map = &mut $state.lists; $body }
            Slice::Items => { let $map = &mut $state.items; $body }
            Slice::Checks => { let $map = &mut $state.checks; $body }
            Slice::Logs => { let $map = &mut $state.logs; $body }
            Slice::RepoMeta => { let $map = &mut $state.repo_meta; $body }
        }
    };
}

/// Where a reply's data broke its shape, without serde's text, which can quote the data
/// (a comment body).
fn shape_error(e: &serde_json::Error) -> String {
    format!("{:?} error at line {} column {}", e.classify(), e.line(), e.column())
}

/// Stores a reply in one resource: the data on success, else the failure over the last data.
/// `Err` names a reply whose data is not `T`.
fn settle_resource<T: DeserializeOwned>(
    map: &mut BTreeMap<String, Resource<T>>,
    key: &str,
    reply: &Reply,
) -> Result<(), String> {
    let resource = map.entry(key.to_owned()).or_default();
    let failure = match reply {
        Ok(data) => match serde_json::from_value::<T>(data.clone()) {
            Ok(data) => {
                *resource = Resource { data: Some(data), status: Status::Idle, error_code: None, error: None };
                return Ok(());
            }
            Err(e) => Failure { code: BAD_REPLY.into(), error: shape_error(&e) },
        },
        Err(failure) => failure.clone(),
    };
    resource.status = Status::Error;
    resource.error_code = Some(failure.code.clone());
    resource.error = Some(failure.error.clone());
    if failure.code == BAD_REPLY { Err(failure.error) } else { Ok(()) }
}

impl Cx<'_> {
    fn add(&mut self, task: Task) -> TaskId {
        let book = &mut self.state.book;
        book.next_task += 1;
        book.tasks.insert(book.next_task, task);
        book.next_task
    }

    /// `track`: mark the resource loading and send the read.
    fn track(&mut self, slice: Slice, key: String, call: Call, recheck: Option<String>, parent: Option<TaskId>) -> TaskId {
        let version = self.state.book.versions.entry((slice, key.clone())).or_default();
        *version += 1;
        let token = *version;
        with_slice!(self.state, slice, |map| map.entry(key.clone()).or_default().status = Status::Loading);
        let id = self.add(Task::Track { slice, key, token, recheck, parent });
        self.effects.push(Effect::Call { task: id, call });
        id
    }

    fn load_context(&mut self, cwd: &str, parent: Option<TaskId>) -> TaskId {
        let call = Call::new("getContext", json!({ "cwd": cwd }));
        self.track(Slice::Contexts, cwd.to_owned(), call, None, parent)
    }

    fn load_list(&mut self, cwd: &str, options: Option<LoadOptions>, parent: Option<TaskId>) -> TaskId {
        let filter = self.state.filter(cwd);
        let key = list_key(cwd, &filter);
        let request = with_refresh(json!({ "cwd": cwd, "filter": filter }), options);
        let call = Call::new("listItems", request);
        self.track(Slice::Lists, key, call, Some(cwd.to_owned()), parent)
    }

    fn load_item(&mut self, at: &ItemRef, options: Option<LoadOptions>, parent: Option<TaskId>) -> TaskId {
        let request = with_refresh(json!({ "cwd": at.cwd, "repo": at.repo, "kind": at.kind, "number": at.number }), options);
        let call = Call::new("getItem", request);
        self.track(Slice::Items, item_key(&at.cwd, at.repo, at.number), call, Some(at.cwd.clone()), parent)
    }

    fn load_checks(&mut self, at: &ItemRef, options: Option<LoadOptions>, parent: Option<TaskId>) -> TaskId {
        let request = with_refresh(json!({ "cwd": at.cwd, "repo": at.repo, "number": at.number }), options);
        let call = Call::new("getChecks", request);
        self.track(Slice::Checks, item_key(&at.cwd, at.repo, at.number), call, None, parent)
    }

    fn load_meta(&mut self, cwd: &str, repo: RepoSlot, options: Option<LoadOptions>, parent: Option<TaskId>) -> TaskId {
        let request = with_refresh(json!({ "cwd": cwd, "repo": repo }), options);
        let call = Call::new("getRepoMeta", request);
        self.track(Slice::RepoMeta, repo_meta_key(cwd, repo), call, Some(cwd.to_owned()), parent)
    }

    fn write(&mut self, write: Write, call: Call) -> TaskId {
        let id = self.add(Task::Write { write, result: None, waiting: 0 });
        self.effects.push(Effect::Call { task: id, call });
        id
    }

    fn reply(&mut self, id: TaskId, reply: Reply) {
        let Some(task) = self.state.book.tasks.get(&id).cloned() else {
            return;
        };
        match task {
            Task::Track { slice, key, token, recheck, parent } => {
                self.state.book.tasks.remove(&id);
                let current = self.state.book.versions.get(&(slice, key.clone())) == Some(&token);
                if current {
                    if let Err(problem) = with_slice!(self.state, slice, |map| settle_resource(map, &key, &reply)) {
                        self.effects.push(Effect::Log { message: format!("github {} reply unreadable: {problem}", slice.name()) });
                    }
                    if let (Some(cwd), Err(failure)) = (recheck, &reply)
                        && CONTEXT_ERROR_CODES.contains(&failure.code.as_str())
                    {
                        self.load_context(&cwd, None);
                    }
                }
                match parent {
                    Some(parent) => self.child_done(parent),
                    None => self.effects.push(Effect::Settled { task: id, outcome: None }),
                }
            }
            Task::Write { write, .. } => self.written(id, write, reply),
        }
    }

    /// The write's reply came: start the re-reads it waits on, or end.
    fn written(&mut self, id: TaskId, write: Write, reply: Reply) {
        let waiting = match &write {
            Write::Merge(at) => {
                // A refused merge can still mean GitHub's state moved: re-read either way.
                self.load_checks(at, refresh(), None);
                self.load_list(&at.cwd, refresh(), None);
                self.load_item(at, refresh(), Some(id));
                1
            }
            Write::Comment(at, target) => {
                if let Ok(data) = &reply {
                    self.show_comment(at, target, data);
                    self.load_item(at, refresh(), None);
                }
                0
            }
            Write::CreateLabel(cwd, repo) => {
                self.load_meta(cwd, *repo, refresh(), Some(id));
                1
            }
            Write::Edit(at) => {
                // A failed edit may have applied part of the change: show what GitHub reports.
                self.load_list(&at.cwd, refresh(), Some(id));
                self.load_item(at, refresh(), Some(id));
                2
            }
        };
        if waiting == 0 {
            self.state.book.tasks.remove(&id);
            self.effects.push(Effect::Settled { task: id, outcome: Some(reply) });
        } else if let Some(Task::Write { result, waiting: left, .. }) = self.state.book.tasks.get_mut(&id) {
            *result = Some(reply);
            *left = waiting;
        }
    }

    fn child_done(&mut self, parent: TaskId) {
        let Some(Task::Write { result, waiting, .. }) = self.state.book.tasks.get_mut(&parent) else {
            return;
        };
        *waiting -= 1;
        if *waiting == 0 {
            let outcome = result.take();
            self.state.book.tasks.remove(&parent);
            self.effects.push(Effect::Settled { task: parent, outcome });
        }
    }

    /// Shows the comment GitHub returned before the re-read lands.
    fn show_comment(&mut self, at: &ItemRef, target: &CommentTarget, data: &Value) {
        let Ok(comment) = serde_json::from_value::<Comment>(data["comment"].clone()) else {
            self.effects.push(Effect::Log { message: "github comment reply unreadable".into() });
            return;
        };
        let key = item_key(&at.cwd, at.repo, at.number);
        let Some(detail) = self.state.items.get_mut(&key).and_then(|r| r.data.as_mut()) else {
            return;
        };
        match target {
            CommentTarget::Conversation => detail.comments.push(comment),
            CommentTarget::ReviewThread { thread_id } => {
                if let Some(thread) = detail.pr.as_mut().and_then(|pr| pr.review_threads.iter_mut().find(|t| &t.id == thread_id)) {
                    thread.comments.push(comment);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
