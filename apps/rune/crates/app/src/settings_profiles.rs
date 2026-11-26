//! The Profiles and Skills pages (`terminal-profiles-section.tsx`, `skills-section.tsx`). A
//! profile's editor parses through `model::settings::profiles`; a save or a delete writes the
//! whole `terminalProfiles` list, and a delete offers Undo on a toast that stays until it is
//! closed or replaced. Skills come from the host's `/skills` list the composer also reads.

use model::settings::profiles::{self, Draft, Errors, Profile};
use rune::{El, TextEdit, col, row, text};
use serde_json::{Value, json};
use theme::Tokens;
use ui::overlay::toast::ToastKind;
use ui::overlay::{Dialog, Select};
use ui::settings::{card, empty_state, filter, group, segmented};
use ui::type_::{mono, sans};
use ui::{Button, ButtonSize, ButtonVariant, Input, Textarea};

use crate::State;

/// The overlay key of the profile editor.
pub const EDITOR: &str = "settings-profile-editor";
/// The editor's shell select (`SettingsUi::select`).
const SHELL_SELECT: &str = "profile-shell";
/// Rows a long skill list shows before "Show N more" (`FIRST_ROWS`).
const FIRST_ROWS: usize = 6;

/// The profile editor's fields; `id` is None for a new profile.
#[derive(Default)]
pub struct ProfileEditor {
    pub id: Option<String>,
    pub name: TextEdit,
    /// The draft's shell: a detected shell's path, empty for the default, or typed.
    pub shell: TextEdit,
    /// "Other path or name" is picked: the shell field shows.
    pub other: bool,
    pub args: TextEdit,
    pub cwd: TextEdit,
    pub env: TextEdit,
    pub errors: Errors,
}

/// The Skills page's own state.
#[derive(Default)]
pub struct SkillsUi {
    pub filter: TextEdit,
    /// 0 all, 1 global, 2 project.
    pub scope: usize,
    /// Lists whose "Show N more" was pressed ("global", "project").
    pub expanded: Vec<&'static str>,
}

/// A detected shell (`ShellInfo`).
pub type ShellInfo = model::settings::project::Shell;

impl State {
    fn profiles(&self) -> Vec<Profile> {
        Profile::list(&self.settings.settings)
    }

    /// The shells `/shells` found, read once per window.
    pub(crate) fn shells(&self) -> &[ShellInfo] {
        self.settings_ui.shells.as_deref().unwrap_or_default()
    }

    pub(crate) fn fetch_shells(&mut self) {
        if self.settings_ui.shells.is_some() {
            return;
        }
        self.settings_ui.shells = Some(Vec::new());
        if let Some(host) = &mut self.host {
            host.fetch("/shells".into(), crate::host::Fetch::Shells);
        }
    }

    pub(crate) fn shells_listed(&mut self, result: Result<Value, client::http::HttpError>) {
        let listed = result.map_err(|e| e.to_string()).and_then(|v| {
            serde_json::from_value::<Vec<ShellInfo>>(v.get("available").cloned().unwrap_or_default()).map_err(|e| e.to_string())
        });
        match listed {
            Ok(list) => {
                log::info!("[settings] {} shells detected", list.len());
                self.settings_ui.shells = Some(list);
            }
            Err(e) => log::warn!("[settings] listing shells failed: {e}"),
        }
    }

    /// Opens the editor on `profile`, or empty for a new one.
    fn edit_profile(&mut self, profile: Option<&Profile>) {
        let draft = profile.map(Draft::of).unwrap_or_default();
        let other = !draft.shell.is_empty() && !self.shells().iter().any(|s| s.path == draft.shell);
        self.settings_ui.profile = ProfileEditor {
            id: profile.map(|p| p.id.clone()),
            name: TextEdit::new(&draft.name),
            shell: TextEdit::new(&draft.shell),
            other,
            args: TextEdit::new(&draft.args),
            cwd: TextEdit::new(&draft.cwd),
            env: TextEdit::new(&draft.env),
            errors: Errors::default(),
        };
        self.fetch_shells();
        self.overlays.open(EDITOR);
    }

