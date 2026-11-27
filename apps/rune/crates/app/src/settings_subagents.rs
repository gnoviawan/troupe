//! Settings → Sub-agents (`subagent-settings-group.tsx`, `ProfileRow.tsx` and
//! `use-subagent-settings.ts`): the orchestrator's switch and caps, and the profiles a calling
//! agent can start. Edits show at once and reach the host one at a time, each built on the
//! host's answer to the one before. The rules are `model::settings::subagents`.

use std::collections::VecDeque;

use model::settings::subagents::{self as rules, Object};
use rune::{El, TextEdit, col, row, text};
use serde_json::{Map, Value, json};
use theme::Tokens;
use troupe_protocol::ws::{WsError, WsRequestType};
use ui::base::Switch;
use ui::overlay::{Menu, MenuItem};
use ui::settings::{Stepper, card, group, letter_tile, notice, segmented, setting, stepper, switch_row};
use ui::type_::{mono, sans};
use ui::{Button, ButtonSize, Input, Textarea};

use crate::State;
use crate::sync::Pending;

pub const PAGE: &str = "subagents";
const MENU: &str = "subagent-profile-menu";

/// One change, turned into the host's input against the newest view when it is sent.
#[derive(Clone, Debug, PartialEq)]
pub enum Change {
    Enabled(bool),
    MaxRunning(i64),
    MaxDepth(i64),
    /// Fields of the profile named `id`, merged over it; a new `id` renames it.
    Edit { id: String, patch: Map<String, Value> },
    Add(Object),
    Delete(String),
}

impl Change {
    /// The input that makes this change on `view`, or None when its profile is gone.
    fn input(&self, view: &Value) -> Option<Value> {
        let profiles = rules::profiles_of(view);
        Some(match self {
            Self::Enabled(on) => json!({ "enabled": on }),
            Self::MaxRunning(n) => json!({ "maxRunning": n }),
            Self::MaxDepth(n) => json!({ "maxDepth": n }),
            Self::Delete(id) => json!({ "deleteProfiles": [id] }),
            Self::Add(profile) => json!({ "upsertProfiles": rules::profile_upserts(&profiles, profile, None) }),
            Self::Edit { id, patch } => {
                let current = profiles.iter().find(|p| p["id"] == json!(id))?;
                let mut next = rules::stored(current);
                next.extend(patch.clone());
                let previous = (next["id"] != json!(id)).then_some(id.as_str());
                json!({ "upsertProfiles": rules::profile_upserts(&profiles, &next, previous) })
            }
        })
    }
}

#[derive(Default)]
pub struct SubagentsUi {
    /// The host's last answer.
    pub host: Option<Value>,
    pub failed: bool,
    /// Changes not answered yet, oldest first; the first is on the wire once `sending`.
    pub queued: VecDeque<Change>,
    sending: bool,
    pub expanded: Option<String>,
    pub menu: Option<String>,
    /// The open row's name and note fields.
    pub name: TextEdit,
    pub note: TextEdit,
}

/// What a sub-agent settings reply is for.
#[derive(Clone, Debug, PartialEq)]
pub enum SubagentCall {
    Read,
    Set,
}

fn patch(pairs: &[(&str, Value)]) -> Map<String, Value> {
    pairs.iter().map(|(k, v)| ((*k).to_owned(), v.clone())).collect()
}

impl State {
    /// The host's view with every queued change applied.
    pub(crate) fn subagents_view(&self) -> Option<Value> {
        let ui = &self.subagents_ui;
        let host = ui.host.clone()?;
        Some(ui.queued.iter().fold(host, |view, change| change.input(&view).map_or_else(|| view.clone(), |input| rules::apply_input(&view, &input))))
    }

    /// The page opened: read the settings.
    pub(crate) fn subagents_opened(&mut self) {
        self.call(WsRequestType::SubagentSettingsGet, json!({}), Pending::Subagents(SubagentCall::Read));
    }

    pub(crate) fn subagents_change(&mut self, change: Change) {
        self.subagents_ui.queued.push_back(change);
        self.subagents_send();
    }

