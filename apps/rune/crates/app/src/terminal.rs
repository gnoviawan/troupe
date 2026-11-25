//! Terminal tabs: "New terminal" spawns a PTY for the visible project and attaches to it, the
//! focused terminal's keys go to its PTY, a measured pane resizes it, and closing the tab
//! kills it. Sessions (`model::terminal`) and their cells outlive project switches: a hidden
//! project's terminals stay attached and keep parsing, so switching back shows the same
//! process with everything it printed meanwhile.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};

use client::CallId;
use client::terminal::{Command, Output};
use model::terminal::{DEFAULT_SIZE, Spawned, Terminals};
use model::workspace::{Event as WorkspaceEvent, Tab};
use rune::{KeyEvent, Tx};
use serde_json::{Value, json};
use troupe_protocol::terminal::{SpawnedTerminal, TerminalKind, TerminalSpawnOptions, WebTerminalEventPayload, WebTerminalRequestType};
use ui::terminal::{Metrics, RowCache};

use crate::State;
use crate::term_host::{Cells, TermEvent, TermHost};

/// A terminal the app draws.
pub struct TermView {
    pub cells: Cells,
    pub cache: RefCell<RowCache>,
    /// Lines scrolled up into the scrollback.
    pub scrolled: usize,
    /// The part of a line the wheel moved that has not scrolled a whole line yet.
    pub wheel: f32,
    pub selection: Option<crate::term_select::Held>,
    pub find: Option<crate::term_select::Find>,
    /// The link under the pointer, underlined.
    pub hover: Option<troupe_term::links::Link>,
}

impl TermView {
    pub(crate) fn new(cells: Cells) -> TermView {
        TermView { cells, cache: RefCell::default(), scrolled: 0, wheel: 0.0, selection: None, find: None, hover: None }
    }
}

#[derive(Default)]
pub struct TerminalUi {
    pub host: Option<TermHost>,
    pub sessions: Terminals,
    pub views: BTreeMap<String, TermView>,
    /// The tab names (`Terminal <n>`), by terminal id.
    pub names: BTreeMap<String, String>,
    /// Terminals opened so far, which numbers the next tab's name.
    opened: usize,
    next_call: CallId,
    /// The terminal tabs the visible project showed last frame, to tell a closed tab from a
    /// project switch.
    shown: Option<(String, BTreeSet<String>)>,
    metrics: Option<Metrics>,
    /// The terminal that took the last key or press: the Terminal Search shortcut's target.
    pub focused: Option<String>,
    /// What a find field edits when its terminal is gone mid-frame.
    pub spare_edit: rune::TextEdit,
}

impl TerminalUi {
    /// The cell box, measured once the fonts are loaded.
    pub fn metrics(&self) -> Metrics {
        self.metrics.unwrap_or_else(|| Metrics::new(ui::terminal::FONT_SIZE, ui::terminal::LINE_HEIGHT))
    }

    fn call(&mut self) -> CallId {
        self.next_call += 1;
        self.next_call
    }

    fn send(&mut self, command: Command) {
        match &mut self.host {
            Some(host) => host.send(command),
            None => log::warn!("[term] no terminal link; {command:?} dropped"),
        }
    }

    fn request(&mut self, kind: WebTerminalRequestType, payload: Value, ledgered: bool) -> CallId {
        let call = self.call();
        self.send(Command::Call { call, kind, payload, ledgered });
        call
    }
}

impl State {
    /// The `+` menu's "New terminal": a login shell in the visible project's folder.
    pub(crate) fn new_terminal(&mut self, pane: Option<String>) {
        let Some(project) = self.active_project().cloned() else {
            log::warn!("[term] new terminal with no project shown");
            return;
        };
        let (cols, rows) = DEFAULT_SIZE;
        let options = TerminalSpawnOptions {
            cwd: project.path.clone(),
            cols: Some(cols),
            rows: Some(rows),
            kind: Some(TerminalKind::Shell),
            project_id: Some(project.id.clone()),
            ..TerminalSpawnOptions::default()
        };
        let payload = serde_json::to_value(options).unwrap_or(Value::Null);
        let call = self.terms.request(WebTerminalRequestType::Spawn, payload, true);
        self.terms.sessions.spawn(call, &project.id, pane);
        log::info!("[term] spawning a terminal for {} (call {call})", project.id);
    }

