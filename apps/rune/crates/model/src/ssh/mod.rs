//! SSH (`stores/ssh-store.ts`, `lib/ssh-sessions.ts`): saved hosts, the SSH workspace's tabs,
//! one session per tab with its connect attempt, the SFTP transfers and the remote file open
//! in the editor. Desktop only: the host serves SSH to the host-local client alone.
//!
//! The store actions are methods named after the TypeScript ones, so `tests/vectors.rs`
//! replays what `tools/parity/vectors/stores/ssh.ts` recorded. The session flow (open, the
//! open's answer, the `ssh` watch's events, prompt answers, cancel and close) returns
//! [`Effect`]s: the host request to send and the terminal to attach or release. Nothing here
//! holds a secret: a password or passphrase goes straight from the form into a request.

pub mod messages;
pub mod prompt;
pub mod sftp;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub use prompt::{CredentialPrompt, HostKeyPrompt, credential_prompt_for};

/// The Hosts tab of the workspace; every other tab is a session tab id.
pub const HOSTS_TAB: &str = "hosts";

/// The PTY size an open asks for; the terminal view resizes it once it shows.
pub const INITIAL_COLS: u16 = 120;
pub const INITIAL_ROWS: u16 = 32;

/// One saved host (`SSHProfile`), without its secrets.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    /// `password`, `key` or `agent`.
    pub auth_method: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub private_key_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jump_host_id: Option<String>,
    #[serde(default)]
    pub port_forwards: Vec<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub favorite: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_connected: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub imported_from: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub has_stored_password: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub has_stored_passphrase: bool,
}

impl Profile {
    /// `user@host`, with `:port` when it is not 22 (`hostLabel`).
    #[must_use]
    pub fn address(&self) -> String {
        if self.port == 22 {
            format!("{}@{}", self.username, self.host)
        } else {
            format!("{}@{}:{}", self.username, self.host, self.port)
        }
    }
}

/// A profile as a save sends it: the stored fields plus a secret typed in the form.
#[must_use]
pub fn save_payload(profile: &Profile, password: Option<&str>, passphrase: Option<&str>) -> Value {
    let mut value = serde_json::to_value(profile).unwrap_or(Value::Null);
    if let Some(fields) = value.as_object_mut() {
        if let Some(password) = password.filter(|p| !p.is_empty()) {
            fields.insert("password".into(), json!(password));
        }
        if let Some(passphrase) = passphrase.filter(|p| !p.is_empty()) {
            fields.insert("passphrase".into(), json!(passphrase));
        }
        fields.insert("portForwards".into(), json!(profile.port_forwards));
    }
    json!({ "profile": value })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Status {
    Connecting,
    /// Paused on the user: a host-key or credential prompt.
    AwaitingInput,
    Connected,
    Disconnected,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionError {
    pub code: String,
    /// Technical detail; never a secret.
    pub detail: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionEnd {
    /// `exited`, `lost` or `closed`.
    pub reason: String,
    pub exit_code: Option<i64>,
    pub detail: Option<String>,
}

/// One session tab (`SSHSession`). A reconnect keeps the tab with a new `attempt_id`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub id: String,
    /// The host's session id of the current connect.
    pub attempt_id: String,
    pub profile_id: String,
    pub status: Status,
    pub stage: Option<String>,
    pub host_key_prompt: Option<HostKeyPrompt>,
    pub credential_prompt: Option<CredentialPrompt>,
    pub error: Option<SessionError>,
    pub ended: Option<SessionEnd>,
    /// The host terminal of the remote shell.
    pub pty_id: Option<String>,
    /// The attach credential for `pty_id`, in memory only.
    pub claim: Option<String>,
    pub attached: bool,
    pub connected_at: Option<String>,
}

impl Session {
    /// A fresh tab connecting `profile_id`.
    #[must_use]
    pub fn connecting(id: &str, profile_id: &str) -> Session {
        Session {
            id: id.to_owned(),
            attempt_id: id.to_owned(),
            profile_id: profile_id.to_owned(),
            status: Status::Connecting,
            stage: Some("connecting".into()),
            host_key_prompt: None,
            credential_prompt: None,
            error: None,
            ended: None,
            pty_id: None,
            claim: None,
            attached: false,
            connected_at: None,
        }
    }
}

/// Progress of one upload or download (`SFTPTransferProgress`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Transfer {
    pub connection_id: String,
    pub remote_path: String,
    pub local_path: String,
    pub bytes_transferred: u64,
    pub total_bytes: u64,
    /// `upload` or `download`.
    pub direction: String,
    /// `in-progress`, `completed`, `failed` or `cancelled`.
    pub status: String,
    #[serde(default)]
    pub error: Option<String>,
}

impl Transfer {
    #[must_use]
    pub fn running(&self) -> bool {
        self.status == "in-progress"
    }
}

/// A remote file open in the editor, bound to the session it came from.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditingFile {
    pub connection_id: String,
    pub path: String,
    pub name: String,
    pub content: String,
    pub original_content: String,
}

