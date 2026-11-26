//! Settings → MCP servers (`mcp-servers-section.tsx`, `McpServersSettings.tsx`): the servers
//! you added as cards, two per row (one on a phone), and one server's detail page with its
//! connection, its tools and Remove. The registry and its probes are the acp model's; the
//! host side is `mcp_host`, and the Add and Edit dialog is `settings_mcp_dialog`.

use std::collections::VecDeque;

use model::acp::Event;
use model::settings::mcp::{self as rules, Object, Tone};
use rune::{El, TextEdit, col, row, text};
use serde_json::{Value, json};
use theme::Tokens;
use ui::settings::{StatusTone, card, empty_state, filter, group, group_with_meta, letter_tile, status_word, toned_notice};
use ui::type_::{mono, sans};
use ui::{Button, ButtonSize, ButtonVariant, Switch};

use crate::State;
use crate::mcp_host::{OnReject, Poll};
use crate::settings_profiles::Undo;

/// The page id.
pub const PAGE: &str = "mcp-servers";
/// Tools a detail page lists before Show all (`FIRST_TOOLS`).
const FIRST_TOOLS: usize = 5;

/// A registry write the page asked for, and what it does once the write lands.
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    /// Remove: the toast offers Undo with the server as it was.
    Delete(Object),
    /// Undo of a Remove.
    Restore,
    Toggle,
    /// The dialog's Add, with how many servers it adds.
    Add(usize),
    Edit,
    /// A catalog install's commit, saved into the registry.
    Install,
}

impl Action {
    /// The toast a failed write shows.
    pub(crate) fn failure(&self) -> &'static str {
        match self {
            Action::Delete(_) => "Could not delete the MCP server. Your previous settings were restored.",
            Action::Restore => "Could not restore the MCP server. Add it again from JSON.",
            Action::Toggle => "Could not update the MCP server. Your previous setting was restored.",
            Action::Add(_) => "Could not save the MCP servers. Your previous settings were restored.",
            Action::Edit => "Could not save the MCP server. Your previous settings were restored.",
            Action::Install => "Troupe couldn't save the server after you approved it. Start over to try again.",
        }
    }
}

#[derive(Default)]
pub struct McpUi {
    /// The server whose detail page is open.
    pub detail: Option<String>,
    pub tools_filter: TextEdit,
    pub show_all_tools: bool,
    /// The ids the open page last probed, joined: a new shape probes again.
    pub probed: Option<String>,
    /// The revision of the shared registry this client last read or wrote.
    pub revision: Option<u64>,
    /// A registry write is on the wire; a watched change waits for its reply.
    pub saving: bool,
    /// Page writes in the order the model saves them.
    pub actions: VecDeque<Action>,
    pub rejection: Option<OnReject>,
    pub polls: Vec<Poll>,
    pub dialog: Option<crate::settings_mcp_dialog::ServerDialog>,
    pub paste: crate::settings_mcp_dialog::PasteUi,
    /// The catalog under the added servers.
    pub catalog: crate::mcp_catalog::CatalogUi,
    pub install: crate::mcp_install::InstallUi,
    /// Every POST this client sent, for the tests (a test state has no host).
    #[cfg(test)]
    pub posted: Vec<(String, Value)>,
}

fn glyph(name: &str) -> &'static rune::Icon {
    theme::icons::named(name).expect("bundled icon")
}

fn tone(t: Tone) -> StatusTone {
    match t {
        Tone::Success => StatusTone::Success,
        Tone::Warning => StatusTone::Warning,
        Tone::Danger => StatusTone::Danger,
        Tone::Muted => StatusTone::Muted,
    }
}

fn id_of(server: &Object) -> &str {
    server.get("id").and_then(Value::as_str).unwrap_or_default()
}

fn name_of(server: &Object) -> &str {
    server.get("name").and_then(Value::as_str).unwrap_or_default()
}

fn enabled(server: &Object) -> bool {
    server.get("enabled") != Some(&json!(false))
}

impl State {
    pub(crate) fn mcp_servers(&self) -> &[Object] {
        &self.chat.acp.mcp_servers
    }

    fn mcp_server(&self, id: &str) -> Option<&Object> {
        self.mcp_servers().iter().find(|s| id_of(s) == id)
    }

    fn probe_status(&self, id: &str) -> Option<&str> {
        self.chat.acp.mcp_probe_status.get(id).and_then(Value::as_str)
    }

