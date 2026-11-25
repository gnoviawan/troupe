//! The code editor tab (`components/editor/EditorPanel.tsx` with `CodeEditor`): the file's text
//! with its gutter, caret, selection, syntax colours and soft wrap, the find and replace widget,
//! go to line, and the draft conflict bar.
//!
//! [`Editors`] is the state the app keeps: the model's open files (`model::editor`) plus each
//! tab's view state. Its methods are the user's actions; [`Editors::effects`] collects what
//! needs the server (reads and writes), which the app drains. [`view`] draws one file.

mod keys;
mod markdown;
mod view;
mod view_find;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod markdown_tests;

use std::cell::RefCell;
use std::collections::BTreeMap;

use model::Level;
use model::editor::{self as store, Docs, Draft, Effect, Event};
use rune::TextEdit;
use troupe_editor::find::{self, Matches, Query};
use troupe_editor::highlight::Highlights;
use troupe_editor::{Command, Selection};

pub use keys::Outcome;
pub use markdown::is_markdown;
pub use view::{LINE, surface_key, view};

/// The find widget over the editor's top right (`FindPanel` in `codemirror-find.tsx`).
pub struct FindBar {
    pub query: TextEdit,
    pub replace: TextEdit,
    pub case_sensitive: bool,
    pub whole_word: bool,
    pub regex: bool,
    /// The replace row shows (Ctrl+H).
    pub replacing: bool,
    /// The query the selection last followed (find as you type).
    followed: Query,
    /// The matches and the buffer revision and query they were read for.
    matches: RefCell<Option<(u64, Query, Matches)>>,
}

impl FindBar {
    fn new(seed: String, replacing: bool) -> Self {
        Self {
            query: TextEdit::new(seed),
            replace: TextEdit::new(""),
            case_sensitive: false,
            whole_word: false,
            regex: false,
            replacing,
            followed: Query::default(),
            matches: RefCell::new(None),
        }
    }

    #[must_use]
    pub fn query(&self) -> Query {
        Query {
            text: self.query.text.clone(),
            case_sensitive: self.case_sensitive,
            whole_word: self.whole_word,
            regex: self.regex,
        }
    }

    /// The matches in `doc`, read again only when the text or the query changed.
    pub fn matches(&self, doc: &store::Doc) -> Matches {
        let query = self.query();
        let revision = doc.editor.revision();
        let mut cache = self.matches.borrow_mut();
        if let Some((r, q, m)) = cache.as_ref()
            && *r == revision
            && *q == query
        {
            return m.clone();
        }
        let m = find::matches(doc.editor.rope(), &query);
        *cache = Some((revision, query, m.clone()));
        m
    }
}

/// Markdown blocks keyed by the buffer revision and length they were split at.
type BlockCache = RefCell<Option<((u64, usize), Vec<model::editor::markdown::Block>)>>;

/// One editor tab's view state.
pub struct Pane {
    pub highlights: RefCell<Highlights>,
    pub find: Option<FindBar>,
    /// The go-to-line field, while it shows.
    pub goto: Option<TextEdit>,
    /// The input method's composition at the caret.
    pub preedit: Option<String>,
    /// Columns that fit the text area; the soft wrap width.
    pub columns: usize,
    /// Rows that fit the viewport, for Page Up and Page Down.
    pub page: usize,
    /// The caret's chunk to scroll into view after the next frame.
    pub reveal: bool,
    /// A Markdown file: it has the toolbar's Preview / Source switch.
    pub markdown: bool,
    /// Drawn as a document (`markdown`), not as source.
    pub preview: bool,
    /// The Markdown blocks, and the buffer revision and length they were split at (a reload
    /// starts the revision again).
    blocks: BlockCache,
}

impl Pane {
    fn new(path: &str) -> Self {
        Self {
            highlights: RefCell::new(Highlights::for_path(path)),
            find: None,
            goto: None,
            preedit: None,
            columns: 0,
            page: 20,
            reveal: false,
            markdown: is_markdown(path),
            preview: is_markdown(path),
            blocks: RefCell::new(None),
        }
    }

