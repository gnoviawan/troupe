//! The rail's body (`RailBody`, `ProjectSidebar`): navigation, the Projects section with its
//! groups, project rows and each project's chats, the Quick chats section, and the footer
//! actions. Rows open their context menus on a secondary press; the overlays themselves live in
//! `rail_menus`, `rail_forms`, `theme_picker` and `keys`.

use model::chats::Chat;
use model::projects::{Event as ProjectEvent, Group, GroupUpdates, Project};
use rune::{El, PointerPress, Tx, col, el, icon, row, text};
use theme::Tokens;
use theme::icons as i;
use ui::shell;
use ui::type_::sans;

use crate::rail::{Paint, Rename};
use crate::rail_forms::rename_field;
use crate::{State, ids, keys, rail_menus, theme_picker};

/// How many chats a project row lists under it (`RAIL_PROJECT_CHAT_LIMIT`).
const CHATS_PER_PROJECT: usize = 5;

fn glyph(name: &str) -> &'static rune::Icon {
    theme::icons::named(name).unwrap_or(&i::X)
}

/// The rail's body. `toggle` is false in the phone drawer, which has its own close button.
pub fn body(
    s: &State,
    t: &'static Tokens,
    collapsed: bool,
    toggle: Option<El>,
    layer: &mut Vec<El>,
) -> El {
    let active = s.active_project();
    let has_project = active.is_some();
    let can_canvas = active.is_some_and(|p| p.path.is_some());
    let nav = col()
        .w_full()
        .pt(8.0)
        .px(if collapsed { 0.0 } else { 8.0 })
        .gap(1.0)
        .child(
            shell::nav_row(t, &i::CHAT_EDIT, "New chat", collapsed, !has_project)
                .key("rail-new-chat"),
        )
        .child(
            shell::nav_row(t, &i::SEARCH_LIST, "Search", collapsed, false)
                .key("rail-search")
                .on_press(|s: &mut State| s.toast_later("Search")),
        )
        .child(shell::nav_row(t, &i::CANVAS, "Canvas", collapsed, !can_canvas).key("rail-canvas"))
        .child(
            shell::nav_row(
                t,
                &i::CALENDAR_CLOCK,
                "Automations",
                collapsed,
                !has_project,
            )
            .key("rail-automations"),
        );

    let mut projects = col()
        .w_full()
        .px(if collapsed { 0.0 } else { 8.0 })
        .gap(1.0)
        .role("list")
        .label("Projects");
    let (groups, ungrouped) = layout(&s.projects.store.projects, &s.projects.store.groups);
    let mut grouped = col().w_full().px(8.0).gap(4.0).role("list").label("Groups");
    let has_groups = !groups.is_empty();
    for (group, members) in groups {
        let folded = group.is_collapsed == Some(true);
        if collapsed {
            for project in members {
                projects = projects.children(project_rows(s, t, project, true));
            }
            continue;
        }
        // An open group sits on a faint block that spans its member rows.
        let mut block = col()
            .key(format!("rail-group-block-{}", group.id))
            .w_full()
            .mx(-4.0)
            .px(4.0)
            .gap(1.0)
            .radius(10.0)
            .child(group_row(s, t, group, folded));
        if !folded {
            block = block.pb(4.0).fill(t.colors.row_hover);
            for project in members {
                block = block.children(project_rows(s, t, project, false));
            }
        }
        grouped = grouped.child(block);
    }
    for project in ungrouped {
        projects = projects.children(project_rows(s, t, project, collapsed));
    }

    let mut body = col()
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .child(nav)
        .child(el().h(22.0));
    if collapsed {
        body = body.child(projects);
    } else {
        let add = shell::rail_button(
            t,
            &i::PLUS,
            "Add project or folder group",
            20.0,
            12.0,
            s.overlays.is_open(crate::rail::ADD_MENU),
        )
        .key("rail-add-project");
        let add = rail_menus::add_menu(s, t, layer, add);
        let mut quick = col()
            .w_full()
            .px(8.0)
            .gap(1.0)
            .role("list")
            .label("Quick chats");
        for chat in s.chats.quick().take(CHATS_PER_PROJECT) {
            quick = quick.child(chat_row(s, t, chat));
        }
        if has_groups {
            let new_group = shell::rail_button(
                t,
                glyph("FolderPlus"),
                "New folder group",
                20.0,
                12.0,
                false,
            )
            .key("rail-new-group")
            .on_press(|s: &mut State| s.overlays.open(crate::rail::NEW_GROUP));
            body = body
                .child(shell::section_header(t, "Groups", Some(new_group)))
                .child(el().h(2.0))
                .child(grouped)
                .child(el().h(14.0));
        }
        body = body
            .child(shell::section_header(t, "Projects", Some(add)))
            .child(el().h(2.0))
            .child(projects)
            .child(el().h(12.0))
            .child(shell::section_header(
                t,
                "Quick chats",
                Some(
                    shell::rail_button(t, &i::PLUS, "New quick chat", 20.0, 12.0, false)
                        .key("rail-new-quick-chat")
                        .on_press(|s: &mut State| s.toast_later("New quick chat")),
                ),
            ))
            .child(quick);
    }
    let footer_items = [
        shell::rail_button(t, &i::SLIDERS, "Settings", 28.0, 14.0, false)
            .key("rail-settings")
            .on_press(|s: &mut State| s.open_settings(None)),
        shell::rail_button(t, &i::BUG, "Report a bug", 28.0, 14.0, false)
            .key("rail-report-bug")
            .on_press(|s: &mut State| s.toast_later("Report a bug")),
        shell::rail_button(
            t,
            &i::PALETTE,
            "Color themes",
            28.0,
            14.0,
            s.overlays.is_open(theme_picker::KEY),
        )
        .key("rail-color-themes")
        .on_press(|s: &mut State| s.open_theme_picker()),
        shell::rail_button(
            t,
            &i::KEYBOARD,
            "Open keyboard shortcuts menu",
            28.0,
            14.0,
            s.overlays.is_open(keys::MENU),
        )
        .key("rail-shortcuts")
        .on_press(|s: &mut State| s.overlays.open(keys::MENU)),
    ];
    let footer = if collapsed {
        col()
            .w_full()
            .items_center()
            .gap(6.0)
            .pb(10.0)
            .children(footer_items)
    } else {
        row()
            .w_full()
            .px(14.0)
            .gap(6.0)
            .pb(10.0)
            .children(footer_items)
    };
    col()
        .w_full()
        .h_full()
        .child(shell::rail_header(t, collapsed, toggle))
        .child(body.scroll(rune::ScrollSpec {
            stick_bottom: false,
        }))
        .child(footer)
}