    fn server_status(&self, server: &Object) -> rules::Status {
        let id = id_of(server);
        rules::status(enabled(server), self.mcp_flag(&self.chat.acp.mcp_probing, id), self.probe_status(id))
    }

    pub(crate) fn mcp_project(&self) -> String {
        self.active_project().map(|p| p.id.clone()).unwrap_or_default()
    }

    fn mcp_page_open(&self) -> bool {
        self.overlays.is_open(crate::settings_view::APP) && self.settings_page().id == PAGE
    }

    /// The page opened: read the registry once, then probe what it lists.
    pub(crate) fn mcp_page_opened(&mut self) {
        self.mcp_ui.probed = None;
        if !self.chat.acp.mcp_servers_loaded {
            log::info!("[settings.McpServersSettings] reading the MCP registry");
            self.acp_step(Event::LoadMcpServers(Vec::new()));
        }
        if self.mcp_ui.catalog.active {
            self.catalog_refresh();
        } else {
            self.catalog_opened();
        }
        self.mcp_settle();
    }

    /// After every registry change: probe a new id set once (each enabled server, and the sign-in
    /// of each remote one), and close a detail page whose server is gone.
    pub(crate) fn mcp_settle(&mut self) {
        if let Some(id) = &self.mcp_ui.detail
            && self.mcp_server(id).is_none()
        {
            self.mcp_ui.detail = None;
        }
        if !self.mcp_page_open() || !self.chat.acp.mcp_servers_loaded {
            return;
        }
        let shape = self.mcp_servers().iter().map(id_of).collect::<Vec<_>>().join("|");
        if self.mcp_ui.probed.as_deref() == Some(shape.as_str()) {
            return;
        }
        self.mcp_ui.probed = Some(shape);
        let project = self.mcp_project();
        let servers: Vec<(String, bool, bool)> = self.mcp_servers().iter().map(|s| (id_of(s).to_owned(), enabled(s), rules::is_remote(s))).collect();
        for (id, on, remote) in servers {
            if on {
                self.acp_step(Event::ProbeMcpServer(id.clone()));
            }
            if remote {
                self.acp_step(Event::CheckMcpOAuthStatus(project.clone(), id));
            }
        }
    }

    /// A page write: the model saves it, and `mcp_host` answers with what the page does next.
    pub(crate) fn mcp_write(&mut self, action: Action, event: Event) {
        self.mcp_ui.actions.push_back(action);
        self.acp_step(event);
    }

    /// The page's write landed.
    pub(crate) fn mcp_action_landed(&mut self, action: Action) {
        self.mcp_dialog_settled();
        match action {
            Action::Delete(server) => {
                log::info!("[settings.McpServersSettings] MCP server deleted; undo offered");
                self.undo_toast(&format!("Deleted {}", name_of(&server)), Undo::McpServer(server));
            }
            Action::Restore => log::info!("[settings.McpServersSettings] MCP server restored by undo"),
            Action::Toggle => {}
            Action::Add(count) => {
                let plural = if count == 1 { "" } else { "s" };
                self.toasts.push(self.now, ui::overlay::ToastKind::Success, format!("Added {count} MCP server{plural}"), None);
                self.close_mcp_dialog();
            }
            Action::Edit => {
                self.toasts.push(self.now, ui::overlay::ToastKind::Success, "MCP server updated", None);
                self.close_mcp_dialog();
            }
            Action::Install => self.install_saved(true),
        }
        self.mcp_settle();
    }

    pub(crate) fn restore_mcp_server(&mut self, server: Object) {
        self.mcp_write(Action::Restore, Event::SaveMcpServer(server));
    }

    fn toggle_mcp_server(&mut self, id: &str, next: bool) {
        log::info!("[settings.McpServersSettings] {} {id}", if next { "enable" } else { "disable" });
        self.mcp_write(Action::Toggle, Event::SetMcpServerEnabled(id.to_owned(), json!(next)));
    }

    /// Remove: exactly the server the page shows, at once, with Undo once the write lands.
    pub(crate) fn remove_mcp_server(&mut self, id: &str) {
        let Some(server) = self.mcp_server(id).cloned() else { return };
        self.mcp_ui.detail = None;
        self.mcp_write(Action::Delete(server), Event::DeleteMcpServer(id.to_owned()));
    }

