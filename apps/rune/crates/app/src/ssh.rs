//! SSH in the desktop app (`components/ssh/`, `lib/ssh-sessions.ts`): the host side of
//! `model::ssh`. The hosts, sessions and SFTP requests go to the host over `/ws`; session
//! progress, host-key questions, session ends and transfer progress come back on the `ssh`
//! watch. Only the desktop app's own server serves them (the host-local credential), so a web
//! client never shows SSH. Secrets typed in a form go straight into one request and are
//! cleared; nothing here logs a payload.

use std::collections::BTreeMap;

use model::ssh::sftp::{self, Entry};
use model::ssh::{Effect, HostKeyPrompt, Opened, Profile, SessionEnd, SessionError, Transfer};
use rune::{FileDialogOptions, TextEdit, Tx};
use serde_json::{Value, json};
use troupe_protocol::ws::{WsError, WsRequestType, WsWatchName};
use ui::overlay::ToastKind;

use crate::State;
use crate::sync::Pending;

/// Overlay keys.
pub const EDITOR: &str = "ssh-host-editor";
pub const DELETE_HOST: &str = "ssh-delete-host";
pub const CLOSE_SESSION: &str = "ssh-close-session";
pub const REMOTE_CONFIRM: &str = "ssh-remote-confirm";
pub const NAME_DIALOG: &str = "ssh-name-dialog";
pub const FILE_EDITOR: &str = "ssh-file-editor";

/// What a host reply answers.
#[derive(Clone, Debug, PartialEq)]
pub enum Call {
    Profiles,
    Save { profile: Box<Profile>, connect: bool },
    Delete { id: String, name: String },
    Duplicate(String),
    Import,
    Open(String),
    Sessions,
    /// A reply that only needs a failure shown.
    Quiet(&'static str),
    List { conn: String, path: String },
    Home(String),
    Transfer { conn: String, remote: String, local: String, upload: bool },
    /// A remote change: the toast for a failure (`Couldn't ...`) and for success.
    Changed { conn: String, what: String, done: String },
    Read { conn: String, path: String, name: String },
    Write { conn: String, path: String },
}

/// A destructive SFTP action waiting for its confirm, with its target captured when it opened.
#[derive(Clone, Debug, PartialEq)]
pub enum RemoteConfirm {
    Delete { conn: String, entry: Entry, host: String },
    Replace { conn: String, dir: String, locals: Vec<String>, conflicts: Vec<String>, host: String },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NameKind {
    Folder,
    File,
    Rename,
}

/// One session's explorer.
#[derive(Clone, Debug, Default)]
pub struct Explorer {
    pub cwd: String,
    pub entries: Vec<Entry>,
    pub loading: bool,
    pub error: Option<String>,
}

/// The host editor's fields (`SSHHostEditor`).
#[derive(Default)]
pub struct HostForm {
    pub id: Option<String>,
    pub name: TextEdit,
    pub host: TextEdit,
    pub port: TextEdit,
    pub username: TextEdit,
    pub auth: String,
    pub key_path: TextEdit,
    pub password: TextEdit,
    pub passphrase: TextEdit,
    pub error: Option<String>,
    pub saving: bool,
}

#[derive(Default)]
pub struct SshUi {
    pub model: model::ssh::State,
    pub form: HostForm,
    /// The credential prompt's field and its "save to keychain" box.
    pub secret: TextEdit,
    pub save_secret: bool,
    /// The host the delete confirm names: id and name, captured when it opened.
    pub delete_host: Option<(String, String)>,
    pub explorers: BTreeMap<String, Explorer>,
    /// Whether the active session shows its files beside the shell.
    pub files_open: bool,
    pub confirm: Option<RemoteConfirm>,
    pub naming: Option<(NameKind, String, Option<Entry>)>,
    pub name: TextEdit,
    pub file_text: TextEdit,
    attempts: u64,
}

fn edit(text: &str) -> TextEdit {
    TextEdit::new(text)
}

fn failure(result: &Result<Option<Value>, WsError>) -> Option<(String, String)> {
    result.as_ref().err().map(|e| (e.code.clone(), e.message.clone()))
}

impl State {
    /// SSH is served to the desktop app's own server only.
    pub(crate) fn ssh_available(&self) -> bool {
        self.first_run.config.as_ref().is_some_and(|c| c.host_local.is_some())
    }

