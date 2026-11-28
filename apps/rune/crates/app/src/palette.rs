//! The command palette (`CommandPalette.tsx`): a search band over the Pinned, Recent and
//! category groups. The rows are this client's commands; `model::settings::palette` filters
//! and orders them as cmdk does. Running a row saves it to this device's recent commands and
//! a row's pin toggles it in the shared pinned list. Up and Down move the highlight, Return
//! runs it, Escape or a press outside closes the palette.

use model::settings::palette::{self, Category, ChatCard, Commands, Event, Heading, QuickChat, Row};
use model::shortcuts::format_key_for_display;
use rune::{El, Key, KeyEvent, TextEdit, Tx, col, el, icon, row, text, text_input};
use serde_json::Value;
use theme::Tokens;
use ui::type_::sans;

use crate::State;
use crate::sync::{Pending, Shared};

pub const KEY: &str = "command-palette";

#[derive(Default)]
pub struct PaletteUi {
    pub query: TextEdit,
    /// The highlighted row and the query it was chosen under: a new query highlights its
    /// first row, as cmdk selects the first item after filtering.
    focus: (usize, String),
    pub lists: Commands,
    /// Opened from the phone shell: the canvas and shortcut menu rows hide, New Project shows.
    phone: bool,
    /// The lists were asked for on this link.
    pub(crate) read: bool,
    /// Each list's read answered on this link; until then a write would replace what
    /// another session saved, so runs and pins wait in `early`.
    recent_ready: bool,
    pins_ready: bool,
    early: Early,
}

/// Runs and pins made before their list's read answered, replayed over what it returns.
#[derive(Default)]
struct Early {
    runs: Vec<String>,
    pins: Vec<String>,
}

impl PaletteUi {
    fn lit(&self, len: usize) -> usize {
        let at = if self.focus.1 == self.query.text { self.focus.0 } else { 0 };
        at.min(len.saturating_sub(1))
    }

    fn set_lit(&mut self, i: usize) {
        self.focus = (i, self.query.text.clone());
    }
}

fn command(id: impl Into<String>, category: Category, label: impl Into<String>, description: impl Into<String>, keywords: &[&str]) -> Row {
    Row {
        id: id.into(),
        category,
        label: label.into(),
        description: Some(description.into()),
        keywords: keywords.iter().map(|k| (*k).to_owned()).collect(),
        shortcut: None,
    }
}

impl State {
    /// `getShortcutLabel`: the binding as the Settings page shows it, or None when unbound.
    fn shortcut_label(&self, id: &str) -> Option<String> {
        let shortcut = self.shortcuts.shortcuts.get(id)?;
        let key = shortcut.custom_key.as_deref().unwrap_or(&shortcut.default_key);
        Some(format_key_for_display(key, self.shortcuts.platform)).filter(|l| !l.is_empty())
    }
}