    fn open_mcp_detail(&mut self, id: &str) {
        log::info!("[settings.McpServersSettings] detail {id}");
        self.mcp_ui.detail = Some(id.to_owned());
        self.mcp_ui.tools_filter = TextEdit::new("");
        self.mcp_ui.show_all_tools = false;
        if self.mcp_server(id).is_some_and(enabled) {
            self.acp_step(Event::LoadMcpTools(id.to_owned()));
        }
    }

    fn connect_mcp(&mut self, id: &str) {
        let project = self.mcp_project();
        self.acp_step(Event::ConnectMcpOAuth(project, id.to_owned()));
    }

    fn disconnect_mcp(&mut self, id: &str) {
        let project = self.mcp_project();
        self.acp_step(Event::DisconnectMcpOAuth(project, id.to_owned()));
    }
}

/// The page body: the detail page, else the cards.
pub fn page(s: &State, t: &'static Tokens, phone: bool) -> Vec<El> {
    if let Some(server) = s.mcp_ui.detail.as_deref().and_then(|id| s.mcp_server(id)) {
        return detail(s, t, server, phone);
    }
    let servers = s.mcp_servers();
    let add = || {
        Button::new("Add server")
            .variant(ButtonVariant::Secondary)
            .size(ButtonSize::Xs)
            .icon(glyph("Plus"))
            .disabled(!s.chat.acp.mcp_servers_loaded)
            .build(t)
            .key("add-server")
            .on_press(|s: &mut State| s.open_mcp_add())
    };
    let later = crate::mcp_catalog::browser(s, t, phone);
    if servers.is_empty() {
        let empty = empty_state(
            t,
            "mcp-empty",
            "Server",
            "No MCP servers yet",
            "Add a stdio, HTTP, or SSE server, or install one from the catalog below. New agent chats can then use its tools.",
            Some(add()),
        );
        return vec![group(t, "mcp-added", "Added", vec![], vec![empty]), later];
    }
    let on = servers.iter().filter(|s| enabled(s)).count();
    let plural = if servers.len() == 1 { "" } else { "s" };
    let meta = format!("{} server{plural} · {on} on", servers.len());
    let per_row = if phone { 1 } else { 2 };
    let cards: Vec<El> = servers.iter().map(|server| server_card(s, t, server)).collect();
    let mut grid = col().key("mcp-grid").w_full().gap(12.0).items_stretch();
    let mut cards = cards.into_iter();
    let mut line = 0;
    loop {
        let chunk: Vec<El> = cards.by_ref().take(per_row).collect();
        if chunk.is_empty() {
            break;
        }
        let mut r = row().key(format!("line-{line}")).w_full().gap(12.0).items_stretch();
        let filled = chunk.len();
        for c in chunk {
            r = r.child(c.grow(1.0).min_w(0.0));
        }
        for gap in filled..per_row {
            r = r.child(rune::el().key(format!("gap-{gap}")).grow(1.0));
        }
        grid = grid.child(r);
        line += 1;
    }
    vec![group_with_meta(t, "mcp-added", "Added", &meta, vec![add()], vec![grid]), later]
}

fn switch(t: &Tokens, server: &Object) -> El {
    let (id, on) = (id_of(server).to_owned(), enabled(server));
    let label = format!("{} {}", if on { "Disable" } else { "Enable" }, name_of(server));
    Switch::new(on).label(label).build(t).key("switch").on_press(move |s: &mut State| s.toggle_mcp_server(&id, !on))
}

