//! The rail's menus and confirms: the project row's context menu with its "Move to group"
//! submenu, the colour swatches, the Projects header's `+` menu, the group row's menu, the chat
//! row's "More actions" menu, and the delete confirms (`ProjectContextMenuContent`,
//! `ColorPickerPopover`, `AddProjectMenu`, `GroupContextMenuContent`, `RailChatMenu`,
//! `ProjectSidebar`'s `ConfirmDialog`s, `ChatDeleteConfirmDialog`).

use model::projects::{Event as ProjectEvent, GroupUpdates};
use rune::{El, Tx, col, el, row, text};
use serde_json::json;
use theme::Tokens;
use troupe_protocol::ws::WsRequestType;
use ui::overlay::{ConfirmDialog, ConfirmVariant, Dialog, DialogKind, Menu, MenuItem, at_point};
use ui::type_::sans;
use ui::{Button, ButtonVariant};

use crate::State;
use crate::rail::*;

fn glyph(name: &str) -> &'static rune::Icon {
    theme::icons::named(name).unwrap_or(&theme::icons::X)
}

/// Every rail overlay that is open, pushed onto `layer`.
pub fn build(s: &State, t: &'static Tokens, layer: &mut Vec<El>) {
    project_menu(s, t, layer);
    group_menu(s, t, layer);
    chat_menu(s, t, layer);
    colour_picker(s, t, layer);
    confirms(s, t, layer);
}

fn project_menu(s: &State, t: &Tokens, layer: &mut Vec<El>) {
    let Some(project) = s
        .rail
        .project
        .as_ref()
        .and_then(|id| s.projects.store.projects.iter().find(|p| p.id == *id))
    else {
        return;
    };
    if !s.overlays.is_open(PROJECT_MENU) {
        return;
    }
    let id = project.id.clone();
    let git = project.client.is_git_repo.unwrap_or(false);
    let groups = &s.projects.store.groups;
    let current = groups
        .iter()
        .find(|g| g.project_ids.contains(&id))
        .map(|g| g.id.clone());
    let mut moves = Menu::new(MOVE_MENU)
        .width(192.0)
        .open(s.overlays.is_open(MOVE_MENU))
        .item({
            let id = id.clone();
            MenuItem::new("No group (root)")
                .checked(current.is_none())
                .on_select(move |s: &mut State| {
                    s.group_step(ProjectEvent::MoveProjectToGroup(id.clone(), None, None));
                    s.close(PROJECT_MENU);
                })
        });
    for group in groups {
        let (id, gid) = (id.clone(), group.id.clone());
        moves = moves.item(
            MenuItem::new(group.name.clone())
                .checked(current.as_deref() == Some(&group.id))
                .on_select(move |s: &mut State| {
                    s.group_step(ProjectEvent::MoveProjectToGroup(
                        id.clone(),
                        Some(gid.clone()),
                        None,
                    ));
                    s.close(PROJECT_MENU);
                }),
        );
    }
    let for_project = id.clone();
    moves = moves.separator().item(
        MenuItem::new("Create new group…")
            .icon(glyph("FolderPlus"))
            .on_select(move |s: &mut State| {
                s.rail.group_for = Some(for_project.clone());
                s.open_from_menu(PROJECT_MENU, NEW_GROUP);
            }),
    );
    let (rename_id, colour_id, archive_id, settings_name) =
        (id.clone(), id.clone(), id.clone(), project.name.clone());
    let menu =
        Menu::new(PROJECT_MENU)
            .width(224.0)
            .at(s.rail.at)
            .item(MenuItem::new("Settings").icon(glyph("Settings")).on_select(
                move |s: &mut State| {
                    s.close(PROJECT_MENU);
                    s.toast_later(&format!("Settings for {settings_name}"));
                },
            ))
            .item(MenuItem::new("Rename").icon(glyph("Edit")).on_select_tx(
                move |s: &mut State, tx: &mut Tx| {
                    s.close(PROJECT_MENU);
                    s.start_rename(Paint::Project(rename_id.clone()), tx);
                },
            ))
            .item(
                MenuItem::new("Change color")
                    .icon(glyph("Palette"))
                    .on_select(move |s: &mut State| {
                        s.rail.paint = Some(Paint::Project(colour_id.clone()));
                        s.open_from_menu(PROJECT_MENU, COLOUR);
                    }),
            )
            .item(
                MenuItem::new("Default shell")
                    .icon(glyph("Terminal"))
                    .submenu(
                        Menu::new(SHELL_MENU)
                            .width(192.0)
                            .open(s.overlays.is_open(SHELL_MENU))
                            .item(MenuItem::new("System default").checked(true).disabled(true)),
                    )
                    .on_select(|s: &mut State| s.overlays.open(SHELL_MENU)),
            )
            .item(
                MenuItem::new("Move to group")
                    .icon(glyph("Folder"))
                    .submenu(moves)
                    .on_select(|s: &mut State| s.overlays.open(MOVE_MENU)),
            )
            .separator()
            .item(
                MenuItem::new("Switch worktree…")
                    .icon(glyph("GitBranch"))
                    .disabled(!git)
                    .on_select(|s: &mut State| {
                        s.close(PROJECT_MENU);
                        s.toast_later("The worktree switcher");
                    }),
            )
            .item(
                MenuItem::new(if git {
                    "New worktree"
                } else {
                    "New worktree (no git repo)"
                })
                .icon(glyph("GitBranch"))
                .disabled(!git)
                .on_select(|s: &mut State| {
                    s.close(PROJECT_MENU);
                    s.toast_later("New worktree");
                }),
            )
            .item(MenuItem::new("Archive").icon(glyph("Archive")).on_select(
                move |s: &mut State| {
                    s.close(PROJECT_MENU);
                    s.host_command(
                        WsRequestType::UpdateProject,
                        json!({ "projectId": archive_id, "isArchived": true }),
                        "Failed to archive the project",
                    );
                },
            ))
            .item(
                MenuItem::new("Delete")
                    .icon(glyph("Trash2"))
                    .destructive()
                    .on_select(|s: &mut State| {
                        s.open_from_menu(PROJECT_MENU, DELETE_PROJECT);
                    }),
            )
            .on_close(|s: &mut State| s.close(PROJECT_MENU));
    let _ = menu.build(t, layer, el());
}

