//! The shell frame. Desktop: rail | main area | side panel | tool strip (`ShellFrame`). Below
//! the phone breakpoint the rail moves into a drawer behind a menu button, until the phone
//! shell's own header, drawer and sheets are ported.

use client::link::Status;
use model::shell_layout::{self, Event as LayoutEvent, Tab};
use rune::{Cx, Drag, El, Phase, Tx, col, el, row, text};
use serde_json::json;
use theme::Tokens;
use theme::icons as i;
use ui::shell::{self, COLLAPSED_RAIL, PHONE_MAX_WIDTH, TOOL_STRIP};
use ui::type_::sans;

use crate::{DRAWER, Edge, Route, State};

const ROOT: &str = "troupe-root";
const RAIL_TOGGLE: &str = "Expand or collapse projects";

pub fn view(s: &State, cx: &mut Cx<State>) -> El {
    let t = theme::tokens(s.theme, true);
    ui::overlay::history::sync(s.overlays.depth());
    let phone = cx
        .layout_size(ROOT)
        .is_some_and(|[w, _]| w <= PHONE_MAX_WIDTH);
    let root = row().key(ROOT).w_full().h_full().fill(t.colors.background);
    if phone {
        root.child(phone_frame(s, t))
    } else {
        root.children(desktop_frame(s, t))
    }
}

fn desktop_frame(s: &State, t: &'static Tokens) -> Vec<El> {
    let expanded = s.layout.record.rail_expanded;
    let mut parts = vec![rail(s, t, !expanded, s.width(Edge::Rail) as f32)];
    let mut shell_row = row()
        .key("shell-row")
        .grow(1.0)
        .h_full()
        .min_w(0.0)
        .child(main_area(s, t, !expanded));
    if s.layout.record.side_panel_visible && s.active_project().is_some() {
        shell_row = shell_row.child(side_panel(s, t));
    }
    parts.push(shell_row.child(tool_strip(s, t)));
    parts
}

fn rail_toggle(t: &Tokens, expanded: bool) -> El {
    let glyph = if expanded {
        &i::PANEL_LEFT_COLLAPSE
    } else {
        &i::PANEL_LEFT_EXPAND
    };
    shell::rail_button(t, glyph, RAIL_TOGGLE, 28.0, 14.0, false)
        .key("rail-toggle")
        .on_press(move |s: &mut State| {
            s.layout_step(LayoutEvent::SetRailExpanded(json!(!expanded)))
        })
}

fn select_project(s: &mut State, id: &str) {
    s.projects_step(model::projects::Event::SelectProject(id.to_owned()));
    s.overlays.close(DRAWER);
    s.route = Route::Workspace;
}

/// The rail's body: navigation, the server's projects, quick chats, and the footer actions.
fn rail_body(s: &State, t: &'static Tokens, collapsed: bool) -> El {
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
        .child(shell::nav_row(t, &i::SEARCH_LIST, "Search", collapsed, false).key("rail-search"))
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
    for project in s
        .projects
        .store
        .projects
        .iter()
        .filter(|p| p.is_archived != Some(true))
    {
        let is_active = active.is_some_and(|a| a.id == project.id);
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
        let id = project.id.clone();
        projects = projects.child(
            shell::project_row(
                t,
                &project.name,
                &project.color,
                is_active,
                collapsed,
                trailing,
            )
            .key(format!("project-item-{}", project.id))
            .on_press(move |s: &mut State| select_project(s, &id)),
        );
    }

    let mut body = col().w_full().grow(1.0).min_h(0.0).child(nav);
    if collapsed {
        body = body.child(el().h(22.0)).child(projects);
    } else {
        let add = shell::rail_button(
            t,
            &i::PLUS,
            "Add project or folder group",
            20.0,
            12.0,
            false,
        );
        body = body
            .child(el().h(22.0))
            .child(shell::section_header(t, "Projects", Some(add)))
            .child(el().h(2.0))
            .child(projects)
            .child(el().h(12.0))
            .child(shell::section_header(
                t,
                "Quick chats",
                Some(shell::rail_button(
                    t,
                    &i::PLUS,
                    "New quick chat",
                    20.0,
                    12.0,
                    false,
                )),
            ));
    }
    let footer_button = |glyph, label: &str| shell::rail_button(t, glyph, label, 28.0, 14.0, false);
    let footer_items = [
        footer_button(&i::SLIDERS, "Settings"),
        footer_button(&i::BUG, "Report a bug"),
        footer_button(&i::PALETTE, "Color themes"),
        footer_button(&i::KEYBOARD, "Keyboard shortcuts"),
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
        .child(shell::rail_header(
            t,
            collapsed,
            (!collapsed).then(|| rail_toggle(t, true)),
        ))
        .child(body.scroll(rune::ScrollSpec {
            stick_bottom: false,
        }))
        .child(footer)
}

/// A drag on a resizable edge: `sign` is +1 when moving right grows the part.
fn resize(edge: Edge, sign: f32) -> impl Fn(&mut State, &mut Drag, &mut Tx) + 'static {
    move |s: &mut State, d: &mut Drag, _tx: &mut Tx| match d.phase {
        Phase::Down => d.data = s.width(edge) as f32,
        Phase::Move => {
            let bounds = match edge {
                Edge::Rail => shell_layout::RAIL_WIDTH,
                Edge::SidePanel => shell_layout::SIDE_PANEL_WIDTH,
            };
            let width = f64::from(d.data + sign * d.dx).clamp(bounds.min, bounds.max);
            s.drag_width = Some((edge, width));
        }
        Phase::Up => {
            let width = s.drag_width.take().map(|(_, w)| w);
            if let Some(width) = width.filter(|_| !d.cancelled()) {
                let event = match edge {
                    Edge::Rail => LayoutEvent::SetRailWidth(json!(width)),
                    Edge::SidePanel => LayoutEvent::SetSidePanelWidth(json!(width)),
                };
                s.layout_step(event);
            }
        }
    }
}