    fn save_profile(&mut self) {
        let e = &self.settings_ui.profile;
        let draft = Draft {
            name: e.name.text.clone(),
            shell: e.shell.text.clone(),
            args: e.args.text.clone(),
            cwd: e.cwd.text.clone(),
            env: e.env.text.clone(),
        };
        let id = e.id.clone().unwrap_or_else(crate::ids::uuid);
        match draft.parse(&id) {
            Ok(profile) => {
                log::info!("[settings] save terminal profile {id}");
                let list = profiles::saved(&self.profiles(), profile);
                self.set_profiles(&list);
                self.close(EDITOR);
            }
            Err(errors) => self.settings_ui.profile.errors = errors,
        }
    }

    fn set_profiles(&mut self, list: &[Profile]) {
        self.set_setting(profiles::KEY, json!(list));
    }

    /// `remove`: the profile goes at once; the toast's Undo puts it back where it was.
    fn delete_profile(&mut self, id: &str) {
        let list = self.profiles();
        let Some(index) = list.iter().position(|p| p.id == id) else { return };
        let profile = list[index].clone();
        let rest: Vec<Profile> = list.into_iter().filter(|p| p.id != id).collect();
        self.set_profiles(&rest);
        log::info!("[TerminalProfilesSection.delete] Terminal profile deleted; undo offered");
        self.undo_toast(&format!("Deleted {}", profile.name), Undo::Profile { profile, index });
    }

    /// `showUndoToast`: one at a time, with no timer.
    pub(crate) fn undo_toast(&mut self, title: &str, undo: Undo) {
        if let Some((old, _)) = self.settings_ui.undo.take() {
            self.toasts.dismiss(old);
        }
        let id = self.toasts.push(self.now, ToastKind::Message, title, None);
        self.toasts.action(id, "Undo");
        self.toasts.stick(id);
        self.settings_ui.undo = Some((id, undo));
    }

    /// The undo toast's action, when `toast` is the undo toast.
    pub(crate) fn run_undo(&mut self, toast: u64) {
        let Some((id, undo)) = self.settings_ui.undo.take_if(|(id, _)| *id == toast) else { return };
        self.toasts.dismiss(id);
        match undo {
            Undo::Profile { profile, index } => {
                if let Some(list) = profiles::restored(&self.profiles(), profile, index) {
                    self.set_profiles(&list);
                    log::info!("[TerminalProfilesSection.undoDelete] Terminal profile restored by undo");
                }
            }
        }
    }
}

/// What an undo toast puts back.
#[derive(Clone, Debug, PartialEq)]
pub enum Undo {
    Profile { profile: Profile, index: usize },
}

fn glyph(name: &str) -> &'static rune::Icon {
    theme::icons::named(name).expect("bundled icon")
}

fn icon_button(t: &Tokens, name: &str, label: String) -> El {
    ui::quiet_icon_button(t, glyph(name), label, 28.0, 14.0)
}

pub fn profiles_page(s: &State, t: &'static Tokens) -> Vec<El> {
    let list = s.profiles();
    let add = Button::new("Add profile")
        .variant(ButtonVariant::Secondary)
        .size(ButtonSize::Xs)
        .icon(glyph("Plus"))
        .disabled(!s.settings.is_loaded)
        .build(t)
        .key("add-profile")
        .on_press(|s: &mut State| s.edit_profile(None));
    if list.is_empty() {
        return vec![empty_state(
            t,
            "profiles-empty",
            "Terminal",
            "No terminal profiles",
            "A profile opens a terminal with your shell, folder and variables in one step.",
            Some(add),
        )];
    }
    let c = &t.colors;
    let rows = list
        .iter()
        .map(|p| {
            let line = p.command_line();
            let default_shell = p.shell.is_empty() && p.args.is_empty();
            let mut meta = row()
                .key("meta")
                .min_w(0.0)
                .mt(2.0)
                .child(text(line).font(if default_shell { sans(12.0, 400.0, 16.0) } else { mono(12.0, 400.0, 16.0) }).color(c.muted_foreground).single_line());
            if !p.cwd.is_empty() {
                meta = meta.child(text(format!(" · {}", p.cwd)).font(mono(12.0, 400.0, 16.0)).color(c.muted_foreground).single_line().truncate().min_w(0.0));
            }
            let (edit_id, delete_id) = (p.id.clone(), p.id.clone());
            row()
                .key(format!("profile-{}", p.id))
                .w_full()
                .py(12.0)
                .pl(16.0)
                .pr(10.0)
                .gap(12.0)
                .items_center()
                .child(
                    col()
                        .grow(1.0)
                        .min_w(0.0)
                        .min_w(0.0)
                        .child(text(p.name.clone()).font(sans(12.0, 500.0, 16.0)).color(c.foreground).truncate())
                        .child(meta),
                )
                .child(icon_button(t, "Edit", format!("Edit {}", p.name)).key("edit").on_press(move |s: &mut State| {
                    let profile = s.profiles().into_iter().find(|p| p.id == edit_id);
                    s.edit_profile(profile.as_ref());
                }))
                .child(icon_button(t, "Trash2", format!("Delete {}", p.name)).key("delete").on_press(move |s: &mut State| s.delete_profile(&delete_id)))
        })
        .collect();
    let title = if list.len() == 1 { "1 profile".to_owned() } else { format!("{} profiles", list.len()) };
    vec![group(t, "profiles", &title, vec![add], vec![card(t, "profiles-card", rows)])]
}

