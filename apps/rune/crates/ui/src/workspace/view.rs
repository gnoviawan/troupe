//! Drawing the workspace: `PaneRenderer` (splits and their handles), `PaneContent` (a leaf:
//! its tab bar over its content), `WorkspaceTabBar` with `TabChrome`, `NewTabMenu`, the
//! `DropZoneOverlay` and the unsaved-changes dialog from `WorkspaceCloseDialogs`.

use std::rc::Rc;

use model::workspace::{Direction, Leaf, PaneNode, Split, Tab};
use rune::{Cursor, Cx, Drag, El, Icon, Phase, Tx, col, el, icon, row, text};
use theme::Tokens;
use theme::icons as i;

use super::dnd::{DropTarget, Geometry, Side, zone_rect};
use super::{Request, Sizing, TabDrag, Workspace};
use crate::overlay::{ConfirmDialog, Menu, MenuItem};
use crate::paint::shadows;
use crate::shell::TOP_ROW;
use crate::type_::sans;

/// How far a pressed tab moves before it drags.
const SLOP: f32 = 4.0;
/// `minSize` of every panel, in percent.
const MIN_SHARE: f64 = 10.0;

/// What the app puts in the workspace.
pub struct Slots<'a> {
    /// At the start of the top-left pane's tab bar (the rail's expand toggle).
    pub leading: Vec<El>,
    /// What a pane with no tab shows (the launcher greeting).
    pub empty: &'a dyn Fn() -> El,
    /// The body of a tab the app draws itself (a chat), or None for the placeholder.
    pub tab: &'a dyn Fn(&Tab) -> Option<El>,
    /// The label of a tab the app names itself (a chat's title), or None for the default.
    pub title: &'a dyn Fn(&Tab) -> Option<String>,
    /// The count pill after a tab's label (the Git tab's changed files), when above zero.
    pub badge: &'a dyn Fn(&Tab) -> usize,
    pub dark: bool,
}

type Lens<S> = fn(&mut S) -> &mut Workspace;

struct Ctx<'a, S> {
    ws: &'a Workspace,
    lens: Lens<S>,
    t: &'static Tokens,
    geometry: Rc<Geometry>,
    dark: bool,
    tab: &'a dyn Fn(&Tab) -> Option<El>,
    title: &'a dyn Fn(&Tab) -> Option<String>,
    badge: &'a dyn Fn(&Tab) -> usize,
}

fn content_key(pane: &str) -> String {
    format!("pane-content-{pane}")
}

fn bar_key(pane: &str) -> String {
    format!("tab-bar-{pane}")
}

fn tab_key(tab: &str) -> String {
    format!("tab-{tab}")
}

fn split_key(split: &str) -> String {
    format!("split-{split}")
}

/// The rects of the last layout, for the drag to test.
fn geometry<S>(ws: &Workspace, cx: &Cx<S>) -> Geometry {
    let mut g = Geometry::default();
    for leaf in model::workspace::leaves(ws.root()) {
        if let Some(rect) = cx.layout(&content_key(&leaf.id)) {
            g.contents.push((leaf.id.clone(), rect));
        }
        if let Some(rect) = cx.layout(&bar_key(&leaf.id)) {
            g.bars.push((leaf.id.clone(), rect));
        }
        for tab in &leaf.tabs {
            if let Some(rect) = cx.layout(&tab_key(tab.id())) {
                g.tabs.push((leaf.id.clone(), tab.id().to_owned(), rect));
            }
        }
    }
    g
}

/// The workspace for an app state `S` whose [`Workspace`] `lens` reaches. Menus and dialogs
/// go into `layer`.
pub fn view<S: 'static>(
    ws: &Workspace,
    lens: Lens<S>,
    t: &'static Tokens,
    cx: &Cx<S>,
    slots: Slots<'_>,
    layer: &mut Vec<El>,
) -> El {
    let ctx = Ctx {
        ws,
        lens,
        t,
        geometry: Rc::new(geometry(ws, cx)),
        dark: slots.dark,
        tab: slots.tab,
        title: slots.title,
        badge: slots.badge,
    };
    let tree = &ws.store.tree;
    let fullscreen = tree
        .fullscreen_pane_id
        .as_deref()
        .and_then(|id| model::workspace::find_leaf(&tree.root, id));
    let mut leading = Some(slots.leading);
    let body = match fullscreen {
        Some(leaf) => pane(&ctx, leaf, &mut leading, slots.empty, layer, cx),
        None => node(&ctx, &tree.root, &mut leading, slots.empty, layer, cx),
    };
    unsaved_dialog(&ctx, layer);
    col()
        .key("workspace")
        .w_full()
        .h_full()
        .min_w(0.0)
        .min_h(0.0)
        .child(body)
}