    /// A fresh link: watch the SSH events, read the hosts and the live sessions.
    pub(crate) fn ssh_begin(&mut self) {
        if !self.ssh_available() {
            return;
        }
        self.send(client::ws::Command::Watch { watch: WsWatchName::Ssh, scope: String::new() });
        self.call(WsRequestType::SshListProfiles, json!({}), Pending::Ssh(Call::Profiles));
        self.call(WsRequestType::SshSessionList, json!({}), Pending::Ssh(Call::Sessions));
    }

    fn ssh_attempt_id(&mut self) -> String {
        self.ssh.attempts += 1;
        format!("ssh-{:x}-{}", (self.now * 1000.0) as u64, self.ssh.attempts)
    }

    pub(crate) fn ssh_run(&mut self, effects: Vec<Effect>) {
        for effect in effects {
            match effect {
                Effect::Request { kind, payload, ledgered } => {
                    let Some(kind) = request_type(kind) else { continue };
                    let pending = match kind {
                        WsRequestType::SshSessionOpen => {
                            Pending::Ssh(Call::Open(payload["request"]["sessionId"].as_str().unwrap_or_default().to_owned()))
                        }
                        _ => Pending::Ssh(Call::Quiet("SSH request failed")),
                    };
                    if ledgered {
                        self.command(kind, payload, pending);
                    } else {
                        self.call(kind, payload, pending);
                    }
                }
                Effect::Attach { tab, pty_id, claim } => {
                    let name = self
                        .ssh
                        .model
                        .session(&tab)
                        .and_then(|s| self.ssh.model.profile(&s.profile_id))
                        .map_or_else(|| "host".to_owned(), |p| p.name.clone());
                    self.attach_ssh_terminal(&pty_id, &claim, format!("SSH: {name}"));
                }
                Effect::Release { pty_id } => self.release_ssh_terminal(&pty_id),
                Effect::ReloadProfiles => {
                    self.call(WsRequestType::SshListProfiles, json!({}), Pending::Ssh(Call::Profiles));
                }
                Effect::Log { warn: true, message } => log::warn!("[ssh] {message}"),
                Effect::Log { message, .. } => log::info!("[ssh] {message}"),
            }
        }
    }

    /// The `ssh` watch: the open questions (state) or one event (delta).
    pub(crate) fn ssh_watched(&mut self, state: Option<Value>, delta: Option<Value>) {
        if let Some(state) = state {
            let prompts: Vec<HostKeyPrompt> = serde_json::from_value(state["hostKeys"].clone()).unwrap_or_default();
            for prompt in prompts {
                let effects = self.ssh.model.host_key_asked(prompt);
                self.ssh_run(effects);
            }
            return;
        }
        let Some(delta) = delta else { return };
        let attempt = delta["sessionId"].as_str().unwrap_or_default().to_owned();
        match delta["event"].as_str().unwrap_or_default() {
            "progress" => self.ssh.model.progressed(&attempt, delta["stage"].as_str().unwrap_or_default()),
            "host_key" => match serde_json::from_value::<HostKeyPrompt>(delta) {
                Ok(prompt) => {
                    let effects = self.ssh.model.host_key_asked(prompt);
                    self.ssh_run(effects);
                }
                Err(e) => log::warn!("[ssh] unreadable host-key question: {e}"),
            },
            "host_key_resolved" => self.ssh.model.host_key_resolved(&attempt),
            "ended" => {
                let end: Result<SessionEnd, _> = serde_json::from_value(delta);
                if let Ok(end) = end {
                    let effects = self.ssh.model.ended(&attempt, end);
                    self.ssh_run(effects);
                }
            }
            "transfer" => {
                if let Ok(progress) = serde_json::from_value::<Transfer>(delta) {
                    self.ssh.model.update_transfer(progress);
                }
            }
            other => log::info!("[ssh] watch event {other} ignored"),
        }
    }

