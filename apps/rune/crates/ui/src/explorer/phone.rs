//! The phone's Files sheet (`components/mobile/MobileFileExplorer.tsx`): one folder at a time,
//! a back button and the folder path over a toolbar (Refresh, New file, New folder), then the
//! folder's rows. A folder row goes into the folder, a file row opens the file, and each row's
//! ⋯ opens its actions (the app's bottom sheet: Rename, Duplicate, Delete).

use model::explorer::{Entry, Event, Inline, Kind, Listing, parent_of};
use rune::{El, Key, ScrollSpec, Tx, col, el, icon, row, text, text_input};
use theme::Tokens;

use super::Explorer;
use crate::mobile::TOUCH;
use crate::overlay::{Close, scrim};
use crate::type_::sans;

type Get<S> = fn(&S) -> &Explorer;
type Lens<S> = fn(&mut S) -> &mut Explorer;

pub const FILES: &str = "phone-files";
pub const PHONE_NAME_KEY: &str = "phone-files-name";

/// What the sheet asks of the app.
pub struct PhoneHooks<S> {
    pub close: fn(&mut S),
    /// A row's ⋯: open the actions sheet for this path.
    pub actions: fn(&mut S, &str),
    pub open_file: fn(&mut S, &str),
}

impl<S> Clone for PhoneHooks<S> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<S> Copy for PhoneHooks<S> {}

fn glyph(name: &str) -> &'static rune::Icon {
    theme::icons::named(name).unwrap_or(&theme::icons::X)
}

fn name_of(path: &str) -> &str {
    path.trim_end_matches(['/', '\\']).rsplit(['/', '\\']).next().filter(|n| !n.is_empty()).unwrap_or(path)
}

impl Explorer {
    /// The folder the Files sheet shows: the last one, while it is under the project, else the
    /// project's folder.
    #[must_use]
    pub fn phone_folder(&self) -> Option<String> {
        let root = self.tree.root.as_deref()?;
        match self.phone_dir.as_deref() {
            Some(dir) if dir == root || dir.strip_prefix(root).is_some_and(|r| r.starts_with(['/', '\\'])) => Some(dir.to_owned()),
            _ => Some(root.to_owned()),
        }
    }

    /// The sheet opened or moved to `dir`.
    pub fn browse(&mut self, dir: String) {
        self.tree.inline = None;
        self.phone_dir = Some(dir.clone());
        self.step(Event::Browse(dir));
    }

    /// The sheet opened: show the last folder, listed again (the web has no watcher).
    pub fn open_phone(&mut self) {
        if let Some(dir) = self.phone_folder() {
            self.tree.listings.remove(&dir);
            self.browse(dir);
        }
    }
}

fn ghost(t: &Tokens, name: &str, label: &str, size: f32, disabled: bool) -> El {
    let c = &t.colors;
    let b = row()
        .role("button")
        .label(label.to_owned())
        .size(TOUCH, TOUCH)
        .center()
        .radius(8.0)
        .child(icon(glyph(name), size).color(c.foreground).a11y_hidden());
    if disabled { b.opacity(0.5) } else { b.hover(c.foreground.alpha(0.06)).focusable(true) }
}

