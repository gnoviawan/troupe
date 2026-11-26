//! The terminal command editor (`components/terminal/command-editor/`, `bind-session.ts`): the
//! host's lifecycle frames drive each terminal's session (`model::terminal::store`), and while
//! the shell sits at a trusted prompt the terminal shows an editor under its screen instead of
//! sending keys to the PTY. Enter sends the draft with `command_editor_submit`, so the host
//! writes it to the shell; Tab asks the host for completions; Arrow Up recalls the project's
//! sent commands. The draft lives in the session store, so it survives the editor's unmount.

use std::collections::{BTreeMap, BTreeSet};

use client::CallId;
use client::terminal::TermError;
use model::terminal::completion::{self, Context, Effect, Reply, Scheduler};
use model::terminal::history::{Entry, History};
use model::terminal::line::{Action, Key as LineKey, Line};
use model::terminal::routes::{self, Call};
use model::terminal::session::{Mode, Phase};
use model::terminal::store::{Lifecycle, RpcResult, Store};
use model::terminal::words::{self, Row};
use rune::{Key, KeyEvent, Tx};
use serde_json::{Value, json};
use troupe_editor::Command;
use troupe_protocol::terminal::{WebTerminalEventPayload, WebTerminalRequestType};
use troupe_protocol::ws::WsRequestType;

use crate::State;
use crate::sync::Pending as SyncPending;

const FIELD: &str = "command-editor-";

/// The focusable strip of terminal `id`'s editor.
pub fn field_key(id: &str) -> String {
    format!("{FIELD}{id}")
}

/// The terminal whose editor field has `key`.
pub fn field_terminal(key: &str) -> Option<&str> {
    key.strip_prefix(FIELD)
}

/// What a session call's reply settles.
#[derive(Clone, Debug, PartialEq)]
pub enum Pending {
    Submit { id: String, epoch: u64, draft: String },
    Completion { id: String, request_id: String },
    /// A cancel or a restore: nothing waits on it.
    Fire,
}

/// A completion the editor shows: the rows for the text it was asked for, and the lit one.
#[derive(Clone, Debug, PartialEq)]
pub struct Shown {
    pub rows: Vec<Row>,
    pub lit: usize,
}

/// A session's id, nonce and prompt generation: what tells one prompt from the next.
type Prompt = (Option<String>, Option<String>, Option<u64>);

#[derive(Default)]
pub struct EditorUi {
    pub sessions: Store,
    /// The mounted editors, by terminal.
    pub lines: BTreeMap<String, Line>,
    pub schedulers: BTreeMap<String, Scheduler>,
    /// The input method's composition at each editor's caret.
    pub preedit: BTreeMap<String, String>,
    /// The completion list open over an editor.
    pub shown: BTreeMap<String, Shown>,
    /// The latest valid completion for each editor, until its text changes.
    completions: BTreeMap<String, completion::Completion>,
    pub pending: BTreeMap<CallId, Pending>,
    pub history: History,
    /// Projects whose history read was sent.
    asked: BTreeSet<String>,
    /// Projects whose stored history was read: until then an accepted command stays in memory,
    /// so a write cannot replace the stored list with a partial one.
    read: BTreeSet<String>,
    /// Each terminal's git branch, from `git_branch_changed`.
    pub branches: BTreeMap<String, String>,
    /// The editor's cell on the desktop and on touch, measured once.
    pub metrics: [std::cell::OnceCell<ui::terminal::Metrics>; 2],
    /// The columns each editor wraps at, as last laid out.
    pub columns: BTreeMap<String, usize>,
    /// An editor that mounted since the last frame; it takes focus when its terminal had it.
    focus: Option<String>,
    /// What each session showed last, to tell a new prompt from the same one.
    seen: BTreeMap<String, Prompt>,
}