    /// Carries out what the terminal link said; true when the view must redraw.
    pub(crate) fn pump_terminals(&mut self, now: f64) -> bool {
        if self.terms.metrics.is_none() && self.terms.host.is_some() {
            self.terms.metrics = Some(Metrics::new(ui::terminal::FONT_SIZE, ui::terminal::LINE_HEIGHT));
        }
        let Some(host) = &mut self.terms.host else { return false };
        let events = host.drain(now);
        let mut changed = false;
        let mut redrawn = Vec::new();
        for event in events {
            changed = true;
            match event {
                TermEvent::Changed(id) => redrawn.push(id),
                TermEvent::Link(output) => self.terminal_output(output),
            }
        }
        changed | self.reap_closed_tabs() | self.refresh_terminal_finds(&redrawn)
    }

    fn terminal_output(&mut self, output: Output) {
        match output {
            Output::Reply { call, result } if self.terms.sessions.spawning.contains_key(&call) => match result {
                Ok(data) => match serde_json::from_value::<SpawnedTerminal>(data) {
                    Ok(spawned) => self.terminal_spawned(call, spawned),
                    Err(e) => {
                        self.terms.sessions.spawn_failed(call);
                        log::error!("[term] unreadable spawn reply: {e}");
                    }
                },
                Err(e) => {
                    self.terms.sessions.spawn_failed(call);
                    log::error!("[term] spawn failed: {} {}", e.code, e.message);
                    self.toast_error("Couldn't open a terminal", &e.message);
                }
            },
            Output::Reply { call, result } if self.terms.sessions.restoring.contains_key(&call) => self.terminal_restored(call, result),
            Output::Reply { result: Err(e), .. } => log::warn!("[term] request failed: {} {}", e.code, e.message),
            Output::Reply { .. } | Output::Status(_) | Output::Data { .. } | Output::Replay { .. } => {}
            Output::Attached(attached) => log::info!("[term] attached {} (pid {})", attached.id, attached.pid),
            Output::Lost { terminal_id, error } => {
                log::warn!("[term] lost {terminal_id}: {}", error.code);
                self.terms.sessions.lost(&terminal_id, &error.message);
            }
            Output::Event(event) => match event {
                WebTerminalEventPayload::Exit { terminal_id, exit_code, .. } => {
                    log::info!("[term] {terminal_id} exited ({exit_code:?})");
                    self.terms.sessions.exited(&terminal_id, exit_code);
                }
                WebTerminalEventPayload::CwdChanged { terminal_id, cwd } => self.terms.sessions.cwd_changed(&terminal_id, &cwd),
                _ => {}
            },
        }
    }

    fn terminal_spawned(&mut self, call: CallId, s: SpawnedTerminal) {
        let spawned = Spawned { id: s.id.clone(), shell: s.shell, cwd: s.cwd, pid: s.pid, cols: s.cols, rows: s.rows, claim: s.claim.clone() };
        let Some((session, pane)) = self.terms.sessions.spawned(call, spawned) else { return };
        let (project, cols, rows) = (session.project_id.clone(), session.cols, session.rows);
        log::info!("[term] spawned {} (pid {}) for {project}", s.id, session.pid);
        if self.workspace.project() != Some(project.as_str()) {
            // The project closed or switched before the PTY started: release it, as React does
            // when the pane it was asked for is gone.
            log::info!("[term] released {}: project {project} is no longer shown", s.id);
            self.terms.sessions.close(&s.id);
            self.terms.request(WebTerminalRequestType::Kill, json!({ "terminalId": s.id }), true);
            return;
        }
        self.terminal_attach(&s.id, &s.claim, cols, rows);
        self.workspace.step(WorkspaceEvent::AddTerminalTab(s.id, pane));
    }

