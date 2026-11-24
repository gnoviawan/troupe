//! The shell frame. Desktop: rail | main area | side panel | tool strip (`ShellFrame`). Below
//! the phone breakpoint the phone shell (`phone`) replaces it. Every open overlay mounts in one
//! layer above the frame.

use client::link::Status;
use model::shell_layout::{self, Event as LayoutEvent, Tab};
use rune::{Cx, Drag, El, Phase, Tx, col, el, row, text};
use serde_json::json;
use theme::Tokens;
use theme::icons as i;
use ui::shell::{self, COLLAPSED_RAIL, PHONE_MAX_WIDTH, TOOL_STRIP};
use ui::type_::{mono, sans};

use crate::{Edge, Route, State};

const ROOT: &str = "troupe-root";
const RAIL_TOGGLE: &str = "Expand or collapse projects";

pub fn view(s: &State, cx: &mut Cx<State>) -> El {
    let t = s.tokens();
    s.zoom.apply(cx);
    ui::overlay::history::sync(s.overlays.depth());
    let size = cx.layout_size(ROOT);
    let phone = size.is_some_and(|[w, _]| w <= PHONE_MAX_WIDTH);
    let window = size.map_or((1280.0, 800.0), |[w, h]| (w, h));
    let root = row().key(ROOT).w_full().h_full().fill(t.colors.background);
    if let Route::NotFound(path) = &s.route {
        return root.child(not_found(t, path));
    }
    let mut layer = Vec::new();
    let root = if phone {
        root.child(crate::phone::frame(s, t, workspace_main(s, t)))
    } else {
        root.children(desktop_frame(s, t, &mut layer))
    };
    crate::rail_menus::build(s, t, &mut layer);
    crate::rail_forms::build(s, t, &mut layer);
    crate::phone::sheets(s, t, window, &mut layer);
    crate::theme_picker::build(s, t, phone, &mut layer);
    crate::keys::build(s, t, window.1, &mut layer);
    layer.push(
        row()
            .key("toaster-layer")
            .pin(None, Some(16.0), Some(16.0), None)
            .child(s.toasts.build(t, |id| Box::new(move |s: &mut State| s.toasts.dismiss(id)))),
    );
    ui::overlay::root(root, layer)
}

fn desktop_frame(s: &State, t: &'static Tokens, layer: &mut Vec<El>) -> Vec<El> {
    let expanded = s.layout.record.rail_expanded;
    let mut parts = vec![rail(s, t, !expanded, s.width(Edge::Rail) as f32, layer)];
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

fn rail(s: &State, t: &'static Tokens, collapsed: bool, width: f32, layer: &mut Vec<El>) -> El {
    let width = if collapsed { COLLAPSED_RAIL } else { width };
    let mut nav = col()
        .key("shell-rail")
        .role("navigation")
        .label("Projects")
        .w(width)
        .h_full()
        .clip_rounded(0.0)
        .fill(t.colors.background)
        .child(crate::rail_view::body(s, t, collapsed, (!collapsed).then(|| rail_toggle(t, true)), layer));
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

/// The web client's `NotFound` page, which replaces the whole shell.
fn not_found(t: &'static Tokens, path: &str) -> El {
    col()
        .key("not-found")
        .w_full()
        .h_full()
        .items_center()
        .justify_center()
        .child(
            text("404")
                .font(mono(60.0, 700.0, 60.0))
                .color(t.colors.primary)
                .role("heading")
                .mb(16.0),
        )
        .child(
            text("Page not found")
                .font(sans(20.0, 400.0, 28.0))
                .color(t.colors.foreground)
                .mb(8.0),
        )
        .child(
            text(path.to_owned())
                .font(mono(14.0, 400.0, 20.0))
                .color(t.colors.muted_foreground)
                .mb(24.0),
        )
        .child(
            ui::Button::new("Return to Workspace")
                .size(ui::ButtonSize::Sm)
                .build(t)
                .key("return-to-workspace")
                .on_press(|s: &mut State| s.route = Route::Workspace),
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

/// The main area under the phone header.
fn workspace_main(s: &State, t: &'static Tokens) -> El {
    col().key("workspace-main").grow(1.0).w_full().min_h(0.0).center().child(main_body(s, t))
}