fn field_label(t: &Tokens, label: &str, optional: bool) -> El {
    let c = &t.colors;
    let mut r = row().child(text(label.to_owned()).font(sans(12.0, 500.0, 16.0)).color(c.foreground));
    if optional {
        r = r.child(text(" (optional)").font(sans(12.0, 400.0, 16.0)).color(c.muted_foreground));
    }
    r
}

fn help(t: &Tokens, words: &str) -> El {
    text(words.to_owned()).font(sans(12.0, 400.0, 16.0)).color(t.colors.muted_foreground).wrap()
}

fn error(t: &Tokens, words: &Option<String>) -> Option<El> {
    words.as_ref().map(|w| text(w.clone()).role("alert").font(sans(12.0, 400.0, 16.0)).color(t.colors.destructive).wrap())
}

fn field(key: &str, parts: Vec<El>) -> El {
    col().key(key.to_owned()).w_full().gap(6.0).items_stretch().children(parts)
}

/// `TerminalProfileDialog`.
pub fn editor(s: &State, t: &'static Tokens, layer: &mut Vec<El>) {
    if !s.overlays.is_open(EDITOR) {
        return;
    }
    let e = &s.settings_ui.profile;
    let shells = s.shells();
    let mut options: Vec<&str> = vec!["Default shell"];
    options.extend(shells.iter().map(|sh| sh.display_name.as_str()));
    options.push("Other path or name");
    let chosen = if e.other {
        options.len() - 1
    } else if e.shell.text.is_empty() {
        0
    } else {
        shells.iter().position(|sh| sh.path == e.shell.text).map_or(0, |i| i + 1)
    };
    let mut menus = Vec::new();
    let shell = Select::new("select-profile-shell", &options)
        .value(Some(chosen))
        .label("Shell")
        .width(462.0)
        .open(s.settings_ui.select == Some(SHELL_SELECT))
        .on_open(|s: &mut State| s.open_select(SHELL_SELECT))
        .on_pick(|s: &mut State, i| {
            s.close_select();
            let count = s.shells().len();
            let path = (1..=count).contains(&i).then(|| s.shells()[i - 1].path.clone());
            let e = &mut s.settings_ui.profile;
            e.other = i == count + 1;
            if !e.other {
                e.shell = TextEdit::new(path.unwrap_or_default());
            }
        })
        .on_close(|s: &mut State| s.close_select())
        .build(t, &mut menus);
    let name = Input::new(&e.name).placeholder("e.g. Node dev").label("Name").invalid(e.errors.name.is_some()).build(t).key("name-input").on_edit(|s: &mut State| &mut s.settings_ui.profile.name);
    let mut body = col().key("profile-fields").w_full().gap(16.0).items_stretch();
    body = body.child(field("name", [Some(field_label(t, "Name", false)), Some(name), error(t, &e.errors.name)].into_iter().flatten().collect()));
    body = body.child(field("shell", vec![field_label(t, "Shell", false), shell]));
    if e.other {
        let path = Input::new(&e.shell).placeholder("e.g. nu or C:\\tools\\nu.exe").label("Shell path or name").build(t).key("shell-input").on_edit(|s: &mut State| &mut s.settings_ui.profile.shell);
        body = body.child(field("shell-path", vec![field_label(t, "Shell path or name", false), path]));
    }
    let args = Textarea::new(&e.args).rows(3).placeholder("-NoProfile").label("Arguments").build(t).key("args-input").on_edit(|s: &mut State| &mut s.settings_ui.profile.args);
    body = body.child(field(
        "args",
        [
            Some(field_label(t, "Arguments", true)),
            Some(args),
            Some(help(t, "One argument per line. Each line reaches the shell as one argument.")),
            error(t, &e.errors.args),
        ]
        .into_iter()
        .flatten()
        .collect(),
    ));
    let cwd = Input::new(&e.cwd).placeholder("Active project or worktree folder").label("Start folder").build(t).key("cwd-input").on_edit(|s: &mut State| &mut s.settings_ui.profile.cwd);
    body = body.child(field("cwd", vec![field_label(t, "Start folder", true), cwd]));
    let env = Textarea::new(&e.env).rows(4).placeholder("NODE_ENV=development").label("Environment variables").build(t).key("env-input").on_edit(|s: &mut State| &mut s.settings_ui.profile.env);
    body = body.child(field(
        "env",
        [
            Some(field_label(t, "Environment variables", true)),
            Some(env),
            Some(help(
                t,
                "One KEY=value per line. They override project variables of the same name and are saved in plain text with your settings, so keep secrets out.",
            )),
            error(t, &e.errors.env),
        ]
        .into_iter()
        .flatten()
        .collect(),
    ));
    let cancel = Button::new("Cancel").variant(ButtonVariant::Outline).size(ButtonSize::Sm).build(t).key("cancel").on_press(|s: &mut State| s.close(EDITOR));
    let save = Button::new("Save profile").size(ButtonSize::Sm).build(t).key("save").on_press(|s: &mut State| s.save_profile());
    Dialog::new("profile-editor", if e.id.is_some() { "Edit profile" } else { "New profile" })
        .description("Empty fields use what New Terminal uses: the default shell and the active folder.")
        .child(body)
        .footer(cancel)
        .footer(save)
        .open(true)
        .on_close(|s: &mut State| s.close(EDITOR))
        .build(t, layer);
    layer.extend(menus);
}

