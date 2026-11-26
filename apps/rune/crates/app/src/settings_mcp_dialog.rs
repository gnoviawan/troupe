//! The MCP servers page's dialogs: Add and Edit (`McpServerDialog.tsx` with `McpServerForm.tsx`,
//! the form view), and the OAuth paste-back (`McpOAuthPasteDialog.tsx`), which opens for a web
//! sign-in once the host knows its link.

use model::acp::Event;
use model::settings::mcp::{self as rules, Auth, Draft, Field, Kind, Local, Object, Remote, Row, Transport};
use rune::{El, TextEdit, Tx, col, row, text};
use serde_json::{Value, json};
use theme::Tokens;
use ui::overlay::{Dialog, Select};
use ui::settings::{Tone, toned_notice};
use ui::type_::{mono, sans};
use ui::{Button, ButtonSize, ButtonVariant, Input};

use crate::State;
use crate::settings_mcp::Action;

/// The overlay keys of the Add and Edit dialog and of the paste-back dialog.
pub const DIALOG: &str = "settings-mcp-dialog";
pub const PASTE: &str = "settings-mcp-paste";
const AUTH_SELECT: &str = "mcp-auth";
const TRANSPORT_SELECT: &str = "mcp-transport";
const COMMAND_STARTERS: [&str; 3] = ["npx -y", "uvx", "docker run -i --rm"];

/// A name and value row of the form.
#[derive(Default)]
pub struct RowEdit {
    pub name: TextEdit,
    pub value: TextEdit,
}

impl RowEdit {
    fn of(row: &Row) -> Self {
        Self { name: TextEdit::new(&row.name), value: TextEdit::new(&row.value) }
    }

    fn row(&self) -> Row {
        Row::new(&self.name.text, &self.value.text)
    }
}

/// The Add or Edit dialog's form (`McpDialogState` in its form view).
pub struct ServerDialog {
    /// The server Edit opened; None for Add.
    pub editing: Option<Object>,
    pub kind: Kind,
    pub name: TextEdit,
    pub url: TextEdit,
    pub transport: Transport,
    pub pinned: bool,
    pub auth: Auth,
    pub headers: Vec<RowEdit>,
    pub command: TextEdit,
    pub env: Vec<RowEdit>,
    pub advanced: bool,
    /// Save was pressed on an invalid form: its errors show from then on.
    pub show_errors: bool,
    pub saving: bool,
}

impl ServerDialog {
    fn of(draft: &Draft, editing: Option<Object>) -> Self {
        let advanced = draft.remote.transport_pinned || (draft.remote.auth == Auth::Browser && draft.remote.headers.iter().any(|r| !r.name.trim().is_empty()));
        Self {
            editing,
            kind: draft.kind,
            name: TextEdit::new(&draft.name),
            url: TextEdit::new(&draft.remote.url),
            transport: draft.remote.transport,
            pinned: draft.remote.transport_pinned,
            auth: draft.remote.auth,
            headers: draft.remote.headers.iter().map(RowEdit::of).collect(),
            command: TextEdit::new(&draft.local.command_line),
            env: draft.local.env.iter().map(RowEdit::of).collect(),
            advanced,
            show_errors: false,
            saving: false,
        }
    }

    /// The form as a draft; the transport follows the URL until the user picks one (`withUrl`).
    pub fn draft(&self) -> Draft {
        let remote = Remote {
            url: self.url.text.clone(),
            transport: self.transport,
            transport_pinned: self.pinned,
            auth: self.auth,
            headers: self.headers.iter().map(RowEdit::row).collect(),
        };
        Draft {
            kind: self.kind,
            name: self.name.text.clone(),
            remote: rules::with_url(&remote, &self.url.text),
            local: Local { command_line: self.command.text.clone(), env: self.env.iter().map(RowEdit::row).collect() },
        }
    }
}

/// The paste-back dialog's fields, for the sign-in they were typed for.
#[derive(Default)]
pub struct PasteUi {
    /// The server id and sign-in link the fields belong to.
    pub flow: Option<(String, String)>,
    pub address: TextEdit,
    /// The inline error, and whether it ends the flow (`final`).
    pub error: Option<(String, bool)>,
    pub completing: bool,
}