fn node<S: 'static>(
    ctx: &Ctx<'_, S>,
    node: &PaneNode,
    leading: &mut Option<Vec<El>>,
    empty: &dyn Fn() -> El,
    layer: &mut Vec<El>,
    cx: &Cx<S>,
) -> El {
    match node {
        PaneNode::Leaf(leaf) => pane(ctx, leaf, leading, empty, layer, cx),
        PaneNode::Split(split) => split_view(ctx, split, leading, empty, layer, cx),
    }
}

fn split_view<S: 'static>(
    ctx: &Ctx<'_, S>,
    split: &Split,
    leading: &mut Option<Vec<El>>,
    empty: &dyn Fn() -> El,
    layer: &mut Vec<El>,
    cx: &Cx<S>,
) -> El {
    let horizontal = split.direction == Direction::Horizontal;
    let key = split_key(&split.id);
    let length = cx
        .layout_size(&key)
        .map_or(0.0, |[w, h]| if horizontal { w } else { h });
    let sizes = match &ctx.ws.sizing {
        Some(Sizing { split: id, sizes, .. }) if *id == split.id => sizes.clone(),
        _ => split.sizes.clone(),
    };
    let container = if horizontal { row() } else { col() };
    let mut container = container.key(key).w_full().h_full().min_w(0.0).min_h(0.0);
    let last = split.children.len().saturating_sub(1);
    for (index, child) in split.children.iter().enumerate() {
        // Only the first child of a split can sit at the window's top-left corner.
        let mut none = None;
        let child_leading = if index == 0 { &mut *leading } else { &mut none };
        let share = sizes.get(index).copied().unwrap_or(50.0) as f32;
        container = container.child(
            el().key(format!("panel-{}", child.id()))
                .grow(share.max(0.001))
                .min_w(0.0)
                .min_h(0.0)
                .w_full()
                .h_full()
                .child(node(ctx, child, child_leading, empty, layer, cx)),
        );
        if index < last {
            container = container.child(handle(ctx, split, index, horizontal, length));
        }
        if index == 0 && !horizontal {
            *leading = None;
        }
    }
    container
}

/// `ResizableHandle`: a 1 pt border line with a 4 pt hit strip that resizes its two
/// neighbours, each kept at 10 % or more, and commits on release.
fn handle<S: 'static>(
    ctx: &Ctx<'_, S>,
    split: &Split,
    index: usize,
    horizontal: bool,
    length: f32,
) -> El {
    let lens = ctx.lens;
    let id = split.id.clone();
    let hit = el()
        .key(format!("resize-{}-{index}", split.id))
        .role("separator")
        .label("Resize panes")
        .cursor(if horizontal {
            Cursor::ResizeColumn
        } else {
            Cursor::ResizeRow
        })
        .on_drag(move |s: &mut S, d: &mut Drag, _: &mut Tx| {
            let ws = lens(s);
            match d.phase {
                Phase::Down => {
                    let sizes = model::workspace::find_split(ws.root(), &id)
                        .map(|split| split.sizes.clone())
                        .unwrap_or_default();
                    ws.sizing = Some(Sizing {
                        split: id.clone(),
                        start: sizes.clone(),
                        sizes,
                    });
                }
                Phase::Move => {
                    let travel = if horizontal { d.dx } else { d.dy };
                    if let Some(sizing) = ws.sizing.as_mut().filter(|s| s.split == id) {
                        sizing.sizes = resized(&sizing.start, index, travel, length);
                    }
                }
                Phase::Up => {
                    let done = ws.sizing.take().filter(|s| s.split == id);
                    if let Some(sizing) = done.filter(|s| !d.cancelled() && s.sizes != s.start) {
                        ws.resize(&id, sizing.sizes);
                    }
                }
            }
        });
    let hit = if horizontal {
        hit.w(4.0).pin(Some(-1.5), Some(0.0), None, Some(0.0))
    } else {
        hit.h(4.0).pin(Some(0.0), Some(-1.5), Some(0.0), None)
    };
    let line = el().fill(ctx.t.colors.border).child(hit);
    if horizontal {
        line.w(1.0).h_full()
    } else {
        line.h(1.0).w_full()
    }
}

