//! The composers the app mounts: one draft per chat tab and one per project's launcher, each a
//! rich field with pills (`ui::composer`). This module keeps the drafts and the data the menus
//! list (the agent's skills from `/skills`, the chat's commands from the model, the project's
//! chats), turns menu picks into pills, and sends a chat's draft through `model::acp`.

use std::collections::BTreeMap;
use std::rc::Rc;

use model::acp::Event;
use model::acp::prompt::SendOptions;
use rune::{ClipboardContent, Key, KeyEvent, TextEdit, Tx};
use serde_json::Value;
use troupe_protocol::ws::WsRequestType;
use ui::composer::field::{self, Trigger, TriggerKind};
use ui::composer::menu::{self, ChatRef, Item, Panel, Row, Skill};
use ui::composer::view::{Hooks, MenuView};
use ui::composer::{Attachment, Pill, Send};

use crate::State;
use crate::sync::Pending;

/// Which composer a draft belongs to.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Host {
    /// A chat tab's composer, by session.
    Chat(String),
    /// The launcher's composer, by project.
    Launcher(String),
}

impl Host {
    pub fn key(&self) -> String {
        match self {
            Host::Chat(id) => format!("composer-chat-{id}"),
            Host::Launcher(id) => format!("composer-launcher-{id}"),
        }
    }
}

/// One composer's draft and its menu state.
pub struct Draft {
    pub edit: TextEdit,
    pub attachments: Vec<Attachment>,
    pub highlight: usize,
    pub panel: Panel,
    /// The trigger start Escape closed the menu at; it stays closed until the trigger moves.
    dismissed: Option<usize>,
    pub add_open: bool,
    pub tools_open: bool,
}

impl Default for Draft {
    fn default() -> Self {
        Draft {
            edit: TextEdit::new("").rich(),
            attachments: Vec::new(),
            highlight: 0,
            panel: Panel::Root,
            dismissed: None,
            add_open: false,
            tools_open: false,
        }
    }
}

/// An agent config from `acp/agents` (`StoredAgentConfig`).
#[derive(Clone, Debug, PartialEq)]
pub struct AgentConfig {
    pub id: String,
    pub name: String,
    pub raw: Value,
}

#[derive(Default)]
pub struct ComposerUi {
    pub drafts: BTreeMap<Host, Draft>,
    /// The skills `/skills` listed, per project folder.
    pub skills: BTreeMap<String, Vec<Skill>>,
    pub agents: Vec<AgentConfig>,
    /// The launcher's agent (`agents/last-selected`), by config id.
    pub picked: Option<String>,
    /// Per session, the agent namespace its history names.
    pub namespaces: BTreeMap<String, String>,
    /// What a composer with no draft yet draws.
    pub empty: Draft,
    /// The agent reads went out on this link.
    pub agents_asked: bool,
    /// Chats the launcher started, by session, with their project: their dock takes the
    /// launcher's card in the morph.
    pub morphed: BTreeMap<String, String>,
    /// The launcher's chat being started.
    pub launching: Option<crate::launch::Launch>,
}

pub const NO_DRAFT: &str = "";

impl State {
    pub fn draft(&self, host: &Host) -> Option<&Draft> {
        self.composer.drafts.get(host)
    }

    pub fn draft_mut(&mut self, host: &Host) -> &mut Draft {
        self.composer.drafts.entry(host.clone()).or_default()
    }

    /// The project folder the composer's skills and mentions read.
    fn root_of(&self, host: &Host) -> Option<String> {
        let project = match host {
            Host::Launcher(id) => Some(id.clone()),
            Host::Chat(id) => self.chats.get(id).and_then(|c| c.project_id.clone()),
        }?;
        self.projects
            .store
            .projects
            .iter()
            .find(|p| p.id == project)
            .and_then(|p| p.path.clone())
    }

    fn project_of(&self, host: &Host) -> Option<String> {
        match host {
            Host::Launcher(id) => Some(id.clone()),
            Host::Chat(id) => self.chats.get(id).and_then(|c| c.project_id.clone()),
        }
    }

