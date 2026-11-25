//! Search over the scrollback and the screen, as xterm.js's `SearchAddon` finds matches: each
//! wrapped run of rows reads as one line, a plain query matches its text case-insensitively,
//! and a regex query runs as written. Matches come back in buffer order with the cells they
//! cover, for the view to decorate.

use std::collections::VecDeque;

use regex_lite::{Regex, RegexBuilder};

use crate::Line;
use crate::text::{Logical, Point, logical_bounds};

/// What to look for (`ISearchOptions`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Query {
    pub text: String,
    pub regex: bool,
    pub case_sensitive: bool,
    pub whole_word: bool,
}

/// One match: from `start` to `end`, the cell after its last character.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Match {
    pub start: Point,
    pub end: Point,
}

/// The compiled query, or why it does not compile.
pub fn compile(query: &Query) -> Result<Regex, String> {
    let pattern = if query.regex { query.text.clone() } else { regex_lite::escape(&query.text) };
    let pattern = if query.whole_word { format!(r"\b(?:{pattern})\b") } else { pattern };
    RegexBuilder::new(&pattern).case_insensitive(!query.case_sensitive).build().map_err(|e| e.to_string())
}

/// Every match of `re` in `lines`, in order. Empty matches are skipped.
pub fn find_all(lines: &VecDeque<Line>, re: &Regex, limit: usize) -> Vec<Match> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < lines.len() && out.len() < limit {
        let (first, last) = logical_bounds(lines, i);
        let logical = Logical::read(lines, first, last);
        for m in re.find_iter(&logical.text) {
            if m.start() == m.end() {
                continue;
            }
            if out.len() == limit {
                break;
            }
            out.push(Match { start: logical.cell_at(m.start()), end: logical.cell_at(m.end()) });
        }
        i = last + 1;
    }
    out
}

/// The index of the match `find_next` lands on from `from` (the cell after the last result,
/// or the cursor), wrapping to the first.
pub fn next_after(matches: &[Match], from: Point) -> Option<usize> {
    if matches.is_empty() {
        return None;
    }
    Some(matches.iter().position(|m| m.start >= from).unwrap_or(0))
}

/// The index of the match `find_previous` lands on before `from`, wrapping to the last.
pub fn previous_before(matches: &[Match], from: Point) -> Option<usize> {
    if matches.is_empty() {
        return None;
    }
    Some(matches.iter().rposition(|m| m.start < from).unwrap_or(matches.len() - 1))
}

#[cfg(test)]
#[path = "search_tests.rs"]
mod tests;