/// The store (`SSHState`, its data fields).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct State {
    pub profiles: Vec<Profile>,
    pub is_loaded: bool,
    pub workspace_open: bool,
    pub active_tab: String,
    pub selected_profile_id: Option<String>,
    pub show_addresses: bool,
    pub sessions: Vec<Session>,
    pub pending_close: Option<String>,
    pub transfers: Vec<Transfer>,
    pub editing_file: Option<EditingFile>,
    pub editing_content: String,
}

impl Default for State {
    fn default() -> Self {
        State {
            profiles: Vec::new(),
            is_loaded: false,
            workspace_open: false,
            active_tab: HOSTS_TAB.into(),
            selected_profile_id: None,
            show_addresses: false,
            sessions: Vec::new(),
            pending_close: None,
            transfers: Vec::new(),
            editing_file: None,
            editing_content: String::new(),
        }
    }
}

/// What a session step asks the app to do.
#[derive(Clone, Debug, PartialEq)]
pub enum Effect {
    /// Send a host request. `ledgered` ones carry a command id.
    Request { kind: &'static str, payload: Value, ledgered: bool },
    /// Attach the terminal view to a session's shell.
    Attach { tab: String, pty_id: String, claim: String },
    /// Drop the view of a terminal and kill it on the host.
    Release { pty_id: String },
    /// The keychain saved a typed secret: read the hosts again for the "saved" flag.
    ReloadProfiles,
    Log { warn: bool, message: String },
}

/// What `ssh_session_open` answered: the session and its terminal, or a refusal.
#[derive(Clone, Debug, PartialEq)]
pub enum Opened {
    Ok { pty_id: String, claim: String, connected_at: Option<String>, secret_saved: bool },
    Err(SessionError),
}

impl State {
    // ---- store actions (`useSSHStore`) ----

    /// `loadProfiles`' success: the hosts, never with secrets.
    pub fn set_profiles(&mut self, profiles: Vec<Profile>) {
        self.profiles = profiles;
        self.is_loaded = true;
    }

    /// `saveProfile`'s success. The keychain flags come from the host's next list.
    pub fn profile_saved(&mut self, profile: Profile) {
        match self.profiles.iter_mut().find(|p| p.id == profile.id) {
            Some(slot) => *slot = profile,
            None => self.profiles.push(profile),
        }
    }

    /// `deleteProfile`'s success.
    pub fn profile_deleted(&mut self, profile_id: &str) {
        self.profiles.retain(|p| p.id != profile_id);
        if self.selected_profile_id.as_deref() == Some(profile_id) {
            self.selected_profile_id = None;
        }
    }

    /// `duplicateProfile`'s success: the copy is listed and selected.
    pub fn profile_duplicated(&mut self, copy: Profile) {
        self.selected_profile_id = Some(copy.id.clone());
        self.profiles.push(copy);
    }

    /// `importConfig`'s success: imported hosts replace those with the same id.
    pub fn profiles_imported(&mut self, imported: Vec<Profile>) {
        for profile in imported {
            self.profile_saved(profile);
        }
    }

    pub fn select_profile(&mut self, profile_id: Option<&str>) {
        let Some(profile_id) = profile_id else {
            self.workspace_open = false;
            return;
        };
        let latest = self.sessions.iter().rev().find(|s| s.profile_id == profile_id);
        self.active_tab = latest.map_or_else(|| HOSTS_TAB.to_owned(), |s| s.id.clone());
        self.workspace_open = true;
        self.selected_profile_id = Some(profile_id.to_owned());
    }

