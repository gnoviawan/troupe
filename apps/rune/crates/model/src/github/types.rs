//! The wire shapes of `shared/types/github.types.ts`, as the host's `/github/*` routes send
//! them. Field names and nulls match the TypeScript JSON, so a snapshot of [`super::State`]
//! equals the store's.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RepoSlot {
    #[default]
    Origin,
    Upstream,
}

impl RepoSlot {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            RepoSlot::Origin => "origin",
            RepoSlot::Upstream => "upstream",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepoRef {
    pub host: String,
    pub owner: String,
    pub name: String,
}

impl RepoRef {
    /// `owner/name`.
    #[must_use]
    pub fn full_name(&self) -> String {
        format!("{}/{}", self.owner, self.name)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TokenSource {
    Keyring,
    Env,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", rename_all_fields = "camelCase")]
pub enum AuthStatus {
    CliMissing,
    Unauthenticated {
        host: String,
        login_command: String,
    },
    Connected {
        host: String,
        account: String,
        scopes: Vec<String>,
        token_source: TokenSource,
        missing_scopes: Vec<String>,
        fix_commands: Vec<String>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RepoResolution {
    NotGithub,
    Resolved {
        origin: RepoRef,
        upstream: Option<RepoRef>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Context {
    pub auth: AuthStatus,
    pub repo: RepoResolution,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ItemKind {
    Issue,
    Pr,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ItemState {
    Open,
    Closed,
    Merged,
    Draft,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Label {
    pub name: String,
    /// Six hex digits without `#`.
    pub color: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CheckRollup {
    Success,
    Failure,
    Pending,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListItem {
    pub kind: ItemKind,
    pub number: u64,
    pub title: String,
    pub state: ItemState,
    pub author: String,
    pub labels: Vec<Label>,
    pub assignees: Vec<String>,
    pub comment_count: u64,
    pub updated_at: String,
    pub url: String,
    pub checks: Option<CheckRollup>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum KindFilter {
    #[default]
    All,
    Issue,
    Pr,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StateFilter {
    All,
    #[default]
    Open,
    Closed,
    Merged,
    Draft,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LabelMode {
    #[default]
    Any,
    All,
    Exclude,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum AssigneeFilter {
    #[default]
    Anyone,
    Me,
    Unassigned,
    User { login: String },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Sort {
    #[default]
    Updated,
    Created,
    Comments,
}

/// `GithubListFilter`; [`Default`] is `DEFAULT_GITHUB_FILTER`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListFilter {
    pub repo: RepoSlot,
    pub kind: KindFilter,
    pub state: StateFilter,
    pub search: String,
    pub labels: Vec<String>,
    pub label_mode: LabelMode,
    pub assignee: AssigneeFilter,
    pub review_requested: bool,
    pub sort: Sort,
}

/// `Partial<GithubListFilter>`: the fields `setFilter` replaces.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FilterPatch {
    pub repo: Option<RepoSlot>,
    pub kind: Option<KindFilter>,
    pub state: Option<StateFilter>,
    pub search: Option<String>,
    pub labels: Option<Vec<String>>,
    pub label_mode: Option<LabelMode>,
    pub assignee: Option<AssigneeFilter>,
    pub review_requested: Option<bool>,
    pub sort: Option<Sort>,
}

impl ListFilter {
    /// `{ ...filter, ...patch }`.
    pub fn apply(&mut self, patch: FilterPatch) {
        let FilterPatch { repo, kind, state, search, labels, label_mode, assignee, review_requested, sort } = patch;
        if let Some(v) = repo {
            self.repo = v;
        }
        if let Some(v) = kind {
            self.kind = v;
        }
        if let Some(v) = state {
            self.state = v;
        }
        if let Some(v) = search {
            self.search = v;
        }
        if let Some(v) = labels {
            self.labels = v;
        }
        if let Some(v) = label_mode {
            self.label_mode = v;
        }
        if let Some(v) = assignee {
            self.assignee = v;
        }
        if let Some(v) = review_requested {
            self.review_requested = v;
        }
        if let Some(v) = sort {
            self.sort = v;
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListResult {
    pub repo: RepoRef,
    pub items: Vec<ListItem>,
    pub fetched_at: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Comment {
    pub id: String,
    pub author: String,
    pub is_bot: bool,
    pub body: String,
    pub created_at: String,
    pub url: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewThread {
    pub id: String,
    pub path: String,
    pub line: Option<u64>,
    pub is_resolved: bool,
    pub is_outdated: bool,
    pub comments: Vec<Comment>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mergeable {
    Mergeable,
    Conflicting,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PullDetail {
    pub head_ref_name: String,
    pub head_sha: String,
    pub base_ref_name: String,
    pub is_cross_repository: bool,
    pub maintainer_can_modify: bool,
    pub mergeable: Mergeable,
    /// GitHub `mergeStateStatus` (CLEAN, BLOCKED, BEHIND, DIRTY, ...).
    pub merge_state_status: String,
    pub auto_merge_enabled: bool,
    pub additions: u64,
    pub deletions: u64,
    pub changed_files: u64,
    pub review_threads: Vec<ReviewThread>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CloseReason {
    Completed,
    NotPlanned,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TimelineEventKind {
    Labeled,
    Unlabeled,
    Assigned,
    Unassigned,
    Closed,
    Reopened,
    Merged,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimelineEvent {
    pub id: String,
    pub kind: TimelineEventKind,
    pub actor: String,
    pub created_at: String,
    pub label: Option<Label>,
    pub assignee: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemDetail {
    pub repo: RepoRef,
    pub item: ListItem,
    pub body: String,
    pub created_at: String,
    pub comments: Vec<Comment>,
    pub timeline: Vec<TimelineEvent>,
    pub state_reason: Option<CloseReason>,
    pub pr: Option<PullDetail>,
    pub fetched_at: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CheckStatus {
    Success,
    Failure,
    Pending,
    Skipped,
    Neutral,
    Cancelled,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Check {
    pub name: String,
    pub workflow: Option<String>,
    pub status: CheckStatus,
    pub details_url: Option<String>,
    pub job_id: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChecksResult {
    pub head_sha: String,
    pub rollup: Option<CheckRollup>,
    pub checks: Vec<Check>,
    pub fetched_at: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckLog {
    pub job_id: u64,
    pub tail: String,
    pub truncated: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MergeMethod {
    #[default]
    Merge,
    Squash,
    Rebase,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MergeAction {
    Merge { method: MergeMethod },
    EnableAutoMerge { method: MergeMethod },
    DisableAutoMerge,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", rename_all_fields = "camelCase")]
pub enum CommentTarget {
    Conversation,
    ReviewThread { thread_id: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepoLabel {
    pub name: String,
    pub color: String,
    pub description: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoMeta {
    pub repo: RepoRef,
    pub labels: Vec<RepoLabel>,
    pub assignable_users: Vec<String>,
    pub viewer: String,
    pub fetched_at: String,
}

/// A label to create: `Pick<GithubCreateLabelRequest, 'name' | 'color' | 'description'>`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewLabel {
    pub name: String,
    pub color: String,
    pub description: Option<String>,
}

/// `GithubItemEdit`: labels and assignees to add or remove. An absent list is not sent.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemEdit {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub add_labels: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remove_labels: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub add_assignees: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remove_assignees: Option<Vec<String>>,
}

impl ItemEdit {
    /// Every list is absent or empty: there is nothing to send.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        [&self.add_labels, &self.remove_labels, &self.add_assignees, &self.remove_assignees]
            .iter()
            .all(|list| list.as_ref().is_none_or(Vec::is_empty))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ItemStateAction {
    Close { reason: CloseReason },
    Reopen,
}

/// `GithubItemRef`: one issue or PR of a working copy's repository.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ItemRef {
    pub cwd: String,
    pub repo: RepoSlot,
    pub kind: ItemKind,
    pub number: u64,
}

/// A load's `{ refresh }`: past the host's read cache.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
pub struct LoadOptions {
    #[serde(default)]
    pub refresh: Option<bool>,
}