fn glyph(name: &str) -> &'static rune::Icon {
    theme::icons::named(name).expect("bundled icon")
}

/// `isLoopbackAuthRedirectUrl`: an http(s) address on this machine.
fn loopback(raw: &str) -> bool {
    let Some(rest) = raw.strip_prefix("http://").or_else(|| raw.strip_prefix("https://")) else { return false };
    let authority = &rest[..rest.find(['/', '?', '#']).unwrap_or(rest.len())];
    let host_port = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
    let host = if host_port.starts_with('[') {
        &host_port[..host_port.find(']').map_or(host_port.len(), |i| i + 1)]
    } else {
        host_port.rsplit_once(':').map_or(host_port, |(h, _)| h)
    };
    let host = host.to_ascii_lowercase();
    if host == "localhost" || host.ends_with(".localhost") || host == "[::1]" {
        return true;
    }
    let octets: Vec<&str> = host.split('.').collect();
    octets.len() == 4 && octets[0] == "127" && octets.iter().all(|o| (1..=3).contains(&o.len()) && o.parse::<u16>().is_ok_and(|n| n <= 255))
}

impl State {
    pub(crate) fn open_mcp_add(&mut self) {
        log::info!("[settings.McpServersSettings] add dialog");
        self.mcp_ui.dialog = Some(ServerDialog::of(&Draft::default(), None));
        self.overlays.open(DIALOG);
    }

    pub(crate) fn open_mcp_edit(&mut self, id: &str) {
        let Some(server) = self.chat.acp.mcp_servers.iter().find(|s| s["id"] == json!(id)).cloned() else { return };
        log::info!("[settings.McpServersSettings] edit dialog {id}");
        self.mcp_ui.dialog = Some(ServerDialog::of(&rules::draft_from_server(&server), Some(server)));
        self.overlays.open(DIALOG);
    }

    pub(crate) fn close_mcp_dialog(&mut self) {
        if self.mcp_ui.dialog.as_ref().is_some_and(|d| d.saving) {
            return;
        }
        if self.settings_ui.select.is_some() {
            self.close_select();
        }
        self.mcp_ui.dialog = None;
        self.close(DIALOG);
    }

    /// `saveForm`: an invalid form shows its errors; a valid one adds or saves the server.
    fn save_mcp_dialog(&mut self) {
        let Some(dialog) = &mut self.mcp_ui.dialog else { return };
        let config = match rules::draft_to_config(&dialog.draft()) {
            Ok(config) => config,
            Err(errors) => {
                let fields: Vec<String> = errors.iter().map(|(f, _)| serde_json::to_value(f).ok().and_then(|v| v.as_str().map(str::to_owned)).unwrap_or_default()).collect();
                log::warn!("[settings.McpServersSettings] MCP server form rejected: {}", fields.join(", "));
                dialog.show_errors = true;
                return;
            }
        };
        dialog.saving = true;
        let mut server = config.clone();
        match dialog.editing.clone() {
            None => {
                server.insert("id".into(), json!(crate::ids::uuid()));
                server.insert("enabled".into(), json!(true));
                self.mcp_write(Action::Add(1), Event::ImportMcpServers(vec![server]));
            }
            Some(target) => {
                server.insert("id".into(), target["id"].clone());
                server.insert("enabled".into(), json!(target.get("enabled") != Some(&json!(false))));
                if let Some(catalog) = target.get("catalog").filter(|_| rules::same_launch(&target, &config)) {
                    server.insert("catalog".into(), catalog.clone());
                }
                self.mcp_write(Action::Edit, Event::SaveMcpServer(server));
            }
        }
    }

    /// A write the dialog sent answered: the form is no longer saving.
    pub(crate) fn mcp_dialog_settled(&mut self) {
        if let Some(dialog) = &mut self.mcp_ui.dialog {
            dialog.saving = false;
        }
    }

    fn with_dialog(&mut self, f: impl FnOnce(&mut ServerDialog)) {
        if let Some(dialog) = &mut self.mcp_ui.dialog {
            f(dialog);
        }
    }

    /// The paste-back fields follow the sign-in on screen.
    fn paste_flow(&self) -> Option<(String, Value)> {
        self.chat.acp.mcp_oauth_paste_back.iter().next().map(|(id, pending)| (id.clone(), pending.clone()))
    }