fn rail(s: &State, t: &'static Tokens, collapsed: bool, width: f32) -> El {
    let width = if collapsed { COLLAPSED_RAIL } else { width };
    let mut nav = col()
        .key("shell-rail")
        .role("navigation")
        .label("Projects")
        .w(width)
        .h_full()
        .clip_rounded(0.0)
        .fill(t.colors.background)
        .child(rail_body(s, t, collapsed));
    if !collapsed {
        nav = nav.child(
            shell::resize_handle(t, "Resize projects rail")
                .key("rail-resize")
                .pin(None, Some(0.0), Some(0.0), Some(0.0))
                .on_drag(resize(Edge::Rail, 1.0)),
        );
    }
    nav
}

fn new_tab_button(t: &Tokens) -> El {
    shell::rail_button(t, &i::PLUS, "New tab", 28.0, 14.0, false).key("new-tab")
}

fn main_area(s: &State, t: &'static Tokens, rail_collapsed: bool) -> El {
    let start = if rail_collapsed {
        vec![rail_toggle(t, false)]
    } else {
        Vec::new()
    };
    col()
        .key("shell-main")
        .grow(1.0)
        .h_full()
        .min_w(0.0)
        .border_sides([0.0, 0.0, 0.0, 1.0], t.colors.border)
        .child(shell::top_row(t, start, vec![new_tab_button(t)]))
        .child(
            col()
                .key("workspace-main")
                .grow(1.0)
                .w_full()
                .min_h(0.0)
                .center()
                .child(main_body(s, t)),
        )
}

/// The main area's content: the launcher greeting for the active project, or what stands in
/// for it while the link is not up.
fn main_body(s: &State, t: &'static Tokens) -> El {
    let note = |line: String| {
        text(line)
            .font(sans(13.0, 400.0, 20.0))
            .color(t.colors.muted_foreground)
    };
    if s.route == Route::NotFound {
        return col()
            .items_center()
            .gap(16.0)
            .child(
                text("Page not found")
                    .font(sans(22.0, 400.0, 28.0))
                    .color(t.colors.foreground)
                    .role("heading"),
            )
            .child(
                ui::Button::new("Return to Workspace")
                    .build(t)
                    .key("return-to-workspace")
                    .on_press(|s: &mut State| s.route = Route::Workspace),
            );
    }
    if let Some(project) = s.active_project() {
        return shell::launcher_heading(t, &project.name);
    }
    match &s.status {
        Status::Halted { code, message } => note(format!(
            "The server refused this client ({code}): {message}"
        )),
        _ if s.server.is_empty() => note("No server: set TROUPE_SERVER_URL".to_owned()),
        Status::Live if s.projects.store.is_loaded => note("No projects yet".to_owned()),
        Status::Reconnecting { .. } => note(format!("Reconnecting to {}", s.server)),
        _ => note(format!("Connecting to {}", s.server)),
    }
}

fn close_panel(s: &mut State) {
    s.layout_step(LayoutEvent::SetSidePanelVisible(json!(false)));
}

