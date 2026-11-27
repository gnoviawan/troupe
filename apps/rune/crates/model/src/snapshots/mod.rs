//! Workspace snapshots (`stores/snapshot-store.ts`): each project's named copies of its
//! terminals, kept newest first in this device's `snapshots/<projectId>` key.
//!
//! Create, rename and delete change the list here at once, then read the saved list, change
//! it the same way and write it back. A project runs one of these at a time (the store's
//! per-project mutation queue), so a rename can never write a list read before a delete. A
//! failed create or rename is undone here; a failed delete stays removed, as React keeps it.
//! Every step returns [`Effect`]s: the read or write to send, boundary logs, and the outcome
//! the dialog that asked waits for.

pub mod trim;

use std::collections::{BTreeMap, VecDeque};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::Level;

/// The host store key of a project's list (device scope).
#[must_use]
pub fn key(project_id: &str) -> String {
    format!("snapshots/{project_id}")
}

/// A placeholder of the `updatedAt` stamp's length (`toISOString` is always 24 characters), so
/// a trim measures the list the app will write.
const STAMP: &str = "0000-00-00T00:00:00.000Z";

/// One snapshot as the page lists it (`Snapshot`). `createdAt` stays the saved ISO text.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub id: String,
    pub project_id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub created_at: String,
    pub pane_count: usize,
    pub process_count: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
}

impl Snapshot {
    /// `persistedToSnapshot`.
    fn of(saved: &Value) -> Option<Snapshot> {
        let text = |key: &str| saved.get(key).and_then(Value::as_str).map(str::to_owned);
        Some(Snapshot {
            id: text("id")?,
            project_id: text("projectId")?,
            name: text("name").unwrap_or_default(),
            description: text("description"),
            created_at: text("createdAt").unwrap_or_default(),
            pane_count: saved.get("terminals").and_then(Value::as_array).map_or(0, Vec::len),
            process_count: 0,
            tag: text("tag"),
        })
    }
}

/// How a read of a project's list came back.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Read {
    /// The saved `{ snapshots, updatedAt }`.
    Found(Value),
    /// The key holds nothing (`KEY_NOT_FOUND`); `error` is the host's text.
    Missing { error: String },
    /// A reply that said success and carried no data.
    Empty,
    Failed { code: String, error: String },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Op {
    Create,
    Rename,
    Delete,
}

