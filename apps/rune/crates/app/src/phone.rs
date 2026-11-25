//! The phone shell (`MobileChatShell`): the header with the drawer button, the title and the
//! project line, New chat and More; the full-screen drawer (`MobileShellDrawer`) with its view
//! tabs, chat search, recents and footer; and the bottom sheets the header and the recents open
//! (`MobileHeaderMoreSheet`, the project sheet, `MobileRecentsActionsSheet`). Each sheet and the
//! drawer is one key on the back stack, so system back closes one per press.

use model::chats::Chat;
use rune::{El, TextEdit, col, el, icon, row, text, text_input};
use theme::Tokens;
use ui::mobile::{self, TOUCH};
use ui::overlay::{MobileSheet, SheetDrag};
use ui::type_::sans;

use crate::{DRAWER, State, ids, rail, rail_menus};

/// The header's ⋯ sheet.
pub const MORE: &str = "phone-more";
/// The project sheet the header's project line opens.
pub const PROJECTS: &str = "phone-projects";
/// A recents row's ⋯ sheet.
pub const RECENTS: &str = "phone-recents";

#[derive(Default)]
pub struct PhoneUi {
    pub drag: SheetDrag,
    /// The drawer's chat search.
    pub query: TextEdit,
    /// The chat a recents sheet acts on.
    pub chat: Option<String>,
}

fn glyph(name: &str) -> &'static rune::Icon {
    theme::icons::named(name).unwrap_or(&theme::icons::X)
}

/// The branch a project shows: its git branch, or `main` as the web client prints for a
/// checkout whose branch it has not read.
fn branch(p: &model::projects::Project) -> &str {
    p.client
        .git_branch
        .as_deref()
        .filter(|b| !b.is_empty())
        .unwrap_or("main")
}

/// The project line: `alpha · main`.
fn project_line(s: &State) -> String {
    match s.active_project() {
        Some(p) => format!("{} · {}", p.name, branch(p)),
        None => String::new(),
    }
}

/// The phone frame: header, main area, and the drawer when open.
pub fn frame(s: &State, t: &'static Tokens, main: El) -> El {
    let c = &t.colors;
    let header = row()
        .key("phone-header")
        .w_full()
        .h(mobile::HEADER)
        .items_center()
        .px(8.0)
        .gap(4.0)
        .border_sides([0.0, 0.0, 1.0, 0.0], c.border)
        .child(
            mobile::icon_button(t, glyph("Menu"), "Open menu")
                .key("phone-menu")
                .on_press(|s: &mut State| s.overlays.open(DRAWER)),
        )
        .child(mobile::header_title(
            t,
            &header_title(s),
            mobile::header_subtitle(
                t,
                &project_line(s),
                &format!("{}, switch project", project_line(s)),
            )
            .key("phone-project-line")
            .on_press(|s: &mut State| s.overlays.open(PROJECTS)),
        ))
        .child(
            mobile::icon_button(t, glyph("MessageSquarePlus"), "New chat")
                .key("phone-new-chat")
                .on_press(|s: &mut State| s.toast_later("New chat")),
        )
        .child(
            mobile::icon_button(t, glyph("MoreHorizontal"), "More")
                .key("phone-more-button")
                .on_press(|s: &mut State| s.overlays.open(MORE)),
        );
    let mut frame = col().w_full().h_full().child(header).child(main);
    if s.overlays.is_open(DRAWER) {
        frame = frame.child(drawer(s, t));
    }
    frame
}

/// `MobileChatShell`'s `headerTitle`: a chat shows its own title ("Agent Chat" before it has
/// one); everything else still reads "Troupe".
fn header_title(s: &State) -> String {
    let active = model::workspace::query(&s.workspace.store, model::workspace::Query::GetActiveTab);
    match active {
        model::workspace::Reply::Tab(Some(tab @ model::workspace::Tab::AgentChat { .. })) => {
            crate::chat_view::tab_title(s, &tab).unwrap_or_else(|| "Agent Chat".to_owned())
        }
        model::workspace::Reply::Tab(Some(tab @ model::workspace::Tab::Terminal { .. })) => {
            crate::terminal_view::tab_title(s, &tab).unwrap_or_else(|| "Terminal".to_owned())
        }
        _ => "Troupe".to_owned(),
    }
}

