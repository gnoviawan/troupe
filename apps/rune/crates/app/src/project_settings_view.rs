//! The project settings pages (`pages/project-settings/`): General, Shell, Environment variables
//! and Worktree symlinks, and the footer that holds Save and Discard while the draft is dirty.
//! Accounts and Browser are later units' pages. The window frame is `settings_view`.

use model::settings::project::{self, NO_SHELL};
use rune::{El, TextEdit, col, icon, row, text, text_input};
use theme::Tokens;
use ui::settings::{Tone, card, empty_state, group, setting, setting_stacked, toned_notice};
use ui::type_::{mono, sans};
use ui::{Button, ButtonSize, ButtonVariant};

use crate::State;

fn glyph(name: &str) -> &'static rune::Icon {
    theme::icons::named(name).expect("bundled icon")
}

pub fn page(s: &State, t: &'static Tokens, id: &str, phone: bool, layer: &mut Vec<El>) -> Vec<El> {
    match id {
        "general" => general(s, t, phone),
        "shell" => shell(s, t, phone, layer),
        "env-vars" => env_vars(s, t, phone),
        "symlinks" => symlinks(s, t),
        _ => Vec::new(),
    }
}

/// `PANEL_FIELD_CLASS` at 32 px (44 px on touch), `font-mono` where the text is a path or a key.
fn field(t: &Tokens, edit: &TextEdit, label: String, placeholder: &str, code: bool, coarse: bool) -> El {
    let c = &t.colors;
    let font = match (code, coarse) {
        (true, true) => mono(16.0, 400.0, 24.0),
        (true, false) => mono(12.0, 400.0, 16.0),
        (false, true) => sans(16.0, 400.0, 24.0),
        (false, false) => sans(12.0, 400.0, 16.0),
    };
    text_input(edit, font)
        .single_line()
        .label(label)
        .placeholder(placeholder.to_owned(), c.muted_foreground)
        .color(c.foreground)
        .h(if coarse { 44.0 } else { 32.0 })
        .py(if coarse { 9.0 } else { 7.0 })
        .px(10.0)
        .radius(8.0)
        .fill(c.card)
        .border(1.0, c.border.alpha(0.6))
        .focus_ring(2.0, -1.0, c.focus_ring)
}

/// A `SettingsRow` that stacks under 35rem, as on a phone.
fn responsive(t: &Tokens, phone: bool, key: &str, label: &str, description: &str, control: El) -> El {
    if phone { setting_stacked(t, key, label, description, control) } else { setting(t, key, label, description, Some(control)) }
}

fn general(s: &State, t: &'static Tokens, phone: bool) -> Vec<El> {
    let ui = &s.project_ui;
    let sized = |e: El| if phone { e.grow(1.0).min_w(0.0) } else { e.w(240.0) };
    let name = sized(field(t, &ui.name, "Name".into(), "", false, phone).key("project-name").on_edit(|s: &mut State| &mut s.project_ui.name));
    let root = sized(field(t, &ui.path, "Root directory".into(), "", true, phone).key("project-root-directory").on_edit(|s: &mut State| &mut s.project_ui.path));
    let browse = Button::new("Browse")
        .variant(ButtonVariant::Outline)
        .size(ButtonSize::Sm)
        .build(t)
        .key("browse")
        .h(if phone { 44.0 } else { 32.0 })
        .on_press(|s: &mut State| s.toast_later("Browsing the server's folders"));
    let colours = crate::settings_pages::swatches(t, "Project color", &ui.color, phone, |s, colour| s.project_ui.color = colour.to_owned());
    vec![group(
        t,
        "project",
        "Project",
        vec![],
        vec![card(
            t,
            "project-card",
            vec![
                responsive(t, phone, "project-name-row", "Name", "", name),
                responsive(t, phone, "project-root-row", "Root directory", "Changing it only affects new terminals.", row().key("root").gap(8.0).grow(1.0).items_center().child(root).child(browse)),
                responsive(t, phone, "project-color-row", "Color", "Marks this project in the sidebar.", colours),
            ],
        )],
    )]
}

