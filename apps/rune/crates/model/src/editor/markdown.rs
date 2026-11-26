//! A Markdown file as the preview editor shows it: a list of top-level blocks, each a run of
//! whole source lines. The source stays the only copy of the document. A block is drawn
//! rendered, except the one under the caret, which is drawn as its source lines; so an edit
//! changes exactly the characters typed, and a file opened and saved with no edit is written
//! back byte for byte (`ui/src/editor/markdown_tests.rs`).
//!
//! The split follows CommonMark's block starts closely enough to render each block alone: a
//! front matter block at the top, fenced code (a `mermaid` fence is a diagram), ATX headings,
//! and runs of other non-blank lines. Blank lines separate blocks and belong to none.

use std::ops::Range;

use troupe_editor::{Rope, line_len};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// `---` ... `---` (or `...`) at the very top.
    Frontmatter,
    /// `#` to `######`.
    Heading(u8),
    /// A fenced code block; `mermaid` when its info string names mermaid.
    Fence { mermaid: bool },
    /// Paragraphs, lists, quotes, tables, HTML, indented code: everything else.
    Prose,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Block {
    pub kind: Kind,
    /// Source lines, end exclusive.
    pub lines: Range<usize>,
}

/// A fence opener's character and run length, after up to three spaces of indent.
fn fence(line: &str) -> Option<(char, usize)> {
    let trimmed = line.trim_start_matches(' ');
    if line.len() - trimmed.len() > 3 {
        return None;
    }
    let ch = trimmed.chars().next().filter(|c| *c == '`' || *c == '~')?;
    let run = trimmed.chars().take_while(|c| *c == ch).count();
    (run >= 3 && !(ch == '`' && trimmed[run..].contains('`'))).then_some((ch, run))
}

fn closes(line: &str, (ch, run): (char, usize)) -> bool {
    fence(line).is_some_and(|(c, n)| c == ch && n >= run && line.trim_start().trim_start_matches(ch).trim().is_empty())
}

fn heading(line: &str) -> Option<u8> {
    let trimmed = line.trim_start_matches(' ');
    if line.len() - trimmed.len() > 3 {
        return None;
    }
    let level = trimmed.chars().take_while(|c| *c == '#').count();
    let rest = &trimmed[level..];
    ((1..=6).contains(&level) && (rest.is_empty() || rest.starts_with([' ', '\t']))).then_some(level as u8)
}

/// The blocks of `text`, in order.
#[must_use]
pub fn blocks(text: &Rope) -> Vec<Block> {
    let n = text.len_lines();
    let line = |i: usize| -> String {
        let start = text.line_to_char(i);
        text.slice(start..start + line_len(text, i)).to_string()
    };
    let mut out = Vec::new();
    let mut i = 0;
    if n > 1 && line(0) == "---"
        && let Some(end) = (1..n).find(|&j| matches!(line(j).as_str(), "---" | "..."))
    {
        out.push(Block { kind: Kind::Frontmatter, lines: 0..end + 1 });
        i = end + 1;
    }
    while i < n {
        let current = line(i);
        if current.trim().is_empty() {
            i += 1;
            continue;
        }
        if let Some(open) = fence(&current) {
            let info = current.trim_start().trim_start_matches(open.0).trim();
            let mermaid = info.split_whitespace().next().is_some_and(|w| w.eq_ignore_ascii_case("mermaid"));
            let end = (i + 1..n).find(|&j| closes(&line(j), open)).map_or(n, |j| j + 1);
            out.push(Block { kind: Kind::Fence { mermaid }, lines: i..end });
            i = end;
            continue;
        }
        if let Some(level) = heading(&current) {
            out.push(Block { kind: Kind::Heading(level), lines: i..i + 1 });
            i += 1;
            continue;
        }
        let start = i;
        i += 1;
        while i < n {
            let next = line(i);
            if next.trim().is_empty() || fence(&next).is_some() || heading(&next).is_some() {
                break;
            }
            i += 1;
        }
        out.push(Block { kind: Kind::Prose, lines: start..i });
    }
    out
}

/// The block that holds `line`, if any.
#[must_use]
pub fn at_line(blocks: &[Block], line: usize) -> Option<usize> {
    blocks.iter().position(|b| b.lines.contains(&line))
}

/// The source of `block`: its lines with their breaks, the last break dropped.
#[must_use]
pub fn source(text: &Rope, block: &Block) -> String {
    let start = text.line_to_char(block.lines.start);
    let last = block.lines.end - 1;
    let end = text.line_to_char(last) + line_len(text, last);
    text.slice(start..end).to_string()
}

/// `key: value` pairs of a front matter block, in order; other lines are skipped.
#[must_use]
pub fn properties(source: &str) -> Vec<(String, String)> {
    source
        .lines()
        .filter(|l| !matches!(*l, "---" | "..."))
        .filter_map(|l| l.split_once(':'))
        .filter(|(k, _)| !k.starts_with([' ', '\t', '-']) && !k.trim().is_empty())
        .map(|(k, v)| (k.trim().to_owned(), v.trim().trim_matches('"').to_owned()))
        .collect()
}

#[cfg(test)]
#[path = "markdown_tests.rs"]
mod tests;
