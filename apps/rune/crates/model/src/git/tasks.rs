//! The store's async actions as tasks. A task runs until it waits on one host call or on
//! child tasks; the reply or the last child's end resumes it. Each `await` of the TypeScript
//! action is one wait here, in the same order, so the calls go out in the order the
//! recorder saw them.

use std::collections::VecDeque;

use serde::de::DeserializeOwned;
use serde_json::Value;

use super::{Call, CommitContext, Effect, ErrorField, Event, Failure, Outcome, State, TaskId, diff_key};
use crate::Level;
use crate::workspace::normalize_path;

#[derive(Clone, Debug)]
pub(super) struct Task {
    parent: Option<TaskId>,
    kind: Kind,
}

impl Task {
    pub(super) fn of(action: Event) -> Kind {
        use Event as E;
        match action {
            E::RefreshStatus(cwd) => Kind::Spawn(Spawn::Status, cwd),
            E::RefreshDiffStat(cwd) => Kind::Spawn(Spawn::DiffStat, cwd),
            E::FetchCommitContext(cwd) => Kind::Spawn(Spawn::Context, cwd),
            E::FetchStashes(cwd) => Kind::Spawn(Spawn::Stashes, cwd),
            E::FetchBranches(cwd) => Kind::Spawn(Spawn::Branches, cwd),
            E::RefreshWorkingCopy(cwd) => Kind::WorkingCopy(cwd),
            E::FetchDiff(cwd, path, staged, options) => Kind::FetchDiff {
                cwd,
                path,
                staged,
                notify: options.and_then(|o| o.notify) != Some(false),
            },
            E::StageFile(cwd, path) => batch(cwd, vec![path], |cwd, path| Call::Stage { cwd, path }),
            E::UnstageFile(cwd, path) => batch(cwd, vec![path], |cwd, path| Call::Unstage { cwd, path }),
            E::DiscardFile(cwd, path) => batch(cwd, vec![path], |cwd, path| Call::Discard { cwd, path }),
            E::StageFiles(cwd, paths) => batch(cwd, paths, |cwd, path| Call::Stage { cwd, path }),
            E::UnstageFiles(cwd, paths) => batch(cwd, paths, |cwd, path| Call::Unstage { cwd, path }),
            E::DiscardFiles(cwd, paths) => batch(cwd, paths, |cwd, path| Call::Discard { cwd, path }),
            E::StageHunk(cwd, path, hunk_patch) => {
                let call = Call::StageHunk { cwd: cwd.clone(), path: path.clone(), hunk_patch };
                chain(cwd.clone(), [Link::write(call, Some(path), OnError::Finally), Link::Finally], after_write(&cwd))
            }
            E::UnstageHunk(cwd, path, hunk_patch) => {
                let call = Call::UnstageHunk { cwd: cwd.clone(), path: path.clone(), hunk_patch };
                chain(cwd.clone(), [Link::write(call, Some(path), OnError::Finally), Link::Finally], after_write(&cwd))
            }
            E::Commit(cwd, summary, description, amend) => chain(
                cwd.clone(),
                [Link::EnsureHead, Link::Commit { summary, description, amend }],
                after_mutation(&cwd),
            ),
            E::Push(cwd) => mutation(Call::Push { cwd: cwd.clone() }, cwd),
            E::StashSave(cwd, message, include_untracked) => {
                mutation(Call::StashSave { cwd: cwd.clone(), message, include_untracked }, cwd)
            }
            E::StashApply(cwd, index) => mutation(Call::StashApply { cwd: cwd.clone(), index }, cwd),
            E::StashPop(cwd, index) => mutation(Call::StashPop { cwd: cwd.clone(), index }, cwd),
            E::StashDrop(cwd, index) => chain(
                cwd.clone(),
                [Link::write(Call::StashDrop { cwd, index }, None, OnError::Abort)],
                [Link::Sub(Spawn::Stashes)],
            ),
            E::BranchSwitch(cwd, name) => chain(
                cwd.clone(),
                [Link::write(Call::BranchSwitch { cwd: cwd.clone(), name }, None, OnError::Abort), Link::ClearSelection],
                after_mutation(&cwd).into_iter().chain([Link::Reconcile]),
            ),
            E::BranchCreate(cwd, name) => chain(
                cwd.clone(),
                [Link::write(Call::CreateBranch { cwd: cwd.clone(), name }, None, OnError::Abort), Link::ClearSelection],
                after_mutation(&cwd),
            ),
            other => unreachable!("not an action: {other:?}"),
        }
    }