/// `McpServerCard`.
fn server_card(s: &State, t: &'static Tokens, server: &Object) -> El {
    let c = &t.colors;
    let id = id_of(server).to_owned();
    let on = enabled(server);
    let status = s.server_status(server);
    let probe = s.probe_status(&id);
    let probing = s.mcp_flag(&s.chat.acp.mcp_probing, &id);
    let open = id.clone();
    let name = text(name_of(server).to_owned())
        .font(sans(14.0, 500.0, 20.0))
        .color(if on { c.foreground } else { c.muted_foreground })
        .single_line()
        .truncate()
        .min_w(0.0)
        .grow(1.0)
        .key("name")
        .role("button")
        .focusable(true)
        .on_press(move |s: &mut State| s.open_mcp_detail(&open));
    let head = row().key("head").w_full().gap(10.0).items_center().child(letter_tile(t, name_of(server), 32.0)).child(name).child(switch(t, server));
    let target = text(rules::target(server).to_owned()).font(mono(12.0, 400.0, 16.0)).color(c.secondary_foreground).wrap().min_h(32.0).max_lines(2).key("target");
    let mut foot = row().key("foot").w_full().h(24.0).gap(8.0).items_center().child(status_word(t, "status", tone(status.tone), status.label, status.pending));
    if status.tone == Tone::Success
        && let Some(tools) = s.chat.acp.mcp_tools.get(&id).and_then(Value::as_array)
    {
        foot = foot.child(text(format!("· {}", rules::tool_count(tools.len()))).font(sans(12.0, 400.0, 16.0)).color(c.muted_foreground).single_line());
    }
    foot = foot.child(rune::el().key("spacer").grow(1.0));
    if on && probe == Some("authRequired") {
        let connecting = s.mcp_flag(&s.chat.acp.mcp_oauth_connecting, &id);
        let target = id.clone();
        foot = foot.child(
            Button::new(if connecting { "Connecting…" } else { "Connect" })
                .variant(ButtonVariant::Secondary)
                .size(ButtonSize::Xs)
                .disabled(connecting)
                .build(t)
                .key("connect")
                .h(24.0)
                .on_press(move |s: &mut State| s.connect_mcp(&target)),
        );
    }
    if on && probe == Some("disconnected") && !probing {
        let target = id.clone();
        foot = foot.child(
            Button::new("Try again")
                .variant(ButtonVariant::Secondary)
                .size(ButtonSize::Xs)
                .icon(glyph("RefreshCw"))
                .build(t)
                .key("retry")
                .h(24.0)
                .on_press(move |s: &mut State| s.acp_step(Event::ProbeMcpServer(target.clone()))),
        );
    }
    let open = id.clone();
    col()
        .key(format!("mcp-server-card-{id}"))
        .role("group")
        .label(name_of(server).to_owned())
        .gap(10.0)
        .padding(14.0)
        .radius(8.0)
        .fill(c.card)
        .border(1.0, c.border)
        .hover(c.secondary)
        .items_stretch()
        .child(head)
        .child(target)
        .child(foot)
        .on_press(move |s: &mut State| s.open_mcp_detail(&open))
}

fn detail_row(t: &Tokens, key: &str, label: &str, value: El, action: Option<El>) -> El {
    let c = &t.colors;
    let mut r = row()
        .key(key.to_owned())
        .w_full()
        .min_h(40.0)
        .px(16.0)
        .py(10.0)
        .gap(12.0)
        .items_center()
        .child(text(label).font(sans(12.0, 400.0, 16.0)).color(c.muted_foreground).w(96.0))
        .child(value.min_w(0.0).grow(1.0));
    if let Some(action) = action {
        r = r.child(action);
    }
    r
}