/// The shares after the handle between `index` and `index + 1` moved `travel` points along
/// a group `length` points long.
#[must_use]
pub fn resized(start: &[f64], index: usize, travel: f32, length: f32) -> Vec<f64> {
    let mut sizes = start.to_vec();
    if length <= 0.0 || index + 1 >= sizes.len() {
        return sizes;
    }
    let pair = sizes[index] + sizes[index + 1];
    let delta = f64::from(travel) / f64::from(length) * 100.0;
    // A pair a stored layout left under twice the minimum splits evenly at most, never below 0.
    let floor = MIN_SHARE.min(pair / 2.0);
    let first = (sizes[index] + delta).clamp(floor, pair - floor);
    sizes[index] = first;
    sizes[index + 1] = pair - first;
    sizes
}

fn pane<S: 'static>(
    ctx: &Ctx<'_, S>,
    leaf: &Leaf,
    leading: &mut Option<Vec<El>>,
    empty: &dyn Fn() -> El,
    layer: &mut Vec<El>,
    cx: &Cx<S>,
) -> El {
    let ws = ctx.ws;
    let t = ctx.t;
    let tree = ws.tree();
    let fullscreen = tree.fullscreen_pane_id.as_deref() == Some(leaf.id.as_str());
    let active = tree.active_pane_id == leaf.id;
    let lens = ctx.lens;
    let pane_id = leaf.id.clone();
    let bar = tab_bar(ctx, leaf, leading.take().unwrap_or_default(), layer);
    let mut content = col()
        .key(content_key(&leaf.id))
        .grow(1.0)
        .w_full()
        .min_h(0.0)
        .fill(t.colors.terminal_bg)
        .clip_rounded(0.0)
        .child(match active_tab(leaf) {
            Some(tab) => (ctx.tab)(tab).unwrap_or_else(|| placeholder(ctx, tab)),
            None => col().w_full().grow(1.0).min_h(0.0).center().child(empty()),
        });
    let dragging = ws.drag.as_ref().filter(|d| d.moving);
    if let Some(drag) = dragging.filter(|_| !fullscreen) {
        content = content.child(drop_zone(ctx, &leaf.id, drag, cx));
    }
    let mut frame = col()
        .key(format!("pane-{}", leaf.id))
        .w_full()
        .h_full()
        .min_w(0.0)
        .min_h(0.0)
        .on_press(move |s: &mut S| lens(s).focus_pane(&pane_id))
        .child(bar)
        .child(content);
    if active && (fullscreen || ws.leaf_count() > 1) {
        frame = frame.child(
            el().key(format!("pane-ring-{}", leaf.id))
                .pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0))
                .radius(if fullscreen { 12.0 } else { 0.0 })
                .border(1.0, t.colors.primary.alpha(0.3))
                .a11y_hidden(),
        );
    }
    frame
}

fn active_tab(leaf: &Leaf) -> Option<&Tab> {
    leaf.tabs
        .iter()
        .find(|tab| Some(tab.id()) == leaf.active_tab_id.as_deref())
}

/// Until the tab kinds are ported, a pane shows its active tab's icon and name.
fn placeholder<S: 'static>(ctx: &Ctx<'_, S>, tab: &Tab) -> El {
    let (glyph, label) = identity(ctx, tab);
    col()
        .key(format!("tab-body-{}", tab.id()))
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .center()
        .gap(10.0)
        .child(icon(glyph, 28.0).color(ctx.t.colors.muted_foreground))
        .child(
            text(label)
                .font(sans(13.0, 400.0, 20.0))
                .color(ctx.t.colors.muted_foreground),
        )
}

fn file_name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

/// A tab's icon and label, as each kind's `*TabInline` draws them before its data loads.
fn identity<S>(ctx: &Ctx<'_, S>, tab: &Tab) -> (&'static Icon, String) {
    match tab {
        Tab::Editor { file_path, .. } => {
            let name = file_name(file_path);
            (i::file(name, ctx.dark), name.to_owned())
        }
        Tab::Terminal { terminal_id, .. } => (&i::TERMINAL, (ctx.title)(tab).unwrap_or_else(|| terminal_id.clone())),
        Tab::Browser { .. } => (&i::GLOBE, (ctx.title)(tab).unwrap_or_else(|| "New Tab".to_owned())),
        Tab::Git { .. } => (&i::GIT_BRANCH, "Git Changes".to_owned()),
        Tab::GitHistory { .. } => (&i::HISTORY, "Git History".to_owned()),
        Tab::AgentChat { .. } => (&i::CIRCLE_DOT, (ctx.title)(tab).unwrap_or_else(|| "New chat".to_owned())),
        Tab::GithubItem { number, title, .. } => (
            &i::GITHUB,
            if title.is_empty() {
                format!("#{number}")
            } else {
                format!("#{number} {title}")
            },
        ),
        Tab::Diff { path, staged, .. } => {
            let name = file_name(path);
            (
                &i::FILE_DIFF,
                if *staged {
                    format!("{name} (staged)")
                } else {
                    name.to_owned()
                },
            )
        }
        Tab::CommitDiff { path, commit, .. } => (
            &i::FILE_DIFF,
            format!("{} @ {}", file_name(path), &commit[..commit.len().min(7)]),
        ),
        Tab::Canvas { doc_path, .. } => (&i::CANVAS, file_name(doc_path).to_owned()),
        Tab::Automations { .. } => (&i::CALENDAR_CLOCK, "Automations".to_owned()),
    }
}

