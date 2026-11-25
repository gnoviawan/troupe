//! Find and replace and go to line, as the React editor's find widget runs them
//! (`editor/find/codemirror-find.tsx` over CodeMirror's `SearchQuery`): plain text or a
//! regular expression, case-insensitive unless asked, optionally whole words only; empty
//! matches skipped; at most [`MATCH_LIMIT`] counted. Ranges are char offsets.

use std::ops::Range;

use regex_lite::{Regex, RegexBuilder};
use ropey::Rope;

use crate::Editor;

/// CodeMirror's select-all-matches cap; past it the count reads "1000+".
pub const MATCH_LIMIT: usize = 1000;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Query {
    pub text: String,
    pub case_sensitive: bool,
    pub whole_word: bool,
    pub regex: bool,
}

/// The matches of a query, in document order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Matches {
    pub ranges: Vec<Range<usize>>,
    /// More than [`MATCH_LIMIT`] matched.
    pub capped: bool,
}

impl Query {
    /// The compiled pattern, or None when the query is empty or its regex does not compile
    /// (CodeMirror's `query.valid`).
    fn pattern(&self) -> Option<Regex> {
        if self.text.is_empty() {
            return None;
        }
        let source = if self.regex { self.text.clone() } else { regex_lite::escape(&self.text) };
        RegexBuilder::new(&source)
            .case_insensitive(!self.case_sensitive)
            .multi_line(true)
            .build()
            .ok()
    }

    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.pattern().is_some()
    }
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Every non-empty match of `query` in `text` as byte ranges, up to `limit` (None for all).
/// One match: its byte range and its groups' (the whole match first).
type Found = (Range<usize>, Vec<Option<Range<usize>>>);

fn byte_matches(text: &str, query: &Query, limit: Option<usize>) -> (Vec<Found>, bool) {
    let Some(pattern) = query.pattern() else {
        return (Vec::new(), false);
    };
    let mut out = Vec::new();
    for caps in pattern.captures_iter(text) {
        let Some(whole) = caps.get(0) else { continue };
        if whole.is_empty() {
            continue;
        }
        if query.whole_word {
            let before = text[..whole.start()].chars().next_back();
            let after = text[whole.end()..].chars().next();
            if before.is_some_and(is_word) || after.is_some_and(is_word) {
                continue;
            }
        }
        if limit.is_some_and(|l| out.len() == l) {
            return (out, true);
        }
        let groups = caps.iter().map(|g| g.map(|g| g.range())).collect();
        out.push((whole.range(), groups));
    }
    (out, false)
}

/// The matches of `query` in `text`, at most [`MATCH_LIMIT`].
#[must_use]
pub fn matches(text: &Rope, query: &Query) -> Matches {
    let source = text.to_string();
    let (found, capped) = byte_matches(&source, query, Some(MATCH_LIMIT));
    Matches {
        ranges: found
            .into_iter()
            .map(|(r, _)| text.byte_to_char(r.start)..text.byte_to_char(r.end))
            .collect(),
        capped,
    }
}

/// The 1-based place of `range` among `ranges`, or 0 when it is not one of them.
#[must_use]
pub fn match_index(ranges: &[Range<usize>], range: &Range<usize>) -> usize {
    ranges
        .binary_search_by(|r| r.start.cmp(&range.start))
        .ok()
        .filter(|i| ranges[*i].end == range.end)
        .map_or(0, |i| i + 1)
}

/// The first match at or after `from`, wrapping to the top (`findNext`); with `backward`, the
/// last match before `from`, wrapping to the bottom (`findPrevious`).
#[must_use]
pub fn next_match(ranges: &[Range<usize>], from: usize, backward: bool) -> Option<Range<usize>> {
    if backward {
        ranges.iter().rev().find(|r| r.start < from).or(ranges.last()).cloned()
    } else {
        ranges.iter().find(|r| r.start >= from).or(ranges.first()).cloned()
    }
}

