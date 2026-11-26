//! The GitHub driver: runs `model::github`'s calls on the server's `/github/*` routes, loads
//! the active project's context and list (`GithubPanel`'s effects and polls), reads the item
//! tab on screen (`GithubItemPanel`'s), and holds the area's own state: the search draft, the
//! open picker's draft, and per item tab its section, merge method, comment drafts and busy
//! flags. What each write does when it settles follows the React components' toasts.

use std::collections::{HashMap, HashSet};

use client::http::{HttpError, HttpRequest, Method};
use model::github::{
    self, Context, Effect, Event, Failure, ItemRef, ItemStateAction, LoadOptions, MergeAction, MergeMethod, Reply,
    RepoResolution, RepoSlot, TaskId,
};
use rune::TextEdit;
use ui::overlay::ToastKind;

use crate::State;

/// `LIST_POLL_VISIBLE_MS`, in seconds.
const LIST_POLL_SECS: f64 = 120.0;
/// `STATUS_POLL_MS`: while signed out or without `gh`.
const STATUS_POLL_SECS: f64 = 15.0;
/// `ITEM_POLL_MS`.
const ITEM_POLL_SECS: f64 = 120.0;
/// `PENDING_CHECKS_POLL_MS`.
const CHECKS_POLL_SECS: f64 = 30.0;
/// `SEARCH_DEBOUNCE_MS`.
const SEARCH_DEBOUNCE_SECS: f64 = 0.35;

pub const REPO_SELECT: &str = "github-repo";
pub const STATE_MENU: &str = "github-state-menu";
pub const ASSIGNEE_MENU: &str = "github-assignee-menu";
pub const SORT_MENU: &str = "github-sort-menu";
pub const PICKER: &str = "github-picker";
pub const ITEM_MENU: &str = "github-item-menu";
pub const STATE_BUTTON_MENU: &str = "github-state-button-menu";
pub const MERGE_METHOD: &str = "github-merge-method";
pub const MERGE_CONFIRM: &str = "github-merge-confirm";
/// The phone's GitHub sheet (`MobileGithubSection` in the drawer).
pub const GITHUB_SHEET: &str = "github-sheet";

/// `GithubBlocker`: why the list cannot show, in the order the user must fix it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Blocker {
    CliMissing,
    NotGithub,
    Unauthenticated { host: String, login_command: String },
}

/// `githubBlocker`.
pub fn blocker(context: &Context) -> Option<Blocker> {
    use model::github::AuthStatus as A;
    match (&context.auth, &context.repo) {
        (A::CliMissing, _) => Some(Blocker::CliMissing),
        (_, RepoResolution::NotGithub) => Some(Blocker::NotGithub),
        (A::Unauthenticated { host, login_command }, _) => {
            Some(Blocker::Unauthenticated { host: host.clone(), login_command: login_command.clone() })
        }
        _ => None,
    }
}

/// Which section of an item tab shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Section {
    #[default]
    Conversation,
    Checks,
    Threads,
    Linked,
}

/// A comment box's draft: the text, and the post in flight or its refusal.
#[derive(Clone, Debug, Default)]
pub struct Draft {
    pub body: TextEdit,
    pub posting: bool,
    pub error: Option<String>,
}

/// One item tab's own state.
#[derive(Clone, Debug, Default)]
pub struct ItemUi {
    pub section: Section,
    pub method: MergeMethod,
    /// A merge action is out.
    pub acting: bool,
    /// A close, reopen or self-assign is out.
    pub busy: bool,
    pub comment: Draft,
    /// Per review thread: the reply box when it is open.
    pub replies: HashMap<String, Draft>,
    /// Bot comments the user unfolded.
    pub shown: HashSet<String>,
    /// Review threads whose fold the user flipped from its default.
    pub flipped: HashSet<String>,
    pub all_checks: bool,
    /// Actions jobs whose log tail is open.
    pub logs: HashSet<u64>,
    /// When the item and its pending checks were last read (seconds).
    read: Option<f64>,
    checks_read: Option<f64>,
}

