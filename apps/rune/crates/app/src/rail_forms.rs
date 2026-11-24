//! The rail's forms: the new project and new group modals (`NewProjectModal`,
//! `NewGroupModal`) and the inline rename field a project or group row turns into.

use model::projects::{Event as ProjectEvent, ProjectUpdates};
use rune::{El, Key, KeyEvent, TextEdit, Tx, col, icon, row, text, text_input};
use theme::Tokens;
use ui::overlay::Modal;
use ui::type_::sans;
use ui::{Button, ButtonSize, ButtonVariant};

use crate::State;
use crate::rail::*;

/// The key of the inline rename field.
pub const RENAME_FIELD: &str = "rail-rename-field";

/// The modals' text field (`bg-secondary border rounded px-3 py-1.5 text-sm`).
fn field(t: &Tokens, edit: &TextEdit, placeholder: &str, label: &str) -> El {
    let c = &t.colors;
    text_input(edit, sans(14.0, 400.0, 20.0))
        .single_line()
        .label(label.to_owned())
        .placeholder(placeholder.to_owned(), c.muted_foreground)
        .color(c.foreground)
        .w_full()
        .px(12.0)
        .py(7.0)
        .radius(4.0)
        .fill(c.secondary)
        .border(1.0, c.border)
        .focus_ring(1.0, 0.0, c.focus_ring)
}

/// A field's caption (`block text-xs font-medium text-muted-foreground mb-1`).
fn caption(t: &Tokens, label: &str) -> El {
    text(label.to_owned())
        .font(sans(12.0, 500.0, 16.0))
        .color(t.colors.muted_foreground)
        .mb(4.0)
}

pub fn build(s: &State, t: &'static Tokens, layer: &mut Vec<El>) {
    new_project(s, t, layer);
    new_group(s, t, layer);
}

/// The folder name at the end of a path: the default project name.
fn basename(path: &str) -> &str {
    path.trim_end_matches(['/', '\\'])
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or_default()
}

fn new_project(s: &State, t: &'static Tokens, layer: &mut Vec<El>) {
    if !s.overlays.is_open(NEW_PROJECT) {
        return;
    }
    let c = &t.colors;
    let r = &s.rail;
    let can_create = !r.project_path.text.trim().is_empty();
    let browse = row()
        .key("browse")
        .role("button")
        .label("Browse")
        .h(34.0)
        .px(12.0)
        .items_center()
        .radius(4.0)
        .fill(c.secondary)
        .border(1.0, c.border)
        .child(
            text("Browse")
                .font(sans(12.0, 400.0, 16.0))
                .color(c.foreground),
        )
        .on_press(|s: &mut State| s.toast_later("Browsing the server's folders"));
    let path = row()
        .key("root-row")
        .w_full()
        .gap(8.0)
        .child(
            field(
                t,
                &r.project_path,
                "No directory selected",
                "Root Directory",
            )
            .key("project-path")
            .grow(1.0)
            .autofocus()
            .on_edit(|s: &mut State| &mut s.rail.project_path)
            .on_submit(|s: &mut State, _tx: &mut Tx| s.create_project()),
        )
        .child(browse);
    let name = field(t, &r.project_name, "My Project", "Project Name")
        .key("project-name")
        .on_edit(|s: &mut State| &mut s.rail.project_name)
        .on_submit(|s: &mut State, _tx: &mut Tx| s.create_project());
    let chevron = theme::icons::named("ChevronRight").expect("bundled icon");
    let advanced = row()
        .key("advanced")
        .role("button")
        .label("Advanced options")
        .gap(4.0)
        .items_center()
        .child(icon(chevron, 12.0).color(c.muted_foreground))
        .child(
            text("Advanced options")
                .font(sans(12.0, 500.0, 16.0))
                .color(c.muted_foreground),
        )
        .on_press(|s: &mut State| s.toast_later("Advanced project options"));
    let note = text(
        "On the web client, this project is saved on the server. It persists across server restarts when the server is configured with a projects registry file; on a memory-only server it is lost when the server restarts.",
    )
    .font(sans(12.0, 400.0, 19.5))
    .color(c.muted_foreground)
    .wrap()
    .w_full()
    .padding(12.0)
    .radius(6.0)
    .fill(c.secondary.alpha(0.5));
    let cancel = Button::new("Cancel")
        .variant(ButtonVariant::Ghost)
        .size(ButtonSize::Sm)
        .build(t)
        .key("cancel")
        .on_press(|s: &mut State| s.close(NEW_PROJECT));
    let create = Button::new("Create")
        .size(ButtonSize::Sm)
        .disabled(!can_create)
        .build(t)
        .key("create")
        .on_press(|s: &mut State| s.create_project());
    Modal::new(NEW_PROJECT, "Create New Project")
        .width(520.0)
        .child(
            col()
                .w_full()
                .child(caption(t, "Root Directory"))
                .child(path),
        )
        .child(col().w_full().child(caption(t, "Project Name")).child(name))
        .child(advanced)
        .child(note)
        .footer(cancel)
        .footer(create)
        .open(true)
        .on_close(|s: &mut State| s.close(NEW_PROJECT))
        .build(t, layer);
}