/// `WorkspaceTabBar`: `leading`, the tabs, drag space, then `+` and (with more than one
/// pane) the focus toggle, on a 44 pt row with a bottom hairline.
fn tab_bar<S: 'static>(ctx: &Ctx<'_, S>, leaf: &Leaf, leading: Vec<El>, layer: &mut Vec<El>) -> El {
    let t = ctx.t;
    let has_leading = !leading.is_empty();
    let mut list = row()
        .role("tablist")
        .h_full()
        .items_center()
        .gap(2.0)
        .px(4.0);
    for tab in &leaf.tabs {
        list = list.child(tab_chrome(ctx, leaf, tab));
    }
    let mut actions = row().h_full().items_center().gap(4.0).pl(4.0);
    actions = actions.child(new_tab_menu(ctx, &leaf.id, layer));
    if ctx.ws.leaf_count() > 1 {
        let lens = ctx.lens;
        let pane_id = leaf.id.clone();
        let fullscreen = ctx.ws.tree().fullscreen_pane_id.as_deref() == Some(leaf.id.as_str());
        let (glyph, label) = if fullscreen {
            (&i::MINIMIZE2, "Restore pane layout")
        } else {
            (&i::MAXIMIZE2, "Focus pane")
        };
        actions = actions.child(
            crate::shell::rail_button(t, glyph, label, 28.0, 12.0, false)
                .key(format!("pane-focus-{}", leaf.id))
                .on_press(move |s: &mut S| lens(s).toggle_fullscreen(&pane_id)),
        );
    }
    row()
        .key(bar_key(&leaf.id))
        .w_full()
        .h(TOP_ROW)
        .items_center()
        .pl(if has_leading { 0.0 } else { 6.0 })
        .pr(6.0)
        .border_sides([0.0, 0.0, 1.0, 0.0], t.colors.border)
        .children(leading)
        .child(
            row()
                .grow(1.0)
                .min_w(0.0)
                .h_full()
                .child(
                    row()
                        .key(format!("tab-scroll-{}", leaf.id))
                        .shrink()
                        .min_w(0.0)
                        .h_full()
                        .scroll_x()
                        .child(list),
                )
                .child(el().grow(1.0).h_full().window_drag()),
        )
        .child(actions)
}

/// `TabChrome`: a 32 pt tab, the active one a raised keycap; the close slot shows the
/// dirty dot until hovered, and the close button on hover or while active.
/// `CountBadge` at `h-4 min-w-[14px] text-3xs`.
fn tab_badge<S>(ctx: &Ctx<'_, S>, tab: &Tab) -> Option<El> {
    let count = (ctx.badge)(tab);
    (count > 0).then(|| {
        crate::CountBadge::new(count as u64)
            .text_size(crate::type_::MICRO)
            .build(ctx.t)
            .key(format!("tab-badge-{}", tab.id()))
            .h(16.0)
            .min_w(14.0)
    })
}