    fn complete_paste(&mut self) {
        let Some((id, pending)) = self.paste_flow() else { return };
        let address = self.mcp_ui.paste.address.text.trim().to_owned();
        if !loopback(&address) {
            let redirect = pending["redirectUri"].as_str().unwrap_or_default();
            self.mcp_ui.paste.error = Some((format!("Paste the address your browser landed on. It starts with {redirect}."), false));
            return;
        }
        self.mcp_ui.paste.completing = true;
        self.mcp_ui.paste.error = None;
        self.acp_step(Event::CompleteMcpOAuthPaste(id, address));
    }

    /// `pasteErrorFor`: the host refused the pasted address.
    pub(crate) fn paste_failed(&mut self, error: &model::acp::mcp::McpError) {
        let redirect = self.paste_flow().and_then(|(_, p)| p["redirectUri"].as_str().map(str::to_owned)).unwrap_or_default();
        let detail = &error.message;
        let paste = &mut self.mcp_ui.paste;
        paste.completing = false;
        paste.error = match error.code.as_deref() {
            Some("OAUTH_UNKNOWN_STATE") => None,
            Some("OAUTH_BAD_REDIRECT") => Some((format!("That address does not belong to this sign-in. Paste the full address that starts with {redirect}."), false)),
            Some("OAUTH_DENIED") => Some((format!("Sign-in was not allowed ({detail}). Dismiss, then press Connect to try again."), true)),
            Some("OAUTH_EXCHANGE_FAILED") => Some((format!("Troupe could not finish signing in ({detail}). Dismiss, then press Connect to try again."), true)),
            _ => Some((if detail.is_empty() { "Could not reach Troupe. Try again.".to_owned() } else { detail.clone() }, false)),
        };
    }

    fn dismiss_paste(&mut self) {
        if self.mcp_ui.paste.completing {
            return;
        }
        if let Some((id, _)) = self.paste_flow() {
            self.acp_step(Event::CancelMcpOAuth(id));
        }
        self.paste_sync();
    }

    /// Fresh fields for a new sign-in on screen, and its overlay open while one waits.
    pub(crate) fn paste_sync(&mut self) {
        let flow = self.paste_flow().map(|(id, p)| (id, p["authUrl"].as_str().unwrap_or_default().to_owned()));
        if flow != self.mcp_ui.paste.flow {
            let open = flow.is_some();
            self.mcp_ui.paste = PasteUi { flow, ..Default::default() };
            if open && !self.overlays.is_open(PASTE) {
                self.overlays.open(PASTE);
            } else if !open && self.overlays.is_open(PASTE) {
                self.overlays.close(PASTE);
            }
        }
    }
}

fn label(t: &Tokens, words: &str) -> El {
    text(words.to_owned()).font(sans(14.0, 500.0, 16.0)).color(t.colors.foreground)
}

fn hint(t: &Tokens, words: &str) -> El {
    text(words.to_owned()).font(sans(12.0, 400.0, 16.0)).color(t.colors.muted_foreground).wrap()
}

fn field_error(t: &Tokens, errors: &[(Field, String)], field: Field) -> Option<El> {
    errors.iter().find(|(f, _)| *f == field).map(|(_, m)| text(m.clone()).role("alert").font(sans(12.0, 400.0, 16.0)).color(t.colors.destructive).wrap())
}

fn block(key: &str, parts: Vec<Option<El>>) -> El {
    col().key(key.to_owned()).w_full().gap(8.0).items_stretch().children(parts.into_iter().flatten().collect::<Vec<_>>())
}

