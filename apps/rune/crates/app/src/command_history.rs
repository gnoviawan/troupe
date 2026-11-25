//! The command history modal (`CommandHistoryModal`): the project's saved commands, newest
//! first, filtered by a search field, for this project or every project read so far. Up and
//! Down move, Return or a press inserts the command at the active terminal's prompt (no Return
//! is sent), and "Clear History" asks first (`ConfirmDialog`), then empties the saved list.
//!
//! The list lives at `projects/<id>/command-history` in this device's store, which the React
//! client writes as its command editor submits (R8 brings that editor here).

use std::collections::BTreeMap;

use model::workspace::{Query, Reply, Tab};
use rune::{El, TextEdit, col, el, icon, row, text, text_input};
use serde_json::{Value, json};
use theme::Tokens;
use troupe_protocol::ws::{WsError, WsRequestType};
use ui::overlay::{ConfirmDialog, ConfirmVariant, Select};
use ui::type_::{mono, sans};

use crate::State;
use crate::sync::Pending;

pub const KEY: &str = "command-history";
pub const FILTER: &str = "command-history-filter";
pub const CONFIRM: &str = "command-history-clear";

/// One saved command (`CommandHistoryEntry`).
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub id: String,
    pub command: String,
    pub terminal_name: String,
    pub project_id: String,
    /// Unix ms.
    pub timestamp: f64,
}

impl Entry {
    fn parse(v: &Value) -> Option<Entry> {
        let s = |k: &str| v.get(k).and_then(Value::as_str).map(str::to_owned);
        Some(Entry {
            id: s("id").unwrap_or_default(),
            command: s("command")?,
            terminal_name: s("terminalName").unwrap_or_default(),
            project_id: s("projectId").unwrap_or_default(),
            timestamp: v.get("timestamp").and_then(Value::as_f64).unwrap_or(0.0),
        })
    }
}

#[derive(Default)]
pub struct HistoryUi {
    pub query: TextEdit,
    /// The highlighted row among the filtered ones.
    pub lit: usize,
    pub all_projects: bool,
    /// Each project's list as last read, newest first.
    pub loaded: BTreeMap<String, Vec<Entry>>,
    /// The clear is waiting for the host.
    pub clearing: bool,
}

/// The device key of `project`'s list (`PersistenceKeys.commandHistory`).
fn key_of(project: &str) -> String {
    format!("projects/{project}/command-history")
}

/// `formatTime`: how long ago `at` was, from `now` (both Unix ms).
pub fn ago(at: f64, now: f64) -> String {
    let ms = now - at;
    let (mins, hours, days) = ((ms / 60_000.0).floor(), (ms / 3_600_000.0).floor(), (ms / 86_400_000.0).floor());
    if mins < 1.0 {
        return "Just now".into();
    }
    if mins < 60.0 {
        return format!("{mins}m ago");
    }
    if hours < 24.0 {
        return format!("{hours}h ago");
    }
    if days < 7.0 {
        return format!("{days}d ago");
    }
    // `toLocaleDateString` in en-US, from the days since the epoch (Hinnant's civil date).
    let z = (at / 86_400_000.0).floor() as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{month}/{day}/{year}")
}

impl State {
    /// The list the modal shows: this project's, or every read project's by time.
    pub(crate) fn history_entries(&self) -> Vec<&Entry> {
        let h = &self.terms.history;
        let mut base: Vec<&Entry> = if h.all_projects {
            let mut all: Vec<&Entry> = h.loaded.values().flatten().collect();
            all.sort_by(|a, b| b.timestamp.total_cmp(&a.timestamp));
            all
        } else {
            self.active_project().and_then(|p| h.loaded.get(&p.id)).map(|l| l.iter().collect()).unwrap_or_default()
        };
        let q = h.query.text.to_lowercase();
        if !q.is_empty() {
            base.retain(|e| e.command.to_lowercase().contains(&q));
        }
        base
    }

