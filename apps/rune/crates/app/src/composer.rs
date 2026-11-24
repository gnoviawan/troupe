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
use ui::composer::menu::{self, ChatRef, Item, Listing, Panel, Row, Skill};
use ui::composer::view::{Hooks, MenuView, QueueAct, QueueRow};
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
    pub agent_open: bool,
    /// The mode menu and the context card are open.
    pub mode_open: bool,
    pub usage_open: bool,
    /// The queue panel was folded or unfolded from where it starts (open on desktop).
    pub queue_toggled: bool,
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
            agent_open: false,
            mode_open: false,
            usage_open: false,
            queue_toggled: false,
        }
    }
}

/// Where a chat the launcher started runs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Launched {
    pub project_id: String,
    pub cwd: String,
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
    /// The folders the mention menu's Explorer read, by absolute path.
    pub listings: BTreeMap<String, Listing>,
    pub agents: Vec<AgentConfig>,
    /// The launcher's agent (`agents/last-selected`), by config id.
    pub picked: Option<String>,
    /// The `agents/last-selected` read came back, so a launch starts the saved agent.
    pub picked_read: bool,
    /// The model refused the send this step (`ActionRejected`): the draft goes back.
    pub(crate) rejected: bool,
    /// Per session, the agent namespace its history names.
    pub namespaces: BTreeMap<String, String>,
    /// The project and folder of each chat the launcher started, until the chat list names it.
    pub launched: BTreeMap<String, Launched>,
    /// What a composer with no draft yet draws.
    pub empty: Draft,
    /// The agent reads went out on this link.
    pub agents_asked: bool,
    /// Chats the launcher started, by session, with their project: their dock takes the
    /// launcher's card in the morph.
    pub morphed: BTreeMap<String, String>,
    /// The launcher's chat being started.
    pub launching: Option<crate::launch::Launch>,
    /// The composer the phone's Add to chat sheet writes into.
    pub sheet_host: Option<Host>,
}

pub const NO_DRAFT: &str = "";

impl State {
    pub fn draft(&self, host: &Host) -> Option<&Draft> {
        self.composer.drafts.get(host)
    }

    pub fn draft_mut(&mut self, host: &Host) -> &mut Draft {
        self.composer.drafts.entry(host.clone()).or_default()
    }

    /// The folder the composer's skills and mentions read: a chat's own working folder
    /// (`session.cwd`, a worktree's for a worktree chat), the launcher's project folder.
    pub(crate) fn root_of(&self, host: &Host) -> Option<String> {
        if let Host::Chat(id) = host {
            let cwd = self
                .chats
                .get(id)
                .and_then(|c| c.cwd.clone())
                .or_else(|| self.composer.launched.get(id).map(|l| l.cwd.clone()))
                .filter(|cwd| !cwd.is_empty());
            if cwd.is_some() {
                return cwd;
            }
        }
        let project = self.project_of(host)?;
        self.projects
            .store
            .projects
            .iter()
            .find(|p| p.id == project)
            .and_then(|p| p.path.clone())
    }

    pub(crate) fn project_of(&self, host: &Host) -> Option<String> {
        match host {
            Host::Launcher(id) => Some(id.clone()),
            Host::Chat(id) => self
                .chats
                .get(id)
                .and_then(|c| c.project_id.clone())
                .or_else(|| self.composer.launched.get(id).map(|l| l.project_id.clone())),
        }
    }