/// `McpServerDetail`.
fn detail(s: &State, t: &'static Tokens, server: &Object, phone: bool) -> Vec<El> {
    let c = &t.colors;
    let id = id_of(server).to_owned();
    let on = enabled(server);
    let status = s.server_status(server);
    let probe = s.probe_status(&id);
    let probing = s.mcp_flag(&s.chat.acp.mcp_probing, &id);
    let connected = s.mcp_flag(&s.chat.acp.mcp_oauth_connected, &id);
    let connecting = s.mcp_flag(&s.chat.acp.mcp_oauth_connecting, &id);
    let remote = rules::is_remote(server);
    let back = row()
        .key("mcp-back")
        .role("button")
        .label("MCP servers")
        .focusable(true)
        .gap(6.0)
        .items_center()
        .child(rune::icon(glyph("ArrowLeft"), 14.0).color(c.secondary_foreground).a11y_hidden())
        .child(text("MCP servers").font(sans(12.0, 500.0, 16.0)).color(c.secondary_foreground))
        .on_press(|s: &mut State| s.mcp_ui.detail = None);
    let word = if on && status.tone == Tone::Success { format!("{} · offered to new chats", status.label) } else { status.label.to_owned() };
    let title = col()
        .key("title")
        .min_w(0.0)
        .grow(1.0)
        .child(text(name_of(server).to_owned()).font(sans(20.0, 500.0, 28.0)).color(c.foreground).single_line().truncate().role("heading"))
        .child(status_word(t, "status", tone(status.tone), &word, status.pending));
    let mut top = col().key("mcp-detail-top").w_full().gap(16.0).items_stretch().child(back).child(
        row().key("identity").w_full().gap(12.0).items_center().child(letter_tile(t, name_of(server), 40.0)).child(title).child(switch(t, server)),
    );
    if let Some(name) = server.get("catalog").and_then(|c| c.get("name")).and_then(Value::as_str) {
        top = top.child(text(format!("From the catalog · {name}")).font(sans(12.0, 400.0, 16.0)).color(c.muted_foreground));
    }
    let small = |label: &str, icon: Option<&str>| {
        let mut b = Button::new(label.to_owned()).variant(ButtonVariant::Secondary).size(ButtonSize::Xs);
        if let Some(icon) = icon {
            b = b.icon(glyph(icon));
        }
        b
    };
    let mut actions = Vec::new();
    if probe != Some("disconnected") {
        let target = id.clone();
        actions.push(small("Test connection", Some("RefreshCw")).disabled(probing || !on).build(t).key("test").on_press(move |s: &mut State| s.acp_step(Event::ProbeMcpServer(target.clone()))));
    }
    let target = id.clone();
    actions.push(small("Edit", Some("Edit")).build(t).key("edit").on_press(move |s: &mut State| s.open_mcp_edit(&target)));
    let mut connection = Vec::new();
    if on && probe == Some("authRequired") {
        let target = id.clone();
        connection.push(
            col()
                .key("auth-notice")
                .w_full()
                .gap(8.0)
                .items_start()
                .child(toned_notice(t, "auth", ui::settings::Tone::Warning, "This server uses OAuth. Select Connect to sign in in your browser."))
                .child(
                    small(if connecting { "Connecting…" } else { "Connect" }, Some("ExternalLink"))
                        .disabled(connecting)
                        .build(t)
                        .key("connect")
                        .on_press(move |s: &mut State| s.connect_mcp(&target)),
                ),
        );
    }
    if on && probe == Some("disconnected") {
        let mut body = "Troupe could not reach this server. Check the command, URL and environment values, then try again.".to_owned();
        if let Some(error) = s.chat.acp.mcp_probe_error.get(&id).and_then(Value::as_str) {
            body = format!("{body}\n{error}");
        }
        let target = id.clone();
        connection.push(
            col()
                .key("unreachable")
                .w_full()
                .gap(8.0)
                .items_start()
                .child(toned_notice(t, "unreachable-notice", ui::settings::Tone::Danger, &body))
                .child(
                    small("Try again", Some("RefreshCw"))
                        .disabled(probing)
                        .build(t)
                        .key("retry")
                        .on_press(move |s: &mut State| s.acp_step(Event::ProbeMcpServer(target.clone()))),
                ),
        );
    }
    let mono_value = |v: &str| text(v.to_owned()).font(mono(12.0, 400.0, 16.0)).color(c.foreground).wrap();
    let mut rows = vec![
        detail_row(t, "transport", "Transport", mono_value(rules::transport_of(server)), None),
        detail_row(t, "target", if remote { "URL" } else { "Command" }, mono_value(rules::target(server)), None),
    ];
    if remote {
        let sign_in = match (probe == Some("authRequired"), connected) {
            (true, true) => "OAuth · sign-in expired",
            (true, false) => "OAuth · not signed in",
            (false, true) => "OAuth · signed in",
            (false, false) => "None needed",
        };
        let target = id.clone();
        let disconnect = connected.then(|| {
            Button::new("Disconnect")
                .variant(ButtonVariant::Ghost)
                .size(ButtonSize::Xs)
                .disabled(connecting)
                .build(t)
                .key("disconnect")
                .h(24.0)
                .on_press(move |s: &mut State| s.disconnect_mcp(&target))
        });
        rows.push(detail_row(t, "sign-in", "Sign-in", text(sign_in).font(sans(12.0, 400.0, 16.0)).color(c.foreground), disconnect));
    }
    connection.push(card(t, "connection-card", rows));
    let target = id.clone();
    let remove = row()
        .key("remove-server")
        .w_full()
        .px(16.0)
        .py(14.0)
        .gap(24.0)
        .items_center()
        .radius(8.0)
        .fill(c.card)
        .border(1.0, c.border)
        .child(
            col()
                .key("words")
                .min_w(0.0)
                .grow(1.0)
                .child(text("Remove server").font(sans(12.0, 500.0, 16.0)).color(c.foreground))
                .child(text("New chats stop offering its tools. You can undo it for a few seconds.").font(sans(12.0, 400.0, 16.0)).color(c.muted_foreground).wrap()),
        )
        .child(
            Button::new("Remove")
                .variant(ButtonVariant::Secondary)
                .size(ButtonSize::Xs)
                .ink(c.destructive)
                .build(t)
                .key("remove")
                .on_press(move |s: &mut State| s.remove_mcp_server(&target)),
        );
    vec![col()
        .key("mcp-server-detail")
        .w_full()
        .gap(32.0)
        .items_stretch()
        .child(top)
        .child(group(t, "connection", "Connection", actions, connection))
        .child(tools(s, t, server, phone))
        .child(remove)]
}