/// What a picker edits.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PickerFor {
    /// The list's label filter for a working copy.
    LabelFilter(String),
    Labels(ItemRef),
    Assignees(ItemRef),
}

/// `GithubPicker`'s draft: the selection is a draft until Done.
#[derive(Clone, Debug)]
pub struct Picker {
    pub of: PickerFor,
    pub selected: Vec<String>,
    /// What the picker opened with: Done applies the difference against it.
    pub opened: Vec<String>,
    pub query: TextEdit,
    pub mode: model::github::LabelMode,
    pub applying: bool,
    /// The create-label form, when open: its name, colour and description.
    pub create: Option<(TextEdit, &'static str, TextEdit)>,
    pub creating: bool,
}

/// What to do when a write's task settles.
#[derive(Clone, Debug)]
enum Then {
    Merge { key: String, action: MergeAction },
    Comment { key: String, thread: Option<String> },
    /// A picker's Done: close the picker on success, else toast `what`.
    Edit { key: String, what: &'static str },
    /// A close, reopen or self-assign.
    Busy { key: String, what: String },
    CreateLabel,
}

#[derive(Default)]
pub struct GithubUi {
    pub cache: github::State,
    then: HashMap<TaskId, Then>,
    /// Calls made with no server link (a headless run or a test), oldest first.
    pub(crate) unsent: Vec<(TaskId, HttpRequest)>,
    /// The working copy the panel last read the context of, and when (seconds).
    context_read: Option<(String, f64)>,
    /// The list the panel last read (working copy and repository), and when.
    list_read: Option<(String, String, f64)>,
    pub search: TextEdit,
    /// The search text last pushed to the filter, its working copy, and when the draft
    /// last changed.
    search_sent: (String, String),
    search_edited: Option<f64>,
    pub items: HashMap<String, ItemUi>,
    pub picker: Option<Picker>,
}

fn failure(error: HttpError) -> Failure {
    match error {
        HttpError::Refused { code, message, .. } => Failure { code, error: message },
        HttpError::Network(detail) => Failure { code: "NETWORK_ERROR".into(), error: detail },
        HttpError::InProgress => Failure { code: "COMMAND_IN_PROGRESS".into(), error: "the command is still running".into() },
    }
}

impl State {
    /// The working copy the GitHub list shows: the active project's folder.
    pub fn github_cwd(&self) -> Option<String> {
        self.git_cwd()
    }

    pub(crate) fn github_step(&mut self, event: Event) -> Option<TaskId> {
        let step = github::step(&mut self.github.cache, event);
        self.run_github(step.effects);
        step.task
    }

    fn github_write(&mut self, event: Event, then: Then) {
        let step = github::step(&mut self.github.cache, event);
        if let Some(task) = step.task {
            self.github.then.insert(task, then);
        }
        self.run_github(step.effects);
    }

    fn run_github(&mut self, effects: Vec<Effect>) {
        for effect in effects {
            match effect {
                Effect::Call { task, call } => {
                    let request = HttpRequest { method: Method::Post, path: call.path.to_owned(), body: Some(call.request), command_id: None };
                    match &mut self.host {
                        Some(host) => host.request(request, crate::host::Fetch::Github(task)),
                        None => self.github.unsent.push((task, request)),
                    }
                }
                Effect::Log { message } => log::warn!("[github] {message}"),
                Effect::Settled { task, outcome } => self.github_settled(task, outcome),
            }
        }
    }

    /// The server answered a GitHub call. Bodies and tokens are never logged.
    pub(crate) fn github_replied(&mut self, task: TaskId, result: Result<serde_json::Value, HttpError>) {
        let reply = result.map_err(failure);
        if let Err(f) = &reply {
            log::warn!("[github] call failed: task={task} code={}", f.code);
        }
        self.github_step(Event::Reply(task, reply));
    }