    pub(crate) fn ssh_replied(&mut self, call: Call, result: Result<Option<Value>, WsError>) {
        let failed = failure(&result);
        let data = result.ok().flatten().unwrap_or(Value::Null);
        match call {
            Call::Profiles => match serde_json::from_value::<Vec<Profile>>(data) {
                Ok(profiles) if failed.is_none() => self.ssh.model.set_profiles(profiles),
                _ => {
                    self.ssh.model.is_loaded = true;
                    if let Some((code, _)) = failed {
                        log::warn!("[ssh] listing hosts failed: {code}");
                    }
                }
            },
            Call::Save { profile, connect } => {
                self.ssh.form.saving = false;
                if let Some((code, message)) = failed {
                    log::warn!("[ssh] saving a host failed: {code}");
                    self.ssh.form.error = Some(message);
                    return;
                }
                let id = profile.id.clone();
                self.ssh.model.profile_saved(*profile);
                self.close(EDITOR);
                self.call(WsRequestType::SshListProfiles, json!({}), Pending::Ssh(Call::Profiles));
                if connect {
                    self.ssh_connect(&id);
                }
            }
            Call::Delete { id, name } => match failed {
                Some((code, _)) => {
                    log::warn!("[ssh] deleting host {id} failed: {code}");
                    self.toast_error(&format!("Could not delete “{name}”. Try again."), "");
                }
                None => {
                    self.ssh.model.profile_deleted(&id);
                    let tabs: Vec<String> = self.ssh.model.sessions_of(&id).map(|s| s.id.clone()).collect();
                    for tab in tabs {
                        let effects = self.ssh.model.close(&tab);
                        self.ssh_run(effects);
                    }
                    self.toasts.push(self.now, ToastKind::Success, format!("Deleted “{name}”"), None);
                }
            },
            Call::Duplicate(name) => match serde_json::from_value::<Profile>(data) {
                Ok(copy) if failed.is_none() => {
                    let message = format!("Duplicated as “{}”", copy.name);
                    self.ssh.model.profile_duplicated(copy);
                    self.toasts.push(self.now, ToastKind::Success, message, None);
                }
                _ => self.toast_error(&format!("Could not duplicate “{name}”"), ""),
            },
            Call::Import => match serde_json::from_value::<Vec<Profile>>(data) {
                Ok(imported) if failed.is_none() => {
                    let count = imported.len();
                    self.ssh.model.profiles_imported(imported);
                    let message = if count == 0 { "No new hosts in ~/.ssh/config".to_owned() } else { format!("Imported {count} hosts") };
                    self.toasts.push(self.now, ToastKind::Success, message, None);
                }
                _ => self.toast_error("Could not import ~/.ssh/config", &failed.map(|f| f.1).unwrap_or_default()),
            },
            Call::Open(attempt) => {
                let answer = match failed {
                    Some((code, detail)) => Opened::Err(SessionError { code, detail }),
                    None => match (data["terminal"]["id"].as_str(), data["terminal"]["claim"].as_str()) {
                        (Some(pty), Some(claim)) => Opened::Ok {
                            pty_id: pty.to_owned(),
                            claim: claim.to_owned(),
                            connected_at: data["session"]["connectedAt"].as_str().map(str::to_owned),
                            secret_saved: data["secretSaved"].as_bool().unwrap_or(false),
                        },
                        // A resend answered from the ledger carries no claim: read it again.
                        _ => {
                            self.call(WsRequestType::SshSessionList, json!({}), Pending::Ssh(Call::Sessions));
                            return;
                        }
                    },
                };
                let effects = self.ssh.model.opened(&attempt, answer);
                self.ssh_run(effects);
            }
            Call::Sessions => self.ssh_restore(&data),
            Call::Quiet(what) => {
                if let Some((code, message)) = failed {
                    log::warn!("[ssh] {what}: {code}");
                    self.toast_error(what, &message);
                }
            }
            Call::List { conn, path } => {
                let explorer = self.ssh.explorers.entry(conn).or_default();
                explorer.loading = false;
                match failed {
                    Some((_, message)) => explorer.error = Some(message),
                    None => {
                        let mut entries: Vec<Entry> = serde_json::from_value(data).unwrap_or_default();
                        sftp::sort_entries(&mut entries);
                        explorer.cwd = path;
                        explorer.entries = entries;
                        explorer.error = None;
                    }
                }
            }
            Call::Home(conn) => match data.as_str().filter(|_| failed.is_none()) {
                Some(home) => {
                    let home = home.to_owned();
                    self.sftp_list(&conn, &home);
                }
                None => {
                    self.sftp_list(&conn, "/");
                }
            },
            Call::Transfer { conn, remote, local, upload } => {
                if upload && failed.as_ref().is_some_and(|(code, _)| code == "SFTP_ALREADY_EXISTS") {
                    let dir = sftp::parent_path(&remote);
                    self.ssh.confirm = Some(RemoteConfirm::Replace {
                        conn: conn.clone(), dir, locals: vec![local.clone()],
                        conflicts: vec![sftp::local_file_name(&remote)], host: self.host_of(&conn),
                    });
                    self.overlays.open(REMOTE_CONFIRM);
                }
                let direction = if upload { "upload" } else { "download" };
                let name = sftp::local_file_name(&remote);
                let result = failed.map_or(Ok(()), |(_, message)| Err(message));
                log::info!("[ssh] SFTP {direction} {}: {name}", if result.is_ok() { "completed" } else { "failed" });
                self.ssh.model.transfer_finished(&conn, &remote, &local, direction, result);
                if upload {
                    self.sftp_refresh(&conn);
                }
            }
            Call::Changed { conn, what, done } => {
                match failed {
                    Some((_, message)) => self.toast_error(&format!("Couldn't {what}"), &message),
                    None => {
                        self.toasts.push(self.now, ToastKind::Success, done, None);
                    }
                }
                self.sftp_refresh(&conn);
            }
            Call::Read { conn, path, name } => match (failed, data.as_str()) {
                (None, Some(content)) => {
                    let file = model::ssh::EditingFile {
                        connection_id: conn,
                        path,
                        name,
                        content: content.to_owned(),
                        original_content: content.to_owned(),
                    };
                    self.ssh.file_text = edit(content);
                    self.ssh.model.set_editing_content(content);
                    self.ssh.model.set_editing_file(Some(file));
                    self.overlays.open(FILE_EDITOR);
                }
                (failed, _) => self.toast_error(&format!("Couldn't open {name}"), &failed.map(|f| f.1).unwrap_or_default()),
            },
            Call::Write { conn, path } => match failed {
                Some((_, message)) => self.toast_error("Couldn't save the file", &message),
                None => {
                    if let Some(file) = self.ssh.model.editing_file.as_mut().filter(|f| f.connection_id == conn && f.path == path) {
                        file.original_content = file.content.clone();
                    }
                    self.toasts.push(self.now, ToastKind::Success, format!("Saved {}", sftp::local_file_name(&path)), None);
                }
            },
        }
    }

