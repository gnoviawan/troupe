//! Git history (`stores/git-history-store.ts`, `HistorySection`, `GitHistoryView`): the log
//! per working copy and scope, the HEAD summary the Changes panel lists, commit details, and
//! the commit diff tabs' patches. Reads go to `/git/log`, `/git/commit-detail` and
//! `/git/commit-file-diff`.

use std::collections::HashMap;

use client::http::{HttpError, HttpRequest, Method};
use model::workspace::Tab;
use rune::TextEdit;
use rune_kit::diff::DiffState;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::State;

/// `GIT_LOG_LIMIT`.
pub const LOG_LIMIT: usize = 200;
/// `HEAD_RECENT_LIMIT`: the commits the Changes panel's History section lists.
pub const HEAD_RECENT: usize = 8;
pub const SCOPE_MENU: &str = "git-history-scope";
pub const AUTHOR_MENU: &str = "git-history-author";

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Commit {
    pub hash: String,
    #[serde(default)]
    pub short_hash: String,
    #[serde(default)]
    pub parents: Vec<String>,
    #[serde(default)]
    pub refs: Vec<String>,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub date: String,
    #[serde(default)]
    pub subject: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitFile {
    pub path: String,
    #[serde(default)]
    pub old_path: Option<String>,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub added: Option<u32>,
    #[serde(default)]
    pub removed: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitDetail {
    pub hash: String,
    #[serde(default)]
    pub author_email: String,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub files: Vec<CommitFile>,
    #[serde(default)]
    pub files_truncated: bool,
}

/// One read's state: in flight, answered, or failed with the message shown.
#[derive(Clone, Debug, PartialEq)]
pub enum Read<T> {
    Loading,
    Ready(T),
    Failed(String),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Scope {
    #[default]
    All,
    Head,
}

impl Scope {
    pub fn label(self) -> &'static str {
        match self {
            Scope::All => "All branches",
            Scope::Head => "Current branch",
        }
    }
}

/// One History tab's filter, author pick and whether its detail pane is closed.
#[derive(Clone, Debug, Default)]
pub struct Filters {
    pub query: TextEdit,
    pub author: Option<String>,
    pub detail_hidden: bool,
}

/// What an HTTP read of this module is for.
#[derive(Clone, Debug, PartialEq)]
pub enum HistoryFetch {
    Log { cwd: String, token: u64 },
    Head { cwd: String, token: u64 },
    Detail { cwd: String, hash: String },
    /// A commit diff tab's patch, by tab id.
    FileDiff { tab: String },
}

#[derive(Default)]
pub struct HistoryUi {
    pub log: HashMap<String, Read<Vec<Commit>>>,
    pub scope: HashMap<String, Scope>,
    /// The HEAD summary: the recent commits and how many the log holds.
    pub head: HashMap<String, (Vec<Commit>, usize)>,
    /// The commit-context signature the summary was read at.
    head_read: HashMap<String, String>,
    pub details: HashMap<(String, String), Read<CommitDetail>>,
    pub selected: HashMap<String, String>,
    /// The History tab's filters and detail toggle, per working copy, as each tab's
    /// `GitHistoryView` keeps its own.
    pub filters: HashMap<String, Filters>,
    pub section_closed: bool,
    tokens: HashMap<String, u64>,
    /// Per commit diff tab: the patch read and its view.
    pub diffs: HashMap<String, Read<(String, bool)>>,
    pub diff_views: HashMap<String, DiffState>,
    /// Reads made with no server link (a test), oldest first.
    pub(crate) unsent: Vec<(String, HistoryFetch)>,
}

fn post(path: &str, body: Value) -> HttpRequest {
    HttpRequest { method: Method::Post, path: path.to_owned(), body: Some(body), command_id: None }
}

impl State {
    fn history_send(&mut self, request: HttpRequest, tag: HistoryFetch) {
        match &mut self.host {
            Some(host) => host.request(request, crate::host::Fetch::History(tag)),
            None => self.history.unsent.push((request.path, tag)),
        }
    }

    fn bump(&mut self, key: String) -> u64 {
        let token = self.history.tokens.entry(key).or_default();
        *token += 1;
        *token
    }

    /// `refreshLog`: the log in the working copy's scope.
    pub(crate) fn refresh_log(&mut self, cwd: &str) {
        let token = self.bump(format!("log:{cwd}"));
        let all = self.history.scope.get(cwd).copied().unwrap_or_default() == Scope::All;
        if !matches!(self.history.log.get(cwd), Some(Read::Ready(_))) {
            self.history.log.insert(cwd.to_owned(), Read::Loading);
        }
        let mut body = json!({ "cwd": cwd, "limit": LOG_LIMIT });
        if all {
            body["all"] = json!(true);
        }
        self.history_send(post("/git/log", body), HistoryFetch::Log { cwd: cwd.to_owned(), token });
    }

    pub(crate) fn set_history_scope(&mut self, cwd: &str, scope: Scope) {
        self.close(SCOPE_MENU);
        if self.history.scope.get(cwd).copied().unwrap_or_default() == scope {
            return;
        }
        self.history.scope.insert(cwd.to_owned(), scope);
        self.refresh_log(cwd);
    }

    /// `refreshHeadSummary`, again whenever the commit context changes.
    fn refresh_head(&mut self, cwd: &str) {
        let signature = self.git.cache.commit_context(cwd).map(|c| {
            format!("{:?}\u{0}{}\u{0}{}\u{0}{}\u{0}{}", c.branch, c.has_head, c.last_subject, c.ahead, c.behind)
        });
        let signature = signature.unwrap_or_default();
        if self.history.head_read.get(cwd) == Some(&signature) {
            return;
        }
        self.history.head_read.insert(cwd.to_owned(), signature);
        let token = self.bump(format!("head:{cwd}"));
        let body = json!({ "cwd": cwd, "limit": LOG_LIMIT });
        self.history_send(post("/git/log", body), HistoryFetch::Head { cwd: cwd.to_owned(), token });
    }

    /// `loadCommitDetail`: one read per commit.
    pub(crate) fn load_commit_detail(&mut self, cwd: &str, hash: &str) {
        let key = (cwd.to_owned(), hash.to_owned());
        if matches!(self.history.details.get(&key), Some(Read::Loading | Read::Ready(_))) {
            return;
        }
        self.history.details.insert(key, Read::Loading);
        let body = json!({ "cwd": cwd, "hash": hash });
        self.history_send(post("/git/commit-detail", body), HistoryFetch::Detail { cwd: cwd.to_owned(), hash: hash.to_owned() });
    }

    pub(crate) fn select_commit(&mut self, cwd: &str, hash: &str) {
        self.history.selected.insert(cwd.to_owned(), hash.to_owned());
        self.history.filters.entry(cwd.to_owned()).or_default().detail_hidden = false;
        self.load_commit_detail(cwd, hash);
    }

    /// Opens a commit file's diff tab (`addCommitDiffTab`).
    pub(crate) fn open_commit_file(&mut self, cwd: &str, hash: &str, file: &CommitFile) {
        let event = model::workspace::Event::AddCommitDiffTab(cwd.to_owned(), hash.to_owned(), file.path.clone(), file.old_path.clone(), None);
        self.workspace.step(event);
    }

    /// Each frame: the HEAD summary of the shown working copy, the log of open history
    /// tabs, and the patches of open commit diff tabs.
    pub(crate) fn history_tick(&mut self) {
        if self.status != client::link::Status::Live {
            return;
        }
        if let Some(cwd) = self.git_cwd()
            && self.git.cache.commit_context(&cwd).is_some()
        {
            self.refresh_head(&cwd);
        }
        let Some(root) = self.workspace.record().map(|_| self.workspace.root().clone()) else {
            return;
        };
        for tab in model::workspace::leaves(&root).into_iter().flat_map(|l| l.tabs.iter()) {
            match tab {
                Tab::GitHistory { cwd, .. } if !self.history.log.contains_key(cwd) => self.refresh_log(cwd),
                Tab::GitHistory { cwd, .. } if !self.history.selected.contains_key(cwd) => {
                    // `GitHistoryView` selects the newest commit once the log loads.
                    if let Some(Read::Ready(commits)) = self.history.log.get(cwd)
                        && let Some(first) = commits.first().map(|c| c.hash.clone())
                    {
                        let cwd = cwd.clone();
                        self.select_commit(&cwd, &first);
                    }
                }
                Tab::CommitDiff { id, cwd, commit, path, old_path } if !self.history.diffs.contains_key(id) => {
                    self.history.diffs.insert(id.clone(), Read::Loading);
                    let mut body = json!({ "cwd": cwd, "hash": commit, "path": path });
                    if let Some(old) = old_path.as_ref().filter(|o| !o.is_empty()) {
                        body["oldPath"] = json!(old);
                    }
                    self.history_send(post("/git/commit-file-diff", body), HistoryFetch::FileDiff { tab: id.clone() });
                }
                _ => {}
            }
        }
    }

    pub(crate) fn history_replied(&mut self, tag: HistoryFetch, result: Result<Value, HttpError>) {
        let message = |e: &HttpError| match e {
            HttpError::Refused { message, .. } => message.clone(),
            other => other.to_string(),
        };
        match tag {
            HistoryFetch::Log { cwd, token } => {
                if self.history.tokens.get(&format!("log:{cwd}")) != Some(&token) {
                    return;
                }
                match result.map(serde_json::from_value::<Vec<Commit>>) {
                    Ok(Ok(commits)) => {
                        self.history.log.insert(cwd, Read::Ready(commits));
                    }
                    Ok(Err(e)) => self.history_failed(cwd, e.to_string()),
                    Err(e) => self.history_failed(cwd, message(&e)),
                }
            }
            HistoryFetch::Head { cwd, token } => {
                if self.history.tokens.get(&format!("head:{cwd}")) != Some(&token) {
                    return;
                }
                let commits = match result.map(serde_json::from_value::<Vec<Commit>>) {
                    Ok(Ok(commits)) => commits,
                    other => {
                        log::warn!("[git-history] HEAD summary failed: {other:?}");
                        Vec::new()
                    }
                };
                let count = commits.len();
                self.history.head.insert(cwd, (commits.into_iter().take(HEAD_RECENT).collect(), count));
            }
            HistoryFetch::Detail { cwd, hash } => {
                let read = match result.map(serde_json::from_value::<CommitDetail>) {
                    Ok(Ok(detail)) => Read::Ready(detail),
                    Ok(Err(e)) => Read::Failed(e.to_string()),
                    Err(e) => {
                        log::warn!("[git-history] commit detail failed for {}: {e}", &hash[..hash.len().min(12)]);
                        Read::Failed(message(&e))
                    }
                };
                self.history.details.insert((cwd, hash), read);
            }
            HistoryFetch::FileDiff { tab } => {
                let read = match result {
                    Ok(value) => {
                        let patch = value.get("diff").and_then(Value::as_str).unwrap_or_default().to_owned();
                        let too_large = value.get("tooLarge").and_then(Value::as_bool).unwrap_or(false);
                        self.history.diff_views.insert(tab.clone(), DiffState::new(rune_kit::diff::parse(&patch)));
                        Read::Ready((patch, too_large))
                    }
                    Err(e) => {
                        log::warn!("[git-history] commit file diff failed: {e}");
                        Read::Failed(message(&e))
                    }
                };
                self.history.diffs.insert(tab, read);
            }
        }
    }

    fn history_failed(&mut self, cwd: String, message: String) {
        log::warn!("[git-history] log failed for {cwd}: {message}");
        self.toast_error(&format!("Failed to load git history: Error: {message}"), "");
        self.history.log.insert(cwd, Read::Failed(message));
    }

    /// The working copy's History filters (none set when it has no entry).
    pub fn history_filters(&self, cwd: &str) -> &Filters {
        static NONE: std::sync::OnceLock<Filters> = std::sync::OnceLock::new();
        self.history.filters.get(cwd).unwrap_or_else(|| NONE.get_or_init(Filters::default))
    }

    /// `GitHistoryView`'s filter: the commits that match the query and the author.
    pub fn visible_commits<'a>(&'a self, cwd: &str) -> Vec<&'a Commit> {
        let Some(Read::Ready(commits)) = self.history.log.get(cwd) else {
            return Vec::new();
        };
        let filters = self.history_filters(cwd);
        let query = filters.query.text.trim().to_lowercase();
        commits
            .iter()
            .filter(|c| filters.author.as_ref().is_none_or(|a| &c.author == a))
            .filter(|c| {
                query.is_empty()
                    || c.subject.to_lowercase().contains(&query)
                    || c.author.to_lowercase().contains(&query)
                    || c.hash.starts_with(&query)
            })
            .collect()
    }
}