/// `KindPicker`: two radio cards.
fn kind_picker(t: &Tokens, kind: Kind, phone: bool) -> El {
    let c = &t.colors;
    let options = [
        (Kind::Remote, "On the internet", "The server is hosted for you. You paste a link and sign in.", "https://mcp.linear.app/mcp"),
        (Kind::Local, "On this computer", "Troupe starts the server with a command, like npx or uvx.", "npx -y @modelcontextprotocol/server-github"),
    ];
    let mut group = if phone { col() } else { row() }.key("kind").role("radiogroup").label("Where the server runs").w_full().gap(12.0).items_stretch();
    for (value, title, description, example) in options {
        let chosen = value == kind;
        let key = if value == Kind::Remote { "kind-remote" } else { "kind-local" };
        let mut option = col()
            .key(key)
            .role("radio")
            .label(title)
            .focusable(true)
            .gap(4.0)
            .padding(16.0)
            .radius(12.0)
            .items_stretch()
            .child(text(title).font(sans(14.0, 500.0, 20.0)).color(c.foreground))
            .child(hint(t, description))
            .child(text(example).font(mono(12.0, 400.0, 16.0)).color(c.muted_foreground).wrap().mt(4.0))
            .on_press(move |s: &mut State| s.with_dialog(|d| d.kind = value));
        option = if chosen { option.fill(c.secondary).border(1.0, c.foreground.alpha(0.2)) } else { option.border(1.0, c.border).hover(c.foreground.alpha(0.03)) };
        group = group.child(if phone { option } else { option.grow(1.0).min_w(0.0) });
    }
    group
}

type RowsOf = fn(&mut ServerDialog) -> &mut Vec<RowEdit>;

/// `KeyValueRows`: name and value per line, Remove on each, Add under them.
fn rows(t: &Tokens, list: &[RowEdit], key: &str, noun: &str, placeholders: (&str, &str), of: RowsOf) -> El {
    let c = &t.colors;
    let mut out = col().key(key.to_owned()).w_full().gap(8.0).items_stretch();
    for (i, r) in list.iter().enumerate() {
        let label = format!("{noun} {}", i + 1);
        let problem = rules::row_problem(&r.row());
        let name = Input::new(&r.name)
            .placeholder(placeholders.0)
            .label(format!("{label} name"))
            .invalid(problem == Some("Needs a name."))
            .build(t)
            .key("name")
            .grow(2.0)
            .min_w(0.0)
            .on_edit(move |s: &mut State| &mut of(s.mcp_ui.dialog.as_mut().expect("the dialog is open"))[i].name);
        let value = Input::new(&r.value)
            .placeholder(placeholders.1)
            .label(format!("{label} value"))
            .invalid(problem == Some("Needs a value."))
            .build(t)
            .key("value")
            .grow(3.0)
            .min_w(0.0)
            .on_edit(move |s: &mut State| &mut of(s.mcp_ui.dialog.as_mut().expect("the dialog is open"))[i].value);
        let remove = ui::quiet_icon_button(t, glyph("X"), format!("Remove {}", label.to_lowercase()), 32.0, 14.0).key("remove").on_press(move |s: &mut State| {
            s.with_dialog(|d| {
                let list = of(d);
                if i < list.len() {
                    list.remove(i);
                }
                if list.is_empty() {
                    list.push(RowEdit::default());
                }
            });
        });
        let mut line = col().key(format!("row-{i}")).w_full().gap(4.0).items_stretch().child(row().key("fields").w_full().gap(8.0).items_center().child(name).child(value).child(remove));
        if let Some(problem) = problem {
            line = line.child(text(problem).font(sans(12.0, 400.0, 16.0)).color(c.warning));
        }
        out = out.child(line);
    }
    out.child(
        row().key("add-row").child(
            Button::new("Add")
                .variant(ButtonVariant::Ghost)
                .size(ButtonSize::Xs)
                .icon(glyph("Plus"))
                .build(t)
                .key("add")
                .on_press(move |s: &mut State| s.with_dialog(|d| of(d).push(RowEdit::default()))),
        ),
    )
}

fn headers_of(d: &mut ServerDialog) -> &mut Vec<RowEdit> {
    &mut d.headers
}

fn env_of(d: &mut ServerDialog) -> &mut Vec<RowEdit> {
    &mut d.env
}

fn dialog_select(s: &State, t: &'static Tokens, menus: &mut Vec<El>, (id, label): (&'static str, &str), options: &[&str], chosen: usize, pick: fn(&mut State, usize)) -> El {
    Select::new(format!("select-{id}"), options)
        .value(Some(chosen))
        .label(label)
        .width(500.0)
        .open(s.settings_ui.select == Some(id))
        .on_open(move |s: &mut State| s.open_select(id))
        .on_pick(move |s: &mut State, i| {
            s.close_select();
            pick(s, i);
        })
        .on_close(|s: &mut State| s.close_select())
        .build(t, menus)
}

