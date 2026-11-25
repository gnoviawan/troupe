//! Drawing one editor tab: the draft conflict bar (`EditorDraftConflictBanner`), then the text
//! in a scroll viewport as `codemirror-theme.ts` styles it: a card-coloured gutter of line
//! numbers with the active line's number in the text colour, the active line washed, the
//! selection a neutral wash, the caret a 2 pt primary bar, find matches in the warning hue.
//!
//! The text is virtualised over 32-line chunks (`docs/rune/decisions.md`, "Code editor"):
//! a chunk is one virtual row, so typing inside a chunk never changes the row count.

use std::cell::Cell;
use std::ops::Range;

use rune::{Color, Cursor, Drag, El, GrowthSpec, Key, Phase, ScrollSpec, Span, TextStyle, Tx, col, el, row, text, text_input};
use theme::Tokens;
use troupe_editor::highlight::Tag;
use troupe_editor::{Command, line_len, wrap};

use super::keys::Outcome;
use super::{Editors, Field};
use crate::type_::{mono, sans};

/// Lines per virtual row.
pub const CHUNK: usize = 32;
/// `.cm-content`'s 0.8125rem at line-height 1.6.
pub const LINE: f32 = 20.8;
const FONT: TextStyle = mono(13.0, 400.0, LINE);
/// `.cm-line`'s left padding and `.cm-content`'s top padding.
const PAD_LEFT: f32 = 6.0;
const PAD_TOP: f32 = 4.0;

thread_local! {
    static ADVANCE: Cell<f32> = const { Cell::new(0.0) };
}

/// The width of one character of the editor face.
fn advance() -> f32 {
    ADVANCE.with(|a| {
        if a.get() == 0.0 {
            let (w, _) = rune::text::measure(&"0".repeat(100), &[], FONT, None);
            a.set(if w > 0.0 { w / 100.0 } else { 7.8 });
        }
        a.get()
    })
}

pub(super) type Get<S> = fn(&S) -> &Editors;
pub(super) type Lens<S> = fn(&mut S) -> &mut Editors;

/// The key of `path`'s text surface, which takes the keyboard.
#[must_use]
pub fn surface_key(path: &str) -> String {
    format!("editor-surface-{path}")
}

fn chunk_key(path: &str, chunk: usize) -> String {
    format!("editor-chunk-{path}-{chunk}")
}

/// The gutter's width for `lines` lines: `lineNumbers()` sizes to the widest number, with
/// 5 pt before and 3 pt after it, then the fold gutter.
fn gutter_width(lines: usize) -> f32 {
    let digits = lines.max(1).to_string().len().max(2) as f32;
    5.0 + digits * advance() + 3.0 + 14.0
}