fn side_panel(s: &State, t: &'static Tokens) -> El {
    let project = s
        .active_project()
        .map(|p| p.id.as_str())
        .unwrap_or_default();
    let tab = s.layout.tab(project);
    let close = shell::rail_button(t, &i::X, "Close panel", 28.0, 13.0, false)
        .key("close-panel")
        .on_press(close_panel);
    col()
        .key("side-panel")
        .role("complementary")
        .label("Side panel")
        .w(s.width(Edge::SidePanel) as f32)
        .h_full()
        .border_sides([0.0, 0.0, 0.0, 1.0], t.colors.border)
        .child(shell::panel_header(t, tab.label(), vec![close]))
        .child(shell::panel_skeleton(t).key(format!("side-panel-{}", tab.label().to_lowercase())))
        .child(
            shell::resize_handle(t, "Resize side panel")
                .key("side-panel-resize")
                .pin(Some(0.0), Some(0.0), None, Some(0.0))
                .on_drag(resize(Edge::SidePanel, -1.0)),
        )
}

/// `handleView`: a strip button opens its view, or hides the panel when that view is open.
fn toggle_view(s: &mut State, tab: Tab) {
    let Some(project) = s.active_project().map(|p| p.id.clone()) else {
        return;
    };
    let visible = s.layout.record.side_panel_visible;
    let open = visible && s.layout.tab(&project) == tab;
    if !open {
        s.layout_step(LayoutEvent::SetLastColumnTab(project, json!(tab)));
    }
    if open || !visible {
        s.layout_step(LayoutEvent::SetSidePanelVisible(json!(!open)));
    }
}

fn tool_strip(s: &State, t: &'static Tokens) -> El {
    let project = s.active_project();
    let has_path = project.is_some_and(|p| p.path.is_some());
    let visible = s.layout.record.side_panel_visible;
    let selected = project.map(|p| s.layout.tab(&p.id));
    let glyph = |tab: Tab| match tab {
        Tab::Explorer => &i::FOLDER2,
        Tab::Changes => &i::GIT_BRANCH,
        Tab::Github => &i::GITHUB,
    };
    let views = Tab::ALL.map(|tab| {
        shell::tool_button(
            t,
            glyph(tab),
            tab.label(),
            visible && selected == Some(tab),
            !has_path,
            None,
        )
        .key(format!("tool-strip-{}", tab.label().to_lowercase()))
        .on_press(move |s: &mut State| toggle_view(s, tab))
    });
    col()
        .key("tool-strip")
        .role("toolbar")
        .label("Tools")
        .w(TOOL_STRIP)
        .h_full()
        .items_center()
        .pt(6.0)
        .pb(8.0)
        .gap(4.0)
        .border_sides([0.0, 0.0, 0.0, 1.0], t.colors.border)
        .children(views)
        .child(el().grow(1.0).w_full().window_drag())
        .child(shell::tool_button(
            t,
            &i::TERMINAL,
            "New terminal",
            false,
            true,
            None,
        ))
        .child(shell::connection_lamp(t, s.status == Status::Live))
}

/// Below the phone breakpoint: a header with the menu button and the project, the main area,
/// and the rail as a drawer. The phone shell's own header and sheets replace it.
fn phone_frame(s: &State, t: &'static Tokens) -> El {
    let project = s
        .active_project()
        .map(|p| p.name.clone())
        .unwrap_or_default();
    let menu = shell::rail_button(t, &i::MENU, "Open navigation", 40.0, 16.0, false)
        .key("phone-menu")
        .on_press(|s: &mut State| s.overlays.open(DRAWER));
    let header = row()
        .key("phone-header")
        .w_full()
        .h(56.0)
        .items_center()
        .gap(8.0)
        .px(8.0)
        .border_sides([0.0, 0.0, 1.0, 0.0], t.colors.border)
        .child(menu)
        .child(
            col()
                .grow(1.0)
                .min_w(0.0)
                .child(
                    text("Troupe")
                        .font(sans(15.0, 500.0, 20.0))
                        .color(t.colors.foreground),
                )
                .child(
                    text(project)
                        .font(sans(13.0, 400.0, 18.0))
                        .color(t.colors.muted_foreground)
                        .single_line()
                        .truncate(),
                ),
        );
    let mut frame = col().w_full().h_full().child(header).child(
        col()
            .key("workspace-main")
            .grow(1.0)
            .w_full()
            .min_h(0.0)
            .center()
            .child(main_body(s, t)),
    );
    if s.overlays.is_open(DRAWER) {
        frame = frame
            .child(
                el().key("drawer-scrim")
                    .pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0))
                    .fill(t.colors.overlay.alpha(0.6))
                    .on_press(|s: &mut State| {
                        s.overlays.close(DRAWER);
                    }),
            )
            .child(
                col()
                    .key(DRAWER)
                    .role("dialog")
                    .label("Navigation")
                    .w(300.0)
                    .pin(Some(0.0), Some(0.0), None, Some(0.0))
                    .fill(t.colors.background)
                    .border_sides([0.0, 1.0, 0.0, 0.0], t.colors.border)
                    .child(rail_body(s, t, false)),
            );
    }
    frame
}