/// `ToolsGroup`: the first five, a filter once there are more, and Show all.
fn tools(s: &State, t: &'static Tokens, server: &Object, phone: bool) -> El {
    let c = &t.colors;
    let id = id_of(server);
    let on = enabled(server);
    let probing = s.mcp_flag(&s.chat.acp.mcp_probing, id);
    let probe = s.probe_status(id);
    let list = s.chat.acp.mcp_tools.get(id).and_then(Value::as_array);
    let note = |body: String| {
        row().key("tools-note").w_full().px(16.0).py(12.0).radius(8.0).fill(c.card).border(1.0, c.border).child(text(body).font(sans(12.0, 400.0, 16.0)).color(c.muted_foreground).wrap())
    };
    let Some(list) = list.filter(|l| !l.is_empty()) else {
        let empty = if !on {
            "Turn the server on to see its tools."
        } else if probing {
            "Checking…"
        } else if probe == Some("authRequired") {
            "Sign in to see this server's tools."
        } else if probe == Some("disconnected") {
            "Tools show once Troupe can reach the server."
        } else if list.is_some() {
            "The server answered but offers no tools."
        } else {
            "Loading tools…"
        };
        return group(t, "tools", "Tools", vec![], vec![note(empty.to_owned())]);
    };
    let query = s.mcp_ui.tools_filter.text.trim().to_lowercase();
    let field = |tool: &Value, key: &str| tool.get(key).and_then(Value::as_str).unwrap_or_default().to_owned();
    let matching: Vec<&Value> = list
        .iter()
        .filter(|tool| query.is_empty() || field(tool, "name").to_lowercase().contains(&query) || field(tool, "description").to_lowercase().contains(&query))
        .collect();
    let shown = if s.mcp_ui.show_all_tools || !query.is_empty() { matching.len() } else { matching.len().min(FIRST_TOOLS) };
    let hidden = matching.len() - shown;
    let mut actions = Vec::new();
    if list.len() > FIRST_TOOLS {
        actions.push(
            filter(
                t,
                "tools-filter",
                &s.mcp_ui.tools_filter,
                "Filter tools",
                "Filter tools",
                phone,
                |e| e.on_edit(|s: &mut State| &mut s.mcp_ui.tools_filter),
                |e| e.on_press(|s: &mut State| s.mcp_ui.tools_filter = TextEdit::new("")),
            )
            .w(208.0),
        );
    }
    let mut content = Vec::new();
    if matching.is_empty() {
        content.push(note(format!("No tools match \"{}\".", s.mcp_ui.tools_filter.text.trim())));
    } else {
        let rows = matching[..shown]
            .iter()
            .enumerate()
            .map(|(i, tool)| {
                let mut r = row()
                    .key(format!("tool-{i}"))
                    .w_full()
                    .px(16.0)
                    .py(10.0)
                    .gap(12.0)
                    .items_start()
                    .child(text(field(tool, "name")).font(mono(12.0, 400.0, 16.0)).color(c.foreground).single_line().truncate().w(176.0));
                let description = field(tool, "description");
                if !description.is_empty() {
                    r = r.child(text(description).font(sans(12.0, 400.0, 16.0)).color(c.muted_foreground).single_line().truncate().min_w(0.0).grow(1.0));
                }
                r
            })
            .collect();
        content.push(card(t, "tools-card", rows));
    }
    if hidden > 0 {
        content.push(
            row().key("show-all-row").w_full().child(
                Button::new(format!("Show all {}", list.len()))
                    .variant(ButtonVariant::Ghost)
                    .size(ButtonSize::Xs)
                    .build(t)
                    .key("show-all")
                    .on_press(|s: &mut State| s.mcp_ui.show_all_tools = true),
            ),
        );
    }
    group_with_meta(t, "tools", "Tools", &list.len().to_string(), actions, content)
}

#[cfg(test)]
#[path = "settings_mcp_tests.rs"]
mod tests;