/// A rune key as one of the editor keymap's keys (`editorKeymap`).
fn line_key(key: &KeyEvent) -> Option<LineKey> {
    let m = key.mods;
    let ctrl_letter = |c: char| m.ctrl && !m.alt && matches!(&key.key, Key::Command(k) if *k == c) || m.ctrl && matches!(&key.key, Key::Text(t) if t.eq_ignore_ascii_case(&c.to_string()));
    Some(match &key.key {
        Key::Enter if m.shift => LineKey::ShiftEnter,
        Key::Enter => LineKey::Enter,
        Key::Up if !m.shift => LineKey::Up,
        Key::Down if !m.shift => LineKey::Down,
        _ if ctrl_letter('a') => LineKey::LineStart,
        _ if ctrl_letter('e') => LineKey::LineEnd,
        _ if ctrl_letter('k') => LineKey::KillToEnd,
        _ if ctrl_letter('u') => LineKey::KillToStart,
        _ if ctrl_letter('w') => LineKey::KillWord,
        _ if ctrl_letter('c') => LineKey::Interrupt,
        _ if ctrl_letter('d') => LineKey::Eof,
        _ if key.is_command('z') && !m.shift => LineKey::Undo,
        _ if key.is_command('y') || (key.is_command('z') && m.shift) => LineKey::Redo,
        _ => return None,
    })
}

/// The editing keys CodeMirror leaves to the browser: typing, Backspace, the arrows across a
/// line, Home and End.
fn edit_command(key: &KeyEvent) -> Option<Command> {
    let m = key.mods;
    let word = m.alt || (m.ctrl && !m.cmd) || (m.cmd && cfg!(not(target_os = "macos")));
    let moving = |motion| Command::Move { motion, extend: m.shift };
    use troupe_editor::Motion;
    Some(match &key.key {
        Key::Text(t) if !(m.cmd || m.ctrl) => Command::Insert(t.clone()),
        Key::Backspace if word => Command::DeleteWordBackward,
        Key::Backspace => Command::Backspace,
        Key::Delete if word => Command::DeleteWordForward,
        Key::Delete => Command::Delete,
        Key::Left => moving(if word { Motion::WordLeft } else { Motion::Left }),
        Key::Right => moving(if word { Motion::WordRight } else { Motion::Right }),
        Key::Up => moving(Motion::Up),
        Key::Down => moving(Motion::Down),
        Key::Home => moving(Motion::LineStart),
        Key::End => moving(Motion::LineEnd),
        _ if key.is_command('a') && !m.ctrl => Command::SelectAll,
        _ => return None,
    })
}

impl State {
    fn sessions(&mut self) -> &mut Store {
        &mut self.terms.editor.sessions
    }

    /// The session's mode for terminal `id` (`PTY` before any frame).
    pub(crate) fn input_mode(&self, id: &str) -> Mode {
        self.terms.editor.sessions.read(id).mode
    }

    fn drain_session_logs(&mut self) {
        for entry in std::mem::take(&mut self.sessions().logs) {
            match entry.level {
                model::Level::Info => log::info!("[sessionStore] {}", entry.message),
                _ => log::warn!("[sessionStore] {}", entry.message),
            }
        }
    }

    /// A `/terminal/ws` event the command editor follows. True when it was one.
    pub(crate) fn command_editor_event(&mut self, event: &WebTerminalEventPayload) -> bool {
        match event {
            WebTerminalEventPayload::SessionLifecycle { terminal_id, session_id, kind, nonce, prompt_suppressed, line_empty, exit_code, prompt_generation, cwd } => {
                let frame = Lifecycle {
                    kind: kind.clone(),
                    session_id: Some(session_id.clone()),
                    nonce: Some(nonce.clone()),
                    prompt_generation: *prompt_generation,
                    line_empty: Some(*line_empty),
                    prompt_suppressed: Some(*prompt_suppressed),
                    cwd: cwd.clone(),
                    exit_code: *exit_code,
                };
                log::info!("[session] {terminal_id} {kind} generation={prompt_generation:?}");
                self.sessions().apply(terminal_id, &frame);
                self.drain_session_logs();
                self.sync_command_editor(terminal_id);
                true
            }
            WebTerminalEventPayload::Exit { terminal_id, .. } => {
                self.sessions().process_exited(terminal_id);
                self.drain_session_logs();
                self.sync_command_editor(terminal_id);
                false
            }
            WebTerminalEventPayload::GitBranchChanged { terminal_id, branch } => {
                match branch {
                    Some(b) => self.terms.editor.branches.insert(terminal_id.clone(), b.clone()),
                    None => self.terms.editor.branches.remove(terminal_id),
                };
                true
            }
            _ => false,
        }
    }