/// The full-screen drawer.
fn drawer(s: &State, t: &'static Tokens) -> El {
    let c = &t.colors;
    let project = s.active_project();
    let name = project
        .map(|p| p.name.clone())
        .unwrap_or_else(|| "Troupe".to_owned());
    let badge = project.map_or_else(el, |p| {
        ui::shell::project_badge(t, &p.name, &p.color).size(20.0, 20.0)
    });
    let branch = project.map_or("main", branch);
    let head = row()
        .key("drawer-head")
        .w_full()
        .h(64.0)
        .pl(20.0)
        .pr(8.0)
        .gap(8.0)
        .items_center()
        .child(badge)
        .child(
            col()
                .grow(1.0)
                .min_w(0.0)
                .child(
                    text(name)
                        .font(sans(16.0, 500.0, 24.0))
                        .color(c.foreground)
                        .single_line()
                        .truncate(),
                )
                .child(
                    text(format!("{branch} · Local"))
                        .font(sans(16.0, 400.0, 24.0))
                        .color(c.muted_foreground)
                        .single_line()
                        .truncate(),
                ),
        )
        .child(
            mobile::icon_button(t, glyph("ChevronDown"), "Switch project")
                .key("drawer-switch-project")
                .on_press(|s: &mut State| s.overlays.open(PROJECTS)),
        )
        .child(
            mobile::icon_button(t, glyph("X"), "Close navigation")
                .key("drawer-close")
                .on_press(|s: &mut State| s.close(DRAWER)),
        );
    let tab = |name: &str, label: &str, on: bool| {
        let mut e = row()
            .role("tab")
            .label(label.to_owned())
            .focus_selected(on)
            .grow(1.0)
            .h(44.0)
            .center()
            .radius(22.0)
            .child(icon(glyph(name), 16.0).color(c.foreground));
        if on {
            e = e.fill(c.background);
        }
        e
    };
    let tabs = row()
        .key("drawer-tabs")
        .role("tablist")
        .label("Views")
        .mx(8.0)
        .padding(4.0)
        .radius(26.0)
        .fill(c.secondary)
        .child(tab("MessageSquare", "Chats", true))
        .child(
            tab("TerminalSquare", "Terminals", false)
                .on_press(|s: &mut State| s.toast_later("Terminals")),
        )
        .child(tab("FileCode", "Files", false).on_press(|s: &mut State| s.toast_later("Files")))
        .child(tab("Github", "GitHub", false).on_press(|s: &mut State| s.toast_later("GitHub")));
    let search = el()
        .key("drawer-search")
        .mx(8.0)
        .mt(12.0)
        .child(
            text_input(&s.phone.query, sans(16.0, 400.0, 24.0))
                .single_line()
                .label("Search chats")
                .placeholder("Search chats...".to_owned(), c.muted_foreground)
                .color(c.foreground)
                .w_full()
                .py(10.0)
                .pl(32.0)
                .pr(12.0)
                .radius(22.0)
                .fill(c.secondary)
                .on_edit(|s: &mut State| &mut s.phone.query),
        )
        .child(
            el().absolute(12.0, 15.0)
                .a11y_hidden()
                .child(icon(glyph("Search"), 14.0).color(c.muted_foreground)),
        );
    let query = s.phone.query.text.trim().to_lowercase();
    let hit = |chat: &&Chat| query.is_empty() || chat.label().to_lowercase().contains(&query);
    let mut list = col()
        .key("drawer-list")
        .role("list")
        .label("Recents")
        .w_full()
        .pt(8.0);
    let recents: Vec<&Chat> = project
        .map(|p| s.chats.of_project(&p.id).filter(hit).collect())
        .unwrap_or_default();
    let spaced = !recents.is_empty();
    if spaced {
        list = list.child(mobile::section(t, "Today", None));
        for chat in recents {
            list = list.child(recents_row(t, chat));
        }
    }
    let new_quick = mobile::icon_button(t, glyph("Plus"), "New quick chat")
        .key("drawer-new-quick")
        .on_press(|s: &mut State| s.toast_later("New quick chat"));
    list = list.child(
        mobile::section(t, "Quick chats", Some(new_quick)).mt(if spaced { 12.0 } else { 0.0 }),
    );
    for chat in s.chats.quick().filter(hit) {
        list = list.child(recents_row(t, chat));
    }
    list = list.child(
        row()
            .key("drawer-import")
            .role("button")
            .label("Import chats")
            .w_full()
            .h(TOUCH + 8.0)
            .px(28.0)
            .gap(16.0)
            .items_center()
            .child(
                icon(glyph("Download"), 16.0)
                    .color(c.foreground)
                    .a11y_hidden(),
            )
            .child(
                text("Import chats")
                    .font(sans(16.0, 500.0, 24.0))
                    .color(c.foreground),
            )
            .on_press(|s: &mut State| s.toast_later("Import chats")),
    );
    let foot = |name: &str, label: &str, what: &'static str| {
        mobile::icon_button(t, glyph(name), label)
            .on_press(move |s: &mut State| s.toast_later(what))
    };
    let new_chat = row()
        .key("drawer-new-chat")
        .role("button")
        .label("New chat")
        .h(36.0)
        .px(14.0)
        .gap(8.0)
        .items_center()
        .radius(6.0)
        .fill(c.primary)
        .child(
            icon(glyph("Plus"), 16.0)
                .color(c.primary_foreground)
                .a11y_hidden(),
        )
        .child(
            text("New chat")
                .font(sans(16.0, 500.0, 24.0))
                .color(c.primary_foreground),
        )
        .on_press(|s: &mut State| s.toast_later("New chat"));
    let footer = row()
        .key("drawer-footer")
        .w_full()
        .h(60.0)
        .px(8.0)
        .gap(8.0)
        .items_center()
        .justify_between()
        .border_sides([1.0, 0.0, 0.0, 0.0], c.border)
        .child(foot("Settings", "Settings", "Settings"))
        .child(foot("Camera", "Screenshot", "Screenshots"))
        .child(foot("History", "Git history", "Git history"))
        .child(foot("CalendarClock", "Automations", "Automations"))
        .child(new_chat);
    col()
        .key(DRAWER)
        .role("dialog")
        .label("Navigation")
        .pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0))
        .fill(c.background)
        .child(head)
        .child(tabs)
        .child(search)
        .child(
            col()
                .w_full()
                .grow(1.0)
                .min_h(0.0)
                .child(list)
                .scroll(rune::ScrollSpec {
                    stick_bottom: false,
                }),
        )
        .child(footer)
        .modal(true, |s: &mut State| s.close(DRAWER))
}