fn remote_fields(s: &State, t: &'static Tokens, d: &ServerDialog, errors: &[(Field, String)], menus: &mut Vec<El>) -> Vec<El> {
    let c = &t.colors;
    let in_advanced = d.auth == Auth::Browser;
    let url = Input::new(&d.url).placeholder("https://mcp.example.com/mcp").label("Server URL").invalid(errors.iter().any(|(f, _)| *f == Field::Url)).build(t).key("url-input").on_edit(|s: &mut State| &mut s.mcp_ui.dialog.as_mut().expect("the dialog is open").url);
    let auth = dialog_select(s, t, menus, (AUTH_SELECT, "Sign-in"), &["Browser sign-in, if the server asks", "API key or token"], usize::from(d.auth == Auth::Token), |s, i| {
        s.with_dialog(|d| {
            let next = if i == 1 { Auth::Token } else { Auth::Browser };
            let remote = rules::with_auth(&d.draft().remote, next);
            d.auth = remote.auth;
            d.headers = remote.headers.iter().map(RowEdit::of).collect();
        });
    });
    let headers = || rows(t, &d.headers, "headers", "Header", if in_advanced { ("Header name", "Value") } else { ("Authorization", "Bearer …") }, headers_of);
    let mut out = vec![
        block("url", vec![Some(label(t, "Server URL")), Some(url), field_error(t, errors, Field::Url), Some(hint(t, "The address the server's docs give you. It usually ends in /mcp or /sse."))]),
        block("auth", vec![Some(label(t, "Sign-in")), Some(auth), Some(hint(t, "Most hosted servers (Linear, Notion, Asana) use browser sign-in. Pick API key only if the docs give you one."))]),
    ];
    if !in_advanced {
        out.push(block("headers-set", vec![Some(label(t, "Headers")), Some(headers()), field_error(t, errors, Field::Headers)]));
    }
    let open = d.advanced || (in_advanced && errors.iter().any(|(f, _)| *f == Field::Headers));
    let chevron = glyph(if open { "ChevronDown" } else { "ChevronRight" });
    let trigger = row()
        .key("advanced-trigger")
        .role("button")
        .label("Advanced")
        .focusable(true)
        .w_full()
        .py(12.0)
        .gap(6.0)
        .items_center()
        .child(rune::icon(chevron, 14.0).color(c.secondary_foreground).a11y_hidden())
        .child(text("Advanced").font(sans(14.0, 400.0, 20.0)).color(c.secondary_foreground))
        .on_press(|s: &mut State| s.with_dialog(|d| d.advanced = !d.advanced));
    let mut advanced = col().key("advanced").w_full().items_stretch().border_sides([1.0, 0.0, 1.0, 0.0], c.border).child(trigger);
    if open {
        let transport = dialog_select(s, t, menus, (TRANSPORT_SELECT, "Connection type"), &["Streamable HTTP (recommended)", "SSE (older servers)"], usize::from(d.draft().remote.transport == Transport::Sse), |s, i| {
            s.with_dialog(|d| {
                d.transport = if i == 1 { Transport::Sse } else { Transport::Http };
                d.pinned = true;
            });
        });
        let mut body = col().key("advanced-body").w_full().gap(16.0).pb(16.0).items_stretch().child(block(
            "transport",
            vec![Some(label(t, "Connection type")), Some(transport), Some(hint(t, "Picked from the URL. Change it only if the docs say SSE and the URL does not end in /sse."))],
        ));
        if in_advanced {
            body = body.child(block("custom-headers", vec![Some(label(t, "Custom headers")), Some(headers()), field_error(t, errors, Field::Headers)]));
        }
        advanced = advanced.child(body);
    }
    out.push(advanced);
    out
}

