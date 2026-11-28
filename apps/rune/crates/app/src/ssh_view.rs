//! The SSH surfaces (`components/ssh/`): the rail's SSH section, the SSH workspace with its
//! Hosts tab and one tab per session, the connection panel (progress, the host-key question,
//! the credential prompt, a failure or an end), and the host editor and confirm dialogs. The
//! SFTP explorer, the transfer tray and the remote file editor are in `ssh_files_view`.

use model::ssh::messages::{self, address_label};
use model::ssh::prompt::SecretKind;
use model::ssh::{HOSTS_TAB, Profile, Session, Status};
use model::workspace::Tab;
use rune::{El, Tx, col, el, icon, row, text, text_input};
use theme::Tokens;
use ui::overlay::Modal;
use ui::type_::{mono, sans};
use ui::{Button, ButtonSize, ButtonVariant};

use crate::State;
use crate::ssh::{CLOSE_SESSION, DELETE_HOST, EDITOR};

pub(crate) fn glyph(name: &str) -> &'static rune::Icon {
    theme::icons::named(name).unwrap_or(&theme::icons::X)
}

fn status_color(t: &Tokens, status: Status) -> rune::Color {
    let c = &t.colors;
    match status {
        Status::Connected => c.success,
        Status::Connecting | Status::AwaitingInput => c.warning,
        Status::Failed => c.destructive,
        Status::Disconnected => c.muted_foreground,
    }
}

fn dot(color: rune::Color) -> El {
    row().size(8.0, 8.0).radius(4.0).fill(color)
}

/// The host's status in lists: its newest session's, or none.
fn host_status(s: &State, profile_id: &str) -> Option<Status> {
    s.ssh.model.sessions_of(profile_id).last().map(|session| session.status)
}

/// The rail's SSH section (`ssh-section`): the hosts, a `+` that adds one, and a press that
/// opens a host's latest session or its row in the Hosts tab.
pub fn rail_section(s: &State, t: &'static Tokens) -> Option<El> {
    if !s.ssh_available() {
        return None;
    }
    let c = &t.colors;
    let add = ui::shell::rail_button(t, glyph("Plus"), "Add SSH host", 20.0, 12.0, false)
        .key("rail-ssh-add")
        .on_press(|s: &mut State| s.ssh_edit_host(None));
    let mut list = col().w_full().px(8.0).gap(1.0).role("list").label("SSH hosts");
    let mut profiles: Vec<&Profile> = s.ssh.model.profiles.iter().collect();
    profiles.sort_by_key(|p| !p.favorite);
    for profile in profiles {
        let id = profile.id.clone();
        let active = s.ssh.model.active_profile_id() == Some(profile.id.as_str());
        let mut line = row()
            .key(format!("rail-ssh-{}", profile.id))
            .role("button")
            .label(format!("SSH host: {}", profile.name))
            .w_full()
            .h(28.0)
            .px(8.0)
            .gap(8.0)
            .items_center()
            .radius(6.0)
            .hover(c.row_hover)
            .child(icon(glyph("Server"), 14.0).color(c.muted_foreground).a11y_hidden())
            .child(text(profile.name.clone()).font(sans(13.0, 500.0, 18.0)).color(c.foreground).single_line().truncate().min_w(0.0).grow(1.0))
            .on_press(move |s: &mut State| s.ssh.model.select_profile(Some(&id)));
        if active {
            line = line.fill(c.row_hover);
        }
        if let Some(status) = host_status(s, &profile.id) {
            line = line.child(dot(status_color(t, status)));
        }
        list = list.child(line);
    }
    if s.ssh.model.profiles.is_empty() && s.ssh.model.is_loaded {
        list = list.child(text("No hosts yet").font(sans(12.0, 400.0, 18.0)).color(c.muted_foreground).px(8.0).py(4.0));
    }
    Some(col().w_full().child(el().h(12.0)).child(ui::shell::section_header(t, "SSH", Some(add))).child(el().h(2.0)).child(list))
}