    fn subagents_send(&mut self) {
        while !self.subagents_ui.sending {
            let Some(change) = self.subagents_ui.queued.front().cloned() else { return };
            let Some(host) = self.subagents_ui.host.clone() else { return };
            match change.input(&host) {
                Some(input) => {
                    self.subagents_ui.sending = true;
                    self.call(WsRequestType::SubagentSettingsSet, input, Pending::Subagents(SubagentCall::Set));
                }
                None => {
                    self.subagents_ui.queued.pop_front();
                }
            }
        }
    }

    pub(crate) fn subagents_answered(&mut self, call: SubagentCall, result: Result<Option<Value>, WsError>) {
        match (call, result) {
            (SubagentCall::Read, Ok(view)) => {
                self.subagents_ui.host = view;
                self.subagents_ui.failed = false;
            }
            (SubagentCall::Read, Err(e)) => {
                log::warn!("[SubagentSettingsGroup] Failed to read the sub-agent settings: {}", e.message);
                self.subagents_ui.host = None;
                self.subagents_ui.failed = true;
            }
            (SubagentCall::Set, result) => {
                self.subagents_ui.sending = false;
                let change = self.subagents_ui.queued.pop_front();
                match result {
                    Ok(view) => self.subagents_ui.host = view.or(self.subagents_ui.host.take()),
                    Err(e) => {
                        self.toasts.push(self.now, ui::overlay::ToastKind::Error, "Could not save the sub-agent setting.", Some(&e.message));
                        log::warn!("[SubagentSettingsGroup] Failed to set subagent settings: {}", e.message);
                        self.subagents_refused(change);
                        self.subagents_opened();
                    }
                }
            }
        }
        self.subagents_send();
    }

    /// A refused add or rename puts the open row back.
    fn subagents_refused(&mut self, change: Option<Change>) {
        let ui = &mut self.subagents_ui;
        match change {
            Some(Change::Add(p)) if ui.expanded.as_deref() == p.get("id").and_then(Value::as_str) => ui.expanded = None,
            Some(Change::Edit { id, patch }) => {
                if let Some(to) = patch.get("id").and_then(Value::as_str).filter(|to| ui.expanded.as_deref() == Some(*to)) {
                    let _ = to;
                    ui.expanded = Some(id);
                }
            }
            _ => {}
        }
    }

    fn subagents_profile(&self, id: &str) -> Option<Object> {
        self.subagents_view().map(|v| rules::profiles_of(&v)).and_then(|ps| ps.into_iter().find(|p| p["id"] == json!(id)))
    }

    /// The open row's name and note, saved as it closes (React saves them on blur).
    fn subagents_commit_open(&mut self) {
        let Some(id) = self.subagents_ui.expanded.clone() else { return };
        let Some(profile) = self.subagents_profile(&id) else { return };
        let note = self.subagents_ui.note.text.trim().to_owned();
        let saved = profile.get("description").and_then(Value::as_str).unwrap_or_default();
        if note != saved {
            let value = if note.is_empty() { Value::Null } else { json!(note) };
            self.subagents_change(Change::Edit { id: id.clone(), patch: patch(&[("description", value)]) });
        }
        self.subagents_rename(&id);
    }

    /// `NameField`'s commit: a valid new name renames the profile.
    pub(crate) fn subagents_rename(&mut self, id: &str) {
        let next = self.subagents_ui.name.text.trim().to_owned();
        if next == id {
            return;
        }
        let others: Vec<Object> = self.subagents_view().map(|v| rules::profiles_of(&v)).unwrap_or_default().into_iter().filter(|p| p["id"] != json!(id)).collect();
        if rules::id_error(&next, &others).is_some() {
            return;
        }
        if self.subagents_ui.expanded.as_deref() == Some(id) {
            self.subagents_ui.expanded = Some(next.clone());
        }
        self.subagents_change(Change::Edit { id: id.to_owned(), patch: patch(&[("id", json!(next))]) });
    }

    pub(crate) fn subagents_toggle(&mut self, id: &str) {
        let closing = self.subagents_ui.expanded.as_deref() == Some(id);
        self.subagents_commit_open();
        if closing {
            self.subagents_ui.expanded = None;
            return;
        }
        self.subagents_expand(id);
    }

    fn subagents_expand(&mut self, id: &str) {
        let note = self.subagents_profile(id).and_then(|p| p.get("description").and_then(Value::as_str).map(str::to_owned)).unwrap_or_default();
        let ui = &mut self.subagents_ui;
        ui.expanded = Some(id.to_owned());
        ui.name = TextEdit::new(id);
        ui.note = TextEdit::new(note);
    }

