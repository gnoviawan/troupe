//! Where a move takes the caret, as CodeMirror's default keymap moves it: by character, by
//! group (`cursorGroupLeft`: a run of word characters, of other non-space characters, or of
//! spaces then the run after them), by row (wrapped rows when the editor wraps), to the
//! line's indent then its start (Home), and by page.

use crate::{Editor, line_len, wrap};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Motion {
    Left,
    Right,
    WordLeft,
    WordRight,
    Up,
    Down,
    /// Home: the end of the indent, or the line start when the caret is there already.
    LineStart,
    LineEnd,
    DocStart,
    DocEnd,
    /// Up or down by this many rows.
    PageUp(usize),
    PageDown(usize),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Group {
    Word,
    Space,
    Other,
    Break,
}

fn group(c: char) -> Group {
    if c == '\n' {
        Group::Break
    } else if c.is_whitespace() {
        Group::Space
    } else if c.is_alphanumeric() || c == '_' {
        Group::Word
    } else {
        Group::Other
    }
}

/// The caret offset `motion` reaches from the editor's caret. Updates the vertical goal.
pub fn target(ed: &mut Editor, motion: Motion) -> usize {
    let text = &ed.text;
    let head = ed.sel.head;
    let len = text.len_chars();
    match motion {
        Motion::Left => head.saturating_sub(1),
        Motion::Right => (head + 1).min(len),
        Motion::WordLeft => {
            let mut at = head;
            if at > 0 && text.char(at - 1) == '\n' {
                return at - 1;
            }
            while at > 0 && group(text.char(at - 1)) == Group::Space {
                at -= 1;
            }
            if at > 0 {
                let g = group(text.char(at - 1));
                while at > 0 && group(text.char(at - 1)) == g && g != Group::Break {
                    at -= 1;
                }
            }
            at
        }
        Motion::WordRight => {
            let mut at = head;
            if at < len && text.char(at) == '\n' {
                return at + 1;
            }
            while at < len && group(text.char(at)) == Group::Space {
                at += 1;
            }
            if at < len {
                let g = group(text.char(at));
                while at < len && group(text.char(at)) == g && g != Group::Break {
                    at += 1;
                }
            }
            at
        }
        Motion::Up => vertical(ed, -1),
        Motion::Down => vertical(ed, 1),
        Motion::PageUp(rows) => vertical(ed, -(rows.max(1) as isize)),
        Motion::PageDown(rows) => vertical(ed, rows.max(1) as isize),
        Motion::LineStart => {
            let line = text.char_to_line(head);
            let start = text.line_to_char(line);
            let indent = text.line(line).chars().take_while(|c| *c == ' ' || *c == '\t').count();
            let indent = start + indent.min(line_len(text, line));
            if head == indent { start } else { indent }
        }
        Motion::LineEnd => {
            let line = text.char_to_line(head);
            text.line_to_char(line) + line_len(text, line)
        }
        Motion::DocStart => 0,
        Motion::DocEnd => len,
    }
}

/// `rows` visual rows up (negative) or down from the caret, aiming for the goal column. At the
/// first or last row the caret goes to the text's start or end, as CodeMirror's does.
fn vertical(ed: &mut Editor, rows: isize) -> usize {
    let text = &ed.text;
    let head = ed.sel.head;
    let line = text.char_to_line(head);
    let line_start = text.line_to_char(line);
    let starts = row_starts(ed, line);
    let row = starts.iter().rposition(|s| *s <= head - line_start).unwrap_or(0);
    let goal = *ed.goal.get_or_insert(head - line_start - starts[row]);
    let (mut line, mut row) = (line, row as isize + rows);
    loop {
        if row < 0 {
            if line == 0 {
                return 0;
            }
            line -= 1;
            row += row_starts(ed, line).len() as isize;
        } else {
            let n = row_starts(ed, line).len() as isize;
            if row < n {
                break;
            }
            if line + 1 >= ed.text.len_lines() {
                return ed.text.len_chars();
            }
            row -= n;
            line += 1;
        }
    }
    let starts = row_starts(ed, line);
    let row = row as usize;
    let row_end = starts.get(row + 1).map_or(line_len(&ed.text, line), |next| next.saturating_sub(1).max(starts[row]));
    ed.text.line_to_char(line) + (starts[row] + goal).min(row_end)
}

fn row_starts(ed: &Editor, line: usize) -> Vec<usize> {
    if ed.wrap == 0 {
        return vec![0];
    }
    wrap::rows(ed.text.line(line).chars().filter(|c| *c != '\n'), ed.wrap)
}