/// The SSH workspace, in place of the main area while it is open.
pub fn workspace(s: &State, t: &'static Tokens) -> El {
    let c = &t.colors;
    let model = &s.ssh.model;
    let mut tabs = row().key("ssh-tabs").h(36.0).gap(2.0).items_center().role("tablist");
    tabs = tabs.child(tab_chip(t, HOSTS_TAB, "Hosts", model.active_tab == HOSTS_TAB, None, false));
    for session in &model.sessions {
        let name = model.profile(&session.profile_id).map_or("Removed host", |p| p.name.as_str());
        tabs = tabs.child(tab_chip(t, &session.id, name, model.active_tab == session.id, Some(status_color(t, session.status)), true));
    }
    let close = Button::icon_only(glyph("X"), "Close SSH", ButtonSize::IconSm)
        .variant(ButtonVariant::Ghost)
        .build(t)
        .key("ssh-close-workspace")
        .on_press(|s: &mut State| s.close_ssh_workspace());
    let header = row()
        .key("ssh-header")
        .w_full()
        .h(44.0)
        .px(12.0)
        .gap(8.0)
        .items_center()
        .border_sides([0.0, 0.0, 1.0, 0.0], c.border)
        .child(tabs.grow(1.0).min_w(0.0))
        .child(close);
    let body = match model.active_session() {
        Some(session) => session_view(s, t, session),
        None => hosts_view(s, t),
    };
    col().key("ssh-workspace").grow(1.0).h_full().min_w(0.0).fill(c.background).child(header).child(body)
}

fn tab_chip(t: &Tokens, id: &str, label: &str, active: bool, status: Option<rune::Color>, closable: bool) -> El {
    let c = &t.colors;
    let tab = id.to_owned();
    let mut chip = row()
        .key(format!("ssh-tab-{id}"))
        .role("tab")
        .label(label.to_owned())
        .h(28.0)
        .px(10.0)
        .gap(6.0)
        .items_center()
        .radius(6.0)
        .hover(c.row_hover)
        .on_press(move |s: &mut State| s.ssh.model.set_active_tab(&tab));
    if active {
        chip = chip.fill(c.secondary);
    }
    if let Some(color) = status {
        chip = chip.child(dot(color));
    }
    chip = chip.child(text(label.to_owned()).font(sans(12.0, 500.0, 18.0)).color(if active { c.foreground } else { c.muted_foreground }).single_line());
    if closable {
        let tab = id.to_owned();
        chip = chip.child(
            Button::icon_only(glyph("X"), "Close session", ButtonSize::IconXs)
                .variant(ButtonVariant::Ghost)
                .build(t)
                .key(format!("ssh-tab-close-{id}"))
                .on_press(move |s: &mut State| s.ssh_request_close(&tab)),
        );
    }
    chip
}

/// The Hosts tab (`SSHHostsView`): the library, with its actions.
fn hosts_view(s: &State, t: &'static Tokens) -> El {
    let c = &t.colors;
    let toolbar = row()
        .key("ssh-hosts-toolbar")
        .w_full()
        .gap(8.0)
        .items_center()
        .child(text("Hosts").font(sans(16.0, 600.0, 24.0)).color(c.foreground).role("heading").grow(1.0))
        .child(
            Button::new(if s.ssh.model.show_addresses { "Hide addresses" } else { "Show addresses" })
                .variant(ButtonVariant::Ghost)
                .size(ButtonSize::Sm)
                .icon(glyph(if s.ssh.model.show_addresses { "EyeOff" } else { "Eye" }))
                .build(t)
                .key("ssh-toggle-addresses")
                .on_press(|s: &mut State| s.ssh.model.toggle_show_addresses()),
        )
        .child(
            Button::new("Import ~/.ssh/config")
                .variant(ButtonVariant::Outline)
                .size(ButtonSize::Sm)
                .build(t)
                .key("ssh-import")
                .on_press(|s: &mut State| s.ssh_import()),
        )
        .child(Button::new("Add host").size(ButtonSize::Sm).icon(glyph("Plus")).build(t).key("ssh-add-host").on_press(|s: &mut State| s.ssh_edit_host(None)));
    let mut list = col().key("ssh-host-list").w_full().gap(8.0).role("list").label("Hosts");
    if s.ssh.model.profiles.is_empty() {
        list = list.child(
            col()
                .w_full()
                .py(48.0)
                .items_center()
                .gap(8.0)
                .child(icon(glyph("Server"), 40.0).color(c.muted_foreground.alpha(0.5)))
                .child(text("No SSH hosts yet").font(sans(16.0, 500.0, 24.0)).color(c.foreground))
                .child(text("Add a host, or import the ones in ~/.ssh/config.").font(sans(13.0, 400.0, 20.0)).color(c.muted_foreground)),
        );
    }
    for profile in &s.ssh.model.profiles {
        list = list.child(host_row(s, t, profile));
    }
    col()
        .key("ssh-hosts")
        .grow(1.0)
        .w_full()
        .min_h(0.0)
        .child(col().w_full().max_w(880.0).padding(24.0).gap(16.0).child(toolbar).child(list))
        .scroll(rune::ScrollSpec { stick_bottom: false })
}