fn group_menu(s: &State, t: &Tokens, layer: &mut Vec<El>) {
    let Some((gid, _)) = &s.rail.group else {
        return;
    };
    if !s.overlays.is_open(GROUP_MENU) {
        return;
    }
    let (rename, colour, keep, all) = (gid.clone(), gid.clone(), gid.clone(), gid.clone());
    let menu = Menu::new(GROUP_MENU)
        .width(224.0)
        .at(s.rail.at)
        .item(
            MenuItem::new("Rename group")
                .icon(glyph("Edit"))
                .on_select_tx(move |s: &mut State, tx: &mut Tx| {
                    s.close(GROUP_MENU);
                    s.start_rename(Paint::Group(rename.clone()), tx);
                }),
        )
        .item(
            MenuItem::new("Change color")
                .icon(glyph("Palette"))
                .on_select(move |s: &mut State| {
                    s.rail.paint = Some(Paint::Group(colour.clone()));
                    s.open_from_menu(GROUP_MENU, COLOUR);
                }),
        )
        .separator()
        .item(
            MenuItem::new("Delete group (keep projects)")
                .icon(glyph("Trash2"))
                .on_select(move |s: &mut State| {
                    s.open_from_menu(GROUP_MENU, DELETE_GROUP);
                    s.rail.group = Some((keep.clone(), false));
                }),
        )
        .item(
            MenuItem::new("Delete group and all projects")
                .icon(glyph("Trash2"))
                .destructive()
                .on_select(move |s: &mut State| {
                    s.open_from_menu(GROUP_MENU, DELETE_GROUP);
                    s.rail.group = Some((all.clone(), true));
                }),
        )
        .on_close(|s: &mut State| s.close(GROUP_MENU));
    let _ = menu.build(t, layer, el());
}