    /// `restoreSessions`: re-adopt live sessions after a reload.
    fn ssh_restore(&mut self, data: &Value) {
        let Some(live) = data.as_array() else { return };
        for session in live {
            let (Some(attempt), Some(profile), Some(pty), Some(claim)) = (
                session["sessionId"].as_str(),
                session["profileId"].as_str(),
                session["terminalId"].as_str(),
                session["claim"].as_str(),
            ) else {
                continue;
            };
            if let Some(known) = self.ssh.model.sessions.iter().find(|s| s.attempt_id == attempt) {
                // A resend's receipt left the tab connecting: attach the shell its open started.
                if known.pty_id.is_none() {
                    let effects = self.ssh.model.opened(
                        attempt,
                        Opened::Ok { pty_id: pty.to_owned(), claim: claim.to_owned(), connected_at: None, secret_saved: false },
                    );
                    self.ssh_run(effects);
                }
                continue;
            }
            let mut restored = model::ssh::Session::connecting(attempt, profile);
            restored.status = model::ssh::Status::Connected;
            restored.stage = None;
            restored.pty_id = Some(pty.to_owned());
            restored.claim = Some(claim.to_owned());
            restored.attached = true;
            restored.connected_at = session["connectedAt"].as_str().map(str::to_owned);
            self.ssh.model.add_session(restored);
            let name = self.ssh.model.profile(profile).map_or_else(|| "host".to_owned(), |p| p.name.clone());
            self.attach_ssh_terminal(pty, claim, format!("SSH: {name}"));
        }
    }