fn action(t: &Tokens, name: &str, label: &str, key: String) -> El {
    Button::icon_only(glyph(name), label, ButtonSize::IconSm).variant(ButtonVariant::Ghost).build(t).key(key)
}

/// One host (`SSHHostRow`).
fn host_row(s: &State, t: &'static Tokens, profile: &Profile) -> El {
    let c = &t.colors;
    let id = profile.id.clone();
    let selected = s.ssh.model.selected_profile_id.as_deref() == Some(profile.id.as_str());
    let mut names = col().min_w(0.0).grow(1.0).gap(2.0).child(
        text(profile.name.clone()).font(sans(14.0, 500.0, 20.0)).color(c.foreground).single_line().truncate(),
    );
    if s.ssh.model.show_addresses {
        names = names.child(text(address_label(profile)).font(mono(12.0, 400.0, 16.0)).color(c.muted_foreground).single_line().truncate());
    } else {
        names = names.child(text(messages::auth_method_label(&profile.auth_method).to_owned()).font(sans(12.0, 400.0, 16.0)).color(c.muted_foreground));
    }
    let (connect, edit, copy, star, delete) = (id.clone(), id.clone(), id.clone(), id.clone(), id.clone());
    let mut line = row()
        .key(format!("ssh-host-{id}"))
        .role("listitem")
        .label(profile.name.clone())
        .w_full()
        .px(12.0)
        .py(10.0)
        .gap(12.0)
        .items_center()
        .radius(8.0)
        .border(1.0, if selected { c.focus_ring } else { c.border })
        .fill(c.card)
        .child(icon(glyph("Server"), 18.0).color(c.muted_foreground).a11y_hidden());
    if let Some(status) = host_status(s, &profile.id) {
        line = line.child(dot(status_color(t, status)));
    }
    line.child(names)
        .child(action(t, "Star", if profile.favorite { "Remove from favorites" } else { "Add to favorites" }, format!("ssh-fav-{id}")).on_press(move |s: &mut State| s.ssh_toggle_favorite(&star)))
        .child(action(t, "Edit", "Edit host", format!("ssh-edit-{id}")).on_press(move |s: &mut State| s.ssh_edit_host(Some(&edit))))
        .child(action(t, "Copy", "Duplicate host", format!("ssh-dup-{id}")).on_press(move |s: &mut State| s.ssh_duplicate(&copy)))
        .child(action(t, "Trash2", "Delete host", format!("ssh-delete-{id}")).on_press(move |s: &mut State| s.ssh_request_delete(&delete)))
        .child(Button::new("Connect").size(ButtonSize::Sm).build(t).key(format!("ssh-connect-{id}")).on_press(move |s: &mut State| s.ssh_connect(&connect)))
}

/// A session tab: the connection panel until the shell runs, then the shell (with the
/// explorer beside it when open).
fn session_view(s: &State, t: &'static Tokens, session: &Session) -> El {
    let c = &t.colors;
    let profile = s.ssh.model.profile(&session.profile_id).cloned().unwrap_or_else(|| Profile {
        name: "Removed host".into(),
        host: "unknown".into(),
        port: 22,
        username: "unknown".into(),
        auth_method: "password".into(),
        ..Profile::default()
    });
    let shell = session.pty_id.as_ref().map(|pty| {
        let tab = Tab::Terminal { id: format!("ssh-{}", session.id), terminal_id: pty.clone() };
        crate::terminal_view::tab_body(s, t, &tab, false).unwrap_or_else(el)
    });
    let panel = match session.status {
        Status::Connected => None,
        _ => Some(connection_panel(s, t, session, &profile)),
    };
    let conn = session.attempt_id.clone();
    let files = Button::new(if s.ssh.files_open { "Hide files" } else { "Files" })
        .variant(ButtonVariant::Ghost)
        .size(ButtonSize::Sm)
        .icon(glyph("Folder"))
        .disabled(session.status != Status::Connected)
        .build(t)
        .key("ssh-files-toggle")
        .on_press(move |s: &mut State| s.sftp_toggle(&conn));
    let bar = row()
        .key("ssh-session-bar")
        .w_full()
        .h(36.0)
        .px(12.0)
        .gap(8.0)
        .items_center()
        .border_sides([0.0, 0.0, 1.0, 0.0], c.border)
        .child(text(address_label(&profile)).font(mono(12.0, 400.0, 16.0)).color(c.muted_foreground).grow(1.0).single_line().truncate())
        .child(files);
    let mut main = col().grow(1.0).min_w(0.0).h_full();
    if let Some(panel) = panel {
        main = main.child(panel);
    }
    if let Some(shell) = shell {
        main = main.child(shell);
    }
    let mut body = row().key(format!("ssh-session-{}", session.id)).grow(1.0).w_full().min_h(0.0).child(main);
    if s.ssh.files_open && session.status == Status::Connected {
        body = body.child(crate::ssh_files_view::explorer(s, t, &session.attempt_id, &profile.name));
    }
    col().grow(1.0).w_full().min_h(0.0).child(bar).child(body)
}