/// The commands `WorkspaceModals` gives the palette on this client, in its order. The browser
/// tab and SSH rows are desktop-app only in React and stay out.
pub(crate) fn rows(s: &State, phone: bool) -> Vec<Row> {
    use Category::{Navigation, Projects, Tools, Workspace};
    let mut out = vec![command("new-terminal", Workspace, "New Terminal", "Open a new shell in the active pane", &["shell", "console", "pty", "workspace"])];
    if let Some(profiles) = s.settings.get("terminalProfiles").as_array() {
        for p in profiles {
            let field = |k: &str| p.get(k).and_then(Value::as_str).unwrap_or_default().to_owned();
            let (id, name, shell) = (field("id"), field("name"), field("shell"));
            let args = p.get("args").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str);
            let line = std::iter::once(if shell.is_empty() { "Default shell" } else { shell.as_str() }).chain(args).collect::<Vec<_>>().join(" ");
            let mut row = command(format!("terminal-profile-{id}"), Workspace, format!("New Terminal: {name}"), line, &["profile", "shell", "terminal"]);
            row.keywords.extend([name, shell]);
            out.push(row);
        }
    }
    if phone {
        out.push(command("new-project", Workspace, "New Project", "Create a new project workspace", &["project", "create", "new", "workspace", "folder"]));
    }
    let mut chat = command("new-chat", Workspace, "New chat", "Start a new chat in the active pane", &["agent", "ai", "claude", "codex", "prompt", "chat", "new"]);
    chat.shortcut = s.shortcut_label("newTerminal");
    out.push(chat);
    out.push(command("new-quick-chat", Workspace, "New quick chat", "Start a chat that has no project", &["agent", "ai", "chat", "quick", "scratch", "no project", "new"]));
    out.push(command("launch-agent", Workspace, "Launch Agent", "Open a CLI agent's TUI in the active pane", &["agent", "ai", "claude", "codex", "gemini", "cursor", "opencode", "cli"]));
    out.push(command("save-snapshot", Workspace, "Save Workspace Snapshot", "Capture the current workspace layout", &["snapshot", "checkpoint", "layout", "save"]));
    if !phone {
        out.push(command("open-canvas", Workspace, "Open Canvas", "Open the project's .figcraft design document as a canvas tab", &["canvas", "figcraft", "design", "drawing", "vector"]));
    }
    let mut browser = command("new-browser-tab", Workspace, "New Browser Tab", "Open a browser tab in the active pane", &["browser", "web", "preview", "url", "tab"]);
    browser.shortcut = s.shortcut_label("newBrowserTab");
    out.push(browser);
    out.push(command("open-project-settings", Navigation, "Project Settings", "Configure the active project workspace", &["settings", "project", "configure", "config"]));
    out.push(command("open-app-preferences", Navigation, "App Preferences", "Open global application preferences", &["preferences", "prefs", "settings", "app", "global"]));
    for (index, p) in s.projects.store.projects.iter().enumerate() {
        let mut keywords = vec!["project".to_owned(), "switch".to_owned(), p.name.clone()];
        keywords.extend(p.path.clone().filter(|path| !path.is_empty()));
        out.push(Row {
            id: format!("project-{}", p.id),
            category: Projects,
            label: p.name.clone(),
            description: Some(p.path.clone().unwrap_or_else(|| "Switch active workspace project".to_owned())),
            keywords: keywords.into_iter().filter(|k| !k.is_empty()).collect(),
            shortcut: (index < 9).then(|| s.shortcut_label(&format!("project-{}", index + 1))).flatten(),
        });
    }
    if s.active_project().is_some() {
        let mut history = command("open-command-history", Tools, "Command History", "Review and reuse recent terminal commands", &["history", "recent", "terminal", "commands", "shell"]);
        history.shortcut = s.shortcut_label("commandHistory");
        out.push(history);
    }
    let mut themes = command("change-color-theme", Tools, "Change Color Theme", "Preview and apply a UI color theme", &["theme", "color", "appearance", "palette", "dark", "dracula", "nord"]);
    themes.shortcut = s.shortcut_label("colorThemePicker");
    out.push(themes);
    if !phone {
        out.push(command("open-shortcut-menu", Tools, "Open Shortcut Menu", "View and edit common keyboard shortcuts", &["keyboard", "shortcuts", "hotkeys", "keys"]));
    }
    out.push(command("report-bug", Tools, "Report a Bug", "File a GitHub issue with diagnostics and screenshots", &["bug", "report", "issue", "github", "feedback", "diagnostics"]));
    out
}

/// The rows in the order the list shows them, with the group each sits in: the chat groups
/// above the commands, as the palette renders them.
fn shown(s: &State) -> Vec<(Heading, Row)> {
    let rows = rows(s, s.palette.phone);
    let mut out = chat_rows(s);
    out.extend(
        palette::groups(&rows, &s.palette.query.text, &s.palette.lists)
            .into_iter()
            .flat_map(|(heading, rows)| rows.into_iter().map(move |r| (heading, r.clone()))),
    );
    out
}

/// `UNTITLED_HOST_TITLE`: what a chat the host has not named is called.
const UNTITLED: &str = "Untitled Chat";
/// A chat row's id prefix: `chat:` opens a project chat, `quick-chat:` a quick chat.
const CHAT: &str = "chat:";
const QUICK_CHAT: &str = "quick-chat:";

/// The project whose chats the Chats group searches (`chatsProject`): the active one, never on
/// the phone shell.
fn chats_project(s: &State) -> Option<&model::projects::Project> {
    (!s.palette.phone).then(|| s.active_project()).flatten()
}