impl Op {
    fn name(self) -> &'static str {
        match self {
            Op::Create => "create",
            Op::Rename => "rename",
            Op::Delete => "delete",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Effect {
    /// Read the list at `key`: answer a `load` with [`Event::Loaded`], else with [`Event::Read`].
    Read {
        key: String,
        project_id: String,
        #[serde(skip)]
        load: bool,
    },
    /// Write `record` at `key` with an `updatedAt` stamp; answer with [`Event::Written`].
    WriteSnapshots { key: String, record: Value },
    Log { level: Level, source: String, message: String },
    Toast { level: String, message: String },
    /// The mutation asked for on `id` finished: `error` is why it failed.
    Settled {
        op: Op,
        id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
}

/// One list mutation, waiting or running.
#[derive(Clone, Debug, PartialEq)]
struct Mutation {
    op: Op,
    project_id: String,
    id: String,
    /// The persisted snapshot a create adds, or the name a rename writes.
    payload: Value,
    /// The name a failed rename restores.
    previous: String,
    /// The write is out: the list is past its read.
    writing: bool,
    /// Snapshots the trim of this write removes.
    dropped: Vec<String>,
    /// Snapshots the trim of this write cut the output of.
    shrunk: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct State {
    pub snapshots: Vec<Snapshot>,
    pub is_loading: bool,
    /// Each project's mutations, the running one first.
    #[serde(skip)]
    queues: BTreeMap<String, VecDeque<Mutation>>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    /// `loadSnapshots`.
    Load(String),
    /// The list a load asked for.
    Loaded { project_id: String, read: Read },
    /// `createSnapshot`, with the id and `createdAt` the app drew.
    Create {
        id: String,
        created_at: String,
        name: String,
        description: Option<String>,
        project_id: String,
        terminals: Vec<Value>,
        active_terminal_id: Option<String>,
    },
    Rename { id: String, name: String },
    Delete(String),
    /// The read the running mutation of `project_id` asked for.
    Read { project_id: String, read: Read },
    /// Its write: `Err` holds the host's text.
    Written { project_id: String, result: Result<(), String> },
}

fn log(level: Level, op: &str, message: String) -> Effect {
    Effect::Log {
        level,
        source: format!("snapshot-store.{op}"),
        message,
    }
}

fn boundary(op: Op, project_id: &str, error: Option<&str>) -> Effect {
    match error {
        None => log(Level::Info, &format!("{}Snapshot", op.name()), format!("snapshot {} ok project={project_id}", op.name())),
        Some(detail) => log(
            Level::Error,
            &format!("{}Snapshot", op.name()),
            format!("snapshot {} failed project={project_id} {detail}", op.name()),
        ),
    }
}

impl State {
    /// The snapshots of `project_id`, newest first (`useSnapshots`).
    pub fn of(&self, project_id: &str) -> impl Iterator<Item = &Snapshot> {
        self.snapshots.iter().filter(move |s| s.project_id == project_id)
    }

    pub fn find(&self, id: &str) -> Option<&Snapshot> {
        self.snapshots.iter().find(|s| s.id == id)
    }

    /// Whether `project_id` has a mutation running or waiting.
    pub fn busy(&self, project_id: &str) -> bool {
        self.queues.get(project_id).is_some_and(|q| !q.is_empty())
    }

    pub fn step(&mut self, event: Event) -> Vec<Effect> {
        let mut fx = Vec::new();
        match event {
            Event::Load(project_id) => {
                self.is_loading = true;
                fx.push(Effect::Read { key: key(&project_id), project_id, load: true });
            }
            Event::Loaded { read, .. } => {
                self.snapshots = match read {
                    Read::Found(list) => list
                        .get("snapshots")
                        .and_then(Value::as_array)
                        .map(|all| all.iter().filter_map(Snapshot::of).collect())
                        .unwrap_or_default(),
                    _ => Vec::new(),
                };
                self.is_loading = false;
            }
            Event::Create {
                id,
                created_at,
                name,
                description,
                project_id,
                terminals,
                active_terminal_id,
            } => {
                let description = description.filter(|d| !d.is_empty());
                let snapshot = Snapshot {
                    id: id.clone(),
                    project_id: project_id.clone(),
                    name,
                    description,
                    created_at,
                    pane_count: terminals.len(),
                    process_count: 0,
                    tag: None,
                };
                let mut saved = json!({
                    "id": snapshot.id,
                    "projectId": snapshot.project_id,
                    "name": snapshot.name,
                });
                if let Some(d) = &snapshot.description {
                    saved["description"] = json!(d);
                }
                saved["createdAt"] = json!(snapshot.created_at);
                saved["terminals"] = Value::Array(terminals);
                saved["activeTerminalId"] = json!(active_terminal_id);
                self.snapshots.insert(0, snapshot);
                self.enqueue(Op::Create, project_id, id, saved, String::new(), &mut fx);
            }
            Event::Rename { id, name } => {
                let Some(found) = self.snapshots.iter_mut().find(|s| s.id == id) else {
                    fx.push(Effect::Settled { op: Op::Rename, id, error: None });
                    return fx;
                };
                let previous = std::mem::replace(&mut found.name, name.clone());
                let project_id = found.project_id.clone();
                self.enqueue(Op::Rename, project_id, id, json!(name), previous, &mut fx);
            }
            Event::Delete(id) => {
                let Some(at) = self.snapshots.iter().position(|s| s.id == id) else {
                    fx.push(Effect::Settled { op: Op::Delete, id, error: None });
                    return fx;
                };
                let project_id = self.snapshots.remove(at).project_id;
                self.enqueue(Op::Delete, project_id, id, Value::Null, String::new(), &mut fx);
            }
            Event::Read { project_id, read } => self.read(&project_id, read, &mut fx),
            Event::Written { project_id, result } => self.written(&project_id, result, &mut fx),
        }
        fx
    }

    fn enqueue(&mut self, op: Op, project_id: String, id: String, payload: Value, previous: String, fx: &mut Vec<Effect>) {
        let queue = self.queues.entry(project_id.clone()).or_default();
        queue.push_back(Mutation {
            op,
            project_id: project_id.clone(),
            id,
            payload,
            previous,
            writing: false,
            dropped: Vec::new(),
            shrunk: 0,
        });
        if queue.len() == 1 {
            fx.push(Effect::Read { key: key(&project_id), project_id, load: false });
        }
    }

    fn running(&mut self, project_id: &str) -> Option<&mut Mutation> {
        self.queues.get_mut(project_id).and_then(VecDeque::front_mut)
    }

    fn read(&mut self, project_id: &str, read: Read, fx: &mut Vec<Effect>) {
        let Some(m) = self.running(project_id).filter(|m| !m.writing).cloned() else {
            return;
        };
        let refused = match (m.op, &read) {
            (Op::Create, Read::Failed { code, error }) => Some(format!("Failed to read persisted snapshots: code={code} error={error}")),
            (Op::Create, Read::Empty) => Some("Failed to read persisted snapshots: success reply carried no data".to_owned()),
            (Op::Rename, Read::Failed { error, .. } | Read::Missing { error }) => {
                Some(format!("snapshot list unreadable for rename: {error}"))
            }
            (Op::Rename, Read::Empty) => Some("snapshot list unreadable for rename: missing data".to_owned()),
            (Op::Delete, Read::Failed { error, .. }) => Some(format!("Failed to read persisted snapshots for delete: {error}")),
            (Op::Delete, Read::Empty) => {
                Some("Failed to read persisted snapshots for delete: success reply carried no data".to_owned())
            }
            _ => None,
        };
        if let Some(error) = refused {
            self.fail(project_id, error, fx);
            return;
        }
        let missing = matches!(read, Read::Missing { .. });
        let saved = match read {
            Read::Found(list) => list.get("snapshots").and_then(Value::as_array).cloned().unwrap_or_default(),
            _ => Vec::new(),
        };
        let snapshots: Vec<Value> = match m.op {
            Op::Create => std::iter::once(m.payload.clone()).chain(saved).collect(),
            Op::Rename => saved
                .into_iter()
                .map(|mut s| {
                    if s["id"] == json!(m.id) {
                        s["name"] = m.payload.clone();
                    }
                    s
                })
                .collect(),
            Op::Delete => saved.into_iter().filter(|s| s["id"] != json!(m.id)).collect(),
        };
        if m.op == Op::Delete && missing {
            // The list is gone: the delete's end state already holds.
            fx.push(boundary(m.op, project_id, None));
            self.finish(project_id, m.op, m.id, None, fx);
            return;
        }
        self.write(project_id, snapshots, fx);
    }

    fn write(&mut self, project_id: &str, snapshots: Vec<Value>, fx: &mut Vec<Effect>) {
        let trimmed = trim::trim(&json!({ "snapshots": snapshots, "updatedAt": STAMP }), trim::STORE_VALUE_MAX_BYTES);
        let mut record = trimmed.list;
        if let Some(fields) = record.as_object_mut() {
            fields.shift_remove("updatedAt");
        }
        let Some(m) = self.running(project_id) else { return };
        m.writing = true;
        m.dropped = trimmed.dropped_ids;
        m.shrunk = trimmed.shrunk;
        fx.push(Effect::WriteSnapshots { key: key(project_id), record });
    }

    fn written(&mut self, project_id: &str, result: Result<(), String>, fx: &mut Vec<Effect>) {
        let Some(m) = self.running(project_id).filter(|m| m.writing).cloned() else {
            return;
        };
        if let Err(error) = result {
            let what = match m.op {
                Op::Create => "Failed to persist snapshot",
                Op::Rename => "Failed to persist snapshot rename",
                Op::Delete => "Failed to persist snapshot delete",
            };
            self.fail(project_id, format!("{what}: {error}"), fx);
            return;
        }
        if m.shrunk > 0 || !m.dropped.is_empty() {
            let (shrunk, dropped) = (m.shrunk, m.dropped.len());
            fx.push(log(
                Level::Warn,
                "trimSnapshots",
                format!("snapshot list over the store value cap project={project_id} shrunk={shrunk} dropped={dropped}"),
            ));
            let message = if dropped > 0 {
                format!("Snapshot storage is full: {dropped} oldest snapshot(s) were removed to save this change.")
            } else {
                format!("Snapshot storage is full: {shrunk} snapshot(s) lost older terminal output to save this change.")
            };
            fx.push(Effect::Toast { level: "warning".into(), message });
        }
        fx.push(boundary(m.op, project_id, None));
        if m.op != Op::Delete && !m.dropped.is_empty() {
            self.snapshots.retain(|s| !m.dropped.contains(&s.id));
        }
        self.finish(project_id, m.op, m.id, None, fx);
    }

    /// The running mutation failed: undo a create or rename, log, settle, start the next.
    fn fail(&mut self, project_id: &str, error: String, fx: &mut Vec<Effect>) {
        let Some(m) = self.running(project_id).cloned() else { return };
        match m.op {
            Op::Create => self.snapshots.retain(|s| s.id != m.id),
            Op::Rename => {
                if let Some(s) = self.snapshots.iter_mut().find(|s| s.id == m.id) {
                    s.name = m.previous.clone();
                }
            }
            Op::Delete => {}
        }
        fx.push(boundary(m.op, project_id, Some(&error)));
        self.finish(project_id, m.op, m.id, Some(error), fx);
    }

    fn finish(&mut self, project_id: &str, op: Op, id: String, error: Option<String>, fx: &mut Vec<Effect>) {
        let queue = self.queues.entry(project_id.to_owned()).or_default();
        queue.pop_front();
        let next = !queue.is_empty();
        if queue.is_empty() {
            self.queues.remove(project_id);
        }
        fx.push(Effect::Settled { op, id, error });
        if next {
            fx.push(Effect::Read { key: key(project_id), project_id: project_id.to_owned(), load: false });
        }
    }
}

/// `formatTime` on a snapshot card: its age from `created_ms` to `now_ms`.
#[must_use]
pub fn age(created_ms: f64, now_ms: f64) -> String {
    let diff = now_ms - created_ms;
    let hours = (diff / 3_600_000.0).floor();
    let days = (diff / 86_400_000.0).floor();
    if hours < 1.0 {
        "Just now".to_owned()
    } else if hours < 24.0 {
        format!("{hours:.0} hours ago")
    } else if days == 1.0 {
        "Yesterday".to_owned()
    } else {
        format!("{days:.0} days ago")
    }
}

/// `Date.prototype.toISOString` of `ms` since the epoch (UTC, milliseconds, `Z`).
#[must_use]
pub fn iso(ms: f64) -> String {
    let ms = ms.floor() as i64;
    let (days, rem) = (ms.div_euclid(86_400_000), ms.rem_euclid(86_400_000));
    // Howard Hinnant's civil_from_days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    let (h, m, s, milli) = (rem / 3_600_000, rem / 60_000 % 60, rem / 1000 % 60, rem % 1000);
    format!("{year:04}-{month:02}-{day:02}T{h:02}:{m:02}:{s:02}.{milli:03}Z")
}

/// The saved snapshot `id` in a read list, with its terminals (`getSnapshot`).
#[must_use]
pub fn saved(list: &Value, id: &str) -> Option<Value> {
    list.get("snapshots")?.as_array()?.iter().find(|s| s["id"] == json!(id)).cloned()
}

#[cfg(test)]
mod tests;
