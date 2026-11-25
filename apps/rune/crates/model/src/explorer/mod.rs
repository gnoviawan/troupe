//! The project's file tree (`stores/file-explorer-store.ts` with `use-explorer-entry-flows.ts`):
//! the folders listed so far, which are open, the selected row, the inline name field that
//! creates or renames an entry, and the delete confirm.
//!
//! [`step`] applies one event and returns the [`Effect`]s the app carries out: list a folder
//! (`GET /fs/ls`), create, rename or delete on disk, follow a rename or delete into the open
//! editor tabs, open a file, toast. A folder lists when it first opens; an operation lists its
//! folder again when it lands, as `refreshDirectory` does.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

use crate::Level;

#[cfg(test)]
mod tests;

/// One `DirectoryEntry` of a listing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub path: String,
    pub dir: bool,
    /// Git ignores it: the row is dimmed.
    pub ignored: bool,
}

impl Entry {
    /// The entries of a `GET /fs/ls` answer, in the host's order (folders first, then A to Z).
    #[must_use]
    pub fn parse_listing(value: &Value) -> Option<Vec<Entry>> {
        let rows = value.as_array()?;
        Some(
            rows.iter()
                .filter_map(|row| {
                    Some(Entry {
                        name: row.get("name")?.as_str()?.to_owned(),
                        path: row.get("path")?.as_str()?.to_owned(),
                        dir: row.get("type")?.as_str()? == "directory",
                        ignored: row.get("ignored").and_then(Value::as_bool).unwrap_or(false),
                    })
                })
                .collect(),
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Listing {
    Loading,
    Ready(Vec<Entry>),
    Failed(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    File,
    Folder,
}

/// The inline name field: a new entry at the top of `parent`'s rows, or an entry's own row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Inline {
    Create { parent: String, kind: Kind },
    Rename { entry: Entry },
}

impl Inline {
    #[must_use]
    pub fn parent(&self) -> &str {
        match self {
            Self::Create { parent, .. } => parent,
            Self::Rename { entry } => parent_of(&entry.path),
        }
    }
}

/// A change on disk the app sends, and the event that answers it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Op {
    Create { path: String, kind: Kind },
    Rename { from: String, to: String },
    /// Duplicate: a copy beside the entry.
    Copy { from: String, to: String },
    Delete { entry: Entry },
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Explorer {
    /// The active project's folder.
    pub root: Option<String>,
    pub listings: BTreeMap<String, Listing>,
    pub open: BTreeSet<String>,
    pub selected: Option<String>,
    pub inline: Option<Inline>,
    /// The confirm is open while this holds an entry.
    pub deleting: Option<Entry>,
    /// A delete is on its way; the confirm's button spins.
    pub busy: bool,
}

/// One row the tree draws, in order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Row<'a> {
    Entry { entry: &'a Entry, depth: u32, open: bool, loading: bool, renaming: bool },
    /// The create field, at the top of its folder's rows.
    Create { kind: Kind, depth: u32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Nav {
    Up,
    Down,
    /// Close the folder, or go to its parent.
    Left,
    /// Open the folder, or go to its first child.
    Right,
    Home,
    End,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    /// The active project's folder changed (or there is none).
    Root(Option<String>),
    /// The phone explorer shows `dir`: list it if it has no listing, without opening it in the
    /// tree.
    Browse(String),
    /// Duplicate an entry as `<stem> copy<ext>` beside it (`handleDuplicate`).
    Duplicate(String),
    Listed { dir: String, result: Result<Vec<Entry>, String> },
    /// A press on a row: a folder opens or closes, a file opens in an editor tab.
    Press(String),
    Select(String),
    Nav(Nav),
    /// Return on the selected row.
    Activate,
    CollapseAll,
    Refresh,
    /// The header's New File and New Folder (`parent: None`, the target follows the selection),
    /// or a folder menu's.
    StartCreate { parent: Option<String>, kind: Kind },
    StartRename(String),
    /// Return in the name field, with its text.
    Submit(String),
    CancelInline,
    RequestDelete(String),
    ConfirmDelete,
    CancelDelete,
    Done { op: Op, result: Result<(), String> },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Effect {
    List(String),
    Run(Op),
    /// A file opens in an editor tab.
    Open(String),
    /// Open editor tabs under `from` follow it to `to`.
    Renamed { from: String, to: String },
    /// Open editor tabs at or under `path` close.
    Deleted { path: String },
    /// The tree scrolls this row into view.
    Reveal(String),
    Toast(String),
    Log { level: Level, message: String },
}

/// The folder holding `path`.
#[must_use]
pub fn parent_of(path: &str) -> &str {
    let trimmed = path.trim_end_matches(['/', '\\']);
    match trimmed.rfind(['/', '\\']) {
        Some(0) => "/",
        Some(i) => &trimmed[..i],
        None => "",
    }
}

/// `joinTreePath`.
#[must_use]
pub fn join(dir: &str, name: &str) -> String {
    format!("{}/{name}", dir.trim_end_matches(['/', '\\']))
}

fn log(level: Level, message: String) -> Effect {
    Effect::Log { level, message }
}

impl Explorer {
    /// The rows on screen, depth first through the open folders.
    #[must_use]
    pub fn rows(&self) -> Vec<Row<'_>> {
        let mut out = Vec::new();
        if let Some(root) = &self.root {
            self.push_rows(root, 0, &mut out);
        }
        out
    }

    fn push_rows<'a>(&'a self, dir: &str, depth: u32, out: &mut Vec<Row<'a>>) {
        if let Some(Inline::Create { parent, kind }) = &self.inline
            && parent == dir
        {
            out.push(Row::Create { kind: *kind, depth });
        }
        let Some(Listing::Ready(entries)) = self.listings.get(dir) else { return };
        for entry in entries {
            let open = entry.dir && self.open.contains(&entry.path);
            let renaming = matches!(&self.inline, Some(Inline::Rename { entry: e }) if e.path == entry.path);
            let loading = open && matches!(self.listings.get(&entry.path), Some(Listing::Loading));
            out.push(Row::Entry { entry, depth, open, loading, renaming });
            if open {
                self.push_rows(&entry.path, depth + 1, out);
            }
        }
    }

    /// The listed entry at `path`.
    #[must_use]
    pub fn entry(&self, path: &str) -> Option<&Entry> {
        match self.listings.get(parent_of(path)) {
            Some(Listing::Ready(entries)) => entries.iter().find(|e| e.path == path),
            _ => None,
        }
    }

    fn visible_paths(&self) -> Vec<String> {
        self.rows()
            .into_iter()
            .filter_map(|r| match r {
                Row::Entry { entry, .. } => Some(entry.path.clone()),
                Row::Create { .. } => None,
            })
            .collect()
    }

    /// Opens `dir`, listing it when it has no listing yet (or the last one failed).
    fn expand(&mut self, dir: &str, fx: &mut Vec<Effect>) {
        self.open.insert(dir.to_owned());
        if !matches!(self.listings.get(dir), Some(Listing::Ready(_) | Listing::Loading)) {
            self.listings.insert(dir.to_owned(), Listing::Loading);
            fx.push(Effect::List(dir.to_owned()));
        }
    }

    /// `getCreateTargetDir`: the selected folder, the selected file's folder, else the root.
    fn create_target(&self) -> Option<String> {
        let selected = self.selected.as_deref().and_then(|p| self.entry(p));
        match selected {
            Some(e) if e.dir => Some(e.path.clone()),
            Some(e) => Some(parent_of(&e.path).to_owned()),
            None => self.root.clone(),
        }
    }

    fn nav(&mut self, nav: Nav, fx: &mut Vec<Effect>) {
        let paths = self.visible_paths();
        let Some(last) = paths.len().checked_sub(1) else { return };
        let at = self.selected.as_ref().and_then(|s| paths.iter().position(|p| p == s));
        let entry = self.selected.as_deref().and_then(|p| self.entry(p)).cloned();
        let next = match (nav, at) {
            (Nav::Home, _) | (Nav::Down | Nav::Up, None) => Some(0),
            (Nav::End, _) => Some(last),
            (Nav::Down, Some(i)) => Some((i + 1).min(last)),
            (Nav::Up, Some(i)) => Some(i.saturating_sub(1)),
            (Nav::Left | Nav::Right, None) => None,
            (Nav::Left, Some(i)) => match entry {
                Some(e) if e.dir && self.open.contains(&e.path) => {
                    self.open.remove(&e.path);
                    Some(i)
                }
                _ => {
                    let parent = self.selected.as_deref().map(parent_of).unwrap_or_default();
                    paths.iter().position(|p| p == parent).or(Some(i))
                }
            },
            (Nav::Right, Some(i)) => match entry {
                Some(e) if e.dir && !self.open.contains(&e.path) => {
                    self.expand(&e.path, fx);
                    Some(i)
                }
                Some(e) if e.dir => paths.get(i + 1).filter(|p| parent_of(p) == e.path).map(|_| i + 1).or(Some(i)),
                _ => Some(i),
            },
        };
        if let Some(path) = next.and_then(|i| paths.get(i)) {
            self.selected = Some(path.clone());
            fx.push(Effect::Reveal(path.clone()));
        }
    }

    fn press(&mut self, path: &str, fx: &mut Vec<Effect>) {
        self.selected = Some(path.to_owned());
        let Some(entry) = self.entry(path).cloned() else { return };
        if !entry.dir {
            fx.push(Effect::Open(entry.path));
        } else if self.open.contains(&entry.path) {
            self.open.remove(&entry.path);
        } else {
            self.expand(&entry.path, fx);
        }
    }

    fn submit(&mut self, typed: &str, fx: &mut Vec<Effect>) {
        let Some(inline) = self.inline.clone() else { return };
        let name = typed.trim();
        if name.is_empty() {
            self.inline = None;
            return;
        }
        if name == "." || name == ".." || name.contains(['/', '\\']) {
            fx.push(Effect::Toast("Invalid name: file and folder names cannot contain path separators".into()));
            return;
        }
        let path = join(inline.parent(), name);
        match inline {
            Inline::Create { kind, .. } => fx.push(Effect::Run(Op::Create { path, kind })),
            Inline::Rename { entry } if entry.name == name => self.inline = None,
            Inline::Rename { entry } => fx.push(Effect::Run(Op::Rename { from: entry.path, to: path })),
        }
    }

    fn done(&mut self, op: Op, result: Result<(), String>, fx: &mut Vec<Effect>) {
        if let Err(message) = result {
            let toast = match &op {
                Op::Delete { entry } => {
                    self.busy = false;
                    format!("Failed to delete {}: {message}", entry.path)
                }
                Op::Copy { .. } => format!("Failed to copy: {message}"),
                _ => message.clone(),
            };
            fx.push(Effect::Toast(toast));
            fx.push(log(Level::Warn, format!("explorer {op:?} failed: {message}")));
            return;
        }
        match op {
            Op::Create { path, .. } => {
                self.inline = None;
                self.selected = Some(path.clone());
                self.relist(parent_of(&path), fx);
                fx.push(Effect::Reveal(path));
            }
            Op::Rename { from, to } => {
                self.inline = None;
                if self.selected.as_deref() == Some(&from) {
                    self.selected = Some(to.clone());
                }
                self.forget(&from);
                self.relist(parent_of(&to), fx);
                fx.push(Effect::Renamed { from, to });
            }
            Op::Copy { to, .. } => self.relist(parent_of(&to), fx),
            Op::Delete { entry } => {
                self.busy = false;
                self.deleting = None;
                self.selected = None;
                self.forget(&entry.path);
                self.relist(parent_of(&entry.path), fx);
                fx.push(Effect::Deleted { path: entry.path });
            }
        }
    }

    /// Drops what the tree knew at and under `path` (it moved or is gone).
    fn forget(&mut self, path: &str) {
        let under = |p: &String| p == path || p.strip_prefix(path).is_some_and(|rest| rest.starts_with(['/', '\\']));
        self.open.retain(|p| !under(p));
        self.listings.retain(|p, _| !under(p));
    }

    fn relist(&mut self, dir: &str, fx: &mut Vec<Effect>) {
        if !matches!(self.listings.get(dir), Some(Listing::Loading)) {
            fx.push(Effect::List(dir.to_owned()));
        }
    }
}

/// Applies `event`.
pub fn step(x: &mut Explorer, event: Event) -> Vec<Effect> {
    let mut fx = Vec::new();
    match event {
        Event::Root(root) => {
            if x.root == root {
                return fx;
            }
            *x = Explorer { root: root.clone(), ..Explorer::default() };
            if let Some(root) = root {
                x.listings.insert(root.clone(), Listing::Loading);
                fx.push(Effect::List(root));
            }
        }
        Event::Listed { dir, result } => {
            let known = x.root.as_deref() == Some(&dir) || x.listings.contains_key(&dir);
            if !known {
                return fx;
            }
            let listing = match result {
                Ok(entries) => Listing::Ready(entries),
                Err(message) => {
                    fx.push(log(Level::Warn, format!("explorer list failed {dir}: {message}")));
                    Listing::Failed(message)
                }
            };
            x.listings.insert(dir, listing);
        }
        Event::Browse(dir) => {
            if !matches!(x.listings.get(&dir), Some(Listing::Ready(_) | Listing::Loading)) {
                x.listings.insert(dir.clone(), Listing::Loading);
                fx.push(Effect::List(dir));
            }
        }
        Event::Duplicate(path) => {
            if let Some(entry) = x.entry(&path) {
                let (stem, ext) = match entry.name.rfind('.') {
                    Some(dot) if dot > 0 => entry.name.split_at(dot),
                    _ => (entry.name.as_str(), ""),
                };
                let to = join(parent_of(&path), &format!("{stem} copy{ext}"));
                fx.push(Effect::Run(Op::Copy { from: path, to }));
            }
        }
        Event::Press(path) => x.press(&path, &mut fx),
        Event::Select(path) => x.selected = Some(path),
        Event::Nav(nav) => x.nav(nav, &mut fx),
        Event::Activate => {
            if let Some(path) = x.selected.clone() {
                x.press(&path, &mut fx);
            }
        }
        Event::CollapseAll => x.open.clear(),
        Event::Refresh => {
            let dirs: Vec<String> = x.root.iter().chain(x.open.iter()).cloned().collect();
            for dir in dirs {
                x.relist(&dir, &mut fx);
            }
        }
        Event::StartCreate { parent, kind } => {
            if x.inline.is_some() {
                return fx;
            }
            let Some(parent) = parent.or_else(|| x.create_target()) else { return fx };
            if x.root.as_deref() != Some(&parent) {
                x.expand(&parent, &mut fx);
            }
            x.inline = Some(Inline::Create { parent, kind });
        }
        Event::StartRename(path) => {
            if let Some(entry) = x.entry(&path).cloned() {
                x.selected = Some(path);
                x.inline = Some(Inline::Rename { entry });
            }
        }
        Event::Submit(name) => x.submit(&name, &mut fx),
        Event::CancelInline => x.inline = None,
        Event::RequestDelete(path) => {
            if let Some(entry) = x.entry(&path).cloned() {
                x.selected = Some(path);
                x.deleting = Some(entry);
            }
        }
        Event::ConfirmDelete => {
            if let Some(entry) = x.deleting.clone().filter(|_| !x.busy) {
                x.busy = true;
                fx.push(Effect::Run(Op::Delete { entry }));
            }
        }
        Event::CancelDelete => {
            if !x.busy {
                x.deleting = None;
            }
        }
        Event::Done { op, result } => x.done(op, result, &mut fx),
    }
    fx
}

/// The delete confirm's title (`deleteDialogTitle`).
#[must_use]
pub fn delete_title(entry: &Entry) -> &'static str {
    if entry.dir { "Delete folder" } else { "Delete file" }
}

/// The delete confirm's message (`deleteDialogMessage`).
#[must_use]
pub fn delete_message(entry: &Entry) -> String {
    format!("Delete \"{}\"? This cannot be undone.", entry.name)
}