/// `SSHConnectionPanel`: the stages, the host-key question, the credential prompt, a failure
/// or the session's end.
fn connection_panel(s: &State, t: &'static Tokens, session: &Session, profile: &Profile) -> El {
    let c = &t.colors;
    let tab = session.id.clone();
    let card = col().key("ssh-connection-panel").w_full().max_w(520.0).padding(20.0).gap(16.0).radius(10.0).border(1.0, c.border).fill(c.card);
    let card = if let Some(prompt) = &session.host_key_prompt {
        card.child(crate::ssh_files_view::host_key_question(t, &tab, prompt))
    } else if let Some(prompt) = &session.credential_prompt {
        let kind = match prompt.kind {
            SecretKind::Password => "Password",
            SecretKind::Passphrase => "Key passphrase",
        };
        let (submit, cancel) = (tab.clone(), tab.clone());
        let field = text_input(&s.ssh.secret, sans(14.0, 400.0, 20.0))
            .single_line()
            .secure()
            .key("ssh-secret")
            .label(kind)
            .color(c.foreground)
            .w_full()
            .h(34.0)
            .px(12.0)
            .radius(4.0)
            .fill(c.secondary)
            .border(1.0, c.border)
            .focus_ring(1.0, 0.0, c.focus_ring)
            .autofocus()
            .on_edit(|s: &mut State| &mut s.ssh.secret)
            .on_submit(move |s: &mut State, _tx: &mut Tx| s.ssh_submit_secret(&submit));
        let error = session.error.as_ref().map(|e| messages::describe_error(&e.code, profile));
        let save = Button::new(if s.ssh.save_secret { "Save in keychain: on" } else { "Save in keychain: off" })
            .variant(ButtonVariant::Ghost)
            .size(ButtonSize::Sm)
            .icon(glyph("KeyRound"))
            .build(t)
            .key("ssh-save-secret")
            .on_press(|s: &mut State| s.ssh.save_secret = !s.ssh.save_secret);
        card.child(text(error.as_ref().map_or_else(|| format!("{kind} needed"), |e| e.title.clone())).font(sans(14.0, 600.0, 20.0)).color(c.foreground).role("heading"))
            .children(error.map(|e| text(e.body).font(sans(13.0, 400.0, 20.0)).color(c.muted_foreground)))
            .child(field)
            .child(
                row()
                    .w_full()
                    .gap(8.0)
                    .items_center()
                    .child(save)
                    .child(el().grow(1.0))
                    .child(Button::new("Cancel").variant(ButtonVariant::Ghost).size(ButtonSize::Sm).build(t).key("ssh-secret-cancel").on_press(move |s: &mut State| s.ssh_cancel(&cancel)))
                    .child(Button::new("Connect").size(ButtonSize::Sm).build(t).key("ssh-secret-connect").on_press({
                        let tab = tab.clone();
                        move |s: &mut State| s.ssh_submit_secret(&tab)
                    })),
            )
    } else if session.status == Status::Connecting {
        let mut stages = col().gap(6.0);
        let current = session.stage.as_deref().unwrap_or("connecting");
        let at = messages::STAGES.iter().position(|stage| *stage == current).unwrap_or(0);
        for (index, stage) in messages::STAGES.iter().enumerate() {
            let (name, ink) = match index.cmp(&at) {
                std::cmp::Ordering::Less => ("Check", c.success),
                std::cmp::Ordering::Equal => ("Loader2", c.foreground),
                std::cmp::Ordering::Greater => ("ChevronRight", c.muted_foreground),
            };
            stages = stages.child(
                row().gap(8.0).items_center().child(icon(glyph(name), 14.0).color(ink)).child(text(messages::stage_label(stage, profile)).font(sans(13.0, 400.0, 20.0)).color(ink)),
            );
        }
        card.child(text(format!("Connecting to {}", profile.name)).font(sans(14.0, 600.0, 20.0)).color(c.foreground).role("heading"))
            .child(stages)
            .child(row().w_full().justify_end().child(Button::new("Cancel").variant(ButtonVariant::Ghost).size(ButtonSize::Sm).build(t).key("ssh-cancel").on_press(move |s: &mut State| s.ssh_cancel(&tab))))
    } else {
        let message = match (&session.ended, &session.error) {
            (Some(end), _) => messages::describe_end(end, profile),
            (None, Some(error)) => messages::describe_error(&error.code, profile),
            (None, None) => messages::describe_error("", profile),
        };
        let detail = session.error.as_ref().map(|e| e.detail.clone()).filter(|d| !d.is_empty());
        let (retry, edit) = (tab.clone(), session.profile_id.clone());
        let mut actions = row().w_full().gap(8.0).justify_end();
        if message.suggest_edit {
            actions = actions.child(Button::new("Edit host").variant(ButtonVariant::Outline).size(ButtonSize::Sm).build(t).key("ssh-edit-from-error").on_press(move |s: &mut State| s.ssh_edit_host(Some(&edit))));
        }
        actions = actions.child(
            Button::new(if session.ended.is_some() { "Reconnect" } else { "Try again" }).size(ButtonSize::Sm).build(t).key("ssh-retry").on_press(move |s: &mut State| s.ssh_retry(&retry)),
        );
        card.child(text(message.title).font(sans(14.0, 600.0, 20.0)).color(c.foreground).role("heading"))
            .child(text(message.body).font(sans(13.0, 400.0, 20.0)).color(c.muted_foreground))
            .children(detail.map(|d| text(d).font(mono(12.0, 400.0, 16.0)).color(c.muted_foreground).selectable(true)))
            .child(actions)
    };
    col().w_full().padding(24.0).items_center().child(card)
}