    fn github_settled(&mut self, task: TaskId, outcome: Option<Reply>) {
        let (Some(then), Some(reply)) = (self.github.then.remove(&task), outcome) else {
            return;
        };
        match then {
            Then::Merge { key, action } => {
                if let Some(item) = self.github.items.get_mut(&key) {
                    item.acting = false;
                }
                match reply {
                    Ok(_) => {
                        let done = match action {
                            MergeAction::Merge { .. } => "Merge requested on GitHub",
                            MergeAction::EnableAutoMerge { .. } => "Auto-merge enabled",
                            MergeAction::DisableAutoMerge => "Auto-merge disabled",
                        };
                        self.toasts.push(self.now, ToastKind::Success, done, None);
                    }
                    Err(f) => {
                        let title = if f.code == "GH_NOT_MERGEABLE" { "GitHub refused the merge" } else { "Merge action failed" };
                        self.toast_error(title, &f.error);
                    }
                }
            }
            Then::Comment { key, thread } => {
                let Some(item) = self.github.items.get_mut(&key) else {
                    return;
                };
                match thread {
                    Some(thread) => match reply {
                        Ok(_) => {
                            item.replies.remove(&thread);
                        }
                        Err(f) => {
                            if let Some(draft) = item.replies.get_mut(&thread) {
                                draft.posting = false;
                                draft.error = Some(refusal(&f));
                            }
                        }
                    },
                    None => {
                        item.comment.posting = false;
                        match reply {
                            Ok(_) => item.comment.body = TextEdit::default(),
                            Err(f) => item.comment.error = Some(refusal(&f)),
                        }
                    }
                }
            }
            Then::Edit { key, what } => {
                let still_open = self.github.picker.as_ref().is_some_and(|p| picker_key(&p.of) == key);
                match reply {
                    Ok(_) if still_open => {
                        self.github.picker = None;
                        self.overlays.close(PICKER);
                    }
                    Ok(_) => {}
                    Err(f) => {
                        if let Some(picker) = self.github.picker.as_mut() {
                            picker.applying = false;
                        }
                        self.report_write_failure(what, &f);
                    }
                }
            }
            Then::Busy { key, what } => {
                if let Some(item) = self.github.items.get_mut(&key) {
                    item.busy = false;
                }
                if let Err(f) = reply {
                    self.report_write_failure(&what, &f);
                }
            }
            Then::CreateLabel => {
                let Some(picker) = self.github.picker.as_mut() else {
                    return;
                };
                picker.creating = false;
                match reply {
                    Ok(data) => {
                        if let Some(name) = data["label"]["name"].as_str() {
                            if !picker.selected.iter().any(|s| s == name) {
                                picker.selected.push(name.to_owned());
                            }
                            picker.query = TextEdit::default();
                            picker.create = None;
                        }
                    }
                    Err(f) => self.report_write_failure("create the label", &f),
                }
            }
        }
    }

    /// `reportGithubWriteFailure`.
    fn report_write_failure(&mut self, what: &str, f: &Failure) {
        log::warn!("[github.itemActions] {what} failed: {}", f.code);
        if f.code == "GH_TIMEOUT" {
            self.toast_error("GitHub didn't answer in time. Try again.", "");
        } else {
            self.toast_error(&format!("Couldn't {what}"), &f.error);
        }
    }

    /// Each frame: the panel's context and list reads and polls, the search debounce, and
    /// the visible item tab's reads. Returns the next time it wants to run.
    pub(crate) fn github_tick(&mut self) -> Option<f64> {
        if self.status != client::link::Status::Live {
            return None;
        }
        let mut next = Vec::new();
        next.extend(self.github_search_due());
        if self.github_panel_shown()
            && let Some(cwd) = self.github_cwd()
        {
            next.extend(self.github_panel_tick(&cwd));
        }
        if let Some(at) = self.github_visible_item() {
            next.extend(self.github_item_tick(&at));
        }
        next.into_iter().reduce(f64::min)
    }

    /// The GitHub view is on screen: the desktop side panel or the phone sheet.
    fn github_panel_shown(&self) -> bool {
        let Some(project) = self.active_project() else {
            return false;
        };
        let side = self.layout.record.side_panel_visible && self.layout.tab(&project.id) == model::shell_layout::Tab::Github;
        side || self.overlays.is_open(GITHUB_SHEET)
    }