    pub fn open_workspace(&mut self) {
        self.workspace_open = true;
    }

    pub fn set_active_tab(&mut self, tab: &str) {
        let session = self.sessions.iter().find(|s| s.id == tab).map(|s| s.profile_id.clone());
        self.workspace_open = true;
        self.active_tab = if session.is_some() || tab == HOSTS_TAB { tab.to_owned() } else { HOSTS_TAB.to_owned() };
        if let Some(profile) = session {
            self.selected_profile_id = Some(profile);
        }
    }

    pub fn set_pending_close(&mut self, tab: Option<&str>) {
        self.pending_close = tab.map(str::to_owned);
    }

    pub fn add_session(&mut self, session: Session) {
        self.sessions.push(session);
    }

    /// `removeSession`: the neighbour tab takes focus, like closing a browser tab.
    pub fn remove_session(&mut self, tab: &str) {
        let Some(index) = self.sessions.iter().position(|s| s.id == tab) else { return };
        self.sessions.remove(index);
        if self.active_tab != tab {
            return;
        }
        let next = self.sessions.get(index).or_else(|| index.checked_sub(1).and_then(|i| self.sessions.get(i)));
        self.active_tab = next.map_or_else(|| HOSTS_TAB.to_owned(), |s| s.id.clone());
    }

    pub fn toggle_show_addresses(&mut self) {
        self.show_addresses = !self.show_addresses;
    }

    pub fn set_editing_file(&mut self, file: Option<EditingFile>) {
        self.editing_file = file;
    }

    pub fn set_editing_content(&mut self, content: &str) {
        content.clone_into(&mut self.editing_content);
    }

    /// `updateTransferProgress`: one row per session, remote path and direction.
    pub fn update_transfer(&mut self, progress: Transfer) {
        let same = |t: &Transfer| {
            t.connection_id == progress.connection_id && t.remote_path == progress.remote_path && t.direction == progress.direction
        };
        match self.transfers.iter_mut().find(|t| same(t)) {
            Some(slot) => *slot = progress,
            None => self.transfers.push(progress),
        }
    }

    /// `clearCompletedTransfers`: finished rows go; only those of `connection_id` when given.
    pub fn clear_completed_transfers(&mut self, connection_id: Option<&str>) {
        self.transfers.retain(|t| t.running() || connection_id.is_some_and(|c| t.connection_id != c));
    }

    // ---- selectors ----

    #[must_use]
    pub fn session(&self, tab: &str) -> Option<&Session> {
        self.sessions.iter().find(|s| s.id == tab)
    }

    fn session_mut(&mut self, tab: &str) -> Option<&mut Session> {
        self.sessions.iter_mut().find(|s| s.id == tab)
    }

    fn by_attempt(&mut self, attempt: &str) -> Option<&mut Session> {
        self.sessions.iter_mut().find(|s| s.attempt_id == attempt)
    }

    #[must_use]
    pub fn profile(&self, id: &str) -> Option<&Profile> {
        self.profiles.iter().find(|p| p.id == id)
    }

    /// `selectActiveSession`.
    #[must_use]
    pub fn active_session(&self) -> Option<&Session> {
        if !self.workspace_open {
            return None;
        }
        self.session(&self.active_tab)
    }

    /// `selectActiveProfileId`.
    #[must_use]
    pub fn active_profile_id(&self) -> Option<&str> {
        if !self.workspace_open {
            return None;
        }
        self.active_session().map(|s| s.profile_id.as_str()).or(self.selected_profile_id.as_deref())
    }

    /// Sessions of one host, oldest first.
    pub fn sessions_of<'a>(&'a self, profile_id: &'a str) -> impl Iterator<Item = &'a Session> {
        self.sessions.iter().filter(move |s| s.profile_id == profile_id)
    }

    // ---- the session flow (`lib/ssh-sessions.ts`) ----

    /// `openSSHSession`: a new tab for a host, focused, and its first connect.
    pub fn open_session(&mut self, tab: &str, profile_id: &str) -> Vec<Effect> {
        self.add_session(Session::connecting(tab, profile_id));
        self.set_active_tab(tab);
        self.attempt(tab, tab, None, false)
    }