/// `layoutRailProjects`: groups in group order with their open projects, then the open
/// projects no group holds.
pub fn layout<'a>(
    projects: &'a [Project],
    groups: &'a [Group],
) -> (Vec<(&'a Group, Vec<&'a Project>)>, Vec<&'a Project>) {
    let open = || projects.iter().filter(|p| p.is_archived != Some(true));
    let laid = groups
        .iter()
        .map(|g| {
            (
                g,
                g.project_ids
                    .iter()
                    .filter_map(|id| open().find(|p| p.id == *id))
                    .collect(),
            )
        })
        .collect();
    let ungrouped = open()
        .filter(|p| !groups.iter().any(|g| g.project_ids.contains(&p.id)))
        .collect();
    (laid, ungrouped)
}

fn renaming<'a>(s: &'a State, target: &Paint) -> Option<&'a Rename> {
    s.rail.rename.as_ref().filter(|r| r.target == *target)
}

/// A group row (`RailGroupRow`): the folder in the group's colour (or the chevron when open),
/// the name, and a press that folds it.
fn group_row(s: &State, t: &'static Tokens, group: &Group, folded: bool) -> El {
    let c = &t.colors;
    let lead = if folded {
        let ink = group
            .color
            .as_deref()
            .map_or(c.muted_foreground, |n| shell::project_color(t, n));
        icon(glyph("Folder"), 14.0).color(ink)
    } else {
        icon(glyph("ChevronDown"), 12.0).color(c.muted_foreground)
    };
    let id = group.id.clone();
    let e = row()
        .key(format!("rail-group-{}", group.id))
        .role("button")
        .label(group.name.clone())
        .expanded(!folded)
        .w_full()
        .h(32.0)
        .px(12.0)
        .gap(8.0)
        .items_center()
        .radius(8.0)
        .hover(c.row_hover)
        .child(row().w(16.0).h(16.0).center().a11y_hidden().child(lead));
    if let Some(r) = renaming(s, &Paint::Group(group.id.clone())) {
        return e.child(rename_field(t, &r.edit));
    }
    let menu_id = id.clone();
    e.child(
        text(group.name.clone())
            .font(sans(12.0, 500.0, 16.0))
            .color(c.foreground)
            .single_line()
            .truncate()
            .min_w(0.0)
            .grow(1.0),
    )
    .on_press(move |s: &mut State| {
        s.group_step(ProjectEvent::UpdateGroup(
            id.clone(),
            GroupUpdates {
                is_collapsed: Some(!folded),
                ..Default::default()
            },
        ));
    })
    .on_secondary_press(move |s: &mut State, at: PointerPress, _tx: &mut Tx| {
        s.open_group_menu(&menu_id, at)
    })
}