    fn github_panel_tick(&mut self, cwd: &str) -> Option<f64> {
        let fresh_context = matches!(&self.github.context_read, Some((read, _)) if read == cwd);
        if !fresh_context {
            self.github.context_read = Some((cwd.to_owned(), self.now));
            self.github_step(Event::LoadContext(cwd.to_owned()));
            return None;
        }
        let context = self.github.cache.context(cwd).data?;
        if let Some(blocker) = blocker(&context) {
            if blocker == Blocker::NotGithub {
                return None;
            }
            let at = self.github.context_read.as_ref().map_or(self.now, |(_, at)| *at);
            if self.now - at >= STATUS_POLL_SECS {
                self.github.context_read = Some((cwd.to_owned(), self.now));
                self.github_step(Event::LoadContext(cwd.to_owned()));
            }
            return Some(at + STATUS_POLL_SECS);
        }
        let RepoResolution::Resolved { origin, upstream } = &context.repo else {
            return None;
        };
        let filter = self.github.cache.filter(cwd);
        // A fork's upstream filter left over from another working copy falls back to origin.
        if upstream.is_none() && filter.repo == RepoSlot::Upstream {
            self.github_set_filter(cwd, model::github::FilterPatch { repo: Some(RepoSlot::Origin), ..Default::default() });
            return None;
        }
        let slot = if filter.repo == RepoSlot::Upstream { upstream.as_ref().unwrap_or(origin) } else { origin };
        let repo_id = format!("{}/{}", slot.host, slot.full_name());
        match &self.github.list_read {
            Some((read, id, at)) if read == cwd && *id == repo_id => {
                let at = *at;
                if self.now - at >= LIST_POLL_SECS {
                    self.github.list_read = Some((cwd.to_owned(), repo_id, self.now));
                    self.github_step(Event::LoadList(cwd.to_owned(), None));
                    return Some(self.now + LIST_POLL_SECS);
                }
                Some(at + LIST_POLL_SECS)
            }
            _ => {
                self.github.list_read = Some((cwd.to_owned(), repo_id, self.now));
                self.github_step(Event::LoadList(cwd.to_owned(), None));
                Some(self.now + LIST_POLL_SECS)
            }
        }
    }

    /// `setFilter`: the list reloads through the store.
    pub fn github_set_filter(&mut self, cwd: &str, patch: model::github::FilterPatch) {
        self.github.list_read = self.github.list_read.take().map(|(c, id, _)| (c, id, self.now));
        self.github_step(Event::SetFilter(cwd.to_owned(), patch));
    }

    pub fn github_refresh_list(&mut self) {
        if let Some(cwd) = self.github_cwd() {
            self.github_step(Event::LoadList(cwd, Some(LoadOptions { refresh: Some(true) })));
        }
    }

    pub fn github_retry_context(&mut self) {
        if let Some(cwd) = self.github_cwd() {
            self.github.context_read = Some((cwd.clone(), self.now));
            self.github_step(Event::LoadContext(cwd));
        }
    }

    /// The search field changed: it commits after a pause, or on Enter.
    pub fn github_search_edited(&mut self) {
        self.github.search_edited = Some(self.now);
    }

    pub fn github_search_commit(&mut self) {
        self.github.search_edited = None;
        let Some(cwd) = self.github_cwd() else { return };
        let text = self.github.search.text.clone();
        if self.github.search_sent == (cwd.clone(), text.clone()) {
            return;
        }
        self.github.search_sent = (cwd.clone(), text.clone());
        self.github_set_filter(&cwd, model::github::FilterPatch { search: Some(text), ..Default::default() });
    }

    fn github_search_due(&mut self) -> Option<f64> {
        let cwd = self.github_cwd()?;
        // Another working copy's filter replaces the text (`SearchField` keyed by cwd).
        if self.github.search_sent.0 != cwd {
            let search = self.github.cache.filter(&cwd).search;
            self.github.search = TextEdit::new(&search);
            self.github.search_sent = (cwd, search);
            self.github.search_edited = None;
            return None;
        }
        let edited = self.github.search_edited?;
        let due = edited + SEARCH_DEBOUNCE_SECS;
        if self.now >= due {
            self.github_search_commit();
            return None;
        }
        Some(due)
    }