    /// The working copy under a composer: a chat's label or the launcher's strip, with its
    /// project's branch when the project has read one.
    pub fn foot(&self, host: &Host) -> ui::composer::view::Foot {
        let branch = self.project_of(host).and_then(|id| {
            self.projects
                .store
                .projects
                .iter()
                .find(|p| p.id == id)
                .and_then(|p| p.client.git_branch.clone())
                .filter(|b| !b.is_empty())
        });
        match host {
            Host::Chat(_) => ui::composer::view::Foot::Chat { branch },
            Host::Launcher(_) => ui::composer::view::Foot::Launcher { branch },
        }
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
                let panel = self.draft(host).map(|d| d.panel.clone()).unwrap_or_default();
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
                let listing = match &panel {
                    Panel::Explorer(dir) => self.composer.listings.get(dir),
                    _ => None,
                };
                menu::mention_sections(&panel, &root, &chats, &trigger.query, listing)
            }
        }
    }

    /// The menu a composer shows now, if any.
    pub fn menu_view(&self, host: &Host) -> Option<MenuView> {
        let trigger = self.open_trigger(host)?;
        let draft = self.draft(host)?;
        let sections = self.sections(host, &trigger);
        let root = self.root_of(host).unwrap_or_default();
        let (title, crumb, empty) = match (trigger.kind, &draft.panel) {
            (TriggerKind::Slash, _) => (None, String::new(), "No commands match. Try another name."),
            (TriggerKind::Mention, Panel::Conversations) => {
                (Some("Conversations".to_owned()), String::new(), "No chats match. Try another title.")
            }
            (TriggerKind::Mention, Panel::Explorer(dir)) => {
                (Some("Explorer".to_owned()), menu::relative_dir(&root, dir), "Nothing matches. Try another name.")
            }
            (TriggerKind::Mention, Panel::Root) => (None, String::new(), "Nothing matches. Try another name."),
        };
        let count = menu::flat(&sections).len();
        Some(MenuView {
            sections,
            highlight: draft.highlight.min(count.saturating_sub(1)),
            title,
            crumb,
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
            Key::Left if draft.panel != Panel::Root => self.panel_back(host),
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
            Row::File(entry) | Row::UseFolder(entry) => Pill::File {
                display: entry.display(),
                abs_path: entry.abs_path.clone(),
            },
            Row::Conversations => return self.drill(host, Panel::Conversations, tx),
            Row::Explorer => {
                let Some(root) = self.root_of(host).filter(|r| !r.is_empty()) else { return };
                return self.drill(host, Panel::Explorer(root), tx);
            }
            Row::Dir(entry) => return self.drill(host, Panel::Explorer(entry.folder()), tx),
        };
        log::info!("[composer] pill {:?} into {}", pill_kind(&pill), host.key());
        let draft = self.draft_mut(host);
        field::put_pill(&mut draft.edit, trigger.range, &pill);
        draft.panel = Panel::Root;
        draft.highlight = 0;
        tx.focus(format!("{}-field", host.key()), true);
    }

    /// Opens a mention panel with the query cleared (`push` and `clearQuery`), reading an
    /// Explorer folder the first time it shows.
    fn drill(&mut self, host: &Host, panel: Panel, tx: &mut Tx) {
        if let Panel::Explorer(dir) = &panel {
            self.read_listing(host, dir.clone());
        }
        let draft = self.draft_mut(host);
        if let Some(t) = field::trigger(&draft.edit).filter(|t| !t.query.is_empty()) {
            draft.edit.set_selection(t.range.start + 1, t.range.end);
            draft.edit.insert("");
        }
        draft.panel = panel;
        draft.highlight = 0;
        tx.focus(format!("{}-field", host.key()), false);
    }

    /// Back one mention panel: an Explorer folder to its parent, the root folder to the root.
    fn panel_back(&mut self, host: &Host) {
        let root = self.root_of(host).unwrap_or_default();
        let draft = self.draft_mut(host);
        draft.panel = match &draft.panel {
            Panel::Explorer(dir) if dir != &root && dir.starts_with(&root) => {
                Panel::Explorer(dir.rsplit_once('/').map_or(root.clone(), |(parent, _)| parent.to_owned()))
            }
            _ => Panel::Root,
        };
        draft.highlight = 0;
    }

    /// `filesystemApi.readDirectory` for an Explorer folder; a ready listing is kept.
    fn read_listing(&mut self, host: &Host, dir: String) {
        if dir.is_empty() || matches!(self.composer.listings.get(&dir), Some(Listing::Ready(_))) {
            return;
        }
        let root = self.root_of(host).unwrap_or_default();
        self.composer.listings.insert(dir.clone(), Listing::Loading);
        let path = format!("/fs/ls?path={}", ui::composer::encode_query(&dir));
        if let Some(link) = &mut self.host {
            link.fetch(path, crate::host::Fetch::Listing { dir, root });
        }
    }

    pub(crate) fn listing_read(&mut self, dir: String, root: &str, result: Result<Value, client::http::HttpError>) {
        let listing = match result {
            Ok(value) => match menu::listing_entries(&value, &dir, root) {
                Some(entries) => Listing::Ready(entries),
                None => {
                    log::warn!("[composer] mention folder listing unreadable: dir={dir}");
                    Listing::Error
                }
            },
            Err(e) => {
                log::warn!("[composer] mention folder listing failed: dir={dir} {e}");
                Listing::Error
            }
        };
        self.composer.listings.insert(dir, listing);
    }

    /// An add-menu row: put a trigger at the caret, a tool pill, or (later) the file picker.
    pub(crate) fn composer_add(&mut self, host: &Host, what: &str, tx: &mut Tx) {
        if self.composer_locked(host) {
            let draft = self.draft_mut(host);
            draft.add_open = false;
            draft.tools_open = false;
            return;
        }
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
        let kept = (draft.edit.clone(), std::mem::take(&mut draft.attachments));
        draft.edit.clear();
        draft.panel = Panel::Root;
        self.composer.rejected = false;
        self.acp_step(event);
        if std::mem::take(&mut self.composer.rejected) {
            let draft = self.draft_mut(host);
            (draft.edit, draft.attachments) = kept;
        }
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
        let (agent_host, pick_agent_host, sheet_host, queue_host) = (host.clone(), host.clone(), host.clone(), host.clone());
        let control_host = host.clone();
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
                s.panel_back(host);
                tx.focus(format!("{}-field", host.key()), false);
            }),
            add_menu: Rc::new(move |s: &mut State, open: bool| {
                let open = open && !s.composer_locked(&menu_host);
                let draft = s.draft_mut(&menu_host);
                draft.add_open = open;
                draft.tools_open &= open;
            }),
            add: Rc::new(move |s: &mut State, what: &str, tx: &mut Tx| s.composer_add(&add_host, what, tx)),
            add_sheet: Rc::new(move |s: &mut State, _: &mut Tx| s.open_add_sheet(&sheet_host)),
            agent: Rc::new(move |s: &mut State, open: bool| match &agent_host {
                Host::Chat(id) if open && s.chat_models(id).is_none() => s.toast_later("Switching agents in a chat"),
                _ => s.draft_mut(&agent_host).agent_open = open,
            }),
            control: Rc::new(move |s: &mut State, act| s.composer_control(&control_host, act)),
            queue: Rc::new(move |s: &mut State, act: QueueAct, tx: &mut Tx| s.composer_queue(&queue_host, act, tx)),
            pick_agent: Rc::new(move |s: &mut State, id: &str, tx: &mut Tx| s.composer_pick_agent(&pick_agent_host, id, tx)),
            remove_attachment: Rc::new(move |s: &mut State, n: usize| {
                if s.composer_locked(&rm_host) {
                    return;
                }
                let draft = s.draft_mut(&rm_host);
                if n < draft.attachments.len() {
                    draft.attachments.remove(n);
                }
            }),
        }
    }

    /// The chat's queued prompts as the panel's rows (`previewQueuedPrompt`, `editableText`).
    pub fn queue_rows(&self, host: &Host) -> Vec<QueueRow> {
        let Host::Chat(id) = host else { return Vec::new() };
        let Some(queue) = self.chat.acp.prompt_queues.get(id) else { return Vec::new() };
        queue
            .iter()
            .filter_map(|item| {
                let id = item.get("id")?.as_str()?.to_owned();
                let (text, editable) = queued_text(item);
                let summary = Some(ui::composer::wire::queue_preview(&text))
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| "(queued message)".to_owned());
                Some(QueueRow { id, summary, editable })
            })
            .collect()
    }

    /// A press in the queue panel: Steer, Remove and Clear go to the model; Edit takes a
    /// text-only prompt back under the draft (`editQueued`).
    fn composer_queue(&mut self, host: &Host, act: QueueAct, tx: &mut Tx) {
        let Host::Chat(session) = host else { return };
        let session = session.clone();
        match act {
            QueueAct::Toggle => {
                let draft = self.draft_mut(host);
                draft.queue_toggled = !draft.queue_toggled;
            }
            QueueAct::Clear => self.acp_step(Event::ClearQueuedPrompts(session)),
            QueueAct::Steer(id) => self.acp_step(Event::SendQueuedPromptNow(session, Value::String(id))),
            QueueAct::Remove(id) => self.acp_step(Event::RemoveQueuedPrompt(session, Value::String(id))),
            QueueAct::Edit(id) => {
                let Some((text, true)) = self
                    .chat
                    .acp
                    .prompt_queues
                    .get(&session)
                    .and_then(|q| q.iter().find(|item| item.get("id").and_then(Value::as_str) == Some(&id)))
                    .map(queued_text)
                else {
                    return;
                };
                let draft = self.draft_mut(host);
                let current = field::doc_of(&draft.edit).display_text();
                let next = if ui::composer::js_trim(&current).is_empty() {
                    text
                } else {
                    format!("{}\n{text}", current.trim_end_matches(ui::composer::is_js_space))
                };
                draft.edit = field::edit_of(&ui::composer::parse_display(&next));
                let end = draft.edit.text.len();
                draft.edit.set_caret(end);
                self.acp_step(Event::RemoveQueuedPrompt(session, Value::String(id)));
                tx.focus(format!("{}-field", host.key()), true);
            }
        }
    }

}

/// A queued prompt's typed text (its display blocks, else its blocks), and whether it is
/// text only, so Edit can take it back.
fn queued_text(item: &Value) -> (String, bool) {
    let blocks = item
        .get("displayBlocks")
        .and_then(Value::as_array)
        .or_else(|| item.get("blocks").and_then(Value::as_array));
    let blocks = blocks.map(Vec::as_slice).unwrap_or_default();
    let text = blocks
        .iter()
        .filter(|b| b.get("type").and_then(Value::as_str) == Some("text"))
        .filter_map(|b| b.get("text").and_then(Value::as_str))
        .collect();
    let editable = !blocks.is_empty() && blocks.iter().all(|b| b.get("type").and_then(Value::as_str) == Some("text"));
    (text, editable)
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

#[cfg(test)]
#[path = "composer_review_tests.rs"]
mod review_tests;
