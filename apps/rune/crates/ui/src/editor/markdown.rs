//! The Markdown editor (`MarkdownEditor.tsx`, which is BlockNote on the web): the file drawn
//! as a document, one top-level block at a time (`model::editor::markdown`). Every block is
//! rendered, mermaid fences as diagrams, except the block under the caret, which shows its
//! source lines in the code editor's rows and takes the keys. The buffer is the file's text,
//! so an edit changes only what was typed and an unedited file saves byte for byte.

use rune::{Drag, El, Phase, ScrollSpec, Tx, col, el, row, text};
use theme::Tokens;
use troupe_editor::{Command, Selection};

use super::Editors;
use super::view::{Get, Lens, Lines, line_row};
use crate::type_::{mono, sans};
use model::editor::markdown::{self as md, Block, Kind};

/// The document column's widest measure and its side padding.
pub(super) const MEASURE: f32 = 760.0;
pub(super) const PAD_X: f32 = 48.0;

/// What one stretch of the document draws as.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Piece {
    /// Block `index`, rendered.
    Rendered(usize),
    /// Lines in source: the block under the caret, or the blank line it sits on.
    Source(std::ops::Range<usize>),
}

/// The document's pieces in order, with the caret's block (or blank line) in source when
/// `editing`.
pub(super) fn pieces(blocks: &[Block], caret_line: usize, editing: bool) -> Vec<Piece> {
    let active = editing.then(|| md::at_line(blocks, caret_line)).flatten();
    let mut out = Vec::with_capacity(blocks.len() + 1);
    let mut blank_shown = !editing || active.is_some();
    for (i, block) in blocks.iter().enumerate() {
        if !blank_shown && caret_line < block.lines.start {
            out.push(Piece::Source(caret_line..caret_line + 1));
            blank_shown = true;
        }
        out.push(if active == Some(i) { Piece::Source(block.lines.clone()) } else { Piece::Rendered(i) });
    }
    if !blank_shown {
        out.push(Piece::Source(caret_line..caret_line + 1));
    }
    out
}

/// The document view of `path`, below the toolbar.
pub(super) fn preview<S: 'static>(s: &S, get: Get<S>, lens: Lens<S>, t: &'static Tokens, path: &str) -> El {
    let editors = get(s);
    let (Some(doc), Some(pane)) = (editors.docs.docs.get(path), editors.panes.get(path)) else {
        return el();
    };
    let c = &t.colors;
    let rope = doc.editor.rope();
    let blocks = pane.blocks(doc);
    let caret_line = rope.char_to_line(doc.editor.selection().head);
    let editing = editors.focused.as_deref() == Some(path);
    let mut page = col().key(format!("markdown-page-{path}")).w_full().max_w(MEASURE).px(PAD_X).py(24.0);
    for (n, piece) in pieces(&blocks, caret_line, editing).into_iter().enumerate() {
        page = page.child(match piece {
            Piece::Source(lines) => source(editors, lens, t, path, lines, n),
            Piece::Rendered(i) => rendered(t, lens, path, rope, &blocks[i], i),
        });
    }
    let end = path.to_owned();
    let tail = el().key(format!("markdown-tail-{path}")).w_full().h(120.0).on_drag(move |s: &mut S, d: &mut Drag, tx| {
        if d.phase == Phase::Down {
            let editors = lens(s);
            let at = editors.docs.docs.get(&end).map_or(0, |doc| doc.editor.rope().len_chars());
            editors.press(&end, at, false);
            tx.focus(super::surface_key(&end), false);
        }
    });
    let viewport = col()
        .key(format!("editor-viewport-{path}"))
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .scroll(ScrollSpec { stick_bottom: false })
        .child(row().w_full().justify_center().child(page))
        .child(tail);
    col().w_full().grow(1.0).min_h(0.0).fill(c.background).child(viewport)
}

/// Lines in source, as the code editor draws them without its gutter.
fn source<S: 'static>(editors: &Editors, lens: Lens<S>, t: &'static Tokens, path: &str, lines: std::ops::Range<usize>, n: usize) -> El {
    let Some(read) = Lines::read(editors, path, lines.clone()) else {
        return el();
    };
    let mut out = col().key(format!("markdown-source-{n}")).w_full().my(4.0).radius(6.0).fill(t.colors.foreground.alpha(0.03));
    for line in lines {
        out = out.child(line_row(&read, lens, t, path, line, None));
    }
    out
}