    /// The GitHub item tab on screen, if any.
    pub(crate) fn github_visible_item(&self) -> Option<ItemRef> {
        self.workspace.record()?;
        let pane = &self.workspace.tree().active_pane_id;
        let leaf = model::workspace::leaves(self.workspace.root()).into_iter().find(|l| &l.id == pane)?;
        let tab = leaf.tabs.iter().find(|tab| Some(tab.id()) == leaf.active_tab_id.as_deref())?;
        item_ref(tab)
    }

    fn github_item_tick(&mut self, at: &ItemRef) -> Option<f64> {
        let key = github::item_key(&at.cwd, at.repo, at.number);
        let now = self.now;
        let item = self.github.items.entry(key).or_default();
        let first = item.read.is_none();
        let due = item.read.is_some_and(|read| now - read >= ITEM_POLL_SECS);
        if first || due {
            item.read = Some(now);
            if first {
                item.checks_read = Some(now);
            }
            self.github_step(Event::LoadItem(at.clone(), None));
            if first {
                if at.kind == model::github::ItemKind::Pr {
                    self.github_step(Event::LoadChecks(at.clone(), None));
                }
                let meta = self.github.cache.meta(&at.cwd, at.repo);
                if meta.is_none_or(|m| m.data.is_none() && m.status != github::Status::Loading) {
                    self.github_step(Event::LoadRepoMeta(at.cwd.clone(), at.repo, None));
                }
            }
        }
        let detail = self.github.cache.item(at).and_then(|r| r.data.clone());
        let checks = self.github.cache.item_checks(at).and_then(|r| r.data.clone());
        // After a push the item reports a new head before the cached checks do.
        let head = detail.as_ref().and_then(|d| d.pr.as_ref()).map(|pr| pr.head_sha.clone());
        let stale = matches!((&head, &checks), (Some(head), Some(c)) if &c.head_sha != head);
        let loading = self.github.cache.item_checks(at).is_some_and(|r| r.status == github::Status::Loading);
        if stale && !loading {
            self.github_step(Event::LoadChecks(at.clone(), Some(LoadOptions { refresh: Some(true) })));
        }
        let rollup = if stale { detail.as_ref().and_then(|d| d.item.checks) } else { checks.and_then(|c| c.rollup).or(detail.and_then(|d| d.item.checks)) };
        let item = self.github.items.get_mut(&github::item_key(&at.cwd, at.repo, at.number))?;
        let mut wake = item.read.map(|r| r + ITEM_POLL_SECS);
        if at.kind == model::github::ItemKind::Pr && rollup == Some(model::github::CheckRollup::Pending) {
            let read = item.checks_read.unwrap_or(now);
            if now - read >= CHECKS_POLL_SECS {
                item.checks_read = Some(now);
                self.github_step(Event::LoadChecks(at.clone(), None));
            }
            wake = wake.map(|w| w.min(read.max(now - CHECKS_POLL_SECS) + CHECKS_POLL_SECS));
        }
        wake
    }

    /// `GithubPanel.openItem`: the item opens as a pane tab.
    pub fn github_open_item(&mut self, cwd: &str, item: &model::github::ListItem) {
        let repo = self.github.cache.filter(cwd).repo;
        let kind = match item.kind {
            model::github::ItemKind::Issue => "issue",
            model::github::ItemKind::Pr => "pr",
        };
        self.workspace.step(model::workspace::Event::AddGithubItemTab(
            model::workspace::GithubItem { cwd: cwd.to_owned(), repo: repo.as_str().to_owned(), kind: kind.to_owned(), number: item.number, title: item.title.clone() },
            None,
        ));
        self.overlays.close(GITHUB_SHEET);
    }