    /// The Command History shortcut or the drawer's History: open the modal and read the
    /// visible project's list again.
    pub(crate) fn open_command_history(&mut self) {
        let h = &mut self.terms.history;
        h.query = TextEdit::new("");
        h.lit = 0;
        h.all_projects = false;
        h.clearing = false;
        self.overlays.open(KEY);
        let Some(project) = self.active_project().map(|p| p.id.clone()) else { return };
        let key = self.device_key(&key_of(&project));
        self.call(WsRequestType::StoreRead, json!({ "key": key }), Pending::ReadHistory(project));
    }

    pub(crate) fn history_read(&mut self, project: String, result: Result<Option<Value>, WsError>) {
        match result {
            Ok(reply) => {
                let list: Vec<Entry> = crate::sync::stored_value(reply)
                    .and_then(|v| v.as_array().map(|a| a.iter().filter_map(Entry::parse).collect()))
                    .unwrap_or_default();
                log::info!("[history] {} commands saved for {project}", list.len());
                self.terms.history.loaded.insert(project, list);
            }
            Err(e) => log::warn!("[history] reading the command history of {project} failed: {}", e.code),
        }
    }

    /// Up or Down moves the highlight; Return inserts the highlighted command.
    pub(crate) fn history_key(&mut self, step: Option<isize>) {
        let len = self.history_entries().len();
        if len == 0 {
            return;
        }
        let at = self.terms.history.lit.min(len - 1);
        match step {
            Some(d) => self.terms.history.lit = (at as isize + d).clamp(0, len as isize - 1) as usize,
            None => {
                let command = self.history_entries()[at].command.clone();
                self.insert_history_command(&command);
            }
        }
    }

    /// `handleInsertCommand`: the command goes to the active terminal's prompt, unsent.
    pub(crate) fn insert_history_command(&mut self, command: &str) {
        self.close(KEY);
        let active = match model::workspace::query(&self.workspace.store, Query::GetActiveTab) {
            Reply::Tab(Some(Tab::Terminal { terminal_id, .. })) => Some(terminal_id),
            _ => self.terms.focused.clone(),
        };
        let Some(id) = active.filter(|id| self.terms.views.contains_key(id)) else {
            log::info!("[history] no active terminal to insert into");
            return;
        };
        log::info!("[history] inserting a saved command into {id}");
        self.terminal_write(&id, command);
    }

    /// The confirm's Clear: write the empty list, then drop it here once the host took it.
    pub(crate) fn clear_command_history(&mut self) {
        if self.terms.history.clearing {
            return;
        }
        let Some(project) = self.active_project().map(|p| p.id.clone()) else { return };
        self.terms.history.clearing = true;
        self.write(&key_of(&project), json!([]), Pending::ClearHistory(project));
    }

    pub(crate) fn history_cleared(&mut self, project: String, result: Result<Option<Value>, WsError>) {
        self.terms.history.clearing = false;
        match result {
            Ok(_) => {
                log::info!("[history] cleared the command history of {project}");
                self.terms.history.loaded.insert(project, Vec::new());
                self.close(CONFIRM);
            }
            Err(e) => {
                log::warn!("[history] clearing the command history of {project} failed: {}: {}", e.code, e.message);
                self.toast_error("Failed to clear history", &e.message);
            }
        }
    }
}

fn glyph(name: &str) -> &'static rune::Icon {
    theme::icons::named(name).unwrap_or(&theme::icons::X)
}

fn kbd(t: &Tokens, key: &str, what: &str) -> El {
    let c = &t.colors;
    row()
        .items_center()
        .gap(4.0)
        .child(text(key.to_owned()).font(sans(12.0, 400.0, 16.0)).color(c.foreground).px(4.0).radius(4.0).fill(c.secondary))
        .child(text(what.to_owned()).font(sans(12.0, 400.0, 16.0)).color(c.muted_foreground))
}