    // ---- hosts ----

    pub(crate) fn close_ssh_workspace(&mut self) {
        self.ssh.model.select_profile(None);
    }

    pub(crate) fn ssh_connect(&mut self, profile_id: &str) {
        let tab = self.ssh_attempt_id();
        log::info!("[ssh] opening a session for host {profile_id}");
        let effects = self.ssh.model.open_session(&tab, profile_id);
        self.ssh_run(effects);
    }

    pub(crate) fn ssh_edit_host(&mut self, profile_id: Option<&str>) {
        let profile = profile_id.and_then(|id| self.ssh.model.profile(id).cloned());
        let form = &mut self.ssh.form;
        *form = HostForm::default();
        match &profile {
            Some(p) => {
                form.id = Some(p.id.clone());
                form.name = edit(&p.name);
                form.host = edit(&p.host);
                form.port = edit(&p.port.to_string());
                form.username = edit(&p.username);
                form.auth = p.auth_method.clone();
                form.key_path = edit(p.private_key_path.as_deref().unwrap_or_default());
            }
            None => {
                form.port = edit("22");
                form.auth = "key".into();
            }
        }
        self.overlays.open(EDITOR);
    }

    /// Saves the editor's host; `connect` opens a session once it is saved.
    pub(crate) fn ssh_save_host(&mut self, connect: bool) {
        let form = &mut self.ssh.form;
        let text = |e: &TextEdit| e.text.trim().to_owned();
        let (host, username) = (text(&form.host), text(&form.username));
        let Ok(port) = text(&form.port).parse::<u16>().map_err(drop).and_then(|p| if p == 0 { Err(()) } else { Ok(p) }) else {
            form.error = Some("Port must be a number from 1 to 65535.".into());
            return;
        };
        if host.is_empty() || username.is_empty() {
            form.error = Some("Enter the host address and the username.".into());
            return;
        }
        let existing = form.id.as_deref().and_then(|id| self.ssh.model.profile(id)).cloned();
        let id = form.id.clone().unwrap_or_else(|| format!("ssh-{:x}", (self.now * 1000.0) as u64));
        let name = Some(text(&form.name)).filter(|n| !n.is_empty()).unwrap_or_else(|| host.clone());
        let key_path = Some(text(&form.key_path)).filter(|k| !k.is_empty() && form.auth == "key");
        let profile = Profile {
            id,
            name,
            host,
            port,
            username,
            auth_method: form.auth.clone(),
            private_key_path: key_path,
            ..existing.unwrap_or_default()
        };
        let password = (form.auth == "password").then(|| form.password.text.clone());
        let passphrase = (form.auth == "key").then(|| form.passphrase.text.clone());
        let payload = model::ssh::save_payload(&profile, password.as_deref(), passphrase.as_deref());
        // The typed secrets leave the form with the request.
        form.password = TextEdit::default();
        form.passphrase = TextEdit::default();
        form.error = None;
        form.saving = true;
        self.call(WsRequestType::SshSaveProfile, payload, Pending::Ssh(Call::Save { profile: Box::new(profile), connect }));
    }

    /// Opens the delete confirm for this host, capturing which one it is.
    pub(crate) fn ssh_request_delete(&mut self, profile_id: &str) {
        let Some(profile) = self.ssh.model.profile(profile_id) else { return };
        self.ssh.delete_host = Some((profile.id.clone(), profile.name.clone()));
        self.overlays.open(DELETE_HOST);
    }

    /// The confirm acts on the host it named when it opened, whatever is selected now.
    pub(crate) fn ssh_confirm_delete(&mut self) {
        let Some((id, name)) = self.ssh.delete_host.take() else { return };
        self.close(DELETE_HOST);
        self.command(WsRequestType::SshDeleteProfile, json!({ "profileId": id }), Pending::Ssh(Call::Delete { id, name }));
    }