    /// Terminal `id`'s screen changed: follow its alternate screen.
    pub(crate) fn command_editor_screen(&mut self, id: &str) {
        let Some(alt) = self.terms.views.get(id).and_then(|v| v.cells.lock().ok().map(|t| t.alt_screen())) else { return };
        if self.terms.editor.sessions.read(id).alternate_screen == alt {
            return;
        }
        self.sessions().apply_screen(id, alt);
        self.drain_session_logs();
        self.sync_command_editor(id);
    }

    /// The terminal closed: its session and draft go.
    pub(crate) fn command_editor_forget(&mut self, id: &str) {
        self.unmount_command_editor(id);
        self.sessions().forget(id);
        self.terms.editor.seen.remove(id);
        self.terms.editor.branches.remove(id);
        self.terms.editor.columns.remove(id);
    }

    /// Mounts or unmounts terminal `id`'s editor to match its session, and drops a completion
    /// asked for a prompt that is gone (`watchCompletionSession`).
    pub(crate) fn sync_command_editor(&mut self, id: &str) {
        let session = self.terms.editor.sessions.read(id);
        let seen = (session.session_id.clone(), session.nonce.clone(), session.prompt_generation);
        let moved = self.terms.editor.seen.insert(id.to_owned(), seen.clone()).is_some_and(|before| before != seen);
        let mounted = self.terms.editor.lines.contains_key(id);
        match (session.mode == Mode::Editor, mounted) {
            (true, false) => {
                let draft = self.terms.editor.sessions.draft(id).to_owned();
                let mut line = Line::new(&draft);
                if let Some(&columns) = self.terms.editor.columns.get(id) {
                    line.editor.set_wrap(columns);
                }
                self.terms.editor.lines.insert(id.to_owned(), line);
                log::info!("[CommandEditor] Command editor mounted on {id}");
                if self.terms.focused.as_deref() == Some(id) {
                    self.terms.editor.focus = Some(id.to_owned());
                }
                self.load_command_history(id);
            }
            (false, true) => self.unmount_command_editor(id),
            (true, true) if moved => self.invalidate_completion(id),
            _ => {}
        }
    }

    fn unmount_command_editor(&mut self, id: &str) {
        self.terms.editor.preedit.remove(id);
        if self.terms.editor.lines.remove(id).is_some() {
            log::info!("[CommandEditor] Command editor unmounted from {id}");
        }
        self.invalidate_completion(id);
    }

    fn invalidate_completion(&mut self, id: &str) {
        self.terms.editor.shown.remove(id);
        self.terms.editor.completions.remove(id);
        let mut out = Vec::new();
        if let Some(scheduler) = self.terms.editor.schedulers.get_mut(id) {
            scheduler.invalidate(&mut out);
        }
        self.run_completion_effects(id, out);
    }

    fn terminal_project(&self, id: &str) -> Option<String> {
        self.terms.sessions.get(id).map(|s| s.project_id.clone())
    }

    /// Reads the project's sent commands once, for Arrow Up (`useCommandHistoryLoader`).
    fn load_command_history(&mut self, id: &str) {
        let Some(project) = self.terminal_project(id) else { return };
        if !self.terms.editor.asked.insert(project.clone()) {
            return;
        }
        let key = self.device_key(&model::terminal::history::key(&project));
        self.call(WsRequestType::StoreRead, json!({ "key": key }), SyncPending::ReadHistory(project));
    }

    /// A project's list read from the host: the editor recalls from it too.
    /// Commands accepted before the first read are kept on top of the stored list and written.
    pub(crate) fn command_history_loaded(&mut self, project: &str, list: &Value) {
        let entries: Vec<Entry> = list.as_array().into_iter().flatten().filter_map(|v| serde_json::from_value(v.clone()).ok()).collect();
        let editor = &mut self.terms.editor;
        let early: Vec<Entry> = if editor.read.insert(project.to_owned()) { editor.history.entries.iter().filter(|e| e.project_id == project).cloned().collect() } else { Vec::new() };
        editor.history.loaded(project, entries);
        let mut write = None;
        for e in early.into_iter().rev() {
            write = editor.history.add(&e.command, &e.terminal_name, &e.terminal_id, project, e.timestamp, e.id).or(write);
        }
        if let Some(write) = write {
            log::info!("[history] writing commands accepted before {project}'s history was read");
            self.write_command_history(project, write);
        }
    }

