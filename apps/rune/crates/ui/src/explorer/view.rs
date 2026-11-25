//! The Explorer's drawing: the tree rows (rune-kit `TreeItem` rows over `model::explorer`'s
//! visible rows), the loading, error and no-project states (`explorer-states.tsx`), the header
//! actions (`explorer-header.tsx`), the row menu (`FileTreeContextMenu.tsx`) and the delete
//! confirm.

use std::hash::{Hash, Hasher};

use model::explorer::{Entry, Event, Inline, Kind, Listing, Nav, Row};
use rune::{Drag, El, GrowthSpec, Key, Phase, ScrollSpec, Tx, col, el, icon, row, text, text_input};
use rune_kit::{TreeItem, TreeRow, TreeStyle};
use theme::Tokens;

use super::{Explorer, FileDrag, MenuAt};
use crate::overlay::{ConfirmDialog, ConfirmVariant, Menu, MenuItem};
use crate::type_::sans;

type Get<S> = fn(&S) -> &Explorer;
type Lens<S> = fn(&mut S) -> &mut Explorer;

pub const TREE_KEY: &str = "explorer-tree";
pub const NAME_KEY: &str = "explorer-name";
const ROW: f32 = 28.0;
const MENU: &str = "explorer-menu";
const MORE: &str = "explorer-more";
const DELETE: &str = "explorer-delete";
/// A press that travels this far drags the file instead.
const SLOP: f32 = 4.0;

/// What the explorer asks of the app beyond its effects.
pub struct Hooks<S> {
    /// A file row's drag past the slop, each phase: the app shows the pane drop zones and,
    /// on release unless cancelled, lands the drop.
    pub file_drag: fn(&mut S, &FileDrag, Phase, bool),
    /// A folder menu's Open in Terminal.
    pub open_terminal: fn(&mut S, &str),
}

impl<S> Clone for Hooks<S> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<S> Copy for Hooks<S> {}

fn glyph(name: &str) -> &'static rune::Icon {
    theme::icons::named(name).unwrap_or(&theme::icons::X)
}

#[must_use]
pub fn row_key(path: &str) -> String {
    format!("explorer-row-{path}")
}

/// `treeRowPaddingLeft`.
fn indent(depth: u32) -> f32 {
    4.0 + depth as f32 * 14.0
}

/// Runs `event`, then scrolls to any row it revealed and focuses the field it opened.
fn act<S>(s: &mut S, lens: Lens<S>, event: Event, tx: &mut Tx) {
    let x = lens(s);
    let had_field = x.tree.inline.is_some();
    x.step(event);
    for path in x.take_reveals() {
        tx.scroll_into_view(row_key(&path));
    }
    match (had_field, x.tree.inline.is_some()) {
        (false, true) => tx.focus(NAME_KEY, false),
        (true, false) => tx.focus(TREE_KEY, false),
        _ => {}
    }
}

/// The panel body under the header.
pub fn view<S: 'static>(s: &S, get: Get<S>, lens: Lens<S>, t: &'static Tokens, hooks: Hooks<S>) -> El {
    let x = get(s);
    let c = &t.colors;
    let Some(root) = x.tree.root.as_deref() else {
        return state(t, "Folder", c.muted_foreground, "No project open", "Pick a project on the left to see its files.", None);
    };
    match x.tree.listings.get(root) {
        Some(Listing::Ready(_)) => {}
        Some(Listing::Failed(message)) => {
            let retry = crate::Button::new("Try again")
                .variant(crate::ButtonVariant::Outline)
                .size(crate::ButtonSize::Sm)
                .build(t)
                .key("explorer-retry")
                .on_press_tx(move |s: &mut S, tx| act(s, lens, Event::Refresh, tx));
            return state(t, "AlertTriangle", c.destructive, "Couldn\u{2019}t read this folder", message, Some(retry));
        }
        _ => return loading(t),
    }
    let rows = x.tree.rows();
    let mut version = std::collections::hash_map::DefaultHasher::new();
    for r in &rows {
        match r {
            Row::Entry { entry, .. } => entry.path.hash(&mut version),
            Row::Create { .. } => "\0create".hash(&mut version),
        }
    }
    let list = col()
        .key("explorer-rows")
        .w_full()
        .px(8.0)
        .pb(8.0)
        .growth(GrowthSpec::default())
        .virtual_rows_version(version.finish())
        .virtual_rows(
            rows.len(),
            ROW,
            move |s: &S, i| match get(s).tree.rows().get(i) {
                Some(Row::Entry { entry, .. }) => row_key(&entry.path).into(),
                _ => "explorer-create".into(),
            },
            move |s: &S, i| match get(s).tree.rows().get(i) {
                Some(r) => tree_row(get(s), lens, t, r, hooks),
                None => el(),
            },
        );
    col()
        .key(TREE_KEY)
        .role("tree")
        .label("Files")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .focusable(true)
        .on_key(move |s: &mut S, key, tx| on_key(s, lens, key, tx))
        .child(col().key("explorer-viewport").w_full().grow(1.0).min_h(0.0).scroll(ScrollSpec { stick_bottom: false }).child(list))
}