/// The Files sheet, onto `layer`.
pub fn files_sheet<S: 'static>(s: &S, get: Get<S>, lens: Lens<S>, t: &'static Tokens, window: (f32, f32), hooks: PhoneHooks<S>, layer: &mut Vec<El>) {
    let x = get(s);
    let c = &t.colors;
    let close = Close::new(hooks.close);
    let root = x.tree.root.clone();
    let dir = x.phone_folder();
    let at_root = dir == root;
    let title = match &dir {
        Some(d) => name_of(d).to_owned(),
        None => "Files".to_owned(),
    };
    let mut back = ghost(t, "ChevronLeft", "Back to parent folder", 19.0, at_root).key("phone-files-back").ml(-8.0);
    if !at_root && let Some(d) = dir.clone() {
        back = back.on_press(move |s: &mut S| lens(s).browse(parent_of(&d).to_owned()));
    }
    let header = row()
        .key("phone-files-header")
        .w_full()
        .items_center()
        .gap(4.0)
        .padding(8.0)
        .pr(48.0)
        .border_sides([0.0, 0.0, 1.0, 0.0], c.border.alpha(0.6))
        .child(back)
        .child(
            col()
                .min_w(0.0)
                .child(text(title.clone()).font(sans(16.0, 600.0, 24.0)).color(c.foreground).single_line().truncate())
                .child(crumbs(t, lens, root.as_deref(), dir.as_deref())),
        );
    let busy = dir.is_none();
    let (refresh_dir, file_dir, folder_dir) = (dir.clone(), dir.clone(), dir.clone());
    let mut refresh = ghost(t, "RefreshCw", "Refresh current folder", 16.0, busy).key("phone-files-refresh");
    let mut new_file = ghost(t, "FilePlus", "New file", 16.0, busy).key("phone-files-new-file");
    let mut new_folder = ghost(t, "FolderPlus", "New folder", 16.0, busy).key("phone-files-new-folder");
    if !busy {
        refresh = refresh.on_press(move |s: &mut S| {
            if let Some(d) = &refresh_dir {
                let x = lens(s);
                x.tree.listings.remove(d);
                x.step(Event::Browse(d.clone()));
            }
        });
        new_file = new_file.on_press_tx(move |s: &mut S, tx: &mut Tx| start_create(s, lens, file_dir.clone(), Kind::File, tx));
        new_folder = new_folder.on_press_tx(move |s: &mut S, tx: &mut Tx| start_create(s, lens, folder_dir.clone(), Kind::Folder, tx));
    }
    let toolbar = row()
        .key("phone-files-toolbar")
        .w_full()
        .items_center()
        .gap(4.0)
        .padding(8.0)
        .border_sides([0.0, 0.0, 1.0, 0.0], c.border.alpha(0.6))
        .child(refresh)
        .child(new_file)
        .child(new_folder);
    let body = body(x, lens, t, dir.as_deref(), hooks);
    let x_button = close.press(
        row()
            .key("phone-files-close")
            .role("button")
            .label("Close")
            .size(TOUCH, TOUCH)
            .center()
            .child(icon(glyph("X"), 16.0).color(c.foreground).opacity(0.7)),
    );
    let width = window.0.min(416.0);
    let panel = col()
        .key("phone-files-dialog")
        .role("dialog")
        .label(title)
        .description("Browse project files")
        .w(width)
        .h_full()
        .fill(c.background)
        .border_sides([0.0, 0.0, 0.0, 1.0], c.border)
        .child(header)
        .child(toolbar)
        .child(col().key("phone-files-body").w_full().grow(1.0).min_h(0.0).scroll(ScrollSpec { stick_bottom: false }).pt(4.0).child(body))
        .child(row().key("phone-files-corner").absolute(0.0, 6.0).w_full().pr(4.0).justify_end().child(x_button));
    layer.push(scrim("phone-files-scrim".into(), c.overlay.alpha(0.8)));
    layer.push(row().key("phone-files-layer").absolute(0.0, 0.0).w_full().h_full().justify_end().child(close.modal(panel)));
}

fn start_create<S>(s: &mut S, lens: Lens<S>, dir: Option<String>, kind: Kind, tx: &mut Tx) {
    let x = lens(s);
    x.tree.inline = None;
    x.step(Event::StartCreate { parent: dir, kind });
    tx.focus(PHONE_NAME_KEY, false);
}

/// The folder path: at the root "Project files", else each folder above as a button.
fn crumbs<S: 'static>(t: &Tokens, lens: Lens<S>, root: Option<&str>, dir: Option<&str>) -> El {
    let c = &t.colors;
    let line = |s: &str| text(s.to_owned()).font(sans(12.0, 400.0, 16.0)).color(c.muted_foreground).single_line();
    let (Some(root), Some(dir)) = (root, dir) else { return line("") };
    if root == dir {
        return line("Project files");
    }
    let mut ancestors = vec![root.to_owned()];
    let mut at = root.to_owned();
    for part in dir[root.len()..].split(['/', '\\']).filter(|p| !p.is_empty()) {
        at = model::explorer::join(&at, part);
        ancestors.push(at.clone());
    }
    ancestors.pop();
    let mut out = row().key("phone-files-crumbs").role("navigation").label("Folder path").items_center().gap(2.0).min_w(0.0);
    for path in ancestors {
        let to = path.clone();
        out = out
            .child(line(name_of(&path)).px(4.0).role("button").on_press(move |s: &mut S| lens(s).browse(to.clone())))
            .child(icon(glyph("ChevronRight"), 12.0).color(c.muted_foreground).a11y_hidden());
    }
    out.child(text(name_of(dir).to_owned()).font(sans(12.0, 400.0, 16.0)).color(c.foreground).px(4.0).single_line())
}

fn note(t: &Tokens, s: &str) -> El {
    row().w_full().justify_center().px(16.0).py(32.0).child(text(s.to_owned()).font(sans(14.0, 400.0, 20.0)).color(t.colors.muted_foreground))
}