    pub(crate) fn ssh_duplicate(&mut self, profile_id: &str) {
        let name = self.ssh.model.profile(profile_id).map(|p| p.name.clone()).unwrap_or_default();
        self.command(WsRequestType::SshDuplicateProfile, json!({ "profileId": profile_id }), Pending::Ssh(Call::Duplicate(name)));
    }

    pub(crate) fn ssh_toggle_favorite(&mut self, profile_id: &str) {
        let Some(mut profile) = self.ssh.model.profile(profile_id).cloned() else { return };
        profile.favorite = !profile.favorite;
        let payload = model::ssh::save_payload(&profile, None, None);
        self.call(WsRequestType::SshSaveProfile, payload, Pending::Ssh(Call::Save { profile: Box::new(profile), connect: false }));
    }

    pub(crate) fn ssh_import(&mut self) {
        self.command(WsRequestType::SshImportConfig, json!({}), Pending::Ssh(Call::Import));
    }

    // ---- sessions ----

    pub(crate) fn ssh_answer_host_key_shown(&mut self, tab: &str, attempt: &str, fingerprint: &str, trust: bool) {
        let effects = self.ssh.model.respond_host_key_shown(tab, attempt, fingerprint, trust);
        self.ssh_run(effects);
    }

    pub(crate) fn ssh_submit_secret(&mut self, tab: &str) {
        let secret = self.ssh.secret.text.clone();
        self.ssh.secret = TextEdit::default();
        let attempt = self.ssh_attempt_id();
        let save = self.ssh.save_secret;
        let effects = self.ssh.model.retry(tab, &attempt, Some(&secret), save);
        self.ssh_run(effects);
    }

    pub(crate) fn ssh_retry(&mut self, tab: &str) {
        let attempt = self.ssh_attempt_id();
        let effects = self.ssh.model.retry(tab, &attempt, None, false);
        self.ssh_run(effects);
    }

    pub(crate) fn ssh_cancel(&mut self, tab: &str) {
        let effects = self.ssh.model.cancel(tab);
        self.ssh_run(effects);
    }

    pub(crate) fn ssh_request_close(&mut self, tab: &str) {
        let effects = self.ssh.model.request_close(tab);
        if self.ssh.model.pending_close.as_deref() == Some(tab) {
            self.overlays.open(CLOSE_SESSION);
        }
        self.ssh_run(effects);
    }

    pub(crate) fn ssh_confirm_close(&mut self) {
        let Some(tab) = self.ssh.model.pending_close.clone() else { return };
        self.close(CLOSE_SESSION);
        let effects = self.ssh.model.close(&tab);
        self.ssh_run(effects);
    }

    pub(crate) fn ssh_cancel_close(&mut self) {
        self.ssh.model.set_pending_close(None);
        self.close(CLOSE_SESSION);
    }

    // ---- SFTP ----

    pub(crate) fn sftp_toggle(&mut self, conn: &str) {
        self.ssh.files_open = !self.ssh.files_open;
        if self.ssh.files_open && !self.ssh.explorers.contains_key(conn) {
            self.ssh.explorers.insert(conn.to_owned(), Explorer { loading: true, ..Explorer::default() });
            let payload = json!({ "request": { "connectionId": conn, "remotePath": "." } });
            self.call(WsRequestType::SftpRealpath, payload, Pending::Ssh(Call::Home(conn.to_owned())));
        }
    }

    pub(crate) fn sftp_list(&mut self, conn: &str, path: &str) {
        self.ssh.explorers.entry(conn.to_owned()).or_default().loading = true;
        let payload = json!({ "request": { "connectionId": conn, "remotePath": path } });
        self.call(WsRequestType::SftpListDir, payload, Pending::Ssh(Call::List { conn: conn.to_owned(), path: path.to_owned() }));
    }

    pub(crate) fn sftp_refresh(&mut self, conn: &str) {
        let cwd = self.ssh.explorers.get(conn).map(|e| e.cwd.clone()).filter(|c| !c.is_empty()).unwrap_or_else(|| "/".into());
        self.sftp_list(conn, &cwd);
    }