fn on_key<S>(s: &mut S, lens: Lens<S>, key: &rune::KeyEvent, tx: &mut Tx) -> bool {
    let m = key.mods;
    if m.cmd || m.ctrl || m.alt {
        return false;
    }
    let selected = lens(s).tree.selected.clone();
    let event = match &key.key {
        Key::Up => Event::Nav(Nav::Up),
        Key::Down => Event::Nav(Nav::Down),
        Key::Left => Event::Nav(Nav::Left),
        Key::Right => Event::Nav(Nav::Right),
        Key::Home => Event::Nav(Nav::Home),
        Key::End => Event::Nav(Nav::End),
        Key::Enter => Event::Activate,
        Key::Function(2) => match selected {
            Some(path) => Event::StartRename(path),
            None => return false,
        },
        Key::Delete => match selected {
            Some(path) => Event::RequestDelete(path),
            None => return false,
        },
        _ => return false,
    };
    act(s, lens, event, tx);
    true
}

/// `ExplorerNoProject` and `ExplorerError`: a 32 pt glyph tile, a title, a line, an action.
fn state(t: &Tokens, glyph_name: &str, ink: rune::Color, title: &str, line: &str, action: Option<El>) -> El {
    let c = &t.colors;
    let tile = row()
        .size(32.0, 32.0)
        .center()
        .radius(8.0)
        .border(1.0, c.border)
        .fill(c.card)
        .child(icon(glyph(glyph_name), 16.0).color(ink).a11y_hidden());
    let mut out = col()
        .key("explorer-state")
        .w_full()
        .items_center()
        .gap(8.0)
        .px(24.0)
        .py(40.0)
        .child(tile)
        .child(text(title.to_owned()).font(sans(12.0, 500.0, 16.0)).color(c.foreground))
        .child(text(line.to_owned()).font(sans(11.0, 400.0, 16.0)).color(c.muted_foreground).align(rune::Align::Center));
    if let Some(action) = action {
        out = out.child(action.mt(4.0));
    }
    out
}

/// `ExplorerLoading`: the fixed skeleton rows.
fn loading(t: &Tokens) -> El {
    const ROWS: [(u32, f32); 8] = [(0, 72.0), (0, 96.0), (1, 84.0), (1, 120.0), (1, 64.0), (0, 88.0), (0, 110.0), (0, 76.0)];
    let bar = t.colors.foreground.alpha(0.06);
    let mut out = col().key("explorer-loading").role("status").label("Loading files…").w_full().px(8.0);
    for (depth, width) in ROWS {
        out = out.child(
            row()
                .h(ROW)
                .items_center()
                .gap(6.0)
                .pl(indent(depth) + 16.0)
                .child(el().size(14.0, 14.0).radius(2.0).fill(bar))
                .child(el().size(width, 8.0).radius(4.0).fill(bar)),
        );
    }
    out
}

fn tree_style(t: &Tokens, k: &rune_kit::Kit) -> TreeStyle {
    let c = &t.colors;
    let mut style = TreeStyle::from_kit(k)
        .hover(c.foreground.alpha(0.03))
        .radius(6.0)
        .gap(6.0)
        .label(sans(12.0, 400.0, 16.0), c.secondary_foreground, c.foreground)
        .chevron_colors(c.muted_foreground.alpha(0.7), c.muted_foreground.alpha(0.7));
    style.row_height = ROW;
    style.indent = 14.0;
    style.padding = 4.0;
    style.guide = c.border;
    style.selected = c.keycap_fill;
    style
}

fn tree_row<S: 'static>(x: &Explorer, lens: Lens<S>, t: &'static Tokens, r: &Row<'_>, hooks: Hooks<S>) -> El {
    match *r {
        Row::Create { kind, depth } => name_row(x, lens, t, depth, kind == Kind::Folder, "explorer-create"),
        Row::Entry { entry, depth, renaming: true, .. } => name_row(x, lens, t, depth, entry.dir, &row_key(&entry.path)),
        Row::Entry { entry, depth, open, .. } => entry_row(x, lens, t, entry, depth, open, hooks),
    }
}