    /// Opens the cells of a session this client now holds and attaches from the start, so the
    /// host replays what the PTY printed.
    fn terminal_attach(&mut self, id: &str, claim: &str, cols: u16, rows: u16) {
        let Some(host) = &mut self.terms.host else { return };
        let cells = host.open(id, cols, rows);
        self.terms.views.insert(id.to_owned(), TermView::new(cells));
        self.terms.opened += 1;
        self.terms.names.insert(id.to_owned(), format!("Terminal {}", self.terms.opened));
        self.terms.send(Command::Attach { terminal_id: id.to_owned(), claim: claim.to_owned(), log_epoch: None, last_seq: 0 });
    }

    /// The `list_preserved` reply for a restored tab: re-attach its PTY when the host still
    /// runs it, as React's restore does.
    fn terminal_restored(&mut self, call: CallId, result: Result<Value, client::terminal::TermError>) {
        let terminal_id = self.terms.sessions.restoring.get(&call).map(|r| r.terminal_id.clone()).unwrap_or_default();
        let found = match result {
            Ok(data) => preserved_entry(&data, &terminal_id),
            Err(e) => {
                log::warn!("[term] list_preserved for a restored tab failed: {} {}", e.code, e.message);
                None
            }
        };
        let Some(session) = self.terms.sessions.restored(call, found) else {
            log::info!("[term] restored tab {terminal_id} has no preserved PTY to attach");
            return;
        };
        let (id, claim, cols, rows, pid) = (session.id.clone(), session.claim.clone(), session.cols, session.rows, session.pid);
        log::info!("[term] re-attaching restored terminal {id} (pid {pid})");
        self.terminal_attach(&id, &claim, cols, rows);
    }

    /// A closed terminal tab kills its PTY, when this client holds it. A tab that went because
    /// the project switched is not closed: its session stays. A shown tab for a PTY this client
    /// does not hold (a saved layout brought it back) asks the host whether it still runs.
    fn reap_closed_tabs(&mut self) -> bool {
        let Some(project) = self.workspace.project().map(str::to_owned) else { return false };
        let now: BTreeSet<String> = model::workspace::leaves(self.workspace.root())
            .into_iter()
            .flat_map(|leaf| leaf.tabs.iter())
            .filter_map(|tab| match tab {
                Tab::Terminal { terminal_id, .. } => Some(terminal_id.clone()),
                _ => None,
            })
            .collect();
        let closed: Vec<String> = match &self.terms.shown {
            Some((shown_project, before)) if *shown_project == project => before.difference(&now).cloned().collect(),
            _ => Vec::new(),
        };
        if self.terms.host.is_some() {
            for id in now.iter().filter(|id| self.terms.sessions.should_restore(id)).cloned().collect::<Vec<_>>() {
                let call = self.terms.request(WebTerminalRequestType::ListPreserved, json!({ "projectId": project, "terminalId": id }), false);
                self.terms.sessions.restore(call, &project, &id);
                log::info!("[term] asking whether restored tab {id} still runs (call {call})");
            }
        }
        self.terms.shown = Some((project, now));
        for id in &closed {
            self.terms.views.remove(id);
            self.terms.names.remove(id);
            if let Some(host) = &mut self.terms.host {
                host.forget(id);
            }
            if self.terms.sessions.close(id).is_none() {
                log::info!("[term] tab closed for {id}, which this client does not hold; nothing to kill");
                continue;
            }
            log::info!("[term] tab closed; killing {id}");
            self.terms.request(WebTerminalRequestType::Kill, json!({ "terminalId": id }), true);
            self.terms.send(Command::Detach { terminal_id: id.clone() });
        }
        !closed.is_empty()
    }