    pub(super) fn diff_stat(cwd: &str) -> Kind {
        Kind::Spawn(Spawn::DiffStat, cwd.to_owned())
    }
}

/// What a task is before it starts.
#[derive(Clone, Debug)]
pub(super) enum Kind {
    Spawn(Spawn, String),
    WorkingCopy(String),
    FetchDiff { cwd: String, path: String, staged: bool, notify: bool },
    Chain(Chain),
    /// Running: one read waiting on its reply.
    Read(Read),
    /// Running: `refreshWorkingCopy`'s `Promise.all`, with the children not yet ended.
    Join(u32),
}

/// The single-read actions, also awaited inside chains.
#[derive(Clone, Copy, Debug)]
pub(super) enum Spawn {
    Status,
    Context,
    DiffStat,
    Stashes,
    Branches,
}

#[derive(Clone, Debug)]
pub(super) enum Read {
    Status { key: String, token: u64 },
    Context { cwd: String, key: String, token: u64 },
    /// The context came back; its HEAD is being read.
    Head { key: String, token: u64, context: CommitContext },
    DiffStat { key: String },
    Diff { cwd: String, key: String, token: u64, notify: bool },
    Stashes { key: String },
    Branches { key: String },
}

#[derive(Clone, Debug)]
pub(super) struct Chain {
    cwd: String,
    links: VecDeque<Link>,
    error: Option<Failure>,
    /// The write whose reply the chain waits on.
    waiting: Option<(Option<String>, OnError)>,
}

#[derive(Clone, Debug)]
enum Link {
    Write { call: Call, invalidate: Option<String>, on_error: OnError },
    /// Read the HEAD when no commit-context fetch recorded one.
    EnsureHead,
    Commit { summary: String, description: String, amend: bool },
    /// Where a failed `Finally` write resumes (the `finally` block).
    Finally,
    Sub(Spawn),
    ClearSelection,
    Reconcile,
}

impl Link {
    fn write(call: Call, invalidate: Option<String>, on_error: OnError) -> Link {
        Link::Write { call, invalidate, on_error }
    }
}

#[derive(Clone, Copy, Debug)]
enum OnError {
    /// Skip to the `finally` block, then reject.
    Finally,
    Abort,
    /// A commit refused because HEAD moved refreshes the panel, then rejects.
    RefreshIfPrecondition,
}

fn chain(cwd: String, head: impl IntoIterator<Item = Link>, tail: impl IntoIterator<Item = Link>) -> Kind {
    Kind::Chain(Chain { cwd, links: head.into_iter().chain(tail).collect(), error: None, waiting: None })
}

fn batch(cwd: String, paths: Vec<String>, call: fn(String, String) -> Call) -> Kind {
    if paths.is_empty() {
        return chain(cwd, [], []);
    }
    let writes: Vec<Link> = paths
        .into_iter()
        .map(|path| Link::write(call(cwd.clone(), path.clone()), Some(path), OnError::Finally))
        .chain([Link::Finally])
        .collect();
    let tail = after_write(&cwd);
    chain(cwd, writes, tail)
}

fn mutation(call: Call, cwd: String) -> Kind {
    let tail = after_mutation(&cwd);
    chain(cwd, [Link::write(call, None, OnError::Abort)], tail)
}

/// `refreshAfterWrite`: status (its failure swallowed), context, diff stat.
fn after_write(_cwd: &str) -> Vec<Link> {
    vec![Link::Sub(Spawn::Status), Link::Sub(Spawn::Context), Link::Sub(Spawn::DiffStat)]
}

/// `refreshAfterMutation`: `refreshAfterWrite`, then the stash and branch lists.
fn after_mutation(cwd: &str) -> Vec<Link> {
    let mut links = after_write(cwd);
    links.extend([Link::Sub(Spawn::Stashes), Link::Sub(Spawn::Branches)]);
    links
}

enum Poll {
    Pending(Kind),
    Ready(Outcome),
}

pub(super) struct Cx<'a> {
    pub(super) state: &'a mut State,
    pub(super) effects: Vec<Effect>,
    pub(super) now: f64,
}