fn entry_row<S: 'static>(x: &Explorer, lens: Lens<S>, t: &'static Tokens, entry: &Entry, depth: u32, open: bool, hooks: Hooks<S>) -> El {
    let c = &t.colors;
    let k = theme::kit(&t.family, t.dark);
    let selected = x.tree.selected.as_deref() == Some(&entry.path);
    let glyph_icon = if entry.dir {
        if open { glyph("FolderOpen") } else { glyph("Folder") }
    } else {
        rune_kit::file_icon(&entry.name)
    };
    let mut item = TreeItem::new(entry.name.clone()).icon(glyph_icon).icon_color(c.muted_foreground);
    if entry.ignored {
        item = item.label_color(c.muted_foreground.alpha(0.6));
    }
    let state = TreeRow::new(&entry.path, depth).expanded(entry.dir.then_some(open)).selected(selected);
    let (pressed, menu_path, dragged) = (entry.path.clone(), entry.path.clone(), entry.path.clone());
    let mut e = item
        .build(k, &state, &tree_style(t, k))
        .key(row_key(&entry.path))
        .on_press_tx(move |s: &mut S, tx| {
            act(s, lens, Event::Press(pressed.clone()), tx);
            tx.focus(TREE_KEY, false);
        })
        .on_secondary_press(move |s: &mut S, at, _tx| {
            let x = lens(s);
            x.step(Event::Select(menu_path.clone()));
            x.menu = Some(MenuAt { path: menu_path.clone(), at });
        });
    if !entry.dir {
        e = e.on_drag(move |s: &mut S, d: &mut Drag, _tx| {
            let x = lens(s);
            let point = d.window_point();
            match d.phase {
                Phase::Down => x.drag = Some(FileDrag { path: dragged.clone(), point, moving: false }),
                Phase::Move => {
                    if let Some(drag) = x.drag.as_mut() {
                        drag.point = point;
                        drag.moving |= d.dx.hypot(d.dy) >= SLOP;
                    }
                }
                Phase::Up => {}
            }
            let drag = if d.phase == Phase::Up { x.drag.take() } else { x.drag.clone() };
            if let Some(drag) = drag.filter(|g| g.moving) {
                (hooks.file_drag)(s, &drag, d.phase, d.cancelled());
            }
        });
    }
    e
}

/// The create or rename row (`InlineCreateRow`, `InlineNameInput`).
fn name_row<S: 'static>(x: &Explorer, lens: Lens<S>, t: &'static Tokens, depth: u32, dir: bool, key: &str) -> El {
    let c = &t.colors;
    let (label, placeholder) = match &x.tree.inline {
        Some(Inline::Rename { .. }) => ("New name", "New name..."),
        Some(Inline::Create { kind: Kind::Folder, .. }) => ("New folder name", "Folder name..."),
        _ => ("New file name", "File name..."),
    };
    let field = text_input(&x.name, sans(12.0, 400.0, 16.0))
        .key(NAME_KEY)
        .single_line()
        .label(label.to_owned())
        .placeholder(placeholder.to_owned(), c.muted_foreground.alpha(0.7))
        .color(c.foreground)
        .grow(1.0)
        .min_w(0.0)
        .h(24.0)
        .px(6.0)
        .radius(4.0)
        .border(1.0, c.border)
        .fill(c.card)
        .on_edit(move |s: &mut S| &mut lens(s).name)
        .on_input_key(move |s: &mut S, key, tx| match key.key {
            Key::Enter => {
                let x = lens(s);
                let had = x.tree.inline.clone();
                x.submit();
                if x.tree.inline.is_none() && had.is_some() {
                    tx.focus(TREE_KEY, false);
                }
                for path in x.take_reveals() {
                    tx.scroll_into_view(row_key(&path));
                }
                true
            }
            Key::Escape => {
                act(s, lens, Event::CancelInline, tx);
                true
            }
            _ => false,
        });
    let glyph_icon = if dir { glyph("Folder") } else { glyph("File") };
    row()
        .key(key.to_owned())
        .w_full()
        .h(ROW)
        .items_center()
        .pl(indent(depth))
        .pr(6.0)
        .child(el().size(16.0, 16.0))
        .child(icon(glyph_icon, 16.0).color(c.muted_foreground).mr(6.0).a11y_hidden())
        .child(field)
}