fn shell(s: &State, t: &'static Tokens, phone: bool, layer: &mut Vec<El>) -> Vec<El> {
    let value = project::shell_value(&s.project_ui.shell, s.shells());
    let options = project::shell_options(&value, s.shells(), "App default");
    let labels: Vec<&str> = options.iter().map(|o| o.label.as_str()).collect();
    let chosen = options.iter().position(|o| o.value == value);
    let select = crate::settings_pages::select(s, t, layer, ("project-default-shell", "Shell"), &labels, chosen, |s, i| {
        let value = project::shell_value(&s.project_ui.shell, s.shells());
        let options = project::shell_options(&value, s.shells(), "App default");
        if let Some(picked) = options.get(i) {
            s.project_ui.shell = project::shell_from_value(&picked.value);
        }
    });
    debug_assert_eq!(options[0].value, NO_SHELL);
    vec![group(
        t,
        "default-shell",
        "Default shell",
        vec![],
        vec![card(t, "default-shell-card", vec![responsive(t, phone, "project-default-shell-row", "Shell", "App default uses the shell set in Preferences.", select)])],
    )]
}

/// The quiet trash button of a list row; `hover:text-destructive`.
fn remove_button(t: &Tokens, label: String, phone: bool) -> El {
    let side = if phone { 44.0 } else { 32.0 };
    ui::quiet_icon_button(t, glyph("Trash2"), label, side, 14.0).key("remove")
}

fn small(t: &'static Tokens, label: &str, icon_name: &str, variant: ButtonVariant, key: &str) -> El {
    Button::new(label.to_owned()).variant(variant).size(ButtonSize::Sm).icon(glyph(icon_name)).build(t).key(key.to_owned()).h(32.0)
}

fn env_vars(s: &State, t: &'static Tokens, phone: bool) -> Vec<El> {
    let ui = &s.project_ui;
    let actions = || {
        vec![
            small(t, "Import from .env", "Upload", ButtonVariant::Outline, "import-env").on_press(|s: &mut State| s.import_env()),
            small(t, "Add variable", "Plus", ButtonVariant::Default, "add-env").on_press(|s: &mut State| s.add_env_var()),
        ]
    };
    let mut notices = Vec::new();
    if let Some(e) = &ui.import_error {
        notices.push(toned_notice(t, "import-error", Tone::Danger, e));
    }
    if let Some(w) = &ui.import_warning {
        notices.push(toned_notice(t, "import-warning", Tone::Warning, w));
    }
    if ui.env.is_empty() {
        let mut out = notices;
        out.push(empty_state(
            t,
            "env-empty",
            "Braces",
            "No environment variables",
            "Add variables by hand, or import them from the .env file in the project root.",
            Some(row().gap(8.0).children(actions())),
        ));
        return vec![col().key("env-vars").w_full().gap(8.0).items_stretch().children(out)];
    }
    let rows = ui
        .env
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let name = if v.key.text.is_empty() { format!("variable {}", i + 1) } else { v.key.text.clone() };
            row()
                .key(format!("env-{i}"))
                .w_full()
                .px(16.0)
                .py(8.0)
                .gap(8.0)
                .items_center()
                .child(
                    field(t, &v.key, format!("Name of {name}"), "KEY", true, phone)
                        .key("key")
                        .w_frac(0.4)
                        .min_w(0.0)
                        .on_edit(move |s: &mut State| &mut s.project_ui.env[i].key),
                )
                .child(
                    field(t, &v.value, format!("Value of {name}"), "value", true, phone)
                        .key("value")
                        .grow(1.0)
                        .min_w(0.0)
                        .on_edit(move |s: &mut State| &mut s.project_ui.env[i].value),
                )
                .child(remove_button(t, format!("Remove {name}"), phone).on_press(move |s: &mut State| s.remove_env_var(i)))
        })
        .collect();
    let title = if ui.env.len() == 1 { "1 variable".to_owned() } else { format!("{} variables", ui.env.len()) };
    let mut content = notices;
    content.push(card(t, "env-card", rows));
    vec![group(t, "env-vars", &title, actions(), content)]
}