    /// The history modal cleared the project's list: nothing accepted earlier comes back.
    pub(crate) fn command_history_cleared(&mut self, project: &str) {
        self.terms.editor.read.insert(project.to_owned());
        self.terms.editor.history.loaded(project, Vec::new());
    }

    /// The read failed: the next editor to mount asks again.
    pub(crate) fn command_history_unread(&mut self, project: &str) {
        self.terms.editor.asked.remove(project);
    }

    fn recall_list(&self, id: &str) -> Vec<String> {
        self.terminal_project(id).map(|p| self.terms.editor.history.recall(&p)).unwrap_or_default()
    }

    /// The completion context for terminal `id`'s editor (`resolveCompletionContext`).
    fn completion_context(&self, id: &str) -> Option<Context> {
        let s = self.terms.editor.sessions.read(id);
        let line = self.terms.editor.lines.get(id)?;
        let ok = s.registered && s.alive && s.trusted && s.mode == Mode::Editor && s.phase == Phase::AtPrompt;
        let (session_id, generation, cwd) = (s.session_id.clone()?, s.prompt_generation?, s.cwd.clone()?);
        (ok && !session_id.is_empty() && !cwd.is_empty()).then(|| Context {
            terminal_id: id.to_owned(),
            session_id,
            mode: s.mode,
            phase: s.phase,
            draft: line.text(),
            buffer_revision: self.terms.editor.sessions.revision(id),
            cursor: line.caret_utf16(),
            prompt_generation: generation,
            cwd: Some(cwd),
        })
    }

    fn scheduler(&mut self, id: &str) -> &mut Scheduler {
        let auto = self.settings.get("automaticCompletionRefresh") != &Value::Bool(false);
        let scheduler = self.terms.editor.schedulers.entry(id.to_owned()).or_default();
        scheduler.auto_refresh = auto;
        scheduler
    }

    /// The editor's text or caret changed (`revisionPlugin`, `draftPersistence`).
    fn command_edited(&mut self, id: &str, text_changed: bool) {
        self.sessions().bump_revision(id);
        if text_changed {
            let text = self.terms.editor.lines.get(id).map(Line::text).unwrap_or_default();
            let mode = self.input_mode(id);
            self.sessions().offer(id, mode, &text);
        }
        self.terms.editor.completions.remove(id);
        self.terms.editor.shown.remove(id);
        let now = self.now * 1000.0;
        let mut out = Vec::new();
        match self.completion_context(id) {
            Some(context) => self.scheduler(id).update(context, now, &mut out),
            None => self.scheduler(id).invalidate(&mut out),
        }
        self.run_completion_effects(id, out);
    }

    /// Runs one editor command (typing, a move) on terminal `id`'s editor.
    pub(crate) fn command_apply(&mut self, id: &str, command: Command) -> bool {
        let now = self.now;
        let Some(line) = self.terms.editor.lines.get_mut(id) else { return false };
        let (revision, selection) = (line.editor.revision(), line.editor.selection());
        line.apply(command, now);
        let text_changed = line.editor.revision() != revision;
        let changed = text_changed || line.editor.selection() != selection;
        if changed {
            self.command_edited(id, text_changed);
        }
        changed
    }

    /// An input method event for terminal `id`'s focused editor. True when it was used.
    pub(crate) fn command_editor_ime(&mut self, id: &str, ime: &rune::Ime) -> bool {
        if !self.terms.editor.lines.contains_key(id) {
            return false;
        }
        match ime {
            rune::Ime::Preedit(text, ..) if text.is_empty() => self.terms.editor.preedit.remove(id).is_some(),
            rune::Ime::Preedit(text, ..) => {
                self.terms.editor.preedit.insert(id.to_owned(), text.clone());
                true
            }
            rune::Ime::Commit(text) => {
                self.terms.editor.preedit.remove(id);
                self.command_apply(id, Command::Insert(text.clone()));
                true
            }
            _ => false,
        }
    }