/// The Chats and Quick chats rows for the query (`CommandPaletteChats`,
/// `CommandPaletteQuickChats`); the description is the meta line under the title.
fn chat_rows(s: &State) -> Vec<(Heading, Row)> {
    let query = &s.palette.query.text;
    let cards: Vec<ChatCard> = chats_project(s)
        .map(|p| {
            let git = p.client.is_git_repo != Some(false);
            s.chats
                .of_project(&p.id)
                .filter(|c| !c.discovered)
                .map(|c| ChatCard {
                    id: c.session_id.clone(),
                    title: if c.title.is_empty() { UNTITLED.to_owned() } else { c.title.clone() },
                    agent_name: None,
                    index_model: c.model_name.clone().filter(|m| !m.is_empty()).or_else(|| c.model_id.clone()),
                    branch: match (git, &c.worktree_path) {
                        (false, _) => String::new(),
                        (true, Some(_)) => c.worktree_branch.clone().unwrap_or_default(),
                        (true, None) => p.client.git_branch.clone().unwrap_or_default(),
                    },
                })
                .collect()
        })
        .unwrap_or_default();
    let quick: Vec<QuickChat> = s
        .chats
        .list
        .iter()
        .filter(|c| c.kind.as_deref() == Some("quick"))
        .map(|c| QuickChat {
            id: c.session_id.clone(),
            title: if c.title.is_empty() { UNTITLED.to_owned() } else { c.title.clone() },
            created_at: c.created_at,
        })
        .collect();
    let (chats, quick) = palette::chat_groups(&cards, &quick, query);
    let row = |id: String, label: &str, meta: &str| Row {
        id,
        category: Category::Workspace,
        label: label.to_owned(),
        description: (!meta.is_empty()).then(|| meta.to_owned()),
        keywords: Vec::new(),
        shortcut: None,
    };
    let mut out: Vec<(Heading, Row)> = chats.into_iter().map(|c| (Heading::Chats, row(format!("{CHAT}{}", c.id), &c.title, &c.branch))).collect();
    out.extend(quick.into_iter().map(|q| (Heading::QuickChats, row(format!("{QUICK_CHAT}{}", q.id), &q.title, "Quick chat"))));
    out
}

fn glyph(id: &str) -> &'static rune::Icon {
    let name = match id {
        "new-project" => "FolderPlus",
        "new-chat" | "new-quick-chat" | "launch-agent" => "Bot",
        "save-snapshot" => "Save",
        "open-canvas" => "Canvas",
        "new-browser-tab" => "Globe",
        "open-project-settings" => "Settings",
        "open-app-preferences" => "SlidersHorizontal",
        "open-command-history" => "History",
        "change-color-theme" => "Palette",
        "open-shortcut-menu" => "Keyboard",
        "report-bug" => "Bug",
        _ => "Terminal",
    };
    theme::icons::named(name).expect("bundled icon")
}

fn item(s: &State, t: &'static Tokens, heading: Heading, r: &Row, index: usize, lit: bool) -> El {
    let c = &t.colors;
    let pinned = s.palette.lists.pinned_command_ids.contains(&r.id);
    let project = r.id.strip_prefix("project-").and_then(|id| s.projects.store.projects.iter().find(|p| p.id == id));
    let badge = match project {
        Some(p) => ui::shell::project_badge(t, &p.name, &p.color),
        None => icon(glyph(&r.id), 16.0).color(if lit { c.foreground } else { c.muted_foreground }),
    };
    if matches!(heading, Heading::Chats | Heading::QuickChats) {
        return chat_item(t, r, index, lit);
    }
    let mut words = col().min_w(0.0).shrink().child(text(r.label.clone()).font(sans(12.0, 500.0, 20.0)).color(c.foreground).truncate());
    if let Some(d) = &r.description {
        words = words.child(text(d.clone()).font(sans(12.0, 400.0, 16.0)).color(c.muted_foreground).truncate());
    }
    let left = row()
        .min_w(0.0)
        .shrink()
        .gap(10.0)
        .items_center()
        .child(row().size(28.0, 28.0).center().radius(6.0).border(1.0, c.border).fill(c.foreground.alpha(0.03)).a11y_hidden().child(badge))
        .child(words);
    let mut right = row().gap(6.0).items_center();
    if let Some(sc) = &r.shortcut {
        right = right.child(row().a11y_hidden().child(ui::kbd(t, sc.clone())));
    }
    let pin_id = r.id.clone();
    let label = if pinned { format!("Unpin {}", r.label) } else { format!("Pin {}", r.label) };
    let pin_icon = theme::icons::named("Pin").expect("bundled icon");
    // Touch has no hover: the pin shows on every row of the phone palette.
    let show_pin = pinned || lit || s.palette.phone;
    right = right.child(
        row()
            .key("pin")
            .role("button")
            .label(label)
            .size(24.0, 24.0)
            .center()
            .radius(6.0)
            .hover(c.foreground.alpha(0.06))
            .opacity(if show_pin { 1.0 } else { 0.0 })
            .child(icon(pin_icon, 13.0).color(if pinned || lit { c.foreground } else { c.muted_foreground }))
            .on_press(move |s: &mut State| s.toggle_pinned(&pin_id)),
    );
    let run_id = r.id.clone();
    let prefix = match heading {
        Heading::Pinned => "pinned:",
        Heading::Recent => "recent:",
        Heading::Chats | Heading::QuickChats | Heading::Category(_) => "",
    };
    let mut e = row()
        .key(format!("cmd-{prefix}{}", r.id))
        .role("option")
        .focus_selected(lit)
        .w_full()
        .px(10.0)
        .py(8.0)
        .gap(12.0)
        .items_center()
        .justify_between()
        .radius(8.0)
        .child(left)
        .child(right)
        .on_hover_change(move |s: &mut State, on: bool, _tx: &mut Tx| {
            if on {
                s.palette.set_lit(index);
            }
        })
        .on_press(move |s: &mut State| s.run_command(&run_id));
    if lit {
        e = e.fill(c.foreground.alpha(0.06));
    }
    e
}