fn new_group(s: &State, t: &'static Tokens, layer: &mut Vec<El>) {
    if !s.overlays.is_open(NEW_GROUP) {
        return;
    }
    let can_create = !s.rail.group_name.text.trim().is_empty();
    let name = field(
        t,
        &s.rail.group_name,
        "Enter group name...",
        "Group Folder Name",
    )
    .key("group-name")
    .autofocus()
    .on_edit(|s: &mut State| &mut s.rail.group_name)
    .on_submit(|s: &mut State, _tx: &mut Tx| s.create_group());
    let cancel = Button::new("Cancel")
        .variant(ButtonVariant::Ghost)
        .size(ButtonSize::Sm)
        .build(t)
        .key("cancel")
        .on_press(|s: &mut State| s.close(NEW_GROUP));
    let create = Button::new("Create")
        .size(ButtonSize::Sm)
        .disabled(!can_create)
        .build(t)
        .key("create")
        .on_press(|s: &mut State| s.create_group());
    Modal::new(NEW_GROUP, "Create New Group")
        .child(
            col()
                .w_full()
                .child(caption(t, "Group Folder Name"))
                .child(name),
        )
        .footer(cancel)
        .footer(create)
        .open(true)
        .on_close(|s: &mut State| s.close(NEW_GROUP))
        .build(t, layer);
}

/// The inline rename field a row shows in place of its name: Return saves, Escape cancels.
pub fn rename_field(t: &Tokens, edit: &TextEdit) -> El {
    let c = &t.colors;
    text_input(edit, sans(12.0, 400.0, 16.0))
        .single_line()
        .key(RENAME_FIELD)
        .label("Rename")
        .color(c.foreground)
        .grow(1.0)
        .min_w(0.0)
        .h(24.0)
        .px(6.0)
        .radius(4.0)
        .fill(c.card)
        .border(1.0, c.border)
        .on_edit(|s: &mut State| {
            &mut s
                .rail
                .rename
                .get_or_insert_with(|| Rename {
                    target: Paint::Project(String::new()),
                    edit: TextEdit::new(""),
                })
                .edit
        })
        .on_submit(|s: &mut State, _tx: &mut Tx| s.save_rename())
        .on_key(|s: &mut State, key: &KeyEvent, _tx: &mut Tx| {
            if matches!(key.key, Key::Escape) {
                s.rail.rename = None;
                return true;
            }
            false
        })
}

impl State {
    fn create_project(&mut self) {
        let path = self.rail.project_path.text.trim().to_owned();
        if path.is_empty() {
            return;
        }
        let typed = self.rail.project_name.text.trim().to_owned();
        let name = if typed.is_empty() {
            basename(&path).to_owned()
        } else {
            typed
        };
        let colour = self.rail.project_colour.unwrap_or("blue");
        self.add_project(&name, &path, colour);
        self.close(NEW_PROJECT);
    }

    fn create_group(&mut self) {
        let name = self.rail.group_name.text.trim().to_owned();
        if name.is_empty() {
            return;
        }
        let project = self.rail.group_for.take();
        let out = model::projects::step(
            &mut self.projects,
            ProjectEvent::AddGroup(name),
            &mut crate::sync::Uuid,
        );
        if let (Some(project), Some(id)) = (project, out.reply.as_ref().and_then(|v| v.as_str())) {
            self.projects_step(ProjectEvent::MoveProjectToGroup(
                project,
                Some(id.to_owned()),
                None,
            ));
        }
        self.projects_step(ProjectEvent::PersistProjectPrefs);
        self.close(NEW_GROUP);
    }

    /// Turns a row's name into the rename field, holding the current name.
    pub fn start_rename(&mut self, target: Paint, tx: &mut Tx) {
        let name = match &target {
            Paint::Project(id) => self
                .projects
                .store
                .projects
                .iter()
                .find(|p| p.id == *id)
                .map(|p| p.name.clone()),
            Paint::Group(id) => self
                .projects
                .store
                .groups
                .iter()
                .find(|g| g.id == *id)
                .map(|g| g.name.clone()),
        };
        let Some(name) = name else { return };
        let mut edit = TextEdit::new(&name);
        edit.anchor = 0;
        edit.focus = name.len();
        self.rail.rename = Some(Rename { target, edit });
        tx.focus_when_shown(RENAME_FIELD, false);
    }

    fn save_rename(&mut self) {
        let Some(Rename { target, edit }) = self.rail.rename.take() else {
            return;
        };
        let name = edit.text.trim().to_owned();
        if name.is_empty() {
            return;
        }
        match target {
            Paint::Project(id) => self.projects_step(ProjectEvent::UpdateProject(
                id,
                ProjectUpdates {
                    name: Some(name),
                    ..Default::default()
                },
            )),
            Paint::Group(id) => self.group_step(ProjectEvent::RenameGroup(id, name)),
        }
    }
}