/// `describeRef`: a ref's label and whether it is HEAD's.
pub fn describe_ref(raw: &str) -> (String, bool) {
    let raw = raw.trim();
    if let Some(target) = raw.strip_prefix("HEAD -> ") {
        return (strip_ref(target.trim()).to_owned(), true);
    }
    if raw == "HEAD" {
        return ("HEAD".to_owned(), true);
    }
    let label = raw.strip_prefix("tag: ").unwrap_or(raw);
    (strip_ref(label.trim()).to_owned(), false)
}

fn strip_ref(r: &str) -> &str {
    for prefix in ["refs/heads/", "refs/remotes/", "refs/tags/"] {
        if let Some(rest) = r.strip_prefix(prefix) {
            return rest;
        }
    }
    r
}

/// Milliseconds since the epoch of an ISO 8601 date (`2026-11-25T10:00:00+07:00`).
pub fn parse_iso(iso: &str) -> Option<f64> {
    let num = |s: &str| s.parse::<i64>().ok();
    let (date, rest) = iso.split_once('T')?;
    let mut d = date.split('-');
    let (y, m, day) = (num(d.next()?)?, num(d.next()?)?, num(d.next()?)?);
    let time_end = rest.find(['Z', '+', '-']).unwrap_or(rest.len());
    let mut t = rest[..time_end].split(':');
    let (h, min) = (num(t.next()?)?, num(t.next()?)?);
    let sec = t.next().map_or(Some(0.0), |s| s.parse::<f64>().ok())?;
    let zone = &rest[time_end..];
    let offset = match zone.chars().next() {
        Some(sign @ ('+' | '-')) => {
            let mut z = zone[1..].split(':');
            let mins = num(z.next()?)? * 60 + z.next().and_then(num).unwrap_or(0);
            if sign == '+' { mins } else { -mins }
        }
        _ => 0,
    };
    let (yy, mm) = if m <= 2 { (y - 1, m + 9) } else { (y, m - 3) };
    let era = yy.div_euclid(400);
    let yoe = yy - era * 400;
    let doy = (153 * mm + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    let secs = days * 86_400 + h * 3600 + min * 60 - offset * 60;
    Some(secs as f64 * 1000.0 + sec * 1000.0)
}

const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/// `formatRelativeTime`: `now`, `5m`, `3h`, `2d`, `4w`, else the date.
pub fn relative_time(iso: &str, now_ms: f64) -> String {
    let Some(then) = parse_iso(iso) else { return String::new() };
    let seconds = ((now_ms - then) / 1000.0).floor().max(0.0) as i64;
    if seconds < 60 {
        return "now".into();
    }
    let minutes = seconds / 60;
    if minutes < 60 {
        return format!("{minutes}m");
    }
    let hours = minutes / 60;
    if hours < 24 {
        return format!("{hours}h");
    }
    let days = hours / 24;
    if days < 7 {
        return format!("{days}d");
    }
    if days / 7 < 8 {
        return format!("{}w", days / 7);
    }
    let date = iso.split('T').next().unwrap_or_default();
    let mut parts = date.split('-');
    let (y, m, d) = (parts.next().unwrap_or(""), parts.next().and_then(|m| m.parse::<usize>().ok()).unwrap_or(1), parts.next().and_then(|d| d.parse::<u32>().ok()).unwrap_or(1));
    format!("{} {d}, {y}", MONTHS[(m.clamp(1, 12)) - 1])
}

/// `commitCountLabel`.
pub fn count_label(count: usize) -> String {
    let plus = if count >= LOG_LIMIT { "+" } else { "" };
    format!("{count}{plus} {}", if count == 1 { "commit" } else { "commits" })
}

/// The wall clock in milliseconds.
pub fn now_ms() -> f64 {
    #[cfg(target_arch = "wasm32")]
    {
        js_sys::Date::now()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0.0, |d| d.as_millis() as f64)
    }
}
