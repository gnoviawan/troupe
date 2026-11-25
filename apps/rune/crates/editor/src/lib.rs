//! Troupe's code editor model for the Rune client.
//!
//! [`Editor`] holds what the React editor's CodeMirror state holds for one file: the text in a
//! [`ropey::Rope`], the selection, and the undo history. Every user action is a [`Command`] run
//! through [`Editor::apply`], so a test replays recorded input the way the view sends it. The
//! moves and edits follow CodeMirror's default keymap as `use-codemirror.ts` configures it:
//! one caret (no `allowMultipleSelections`), Tab and Shift+Tab indent lines by two spaces
//! (`indentWithTab`), Enter keeps the line's indent, and Home goes to the indent first.
//!
//! [`find`] holds find and replace and go to line, [`wrap`] the soft wrap of a line, and
//! [`highlight`] the syntax tags. The crate has no UI and no I/O and compiles for wasm.
//! `docs/rune/decisions.md` ("Code editor buffer") records why a rope with snapshot undo.

#![forbid(unsafe_code)]

pub mod find;
pub mod highlight;
mod history;
mod motion;
mod recut;
pub mod wrap;

#[cfg(test)]
mod buffer_tests;
#[cfg(test)]
mod find_tests;
#[cfg(test)]
mod highlight_tests;
#[cfg(test)]
mod wrap_tests;

use std::ops::Range;

pub use ropey::Rope;

use history::{History, Kind};
pub use motion::Motion;

/// CodeMirror's default `indentUnit`.
pub const INDENT: &str = "  ";

/// The anchor and the caret (`head`), as char offsets into the text. Equal when nothing is
/// selected.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Selection {
    pub anchor: usize,
    pub head: usize,
}

impl Selection {
    #[must_use]
    pub const fn caret(at: usize) -> Self {
        Self { anchor: at, head: at }
    }

    #[must_use]
    pub fn range(self) -> Range<usize> {
        self.anchor.min(self.head)..self.anchor.max(self.head)
    }

    #[must_use]
    pub fn is_empty(self) -> bool {
        self.anchor == self.head
    }
}

/// One user action.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    /// Typed, pasted or composed text, replacing the selection.
    Insert(String),
    /// Enter: a line break that keeps the line's indent, one unit more after an opening bracket.
    Newline,
    Backspace,
    Delete,
    DeleteWordBackward,
    DeleteWordForward,
    /// Tab: every selected line one indent unit deeper.
    Indent,
    /// Shift+Tab: every selected line one indent unit shallower.
    Outdent,
    Move { motion: Motion, extend: bool },
    SelectAll,
    /// A pointer press or drag, or a find match.
    Select(Selection),
    Undo,
    Redo,
}

/// One open file's text, selection and history.
#[derive(Clone)]
pub struct Editor {
    text: Rope,
    sel: Selection,
    history: History,
    /// Bumped by every change to the text.
    revision: u64,
    /// The column Up and Down aim for, kept across a run of vertical moves.
    goal: Option<usize>,
    /// The soft wrap width in columns (0 is no wrap); Up and Down move by wrapped rows.
    wrap: usize,
}

impl Editor {
    /// An editor over `text`, line breaks read as CodeMirror reads them: `\r\n` and `\r`
    /// become `\n`, so a save writes `\n` as the React editor does.
    #[must_use]
    pub fn new(text: &str) -> Self {
        Self {
            text: Rope::from_str(&normalize(text)),
            sel: Selection::default(),
            history: History::default(),
            revision: 0,
            goal: None,
            wrap: 0,
        }
    }

    #[must_use]
    pub fn rope(&self) -> &Rope {
        &self.text
    }

    #[must_use]
    pub fn text(&self) -> String {
        self.text.to_string()
    }

    #[must_use]
    pub fn selection(&self) -> Selection {
        self.sel
    }

    #[must_use]
    pub fn revision(&self) -> u64 {
        self.revision
    }

    #[must_use]
    pub fn len_lines(&self) -> usize {
        self.text.len_lines()
    }

    /// The caret's line and column, both from 1 (CodeMirror's `line.number` and
    /// `head - line.from + 1`).
    #[must_use]
    pub fn line_col(&self) -> (usize, usize) {
        let line = self.text.char_to_line(self.sel.head);
        (line + 1, self.sel.head - self.text.line_to_char(line) + 1)
    }

    pub fn set_wrap(&mut self, columns: usize) {
        self.wrap = columns;
    }

    #[must_use]
    pub fn can_undo(&self) -> bool {
        self.history.can_undo()
    }