/// A project row and, expanded, its newest chats under it.
fn project_rows(s: &State, t: &'static Tokens, project: &Project, collapsed: bool) -> Vec<El> {
    let is_active = s.active_project().is_some_and(|a| a.id == project.id);
    let id = project.id.clone();
    let menu_id = project.id.clone();
    let key = format!("project-item-{}", project.id);
    let row_el = match renaming(s, &Paint::Project(project.id.clone())).filter(|_| !collapsed) {
        Some(r) => row()
            .key(key)
            .w_full()
            .h(30.0)
            .px(12.0)
            .gap(8.0)
            .items_center()
            .child(shell::project_badge(t, &project.name, &project.color))
            .child(rename_field(t, &r.edit)),
        None => {
            let trailing = is_active.then(|| {
                shell::rail_button(
                    t,
                    &i::PLUS,
                    &format!("New chat in {}", project.name),
                    20.0,
                    12.0,
                    false,
                )
            });
            shell::project_row(
                t,
                &project.name,
                &project.color,
                is_active,
                collapsed,
                trailing,
            )
            .key(key)
            .on_press(move |s: &mut State| s.select_project(&id))
            .on_secondary_press(
                move |s: &mut State, at: PointerPress, _tx: &mut Tx| {
                    s.open_project_menu(&menu_id, at)
                },
            )
        }
    };
    let mut out = vec![row_el];
    if !collapsed {
        out.extend(
            s.chats
                .of_project(&project.id)
                .take(CHATS_PER_PROJECT)
                .map(|chat| chat_row(s, t, chat)),
        );
        if out.len() > 1 {
            // The chat list's `pb-1` under its last row.
            out.push(el().h(3.0));
        }
    }
    out
}

/// A chat row (`ProjectChatRow`): a 12 px bot glyph on the label column, the title, and its
/// age; hovered, a "More actions" button takes the age's place.
fn chat_row(s: &State, t: &'static Tokens, chat: &Chat) -> El {
    let c = &t.colors;
    let hovered = s.rail.hovered_chat.as_deref() == Some(chat.session_id.as_str())
        || s.rail.chat.as_deref() == Some(&chat.session_id);
    let (hover_id, menu_id, more_id, open_id) = (
        chat.session_id.clone(),
        chat.session_id.clone(),
        chat.session_id.clone(),
        chat.session_id.clone(),
    );
    let menu_open = s.overlays.is_open(crate::rail::CHAT_MENU)
        && s.rail.chat.as_deref() == Some(&chat.session_id);
    // The "More actions" button is always there for keyboards and screen readers; it shows,
    // in place of the age, while the row is hovered or its menu is open.
    let more = row()
        .key(if menu_open && s.rail.at.is_none() {
            "chat-menu-trigger"
        } else {
            "more"
        })
        .role("button")
        .label(format!("More actions for {}", chat.label()))
        .size(20.0, 20.0)
        .center()
        .radius(4.0)
        .hover(c.row_hover)
        .opacity(if hovered { 1.0 } else { 0.0 })
        .child(icon(glyph("MoreHorizontal"), 14.0).color(c.muted_foreground))
        .on_press(move |s: &mut State| s.open_chat_dropdown(&more_id));
    let trailing = if hovered {
        more
    } else {
        el().child(
            text(ids::age(ids::now_ms(), chat.last_activity_at))
                .font(sans(12.0, 400.0, 16.0))
                .color(c.muted_foreground),
        )
        .child(more.absolute(0.0, -2.0))
    };
    row()
        .key(format!("rail-chat-{}", chat.session_id))
        .role("button")
        .label(chat.label().to_owned())
        .w_full()
        .h(30.0)
        .pl(34.0)
        .pr(12.0)
        .gap(10.0)
        .items_center()
        .radius(8.0)
        .hover(c.row_hover)
        .child(
            icon(glyph("Bot"), 12.0)
                .color(c.muted_foreground)
                .a11y_hidden(),
        )
        .child(
            text(chat.label().to_owned())
                .font(sans(12.0, 400.0, 16.0))
                .color(c.secondary_foreground)
                .single_line()
                .truncate()
                .min_w(0.0)
                .grow(1.0),
        )
        .child(trailing)
        .on_hover_change(move |s: &mut State, on: bool, _tx: &mut Tx| {
            if on {
                s.rail.hovered_chat = Some(hover_id.clone());
            } else if s.rail.hovered_chat.as_deref() == Some(&hover_id) {
                s.rail.hovered_chat = None;
            }
        })
        .on_press(move |s: &mut State| s.open_chat(&open_id))
        .on_secondary_press(move |s: &mut State, at: PointerPress, _tx: &mut Tx| {
            s.open_chat_menu(&menu_id, at)
        })
}
