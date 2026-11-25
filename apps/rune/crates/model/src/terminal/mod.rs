//! The terminals the client holds (`stores/terminal-store.ts`, the web terminal set): which PTY
//! each terminal tab shows, the project it was spawned for, its size and whether it still runs.
//!
//! A session outlives navigation. Switching projects hides a project's terminal tabs but keeps
//! their sessions here, attached and fed, so switching back shows the same process; only a
//! close or the PTY's own exit ends one. The cells live in the app beside each session.

use std::collections::BTreeMap;

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

/// What a `spawn` reply carried (`SpawnedTerminal`).
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

    /// Forgets a closed terminal; the caller kills its PTY.
    pub fn close(&mut self, id: &str) -> Option<Session> {
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