    #[must_use]
    pub fn can_redo(&self) -> bool {
        self.history.can_redo()
    }

    /// Runs one action at `now` (seconds; edits within half a second join one undo step).
    /// Returns whether the text or the selection changed.
    pub fn apply(&mut self, command: Command, now: f64) -> bool {
        let before = (self.revision, self.sel);
        if !matches!(command, Command::Move { motion: Motion::Up | Motion::Down | Motion::PageUp(_) | Motion::PageDown(_), .. }) {
            self.goal = None;
        }
        match command {
            Command::Insert(text) => {
                let text = normalize(&text);
                let kind = if text.chars().count() == 1 { Kind::Type } else { Kind::Other };
                self.replace_selection(&text, kind, now);
            }
            Command::Newline => self.newline(now),
            Command::Backspace => self.delete_by(Motion::Left, Kind::DeleteBackward, now),
            Command::Delete => self.delete_by(Motion::Right, Kind::DeleteForward, now),
            Command::DeleteWordBackward => self.delete_by(Motion::WordLeft, Kind::DeleteBackward, now),
            Command::DeleteWordForward => self.delete_by(Motion::WordRight, Kind::DeleteForward, now),
            Command::Indent => self.indent(true, now),
            Command::Outdent => self.indent(false, now),
            Command::Move { motion, extend } => self.move_caret(motion, extend),
            Command::SelectAll => self.sel = Selection { anchor: 0, head: self.text.len_chars() },
            Command::Select(sel) => self.sel = self.clamp(sel),
            Command::Undo => {
                if let Some((text, sel)) = self.history.undo(&self.text, self.sel) {
                    self.restore(text, sel);
                }
            }
            Command::Redo => {
                if let Some((text, sel)) = self.history.redo(&self.text, self.sel) {
                    self.restore(text, sel);
                }
            }
        }
        before != (self.revision, self.sel)
    }

    /// Replaces `range` (char offsets) with `text` as one undo step and puts the caret after
    /// it: a find match's replace.
    pub fn replace(&mut self, range: Range<usize>, text: &str, now: f64) {
        let text = normalize(text);
        let snapshot = (self.text.clone(), self.sel);
        self.splice(range.clone(), &text);
        self.sel = Selection::caret(range.start + text.chars().count());
        self.history.record(snapshot, Kind::Other, self.sel.head, now);
    }

    /// Replaces each of `ranges` (sorted, not overlapping) with its text, as one undo step.
    pub fn replace_many(&mut self, edits: Vec<(Range<usize>, String)>, now: f64) {
        if edits.is_empty() {
            return;
        }
        let edits: Vec<(Range<usize>, String)> = edits.into_iter().map(|(r, t)| (r, normalize(&t))).collect();
        let snapshot = (self.text.clone(), self.sel);
        for (range, text) in edits.iter().rev() {
            self.splice(range.clone(), text);
        }
        let (mut shift, mut end) = (0isize, 0usize);
        for (range, text) in &edits {
            let n = text.chars().count();
            end = range.start.saturating_add_signed(shift) + n;
            shift += n as isize - range.len() as isize;
        }
        self.sel = self.clamp(Selection::caret(end));
        self.history.record(snapshot, Kind::Other, self.sel.head, now);
    }

    /// Swaps the whole text for `text` (a reload from disk) as one undo step, keeping the
    /// caret's offset where it still fits.
    pub fn reset(&mut self, text: &str, now: f64) {
        let head = self.sel.head;
        self.replace(0..self.text.len_chars(), text, now);
        self.sel = self.clamp(Selection::caret(head));
    }

    /// The char offset of `line` and `column` (both from 1), each clamped into the text.
    #[must_use]
    pub fn offset_of(&self, line: usize, column: usize) -> usize {
        let line = line.clamp(1, self.text.len_lines()) - 1;
        let start = self.text.line_to_char(line);
        start + (column.max(1) - 1).min(line_len(&self.text, line))
    }

    fn clamp(&self, sel: Selection) -> Selection {
        let len = self.text.len_chars();
        Selection { anchor: sel.anchor.min(len), head: sel.head.min(len) }
    }

    fn restore(&mut self, text: Rope, sel: Selection) {
        self.text = text;
        self.revision += 1;
        self.sel = self.clamp(sel);
    }

    fn splice(&mut self, range: Range<usize>, text: &str) {
        if !range.is_empty() {
            self.text.remove(range.clone());
        }
        if !text.is_empty() {
            self.text.insert(range.start, text);
        }
        self.revision += 1;
    }