/// One editor tab for `path`, or None while the app holds no such file.
pub fn view<S: 'static>(s: &S, get: Get<S>, lens: Lens<S>, t: &'static Tokens, path: &str) -> Option<El> {
    let editors = get(s);
    let doc = editors.docs.docs.get(path)?;
    let pane = editors.panes.get(path)?;
    let c = &t.colors;
    let mut body = col().key(format!("editor-{path}")).w_full().grow(1.0).min_h(0.0).fill(c.background);
    if doc.draft_conflict.is_some() {
        body = body.child(conflict_bar(t, lens, path));
    }
    match &doc.load {
        model::editor::Load::Loading => {
            return Some(body.child(col().grow(1.0).w_full().center().child(text("Loading...").font(sans(14.0, 400.0, 20.0)).color(c.muted_foreground))));
        }
        model::editor::Load::Unavailable(why) => return Some(body.child(unavailable::<S>(t, path, why))),
        model::editor::Load::Ready => {}
    }
    if pane.markdown {
        body = body.child(super::markdown::toolbar(t, lens, path, pane.preview));
    }
    let preview = pane.preview;
    let lines = doc.editor.len_lines();
    let gutter = gutter_width(lines);
    let (keyed, measured, dragged_path) = (path.to_owned(), path.to_owned(), path.to_owned());
    let viewport = if preview { super::markdown::preview(s, get, lens, t, path) } else { code_viewport(get, lens, t, path, lines, gutter) };
    let mut surface = col()
        .key(surface_key(path))
        .role("textbox")
        .label(path.rsplit(['/', '\\']).next().unwrap_or(path).to_owned())
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .clip_rounded(0.0)
        .focusable(true)
        .cursor(Cursor::Text)
        .on_key(move |s: &mut S, key, tx| on_key(s, lens, &keyed, key, tx))
        .on_layout_tx(move |s: &mut S, [_, _, w, h], _tx| {
            let text_w = if preview { w.min(super::markdown::MEASURE) - 2.0 * super::markdown::PAD_X } else { w - gutter };
            let columns = (((text_w - PAD_LEFT - 2.0) / advance()).floor() as usize).max(1);
            let editors = lens(s);
            if let Some(pane) = editors.panes.get_mut(&measured) {
                pane.columns = columns;
                pane.page = ((h / LINE) as usize).saturating_sub(1).max(1);
            }
            if let Some(doc) = editors.docs.docs.get_mut(&measured) {
                doc.editor.set_wrap(columns);
            }
        })
        .on_drag(move |s: &mut S, d: &mut Drag, _tx| {
            if d.phase == Phase::Down {
                lens(s).focused = Some(dragged_path.clone());
            }
        })
        .child(viewport);
    if let Some(bar) = &pane.find {
        surface = surface.child(super::view_find::bar(t, lens, path, doc, bar));
    }
    if let Some(field) = &pane.goto {
        surface = surface.child(super::view_find::goto(t, lens, path, field));
    }
    Some(body.child(surface))
}

/// The code view's scroll viewport: the text in 32-line virtual rows beside the gutter.
fn code_viewport<S: 'static>(get: Get<S>, lens: Lens<S>, t: &'static Tokens, path: &str, lines: usize, gutter: f32) -> El {
    let chunks = lines.div_ceil(CHUNK);
    let rows_path = path.to_owned();
    let text_rows = col()
        .key(format!("editor-lines-{path}"))
        .w_full()
        .pt(PAD_TOP)
        .pb(PAD_TOP)
        .growth(GrowthSpec::default())
        .virtual_rows_version(0u8)
        .virtual_rows(
            chunks,
            CHUNK as f32 * LINE,
            |_s: &S, i| i.to_string().into(),
            move |s: &S, i| chunk(s, get, lens, t, &rows_path, i, gutter),
        );
    col()
        .key(format!("editor-viewport-{path}"))
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .scroll(ScrollSpec { stick_bottom: false })
        .child(text_rows)
}

fn on_key<S: 'static>(s: &mut S, lens: Lens<S>, path: &str, key: &rune::KeyEvent, tx: &mut Tx) -> bool {
    let editors = lens(s);
    editors.focused = Some(path.to_owned());
    let outcome = editors.key(path, key);
    let reveal = editors.panes.get_mut(path).is_some_and(|p| std::mem::take(&mut p.reveal));
    if reveal && let Some(doc) = editors.docs.docs.get(path) {
        let line = doc.editor.rope().char_to_line(doc.editor.selection().head);
        tx.scroll_into_view(caret_key(path, line));
    }
    match outcome {
        Outcome::Ignored => false,
        Outcome::Handled => true,
        Outcome::Focus(which) => {
            tx.focus(field_key(path, which), false);
            true
        }
        Outcome::Copy(text) => {
            tx.copy(text);
            true
        }
        Outcome::Paste => {
            let path = path.to_owned();
            tx.read_clipboard(move |s: &mut S, content, _tx| {
                lens(s).apply(&path, Command::Insert(content.text().to_owned()));
            });
            true
        }
    }
}

/// The key of `path`'s find, replace or go-to-line field.
#[must_use]
pub fn field_key(path: &str, which: Field) -> String {
    format!("editor-field-{which:?}-{path}")
}

fn caret_key(path: &str, line: usize) -> String {
    format!("editor-line-{path}-{line}")
}