impl Cx<'_> {
    /// `trackPath`: the key, recording the first display path seen for it.
    pub(super) fn track(&mut self, cwd: &str) -> String {
        let key = normalize_path(cwd);
        self.state.paths.entry(key.clone()).or_insert_with(|| cwd.to_owned());
        key
    }

    /// Starts a root task; one that ends at once settles in this step.
    pub(super) fn start(&mut self, kind: Kind, parent: Option<TaskId>) -> TaskId {
        let (id, done) = self.launch(kind, parent);
        if let Some(outcome) = done {
            self.effects.push(Effect::Settled { task: id, outcome });
        }
        id
    }

    /// Runs a task to its first wait. `Some` when it ended without one.
    fn launch(&mut self, kind: Kind, parent: Option<TaskId>) -> (TaskId, Option<Outcome>) {
        let id = self.state.book.next_task;
        self.state.book.next_task += 1;
        match self.begin(id, kind) {
            Poll::Pending(kind) => {
                self.state.book.tasks.insert(id, Task { parent, kind });
                (id, None)
            }
            Poll::Ready(outcome) => (id, Some(outcome)),
        }
    }

    fn call(&mut self, task: TaskId, call: Call) {
        self.effects.push(Effect::Call { task, call });
    }

    fn log(&mut self, outcome: &str, key: &str, failure: &Failure) {
        self.effects.push(Effect::Log {
            level: Level::Warn,
            message: format!("[git-status] {outcome} failed for {key}: {}", failure.message),
        });
    }

    fn begin(&mut self, id: TaskId, kind: Kind) -> Poll {
        match kind {
            Kind::Spawn(spawn, cwd) => self.begin_read(id, spawn, cwd),
            Kind::WorkingCopy(cwd) => {
                let mut left = 0;
                for spawn in [Spawn::Status, Spawn::Context, Spawn::DiffStat] {
                    if self.launch(Kind::Spawn(spawn, cwd.clone()), Some(id)).1.is_none() {
                        left += 1;
                    }
                }
                if left == 0 { Poll::Ready(Ok(None)) } else { Poll::Pending(Kind::Join(left)) }
            }
            Kind::FetchDiff { cwd, path, staged, notify } => {
                let key = diff_key(&cwd, &path, staged);
                self.state.diff_errors.remove(&key);
                let token = self.state.book.diff_versions.get(&key).copied().unwrap_or(0);
                self.call(id, Call::Diff { cwd: cwd.clone(), path, staged });
                Poll::Pending(Kind::Read(Read::Diff { cwd, key, token, notify }))
            }
            Kind::Chain(chain) => self.advance(id, chain),
            running @ (Kind::Read(_) | Kind::Join(_)) => Poll::Pending(running),
        }
    }

    fn begin_read(&mut self, id: TaskId, spawn: Spawn, cwd: String) -> Poll {
        let read = match spawn {
            Spawn::Status => {
                let key = self.track(&cwd);
                let token = bump(&mut self.state.book.status_requests, &key);
                self.state.status_fetch_count += 1;
                self.state.is_fetching_status = true;
                self.call(id, Call::Status { cwd });
                Read::Status { key, token }
            }
            Spawn::Context => {
                let key = self.track(&cwd);
                let token = bump(&mut self.state.book.context_requests, &key);
                self.call(id, Call::CommitContext { cwd: cwd.clone() });
                Read::Context { cwd, key, token }
            }
            Spawn::DiffStat => {
                let key = normalize_path(&cwd);
                if !self.state.book.watchers.contains_key(&key) {
                    return Poll::Ready(Ok(None));
                }
                let book = &mut self.state.book;
                if let Some(&last) = book.last_fetch.get(&key)
                    && self.now - last < super::DIFF_STAT_MIN_INTERVAL_MS
                {
                    if book.trailing.insert(key) {
                        let at = last + super::DIFF_STAT_MIN_INTERVAL_MS;
                        self.effects.push(Effect::ScheduleDiffStat { cwd, at });
                    }
                    return Poll::Ready(Ok(None));
                }
                book.last_fetch.insert(key.clone(), self.now);
                self.track(&cwd);
                self.call(id, Call::DiffStat { cwd });
                Read::DiffStat { key }
            }
            Spawn::Stashes => {
                let key = self.track(&cwd);
                self.call(id, Call::StashList { cwd });
                Read::Stashes { key }
            }
            Spawn::Branches => {
                let key = self.track(&cwd);
                self.call(id, Call::BranchList { cwd });
                Read::Branches { key }
            }
        };
        Poll::Pending(Kind::Read(read))
    }

    /// Runs a chain's links until one waits.
    fn advance(&mut self, id: TaskId, mut chain: Chain) -> Poll {
        while let Some(link) = chain.links.pop_front() {
            match link {
                Link::Finally => {}
                Link::ClearSelection => self.state.selected_file = None,
                Link::Reconcile => self.effects.push(Effect::ReconcileWorktree { cwd: chain.cwd.clone() }),
                Link::Sub(spawn) => {
                    if self.launch(Kind::Spawn(spawn, chain.cwd.clone()), Some(id)).1.is_none() {
                        return Poll::Pending(Kind::Chain(chain));
                    }
                }
                Link::EnsureHead => {
                    let key = self.track(&chain.cwd);
                    if !self.state.book.known_heads.contains_key(&key)
                        && self.launch(Kind::Spawn(Spawn::Context, chain.cwd.clone()), Some(id)).1.is_none()
                    {
                        return Poll::Pending(Kind::Chain(chain));
                    }
                }
                Link::Commit { summary, description, amend } => {
                    let key = normalize_path(&chain.cwd);
                    let Some(Some(expected_head)) = self.state.book.known_heads.get(&key).cloned() else {
                        return Poll::Ready(Err(Failure::new("HEAD could not be read. Refresh and try again.")));
                    };
                    let call = Call::Commit { cwd: chain.cwd.clone(), summary, description, amend, expected_head };
                    self.call(id, call);
                    chain.waiting = Some((None, OnError::RefreshIfPrecondition));
                    return Poll::Pending(Kind::Chain(chain));
                }
                Link::Write { call, invalidate, on_error } => {
                    self.call(id, call);
                    chain.waiting = Some((invalidate, on_error));
                    return Poll::Pending(Kind::Chain(chain));
                }
            }
        }
        Poll::Ready(chain.error.map_or(Ok(None), Err))
    }

    /// The host answered `id`'s call.
    pub(super) fn reply(&mut self, id: TaskId, result: Result<Value, Failure>) {
        let Some(Task { parent, kind }) = self.state.book.tasks.remove(&id) else {
            return;
        };
        let poll = match kind {
            Kind::Read(read) => self.read_done(id, read, result),
            Kind::Chain(mut chain) => match (chain.waiting.take(), result) {
                (Some((invalidate, _)), Ok(_)) => {
                    if let Some(path) = invalidate {
                        let cwd = chain.cwd.clone();
                        self.state.invalidate_file_diffs(&cwd, &path);
                    }
                    self.advance(id, chain)
                }
                (Some((_, on_error)), Err(failure)) => match on_error {
                    OnError::Abort => Poll::Ready(Err(failure)),
                    OnError::RefreshIfPrecondition if !failure.is_precondition_failed() => Poll::Ready(Err(failure)),
                    OnError::RefreshIfPrecondition => {
                        chain.links = after_write(&chain.cwd).into();
                        chain.error = Some(failure);
                        self.advance(id, chain)
                    }
                    OnError::Finally => {
                        while let Some(link) = chain.links.pop_front() {
                            if matches!(link, Link::Finally) {
                                break;
                            }
                        }
                        chain.error = Some(failure);
                        self.advance(id, chain)
                    }
                },
                (None, _) => Poll::Pending(Kind::Chain(chain)),
            },
            other => Poll::Pending(other),
        };
        self.settle(id, parent, poll);
    }

    fn settle(&mut self, id: TaskId, parent: Option<TaskId>, poll: Poll) {
        match poll {
            Poll::Pending(kind) => {
                self.state.book.tasks.insert(id, Task { parent, kind });
            }
            Poll::Ready(outcome) => match parent {
                Some(parent) => self.resume(parent),
                None => self.effects.push(Effect::Settled { task: id, outcome }),
            },
        }
    }

    /// A child of `id` ended.
    fn resume(&mut self, id: TaskId) {
        let Some(Task { parent, kind }) = self.state.book.tasks.remove(&id) else {
            return;
        };
        let poll = match kind {
            Kind::Join(left) if left > 1 => Poll::Pending(Kind::Join(left - 1)),
            Kind::Join(_) => Poll::Ready(Ok(None)),
            Kind::Chain(chain) => self.advance(id, chain),
            other => Poll::Pending(other),
        };
        self.settle(id, parent, poll);
    }

    fn read_done(&mut self, id: TaskId, read: Read, result: Result<Value, Failure>) -> Poll {
        match read {
            Read::Status { key, token } => {
                let current = self.state.book.status_requests.get(&key) == Some(&token);
                let outcome = match result.and_then(parse_reply) {
                    Ok(status) => {
                        if current {
                            self.state.statuses.insert(key.clone(), status);
                            self.state.patch_error(&key, ErrorField::Status, None);
                        }
                        Ok(None)
                    }
                    Err(failure) => {
                        if current {
                            self.state.patch_error(&key, ErrorField::Status, Some(failure.message.clone()));
                        }
                        self.log("status refresh", &key, &failure);
                        Err(failure)
                    }
                };
                let count = self.state.status_fetch_count.saturating_sub(1);
                self.state.status_fetch_count = count;
                self.state.is_fetching_status = count > 0;
                Poll::Ready(outcome)
            }
            Read::Context { cwd, key, token } => {
                let current = self.state.book.context_requests.get(&key) == Some(&token);
                match result.and_then(parse_reply::<CommitContext>) {
                    Ok(context) if current && context.has_head => {
                        self.call(id, Call::Head { cwd });
                        Poll::Pending(Kind::Read(Read::Head { key, token, context }))
                    }
                    Ok(context) => Poll::Ready(self.context_read(&key, token, context, current.then(|| Some(String::new())))),
                    Err(_) if !current => Poll::Ready(Ok(Some(Value::Null))),
                    Err(failure) => {
                        self.state.book.known_heads.remove(&key);
                        self.state.commit_contexts.remove(&key);
                        self.state.patch_error(&key, ErrorField::CommitContext, Some(failure.message.clone()));
                        self.log("commit context fetch", &key, &failure);
                        Poll::Ready(Ok(Some(Value::Null)))
                    }
                }
            }
            Read::Head { key, token, context } => {
                let head = result
                    .ok()
                    .and_then(|log| log.get(0)?.get("hash")?.as_str().map(str::to_owned));
                Poll::Ready(self.context_read(&key, token, context, Some(head)))
            }
            Read::DiffStat { key } => {
                match result.and_then(parse_reply) {
                    Ok(stat) => {
                        self.state.diff_stats.insert(key.clone(), stat);
                        self.state.patch_error(&key, ErrorField::DiffStat, None);
                    }
                    Err(failure) => {
                        self.state.patch_error(&key, ErrorField::DiffStat, Some(failure.message.clone()));
                        self.log("diff stat refresh", &key, &failure);
                    }
                }
                Poll::Ready(Ok(None))
            }
            Read::Diff { cwd, key, token, notify } => {
                if self.state.book.diff_versions.get(&key).copied().unwrap_or(0) != token {
                    return Poll::Ready(Ok(None));
                }
                match result.and_then(parse_reply::<String>) {
                    Ok(diff) => {
                        self.state.diffs.insert(key.clone(), diff);
                        self.state.book.diff_revision += 1;
                        self.state.diff_errors.remove(&key);
                    }
                    Err(failure) => {
                        self.state.diff_errors.insert(key.clone(), failure.message.clone());
                        if notify {
                            self.state.diffs.insert(key, String::new());
                            self.state.book.diff_revision += 1;
                            self.effects.push(Effect::Toast { message: format!("Failed to load diff: {}", failure.shown()) });
                        } else {
                            self.log("diff fetch", &normalize_path(&cwd), &failure);
                        }
                    }
                }
                Poll::Ready(Ok(None))
            }
            Read::Stashes { key } => {
                if let Ok(stashes) = result.and_then(parse_reply) {
                    self.state.stashes.insert(key, stashes);
                }
                Poll::Ready(Ok(None))
            }
            Read::Branches { key } => {
                if let Ok(branches) = result.and_then(parse_reply) {
                    self.state.branches.insert(key, branches);
                }
                Poll::Ready(Ok(None))
            }
        }
    }

    /// The end of `fetchCommitContext`: `head` is `None` when no HEAD was read (superseded).
    fn context_read(&mut self, key: &str, token: u64, context: CommitContext, head: Option<Option<String>>) -> Outcome {
        let current = self.state.book.context_requests.get(key) == Some(&token);
        if !current {
            return Ok(Some(Value::Null));
        }
        if let Some(head) = head {
            self.state.book.known_heads.insert(key.to_owned(), head);
        }
        let value = serde_json::to_value(&context).unwrap_or(Value::Null);
        self.state.commit_contexts.insert(key.to_owned(), context);
        self.state.patch_error(key, ErrorField::CommitContext, None);
        Ok(Some(value))
    }
}

fn bump(tokens: &mut std::collections::HashMap<String, u64>, key: &str) -> u64 {
    let token = tokens.entry(key.to_owned()).or_default();
    *token += 1;
    *token
}

fn parse_reply<T: DeserializeOwned>(value: Value) -> Result<T, Failure> {
    serde_json::from_value(value).map_err(|e| Failure::new(format!("unreadable host reply: {e}")))
}
