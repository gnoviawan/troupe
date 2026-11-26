//! The terminals the client holds (`stores/terminal-store.ts`, the web terminal set): which PTY
//! each terminal tab shows, the project it was spawned for, its size and whether it still runs.
//!
//! A session outlives navigation. Switching projects hides a project's terminal tabs but keeps
//! their sessions here, attached and fed, so switching back shows the same process; only a
//! close or the PTY's own exit ends one. The cells live in the app beside each session.
//!
//! The command editor's state is in the submodules: [`session`] and [`store`] the shell session
//! the host's lifecycle frames drive and the draft, [`completion`] when completions are asked
//! for, [`words`] how a candidate is spelled, [`routes`] the `/terminal/ws` session calls, and
//! [`history`] the commands sent. `tools/parity/vectors/terminal.ts` records their vectors.

use std::collections::{BTreeMap, BTreeSet};

pub mod completion;
pub mod history;
pub mod routes;
pub mod session;
pub mod store;
pub mod words;

/// Where a terminal stands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Status {
    /// Spawned or re-attached; output flows.
    Live,
    /// The PTY exited.
    Exited { code: Option<i32> },
    /// The host refused the claim (it restarted, or the claim was revoked).
    Lost { reason: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Session {
    /// The PTY id the host issued; the terminal tab's `terminalId`.
    pub id: String,
    pub project_id: String,
    pub shell: String,
    pub cwd: String,
    pub pid: u32,
    pub cols: u16,
    pub rows: u16,
    /// The credential that attaches this session.
    pub claim: String,
    pub status: Status,
    pub title: String,
}

/// A `spawn` in flight, keyed by its call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Spawning {
    pub project_id: String,
    /// The pane whose `+` menu asked, where the tab opens.
    pub pane: Option<String>,
}

/// A `list_preserved` in flight for a terminal tab a saved layout brought back.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Restoring {
    pub project_id: String,
    pub terminal_id: String,
}

/// What a `spawn` reply carried (`SpawnedTerminal`), or a `list_preserved` entry with a claim.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Spawned {
    pub id: String,
    pub shell: String,
    pub cwd: String,
    pub pid: u32,
    pub cols: u16,
    pub rows: u16,
    pub claim: String,
}

/// The size a terminal opens at before its pane has been measured, as xterm.js starts.
pub const DEFAULT_SIZE: (u16, u16) = (80, 24);

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Terminals {
    pub sessions: BTreeMap<String, Session>,
    pub spawning: BTreeMap<u64, Spawning>,
    pub restoring: BTreeMap<u64, Restoring>,
    /// Restored tabs already asked about, so each is asked once.
    asked: BTreeSet<String>,
}

impl Terminals {
    /// Notes a `spawn` sent as `call`.
    pub fn spawn(&mut self, call: u64, project_id: &str, pane: Option<String>) {
        self.spawning.insert(call, Spawning { project_id: project_id.to_owned(), pane });
    }

    /// The `spawn` reply for `call`: the new session and the pane its tab opens in. A reply
    /// to a call this store did not send is ignored.
    pub fn spawned(&mut self, call: u64, spawned: Spawned) -> Option<(&Session, Option<String>)> {
        let Spawning { project_id, pane } = self.spawning.remove(&call)?;
        let id = spawned.id.clone();
        let session = Session {
            id: spawned.id,
            project_id,
            shell: spawned.shell,
            cwd: spawned.cwd,
            pid: spawned.pid,
            cols: spawned.cols,
            rows: spawned.rows,
            claim: spawned.claim,
            status: Status::Live,
            title: String::new(),
        };
        self.sessions.insert(id.clone(), session);
        self.sessions.get(&id).map(|s| (s, pane))
    }

    /// Whether a restored tab showing `terminal_id` should ask the host for its PTY: not for a
    /// session this client holds, nor twice.
    pub fn should_restore(&self, terminal_id: &str) -> bool {
        !self.sessions.contains_key(terminal_id) && !self.asked.contains(terminal_id)
    }

    /// Notes a `list_preserved` sent as `call` for the restored tab showing `terminal_id`.
    pub fn restore(&mut self, call: u64, project_id: &str, terminal_id: &str) {
        self.asked.insert(terminal_id.to_owned());
        self.restoring.insert(call, Restoring { project_id: project_id.to_owned(), terminal_id: terminal_id.to_owned() });
    }

    /// The `list_preserved` reply for `call`: the PTY it found with a fresh claim becomes this
    /// client's session, as React re-attaches a preserved terminal. `found` is None when the
    /// host no longer runs it or would not issue a claim.
    pub fn restored(&mut self, call: u64, found: Option<Spawned>) -> Option<&Session> {
        let Restoring { project_id, terminal_id } = self.restoring.remove(&call)?;
        let spawned = found.filter(|s| s.id == terminal_id && !s.claim.is_empty())?;
        self.spawning.insert(call, Spawning { project_id, pane: None });
        self.spawned(call, spawned).map(|(session, _)| session)
    }

    /// The `spawn` for `call` failed.
    pub fn spawn_failed(&mut self, call: u64) -> Option<Spawning> {
        self.spawning.remove(&call)
    }

    /// The pane was measured at `cols` x `rows`: true when the PTY needs a `resize`.
    pub fn resize(&mut self, id: &str, cols: u16, rows: u16) -> bool {
        match self.sessions.get_mut(id) {
            Some(s) if s.status == Status::Live && (s.cols, s.rows) != (cols, rows) && cols > 0 && rows > 0 => {
                s.cols = cols;
                s.rows = rows;
                true
            }
            _ => false,
        }
    }

    pub fn exited(&mut self, id: &str, code: Option<i32>) {
        if let Some(s) = self.sessions.get_mut(id) {
            s.status = Status::Exited { code };
        }
    }

    pub fn lost(&mut self, id: &str, reason: &str) {
        if let Some(s) = self.sessions.get_mut(id) {
            s.status = Status::Lost { reason: reason.to_owned() };
        }
    }

    pub fn cwd_changed(&mut self, id: &str, cwd: &str) {
        if let Some(s) = self.sessions.get_mut(id) {
            s.cwd = cwd.to_owned();
        }
    }

    /// Forgets a closed terminal; the caller kills its PTY when this returns the session. A
    /// `list_preserved` still in flight for its tab is dropped, so its late reply attaches
    /// nothing.
    pub fn close(&mut self, id: &str) -> Option<Session> {
        self.restoring.retain(|_, r| r.terminal_id != id);
        self.sessions.remove(id)
    }

    pub fn get(&self, id: &str) -> Option<&Session> {
        self.sessions.get(id)
    }

    /// The sessions spawned for `project_id`, whether or not its workspace is shown.
    pub fn of_project<'a>(&'a self, project_id: &'a str) -> impl Iterator<Item = &'a Session> + 'a {
        self.sessions.values().filter(move |s| s.project_id == project_id)
    }
}

#[cfg(test)]
mod tests;