/// The style CodeMirror's `HighlightStyle` gives `tag`.
fn tag_span(t: &Tokens, tag: Tag, range: Range<usize>) -> Span {
    let s = &t.syntax;
    let span = Span::new(range.start, range.end);
    match tag {
        Tag::Keyword => span.color(s.keyword),
        Tag::Comment => span.color(s.comment).italic(),
        Tag::String => span.color(s.string),
        Tag::Number => span.color(s.number),
        Tag::Bool => span.color(s.bool),
        Tag::Function => span.color(s.function),
        Tag::Property => span.color(s.property),
        Tag::Definition => span.color(s.definition),
        Tag::Variable => span.color(s.variable),
        Tag::Type => span.color(s.type_),
        Tag::Operator => span.color(s.operator),
        Tag::Punctuation => span.color(s.punctuation),
        Tag::TagName => span.color(s.tag),
        Tag::AttributeName => span.color(s.attribute_name),
        Tag::AttributeValue => span.color(s.attribute_value),
        Tag::Heading => span.color(s.heading).bold(),
        Tag::Link => span.color(s.link),
        Tag::Emphasis => span.italic(),
        Tag::Strong => span.bold(),
    }
}

/// One 32-line chunk: each line's gutter number beside its wrapped rows.
fn chunk<S: 'static>(s: &S, get: Get<S>, lens: Lens<S>, t: &'static Tokens, path: &str, index: usize, gutter: f32) -> El {
    let first = index * CHUNK;
    let Some(lines) = Lines::read(get(s), path, first..first + CHUNK) else {
        return el();
    };
    let mut out = col().key(chunk_key(path, index)).w_full();
    for line in lines.range.clone() {
        out = out.child(line_row(&lines, lens, t, path, line, Some(gutter)));
    }
    out
}

/// What drawing a run of lines reads once: their colours, the selection, the find matches.
pub(super) struct Lines<'a> {
    editors: &'a Editors,
    pane: &'a super::Pane,
    doc: &'a model::editor::Doc,
    range: Range<usize>,
    spans: Vec<troupe_editor::highlight::LineSpans>,
    matches: Vec<Range<usize>>,
}

impl<'a> Lines<'a> {
    /// Lines `range` of `path` (clamped to the file), or None while the app holds no such file.
    pub(super) fn read(editors: &'a Editors, path: &str, range: Range<usize>) -> Option<Self> {
        let (doc, pane) = (editors.docs.docs.get(path)?, editors.panes.get(path)?);
        let rope = doc.editor.rope();
        let range = range.start..range.end.min(rope.len_lines());
        let spans = pane.highlights.borrow_mut().chunk(rope, range.clone()).to_vec();
        let matches = pane.find.as_ref().map(|bar| bar.matches(doc).ranges).unwrap_or_default();
        Some(Self { editors, pane, doc, range, spans, matches })
    }
}