fn local_fields(t: &'static Tokens, d: &ServerDialog, errors: &[(Field, String)]) -> Vec<El> {
    let command = Input::new(&d.command)
        .placeholder("npx -y @scope/mcp-server")
        .label("Command")
        .invalid(errors.iter().any(|(f, _)| *f == Field::CommandLine))
        .build(t)
        .key("command-input")
        .on_edit(|s: &mut State| &mut s.mcp_ui.dialog.as_mut().expect("the dialog is open").command);
    let mut starters = row().key("starters").w_full().gap(8.0).items_center().child(hint(t, "Common:"));
    for starter in COMMAND_STARTERS {
        starters = starters.child(
            Button::new(format!("{starter} …")).variant(ButtonVariant::Outline).size(ButtonSize::Xs).build(t).key(format!("starter-{starter}")).on_press(move |s: &mut State| {
                s.with_dialog(|d| {
                    let rest = d.command.text.trim_start().to_owned();
                    let line = if rest.starts_with(starter) { rest } else { format!("{starter} {rest}") };
                    d.command = TextEdit::new(&line);
                });
            }),
        );
    }
    vec![
        block(
            "command",
            vec![
                Some(label(t, "Command")),
                Some(command),
                field_error(t, errors, Field::CommandLine),
                Some(hint(t, "What you would type in a terminal to start the server. Troupe runs it on the computer that hosts this workspace.")),
                Some(starters),
            ],
        ),
        block(
            "env",
            vec![
                Some(label(t, "Environment variables")),
                Some(rows(t, &d.env, "env-rows", "Environment variable", ("NAME", "value"), env_of)),
                field_error(t, errors, Field::Env),
                Some(hint(t, "API keys and settings the server reads. Values stay in Troupe's app data. Write $VARIABLE to read one from your shell instead.")),
            ],
        ),
    ]
}

/// The Add or Edit dialog, in its form view.
/// What the dialog's header, footer and padding take: the form scrolls in the rest of 90% of
/// the window (`max-h-[90dvh] overflow-y-auto`).
const DIALOG_CHROME: f32 = 190.0;

pub fn server_dialog(s: &State, t: &'static Tokens, phone: bool, height: f32, layer: &mut Vec<El>) {
    let Some(d) = &s.mcp_ui.dialog else { return };
    let errors = if d.show_errors { rules::draft_to_config(&d.draft()).err().unwrap_or_default() } else { Vec::new() };
    let mut menus = Vec::new();
    let name = Input::new(&d.name)
        .placeholder("e.g. linear")
        .label("Name")
        .invalid(errors.iter().any(|(f, _)| *f == Field::Name))
        .build(t)
        .key("name-input")
        .on_edit(|s: &mut State| &mut s.mcp_ui.dialog.as_mut().expect("the dialog is open").name);
    let mut body = col()
        .key("mcp-server-form")
        .w_full()
        .gap(20.0)
        .items_stretch()
        .child(kind_picker(t, d.kind, phone))
        .child(block("name", vec![Some(label(t, "Name")), Some(name), field_error(t, &errors, Field::Name), Some(hint(t, "Shown in the server list and in chat tool calls."))]));
    let fields = match d.kind {
        Kind::Remote => remote_fields(s, t, d, &errors, &mut menus),
        Kind::Local => local_fields(t, d, &errors),
    };
    body = body.children(fields);
    body = body.child(toned_notice(t, "secrets", Tone::Warning, "Environment values and headers are saved in app data. Use $VARIABLE references instead of real secrets."));
    let adding = d.editing.is_none();
    if adding {
        body = body.child(hint(t, "Only add servers from developers you trust. A server's tools can read and change things on your behalf, and Troupe can't check what they do."));
    }
    let cancel = Button::new("Cancel").variant(ButtonVariant::Outline).size(ButtonSize::Sm).disabled(d.saving).build(t).key("cancel").on_press(|s: &mut State| s.close_mcp_dialog());
    let save_label = match (d.saving, adding) {
        (true, _) => "Saving…",
        (false, true) => "Add server",
        (false, false) => "Save",
    };
    let save = Button::new(save_label).size(ButtonSize::Sm).disabled(d.saving).build(t).key("save").on_press(|s: &mut State| s.save_mcp_dialog());
    Dialog::new("mcp-server-dialog", if adding { "Add MCP server" } else { "Edit MCP server" })
        .description(if adding { "Where does this server run? The setup page or README usually tells you." } else { "Change how Troupe reaches this server." })
        .width(576.0)
        .child(col().key("mcp-form-scroll").w_full().max_h((height * 0.9 - DIALOG_CHROME).max(160.0)).child(body).scroll(rune::ScrollSpec { stick_bottom: false }))
        .footer(cancel)
        .footer(save)
        .open(true)
        .on_close(|s: &mut State| s.close_mcp_dialog())
        .build(t, layer);
    layer.extend(menus);
}