    /// The item tab's Refresh: the item, its checks and the repository meta, past the cache.
    pub fn github_refresh_item(&mut self, at: &ItemRef) {
        let refresh = Some(LoadOptions { refresh: Some(true) });
        self.github_step(Event::LoadItem(at.clone(), refresh));
        if at.kind == model::github::ItemKind::Pr {
            self.github_step(Event::LoadChecks(at.clone(), refresh));
        }
        self.github_step(Event::LoadRepoMeta(at.cwd.clone(), at.repo, refresh));
    }

    pub fn github_item_ui(&mut self, at: &ItemRef) -> &mut ItemUi {
        self.github.items.entry(github::item_key(&at.cwd, at.repo, at.number)).or_default()
    }

    /// The merge confirm's Confirm, or an auto-merge button: the action runs on GitHub.
    pub fn github_merge(&mut self, at: &ItemRef, action: MergeAction) {
        self.overlays.close(MERGE_CONFIRM);
        let key = github::item_key(&at.cwd, at.repo, at.number);
        self.github_item_ui(at).acting = true;
        log::info!("[github] merge action {action:?} on #{}", at.number);
        self.github_write(Event::Merge(at.clone(), action), Then::Merge { key, action });
    }

    /// Posts the conversation comment, or a reply into `thread`. The text clears only once
    /// GitHub accepts it.
    pub fn github_post_comment(&mut self, at: &ItemRef, thread: Option<String>) {
        let key = github::item_key(&at.cwd, at.repo, at.number);
        let item = self.github_item_ui(at);
        let draft = match &thread {
            Some(id) => item.replies.entry(id.clone()).or_default(),
            None => &mut item.comment,
        };
        if draft.posting || draft.body.text.trim().is_empty() {
            return;
        }
        draft.posting = true;
        draft.error = None;
        let body = draft.body.text.clone();
        let target = match &thread {
            Some(id) => model::github::CommentTarget::ReviewThread { thread_id: id.clone() },
            None => model::github::CommentTarget::Conversation,
        };
        self.github_write(Event::Comment(at.clone(), target, body), Then::Comment { key, thread });
    }

    /// Close or reopen; the store re-reads the item and the list after it.
    pub fn github_set_state(&mut self, at: &ItemRef, action: ItemStateAction, label: &str) {
        self.overlays.close(ITEM_MENU);
        self.overlays.close(STATE_BUTTON_MENU);
        let key = github::item_key(&at.cwd, at.repo, at.number);
        self.github_item_ui(at).busy = true;
        self.github_write(Event::SetItemState(at.clone(), action), Then::Busy { key, what: label.to_lowercase() });
    }

    /// Assign or unassign the signed-in account.
    pub fn github_toggle_self(&mut self, at: &ItemRef, viewer: &str, assigned: bool) {
        self.overlays.close(ITEM_MENU);
        let key = github::item_key(&at.cwd, at.repo, at.number);
        self.github_item_ui(at).busy = true;
        let me = Some(vec![viewer.to_owned()]);
        let edit = if assigned {
            model::github::ItemEdit { remove_assignees: me, ..Default::default() }
        } else {
            model::github::ItemEdit { add_assignees: me, ..Default::default() }
        };
        let what = if assigned { "unassign you" } else { "assign you" };
        self.github_write(Event::EditItem(at.clone(), edit), Then::Busy { key, what: what.into() });
    }

    /// Opens a picker from what GitHub reports now: a cancelled draft never comes back.
    pub fn github_open_picker(&mut self, of: PickerFor, initial: Vec<String>) {
        let (cwd, repo, mode) = match &of {
            PickerFor::LabelFilter(cwd) => {
                let filter = self.github.cache.filter(cwd);
                (cwd.clone(), filter.repo, filter.label_mode)
            }
            PickerFor::Labels(at) | PickerFor::Assignees(at) => (at.cwd.clone(), at.repo, Default::default()),
        };
        let meta = self.github.cache.meta(&cwd, repo);
        if meta.is_none_or(|m| m.data.is_none() && m.status != github::Status::Loading) {
            self.github_step(Event::LoadRepoMeta(cwd, repo, None));
        }
        self.overlays.close(ITEM_MENU);
        self.github.picker = Some(Picker {
            of,
            opened: initial.clone(),
            selected: initial,
            query: TextEdit::default(),
            mode,
            applying: false,
            create: None,
            creating: false,
        });
        self.overlays.open(PICKER);
    }