/// The chat row's menu, opened from its "More actions" button (a dropdown under it) or a
/// secondary press (at the pointer).
fn chat_menu(s: &State, t: &Tokens, layer: &mut Vec<El>) {
    let Some(chat) = s.rail.chat.as_ref().and_then(|id| s.chats.get(id)) else {
        return;
    };
    if !s.overlays.is_open(CHAT_MENU) {
        return;
    }
    let reference = chat_reference(s, &chat.session_id);
    let (id_copy, id) = (chat.session_id.clone(), chat.session_id.clone());
    let menu = Menu::new(CHAT_MENU)
        .width(224.0)
        .at(s.rail.at)
        .placement(ui::overlay::Placement::BottomEnd)
        .open(true)
        .item(
            MenuItem::new("Reload this chat")
                .icon(glyph("RotateCcw"))
                .on_select(|s: &mut State| {
                    s.close(CHAT_MENU);
                    s.toast_later("Reload this chat");
                }),
        )
        .item(
            MenuItem::new("Open in split")
                .icon(glyph("Columns2"))
                .on_select(|s: &mut State| {
                    s.close(CHAT_MENU);
                    s.toast_later("Open in split");
                }),
        )
        .separator()
        .item(
            MenuItem::new("Open terminal here")
                .icon(glyph("Terminal"))
                .on_select(|s: &mut State| {
                    s.close(CHAT_MENU);
                    s.toast_later("Open terminal here");
                }),
        )
        .separator()
        .item(
            MenuItem::new("Copy chat reference")
                .icon(glyph("Copy"))
                .on_select_tx(move |s: &mut State, tx: &mut Tx| {
                    tx.copy(reference.clone());
                    s.close(CHAT_MENU);
                    s.toasts.push(
                        s.now,
                        ui::overlay::ToastKind::Success,
                        "Chat reference copied",
                        Some(&id),
                    );
                }),
        )
        .item(
            MenuItem::new("Copy chat ID")
                .icon(glyph("Hash"))
                .on_select_tx(move |s: &mut State, tx: &mut Tx| {
                    tx.copy(id_copy.clone());
                    s.close(CHAT_MENU);
                    s.toasts.push(
                        s.now,
                        ui::overlay::ToastKind::Success,
                        "Chat ID copied",
                        Some(&id_copy),
                    );
                }),
        )
        .separator()
        .item(
            MenuItem::new("Delete chat")
                .icon(glyph("Trash2"))
                .destructive()
                .on_select(|s: &mut State| {
                    let chat = s.rail.chat.clone();
                    s.open_from_menu(CHAT_MENU, DELETE_CHAT);
                    s.rail.chat = chat;
                }),
        )
        .on_close(|s: &mut State| s.close(CHAT_MENU));
    let _ = menu.build(t, layer, el());
}

/// `formatChatReference`: a labeled plain-text block another agent can read the chat from.
pub fn chat_reference(s: &State, session_id: &str) -> String {
    let Some(chat) = s.chats.get(session_id) else {
        return session_id.to_owned();
    };
    let title = chat
        .label()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let mut lines = vec![
        format!("Troupe chat: {title}"),
        format!("Chat ID: {session_id}"),
    ];
    if let Some(pid) = chat.project_id.as_deref().filter(|p| !p.is_empty()) {
        match s.projects.store.projects.iter().find(|p| p.id == pid) {
            Some(p) => lines.push(format!("Project: {} (ID {pid})", p.name)),
            None => lines.push(format!("Project: ID {pid}")),
        }
    }
    lines.push("Read it with the Troupe read_chat tool.".to_owned());
    lines.join("\n")
}