    /// The agent a composer's chip names.
    pub fn agent_label(&self, host: &Host) -> Option<String> {
        let agents = &self.composer.agents;
        let by_id = |id: &str| agents.iter().find(|a| a.id == id);
        let found = match host {
            Host::Chat(id) => self.composer.namespaces.get(id).and_then(|ns| {
                agents
                    .iter()
                    .find(|a| ns.strip_prefix("config:").unwrap_or(ns) == a.id)
            }),
            Host::Launcher(_) => None,
        };
        found
            .or_else(|| self.composer.picked.as_deref().and_then(by_id))
            .or_else(|| agents.first())
            .map(|a| a.name.clone())
    }

    /// The trigger at the draft's caret, unless Escape closed it there.
    fn open_trigger(&self, host: &Host) -> Option<Trigger> {
        let draft = self.draft(host)?;
        let trigger = field::trigger(&draft.edit)?;
        (draft.dismissed != Some(trigger.range.start)).then_some(trigger)
    }

    fn sections(&self, host: &Host, trigger: &Trigger) -> Vec<menu::Section> {
        match trigger.kind {
            TriggerKind::Slash => {
                let commands = match host {
                    Host::Chat(id) => self.chat.acp.commands.get(id).cloned().unwrap_or_default(),
                    Host::Launcher(_) => Vec::new(),
                };
                let root = self.root_of(host).unwrap_or_default();
                let skills = self.composer.skills.get(&root).cloned().unwrap_or_default();
                menu::slash_sections(&commands, &skills, &trigger.query)
            }
            TriggerKind::Mention => {
                let panel = self.draft(host).map(|d| d.panel).unwrap_or_default();
                let root = self.root_of(host).unwrap_or_default();
                let skip = match host {
                    Host::Chat(id) => id.as_str(),
                    Host::Launcher(_) => NO_DRAFT,
                };
                let chats: Vec<ChatRef> = self
                    .project_of(host)
                    .map(|p| {
                        self.chats
                            .of_project(&p)
                            .filter(|c| c.session_id != skip)
                            .map(|c| ChatRef {
                                title: c.label().to_owned(),
                                session_id: c.session_id.clone(),
                                agent: None,
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                menu::mention_sections(panel, &root, &chats, &trigger.query)
            }
        }
    }

    /// The menu a composer shows now, if any.
    pub fn menu_view(&self, host: &Host) -> Option<MenuView> {
        let trigger = self.open_trigger(host)?;
        let draft = self.draft(host)?;
        let sections = self.sections(host, &trigger);
        let (title, empty) = match (trigger.kind, draft.panel) {
            (TriggerKind::Slash, _) => (None, "No commands match. Try another name."),
            (TriggerKind::Mention, Panel::Conversations) => {
                (Some("Conversations".to_owned()), "No chats match. Try another title.")
            }
            (TriggerKind::Mention, Panel::Root) => (None, "Nothing matches. Try another name."),
        };
        let count = menu::flat(&sections).len();
        Some(MenuView {
            sections,
            highlight: draft.highlight.min(count.saturating_sub(1)),
            title,
            empty,
        })
    }

    /// The field changed: reset the highlight, and leave a drilled panel when the trigger goes.
    fn composer_changed(&mut self, host: &Host) {
        let trigger = self.draft(host).and_then(|d| field::trigger(&d.edit));
        let draft = self.draft_mut(host);
        draft.highlight = 0;
        if trigger.as_ref().is_none_or(|t| t.kind != TriggerKind::Mention) {
            draft.panel = Panel::Root;
        }
        if draft
            .dismissed
            .is_some_and(|at| trigger.as_ref().is_none_or(|t| t.range.start != at))
        {
            draft.dismissed = None;
        }
        if trigger.is_some_and(|t| t.kind == TriggerKind::Slash) {
            self.fetch_skills(host);
        }
    }

    /// Asks the host for the project's skills once per folder (`skillsApi.listSkills`).
    fn fetch_skills(&mut self, host: &Host) {
        let root = self.root_of(host).unwrap_or_default();
        if self.composer.skills.contains_key(&root) {
            return;
        }
        self.composer.skills.insert(root.clone(), Vec::new());
        let path = if root.is_empty() {
            "/skills".to_owned()
        } else {
            format!("/skills?projectRoot={}", ui::composer::encode_query(&root))
        };
        log::info!("[composer] listing skills for {root:?}");
        if let Some(host) = &mut self.host {
            host.fetch(path, crate::host::Fetch::Skills(root));
        }
    }

    pub(crate) fn skills_listed(&mut self, root: String, result: Result<Value, client::http::HttpError>) {
        match result.map(serde_json::from_value::<Vec<Skill>>) {
            Ok(Ok(list)) => {
                log::info!("[composer] {} skills for {root:?}", list.len());
                self.composer.skills.insert(root, list);
            }
            Ok(Err(e)) => log::warn!("[composer] unreadable skills list: {e}"),
            Err(e) => {
                log::warn!("[composer] listing skills failed: {e}");
                self.composer.skills.remove(&root);
            }
        }
    }

    /// The menu's keys: move, pick, back out and close. False when no menu is open.
    fn composer_key(&mut self, host: &Host, key: &KeyEvent, tx: &mut Tx) -> bool {
        let Some(view) = self.menu_view(host) else {
            return false;
        };
        let items: Vec<Item> = menu::flat(&view.sections).into_iter().cloned().collect();
        let draft = self.draft_mut(host);
        match key.key {
            Key::Down | Key::Up if !items.is_empty() => {
                let step = if key.key == Key::Down { 1 } else { items.len() - 1 };
                draft.highlight = (view.highlight + step) % items.len();
                tx.scroll_into_view(format!("{}-row-{}", host.key(), draft.highlight));
            }
            Key::Enter | Key::Tab if !key.mods.shift => match items.get(view.highlight) {
                Some(item) => self.composer_pick(host, item, tx),
                None => return false,
            },
            Key::Escape => {
                if draft.panel != Panel::Root {
                    draft.panel = Panel::Root;
                } else {
                    draft.dismissed = field::trigger(&draft.edit).map(|t| t.range.start);
                }
            }
            Key::Left if draft.panel != Panel::Root => draft.panel = Panel::Root,
            _ => return false,
        }
        true
    }

    pub(crate) fn composer_pick(&mut self, host: &Host, item: &Item, tx: &mut Tx) {
        let Some(trigger) = self.draft(host).and_then(|d| field::trigger(&d.edit)) else {
            return;
        };
        let pill = match &item.row {
            Row::Skill(skill) => Pill::Skill {
                name: skill.name.clone(),
                path: skill.path.clone(),
            },
            Row::Command { name, .. } => {
                let doc = field::doc_of(&self.draft_mut(host).edit);
                if doc.pills().any(|p| matches!(p, Pill::Command { .. })) {
                    log::warn!("[composer] Rejected a second slash command in one composer draft");
                    self.toast_error("This message already has a command.", "");
                    return;
                }
                Pill::Command { name: name.clone() }
            }
            Row::Chat(chat) => Pill::Conversation {
                title: chat.title.clone(),
                session_id: chat.session_id.clone(),
            },
            Row::Conversations => {
                let draft = self.draft_mut(host);
                draft.panel = Panel::Conversations;
                draft.highlight = 0;
                tx.focus(format!("{}-field", host.key()), false);
                return;
            }
            Row::Explorer => {
                self.toast_later("The file explorer in the mention menu");
                return;
            }
        };
        log::info!("[composer] pill {:?} into {}", pill_kind(&pill), host.key());
        let draft = self.draft_mut(host);
        field::put_pill(&mut draft.edit, trigger.range, &pill);
        draft.panel = Panel::Root;
        draft.highlight = 0;
        tx.focus(format!("{}-field", host.key()), true);
    }

    /// An add-menu row: put a trigger at the caret, a tool pill, or (later) the file picker.
    fn composer_add(&mut self, host: &Host, what: &str, tx: &mut Tx) {
        let draft = self.draft_mut(host);
        if what == "tools" {
            draft.tools_open = true;
            // The submenu's first tool (entry 0 is its label), as Radix moves focus into it.
            tx.focus(format!("{}-tools-item1", host.key()), true);
            return;
        }
        draft.add_open = false;
        draft.tools_open = false;
        match what {
            "@" | "/" => {
                let (a, _) = draft.edit.range();
                let before = draft.edit.text[..a].chars().next_back();
                let lead = if before.is_some_and(|c| !ui::composer::is_js_space(c)) {
                    " "
                } else {
                    ""
                };
                draft.edit.insert(&format!("{lead}{what}"));
            }
            "attach" => {
                self.toast_later("Attaching files from the picker");
                return;
            }
            id => {
                let (a, b) = draft.edit.range();
                field::put_pill(&mut draft.edit, a..b, &Pill::Tool { id: id.to_owned() });
            }
        }
        self.composer_changed(host);
        tx.focus(format!("{}-field", host.key()), true);
    }

    /// A paste with an image becomes an image attachment; text pastes as usual.
    fn composer_paste(&mut self, host: &Host, content: &ClipboardContent) -> bool {
        let Some(image) = content.image() else {
            return false;
        };
        let draft = self.draft_mut(host);
        let n = draft.attachments.len() + 1;
        log::info!(
            "[composer] pasted an image ({} bytes) into {}",
            image.png().len(),
            host.key()
        );
        draft.attachments.push(Attachment::Image {
            id: format!("paste-{n}"),
            name: "pasted-image.png".into(),
            mime_type: "image/png".into(),
            base64: ui::composer::base64(image.png()),
        });
        true
    }

    /// Send a chat's draft through the model (`ChatInputBar.submit`).
    pub fn composer_send(&mut self, host: &Host) {
        let Host::Chat(session) = host else {
            self.launch_from(host);
            return;
        };
        if !self.chat.loaded.contains(session) {
            log::warn!("[composer] send to {session} before its transcript loaded; kept the draft");
            return;
        }
        let Some(draft) = self.draft(host) else { return };
        let doc = field::doc_of(&draft.edit);
        let root = self.root_of(host).unwrap_or_default();
        let skills: Vec<(String, String)> = self
            .composer
            .skills
            .get(&root)
            .into_iter()
            .flatten()
            .map(|s| (s.name.clone(), s.path.clone()))
            .collect();
        let send = match ui::composer::compose(&doc, &skills, None, &draft.attachments) {
            Ok(Some(send)) => send,
            Ok(None) => return,
            Err(e) => {
                self.toast_error(&e.to_string(), "");
                return;
            }
        };
        log::info!("[composer] send to {session}: {}", send_kind(&send));
        let event = match send {
            Send::Text { text } => Event::SendPrompt(session.clone(), text),
            Send::Blocks { wire, display } => Event::SendPromptBlocks(
                session.clone(),
                wire,
                Some(SendOptions {
                    display_blocks: display,
                }),
            ),
        };
        let draft = self.draft_mut(host);
        draft.edit.clear();
        draft.attachments.clear();
        draft.panel = Panel::Root;
        self.acp_step(event);
    }

    /// The prompt command an `Effect::DispatchPrompt` asked for.
    pub(crate) fn dispatch_prompt(
        &mut self,
        agent_id: String,
        session_id: String,
        turn_id: String,
        blocks: Vec<Value>,
        display: Option<Vec<Value>>,
    ) {
        let mut payload = serde_json::json!({
            "agentId": agent_id, "sessionId": session_id, "content": blocks, "turnId": turn_id,
        });
        if let Some(display) = display {
            payload["displayContent"] = Value::Array(display);
        }
        // Ledgered under its turn id, so a dropped socket resends it and the host answers with
        // a receipt instead of running the turn twice.
        self.command(
            WsRequestType::SendPrompt,
            payload,
            Pending::Prompt { session_id, turn_id },
        );
    }

    /// The hooks one composer's view runs.
    pub fn composer_hooks(host: &Host) -> Hooks<State> {
        let h = |f: fn(&mut State, &Host, &mut Tx)| {
            let host = host.clone();
            Rc::new(move |s: &mut State, tx: &mut Tx| f(s, &host, tx)) as Rc<dyn Fn(&mut State, &mut Tx)>
        };
        let (edit_host, key_host, paste_host, pick_host, hover_host, menu_host, add_host, rm_host) = (
            host.clone(),
            host.clone(),
            host.clone(),
            host.clone(),
            host.clone(),
            host.clone(),
            host.clone(),
            host.clone(),
        );
        Hooks {
            edit: Rc::new(move |s: &mut State| &mut s.draft_mut(&edit_host).edit),
            send: h(|s, host, _| s.composer_send(host)),
            cancel: h(|s, host, _| {
                if let Host::Chat(id) = host {
                    s.acp_step(Event::CancelPrompt(id.clone()));
                }
            }),
            changed: h(|s, host, _| s.composer_changed(host)),
            key: Rc::new(move |s: &mut State, k: &KeyEvent, tx: &mut Tx| s.composer_key(&key_host, k, tx)),
            paste: Rc::new(move |s: &mut State, c: &ClipboardContent, _: &mut Tx| s.composer_paste(&paste_host, c)),
            pick: Rc::new(move |s: &mut State, item: &Item, tx: &mut Tx| s.composer_pick(&pick_host, item, tx)),
            hover: Rc::new(move |s: &mut State, n: usize| s.draft_mut(&hover_host).highlight = n),
            back: h(|s, host, tx| {
                s.draft_mut(host).panel = Panel::Root;
                tx.focus(format!("{}-field", host.key()), false);
            }),
            add_menu: Rc::new(move |s: &mut State, open: bool| {
                let draft = s.draft_mut(&menu_host);
                draft.add_open = open;
                draft.tools_open &= open;
            }),
            add: Rc::new(move |s: &mut State, what: &str, tx: &mut Tx| s.composer_add(&add_host, what, tx)),
            agent: h(|s, _, _| s.toast_later("Switching agents from the composer")),
            remove_attachment: Rc::new(move |s: &mut State, n: usize| {
                let draft = s.draft_mut(&rm_host);
                if n < draft.attachments.len() {
                    draft.attachments.remove(n);
                }
            }),
        }
    }

    /// The agent configs (`acp/agents`) as read.
    pub(crate) fn agents_loaded(&mut self, data: Option<Value>) {
        self.composer.agents = data
            .and_then(|v| v.as_array().cloned())
            .unwrap_or_default()
            .into_iter()
            .filter_map(|raw| {
                let id = raw.get("id").and_then(Value::as_str)?.to_owned();
                let name = raw.get("name").and_then(Value::as_str).unwrap_or(&id).to_owned();
                Some(AgentConfig { id, name, raw })
            })
            .collect();
        log::info!("[composer] {} agent configs", self.composer.agents.len());
    }

    pub(crate) fn last_agent_loaded(&mut self, data: Option<Value>) {
        self.composer.picked = data.and_then(|v| v.get("agentId").and_then(Value::as_str).map(str::to_owned));
    }
}

fn pill_kind(pill: &Pill) -> &'static str {
    match pill {
        Pill::Skill { .. } => "skill",
        Pill::Command { .. } => "command",
        Pill::File { .. } => "file",
        Pill::Context { .. } => "context",
        Pill::Conversation { .. } => "conversation",
        Pill::Tool { .. } => "tool",
    }
}

fn send_kind(send: &Send) -> String {
    match send {
        Send::Text { text } => format!("text, {} bytes", text.len()),
        Send::Blocks { wire, display } => {
            format!(
                "{} blocks, display {}",
                wire.len(),
                display.as_ref().map_or(0, Vec::len)
            )
        }
    }
}

#[cfg(test)]
#[path = "composer_tests.rs"]
mod tests;