/// One line's wrapped rows, beside its gutter number when `gutter` is set: the match and
/// selection washes, the coloured text, the caret and the input method's composition.
pub(super) fn line_row<S: 'static>(lines: &Lines, lens: Lens<S>, t: &'static Tokens, path: &str, line: usize, gutter: Option<f32>) -> El {
    let (doc, pane) = (lines.doc, lines.pane);
    let rope = doc.editor.rope();
    let sel = doc.editor.selection();
    let caret_line = rope.char_to_line(sel.head);
    let focused = lines.editors.focused.as_deref() == Some(path);
    let c = &t.colors;
    let line_start = rope.line_to_char(line);
    let len = line_len(rope, line);
    let content: String = rope.slice(line_start..line_start + len).to_string();
    let starts = if pane.columns > 0 { wrap::rows(content.chars(), pane.columns) } else { vec![0] };
    let height = starts.len() as f32 * LINE;
    let active = line == caret_line;
    let mut body = el().key(caret_key(path, line)).grow(1.0).h(height).min_w(0.0);
    if active && gutter.is_some() {
        body = body.fill(c.foreground.alpha(0.03));
    }
    let line_spans = lines.spans.get(line - lines.range.start).cloned().unwrap_or_default();
    let x_of = |col: usize, row: usize| PAD_LEFT + (col - starts[row]) as f32 * advance();
    let row_of = |col: usize| starts.iter().rposition(|s| *s <= col).unwrap_or(0);
    // Match washes, then the selection, then the text, then the caret.
    let char_range = line_start..line_start + len;
    let mut washes: Vec<(Range<usize>, Color)> = Vec::new();
    for m in &lines.matches {
        let selected = m.start == sel.range().start && m.end == sel.range().end;
        washes.push((m.clone(), if selected { c.warning.alpha(0.4) } else { c.warning.alpha(0.15) }));
    }
    if !sel.is_empty() {
        washes.push((sel.range(), c.foreground.alpha(0.14)));
    }
    for (range, colour) in washes {
        if range.end < char_range.start || range.start > char_range.end {
            continue;
        }
        let from = range.start.max(char_range.start) - line_start;
        let to = range.end.min(char_range.end) - line_start;
        let past_end = range.end > char_range.end && range.start <= char_range.end;
        if from > to || (from == to && !past_end) {
            continue;
        }
        let (r0, r1) = (row_of(from), row_of(to));
        for r in r0..=r1 {
            let a = if r == r0 { from } else { starts[r] };
            let b = if r == r1 { to } else { starts.get(r + 1).copied().unwrap_or(len) };
            let extra = if r == r1 && past_end { advance() } else { 0.0 };
            let w = (b - a) as f32 * advance() + extra;
            if w > 0.0 {
                body = body.child(el().absolute(x_of(a, r), r as f32 * LINE).size(w, LINE).fill(colour));
            }
        }
    }
    // One pass over the line: each row's byte range, and the spans walked in order, so a
    // long line costs its length, not its length times its rows.
    let bytes = row_bytes(&content, &starts);
    let mut first_span = 0;
    for (r, pair) in bytes.windows(2).enumerate() {
        let (byte_start, byte_end) = (pair[0], pair[1]);
        let piece = &content[byte_start..byte_end];
        while line_spans.get(first_span).is_some_and(|(range, _)| range.end <= byte_start) {
            first_span += 1;
        }
        let row_spans: Vec<Span> = line_spans[first_span..]
            .iter()
            .take_while(|(range, _)| range.start < byte_end)
            .filter(|(range, _)| range.end > byte_start)
            .map(|(range, tag)| tag_span(t, *tag, range.start.max(byte_start) - byte_start..range.end.min(byte_end) - byte_start))
            .collect();
        body = body.child(
            text(piece.to_owned())
                .font(FONT)
                .color(c.foreground)
                .spans(row_spans)
                .absolute(PAD_LEFT, r as f32 * LINE),
        );
    }
    if active && focused {
        let col_in = sel.head - line_start;
        let r = row_of(col_in);
        let x = x_of(col_in, r);
        body = body.child(el().absolute(x, r as f32 * LINE + 1.0).size(2.0, LINE - 2.0).fill(c.primary).ime_cursor());
        if let Some(preedit) = &pane.preedit {
            body = body.child(
                text(preedit.clone())
                    .font(FONT)
                    .color(c.foreground)
                    .spans(vec![Span { underline: Some(c.foreground), ..Span::new(0, preedit.len()) }])
                    .fill(c.background)
                    .absolute(x, r as f32 * LINE),
            );
        }
    }
    let pressed = path.to_owned();
    let rows = starts.clone();
    body = body.on_drag(move |s: &mut S, d: &mut Drag, _tx| press(s, lens, &pressed, line, &rows, d));
    let mut out = row().w_full().h(height);
    if let Some(gutter) = gutter {
        let number = text((line + 1).to_string())
            .font(FONT)
            .color(if active { c.foreground } else { c.muted_foreground });
        out = out.child(
            row()
                .w(gutter)
                .h(height)
                .fill(c.card)
                .border_sides([0.0, 1.0, 0.0, 0.0], c.border)
                .items_start()
                .child(row().w(gutter - 14.0 - 3.0).justify_end().child(number)),
        );
    }
    out.child(body)
}