    /// `runAttempt`'s start: the tab connects again under `attempt`.
    fn attempt(&mut self, tab: &str, attempt: &str, secret: Option<&str>, save_secret: bool) -> Vec<Effect> {
        let Some(session) = self.session_mut(tab) else { return Vec::new() };
        let fresh = Session::connecting(tab, &session.profile_id);
        *session = Session { attempt_id: attempt.to_owned(), ..fresh };
        let mut request = json!({
            "sessionId": attempt,
            "profileId": session.profile_id,
            "cols": INITIAL_COLS,
            "rows": INITIAL_ROWS,
        });
        if let Some(secret) = secret.filter(|s| !s.is_empty()) {
            request["secret"] = json!(secret);
            request["saveSecret"] = json!(save_secret);
        }
        vec![Effect::Request { kind: "ssh_session_open", payload: json!({ "request": request }), ledgered: true }]
    }

    /// `retrySSHSession`: connect a failed, prompted or ended tab again, releasing its shell.
    pub fn retry(&mut self, tab: &str, attempt: &str, secret: Option<&str>, save_secret: bool) -> Vec<Effect> {
        let Some(session) = self.session(tab) else { return Vec::new() };
        if matches!(session.status, Status::Connecting | Status::Connected) {
            return Vec::new();
        }
        let mut effects: Vec<Effect> = session.pty_id.clone().map(|pty_id| Effect::Release { pty_id }).into_iter().collect();
        effects.extend(self.attempt(tab, attempt, secret, save_secret));
        effects
    }

    /// What the open answered, for the attempt it was sent under.
    pub fn opened(&mut self, attempt: &str, answer: Opened) -> Vec<Effect> {
        let auth = self
            .sessions
            .iter()
            .find(|s| s.attempt_id == attempt)
            .and_then(|s| self.profile(&s.profile_id))
            .map(|p| p.auth_method.clone());
        let Some(session) = self.by_attempt(attempt) else {
            // The tab closed or connected again meanwhile: drop this shell.
            return match answer {
                Opened::Ok { pty_id, .. } => vec![
                    Effect::Request { kind: "ssh_session_close", payload: json!({ "sessionId": attempt }), ledgered: false },
                    Effect::Release { pty_id },
                ],
                Opened::Err(_) => Vec::new(),
            };
        };
        match answer {
            Opened::Err(error) => {
                let prompt = credential_prompt_for(&error.code, auth.as_deref());
                let message = format!("SSH session for profile {} failed: {}", session.profile_id, error.code);
                session.status = if prompt.is_some() { Status::AwaitingInput } else { Status::Failed };
                session.stage = None;
                session.host_key_prompt = None;
                session.credential_prompt = prompt;
                let warn = error.code != "SSH_CANCELLED";
                session.error = Some(error);
                vec![Effect::Log { warn, message }]
            }
            Opened::Ok { pty_id, claim, connected_at, secret_saved } => {
                // An `ended` event can beat the answer for a shell that exits at once.
                session.status = if session.ended.is_some() { Status::Disconnected } else { Status::Connected };
                session.stage = None;
                session.pty_id = Some(pty_id.clone());
                session.claim = Some(claim.clone());
                session.connected_at = connected_at;
                session.attached = true;
                let mut effects = vec![Effect::Attach { tab: session.id.clone(), pty_id, claim }];
                if secret_saved {
                    effects.push(Effect::ReloadProfiles);
                }
                effects
            }
        }
    }

    /// An `ssh` watch `progress` delta.
    pub fn progressed(&mut self, attempt: &str, stage: &str) {
        if let Some(session) = self.by_attempt(attempt).filter(|s| s.status == Status::Connecting) {
            session.stage = Some(stage.to_owned());
        }
    }

    /// A host-key question (a `host_key` delta, or one listed in the watch state). Nobody can
    /// answer one for an unknown attempt, so it is rejected and the connect fails closed.
    pub fn host_key_asked(&mut self, prompt: HostKeyPrompt) -> Vec<Effect> {
        let attempt = prompt.session_id.clone();
        match self.by_attempt(&attempt) {
            Some(session) => {
                if session.host_key_prompt.as_ref() != Some(&prompt) {
                    session.status = Status::AwaitingInput;
                    session.host_key_prompt = Some(prompt);
                }
                Vec::new()
            }
            None => vec![Effect::Request {
                kind: "ssh_session_respond_host_key",
                payload: json!({ "sessionId": attempt, "trust": false }),
                ledgered: true,
            }],
        }
    }

