//! Cells read back as text, as xterm.js's `translateToString` reads them: a blank cell is a
//! space, a wide character's right half is nothing, and a row the terminal wrapped continues
//! the row above it.

use std::collections::VecDeque;

use crate::Line;

/// A cell of a buffer: `line` indexes [`crate::Term::lines`], `col` a column.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Point {
    pub line: usize,
    pub col: usize,
}

impl Point {
    pub fn new(line: usize, col: usize) -> Point {
        Point { line, col }
    }
}

/// The columns of `line` after its last cell with content.
pub fn trimmed_len(line: &Line) -> usize {
    line.cells.iter().rposition(|c| c.has_content()).map_or(0, |i| i + usize::from(line.cells[i].width.max(1)))
}

/// Columns `start..end` of `line` as text; with `trim` the blank cells after the last content
/// are left out (`translateToString(true, start, end)`).
pub fn line_text(line: &Line, start: usize, end: usize, trim: bool) -> String {
    let end = if trim { end.min(trimmed_len(line)) } else { end.min(line.cells.len()) };
    let mut s = String::new();
    let mut col = start;
    while col < end {
        let cell = &line.cells[col];
        if cell.width == 0 {
            col += 1;
            continue;
        }
        let text = cell.text();
        if text.is_empty() {
            s.push(' ');
        } else {
            s.push_str(&text);
        }
        col += usize::from(cell.width.max(1));
    }
    s
}

/// The first and last line of the wrapped run holding line `i`.
pub fn logical_bounds(lines: &VecDeque<Line>, i: usize) -> (usize, usize) {
    let mut first = i;
    while first > 0 && lines.get(first).is_some_and(|l| l.wrapped) {
        first -= 1;
    }
    let mut last = i;
    while lines.get(last + 1).is_some_and(|l| l.wrapped) {
        last += 1;
    }
    (first, last)
}

/// Lines `first..=last` as one string, with the cell each character starts at: `cells[k]` is
/// the cell of the character starting at byte `offsets[k]`. A trailing entry maps the end of
/// the string to the cell after the text.
pub struct Logical {
    pub text: String,
    pub offsets: Vec<usize>,
    pub cells: Vec<Point>,
}

impl Logical {
    pub fn read(lines: &VecDeque<Line>, first: usize, last: usize) -> Logical {
        let mut out = Logical { text: String::new(), offsets: Vec::new(), cells: Vec::new() };
        let mut end = Point::new(first, 0);
        for i in first..=last {
            let Some(line) = lines.get(i) else { break };
            let stop = if i == last { trimmed_len(line) } else { line.cells.len() };
            let mut col = 0;
            while col < stop {
                let cell = &line.cells[col];
                if cell.width == 0 {
                    col += 1;
                    continue;
                }
                out.offsets.push(out.text.len());
                out.cells.push(Point::new(i, col));
                let text = cell.text();
                out.text.push_str(if text.is_empty() { " " } else { &text });
                col += usize::from(cell.width.max(1));
            }
            end = Point::new(i, col);
        }
        out.offsets.push(out.text.len());
        out.cells.push(end);
        out
    }

    /// The cell where the character at byte `offset` starts, or the cell after the text.
    pub fn cell_at(&self, offset: usize) -> Point {
        let k = self.offsets.partition_point(|&o| o < offset);
        self.cells[k.min(self.cells.len() - 1)]
    }
}