fn count_text(n: usize) -> String {
    if n == 1 { "1 skill".into() } else { format!("{n} skills") }
}

/// The Skills page: the filter and the scope switch, then the project's and the home folder's
/// skills. A skill's name opens its SKILL.md in the editor.
pub fn skills_page(s: &State, t: &'static Tokens, phone: bool) -> Vec<El> {
    let c = &t.colors;
    let project = s.active_project();
    let root = project.and_then(|p| p.path.clone()).unwrap_or_default();
    let listed = s.composer.skills.get(&root);
    let query = s.settings_ui.skills.filter.text.trim().to_lowercase();
    let matching: Vec<&ui::composer::menu::Skill> = listed
        .into_iter()
        .flatten()
        .filter(|k| query.is_empty() || k.name.to_lowercase().contains(&query) || k.description.to_lowercase().contains(&query))
        .collect();
    let global: Vec<_> = matching.iter().copied().filter(|k| k.scope == "global").collect();
    let local: Vec<_> = matching.iter().copied().filter(|k| k.scope == "project").collect();
    let scope = s.settings_ui.skills.scope;
    let (show_project, show_global) = (scope != 1, scope != 2);
    let filter_well = filter(
        t,
        "skills-filter",
        &s.settings_ui.skills.filter,
        "Filter skills",
        "Filter by name or description",
        phone,
        |e| e.on_edit(|s: &mut State| &mut s.settings_ui.skills.filter),
        |e| e.on_press(|s: &mut State| s.settings_ui.skills.filter = TextEdit::new("")),
    )
    .grow(1.0)
    .min_w(192.0);
    let switch = segmented(
        t,
        "skills-scope",
        "Which skills",
        &[("All", Some(global.len() + local.len())), ("Global", Some(global.len())), ("Project", Some(local.len()))],
        scope,
        true,
        |i, e| e.on_press(move |s: &mut State| s.settings_ui.skills.scope = i),
    );
    let mut out = vec![row().key("skills-tools").w_full().gap(8.0).items_center().wrap().child(filter_well).child(switch)];
    if s.composer.skills_loading.contains(&root) {
        out.push(text("Loading skills…").font(sans(12.0, 400.0, 16.0)).color(c.muted_foreground));
        return out;
    }
    let shown = if show_project { local.len() } else { 0 } + if show_global { global.len() } else { 0 };
    if !query.is_empty() && shown == 0 {
        let clear = Button::new("Clear filter").variant(ButtonVariant::Secondary).size(ButtonSize::Xs).build(t).key("clear-filter").on_press(|s: &mut State| s.settings_ui.skills.filter = TextEdit::new(""));
        out.push(
            col()
                .key("skills-none")
                .w_full()
                .gap(12.0)
                .px(16.0)
                .py(32.0)
                .items_center()
                .radius(8.0)
                .fill(c.card)
                .border(1.0, c.border)
                .child(text(format!("No skills match \"{}\".", s.settings_ui.skills.filter.text.trim())).font(sans(12.0, 400.0, 16.0)).color(c.muted_foreground))
                .child(clear),
        );
        return out;
    }
    if show_project {
        let content = match project {
            Some(_) => skill_list(s, t, "project", &local),
            None => vec![note(t, "Open a project to see its skills.")],
        };
        let title_meta = project.map(|p| format!("{} · {}", p.name, count_text(local.len())));
        out.push(group_with_meta(t, "skills-project", "Project", title_meta.as_deref(), content));
    }
    if show_global {
        let meta = format!("Your home folder · {}", count_text(global.len()));
        out.push(group_with_meta(t, "skills-global", "Global", Some(&meta), skill_list(s, t, "global", &global)));
    }
    out
}