    /// A `host_key_resolved` delta: the question is gone (answered elsewhere, cancelled or
    /// timed out); the open's answer follows.
    pub fn host_key_resolved(&mut self, attempt: &str) {
        if let Some(session) = self.by_attempt(attempt).filter(|s| s.host_key_prompt.is_some()) {
            session.host_key_prompt = None;
            session.status = Status::Connecting;
        }
    }

    /// `respondSSHHostKey`. Trusting names the fingerprint the prompt showed.
    pub fn respond_host_key(&mut self, tab: &str, trust: bool) -> Vec<Effect> {
        let Some(session) = self.session_mut(tab) else { return Vec::new() };
        let Some(prompt) = session.host_key_prompt.take() else { return Vec::new() };
        session.status = Status::Connecting;
        let mut payload = json!({ "sessionId": session.attempt_id, "trust": trust });
        if trust {
            payload["fingerprint"] = json!(prompt.fingerprint);
        }
        vec![Effect::Request { kind: "ssh_session_respond_host_key", payload, ledgered: true }]
    }

    /// An `ended` delta.
    pub fn ended(&mut self, attempt: &str, end: SessionEnd) -> Vec<Effect> {
        let Some(session) = self.by_attempt(attempt) else { return Vec::new() };
        session.status = Status::Disconnected;
        let lost = end.reason == "lost";
        session.ended = Some(end);
        if lost {
            vec![Effect::Log { warn: true, message: format!("SSH session {attempt} lost its connection") }]
        } else {
            Vec::new()
        }
    }

    /// `cancelSSHSession`.
    pub fn cancel(&mut self, tab: &str) -> Vec<Effect> {
        let Some(session) = self.session_mut(tab) else { return Vec::new() };
        if session.status == Status::Connecting || session.host_key_prompt.is_some() {
            return vec![Effect::Request {
                kind: "ssh_session_cancel",
                payload: json!({ "sessionId": session.attempt_id }),
                ledgered: false,
            }];
        }
        if session.status == Status::AwaitingInput {
            session.status = Status::Failed;
            session.credential_prompt = None;
            if session.error.is_none() {
                session.error = Some(SessionError { code: "SSH_CANCELLED".into(), detail: "Cancelled".into() });
            }
        }
        Vec::new()
    }

    /// `requestCloseSSHSession`: a live shell asks first.
    pub fn request_close(&mut self, tab: &str) -> Vec<Effect> {
        match self.session(tab).map(|s| s.status) {
            Some(Status::Connected) => {
                self.set_pending_close(Some(tab));
                Vec::new()
            }
            Some(_) => self.close(tab),
            None => Vec::new(),
        }
    }

    /// `closeSSHSession`: the tab goes, with its connect, shell and terminal.
    pub fn close(&mut self, tab: &str) -> Vec<Effect> {
        let Some(session) = self.session(tab).cloned() else { return Vec::new() };
        self.remove_session(tab);
        if self.pending_close.as_deref() == Some(tab) {
            self.pending_close = None;
        }
        let mut effects = Vec::new();
        let id = json!({ "sessionId": session.attempt_id });
        match session.status {
            Status::Connecting | Status::AwaitingInput => {
                effects.push(Effect::Request { kind: "ssh_session_cancel", payload: id, ledgered: false });
            }
            Status::Connected => effects.push(Effect::Request { kind: "ssh_session_close", payload: id, ledgered: false }),
            Status::Disconnected | Status::Failed => {}
        }
        if let Some(pty_id) = session.pty_id {
            effects.push(Effect::Release { pty_id });
        }
        effects
    }

    /// `cycleSSHTab`.
    pub fn cycle_tab(&mut self, forward: bool) {
        let tabs: Vec<String> = std::iter::once(HOSTS_TAB.to_owned()).chain(self.sessions.iter().map(|s| s.id.clone())).collect();
        let index = tabs.iter().position(|t| *t == self.active_tab).unwrap_or(0);
        let next = if forward { (index + 1) % tabs.len() } else { (index + tabs.len() - 1) % tabs.len() };
        let tab = tabs[next].clone();
        self.set_active_tab(&tab);
    }
}

#[cfg(test)]
mod tests;