fn tab_chrome<S: 'static>(ctx: &Ctx<'_, S>, leaf: &Leaf, tab: &Tab) -> El {
    let t = ctx.t;
    let c = &t.colors;
    let ws = ctx.ws;
    let active = leaf.active_tab_id.as_deref() == Some(tab.id());
    let (glyph, label) = identity(ctx, tab);
    let dirty = match tab {
        Tab::Editor { file_path, .. } => ws.files.get(file_path).is_some_and(|f| f.needs_attention()),
        _ => false,
    };
    let ink = if active {
        c.secondary_foreground
    } else {
        c.muted_foreground
    };
    let drag = ws.drag.as_ref();
    let dragging = drag.is_some_and(|d| d.moving && d.tab == tab.id());
    let indicator = drag.and_then(|d| match &d.target {
        Some(DropTarget::Bar {
            pane,
            at: Some((over, side)),
        }) if d.moving && *pane == leaf.id && d.source == leaf.id && over == tab.id() && d.tab != tab.id() => {
            Some(*side)
        }
        _ => None,
    });

    let lens = ctx.lens;
    let tab_id = tab.id().to_owned();
    let close_label = "Close tab";
    let mut close_slot = row().w(24.0).h(32.0).center();
    if dirty {
        close_slot = close_slot.child(
            el().key(format!("tab-dirty-{}", tab.id()))
                .size(8.0, 8.0)
                .radius(4.0)
                .fill(c.foreground.alpha(0.7))
                .pin(Some(8.0), Some(12.0), None, None)
                .group_opacity(1.0, 0.0)
                .a11y_hidden(),
        );
    }
    let close_ink = icon(&i::X, 11.0)
        .color(c.muted_foreground)
        .group_color(c.foreground)
        .a11y_hidden();
    let close_id = tab_id.clone();
    let close_pane = leaf.id.clone();
    let close = row()
        .key(format!("tab-close-{}", tab.id()))
        .role("button")
        .label(close_label)
        .help(close_label)
        .size(16.0, 16.0)
        .radius(6.0)
        .center()
        .focusable(active)
        .group_opacity(if active && !dirty { 1.0 } else { 0.0 }, 1.0)
        .child(close_ink)
        .on_press(move |s: &mut S| {
            let ws = lens(s);
            ws.focus_pane(&close_pane);
            ws.close(&close_id);
        });
    close_slot = close_slot.child(close);

    let mut chrome = row()
        .key(tab_key(tab.id()))
        .role("tab")
        .label(label.clone())
        .checked(active)
        .h(32.0)
        .min_w(100.0)
        .max_w(200.0)
        .pl(10.0)
        .pr(6.0)
        .radius(8.0)
        .items_center()
        .hover_group()
        .focusable(active)
        .child(
            row()
                .grow(1.0)
                .min_w(0.0)
                .items_center()
                .gap(8.0)
                .child(icon(glyph, 12.0).color(ink).group_color(if active { ink } else { c.foreground }).a11y_hidden())
                .child(
                    text(label)
                        .font(sans(12.0, 500.0, 12.0))
                        .color(ink)
                        .group_color(if active { ink } else { c.foreground })
                        .single_line()
                        .truncate()
                        .min_w(0.0),
                )
                .children(tab_badge(ctx, tab)),
        )
        .child(close_slot);
    chrome = if active {
        shadows(chrome.fill(c.keycap_fill), &t.shadows.keycap_shadow)
    } else {
        chrome.hover(c.secondary)
    };
    if dragging {
        chrome = chrome.opacity(0.5).scale(0.98);
    }
    if let Some(side) = indicator {
        let line = el()
            .w(2.0)
            .radius(1.0)
            .fill(c.primary_fill)
            .a11y_hidden();
        chrome = chrome.child(match side {
            Side::Before => line.pin(Some(0.0), Some(4.0), None, Some(4.0)),
            Side::After => line.pin(None, Some(4.0), Some(0.0), Some(4.0)),
        });
    }
    let (select_pane, select_tab) = (leaf.id.clone(), tab_id.clone());
    chrome = chrome.on_press(move |s: &mut S| lens(s).select(&select_pane, &select_tab));
    let pane_id = leaf.id.clone();
    let geometry = Rc::clone(&ctx.geometry);
    chrome.on_drag(move |s: &mut S, d: &mut Drag, _: &mut Tx| {
        let ws = lens(s);
        let point = d.window_point();
        match d.phase {
            Phase::Down => {
                // `PaneContent`'s mousedown: any press in a pane makes it the active one.
                ws.focus_pane(&pane_id);
                ws.drag = Some(TabDrag {
                    tab: tab_id.clone(),
                    source: pane_id.clone(),
                    point,
                    moving: false,
                    target: None,
                });
            }
            Phase::Move => {
                let fullscreen = ws.tree().fullscreen_pane_id.clone();
                if let Some(drag) = ws.drag.as_mut() {
                    drag.point = point;
                    drag.moving |= d.dx.hypot(d.dy) >= SLOP;
                    if drag.moving {
                        drag.target = geometry.target(point, fullscreen.as_deref());
                    }
                }
            }
            Phase::Up => {
                let Some(drag) = ws.drag.take() else {
                    return;
                };
                if d.cancelled() {
                    return;
                }
                if let Some(target) = drag.target.as_ref().filter(|_| drag.moving) {
                    ws.drop_tab(&drag.tab, &drag.source, target);
                }
            }
        }
    })
}