/// The host editor and the delete and close confirms.
pub fn dialogs(s: &State, t: &'static Tokens, layer: &mut Vec<El>) {
    host_editor(s, t, layer);
    delete_host(s, t, layer);
    close_session(s, t, layer);
    crate::ssh_files_view::dialogs(s, t, layer);
}

fn field(t: &Tokens, label: &str, edit: &rune::TextEdit, key: &str, on_edit: fn(&mut State) -> &mut rune::TextEdit, secret: bool) -> El {
    let c = &t.colors;
    let mut input = text_input(edit, sans(14.0, 400.0, 20.0))
        .single_line()
        .key(key.to_owned())
        .label(label.to_owned())
        .color(c.foreground)
        .w_full()
        .h(34.0)
        .px(12.0)
        .radius(4.0)
        .fill(c.secondary)
        .border(1.0, c.border)
        .focus_ring(1.0, 0.0, c.focus_ring)
        .on_edit(on_edit);
    if secret {
        input = input.secure();
    }
    col().w_full().gap(4.0).child(text(label.to_owned()).font(sans(12.0, 500.0, 16.0)).color(c.muted_foreground)).child(input)
}

/// `SSHHostEditor`: name, address, port, user and sign-in method.
fn host_editor(s: &State, t: &'static Tokens, layer: &mut Vec<El>) {
    if !s.overlays.is_open(EDITOR) {
        return;
    }
    let c = &t.colors;
    let form = &s.ssh.form;
    let mut methods = row().gap(4.0);
    for (method, label) in [("key", "Private key"), ("password", "Password"), ("agent", "SSH agent")] {
        let mut chip = Button::new(label).size(ButtonSize::Sm).variant(if form.auth == method { ButtonVariant::Secondary } else { ButtonVariant::Ghost }).build(t).key(format!("ssh-auth-{method}"));
        chip = chip.on_press(move |s: &mut State| s.ssh.form.auth = method.to_owned());
        methods = methods.child(chip);
    }
    let mut body = col()
        .w_full()
        .gap(12.0)
        .child(field(t, "Name", &form.name, "ssh-form-name", |s| &mut s.ssh.form.name, false))
        .child(
            row()
                .w_full()
                .gap(8.0)
                .child(field(t, "Host", &form.host, "ssh-form-host", |s| &mut s.ssh.form.host, false).grow(1.0))
                .child(field(t, "Port", &form.port, "ssh-form-port", |s| &mut s.ssh.form.port, false).w(96.0)),
        )
        .child(field(t, "Username", &form.username, "ssh-form-user", |s| &mut s.ssh.form.username, false))
        .child(col().gap(4.0).child(text("Sign in with").font(sans(12.0, 500.0, 16.0)).color(c.muted_foreground)).child(methods));
    body = match form.auth.as_str() {
        "key" => body
            .child(field(t, "Private key path", &form.key_path, "ssh-form-key", |s| &mut s.ssh.form.key_path, false))
            .child(field(t, "Key passphrase (optional, saved to the keychain)", &form.passphrase, "ssh-form-passphrase", |s| &mut s.ssh.form.passphrase, true)),
        "password" => body.child(field(t, "Password (optional, saved to the keychain)", &form.password, "ssh-form-password", |s| &mut s.ssh.form.password, true)),
        _ => body,
    };
    if let Some(error) = &form.error {
        body = body.child(text(error.clone()).font(sans(13.0, 400.0, 20.0)).color(c.destructive).role("alert"));
    }
    let footer = row()
        .w_full()
        .gap(8.0)
        .justify_end()
        .child(Button::new("Cancel").variant(ButtonVariant::Ghost).size(ButtonSize::Sm).build(t).key("ssh-form-cancel").on_press(|s: &mut State| s.close(EDITOR)))
        .child(Button::new("Save").variant(ButtonVariant::Outline).size(ButtonSize::Sm).disabled(form.saving).build(t).key("ssh-form-save").on_press(|s: &mut State| s.ssh_save_host(false)))
        .child(Button::new("Save and connect").size(ButtonSize::Sm).disabled(form.saving).build(t).key("ssh-form-connect").on_press(|s: &mut State| s.ssh_save_host(true)));
    let title = if form.id.is_some() { "Edit host" } else { "Add host" };
    Modal::new(EDITOR, title).width(520.0).child(body.child(footer)).open(true).on_close(|s: &mut State| s.close(EDITOR)).build(t, layer);
}