/// `McpOAuthPasteDialog`, for the first sign-in waiting on a paste.
pub fn paste_dialog(s: &State, t: &'static Tokens, layer: &mut Vec<El>) {
    let Some((_, pending)) = s.paste_flow() else { return };
    let c = &t.colors;
    let paste = &s.mcp_ui.paste;
    let auth_url = pending["authUrl"].as_str().unwrap_or_default().to_owned();
    let redirect = pending["redirectUri"].as_str().unwrap_or_default();
    let name = pending["serverName"].as_str().unwrap_or_default();
    let (open_url, copy_url) = (auth_url.clone(), auth_url.clone());
    let link = row()
        .key("link")
        .w_full()
        .gap(8.0)
        .items_center()
        .child(text(auth_url).font(mono(12.0, 400.0, 16.0)).color(c.muted_foreground).single_line().truncate().min_w(0.0).grow(1.0).px(8.0).py(6.0).radius(6.0).fill(c.muted.alpha(0.4)).border(1.0, c.border.alpha(0.6)))
        .child(Button::new("Open").variant(ButtonVariant::Outline).size(ButtonSize::Sm).build(t).key("open").on_press_tx(move |_: &mut State, tx: &mut Tx| {
            if let Err(e) = tx.open_url(&open_url) {
                log::warn!("[settings.McpOAuthPasteDialog] the sign-in page did not open: {e}");
            }
        }))
        .child(Button::new("Copy").variant(ButtonVariant::Outline).size(ButtonSize::Sm).build(t).key("copy").on_press_tx(move |s: &mut State, tx: &mut Tx| {
            tx.copy(copy_url.clone());
            s.toasts.push(s.now, ui::overlay::ToastKind::Success, "Sign-in link copied", None);
        }));
    let final_error = paste.error.as_ref().is_some_and(|(_, f)| *f);
    let input = Input::new(&paste.address)
        .placeholder(format!("{redirect}?code=…"))
        .label("Paste the redirect address")
        .disabled(final_error)
        .build(t)
        .key("address")
        .on_edit(|s: &mut State| &mut s.mcp_ui.paste.address);
    let mut body = col()
        .key("paste-body")
        .w_full()
        .gap(16.0)
        .items_stretch()
        .child(link)
        .child(block("address-field", vec![Some(text("Paste the redirect address").font(sans(12.0, 500.0, 16.0)).color(c.foreground)), Some(input), Some(hint(t, "A browser on the same machine as Troupe finishes by itself, and this dialog closes."))]));
    if let Some((message, _)) = &paste.error {
        body = body.child(toned_notice(t, "paste-error", Tone::Danger, message));
    }
    let can_complete = !paste.completing && !final_error && !paste.address.text.trim().is_empty();
    let dismiss = Button::new("Dismiss").variant(ButtonVariant::Outline).size(ButtonSize::Sm).disabled(paste.completing).build(t).key("dismiss").on_press(|s: &mut State| s.dismiss_paste());
    let complete = Button::new(if paste.completing { "Completing…" } else { "Complete sign-in" }).size(ButtonSize::Sm).disabled(!can_complete).build(t).key("complete").on_press(|s: &mut State| s.complete_paste());
    Dialog::new("mcp-paste-dialog", format!("Finish signing in to {name}"))
        .description("Sign in on the page that opened. If it did not open, use Open or Copy below. When your browser then lands on a 127.0.0.1 address that fails to load or shows an error, paste that address here.")
        .width(520.0)
        .child(body)
        .footer(dismiss)
        .footer(complete)
        .open(true)
        .on_close(|s: &mut State| s.dismiss_paste())
        .build(t, layer);
}

#[cfg(test)]
mod loopback_tests {
    #[test]
    fn only_addresses_on_this_machine_pass() {
        for (raw, ok) in [
            ("http://127.0.0.1:4321/callback?code=x", true),
            ("http://localhost/cb", true),
            ("https://app.localhost:9/cb", true),
            ("http://[::1]:80/cb", true),
            ("http://127.999.1.1/cb", false),
            ("http://example.com/cb", false),
            ("ftp://127.0.0.1/cb", false),
        ] {
            assert_eq!(super::loopback(raw), ok, "{raw}");
        }
    }
}