    /// `addProfile`: a profile of `agent`, a copy of `base` when given, named after `stem`, opened.
    pub(crate) fn subagents_add(&mut self, agent: &str, stem: &str, base: Option<Object>) {
        let profiles = self.subagents_view().map(|v| rules::profiles_of(&v)).unwrap_or_default();
        let id = rules::unique_id(stem, &profiles);
        let profile = match base {
            Some(mut b) => {
                b.insert("id".into(), json!(id));
                b
            }
            None => rules::fresh(&id, agent),
        };
        self.subagents_commit_open();
        self.subagents_change(Change::Add(profile));
        self.subagents_expand(&id);
    }

    pub(crate) fn subagents_edit(&mut self, id: &str, pairs: &[(&str, Value)]) {
        self.subagents_change(Change::Edit { id: id.to_owned(), patch: patch(pairs) });
    }
}

fn glyph(name: &str) -> &'static rune::Icon {
    theme::icons::named(name).expect("bundled icon")
}

fn field_row(t: &Tokens, key: &str, label: &str, body: Vec<El>) -> El {
    col()
        .key(key.to_owned())
        .w_full()
        .gap(6.0)
        .items_stretch()
        .child(text(label.to_owned()).font(sans(14.0, 400.0, 20.0)).color(t.colors.foreground))
        .child(col().key("body").w_full().gap(4.0).items_stretch().children(body))
}

fn hint(t: &Tokens, words: &str, error: bool) -> El {
    text(words.to_owned()).font(sans(12.0, 400.0, 16.0)).color(if error { t.colors.destructive } else { t.colors.muted_foreground }).wrap()
}

/// `ProfileRow`.
fn profile_row(s: &State, t: &'static Tokens, p: &Object, all: &[Object], layer: &mut Vec<El>) -> El {
    let c = &t.colors;
    let id = p["id"].as_str().unwrap_or_default().to_owned();
    let agent_name = p.get("agentName").and_then(Value::as_str).unwrap_or(p["agentId"].as_str().unwrap_or_default()).to_owned();
    let installed = p.get("agentName").is_some_and(|n| !n.is_null());
    let skips = rules::skips_prompts(&rules::stored(p));
    let expanded = s.subagents_ui.expanded.as_deref() == Some(id.as_str());
    let mut words = col().key("words").min_w(0.0).grow(1.0).child(text(id.clone()).font(mono(14.0, 400.0, 20.0)).color(c.foreground).single_line().truncate()).child(
        text(rules::subtitle(p, &agent_name)).font(sans(12.0, 400.0, 16.0)).color(c.muted_foreground).single_line().truncate(),
    );
    if skips {
        words = words.label(format!("{id}, runs without asking for approval."));
    }
    let toggle_id = id.clone();
    let open = row()
        .key("open")
        .role("button")
        .label(id.clone())
        .focusable(true)
        .grow(1.0)
        .min_w(192.0)
        .gap(12.0)
        .items_center()
        .radius(6.0)
        .hover(c.foreground.alpha(0.03))
        .child(letter_tile(t, &agent_name, 28.0))
        .child(words)
        .on_press(move |s: &mut State| s.subagents_toggle(&toggle_id));
    let mut tools = row().key("tools").gap(8.0).items_center();
    if skips {
        tools = tools.child(rune::icon(glyph("ShieldAlert"), 13.0).color(c.warning).key("shield").a11y_hidden());
    }
    let on = p.get("enabled") == Some(&json!(true));
    let switch_id = id.clone();
    tools = tools.child(Switch::new(on).label(format!("Let agents start {id}")).build(t).key("switch").on_press(move |s: &mut State| s.subagents_edit(&switch_id, &[("enabled", json!(!on))])));
    let menu_key = format!("{MENU}-{id}");
    let (menu_id, dup, del) = (id.clone(), p.clone(), id.clone());
    let deletable = rules::can_delete(p, all);
    let trigger = ui::quiet_icon_button(t, glyph("MoreHorizontal"), format!("Actions for {id}"), 28.0, 14.0).key("actions").on_press(move |s: &mut State| s.subagents_ui.menu = Some(menu_id.clone()));
    let delete = MenuItem::new("Delete").icon(glyph("Trash2")).destructive().disabled(!deletable).on_select(move |s: &mut State| {
        s.subagents_ui.menu = None;
        if s.subagents_ui.expanded.as_deref() == Some(del.as_str()) {
            s.subagents_ui.expanded = None;
        }
        s.subagents_change(Change::Delete(del.clone()));
    });
    let mut menu = Menu::new(menu_key)
        .width(208.0)
        .open(s.subagents_ui.menu.as_deref() == Some(id.as_str()))
        .on_close(|s: &mut State| s.subagents_ui.menu = None)
        .item(MenuItem::new("Duplicate").icon(glyph("Copy")).on_select(move |s: &mut State| {
            s.subagents_ui.menu = None;
            let agent = dup["agentId"].as_str().unwrap_or_default().to_owned();
            let implicit = dup.get("implicit") == Some(&json!(true));
            let stem = if implicit { rules::agent_short_name(&agent) } else { dup["id"].as_str().unwrap_or_default().to_owned() };
            s.subagents_add(&agent, &stem, Some(rules::stored(&dup)));
        }))
        .separator()
        .item(delete);
    if !deletable {
        menu = menu.label("This agent's only profile. Switch it off instead.");
    }
    tools = tools.child(menu.build(t, layer, trigger));
    let head = row().key("head").w_full().px(16.0).py(12.0).gap(12.0).items_center().flex_wrap().child(open).child(tools);
    let mut out = col().key(format!("subagent-profile-{id}")).w_full().items_stretch().child(head);
    if expanded {
        out = out.child(details(s, t, p, all, &agent_name, installed));
    }
    out
}