    pub(crate) fn sftp_up(&mut self, conn: &str) {
        let cwd = self.ssh.explorers.get(conn).map(|e| e.cwd.clone()).unwrap_or_default();
        self.sftp_list(conn, &sftp::parent_path(&cwd));
    }

    pub(crate) fn sftp_activate(&mut self, conn: &str, entry: &Entry) {
        if entry.is_dir() {
            self.sftp_list(conn, &entry.path);
        } else {
            let payload = json!({ "request": { "connectionId": conn, "remotePath": entry.path } });
            let call = Call::Read { conn: conn.to_owned(), path: entry.path.clone(), name: entry.name.clone() };
            self.call(WsRequestType::SftpReadFile, payload, Pending::Ssh(call));
        }
    }

    fn host_of(&self, conn: &str) -> String {
        self.ssh
            .model
            .sessions
            .iter()
            .find(|s| s.attempt_id == conn)
            .and_then(|s| self.ssh.model.profile(&s.profile_id))
            .map_or_else(|| "the server".to_owned(), |p| p.name.clone())
    }

    /// Opens the delete confirm for this entry, capturing it.
    pub(crate) fn sftp_request_delete(&mut self, conn: &str, entry: &Entry) {
        self.ssh.confirm = Some(RemoteConfirm::Delete { conn: conn.to_owned(), entry: entry.clone(), host: self.host_of(conn) });
        self.overlays.open(REMOTE_CONFIRM);
    }

    /// Runs the confirmed action on the target captured when the confirm opened.
    pub(crate) fn sftp_confirm(&mut self) {
        let Some(confirm) = self.ssh.confirm.take() else { return };
        self.close(REMOTE_CONFIRM);
        match confirm {
            RemoteConfirm::Delete { conn, entry, .. } => {
                let payload = json!({ "request": { "connectionId": conn, "remotePath": entry.path } });
                let call = Call::Changed { conn, what: format!("delete {}", entry.name), done: format!("Deleted {}", entry.name) };
                self.command(WsRequestType::SftpDelete, payload, Pending::Ssh(call));
            }
            RemoteConfirm::Replace { conn, dir, locals, conflicts, .. } => {
                for local in locals {
                    let overwrite = conflicts.contains(&sftp::local_file_name(&local));
                    self.sftp_send(&conn, &dir, &[local], overwrite);
                }
            },
        }
    }

    pub(crate) fn sftp_cancel_confirm(&mut self) {
        self.ssh.confirm = None;
        self.close(REMOTE_CONFIRM);
    }

    /// Upload: the system file picker, then the replace check against the listing.
    pub(crate) fn sftp_pick_upload(&mut self, conn: &str, tx: &mut Tx) {
        let conn = conn.to_owned();
        let options = FileDialogOptions { title: "Upload files".into(), ..Default::default() };
        tx.open_file(options, move |s: &mut State, answer, _tx| match answer {
            Ok(Some(path)) => s.sftp_upload(&conn, vec![path.to_string_lossy().into_owned()]),
            Ok(None) => log::info!("[ssh] upload cancelled"),
            Err(e) => s.toast_error("Could not open the file picker", &e.to_string()),
        });
    }

    /// Uploads `locals` into the explorer's folder, asking first when a name exists there.
    pub(crate) fn sftp_upload(&mut self, conn: &str, locals: Vec<String>) {
        let Some(explorer) = self.ssh.explorers.get(conn) else { return };
        let dir = explorer.cwd.clone();
        let conflicts = sftp::conflicts(&locals, &explorer.entries);
        if conflicts.is_empty() {
            return self.sftp_send(conn, &dir, &locals, false);
        }
        self.ssh.confirm = Some(RemoteConfirm::Replace { conn: conn.to_owned(), dir, locals, conflicts, host: self.host_of(conn) });
        self.overlays.open(REMOTE_CONFIRM);
    }