/// A chat row: the agent glyph in its box, the title, and the branch or "Quick chat" under it.
/// Picking it opens the chat; it is not a command, so it has no pin and no recent entry.
fn chat_item(t: &'static Tokens, r: &Row, index: usize, lit: bool) -> El {
    let c = &t.colors;
    let bot = theme::icons::named("Bot").expect("bundled icon");
    let mut words = col().min_w(0.0).shrink().child(text(r.label.clone()).font(sans(12.0, 500.0, 20.0)).color(c.foreground).truncate());
    if let Some(d) = &r.description {
        words = words.child(text(d.clone()).font(sans(12.0, 400.0, 16.0)).color(c.muted_foreground).truncate());
    }
    let run_id = r.id.clone();
    let mut e = row()
        .key(format!("cmd-{}", r.id))
        .role("option")
        .label(r.label.clone())
        .focus_selected(lit)
        .w_full()
        .px(10.0)
        .py(8.0)
        .gap(10.0)
        .items_center()
        .radius(8.0)
        .child(row().size(28.0, 28.0).center().radius(6.0).border(1.0, c.border).fill(c.foreground.alpha(0.03)).a11y_hidden().child(icon(bot, 14.0).color(c.muted_foreground)))
        .child(words)
        .on_hover_change(move |s: &mut State, on: bool, _tx: &mut Tx| {
            if on {
                s.palette.set_lit(index);
            }
        })
        .on_press(move |s: &mut State| s.run_command(&run_id));
    if lit {
        e = e.fill(c.foreground.alpha(0.06));
    }
    e
}

fn hint(t: &Tokens, keys: &str, label: &str) -> El {
    row()
        .gap(6.0)
        .items_center()
        .child(ui::kbd(t, keys))
        .child(text(label.to_owned()).font(sans(12.0, 400.0, 16.0)).color(t.colors.muted_foreground))
}