/// `ColorPickerPopover`: "Select Color" over nine 24 px swatches on a `card` panel, at the
/// context menu's point; the current colour wears a 2 px ring.
fn colour_picker(s: &State, t: &Tokens, layer: &mut Vec<El>) {
    let (Some(paint), Some(at)) = (&s.rail.paint, s.rail.at) else {
        return;
    };
    if !s.overlays.is_open(COLOUR) {
        return;
    }
    let c = &t.colors;
    let current = match paint {
        Paint::Project(id) => s
            .projects
            .store
            .projects
            .iter()
            .find(|p| p.id == *id)
            .map(|p| p.color.clone()),
        Paint::Group(id) => s
            .projects
            .store
            .groups
            .iter()
            .find(|g| g.id == *id)
            .and_then(|g| g.color.clone()),
    }
    .unwrap_or_else(|| "blue".to_owned());
    let swatches = COLOURS.map(|colour| {
        let fill = ui::shell::project_color(t, colour);
        let mut dot = el()
            .key(format!("swatch-{colour}"))
            .role("button")
            .label(colour)
            .size(24.0, 24.0)
            .radius(12.0)
            .fill(fill);
        if current == colour {
            dot = dot.child(
                el().key("ring")
                    .absolute(-4.0, -4.0)
                    .size(32.0, 32.0)
                    .radius(16.0)
                    .border(2.0, fill)
                    .a11y_hidden(),
            );
        }
        let paint = paint.clone();
        dot.on_press(move |s: &mut State| {
            s.recolour(&paint, colour);
            s.close(COLOUR);
        })
    });
    let panel = col()
        .key("colour-picker-panel")
        .role("dialog")
        .label("Select Color")
        .padding(13.0)
        .radius(8.0)
        .fill(c.card)
        .border(1.0, c.border)
        .child(
            text("Select Color")
                .font(sans(12.0, 400.0, 16.0))
                .color(c.muted_foreground)
                .mb(8.0),
        )
        .child(row().gap(8.0).children(swatches))
        .modal(true, |s: &mut State| s.close(COLOUR));
    at_point(
        COLOUR,
        at,
        ui::paint::shadows(panel, &t.shadows.elevation_lg),
        layer,
    );
}

fn confirms(s: &State, t: &Tokens, layer: &mut Vec<El>) {
    let project = s
        .rail
        .project
        .as_ref()
        .and_then(|id| s.projects.store.projects.iter().find(|p| p.id == *id));
    if let Some(project) = project.filter(|_| s.overlays.is_open(DELETE_PROJECT)) {
        let id = project.id.clone();
        ConfirmDialog::new(
            DELETE_PROJECT,
            "Delete Project",
            format!(
                "Are you sure you want to delete \"{}\"? This action cannot be undone.",
                project.name
            ),
        )
        .variant(ConfirmVariant::Danger)
        .confirm_label("Delete")
        .open(true)
        .on_cancel(|s: &mut State| s.close(DELETE_PROJECT))
        .on_confirm(move |s: &mut State| {
            s.close(DELETE_PROJECT);
            s.host_command(
                WsRequestType::RemoveProject,
                json!({ "projectId": id }),
                "Failed to delete the project",
            );
            s.projects_step(ProjectEvent::ForgetProjectPrefs(id.clone()));
        })
        .build(t, layer);
    }
    let group =
        s.rail.group.as_ref().and_then(|(id, all)| {
            Some((s.projects.store.groups.iter().find(|g| g.id == *id)?, *all))
        });
    if let Some((group, all)) = group.filter(|_| s.overlays.is_open(DELETE_GROUP)) {
        let message = if all {
            format!(
                "Are you sure you want to delete the group folder \"{}\" and all projects inside it? This action cannot be undone.",
                group.name
            )
        } else {
            format!(
                "Are you sure you want to delete the group folder \"{}\"? Projects inside this group will be moved to the root folder list.",
                group.name
            )
        };
        let id = group.id.clone();
        ConfirmDialog::new(DELETE_GROUP, "Delete Group Folder", message)
            .variant(ConfirmVariant::Danger)
            .confirm_label("Delete")
            .open(true)
            .on_cancel(|s: &mut State| s.close(DELETE_GROUP))
            .on_confirm(move |s: &mut State| {
                s.close(DELETE_GROUP);
                s.group_step(ProjectEvent::RemoveGroup(id.clone(), all));
            })
            .build(t, layer);
    }
    let chat = s.rail.chat.as_ref().and_then(|id| s.chats.get(id));
    if let Some(chat) = chat.filter(|_| s.overlays.is_open(DELETE_CHAT)) {
        let id = chat.session_id.clone();
        let cancel = Button::new("Cancel")
            .variant(ButtonVariant::Outline)
            .build(t)
            .key("cancel")
            .on_press(|s: &mut State| s.close(DELETE_CHAT));
        let delete = Button::new("Delete")
            .variant(ButtonVariant::Destructive)
            .build(t)
            .key("confirm")
            .on_press(move |s: &mut State| {
                s.close(DELETE_CHAT);
                s.chats.remove(&id);
                s.host_command(
                    WsRequestType::DeleteSession,
                    json!({ "sessionId": id }),
                    "Failed to delete the chat",
                );
            });
        Dialog::new(DELETE_CHAT, "Delete chat")
            .kind(DialogKind::Alert)
            .description(format!(
                "Delete “{}”? This action cannot be undone.",
                chat.label()
            ))
            .footer(cancel)
            .footer(delete)
            .open(true)
            .on_close(|s: &mut State| s.close(DELETE_CHAT))
            .build(t, layer);
    }
}