    /// Text pasted into terminal `id`'s editor.
    pub(crate) fn command_paste(&mut self, id: &str, text: &str) {
        if text.len() > crate::term_input::MAX_PASTE {
            log::warn!("[term] paste of {} bytes refused; the most is {}", text.len(), crate::term_input::MAX_PASTE);
            return;
        }
        let clean = words::sanitize_paste(text);
        if !clean.is_empty() {
            self.command_apply(id, Command::Insert(clean));
        }
    }

    /// A key while terminal `id`'s editor has focus. True when the editor took it.
    pub(crate) fn command_editor_key(&mut self, id: &str, key: &KeyEvent, tx: &mut Tx) -> bool {
        self.terms.focused = Some(id.to_owned());
        if self.terms.editor.sessions.read(id).phase == Phase::Submitting {
            return true;
        }
        if self.completion_key(id, key) {
            return true;
        }
        if key.is_command('v') && !key.mods.shift {
            let id = id.to_owned();
            tx.read_clipboard(move |s: &mut State, content, _tx| s.command_paste(&id, content.text()));
            return true;
        }
        if let Some(chord) = crate::keys::chord(key, self.shortcuts.platform)
            && model::shortcuts::find_match(&self.shortcuts, &chord).is_some()
        {
            return false;
        }
        if let Some(k) = line_key(key) {
            return self.command_line_key(id, k, tx);
        }
        if let Some(command) = edit_command(key) {
            self.command_apply(id, command);
            return true;
        }
        false
    }

    fn command_line_key(&mut self, id: &str, key: LineKey, tx: &mut Tx) -> bool {
        let history = self.recall_list(id);
        let selection = self.terminal_selection_text(id).unwrap_or_default();
        let now = self.now;
        let Some(line) = self.terms.editor.lines.get_mut(id) else { return false };
        let (revision, sel) = (line.editor.revision(), line.editor.selection());
        let action = line.key(key, &history, &selection, now);
        let text_changed = line.editor.revision() != revision;
        if text_changed || line.editor.selection() != sel {
            self.command_edited(id, text_changed);
        }
        match action {
            Action::Ignored => false,
            Action::Handled => true,
            Action::Submit(text) => {
                self.submit_command(id, &text);
                true
            }
            Action::SendEof => {
                self.command_eof(id);
                true
            }
            Action::Copy(text) => {
                tx.copy(text);
                true
            }
        }
    }

    /// Ctrl+D on an empty draft (`sendEditorEof`): `^D` to the PTY while the editor is mounted.
    fn command_eof(&mut self, id: &str) {
        let s = self.terms.editor.sessions.read(id);
        if !(s.alive && s.trusted && s.mode == Mode::Editor && s.phase == Phase::AtPrompt) {
            return;
        }
        self.terminal_write_raw(id, "\u{4}");
    }

    /// Enter: send the draft as one transaction (`submitBoundDraft`). The host writes it to the
    /// shell; the editor unmounts until the next prompt.
    pub(crate) fn submit_command(&mut self, id: &str, draft: &str) {
        if self.terms.sessions.get(id).is_none() {
            log::info!("[bindSessionClient] submit skipped: pty id is null");
            return;
        }
        let Some(token) = self.sessions().begin_submit(id) else {
            log::info!("[bindSessionClient] submit skipped: the session cannot take one");
            self.drain_session_logs();
            return;
        };
        self.drain_session_logs();
        let mut out = Vec::new();
        self.scheduler(id).prepare_submit(&mut out);
        self.run_completion_effects(id, out);
        let call = Call::Submit { terminal_id: id.to_owned(), draft: draft.to_owned() };
        log::info!("[CommandEditor] submit on {id}: {} chars", draft.chars().count());
        let sent = self.session_call(&call);
        self.terms.editor.pending.insert(sent, Pending::Submit { id: id.to_owned(), epoch: token.epoch, draft: draft.to_owned() });
        self.sync_command_editor(id);
    }