/// `DropZoneOverlay`: while a tab drags, the zone under the pointer lights up in the pane.
fn drop_zone<S>(ctx: &Ctx<'_, S>, pane: &str, drag: &TabDrag, cx: &Cx<S>) -> El {
    let overlay = el()
        .key(format!("drop-zones-{pane}"))
        .pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0))
        .a11y_hidden();
    let hovered = match &drag.target {
        Some(DropTarget::Zone { pane: over, position }) if over == pane => Some(*position),
        _ => None,
    };
    let (Some(position), Some([w, h])) = (hovered, cx.layout_size(&content_key(pane))) else {
        return overlay;
    };
    let [left, top, right, bottom] = zone_rect(position);
    let c = &ctx.t.colors;
    overlay.child(
        el().key(format!("drop-zone-{pane}"))
            .pin(Some(left * w), Some(top * h), Some((1.0 - right) * w), Some((1.0 - bottom) * h))
            .radius(2.0)
            .fill(c.primary.alpha(0.1))
            .border(1.0, c.primary.alpha(0.4)),
    )
}

/// `NewTabMenu`: a new chat or terminal in this pane. The menu's items hand the app a
/// [`Request`]; until chats and terminals are ported the app logs them.
fn new_tab_menu<S: 'static>(ctx: &Ctx<'_, S>, pane: &str, layer: &mut Vec<El>) -> El {
    let t = ctx.t;
    let lens = ctx.lens;
    let open = ctx.ws.menu.as_deref() == Some(pane);
    let opener = pane.to_owned();
    let trigger = crate::shell::rail_button(t, &i::PLUS, "New tab", 28.0, 14.0, open)
        .key(format!("new-tab-{pane}"))
        .on_press(move |s: &mut S| {
            let ws = lens(s);
            ws.focus_pane(&opener);
            ws.menu = if ws.menu.as_deref() == Some(opener.as_str()) {
                None
            } else {
                Some(opener.clone())
            };
        });
    let chat_pane = pane.to_owned();
    let terminal_pane = pane.to_owned();
    let browser_pane = pane.to_owned();
    Menu::new(format!("new-tab-menu-{pane}"))
        .width(224.0)
        .item(
            MenuItem::new("New chat")
                .icon(&i::CHAT_EDIT)
                .on_select(move |s: &mut S| {
                    lens(s).new_tab(Request::NewChat {
                        pane: chat_pane.clone(),
                    });
                }),
        )
        .item(
            MenuItem::new("New terminal")
                .icon(&i::TERMINAL)
                .on_select(move |s: &mut S| {
                    lens(s).new_tab(Request::NewTerminal {
                        pane: terminal_pane.clone(),
                    });
                }),
        )
        .item(
            MenuItem::new("New browser tab")
                .icon(&i::GLOBE)
                .on_select(move |s: &mut S| {
                    lens(s).new_tab(Request::NewBrowser {
                        pane: browser_pane.clone(),
                    });
                }),
        )
        .item(
            MenuItem::new("New terminal with")
                .icon(&i::TERMINAL)
                .submenu(
                    Menu::new(format!("new-tab-shells-{pane}"))
                        .label("Shells")
                        .item(MenuItem::new("Detecting shells…").disabled(true)),
                ),
        )
        .open(open)
        .on_close(move |s: &mut S| {
            lens(s).menu = None;
        })
        .build(t, layer, trigger)
}

/// `WorkspaceCloseDialogs`' dirty-file confirm: Save, Discard or Cancel.
fn unsaved_dialog<S: 'static>(ctx: &Ctx<'_, S>, layer: &mut Vec<El>) {
    let Some(path) = ctx.ws.confirm_close.as_deref() else {
        return;
    };
    let lens = ctx.lens;
    ConfirmDialog::new(
        "unsaved-changes",
        "Unsaved Changes",
        format!("Save changes to \"{}\" before closing?", file_name(path)),
    )
    .confirm_label("Save")
    .cancel_label("Cancel")
    .secondary("Discard", move |s: &mut S| lens(s).discard_and_close())
    .open(true)
    .on_cancel(move |s: &mut S| lens(s).cancel_close())
    .on_confirm(move |s: &mut S| lens(s).save_and_close())
    .build(ctx.t, layer);
}