/// A recents row (`MobileRecentsList`): the bot glyph, the title, the time, and ⋯.
fn recents_row(t: &'static Tokens, chat: &Chat) -> El {
    let c = &t.colors;
    let id = chat.session_id.clone();
    let open = chat.session_id.clone();
    row()
        .key(format!("recent-{}", chat.session_id))
        .w_full()
        .h(TOUCH)
        .pl(24.0)
        .pr(8.0)
        .gap(12.0)
        .items_center()
        .child(
            icon(glyph("Bot"), 14.0)
                .color(c.muted_foreground)
                .a11y_hidden(),
        )
        .child(
            text(chat.label().to_owned())
                .font(sans(16.0, 400.0, 24.0))
                .color(c.foreground)
                .single_line()
                .truncate()
                .min_w(0.0)
                .grow(1.0)
                .role("button")
                .label(chat.label().to_owned())
                .on_press(move |s: &mut State| {
                    s.close(DRAWER);
                    s.open_chat(&open);
                }),
        )
        .child(
            text(ids::clock_time(chat.last_activity_at))
                .font(sans(16.0, 400.0, 24.0))
                .color(c.muted_foreground),
        )
        .child(
            mobile::icon_button(
                t,
                glyph("MoreHorizontal"),
                &format!("More actions for {}", chat.label()),
            )
            .key("actions")
            .on_press(move |s: &mut State| {
                s.phone.chat = Some(id.clone());
                s.overlays.open(RECENTS);
            }),
        )
}

/// A sheet's rows, flush (the sheet body spaces its children 8 px apart).
fn stack(rows: Vec<El>, gap: f32) -> El {
    col().w_full().gap(gap).items_stretch().children(rows)
}

/// The open phone sheets, bottom-most first.
pub fn sheets(s: &State, t: &'static Tokens, window: (f32, f32), layer: &mut Vec<El>) {
    for key in s.overlays.keys().iter().map(String::as_str) {
        match key {
            MORE => more_sheet(s, t, window, layer),
            PROJECTS => projects_sheet(s, t, window, layer),
            RECENTS => recents_sheet(s, t, window, layer),
            crate::composer_sheet::ADD_SHEET => crate::composer_sheet::add_sheet(s, t, window, layer),
            crate::model_sheet::MODEL_SHEET => crate::model_sheet::model_sheet(s, t, window, layer),
            crate::git::GIT_SHEET => {
                sheet(crate::git::GIT_SHEET, "Git changes".to_owned(), s)
                    .child(crate::git_view::phone_panel(s, t, window, layer))
                    .build(t, window, layer);
            }
            _ => {}
        }
    }
}

pub(crate) fn sheet(key: &'static str, title: String, s: &State) -> MobileSheet {
    MobileSheet::new(key, title)
        .open(true)
        .drag(if s.overlays.top() == Some(key) {
            s.phone.drag
        } else {
            SheetDrag::default()
        })
        .on_drag(
            |s: &mut State| &mut s.phone.drag,
            move |s: &mut State| s.close(key),
        )
        .on_close(move |s: &mut State| s.close(key))
}