/// The open row: name, agent, approvals and the note.
fn details(s: &State, t: &'static Tokens, p: &Object, all: &[Object], agent_name: &str, installed: bool) -> El {
    let c = &t.colors;
    let id = p["id"].as_str().unwrap_or_default().to_owned();
    let others: Vec<Object> = all.iter().filter(|o| o["id"] != json!(id)).cloned().collect();
    let draft = s.subagents_ui.name.text.trim().to_owned();
    let error = if draft == id { None } else { rules::id_error(&draft, &others) };
    let rename_id = id.clone();
    let name = Input::new(&s.subagents_ui.name)
        .label("Name")
        .invalid(error.is_some())
        .build(t)
        .key("name-input")
        .max_w(320.0)
        .on_edit(|s: &mut State| &mut s.subagents_ui.name)
        .on_input_key(move |s: &mut State, key, _| {
            if key.key == rune::Key::Enter {
                s.subagents_rename(&rename_id);
                return true;
            }
            false
        });
    let mut body = col()
        .key("details")
        .w_full()
        .px(16.0)
        .pb(16.0)
        .pt(4.0)
        .gap(16.0)
        .items_stretch()
        .label(format!("{id} settings"))
        .child(field_row(t, "name", "Name", vec![name, hint(t, error.unwrap_or("The calling agent passes this name to start the profile."), error.is_some())]));
    if installed {
        let lock = rules::agent_lock_reason(p, all).unwrap_or("Pick another agent from the tabs of the model picker.");
        body = body.child(field_row(t, "agent", "Agent", vec![text(agent_name.to_owned()).font(sans(14.0, 400.0, 20.0)).color(c.foreground), hint(t, lock, false)]));
    }
    let allow = p.get("approvals").and_then(Value::as_str) == Some("allowAll");
    let approvals_id = id.clone();
    let mut approvals = vec![segmented(t, "approvals", "Approvals", &[("Ask in chat", None), ("Allow everything", None)], usize::from(allow), false, move |i, e| {
        let id = approvals_id.clone();
        e.on_press(move |s: &mut State| {
            if (i == 1) != allow {
                s.subagents_edit(&id, &[("approvals", json!(if i == 1 { "allowAll" } else { "ask" }))]);
            }
        })
    })];
    if allow {
        approvals.push(
            row()
                .key("allow-warning")
                .gap(6.0)
                .items_start()
                .child(rune::icon(glyph("AlertTriangle"), 14.0).color(c.warning).a11y_hidden())
                .child(text("Troupe answers every permission prompt with Allow, except mode switches. Runs stay in their own worktree.").font(sans(12.0, 400.0, 19.5)).color(c.warning).wrap().grow(1.0).min_w(0.0)),
        );
    }
    body = body.child(field_row(t, "approvals-row", "Approvals", approvals));
    let note = Textarea::new(&s.subagents_ui.note).rows(3).placeholder("e.g. Fast at UI tweaks and CSS; weak at Rust.").label("When to use").build(t).key("note-input").on_edit(|s: &mut State| &mut s.subagents_ui.note);
    body.child(field_row(t, "note", "When to use", vec![note, hint(t, "Tells the calling agent what this profile is good at.", false)]))
}