/// `replacement` for one match: CodeMirror's `$&` (the match), `$1`..`$9` (groups) and `$$`
/// for a regex query, the literal text otherwise.
fn expand(query: &Query, replacement: &str, source: &str, groups: &[Option<Range<usize>>]) -> String {
    if !query.regex {
        return replacement.to_owned();
    }
    let mut out = String::new();
    let mut chars = replacement.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '$' {
            out.push(c);
            continue;
        }
        match chars.peek().copied() {
            Some('$') => {
                chars.next();
                out.push('$');
            }
            Some('&') => {
                chars.next();
                out.push_str(groups[0].clone().map_or("", |r| &source[r]));
            }
            Some(d @ '1'..='9') => {
                chars.next();
                let i = d as usize - '0' as usize;
                out.push_str(groups.get(i).cloned().flatten().map_or("", |r| &source[r]));
            }
            _ => out.push('$'),
        }
    }
    out
}

/// Replaces the match at the selection, if the selection is one, and selects the next match
/// (`replaceNext`). Returns whether it replaced.
pub fn replace_next(ed: &mut Editor, query: &Query, replacement: &str, now: f64) -> bool {
    let source = ed.text();
    let (found, _) = byte_matches(&source, query, None);
    let rope = ed.rope();
    let sel = ed.selection().range();
    let hit = found
        .iter()
        .find(|(r, _)| rope.byte_to_char(r.start) == sel.start && rope.byte_to_char(r.end) == sel.end);
    let Some((_, groups)) = hit else {
        if let Some(next) = next_match(&matches(ed.rope(), query).ranges, sel.start, false) {
            ed.apply(crate::Command::Select(crate::Selection { anchor: next.start, head: next.end }), now);
        }
        return false;
    };
    let text = expand(query, replacement, &source, groups);
    let at = sel.start;
    ed.replace(sel, &text, now);
    let after = at + text.chars().count();
    if let Some(next) = next_match(&matches(ed.rope(), query).ranges, after, false) {
        ed.apply(crate::Command::Select(crate::Selection { anchor: next.start, head: next.end }), now);
    }
    true
}

/// Replaces every match (not only the first [`MATCH_LIMIT`]) as one undo step. Returns how
/// many it replaced.
pub fn replace_all(ed: &mut Editor, query: &Query, replacement: &str, now: f64) -> usize {
    let source = ed.text();
    let (found, _) = byte_matches(&source, query, None);
    let rope = ed.rope();
    let edits: Vec<(Range<usize>, String)> = found
        .iter()
        .map(|(r, groups)| (rope.byte_to_char(r.start)..rope.byte_to_char(r.end), expand(query, replacement, &source, groups)))
        .collect();
    let n = edits.len();
    ed.replace_many(edits, now);
    n
}

/// The offset a go-to-line entry names, or None when it names none. The grammar is
/// CodeMirror's `gotoLine` (`12`, `+5`, `-5`, `50%`, `12:4`), with columns from 1
/// (`resolveGoToLine`).
#[must_use]
pub fn go_to_line(ed: &Editor, input: &str) -> Option<usize> {
    let input = input.trim();
    let (sign, rest) = match input.chars().next() {
        Some(c @ ('+' | '-')) => (Some(c), &input[1..]),
        _ => (None, input),
    };
    let (rest, percent) = match rest.strip_suffix('%') {
        Some(r) => (r, true),
        None => (rest, false),
    };
    let (line_text, column_text) = match rest.split_once(':') {
        Some((l, c)) => (l, Some(c)),
        None => (rest, None),
    };
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    if !(line_text.is_empty() || digits(line_text)) || column_text.is_some_and(|c| !digits(c)) {
        return None;
    }
    if line_text.is_empty() && column_text.is_none() {
        return None;
    }
    let lines = ed.len_lines() as f64;
    let current = ed.line_col().0 as f64;
    let mut line = if line_text.is_empty() { current } else { line_text.parse::<f64>().ok()? };
    if !line_text.is_empty() && percent {
        let fraction = line / 100.0;
        line = match sign {
            Some(s) => (lines * (fraction * if s == '-' { -1.0 } else { 1.0 } + current / lines)).round(),
            None => (lines * fraction).round(),
        };
    } else if !line_text.is_empty() && let Some(s) = sign {
        line = if s == '-' { current - line } else { current + line };
    }
    let line = line.clamp(1.0, lines) as usize;
    let column = column_text.map_or(Some(1), |c| c.parse().ok())?;
    Some(ed.offset_of(line, column))
}