    /// `doc`'s Markdown blocks, split again only when the text changed.
    pub fn blocks(&self, doc: &store::Doc) -> Vec<model::editor::markdown::Block> {
        let revision = (doc.editor.revision(), doc.editor.rope().len_chars());
        let mut cache = self.blocks.borrow_mut();
        if let Some((r, blocks)) = cache.as_ref()
            && *r == revision
        {
            return blocks.clone();
        }
        let blocks = model::editor::markdown::blocks(doc.editor.rope());
        *cache = Some((revision, blocks.clone()));
        blocks
    }
}

/// The open text files and their tabs' view state.
#[derive(Default)]
pub struct Editors {
    pub docs: Docs,
    pub panes: BTreeMap<String, Pane>,
    /// What the files ask of the server and the log, in order.
    pub effects: Vec<Effect>,
    /// The file the keyboard is in.
    pub focused: Option<String>,
    /// Milliseconds since the epoch, set by the app each frame.
    pub now: f64,
    /// A field for handlers whose file closed under them.
    pub spare: TextEdit,
}

impl Editors {
    pub fn step(&mut self, event: Event) {
        let fx = store::step(&mut self.docs, event, self.now);
        self.effects.extend(fx);
    }

    /// Opens `path` (an editor tab mounted), with the draft the record kept for it.
    pub fn open(&mut self, path: &str, draft: Option<Draft>) {
        self.panes.entry(path.to_owned()).or_insert_with(|| Pane::new(path));
        self.step(Event::Open { path: path.to_owned(), draft });
    }

    pub fn close(&mut self, path: &str) {
        self.panes.remove(path);
        if self.focused.as_deref() == Some(path) {
            self.focused = None;
        }
        self.step(Event::Close { path: path.to_owned() });
    }

    /// The file moved on disk (an explorer rename, or its folder's): the buffer, its unsaved
    /// edits and the keyboard follow it to `to` (`renameOpenFile`).
    pub fn rename(&mut self, from: &str, to: &str) {
        if from == to || !self.docs.docs.contains_key(from) {
            return;
        }
        self.panes.remove(from);
        self.panes.insert(to.to_owned(), Pane::new(to));
        if self.focused.as_deref() == Some(from) {
            self.focused = Some(to.to_owned());
        }
        self.step(Event::Rename { from: from.to_owned(), to: to.to_owned() });
    }

    pub fn save(&mut self, path: &str) {
        self.step(Event::Save { path: path.to_owned() });
    }

    /// Runs `command` on `path`'s buffer, unless the buffer is read-only. Returns whether
    /// anything changed.
    pub fn apply(&mut self, path: &str, command: Command) -> bool {
        let now = self.now / 1000.0;
        let Some(doc) = self.docs.docs.get_mut(path) else {
            return false;
        };
        let edits = !matches!(command, Command::Move { .. } | Command::Select(_) | Command::SelectAll);
        if edits && doc.read_only() {
            return false;
        }
        let changed = doc.editor.apply(command, now);
        if changed && let Some(pane) = self.panes.get_mut(path) {
            pane.reveal = true;
        }
        changed
    }

    /// A press at char offset `at`; with `extend`, the selection grows to it.
    pub fn press(&mut self, path: &str, at: usize, extend: bool) {
        self.focused = Some(path.to_owned());
        let anchor = match self.docs.docs.get(path) {
            Some(doc) if extend => doc.editor.selection().anchor,
            _ => at,
        };
        self.apply(path, Command::Select(Selection { anchor, head: at }));
    }

    /// Ctrl+F and Ctrl+H: open the find widget seeded with the selection, or focus it.
    pub fn open_find(&mut self, path: &str, replacing: bool) {
        let seed = self
            .docs
            .docs
            .get(path)
            .map(|doc| {
                let range = doc.editor.selection().range();
                doc.editor.rope().slice(range).to_string()
            })
            .filter(|s| !s.contains('\n'))
            .unwrap_or_default();
        let Some(pane) = self.panes.get_mut(path) else {
            return;
        };
        pane.goto = None;
        match pane.find.as_mut() {
            Some(bar) => {
                bar.replacing |= replacing;
                if !seed.is_empty() {
                    bar.query = TextEdit::new(seed);
                }
            }
            None => pane.find = Some(FindBar::new(seed, replacing)),
        }
    }