    fn session_call(&mut self, call: &Call) -> CallId {
        let kind = match call {
            Call::Submit { .. } => WebTerminalRequestType::CommandEditorSubmit,
            Call::RequestCompletion { .. } => WebTerminalRequestType::CommandEditorRequestCompletion,
            Call::CancelCompletion { .. } => WebTerminalRequestType::CommandEditorCancelCompletion,
            Call::RestorePrompt { .. } => WebTerminalRequestType::CommandEditorRestorePrompt,
        };
        self.terms.session_request(kind, call.payload())
    }

    /// A reply to a session call. True when it was one.
    pub(crate) fn command_editor_reply(&mut self, call: CallId, result: &Result<Value, TermError>) -> bool {
        let Some(pending) = self.terms.editor.pending.remove(&call) else { return false };
        let reply = result.clone().map_err(|e| e.code);
        if reply.is_err() {
            log::warn!("[WebCommandEditorTransport] session RPC failed: {pending:?}");
        }
        match pending {
            Pending::Submit { id, epoch, draft } => self.submit_replied(&id, epoch, &draft, routes::read_result(&reply)),
            Pending::Completion { id, request_id } => {
                let reply: Reply = routes::read_completion(&reply);
                let now = self.now * 1000.0;
                let mut out = Vec::new();
                self.scheduler(&id).reply(&request_id, &reply, now, &mut out);
                self.run_completion_effects(&id, out);
            }
            Pending::Fire => {}
        }
        true
    }

    fn submit_replied(&mut self, id: &str, epoch: u64, draft: &str, result: RpcResult) {
        log::info!("[CommandEditor] submit reply on {id}: {result:?}");
        let applied = self.sessions().record_submit(id, result, epoch);
        self.drain_session_logs();
        if applied && matches!(result, RpcResult::Accepted | RpcResult::Partial) {
            self.sessions().submit_accepted(id);
        }
        if result == RpcResult::Accepted {
            self.remember_command(id, draft);
        }
        self.sync_command_editor(id);
    }

    /// An accepted command joins the project's history and the host's copy is rewritten.
    fn remember_command(&mut self, id: &str, draft: &str) {
        let Some(project) = self.terminal_project(id) else { return };
        let name = self.terms.names.get(id).cloned().unwrap_or_else(|| "terminal".into());
        let at = (crate::editor_host::epoch_ms()) as i64;
        let entry_id = format!("{at}-{}", self.terms.editor.history.entries.len());
        let Some(write) = self.terms.editor.history.add(draft, &name, id, &project, at, entry_id) else { return };
        if !self.terms.editor.read.contains(&project) {
            log::info!("[history] {project}'s history is not read yet; the command waits in memory");
            return;
        }
        self.write_command_history(&project, write);
    }

    fn write_command_history(&mut self, project: &str, write: model::terminal::history::Write) {
        if let Some(list) = write.value.get("data") {
            self.terms.history.loaded.insert(project.to_owned(), crate::command_history::parse_list(list));
        }
        let key = self.device_key(&write.key);
        self.call(WsRequestType::StoreWrite, json!({ "key": key, "value": write.value }), SyncPending::Write(write.key));
    }

    fn run_completion_effects(&mut self, id: &str, effects: Vec<Effect>) {
        for effect in effects {
            match effect {
                Effect::Request { terminal_id, request } => {
                    let request_id = request.request_id.clone();
                    let call = Call::RequestCompletion { terminal_id, request };
                    if !call.sendable() {
                        continue;
                    }
                    let sent = self.session_call(&call);
                    self.terms.editor.pending.insert(sent, Pending::Completion { id: id.to_owned(), request_id });
                }
                Effect::Cancel { terminal_id, request_id } => {
                    let sent = self.session_call(&Call::CancelCompletion { terminal_id, request_id });
                    self.terms.editor.pending.insert(sent, Pending::Fire);
                }
                Effect::Show(completion) => {
                    self.terms.editor.completions.insert(id.to_owned(), completion);
                    self.show_completion(id);
                }
                Effect::Log { name, detail } => log::info!("[CompletionScheduler] {name} {detail}"),
            }
        }
    }