/// The `+` in the Projects header and its menu (`AddProjectMenu`), anchored under it.
pub fn add_menu(s: &State, t: &Tokens, layer: &mut Vec<El>, trigger: El) -> El {
    let import = s.active_project().is_some();
    let mut menu = Menu::new(ADD_MENU)
        .width(145.0)
        .placement(ui::overlay::Placement::BottomEnd)
        .open(s.overlays.is_open(ADD_MENU))
        .item(
            MenuItem::new("New project")
                .icon(glyph("Plus"))
                .on_select(|s: &mut State| s.open_from_menu(ADD_MENU, NEW_PROJECT)),
        )
        .item(
            MenuItem::new("New folder group")
                .icon(glyph("FolderPlus"))
                .on_select(|s: &mut State| s.open_from_menu(ADD_MENU, NEW_GROUP)),
        );
    if import {
        menu = menu.separator().item(
            MenuItem::new("Import chats")
                .icon(glyph("Download"))
                .on_select(|s: &mut State| {
                    s.close(ADD_MENU);
                    s.toast_later("Import chats");
                }),
        );
    }
    menu.on_close(|s: &mut State| s.close(ADD_MENU)).build(
        t,
        layer,
        trigger.on_press(|s: &mut State| s.overlays.open(ADD_MENU)),
    )
}

impl State {
    fn recolour(&mut self, paint: &Paint, colour: &str) {
        match paint {
            Paint::Project(id) => self.projects_step(ProjectEvent::UpdateProject(
                id.clone(),
                model::projects::ProjectUpdates {
                    color: Some(colour.to_owned()),
                    ..Default::default()
                },
            )),
            Paint::Group(id) => self.group_step(ProjectEvent::UpdateGroup(
                id.clone(),
                GroupUpdates {
                    color: Some(colour.to_owned()),
                    ..Default::default()
                },
            )),
        }
    }

    /// Opens a project row's context menu at the pointer.
    pub fn open_project_menu(&mut self, id: &str, at: rune::PointerPress) {
        self.rail.project = Some(id.to_owned());
        self.rail.at = Some(at);
        self.overlays.open(PROJECT_MENU);
    }

    pub fn open_group_menu(&mut self, id: &str, at: rune::PointerPress) {
        self.rail.group = Some((id.to_owned(), false));
        self.rail.at = Some(at);
        self.overlays.open(GROUP_MENU);
    }

    /// Opens a chat row's menu under its "More actions" button.
    pub fn open_chat_dropdown(&mut self, id: &str) {
        self.rail.chat = Some(id.to_owned());
        self.rail.at = None;
        self.overlays.open(CHAT_MENU);
    }

    /// Opens a chat row's menu at a secondary press.
    pub fn open_chat_menu(&mut self, id: &str, at: rune::PointerPress) {
        self.rail.chat = Some(id.to_owned());
        self.rail.at = Some(at);
        self.overlays.open(CHAT_MENU);
    }
}

#[cfg(test)]
#[path = "rail_menus_tests.rs"]
mod tests;