/// A block drawn as the reader sees it; a press puts the caret at its start.
fn rendered<S: 'static>(t: &'static Tokens, lens: Lens<S>, path: &str, rope: &troupe_editor::Rope, block: &Block, i: usize) -> El {
    let src = md::source(rope, block);
    let start = rope.line_to_char(block.lines.start);
    let body = match block.kind {
        Kind::Frontmatter => properties(t, &src),
        _ => crate::markdown::markdown::<S>(t, src, false, &crate::markdown::Hooks::default())
            .render(&crate::markdown::markdown_style(t)),
    };
    let pressed = path.to_owned();
    col()
        .key(format!("markdown-block-{i}-{}", block.lines.start))
        .w_full()
        .min_w(0.0)
        .child(body)
        .on_drag(move |s: &mut S, d: &mut Drag, tx: &mut Tx| {
            if d.phase == Phase::Down {
                lens(s).press(&pressed, start, false);
                tx.focus(super::surface_key(&pressed), false);
            }
        })
}

/// Front matter as `FrontmatterProperties` lists it: each key beside its value.
fn properties(t: &'static Tokens, src: &str) -> El {
    let c = &t.colors;
    let mut list = col().w_full().gap(4.0).py(8.0).mb(12.0).border_sides([0.0, 0.0, 1.0, 0.0], c.border);
    for (key, value) in md::properties(src) {
        list = list.child(
            row()
                .gap(12.0)
                .child(text(key).font(sans(13.0, 500.0, 20.0)).color(c.muted_foreground).w(140.0))
                .child(text(value).font(mono(13.0, 400.0, 20.0)).color(c.foreground).grow(1.0).min_w(0.0)),
        );
    }
    list
}

/// The toolbar of a Markdown file (`EditorToolbar`): its folder and name, and the
/// Preview / Source switch.
pub(super) fn toolbar<S: 'static>(t: &'static Tokens, lens: Lens<S>, path: &str, preview: bool) -> El {
    let c = &t.colors;
    let mut parts: Vec<&str> = path.split(['/', '\\']).filter(|p| !p.is_empty()).collect();
    let name = parts.pop().unwrap_or(path).to_owned();
    let mut crumbs = row().gap(6.0).items_center().min_w(0.0);
    if let Some(folder) = parts.pop() {
        crumbs = crumbs
            .child(text(folder.to_owned()).font(sans(12.0, 400.0, 16.0)).color(c.muted_foreground.alpha(0.7)))
            .child(rune::icon(&theme::icons::CHEVRON_RIGHT, 12.0).color(c.muted_foreground.alpha(0.7)));
    }
    crumbs = crumbs.child(text(name).font(sans(12.0, 500.0, 16.0)).color(c.muted_foreground));
    let picked = path.to_owned();
    let switch = crate::Segmented::new("View mode", &["Preview", "Source"], usize::from(!preview)).build(
        t,
        &format!("editor-view-mode-{path}"),
        false,
        move |s: &mut S, i| lens(s).set_preview(&picked, i == 0),
    );
    row()
        .key(format!("editor-toolbar-{path}"))
        .w_full()
        .h(40.0)
        .pl(16.0)
        .pr(6.0)
        .items_center()
        .justify_between()
        .gap(8.0)
        .fill(c.background)
        .border_sides([0.0, 0.0, 1.0, 0.0], c.border)
        .child(crumbs)
        .child(switch)
}

impl Editors {
    /// Shows `path` as a document (true) or as source.
    pub fn set_preview(&mut self, path: &str, preview: bool) {
        if let Some(pane) = self.panes.get_mut(path) {
            pane.preview = preview && pane.markdown;
        }
    }

    /// Whether `path` shows as a document.
    #[must_use]
    pub fn previewing(&self, path: &str) -> bool {
        self.panes.get(path).is_some_and(|p| p.preview)
    }

    /// Puts the caret at the start of `path`'s Markdown block `index` (a press on it).
    pub fn edit_block(&mut self, path: &str, index: usize) {
        let Some(doc) = self.docs.docs.get(path) else { return };
        let blocks = md::blocks(doc.editor.rope());
        if let Some(block) = blocks.get(index) {
            let at = doc.editor.rope().line_to_char(block.lines.start);
            self.focused = Some(path.to_owned());
            self.apply(path, Command::Select(Selection::caret(at)));
        }
    }
}

/// Whether `path` opens in the Markdown editor (`textPreviewForPath`).
#[must_use]
pub fn is_markdown(path: &str) -> bool {
    path.rsplit(['/', '\\']).next().and_then(|n| n.rsplit_once('.')).is_some_and(|(stem, ext)| !stem.is_empty() && ext.eq_ignore_ascii_case("md"))
}