/// The rows stay 32 px on touch too, as `symlinks-section.tsx` draws them.
fn symlinks(s: &State, t: &'static Tokens) -> Vec<El> {
    let ui = &s.project_ui;
    let can_sync = s.settings_project().is_some_and(|p| p.client.is_git_repo == Some(true));
    let actions = || {
        let sync = Button::new("Sync from .gitignore")
            .variant(ButtonVariant::Outline)
            .size(ButtonSize::Sm)
            .icon(glyph(if ui.syncing { "Loader2" } else { "RefreshCw" }))
            .disabled(ui.syncing || !can_sync)
            .build(t)
            .key("sync-gitignore")
            .h(32.0)
            .on_press(|s: &mut State| s.sync_gitignore());
        vec![sync, small(t, "Add directory", "Plus", ButtonVariant::Default, "add-dir").on_press(|s: &mut State| s.add_symlink_dir())]
    };
    if ui.dirs.is_empty() {
        return vec![empty_state(
            t,
            "symlinks-empty",
            "Link2",
            "No linked directories",
            "New worktrees link these from the project root instead of reinstalling them. Sync finds ignored folders such as node_modules.",
            Some(row().gap(8.0).children(actions())),
        )];
    }
    let rows = ui
        .dirs
        .iter()
        .enumerate()
        .map(|(i, d)| {
            let name = if d.text.is_empty() { format!("directory {}", i + 1) } else { d.text.clone() };
            row()
                .key(format!("dir-{i}"))
                .w_full()
                .px(16.0)
                .py(8.0)
                .gap(8.0)
                .items_center()
                .child(
                    field(t, d, format!("Path of {name}"), "node_modules", true, false)
                        .key("path")
                        .grow(1.0)
                        .min_w(0.0)
                        .on_edit(move |s: &mut State| &mut s.project_ui.dirs[i]),
                )
                .child(remove_button(t, format!("Remove {name}"), false).on_press(move |s: &mut State| s.remove_symlink_dir(i)))
        })
        .collect();
    let title = if ui.dirs.len() == 1 { "1 directory".to_owned() } else { format!("{} directories", ui.dirs.len()) };
    vec![group(t, "symlinks", &title, actions(), vec![card(t, "symlinks-card", rows)])]
}

/// The bar under the page while the draft is dirty: "Unsaved changes", Discard, Save changes.
pub fn footer(t: &'static Tokens, phone: bool) -> El {
    let c = &t.colors;
    let status = row()
        .key("unsaved")
        .role("status")
        .gap(8.0)
        .items_center()
        .grow(1.0)
        .child(icon(glyph("Info"), 14.0).color(c.warning).a11y_hidden())
        .child(text("Unsaved changes").font(sans(12.0, 400.0, 16.0)).color(c.muted_foreground));
    let discard = Button::new("Discard").variant(ButtonVariant::Ghost).size(ButtonSize::Sm).build(t).key("discard").h(32.0).on_press(|s: &mut State| s.discard_project_settings());
    let save = Button::new("Save changes").size(ButtonSize::Sm).build(t).key("save").h(32.0).on_press(|s: &mut State| s.save_project_settings());
    row()
        .key("settings-footer")
        .w_full()
        .px(if phone { 16.0 } else { 40.0 })
        .py(12.0)
        .justify_center()
        .fill(c.background)
        .border_sides([1.0, 0.0, 0.0, 0.0], c.border)
        .child(row().key("bar").w_full().max_w(ui::settings::COLUMN).gap(8.0).items_center().child(status).child(discard).child(save))
}