/// The open modal and its confirm, pushed onto `layer`.
pub fn build(s: &State, t: &'static Tokens, phone: bool, height: f32, layer: &mut Vec<El>) {
    if !s.overlays.is_open(KEY) {
        return;
    }
    let c = &t.colors;
    let h = &s.terms.history;
    let mut filter_layer = Vec::new();
    let filter = Select::new(FILTER, &["This Project", "All Projects"])
        .value(Some(usize::from(h.all_projects)))
        .label("Filter")
        .width(140.0)
        .open(s.overlays.is_open(FILTER))
        .on_open(|s: &mut State| s.overlays.open(FILTER))
        .on_pick(|s: &mut State, i: usize| {
            s.terms.history.all_projects = i == 1;
            s.terms.history.lit = 0;
            s.close(FILTER);
        })
        .on_close(|s: &mut State| s.close(FILTER))
        .build(t, &mut filter_layer);
    let header = row()
        .key("command-history-header")
        .w_full()
        .px(16.0)
        .py(12.0)
        .gap(8.0)
        .items_center()
        .justify_between()
        .border_sides([0.0, 0.0, 1.0, 0.0], c.border)
        .child(
            row()
                .gap(8.0)
                .items_center()
                .child(icon(glyph("History"), 18.0).color(c.muted_foreground).a11y_hidden())
                .child(text("Command History").font(sans(14.0, 500.0, 20.0)).color(c.foreground).role("heading")),
        )
        .child(filter);
    let field = text_input(&h.query, sans(14.0, 400.0, 20.0))
        .single_line()
        .key("command-history-search")
        .label("Search commands")
        .placeholder("Search commands...".to_owned(), c.muted_foreground)
        .color(c.foreground)
        .w_full()
        .px(12.0)
        .py(8.0)
        .radius(4.0)
        .fill(c.background)
        .border(1.0, c.border)
        .focus_ring(1.0, 0.0, c.focus_ring)
        .autofocus()
        .on_submit(|s: &mut State, _tx: &mut rune::Tx| s.history_key(None))
        .on_input_key(|s: &mut State, key: &rune::KeyEvent, _tx: &mut rune::Tx| {
            let step = match key.key {
                rune::Key::Up => -1,
                rune::Key::Down => 1,
                _ => return false,
            };
            s.history_key(Some(step));
            true
        })
        .on_edit(|s: &mut State| {
            s.terms.history.lit = 0;
            &mut s.terms.history.query
        });
    let entries = s.history_entries();
    let lit = h.lit.min(entries.len().saturating_sub(1));
    let now = crate::ids::now_ms();
    let body = if entries.is_empty() {
        let empty = if h.all_projects { h.loaded.values().all(Vec::is_empty) } else { s.active_project().and_then(|p| h.loaded.get(&p.id)).is_none_or(Vec::is_empty) };
        col()
            .key("command-history-empty")
            .w_full()
            .padding(32.0)
            .items_center()
            .gap(8.0)
            .child(icon(glyph("History"), 32.0).color(c.muted_foreground.alpha(0.5)).a11y_hidden())
            .child(
                text(if empty { "No command history yet" } else { "No matching commands" })
                    .font(sans(14.0, 400.0, 20.0))
                    .color(c.muted_foreground),
            )
    } else {
        let mut list = col().key("command-history-list").role("listbox").label("Commands").w_full();
        for (i, e) in entries.iter().enumerate() {
            let command = e.command.clone();
            let mut item = col()
                .key(format!("command-history-{i}"))
                .role("option")
                .label(e.command.clone())
                .focus_selected(i == lit)
                .w_full()
                .px(16.0)
                .py(12.0)
                .hover(c.foreground.alpha(0.03))
                .child(text(e.command.clone()).font(mono(14.0, 400.0, 20.0)).color(c.foreground).w_full())
                .child(
                    row()
                        .mt(4.0)
                        .gap(12.0)
                        .items_center()
                        .child(
                            row().gap(4.0).items_center().child(icon(glyph("Terminal"), 12.0).color(c.muted_foreground)).child(
                                text(e.terminal_name.clone()).font(sans(12.0, 400.0, 16.0)).color(c.muted_foreground),
                            ),
                        )
                        .child(
                            row().gap(4.0).items_center().child(icon(glyph("Clock"), 12.0).color(c.muted_foreground)).child(
                                text(ago(e.timestamp, now)).font(sans(12.0, 400.0, 16.0)).color(c.muted_foreground),
                            ),
                        ),
                )
                .on_press(move |s: &mut State| s.insert_history_command(&command));
            if i == lit {
                item = ui::paint::shadows(item.fill(c.keycap_fill), &t.shadows.keycap_shadow);
            }
            list = list.child(item);
        }
        col().w_full().max_h(height * 0.5).child(list).scroll(rune::ScrollSpec { stick_bottom: false })
    };
    let can_clear = !h.all_projects && !h.clearing && s.active_project().and_then(|p| h.loaded.get(&p.id)).is_some_and(|l| !l.is_empty());
    let mut clear = row()
        .key("command-history-clear-button")
        .role("button")
        .label(if h.clearing { "Clearing..." } else { "Clear History" })
        .px(8.0)
        .py(4.0)
        .gap(4.0)
        .items_center()
        .radius(4.0)
        .child(icon(glyph("Trash2"), 12.0).color(c.destructive))
        .child(text(if h.clearing { "Clearing..." } else { "Clear History" }).font(sans(12.0, 400.0, 16.0)).color(c.destructive));
    clear = if can_clear {
        clear.hover(c.secondary.alpha(0.5)).on_press(|s: &mut State| s.overlays.open(CONFIRM))
    } else {
        clear.opacity(0.5).disabled(true)
    };
    let mut footer = row()
        .key("command-history-footer")
        .w_full()
        .px(16.0)
        .py(8.0)
        .items_center()
        .justify_between()
        .fill(c.background)
        .border_sides([1.0, 0.0, 0.0, 0.0], c.border);
    footer = if phone {
        footer.child(el().grow(1.0))
    } else {
        footer.child(row().gap(16.0).child(kbd(t, "↑↓", "to navigate")).child(kbd(t, "↵", "to insert")).child(kbd(t, "Esc", "to close")))
    };
    let card = col()
        .key("command-history-panel")
        .role("dialog")
        .label("Command History")
        .w_full()
        .max_w(672.0)
        .radius(12.0)
        .clip_rounded(12.0)
        .fill(c.card)
        .border(1.0, c.border)
        .child(header)
        .child(col().w_full().padding(8.0).border_sides([0.0, 0.0, 1.0, 0.0], c.border).child(field))
        .child(body)
        .child(footer.child(clear))
        .modal(true, |s: &mut State| s.close(KEY));
    layer.push(
        el().key("command-history-scrim").absolute(0.0, 0.0).w_full().h_full().fill(c.overlay.alpha(0.4)).backdrop_blur(4.0).a11y_hidden(),
    );
    layer.push(
        col()
            .key("command-history-layer")
            .absolute(0.0, 0.0)
            .w_full()
            .h_full()
            .items_center()
            .px(16.0)
            .pt((height * 0.1).max(16.0))
            .child(ui::paint::shadows(card, &t.shadows.elevation_2xl)),
    );
    layer.extend(filter_layer);
    ConfirmDialog::new(
        CONFIRM,
        "Clear Command History",
        "Are you sure you want to clear the command history for this project? This action cannot be undone.",
    )
    .confirm_label("Clear")
    .cancel_label("Cancel")
    .variant(ConfirmVariant::Danger)
    .loading(h.clearing)
    .open(s.overlays.is_open(CONFIRM))
    .on_cancel(|s: &mut State| s.close(CONFIRM))
    .on_confirm(|s: &mut State| s.clear_command_history())
    .build(t, layer);
}

#[cfg(test)]
#[path = "command_history_tests.rs"]
mod tests;