/// The byte offset of each row start in `content`, then its length: `starts` are char offsets
/// in order, read in one pass.
pub(super) fn row_bytes(content: &str, starts: &[usize]) -> Vec<usize> {
    let mut out = Vec::with_capacity(starts.len() + 1);
    let mut next = starts.iter().peekable();
    for (char_index, (byte, _)) in content.char_indices().enumerate() {
        while next.next_if(|s| **s <= char_index).is_some() {
            out.push(byte);
        }
    }
    out.extend(next.map(|_| content.len()));
    out.push(content.len());
    out
}

/// A press or drag on `line`'s text at the pointer.
fn press<S>(s: &mut S, lens: Lens<S>, path: &str, line: usize, starts: &[usize], d: &Drag) {
    let editors = lens(s);
    let Some(doc) = editors.docs.docs.get(path) else { return };
    let rope = doc.editor.rope();
    // A drag past this line's box keeps counting rows into the lines around it.
    let mut line = line as isize;
    let mut row = (d.y / LINE).floor() as isize;
    let mut starts = starts.to_vec();
    while row < 0 && line > 0 {
        line -= 1;
        starts = row_starts(editors, path, line as usize);
        row += starts.len() as isize;
    }
    while row >= starts.len() as isize && (line as usize) + 1 < rope.len_lines() {
        row -= starts.len() as isize;
        line += 1;
        starts = row_starts(editors, path, line as usize);
    }
    let line = line.max(0) as usize;
    let row = row.clamp(0, starts.len() as isize - 1) as usize;
    let len = line_len(rope, line);
    let row_end = starts.get(row + 1).map_or(len, |n| n.saturating_sub(1).max(starts[row]));
    let col = starts[row] + (((d.x - PAD_LEFT) / advance()).round().max(0.0) as usize);
    let at = rope.line_to_char(line) + col.min(row_end);
    match d.phase {
        Phase::Down => editors.press(path, at, false),
        Phase::Move => editors.press(path, at, true),
        Phase::Up => {}
    }
}

fn row_starts(editors: &Editors, path: &str, line: usize) -> Vec<usize> {
    let (Some(doc), Some(pane)) = (editors.docs.docs.get(path), editors.panes.get(path)) else {
        return vec![0];
    };
    let rope = doc.editor.rope();
    let start = rope.line_to_char(line);
    let content = rope.slice(start..start + line_len(rope, line));
    if pane.columns == 0 { vec![0] } else { wrap::rows(content.chars(), pane.columns) }
}

/// `UnsupportedFileView`: the file's name, why it shows no text, and Copy Path.
pub(crate) fn unavailable<S: 'static>(t: &'static Tokens, path: &str, why: &model::editor::Unavailable) -> El {
    let c = &t.colors;
    let name = path.rsplit(['/', '\\']).next().unwrap_or(path).to_owned();
    let mut lines = col()
        .items_center()
        .gap(4.0)
        .child(text(name).font(sans(14.0, 500.0, 20.0)).color(c.foreground))
        .child(text(why.headline()).font(sans(12.0, 400.0, 16.0)).color(c.muted_foreground));
    if let Some(detail) = why.detail() {
        lines = lines.child(text(detail).font(sans(12.0, 400.0, 16.0)).color(c.muted_foreground));
    }
    let copied = path.to_owned();
    col()
        .key(format!("editor-unavailable-{path}"))
        .grow(1.0)
        .w_full()
        .center()
        .gap(12.0)
        .px(24.0).py(24.0)
        .child(rune::icon(theme::icons::named("FileQuestion").unwrap_or(&theme::icons::FILE_DIFF), 32.0).color(c.muted_foreground))
        .child(lines)
        .child(
            crate::Button::new("Copy Path")
                .variant(crate::ButtonVariant::Ghost)
                .size(crate::ButtonSize::Sm)
                .icon(&theme::icons::COPY)
                .build(t)
                .key("editor-copy-path")
                .on_press_tx(move |_s: &mut S, tx| tx.copy(copied.clone())),
        )
}

