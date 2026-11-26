//! The commands sent from the command editor, per project (`stores/command-history-store.ts`,
//! `hooks/use-command-history.ts`). Newest first, at most [`MAX_ENTRIES`] per project. After
//! each accepted submit the project's list is written to the host store under [`key`], cut to
//! the host's value cap from the oldest end. Arrow Up in the editor walks [`History::recall`].

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::persistence::stored;

pub const MAX_ENTRIES: usize = 500;

/// The largest serialized value the host store accepts (`STORE_VALUE_MAX_BYTES`).
pub const STORE_VALUE_MAX_BYTES: usize = 256 * 1024;

/// One sent command (`CommandHistoryEntry`). Field order is the web client's, so the stored
/// value is the same bytes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub command: String,
    pub terminal_name: String,
    pub terminal_id: String,
    pub project_id: String,
    pub timestamp: i64,
    pub id: String,
}

/// The host store key of a project's history (project scope, shared by every device).
#[must_use]
pub fn key(project_id: &str) -> String {
    format!("projects/{project_id}/command-history")
}

/// A write of one project's list.
#[derive(Clone, Debug, PartialEq)]
pub struct Write {
    pub key: String,
    pub value: Value,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct History {
    /// Every project's entries, each project's newest first.
    pub entries: Vec<Entry>,
}

impl History {
    /// Records a sent command (`useAddCommand`). Blank commands are not kept. Returns the write
    /// the project's list needs.
    pub fn add(&mut self, command: &str, terminal_name: &str, terminal_id: &str, project_id: &str, timestamp: i64, id: String) -> Option<Write> {
        let command = command.trim();
        if command.is_empty() {
            return None;
        }
        let entry = Entry {
            command: command.to_owned(),
            terminal_name: terminal_name.to_owned(),
            terminal_id: terminal_id.to_owned(),
            project_id: project_id.to_owned(),
            timestamp,
            id,
        };
        let (mine, others): (Vec<Entry>, Vec<Entry>) = std::mem::take(&mut self.entries).into_iter().partition(|e| e.project_id == project_id);
        let mut entries: Vec<Entry> = std::iter::once(entry).chain(mine).take(MAX_ENTRIES).collect();
        entries.extend(others);
        self.entries = entries;
        Some(self.write(project_id))
    }

    /// The project's list as written: the newest entries that fit the host's value cap.
    #[must_use]
    pub fn write(&self, project_id: &str) -> Write {
        let mine: Vec<&Entry> = self.of_project(project_id).collect();
        let kept = newest_within_cap(&mine);
        Write { key: key(project_id), value: stored(json!(kept)) }
    }

    /// A project's list read from the host store (`useCommandHistoryLoader`): it replaces that
    /// project's entries in front of the other projects'.
    pub fn loaded(&mut self, project_id: &str, entries: Vec<Entry>) {
        self.entries.retain(|e| e.project_id != project_id);
        let others = std::mem::take(&mut self.entries);
        self.entries = entries;
        self.entries.extend(others);
    }

    fn of_project<'a>(&'a self, project_id: &'a str) -> impl Iterator<Item = &'a Entry> + 'a {
        self.entries.iter().filter(move |e| e.project_id == project_id)
    }

    /// What Arrow Up walks for a project, oldest first: the editor recalls from the end.
    #[must_use]
    pub fn recall(&self, project_id: &str) -> Vec<String> {
        let mut commands: Vec<String> = self.of_project(project_id).map(|e| e.command.clone()).collect();
        commands.reverse();
        commands
    }
}

/// The longest newest-first run whose stored value (`{"_version":1,"data":[...]}`) fits the
/// cap (`trimListToCap(..., 'head')`). `serde_json` writes the bytes `JSON.stringify` does, so
/// the length is the sum of the entries' lengths, the commas between them and the envelope.
fn newest_within_cap<'a>(entries: &[&'a Entry]) -> Vec<&'a Entry> {
    let envelope = serde_json::to_string(&stored(json!([]))).map_or(0, |s| s.len());
    let mut bytes = envelope;
    let mut kept = 0;
    for (i, entry) in entries.iter().enumerate() {
        bytes += serde_json::to_string(entry).map_or(usize::MAX / 2, |s| s.len()) + usize::from(i > 0);
        if bytes > STORE_VALUE_MAX_BYTES {
            break;
        }
        kept += 1;
    }
    entries[..kept].to_vec()
}

/// What the editor recalls on Arrow Up and Down (`recallHandler` in `editor-keys.ts`): the
/// entries that start with the first line of the draft the walk began from, newest last.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Recall {
    /// The draft before the walk began, given back when the walk comes back past it.
    pub draft: String,
    pub prefix: String,
    /// How far back from the newest match the editor shows; -1 is the draft itself.
    pub offset: i64,
}

impl Recall {
    /// A walk begun from `draft`.
    #[must_use]
    pub fn begin(draft: &str) -> Recall {
        let prefix = draft.split('\n').next().unwrap_or("").to_owned();
        Recall { draft: draft.to_owned(), prefix, offset: -1 }
    }

    /// One step back (`direction` -1, Arrow Up) or forward (1). Returns the text to show and
    /// whether the walk goes on; `None` when the step goes nowhere (past the oldest match).
    pub fn step(&mut self, history: &[String], direction: i64) -> Option<(String, bool)> {
        let matches: Vec<&String> = history.iter().filter(|h| h.starts_with(&self.prefix)).collect();
        let offset = self.offset - direction;
        if offset >= matches.len() as i64 {
            return None;
        }
        if offset < 0 {
            return Some((self.draft.clone(), false));
        }
        self.offset = offset;
        let index = matches.len() - 1 - offset as usize;
        Some((matches[index].clone(), true))
    }
}