    fn sftp_send(&mut self, conn: &str, dir: &str, locals: &[String], overwrite: bool) {
        for local in locals {
            let remote = sftp::join_path(dir, &sftp::local_file_name(local));
            log::info!("[ssh] SFTP upload started: {}", sftp::local_file_name(local));
            self.ssh.model.transfer_started(conn, &remote, local, "upload");
            let payload = json!({ "request": { "connectionId": conn, "remotePath": remote, "localPath": local, "overwrite": overwrite } });
            let call = Call::Transfer { conn: conn.to_owned(), remote, local: local.clone(), upload: true };
            self.command(WsRequestType::SftpUpload, payload, Pending::Ssh(call));
        }
    }

    pub(crate) fn sftp_pick_download(&mut self, conn: &str, entry: &Entry, tx: &mut Tx) {
        let (conn, remote) = (conn.to_owned(), entry.path.clone());
        let options = FileDialogOptions { title: "Download".into(), file_name: Some(entry.name.clone()), ..Default::default() };
        tx.save_file(options, move |s: &mut State, answer, _tx| match answer {
            Ok(Some(dest)) => {
                let local = dest.to_string_lossy().into_owned();
                s.ssh.model.transfer_started(&conn, &remote, &local, "download");
                let payload = json!({ "request": { "connectionId": conn, "remotePath": remote, "localPath": local } });
                let call = Call::Transfer { conn: conn.clone(), remote: remote.clone(), local, upload: false };
                s.command(WsRequestType::SftpDownload, payload, Pending::Ssh(call));
            }
            Ok(None) => log::info!("[ssh] download cancelled"),
            Err(e) => s.toast_error("Could not open the save dialog", &e.to_string()),
        });
    }

    pub(crate) fn sftp_ask_name(&mut self, kind: NameKind, conn: &str, entry: Option<Entry>) {
        self.ssh.name = edit(entry.as_ref().map_or("", |e| e.name.as_str()));
        self.ssh.naming = Some((kind, conn.to_owned(), entry));
        self.overlays.open(NAME_DIALOG);
    }

    pub(crate) fn sftp_submit_name(&mut self) {
        let Some((kind, conn, entry)) = self.ssh.naming.take() else { return };
        self.close(NAME_DIALOG);
        let name = self.ssh.name.text.trim().to_owned();
        if name.is_empty() || name.contains('/') {
            self.toast_error("Enter a name without “/”", "");
            return;
        }
        let dir = self.ssh.explorers.get(&conn).map(|e| e.cwd.clone()).unwrap_or_else(|| "/".into());
        let path = sftp::join_path(&dir, &name);
        let target = json!({ "request": { "connectionId": conn, "remotePath": path } });
        let (kind, payload, what, done) = match (kind, entry) {
            (NameKind::Folder, _) => (WsRequestType::SftpMkdir, target, format!("create {name}"), format!("Created {name}")),
            (NameKind::File, _) => (WsRequestType::SftpCreateFile, target, format!("create {name}"), format!("Created {name}")),
            (NameKind::Rename, Some(entry)) => (
                WsRequestType::SftpRename,
                json!({ "request": { "connectionId": conn, "oldPath": entry.path, "newPath": path } }),
                format!("rename {} to {name}", entry.name),
                format!("Renamed to {name}"),
            ),
            (NameKind::Rename, None) => return,
        };
        self.command(kind, payload, Pending::Ssh(Call::Changed { conn, what, done }));
    }

    pub(crate) fn sftp_save_file(&mut self) {
        let Some(file) = self.ssh.model.editing_file.as_mut() else { return };
        file.content = self.ssh.file_text.text.clone();
        let (conn, path, content) = (file.connection_id.clone(), file.path.clone(), file.content.clone());
        let payload = json!({ "request": { "connectionId": conn, "remotePath": path, "content": content } });
        self.call(WsRequestType::SftpWriteFile, payload, Pending::Ssh(Call::Write { conn, path }));
    }

    pub(crate) fn sftp_close_file(&mut self) {
        self.ssh.model.set_editing_file(None);
        self.close(FILE_EDITOR);
    }
}

/// The request type of a model effect's request name.
fn request_type(name: &str) -> Option<WsRequestType> {
    WsRequestType::ALL.iter().copied().find(|kind| kind.as_str() == name)
}

#[cfg(test)]
#[path = "ssh_tests.rs"]
mod tests;