pub fn build(s: &State, t: &'static Tokens, phone: bool, window: (f32, f32), layer: &mut Vec<El>) {
    if !s.overlays.is_open(KEY) {
        return;
    }
    let c = &t.colors;
    let shown = shown(s);
    let lit = s.palette.lit(shown.len());
    let search = theme::icons::named("Search").expect("bundled icon");
    let placeholder = if chats_project(s).is_some() {
        "Search commands, projects, chats, settings…"
    } else {
        "Search commands, projects, settings…"
    };
    let field = text_input(&s.palette.query, sans(14.0, 400.0, 20.0))
        .single_line()
        .key("palette-input")
        .role("combobox")
        .label("Search commands")
        .placeholder(placeholder.to_owned(), c.muted_foreground)
        .color(c.foreground)
        .grow(1.0)
        .py(12.0)
        .no_focus_ring()
        .autofocus()
        .on_submit(|s: &mut State, _tx: &mut Tx| s.palette_key(None))
        .on_input_key(|s: &mut State, key: &KeyEvent, _tx: &mut Tx| {
            let step = match key.key {
                Key::Up => -1,
                Key::Down => 1,
                _ => return false,
            };
            s.palette_key(Some(step));
            true
        })
        .on_edit(|s: &mut State| &mut s.palette.query);
    let mut band = row()
        .key("palette-band")
        .w_full()
        .h(49.0)
        .px(12.0)
        .items_center()
        .gap(8.0)
        .border_sides([0.0, 0.0, 1.0, 0.0], c.border)
        .child(icon(search, 16.0).color(c.foreground).opacity(0.5).a11y_hidden())
        .child(field);
    if phone {
        let x = theme::icons::named("X").expect("bundled icon");
        band = band.child(
            row()
                .key("palette-close")
                .role("button")
                .label("Close command palette")
                .size(44.0, 44.0)
                .center()
                .radius(8.0)
                .child(icon(x, 20.0).color(c.muted_foreground))
                .on_press(|s: &mut State| s.close(KEY)),
        );
    }
    let mut list = col().key("palette-list").role("listbox").label("Commands").w_full().px(4.0).py(4.0).items_stretch();
    if shown.is_empty() {
        list = list.child(row().key("empty").w_full().py(24.0).center().child(text("No commands found.").font(sans(14.0, 400.0, 20.0)).color(c.foreground)));
    }
    let mut heading = None;
    let mut group = col();
    for (i, (h, r)) in shown.iter().enumerate() {
        if heading != Some(*h) {
            if heading.is_some() {
                list = list.child(group);
            }
            heading = Some(*h);
            group = col()
                .key(format!("group-{}", h.label()))
                .role("group")
                .label(h.label())
                .items_stretch()
                .child(row().px(10.0).pt(8.0).pb(4.0).child(text(h.label()).font(sans(12.0, 500.0, 16.0)).color(c.muted_foreground)));
        }
        group = group.child(item(s, t, *h, r, i, i == lit));
    }
    if heading.is_some() {
        list = list.child(group);
    }
    let mut card = col()
        .key("palette-panel")
        .role("dialog")
        .label("Command palette")
        .w_full()
        .radius(12.0)
        .clip_rounded(12.0)
        .fill(c.popover)
        .border(1.0, c.border)
        .child(band)
        .child(col().key("palette-scroll").w_full().max_h(window.1 * 0.52).child(list).scroll(rune::ScrollSpec { stick_bottom: false }));
    if !phone {
        card = card.max_w(576.0).child(
            row()
                .key("palette-hints")
                .w_full()
                .h(36.0)
                .px(14.0)
                .gap(14.0)
                .items_center()
                .justify_end()
                .border_sides([1.0, 0.0, 0.0, 0.0], c.border)
                .child(hint(t, "↑↓", "Move"))
                .child(hint(t, "↵", "Run"))
                .child(hint(t, "Esc", "Close")),
        );
    }
    let card = ui::paint::shadows(card, &t.shadows.elevation_2xl).modal(true, |s: &mut State| s.close(KEY));
    layer.push(el().key("palette-scrim").absolute(0.0, 0.0).w_full().h_full().fill(c.overlay.alpha(0.4)).backdrop_blur(4.0).a11y_hidden());
    let placed = col().key("palette-layer").absolute(0.0, 0.0).w_full().h_full().items_center();
    layer.push(if phone { placed.justify_end().px(12.0).pb(32.0).child(card) } else { placed.pt(window.1 * 0.07).child(card) });
}

impl State {
    pub fn open_palette(&mut self, phone: bool) {
        self.palette.query = TextEdit::new("");
        self.palette.focus = (0, String::new());
        self.palette.phone = phone;
        self.read_palette_lists();
        self.overlays.open(KEY);
    }

    /// Up or Down moves the highlight; Return runs the highlighted row.
    pub(crate) fn palette_key(&mut self, step: Option<isize>) {
        let shown = shown(self);
        if shown.is_empty() {
            return;
        }
        let at = self.palette.lit(shown.len());
        match step {
            Some(d) => self.palette.set_lit((at as isize + d).clamp(0, shown.len() as isize - 1) as usize),
            None => {
                let id = shown[at].1.id.clone();
                self.run_command(&id);
            }
        }
    }

