//! Selection and copy, as xterm.js's `SelectionService` does them: a drag selects cells, a
//! double click words and a triple click lines, and the copied text joins a wrapped row to the
//! row above it and puts a newline between other rows.

use std::collections::VecDeque;

use crate::Line;
use crate::text::{Point, line_text, logical_bounds};

/// What a drag extends by.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Unit {
    #[default]
    Cell,
    Word,
    Line,
}

impl Unit {
    /// The unit a run of `clicks` presses selects by.
    pub fn of_clicks(clicks: u32) -> Unit {
        match clicks {
            0 | 1 => Unit::Cell,
            2 => Unit::Word,
            _ => Unit::Line,
        }
    }
}

/// The characters that end a word for a double click (xterm.js's `wordSeparator`).
const WORD_SEPARATORS: &str = " ()[]{}',\"`";

/// A selection from the press (`anchor`) to where the pointer is (`head`). Points are in
/// [`crate::Term::lines`]; when the scrollback drops lines, [`Selection::shift`] follows them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Selection {
    pub anchor: Point,
    pub head: Point,
    pub unit: Unit,
}

impl Selection {
    pub fn new(at: Point, unit: Unit) -> Selection {
        Selection { anchor: at, head: at, unit }
    }

    /// Every cell of every line from `first` to `last` (Select All).
    pub fn all(lines: &VecDeque<Line>) -> Selection {
        let last = lines.len().saturating_sub(1);
        Selection { anchor: Point::new(0, 0), head: Point::new(last, 0), unit: Unit::Line }
    }

    /// The selected cells: the start, and the end column on the end line (exclusive). A cell
    /// selection that has not moved selects nothing.
    pub fn range(&self, lines: &VecDeque<Line>) -> Option<(Point, Point)> {
        let (a, b) = if self.anchor <= self.head { (self.anchor, self.head) } else { (self.head, self.anchor) };
        match self.unit {
            Unit::Cell => (a != b).then_some((a, b)),
            Unit::Word => Some((word_at(lines, a).0, word_at(lines, b).1)),
            Unit::Line => {
                let start = logical_bounds(lines, a.line).0;
                let end = logical_bounds(lines, b.line).1;
                let cols = lines.get(end).map_or(0, |l| l.cells.len());
                Some((Point::new(start, 0), Point::new(end, cols)))
            }
        }
    }

    /// The text a copy puts on the clipboard.
    pub fn text(&self, lines: &VecDeque<Line>) -> String {
        self.range(lines).map(|(a, b)| range_text(lines, a, b)).unwrap_or_default()
    }

    /// The selection after the buffer dropped `n` lines from its top; None once its start went.
    pub fn shift(self, n: usize) -> Option<Selection> {
        if n == 0 {
            return Some(self);
        }
        let up = |p: Point| Some(Point::new(p.line.checked_sub(n)?, p.col));
        Some(Selection { anchor: up(self.anchor)?, head: up(self.head)?, unit: self.unit })
    }
}

/// The text of the cells from `start` to `end` (exclusive), as `_getSelectionText` reads it:
/// each row trimmed on the right, a wrapped row appended to the row before it, other rows
/// joined by a newline.
pub fn range_text(lines: &VecDeque<Line>, start: Point, end: Point) -> String {
    let mut rows: Vec<String> = Vec::new();
    for i in start.line..=end.line {
        let Some(line) = lines.get(i) else { break };
        let from = if i == start.line { start.col } else { 0 };
        let to = if i == end.line { end.col } else { line.cells.len() };
        let text = line_text(line, from, to, true);
        match rows.last_mut() {
            Some(last) if line.wrapped => last.push_str(&text),
            _ => rows.push(text),
        }
    }
    rows.join("\n").replace('\u{a0}', " ")
}

/// The word around `at` and the cell after it, as a double click selects it. A press on a
/// separator selects that cell alone.
pub fn word_at(lines: &VecDeque<Line>, at: Point) -> (Point, Point) {
    let Some(line) = lines.get(at.line) else { return (at, at) };
    let cols = line.cells.len();
    let is_word = |col: usize| {
        line.cells.get(col).is_some_and(|c| {
            if c.width == 0 {
                return true;
            }
            let t = c.text();
            !t.is_empty() && !t.chars().all(|ch| WORD_SEPARATORS.contains(ch))
        })
    };
    let mut col = at.col.min(cols.saturating_sub(1));
    // A press on a wide character's right half selects the character.
    while col > 0 && line.cells[col].width == 0 {
        col -= 1;
    }
    if !is_word(col) {
        let w = line.cells.get(col).map_or(1, |c| usize::from(c.width.max(1)));
        return (Point::new(at.line, col), Point::new(at.line, col + w));
    }
    let mut start = col;
    while start > 0 && is_word(start - 1) {
        start -= 1;
    }
    let mut end = col;
    while end < cols && is_word(end) {
        end += 1;
    }
    (Point::new(at.line, start), Point::new(at.line, end))
}

#[cfg(test)]
#[path = "selection_tests.rs"]
mod tests;