    pub fn github_close_picker(&mut self) {
        self.github.picker = None;
        self.overlays.close(PICKER);
    }

    pub fn github_toggle_pick(&mut self, value: &str) {
        let Some(picker) = self.github.picker.as_mut() else { return };
        // Done already sent its snapshot: a pick now would vanish when the picker closes.
        if picker.applying {
            return;
        }
        if let Some(i) = picker.selected.iter().position(|v| v == value) {
            picker.selected.remove(i);
        } else {
            picker.selected.push(value.to_owned());
        }
    }

    /// The picker's apply button.
    pub fn github_apply_picker(&mut self) {
        let Some(picker) = self.github.picker.as_mut() else { return };
        let added = |next: &[String], prev: &[String]| -> Vec<String> { next.iter().filter(|v| !prev.contains(v)).cloned().collect() };
        let (selected, opened) = (picker.selected.clone(), picker.opened.clone());
        match picker.of.clone() {
            PickerFor::LabelFilter(cwd) => {
                let mode = picker.mode;
                self.github_close_picker();
                self.github_set_filter(&cwd, model::github::FilterPatch { labels: Some(selected), label_mode: Some(mode), ..Default::default() });
            }
            of @ (PickerFor::Labels(_) | PickerFor::Assignees(_)) => {
                let (at, edit, what) = match &of {
                    PickerFor::Labels(at) => (at.clone(), model::github::ItemEdit { add_labels: Some(added(&selected, &opened)), remove_labels: Some(added(&opened, &selected)), ..Default::default() }, "update the labels"),
                    PickerFor::Assignees(at) => (at.clone(), model::github::ItemEdit { add_assignees: Some(added(&selected, &opened)), remove_assignees: Some(added(&opened, &selected)), ..Default::default() }, "update the assignees"),
                    PickerFor::LabelFilter(_) => unreachable!(),
                };
                if edit.is_empty() {
                    self.github_close_picker();
                    return;
                }
                picker.applying = true;
                self.github_write(Event::EditItem(at, edit), Then::Edit { key: picker_key(&of), what });
            }
        }
    }

    /// The create form's Create label.
    pub fn github_create_label(&mut self) {
        let Some(picker) = self.github.picker.as_mut() else { return };
        let (PickerFor::Labels(at), Some((name, color, description))) = (&picker.of, &picker.create) else { return };
        let name = name.text.trim().to_owned();
        if name.is_empty() || picker.creating {
            return;
        }
        let description = description.text.trim();
        let label = model::github::NewLabel { name, color: (*color).to_owned(), description: (!description.is_empty()).then(|| description.to_owned()) };
        picker.creating = true;
        let event = Event::CreateLabel(at.cwd.clone(), at.repo, label);
        self.github_write(event, Then::CreateLabel);
    }
}

fn picker_key(of: &PickerFor) -> String {
    match of {
        PickerFor::LabelFilter(cwd) => cwd.clone(),
        PickerFor::Labels(at) | PickerFor::Assignees(at) => github::item_key(&at.cwd, at.repo, at.number),
    }
}

/// A comment's refusal as the box shows it.
fn refusal(f: &Failure) -> String {
    if f.error.is_empty() { "GitHub didn't accept the comment.".into() } else { f.error.clone() }
}

/// A GitHub item tab's identity, read from the workspace tab.
pub fn item_ref(tab: &model::workspace::Tab) -> Option<ItemRef> {
    let model::workspace::Tab::GithubItem { cwd, repo, kind, number, .. } = tab else {
        return None;
    };
    let repo = if repo == "upstream" { RepoSlot::Upstream } else { RepoSlot::Origin };
    let kind = if kind == "pr" { model::github::ItemKind::Pr } else { model::github::ItemKind::Issue };
    Some(ItemRef { cwd: cwd.clone(), repo, kind, number: *number })
}