    /// The rows of the editor's valid completion, for the text it shows now.
    fn completion_rows(&self, id: &str) -> Vec<Row> {
        let (Some(c), Some(line)) = (self.terms.editor.completions.get(id), self.terms.editor.lines.get(id)) else { return Vec::new() };
        if c.buffer_revision != self.terms.editor.sessions.revision(id) || c.cursor != line.caret_utf16() {
            return Vec::new();
        }
        let cursor = usize::try_from(c.cursor).unwrap_or(usize::MAX);
        words::parse_output(&line.text(), cursor, &c.output)
    }

    fn show_completion(&mut self, id: &str) {
        let rows = self.completion_rows(id);
        if rows.is_empty() {
            self.terms.editor.shown.remove(id);
        } else {
            self.terms.editor.shown.insert(id.to_owned(), Shown { rows, lit: 0 });
        }
    }

    /// The completion keys (`completionKeymap`): Tab asks or accepts, Right accepts the ghost
    /// at the end, Escape closes, Up and Down move the highlight.
    fn completion_key(&mut self, id: &str, key: &KeyEvent) -> bool {
        let open = self.terms.editor.shown.contains_key(id);
        match key.key {
            Key::Tab if !key.mods.shift => {
                if open {
                    self.accept_completion(id, None);
                } else {
                    self.request_completion(id);
                }
                true
            }
            Key::Escape if open => {
                self.terms.editor.shown.remove(id);
                true
            }
            Key::Up | Key::Down if open => {
                let shown = self.terms.editor.shown.get_mut(id).expect("open");
                let n = shown.rows.len();
                shown.lit = if key.key == Key::Up { (shown.lit + n - 1) % n } else { (shown.lit + 1) % n };
                true
            }
            Key::Right if open && self.ghost(id).is_some() => {
                let at_end = self.terms.editor.lines.get(id).is_some_and(|l| l.editor.selection().head == l.editor.rope().len_chars());
                if at_end {
                    self.accept_completion(id, None);
                } else {
                    self.command_apply(id, Command::Move { motion: troupe_editor::Motion::Right, extend: false });
                }
                true
            }
            _ => false,
        }
    }

    /// The lit row's text past what is typed, drawn after the caret.
    pub(crate) fn ghost(&self, id: &str) -> Option<String> {
        let shown = self.terms.editor.shown.get(id)?;
        let row = shown.rows.get(shown.lit)?;
        let line = self.terms.editor.lines.get(id)?;
        let units: Vec<u16> = line.text().encode_utf16().collect();
        let head = usize::try_from(line.caret_utf16()).ok()?;
        if !line.editor.selection().is_empty() || head != row.to || row.from > head {
            return None;
        }
        let typed = String::from_utf16_lossy(&units[row.from..head]);
        let insert = row.insert.as_deref().unwrap_or(&row.text);
        insert.strip_prefix(&typed).filter(|s| !s.is_empty()).map(str::to_owned)
    }

    /// The lit row's start, as a char index into the draft.
    pub(crate) fn completion_from(&self, id: &str) -> Option<usize> {
        let shown = self.terms.editor.shown.get(id)?;
        let row = shown.rows.get(shown.lit)?;
        let units: Vec<u16> = self.terms.editor.lines.get(id)?.text().encode_utf16().collect();
        Some(String::from_utf16_lossy(&units[..row.from.min(units.len())]).chars().count())
    }

    /// What is typed of the word being completed: the draft from the lit row's start to the caret.
    pub(crate) fn completion_typed(&self, id: &str) -> String {
        let (Some(shown), Some(line)) = (self.terms.editor.shown.get(id), self.terms.editor.lines.get(id)) else { return String::new() };
        let Some(row) = shown.rows.get(shown.lit) else { return String::new() };
        let units: Vec<u16> = line.text().encode_utf16().collect();
        let head = usize::try_from(line.caret_utf16()).unwrap_or(0).min(units.len());
        String::from_utf16_lossy(&units[row.from.min(head)..head])
    }

    /// Tab with nothing open: ask now (`binder.requestCompletion`).
    fn request_completion(&mut self, id: &str) {
        let now = self.now * 1000.0;
        let mut out = Vec::new();
        match self.completion_context(id) {
            Some(context) => self.scheduler(id).request(context, false, now, &mut out),
            None => {
                self.terms.editor.completions.remove(id);
                self.scheduler(id).invalidate(&mut out);
            }
        }
        self.run_completion_effects(id, out);
    }