    /// `executeCommand`: remember it, close the palette, then run it.
    pub(crate) fn run_command(&mut self, id: &str) {
        if let Some(chat) = id.strip_prefix(CHAT).or_else(|| id.strip_prefix(QUICK_CHAT)) {
            log::info!("[palette] open chat {chat}");
            self.close(KEY);
            self.open_chat(chat);
            return;
        }
        log::info!("[palette] run {id}");
        self.palette.lists.step(Event::AddRecentCommand(id.to_owned()));
        if self.palette.recent_ready {
            self.write_recent();
        } else {
            self.palette.early.runs.push(id.to_owned());
        }
        self.close(KEY);
        let later = |s: &mut State, what: &str| s.toast_later(what);
        match id {
            "new-terminal" => self.new_terminal(None),
            "new-project" => self.overlays.open(crate::rail::NEW_PROJECT),
            "open-app-preferences" => self.open_settings(None),
            "open-project-settings" => self.open_project_settings(None, None),
            "open-command-history" => self.open_command_history(),
            "change-color-theme" => self.open_theme_picker(),
            "open-shortcut-menu" => self.overlays.open(crate::keys::MENU),
            "save-snapshot" => self.open_create_snapshot(),
            "open-canvas" => self.open_project_canvas(),
            "new-browser-tab" => self.open_browser_tab(None, None),
            "report-bug" => self.open_bug_report(),
            _ => {
                if let Some(project) = id.strip_prefix("project-") {
                    self.select_project(project);
                } else {
                    let label = rows(self, self.palette.phone).into_iter().find(|r| r.id == id).map(|r| r.label);
                    later(self, &label.unwrap_or_else(|| id.to_owned()));
                }
            }
        }
    }

    /// The pin on a row: toggles it in the shared pinned list.
    pub(crate) fn toggle_pinned(&mut self, id: &str) {
        log::info!("[palette] pin {id}");
        self.palette.lists.step(Event::TogglePinned(id.to_owned()));
        if self.palette.pins_ready {
            self.write_pins();
        } else {
            self.palette.early.pins.push(id.to_owned());
        }
    }

    fn write_recent(&mut self) {
        let ids = Value::from(self.palette.lists.recent_command_ids.clone());
        let key = self.device_key(palette::RECENT_KEY);
        self.write(palette::RECENT_KEY, ids, Pending::Write(key));
    }

    fn write_pins(&mut self) {
        let ids = Value::from(self.palette.lists.pinned_command_ids.clone());
        self.write_shared(Shared::PinnedCommands, ids);
    }

    /// A new link: the lists are read again the next time the palette opens.
    pub(crate) fn palette_link_reset(&mut self) {
        self.palette.read = false;
        self.palette.recent_ready = false;
        self.palette.pins_ready = false;
    }

    /// `settings/recent-commands` was read (None when it is empty): the saved list, with the
    /// runs made before the read added on top and written back.
    pub(crate) fn recent_commands_loaded(&mut self, value: Option<Value>) {
        let saved = value.and_then(|v| serde_json::from_value::<Vec<String>>(v).ok()).unwrap_or_default();
        self.palette.lists.step(Event::SetRecentCommands(saved));
        let early = std::mem::take(&mut self.palette.early.runs);
        for id in &early {
            self.palette.lists.step(Event::AddRecentCommand(id.clone()));
        }
        self.palette.recent_ready = true;
        if !early.is_empty() {
            self.write_recent();
        }
    }

    /// The recent list could not be read: runs stay on this screen and nothing is written,
    /// so the saved list survives.
    pub(crate) fn recent_commands_unread(&mut self, code: &str) {
        log::warn!("[palette] reading the recent commands failed: {code}; runs are not saved on this link");
        self.palette.early.runs.clear();
    }

    /// `settings/pinned-commands` was read (None when it is empty): the saved list, with the
    /// pins toggled before the read replayed on top and written back.
    pub(crate) fn pinned_commands_loaded(&mut self, value: Option<Value>) {
        let saved = value.and_then(|v| serde_json::from_value::<Vec<String>>(v).ok()).unwrap_or_default();
        self.palette.lists.step(Event::SetPinned(saved));
        let early = std::mem::take(&mut self.palette.early.pins);
        for id in &early {
            self.palette.lists.step(Event::TogglePinned(id.clone()));
        }
        self.palette.pins_ready = true;
        if !early.is_empty() {
            self.write_pins();
        }
    }

    /// The pinned list could not be read: it is never written on this link.
    pub(crate) fn pinned_commands_unread(&mut self, code: &str) {
        log::warn!("[palette] reading the pinned commands failed: {code}; pins are not saved on this link");
        self.palette.early.pins.clear();
    }

    /// Another device changed the pinned list, or the host merged our write with its change
    /// (`useSharedKeySync`): anything but a list of ids is ignored. A change seen while this
    /// client's own write is out waits for that write's reply, which carries the merge.
    pub(crate) fn pinned_commands_changed(&mut self, value: Value) {
        if !self.palette.pins_ready {
            return;
        }
        if let Ok(ids) = serde_json::from_value::<Vec<String>>(value) {
            self.palette.lists.step(Event::SetPinned(ids));
        }
    }
}

#[cfg(test)]
#[path = "palette_tests.rs"]
mod tests;