/// A group whose heading carries a muted meta line after the title (`SettingsGroup meta`).
fn group_with_meta(t: &Tokens, key: &str, title: &str, meta: Option<&str>, content: Vec<El>) -> El {
    let c = &t.colors;
    let mut head = row().key("head").w_full().min_h(28.0).gap(8.0).items_center().child(text(title.to_owned()).font(sans(14.0, 500.0, 20.0)).color(c.foreground).role("heading"));
    if let Some(meta) = meta {
        head = head.child(text(meta.to_owned()).font(sans(12.0, 400.0, 16.0)).color(c.muted_foreground).truncate().min_w(0.0));
    }
    col().key(key.to_owned()).role("group").label(title.to_owned()).w_full().gap(12.0).items_stretch().child(head).children(content)
}

fn note(t: &Tokens, words: &str) -> El {
    let c = &t.colors;
    row().key("note").w_full().px(16.0).py(12.0).radius(8.0).fill(c.card).border(1.0, c.border).child(text(words.to_owned()).font(sans(12.0, 400.0, 16.0)).color(c.muted_foreground))
}

fn skill_list(s: &State, t: &'static Tokens, which: &'static str, skills: &[&ui::composer::menu::Skill]) -> Vec<El> {
    if skills.is_empty() {
        return vec![note(t, "No skills in this folder.")];
    }
    let c = &t.colors;
    let all = s.settings_ui.skills.expanded.contains(&which);
    let shown = if all { skills.len() } else { skills.len().min(FIRST_ROWS) };
    let rows = skills[..shown]
        .iter()
        .map(|k| {
            let path = k.path.clone();
            let name = k.name.clone();
            let mut words = col().grow(1.0).min_w(0.0).child(
                row()
                    .key("open")
                    .role("button")
                    .label(format!("Open {}", k.name))
                    .focusable(true)
                    .child(text(k.name.clone()).font(sans(12.0, 500.0, 16.0)).color(c.foreground).truncate())
                    .on_press(move |s: &mut State| {
                        log::info!("[SkillsSection] open {name}");
                        s.close_settings();
                        s.open_file(&path);
                    }),
            );
            if !k.description.is_empty() {
                words = words.child(text(k.description.clone()).font(sans(12.0, 400.0, 16.0)).color(c.muted_foreground).single_line().truncate().mt(2.0));
            }
            row().key(format!("skill-{}-{}", k.scope, k.name)).w_full().py(10.0).pl(16.0).pr(10.0).gap(12.0).items_center().child(words)
        })
        .collect();
    let mut out = vec![card(t, &format!("{which}-card"), rows)];
    let hidden = skills.len() - shown;
    if hidden > 0 {
        out.push(
            Button::new(format!("Show {hidden} more"))
                .variant(ButtonVariant::Ghost)
                .size(ButtonSize::Xs)
                .build(t)
                .key(format!("{which}-more"))
                .on_press(move |s: &mut State| s.settings_ui.skills.expanded.push(which)),
        );
    }
    out
}

#[cfg(test)]
#[path = "settings_profiles_tests.rs"]
mod tests;