fn body<S: 'static>(x: &Explorer, lens: Lens<S>, t: &'static Tokens, dir: Option<&str>, hooks: PhoneHooks<S>) -> El {
    let Some(dir) = dir else { return note(t, "No active project") };
    if let Some(Inline::Create { parent, kind }) = &x.tree.inline
        && parent == dir
    {
        let (label, placeholder) = if *kind == Kind::File { ("New file name", "new-file.txt") } else { ("New folder name", "new-folder") };
        return col().w_full().px(8.0).py(4.0).child(name_field(x, lens, t, label, placeholder));
    }
    match x.tree.listings.get(dir) {
        Some(Listing::Ready(entries)) if entries.is_empty() => note(t, "Empty"),
        Some(Listing::Ready(entries)) => {
            let mut list = col().key("phone-files-rows").role("list").label(format!("Files in {}", name_of(dir))).w_full();
            for entry in entries {
                list = list.child(entry_row(x, lens, t, entry, hooks));
            }
            list
        }
        Some(Listing::Failed(message)) => col()
            .w_full()
            .items_center()
            .gap(8.0)
            .px(16.0)
            .py(32.0)
            .child(text(message.clone()).font(sans(14.0, 400.0, 20.0)).color(t.colors.muted_foreground))
            .child({
                let d = dir.to_owned();
                crate::Button::new("Retry")
                    .variant(crate::ButtonVariant::Secondary)
                    .size(crate::ButtonSize::Sm)
                    .build(t)
                    .on_press(move |s: &mut S| {
                        let x = lens(s);
                        x.tree.listings.remove(&d);
                        x.step(Event::Browse(d.clone()));
                    })
            }),
        _ => note(t, "Loading…"),
    }
}

fn name_field<S: 'static>(x: &Explorer, lens: Lens<S>, t: &'static Tokens, label: &str, placeholder: &str) -> El {
    let c = &t.colors;
    text_input(&x.name, sans(14.0, 400.0, 20.0))
        .key(PHONE_NAME_KEY)
        .single_line()
        .label(label.to_owned())
        .placeholder(placeholder.to_owned(), c.muted_foreground)
        .color(c.foreground)
        .w_full()
        .h(TOUCH)
        .px(12.0)
        .radius(6.0)
        .border(1.0, c.input)
        .on_edit(move |s: &mut S| &mut lens(s).name)
        .on_input_key(move |s: &mut S, key, _tx| match key.key {
            Key::Enter => {
                lens(s).submit();
                true
            }
            Key::Escape => {
                lens(s).step(Event::CancelInline);
                true
            }
            _ => false,
        })
}

fn entry_row<S: 'static>(x: &Explorer, lens: Lens<S>, t: &'static Tokens, entry: &Entry, hooks: PhoneHooks<S>) -> El {
    let c = &t.colors;
    if matches!(&x.tree.inline, Some(Inline::Rename { entry: e }) if e.path == entry.path) {
        return col().w_full().padding(4.0).child(name_field(x, lens, t, &format!("Rename {}", entry.name), ""));
    }
    let glyph_icon = if entry.dir { glyph("Folder") } else { rune_kit::file_icon(&entry.name) };
    let mut open = row()
        .key(format!("phone-files-open-{}", entry.path))
        .role("button")
        .label(if entry.dir { format!("Open folder {}", entry.name) } else { format!("Open {}", entry.name) })
        .grow(1.0)
        .min_w(0.0)
        .h(TOUCH)
        .items_center()
        .gap(8.0)
        .child(icon(glyph_icon, 18.0).color(c.muted_foreground).a11y_hidden())
        .child(
            text(entry.name.clone())
                .font(sans(14.0, 400.0, 20.0))
                .color(if entry.ignored { c.muted_foreground } else { c.foreground })
                .grow(1.0)
                .min_w(0.0)
                .single_line()
                .truncate(),
        );
    if entry.dir {
        open = open.child(icon(glyph("ChevronRight"), 16.0).color(c.muted_foreground).a11y_hidden());
    }
    let (tapped, dir, acted) = (entry.path.clone(), entry.dir, entry.path.clone());
    let open = open.on_press(move |s: &mut S| {
        if dir {
            lens(s).browse(tapped.clone());
        } else {
            lens(s).tree.selected = Some(tapped.clone());
            (hooks.open_file)(s, &tapped);
        }
    });
    let more = ghost(t, "MoreHorizontal", &format!("Actions for {}", entry.name), 18.0, false)
        .key(format!("phone-files-more-{}", entry.path))
        .mr(-8.0)
        .on_press(move |s: &mut S| (hooks.actions)(s, &acted));
    row().w_full().px(16.0).h(TOUCH).items_center().gap(8.0).child(open).child(more).child(el().w(0.0))
}

/// The Files sheet's delete confirm (`MobileFileExplorer`'s `AlertDialog`).
pub fn phone_confirm<S: 'static>(s: &S, get: Get<S>, lens: Lens<S>, t: &'static Tokens, layer: &mut Vec<El>) {
    let Some(entry) = &get(s).tree.deleting else { return };
    let message = if entry.dir {
        "This will delete the folder and all its contents. This cannot be undone."
    } else {
        "This will delete the file. This cannot be undone."
    };
    crate::overlay::ConfirmDialog::new("phone-files-delete", format!("Delete {}", entry.name), message)
        .variant(crate::overlay::ConfirmVariant::Danger)
        .confirm_label("Delete")
        .loading(get(s).tree.busy)
        .open(true)
        .on_cancel(move |s: &mut S| lens(s).step(Event::CancelDelete))
        .on_confirm(move |s: &mut S| lens(s).step(Event::ConfirmDelete))
        .build(t, layer);
}