fn more_sheet(s: &State, t: &'static Tokens, window: (f32, f32), layer: &mut Vec<El>) {
    let row_of = |name: &str, label: &'static str| {
        mobile::sheet_row(t, glyph(name), label, false).on_press(move |s: &mut State| {
            s.close(MORE);
            s.toast_later(label);
        })
    };
    sheet(MORE, "Troupe".to_owned(), s)
        .child(stack(
            vec![
                mobile::sheet_subtitle(t, &project_line(s)),
                mobile::sheet_row(t, glyph("GitBranch"), "Git changes", false).key("more-git").on_press(|s: &mut State| {
                    s.close(MORE);
                    s.overlays.open(crate::git::GIT_SHEET);
                }),
                row_of("FolderOpen", "Files"),
                row_of("Search", "Command palette"),
                mobile::sheet_row(t, glyph("TerminalSquare"), "New terminal", false).on_press(|s: &mut State| {
                    s.close(MORE);
                    s.new_terminal(None);
                }),
                row_of("Settings", "Project settings"),
                row_of("Bug", "Report a bug"),
            ],
            0.0,
        ))
        .build(t, window, layer);
}

fn projects_sheet(s: &State, t: &'static Tokens, window: (f32, f32), layer: &mut Vec<El>) {
    let c = &t.colors;
    let active = s.active_project().map(|p| p.id.clone());
    let mut rows = Vec::new();
    for project in s
        .projects
        .store
        .projects
        .iter()
        .filter(|p| p.is_archived != Some(true))
    {
        let current = active.as_deref() == Some(project.id.as_str());
        let default = project.is_default == Some(true);
        let mut line = row()
            .key(format!("sheet-project-{}", project.id))
            .role("button")
            .label(project.name.clone())
            .grow(1.0)
            .min_w(0.0)
            .min_h(TOUCH)
            .px(12.0)
            .gap(10.0)
            .items_center()
            .radius(10.0)
            .child(ui::shell::project_badge(t, &project.name, &project.color))
            .child(
                text(project.name.clone())
                    .font(sans(16.0, 400.0, 24.0))
                    .color(c.foreground)
                    .grow(1.0)
                    .truncate(),
            );
        if default {
            line = line
                .child(
                    icon(glyph("Home"), 14.0)
                        .color(c.muted_foreground)
                        .a11y_hidden(),
                )
                .child(
                    text("Default")
                        .font(sans(16.0, 400.0, 24.0))
                        .color(c.muted_foreground),
                );
        }
        if current {
            line = line.fill(c.muted).child(
                text("Current")
                    .font(sans(16.0, 400.0, 24.0))
                    .color(c.muted_foreground),
            );
        }
        let id = project.id.clone();
        line = line.on_press(move |s: &mut State| {
            s.close(PROJECTS);
            s.select_project(&id);
        });
        let (glyph_name, label) = if current {
            ("Settings", "Project settings")
        } else {
            ("Home", "Make default")
        };
        let action = mobile::icon_button(t, glyph(glyph_name), label)
            .on_press(move |s: &mut State| s.toast_later(label));
        rows.push(
            row()
                .w_full()
                .gap(4.0)
                .items_center()
                .child(line)
                .child(action),
        );
    }
    sheet(PROJECTS, "Projects".to_owned(), s)
        .child(stack(rows, 2.0))
        .child(el().w_full().h(0.5).fill(c.border).my(4.0))
        .child(
            mobile::sheet_row(t, glyph("Plus"), "Add project", false)
                .key("sheet-add-project")
                .on_press(|s: &mut State| {
                    s.close(PROJECTS);
                    s.close(DRAWER);
                    s.overlays.open(rail::NEW_PROJECT);
                }),
        )
        .build(t, window, layer);
}

fn recents_sheet(s: &State, t: &'static Tokens, window: (f32, f32), layer: &mut Vec<El>) {
    let Some(chat) = s.phone.chat.as_ref().and_then(|id| s.chats.get(id)) else {
        return;
    };
    let c = &t.colors;
    let id = chat.session_id.clone();
    let reference = rail_menus::chat_reference(s, &id);
    let delete_id = id.clone();
    sheet(RECENTS, chat.label().to_owned(), s)
        .child(
            mobile::sheet_row(t, glyph("Copy"), "Copy chat reference", false)
                .key("recents-copy")
                .on_press_tx(move |s: &mut State, tx: &mut rune::Tx| {
                    tx.copy(reference.clone());
                    s.close(RECENTS);
                    s.toasts.push(
                        s.now,
                        ui::overlay::ToastKind::Success,
                        "Chat reference copied",
                        Some(&id),
                    );
                }),
        )
        .child(el().w_full().h(0.5).fill(c.border).my(4.0))
        .child(
            mobile::sheet_row(t, glyph("Trash2"), "Delete chat", true)
                .key("recents-delete")
                .on_press(move |s: &mut State| {
                    s.close(RECENTS);
                    s.rail.chat = Some(delete_id.clone());
                    s.overlays.open(rail::DELETE_CHAT);
                }),
        )
        .build(t, window, layer);
}