/// A confirm with a destructive action: `title`, `message`, and the two buttons.
pub(crate) fn confirm(t: &'static Tokens, key: &'static str, title: &str, message: String, label: &str, on_confirm: fn(&mut State), on_cancel: fn(&mut State), layer: &mut Vec<El>) {
    let c = &t.colors;
    let footer = row()
        .w_full()
        .gap(8.0)
        .justify_end()
        .child(Button::new("Cancel").variant(ButtonVariant::Ghost).size(ButtonSize::Sm).build(t).key("cancel").autofocus().on_press(move |s: &mut State| on_cancel(s)))
        .child(
            Button::new(label)
                .size(ButtonSize::Sm)
                .fill(c.destructive_fill)
                .ink(c.destructive_foreground)
                .build(t)
                .key("confirm")
                .on_press(move |s: &mut State| on_confirm(s)),
        );
    let body = col().w_full().gap(16.0).child(text(message).font(sans(14.0, 400.0, 20.0)).color(c.muted_foreground)).child(footer);
    Modal::new(key, title).width(440.0).child(body).open(true).on_close(move |s: &mut State| on_cancel(s)).build(t, layer);
}

fn delete_host(s: &State, t: &'static Tokens, layer: &mut Vec<El>) {
    let Some((id, name)) = s.ssh.delete_host.as_ref().filter(|_| s.overlays.is_open(DELETE_HOST)) else { return };
    let open = s.ssh.model.sessions_of(id).count();
    confirm(
        t,
        DELETE_HOST,
        "Delete host",
        messages::delete_host_message(name, open),
        "Delete",
        State::ssh_confirm_delete,
        |s| {
            s.ssh.delete_host = None;
            s.close(DELETE_HOST);
        },
        layer,
    );
}

fn close_session(s: &State, t: &'static Tokens, layer: &mut Vec<El>) {
    let Some(tab) = s.ssh.model.pending_close.as_ref().filter(|_| s.overlays.is_open(CLOSE_SESSION)) else { return };
    let profile = s.ssh.model.session(tab).and_then(|session| s.ssh.model.profile(&session.profile_id));
    let message = match profile {
        Some(p) => format!("“{}” on {} ends, and any command still running there stops.", p.name, messages::host_label(&p.host, p.port)),
        None => "The shell ends, and any command still running there stops.".to_owned(),
    };
    confirm(
        t,
        CLOSE_SESSION,
        "Close session?",
        message,
        "Close session",
        State::ssh_confirm_close,
        State::ssh_cancel_close,
        layer,
    );
}