/// The panel header's buttons (`ExplorerHeader`): New File, New Folder, Collapse All, More.
pub fn header_actions<S: 'static>(s: &S, get: Get<S>, lens: Lens<S>, t: &'static Tokens, layer: &mut Vec<El>) -> Vec<El> {
    let x = get(s);
    let disabled = !matches!(x.tree.root.as_deref().and_then(|r| x.tree.listings.get(r)), Some(Listing::Ready(_)));
    let button = |name: &str, label: &str| {
        let b = crate::shell::rail_button(t, glyph(name), label, 24.0, 14.0, false).key(format!("explorer-{}", label.to_lowercase().replace(' ', "-")));
        if disabled { b.opacity(0.5).focusable(false) } else { b }
    };
    let create = |kind: Kind| move |s: &mut S, tx: &mut Tx| act(s, lens, Event::StartCreate { parent: None, kind }, tx);
    let mut out = vec![button("FilePlus", "New File"), button("FolderPlus", "New Folder"), button("ChevronsDownUp", "Collapse All")];
    if !disabled {
        out[0] = std::mem::replace(&mut out[0], el()).on_press_tx(create(Kind::File));
        out[1] = std::mem::replace(&mut out[1], el()).on_press_tx(create(Kind::Folder));
        out[2] = std::mem::replace(&mut out[2], el()).on_press_tx(move |s: &mut S, tx| act(s, lens, Event::CollapseAll, tx));
    }
    let more = button("MoreHorizontal", "More explorer actions");
    let more = if disabled { more } else { more.on_press(move |s: &mut S| lens(s).more = !lens(s).more) };
    let menu = Menu::new(MORE)
        .width(192.0)
        .placement(crate::overlay::Placement::BottomEnd)
        .open(x.more && !disabled)
        .item(MenuItem::new("Refresh").icon(glyph("RefreshCw")).on_select_tx(move |s: &mut S, tx: &mut Tx| {
            lens(s).more = false;
            act(s, lens, Event::Refresh, tx);
        }))
        .on_close(move |s: &mut S| lens(s).more = false);
    out.push(menu.build(t, layer, more));
    out
}

/// The row menu and the delete confirm, when open.
pub fn layer<S: 'static>(s: &S, get: Get<S>, lens: Lens<S>, t: &'static Tokens, hooks: Hooks<S>, layer: &mut Vec<El>) {
    let x = get(s);
    if let Some(MenuAt { path, at }) = &x.menu
        && let Some(entry) = x.tree.entry(path)
    {
        row_menu(entry, *at, lens, hooks).build(t, layer, el());
    }
    if let Some(entry) = &x.tree.deleting {
        ConfirmDialog::new(DELETE, model::explorer::delete_title(entry), model::explorer::delete_message(entry))
            .variant(ConfirmVariant::Danger)
            .confirm_label("Delete")
            .loading(x.tree.busy)
            .open(true)
            .on_cancel(move |s: &mut S| lens(s).step(Event::CancelDelete))
            .on_confirm(move |s: &mut S| lens(s).step(Event::ConfirmDelete))
            .build(t, layer);
    }
}

/// `FileTreeContextMenuContent` for one entry: a folder's create items, Rename, Copy Path, Open
/// in Terminal for a folder, then Delete.
fn row_menu<S: 'static>(entry: &Entry, at: rune::PointerPress, lens: Lens<S>, hooks: Hooks<S>) -> Menu {
    let close = move |s: &mut S| lens(s).menu = None;
    let path = entry.path.clone();
    let mut menu = Menu::new(MENU).width(224.0).at(Some(at)).on_close(close);
    if entry.dir {
        let (file_in, folder_in) = (path.clone(), path.clone());
        menu = menu
            .item(MenuItem::new("New File").icon(glyph("FilePlus")).on_select_tx(move |s: &mut S, tx: &mut Tx| {
                close(s);
                act(s, lens, Event::StartCreate { parent: Some(file_in.clone()), kind: Kind::File }, tx);
            }))
            .item(MenuItem::new("New Folder").icon(glyph("FolderPlus")).on_select_tx(move |s: &mut S, tx: &mut Tx| {
                close(s);
                act(s, lens, Event::StartCreate { parent: Some(folder_in.clone()), kind: Kind::Folder }, tx);
            }))
            .separator();
    }
    let (renamed, copied, deleted, terminal) = (path.clone(), path.clone(), path.clone(), path);
    menu = menu
        .item(MenuItem::new("Rename").icon(glyph("Edit")).on_select_tx(move |s: &mut S, tx: &mut Tx| {
            close(s);
            act(s, lens, Event::StartRename(renamed.clone()), tx);
        }))
        .item(MenuItem::new("Copy Path").icon(glyph("Copy")).on_select_tx(move |s: &mut S, tx: &mut Tx| {
            close(s);
            tx.copy(copied.clone());
        }));
    if entry.dir {
        menu = menu.separator().item(MenuItem::new("Open in Terminal").icon(glyph("Terminal")).on_select(move |s: &mut S| {
            close(s);
            (hooks.open_terminal)(s, &terminal);
        }));
    }
    menu.separator().item(MenuItem::new("Delete…").icon(glyph("Trash2")).destructive().on_select(move |s: &mut S| {
        close(s);
        lens(s).step(Event::RequestDelete(deleted.clone()));
    }))
}