    /// Replaces the typed word with row `index` (the lit one by default).
    pub(crate) fn accept_completion(&mut self, id: &str, index: Option<usize>) {
        let Some(shown) = self.terms.editor.shown.remove(id) else { return };
        let Some(row) = shown.rows.get(index.unwrap_or(shown.lit)).cloned() else { return };
        let Some(line) = self.terms.editor.lines.get(id) else { return };
        let text = line.text();
        let units: Vec<u16> = text.encode_utf16().collect();
        let from = String::from_utf16_lossy(&units[..row.from.min(units.len())]).chars().count();
        let to = String::from_utf16_lossy(&units[..row.to.min(units.len())]).chars().count();
        let insert = row.insert.unwrap_or(row.text);
        log::info!("[CommandEditor] completion accepted on {id}");
        self.command_apply(id, Command::Select(troupe_editor::Selection { anchor: from, head: to }));
        self.command_apply(id, Command::Insert(insert));
    }

    /// Fires the completion timers and hands focus to an editor that just mounted.
    pub(crate) fn pump_command_editors(&mut self, tx: &mut Tx) -> Option<f64> {
        let now = self.now * 1000.0;
        let ids: Vec<String> = self.terms.editor.schedulers.keys().cloned().collect();
        let mut next: Option<f64> = None;
        for id in ids {
            let mut out = Vec::new();
            if let Some(s) = self.terms.editor.schedulers.get_mut(&id) {
                s.tick(now, &mut out);
                if let Some(due) = s.deadline() {
                    next = Some(next.map_or(due, |n: f64| n.min(due)));
                }
            }
            self.run_completion_effects(&id, out);
        }
        if let Some(id) = self.terms.editor.focus.take()
            && self.terms.editor.lines.contains_key(&id)
        {
            tx.focus(field_key(&id), false);
        }
        next.map(|ms| ms / 1000.0)
    }

    /// A key bar key while terminal `id`'s editor is mounted: the key it stands for goes to
    /// the editor, and Page Up and Down scroll the screen. False when no editor is mounted.
    pub(crate) fn key_bar_to_editor(&mut self, id: &str, bytes: &str, tx: &mut Tx) -> bool {
        if !self.terms.editor.lines.contains_key(id) {
            return false;
        }
        // The key bar is not where typing goes: the editor keeps the soft keyboard.
        tx.focus(field_key(id), false);
        let ctrl = rune::Mods { ctrl: true, cmd: cfg!(not(target_os = "macos")), ..rune::Mods::default() };
        let (key, mods) = match bytes {
            "\x1b" => (Key::Escape, rune::Mods::default()),
            "\t" => (Key::Tab, rune::Mods::default()),
            "\x03" => (Key::Command('c'), ctrl),
            "\x1b[D" => (Key::Left, rune::Mods::default()),
            "\x1b[C" => (Key::Right, rune::Mods::default()),
            "\x1b[A" => (Key::Up, rune::Mods::default()),
            "\x1b[B" => (Key::Down, rune::Mods::default()),
            "\x1b[5~" | "\x1b[6~" => {
                if let Some(view) = self.terms.views.get_mut(id) {
                    let (rows, history) = view.cells.lock().map(|t| (t.rows(), t.screen_start())).unwrap_or((0, 0));
                    let page = rows.saturating_sub(1).max(1);
                    view.scrolled = if bytes == "\x1b[5~" { (view.scrolled + page).min(history) } else { view.scrolled.saturating_sub(page) };
                }
                return true;
            }
            _ => return true,
        };
        self.command_editor_key(id, &KeyEvent { key, mods }, tx);
        true
    }

    /// A saved command from the history modal, into the editor when it is mounted.
    pub(crate) fn insert_into_command_editor(&mut self, id: &str, command: &str) -> bool {
        if !self.terms.editor.lines.contains_key(id) {
            return false;
        }
        self.terms.editor.focus = Some(id.to_owned());
        self.command_apply(id, Command::Insert(command.to_owned()));
        true
    }
}

#[cfg(test)]
#[path = "command_editor_tests.rs"]
mod tests;