    pub fn close_find(&mut self, path: &str) {
        if let Some(pane) = self.panes.get_mut(path) {
            pane.find = None;
            pane.goto = None;
        }
    }

    /// Enter in find, or its arrows: select the next (or previous) match from the caret.
    pub fn find_step(&mut self, path: &str, forward: bool) {
        let (Some(doc), Some(pane)) = (self.docs.docs.get(path), self.panes.get(path)) else {
            return;
        };
        let Some(bar) = &pane.find else { return };
        let matches = bar.matches(doc);
        let sel = doc.editor.selection().range();
        let from = if forward { sel.end.max(sel.start + usize::from(!sel.is_empty())) } else { sel.start };
        if let Some(next) = find::next_match(&matches.ranges, from, !forward) {
            self.apply(path, Command::Select(Selection { anchor: next.start, head: next.end }));
        }
    }

    pub fn replace_next(&mut self, path: &str) {
        let now = self.now / 1000.0;
        let (Some(doc), Some(pane)) = (self.docs.docs.get_mut(path), self.panes.get_mut(path)) else {
            return;
        };
        let Some(bar) = &pane.find else { return };
        if doc.read_only() {
            return;
        }
        find::replace_next(&mut doc.editor, &bar.query(), &bar.replace.text.clone(), now);
        pane.reveal = true;
    }

    pub fn replace_all(&mut self, path: &str) {
        let now = self.now / 1000.0;
        let (Some(doc), Some(pane)) = (self.docs.docs.get_mut(path), self.panes.get_mut(path)) else {
            return;
        };
        let Some(bar) = &pane.find else { return };
        if doc.read_only() {
            return;
        }
        let n = find::replace_all(&mut doc.editor, &bar.query(), &bar.replace.text.clone(), now);
        self.effects.push(Effect::Log { level: Level::Info, message: format!("editor replace all: {n} matches") });
    }

    /// Find as you type (`FindPanel.commit`): when the query's pattern changed, the match under
    /// the caret stays selected while it still matches, else the next one is selected. The app
    /// calls this each frame.
    pub fn follow_find(&mut self) {
        let mut moves = Vec::new();
        for (path, pane) in &mut self.panes {
            let (Some(bar), Some(doc)) = (pane.find.as_mut(), self.docs.docs.get(path)) else { continue };
            let query = bar.query();
            if query == bar.followed {
                continue;
            }
            bar.followed = query.clone();
            if !query.is_valid() {
                continue;
            }
            let from = doc.editor.selection().range().start;
            if let Some(m) = find::next_match(&bar.matches(doc).ranges, from, false) {
                moves.push((path.clone(), m));
            }
        }
        for (path, m) in moves {
            self.apply(&path, Command::Select(Selection { anchor: m.start, head: m.end }));
        }
    }

    /// Ctrl+G.
    pub fn open_goto(&mut self, path: &str) {
        if let Some(pane) = self.panes.get_mut(path) {
            pane.find = None;
            pane.goto.get_or_insert_with(|| TextEdit::new(""));
        }
    }

    /// Return in the go-to-line field: move there and close it, or keep it open when the entry
    /// names no line.
    pub fn go_to_line(&mut self, path: &str) -> bool {
        let (Some(doc), Some(pane)) = (self.docs.docs.get(path), self.panes.get(path)) else {
            return false;
        };
        let Some(field) = &pane.goto else { return false };
        let Some(at) = find::go_to_line(&doc.editor, &field.text) else {
            return false;
        };
        self.apply(path, Command::Select(Selection::caret(at)));
        if let Some(pane) = self.panes.get_mut(path) {
            pane.goto = None;
            pane.reveal = true;
        }
        true
    }

    /// The find or go-to-line field a handler edits.
    pub fn field(&mut self, path: &str, which: Field) -> &mut TextEdit {
        let pane = self.panes.get_mut(path);
        match (pane, which) {
            (Some(Pane { find: Some(bar), .. }), Field::Query) => &mut bar.query,
            (Some(Pane { find: Some(bar), .. }), Field::Replace) => &mut bar.replace,
            (Some(Pane { goto: Some(field), .. }), Field::GoTo) => field,
            _ => &mut self.spare,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    Query,
    Replace,
    GoTo,
}