    fn replace_selection(&mut self, text: &str, kind: Kind, now: f64) {
        let range = self.sel.range();
        if range.is_empty() && text.is_empty() {
            return;
        }
        let snapshot = (self.text.clone(), self.sel);
        self.splice(range.clone(), text);
        self.sel = Selection::caret(range.start + text.chars().count());
        self.history.record(snapshot, kind, self.sel.head, now);
    }

    fn delete_by(&mut self, motion: Motion, kind: Kind, now: f64) {
        let snapshot = (self.text.clone(), self.sel);
        let mut range = self.sel.range();
        if range.is_empty() {
            let to = motion::target(self, motion);
            if to == self.sel.head {
                return;
            }
            range = to.min(self.sel.head)..to.max(self.sel.head);
        }
        let at = range.start;
        self.splice(range, "");
        self.sel = Selection::caret(at);
        self.history.record(snapshot, kind, at, now);
    }

    fn newline(&mut self, now: f64) {
        let start = self.sel.range().start;
        let line = self.text.char_to_line(start);
        let line_start = self.text.line_to_char(line);
        let indent: String = self
            .text
            .slice(line_start..start)
            .chars()
            .take_while(|c| *c == ' ' || *c == '\t')
            .collect();
        let before = start.checked_sub(1).map(|i| self.text.char(i));
        let after = self.sel.range().end;
        let next = (after < self.text.len_chars()).then(|| self.text.char(after));
        let opens = matches!(before, Some('{' | '(' | '['));
        let text = match (opens, before, next) {
            (true, Some('{'), Some('}')) | (true, Some('('), Some(')')) | (true, Some('['), Some(']')) => {
                let inner = format!("\n{indent}{INDENT}");
                let snapshot = (self.text.clone(), self.sel);
                self.splice(self.sel.range(), &format!("{inner}\n{indent}"));
                self.sel = Selection::caret(start + inner.chars().count());
                self.history.record(snapshot, Kind::Type, self.sel.head, now);
                return;
            }
            (true, ..) => format!("\n{indent}{INDENT}"),
            _ => format!("\n{indent}"),
        };
        self.replace_selection(&text, Kind::Type, now);
    }

    fn indent(&mut self, deeper: bool, now: f64) {
        let range = self.sel.range();
        let first = self.text.char_to_line(range.start);
        let mut last = self.text.char_to_line(range.end);
        if last > first && range.end == self.text.line_to_char(last) {
            last -= 1;
        }
        let snapshot = (self.text.clone(), self.sel);
        let (mut anchor, mut head) = (self.sel.anchor, self.sel.head);
        let mut changed = false;
        for line in (first..=last).rev() {
            let start = self.text.line_to_char(line);
            let (removed, added) = if deeper {
                self.text.insert(start, INDENT);
                (0, INDENT.len())
            } else {
                let spaces = self.text.slice(start..).chars().take(INDENT.len()).take_while(|c| *c == ' ').count();
                let tab = spaces == 0 && self.text.len_chars() > start && self.text.char(start) == '\t';
                let n = if tab { 1 } else { spaces };
                if n == 0 {
                    continue;
                }
                self.text.remove(start..start + n);
                (n, 0)
            };
            changed = true;
            let shift = |at: usize| -> usize {
                if at < start {
                    at
                } else {
                    (at + added).saturating_sub(removed).max(start)
                }
            };
            anchor = shift(anchor);
            head = shift(head);
        }
        if changed {
            self.revision += 1;
            self.sel = Selection { anchor, head };
            self.history.record(snapshot, Kind::Other, self.sel.head, now);
        }
    }

    fn move_caret(&mut self, motion: Motion, extend: bool) {
        if !extend && !self.sel.is_empty() && matches!(motion, Motion::Left | Motion::Right) {
            let range = self.sel.range();
            self.sel = Selection::caret(if motion == Motion::Left { range.start } else { range.end });
            return;
        }
        let head = motion::target(self, motion);
        self.sel = if extend { Selection { anchor: self.sel.anchor, head } } else { Selection::caret(head) };
    }
}

/// `line`'s length in chars without its line break.
#[must_use]
pub fn line_len(text: &Rope, line: usize) -> usize {
    let slice = text.line(line);
    let n = slice.len_chars();
    if n > 0 && slice.char(n - 1) == '\n' { n - 1 } else { n }
}

fn normalize(text: &str) -> String {
    if text.contains('\r') {
        text.replace("\r\n", "\n").replace('\r', "\n")
    } else {
        text.to_owned()
    }
}