/// The page.
pub fn page(s: &State, t: &'static Tokens, layer: &mut Vec<El>) -> Vec<El> {
    let ui = &s.subagents_ui;
    if ui.failed {
        return vec![group(t, "subagents", "Sub-agents", vec![], vec![notice(t, "failed", "Could not read the sub-agent settings. Agent chats keep working.")])];
    }
    let Some(view) = s.subagents_view() else {
        return vec![group(t, "subagents", "Sub-agents", vec![], vec![notice(t, "loading", "Loading sub-agent settings…")])];
    };
    let enabled = view["enabled"].as_bool().unwrap_or(false);
    let running = view["maxRunning"].as_i64().unwrap_or(rules::RUNNING.0);
    let depth = view["maxDepth"].as_i64().unwrap_or(rules::DEPTH.0);
    let toggle = switch_row(t, "subagent-enabled", "Let agents start other agents", "An agent chat can hand parts of a task to the profiles below. Their approvals come to that chat.", enabled, |e| {
        e.on_press(move |s: &mut State| s.subagents_change(Change::Enabled(!enabled)))
    });
    let step = |key: &'static str, label: &'static str, (dec, inc): (&'static str, &'static str), value: i64, (min, max): (i64, i64), make: fn(i64) -> Change| {
        stepper(t, Stepper { key, label, value: value.to_string(), decrement: dec, increment: inc, at_min: value <= min, at_max: value >= max, reset: None }, move |k, e| {
            let next = if k.ends_with("-dec") { value - 1 } else { value + 1 };
            e.on_press(move |s: &mut State| s.subagents_change(make(next)))
        })
    };
    let general = card(
        t,
        "general-card",
        vec![
            toggle,
            setting(t, "subagent-max-running", "Max running sub-agents", "Per chat. Extra starts are refused until one finishes.", Some(step("max-running", "Max running sub-agents", ("Fewer running sub-agents", "More running sub-agents"), running, rules::RUNNING, Change::MaxRunning))),
            setting(
                t,
                "subagent-max-depth",
                "Max depth",
                "How many levels of sub-agents can start their own sub-agents. 1 means sub-agents cannot start more.",
                Some(step("max-depth", "Max depth", ("Less depth", "More depth"), depth, rules::DEPTH, Change::MaxDepth)),
            ),
        ],
    );
    let profiles = rules::profiles_of(&view);
    let first = profiles.iter().find(|p| p.get("agentName").is_some_and(|n| !n.is_null())).and_then(|p| p["agentId"].as_str()).map(str::to_owned);
    let on = profiles.iter().filter(|p| p.get("enabled") == Some(&json!(true)) && p.get("agentName").is_some_and(|n| !n.is_null())).count();
    let add = Button::new("Add profile").size(ButtonSize::Sm).icon(glyph("Plus")).disabled(first.is_none()).build(t).key("add-profile").on_press(move |s: &mut State| {
        if let Some(agent) = first.clone() {
            s.subagents_add(&agent, &rules::agent_short_name(&agent), None);
        }
    });
    let rows: Vec<El> = if profiles.is_empty() {
        vec![setting(t, "no-agents", "No agents yet", "Install an agent and its default profile appears here.", None)]
    } else {
        profiles.iter().map(|p| profile_row(s, t, p, &profiles, layer)).collect()
    };
    let foot = text(rules::footnote(&profiles)).font(sans(12.0, 400.0, 16.0)).color(t.colors.muted_foreground).wrap().key("footnote");
    vec![
        group(t, "general", "General", vec![], vec![general]),
        group(t, "subagent-profiles", &format!("Profiles · {} · {on} on", profiles.len()), vec![add], vec![card(t, "profiles-card", rows), foot]),
    ]
}

#[cfg(test)]
#[path = "settings_subagents_tests.rs"]
mod tests;