/// `EditorDraftConflictBanner`.
fn conflict_bar<S: 'static>(t: &'static Tokens, lens: Lens<S>, path: &str) -> El {
    let c = &t.colors;
    let (keep, disk) = (path.to_owned(), path.to_owned());
    row()
        .key(format!("editor-draft-conflict-{path}"))
        .role("alert")
        .w_full()
        .items_center()
        .justify_between()
        .gap(8.0)
        .px(12.0)
        .py(8.0)
        .fill(c.warning.alpha(0.1))
        .border_sides([0.0, 0.0, 1.0, 0.0], c.warning.alpha(0.5))
        .child(text("An unsaved draft differs from the newer file on disk.").font(sans(12.0, 400.0, 16.0)).color(c.warning))
        .child(
            row()
                .gap(8.0)
                .child(crate::Button::new("Keep my draft").size(crate::ButtonSize::Xs).build(t).key("editor-keep-draft").on_press(move |s: &mut S| {
                    lens(s).step(model::editor::Event::ResolveDraft { path: keep.clone(), keep_draft: true });
                }))
                .child(
                    crate::Button::new("Use disk version")
                        .variant(crate::ButtonVariant::Secondary)
                        .size(crate::ButtonSize::Xs)
                        .build(t)
                        .key("editor-use-disk")
                        .on_press(move |s: &mut S| {
                            lens(s).step(model::editor::Event::ResolveDraft { path: disk.clone(), keep_draft: false });
                        }),
                ),
        )
}

/// Whether a key in a find field is the field's own (Enter, Escape).
pub(super) fn on_field_key<S: 'static>(s: &mut S, lens: Lens<S>, path: &str, which: Field, key: &rune::KeyEvent, tx: &mut Tx) -> bool {
    let editors = lens(s);
    match editors.field_chord(path, key) {
        Outcome::Focus(next) => {
            tx.focus(field_key(path, next), false);
            return true;
        }
        Outcome::Ignored => {}
        _ => return true,
    }
    match (&key.key, which) {
        (Key::Escape, _) => {
            editors.close_find(path);
            tx.focus(surface_key(path), false);
            true
        }
        (Key::Enter, Field::GoTo) => {
            if editors.go_to_line(path) {
                tx.focus(surface_key(path), false);
                if let Some(doc) = editors.docs.docs.get(path) {
                    let line = doc.editor.rope().char_to_line(doc.editor.selection().head);
                    tx.scroll_into_view(caret_key(path, line));
                }
            }
            true
        }
        (Key::Enter, Field::Query) => {
            editors.find_step(path, !key.mods.shift);
            if let Some(doc) = editors.docs.docs.get(path) {
                let line = doc.editor.rope().char_to_line(doc.editor.selection().head);
                tx.scroll_into_view(caret_key(path, line));
            }
            true
        }
        (Key::Enter, Field::Replace) => {
            editors.replace_next(path);
            true
        }
        _ => false,
    }
}

/// The field for `which`.
pub(super) fn field<S: 'static>(t: &'static Tokens, lens: Lens<S>, path: &str, edit: &rune::TextEdit, which: Field, label: &str, width: f32) -> El {
    let c = &t.colors;
    let (edited, keyed) = (path.to_owned(), path.to_owned());
    text_input(edit, sans(12.0, 400.0, 16.0))
        .key(field_key(path, which))
        .single_line()
        .label(label.to_owned())
        .placeholder(label.to_owned(), c.muted_foreground)
        .color(c.foreground)
        .w(width)
        .px(8.0)
        .py(4.0)
        .on_edit(move |s: &mut S| lens(s).field(&edited, which))
        .on_input_key(move |s: &mut S, key, tx| on_field_key(s, lens, &keyed, which, key, tx))
}