    /// A key while terminal `id` has focus: true when the terminal took it.
    pub(crate) fn terminal_key(&mut self, id: &str, key: &KeyEvent, tx: &mut Tx) -> bool {
        self.terms.focused = Some(id.to_owned());
        // The platform modifier (⌘, or Ctrl elsewhere) with C copies a selection and with A
        // selects everything, as React's clipboard keys do; ^C without a selection interrupts.
        if key.mods.cmd && !key.mods.alt && !key.mods.shift && let rune::Key::Text(t) = &key.key {
            match t.to_ascii_lowercase().as_str() {
                "c" if self.terminal_copy(id, tx) => return true,
                "a" => {
                    self.terminal_select_all(id);
                    return true;
                }
                _ => {}
            }
        }
        if let Some(chord) = crate::keys::chord(key, self.shortcuts.platform)
            && model::shortcuts::find_match(&self.shortcuts, &chord).is_some()
        {
            return false;
        }
        let Some(view) = self.terms.views.get_mut(id) else { return false };
        let app_cursor = view.cells.lock().map(|t| t.modes().application_cursor_keys).unwrap_or(false);
        if key.mods.shift && matches!(key.key, rune::Key::PageUp | rune::Key::PageDown) {
            let (rows, history) = view.cells.lock().map(|t| (t.rows(), t.screen_start())).unwrap_or((0, 0));
            let page = rows.saturating_sub(1).max(1);
            view.scrolled = if matches!(key.key, rune::Key::PageUp) {
                (view.scrolled + page).min(history)
            } else {
                view.scrolled.saturating_sub(page)
            };
            return true;
        }
        let Some(bytes) = ui::terminal::keys::encode(key, app_cursor) else { return false };
        view.scrolled = 0;
        view.selection = None;
        self.terminal_write(id, &String::from_utf8_lossy(&bytes));
        true
    }

    /// The wheel over terminal `id` moved its content `dy` points down: scroll the scrollback.
    pub(crate) fn terminal_wheel(&mut self, id: &str, dy: f32) -> bool {
        let cell_h = self.terms.metrics().cell_h;
        let Some(view) = self.terms.views.get_mut(id) else { return false };
        let history = view.cells.lock().map(|t| t.screen_start()).unwrap_or(0);
        view.wheel += dy / cell_h;
        let lines = view.wheel.trunc();
        view.wheel -= lines;
        view.scrolled = (view.scrolled as f32 + lines).clamp(0.0, history as f32) as usize;
        true
    }

    pub(crate) fn terminal_write(&mut self, id: &str, data: &str) {
        let live = self.terms.sessions.get(id).is_some_and(|s| s.status == model::terminal::Status::Live);
        if live {
            self.terms.request(WebTerminalRequestType::Write, json!({ "terminalId": id, "data": data }), true);
        }
    }

    /// The pane holding terminal `id` was laid out at `width` x `height` points of content.
    pub(crate) fn terminal_measured(&mut self, id: &str, width: f32, height: f32, _tx: &mut Tx) {
        let (cols, rows) = self.terms.metrics().fit(width, height);
        if !self.terms.sessions.resize(id, cols, rows) {
            return;
        }
        if let Some(view) = self.terms.views.get(id)
            && let Ok(mut term) = view.cells.lock()
        {
            term.resize(cols, rows);
        }
        log::info!("[term] resize {id} to {cols}x{rows}");
        self.terms.request(WebTerminalRequestType::Resize, json!({ "terminalId": id, "cols": cols, "rows": rows }), false);
    }
}

/// The `list_preserved` entry for `terminal_id` with a claim, if the reply holds one.
fn preserved_entry(data: &Value, terminal_id: &str) -> Option<Spawned> {
    let entry = data.get("terminals")?.as_array()?.iter().find(|e| e.get("id").and_then(Value::as_str) == Some(terminal_id))?;
    let text = |key: &str| entry.get(key).and_then(Value::as_str).map(str::to_owned);
    let number = |key: &str| entry.get(key).and_then(Value::as_u64);
    Some(Spawned {
        id: terminal_id.to_owned(),
        shell: text("shell").unwrap_or_default(),
        cwd: text("cwd").unwrap_or_default(),
        pid: number("pid").and_then(|n| u32::try_from(n).ok()).unwrap_or(0),
        cols: number("cols").and_then(|n| u16::try_from(n).ok()).unwrap_or(DEFAULT_SIZE.0),
        rows: number("rows").and_then(|n| u16::try_from(n).ok()).unwrap_or(DEFAULT_SIZE.1),
        claim: text("claim")?,
    })
}

#[cfg(test)]
#[path = "terminal_tests.rs"]
mod tests;
