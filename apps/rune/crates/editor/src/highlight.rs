//! Syntax tags for the editor: rune-highlights' lexers, each token named by the Lezer tag the
//! React editor's grammar gives the same text, so the view colours it from the theme exactly as
//! `codemirror-theme.ts`'s `HighlightStyle` does. Only the languages the React editor loads a
//! grammar for are lexed (`use-codemirror.ts` `loadLanguage`); the rest draw plain, as they do
//! there.
//!
//! The view lexes only the chunks on screen. A chunk is lexed together with up to
//! [`LOOKBACK`] lines before it, so a block comment or a long string that starts above the
//! chunk still colours it. `docs/rune/decisions.md` ("Code editor highlighting") records the
//! choice and `tools/parity/editor-highlight/` measures it against CodeMirror.

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::ops::Range;

use rune_highlights::{Highlighter, Lang, Token};
use ropey::Rope;

/// Lines lexed above a chunk for context.
pub const LOOKBACK: usize = 64;

/// The style a token gets: one entry of `codemirror-theme.ts`'s `buildHighlightStyle`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Tag {
    Keyword,
    Comment,
    String,
    Number,
    Bool,
    Function,
    Property,
    Definition,
    Variable,
    Type,
    Operator,
    Punctuation,
    TagName,
    AttributeName,
    AttributeValue,
    Heading,
    Link,
    Emphasis,
    Strong,
}

impl Tag {
    /// The Lezer tag names CodeMirror's parity dump prints for this style.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Tag::Keyword => "keyword",
            Tag::Comment => "comment",
            Tag::String => "string",
            Tag::Number => "number",
            Tag::Bool => "bool",
            Tag::Function => "function",
            Tag::Property => "property",
            Tag::Definition => "definition",
            Tag::Variable => "variable",
            Tag::Type => "type",
            Tag::Operator => "operator",
            Tag::Punctuation => "punctuation",
            Tag::TagName => "tag",
            Tag::AttributeName => "attributeName",
            Tag::AttributeValue => "attributeValue",
            Tag::Heading => "heading",
            Tag::Link => "link",
            Tag::Emphasis => "emphasis",
            Tag::Strong => "strong",
        }
    }
}

/// The React editor's language for a path (`detectLanguage` in `editor-file-kind.ts`), when
/// it loads a grammar for it.
#[must_use]
pub fn language_for_path(path: &str) -> Option<&'static str> {
    let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
    let dot = name.rfind('.').filter(|i| *i > 0)?;
    match name[dot + 1..].to_ascii_lowercase().as_str() {
        "ts" | "tsx" => Some("typescript"),
        "js" | "jsx" => Some("javascript"),
        "json" => Some("json"),
        "css" | "scss" => Some("css"),
        "html" | "htm" => Some("html"),
        "md" => Some("markdown"),
        "yaml" | "yml" => Some("yaml"),
        "py" => Some("python"),
        "rs" => Some("rust"),
        _ => None,
    }
}

/// The style of a token rune-highlights names `token` in `lang`, or None for plain text.
pub(crate) fn tag_of(lang: &str, token: Token) -> Option<Tag> {
    let name = token.name();
    let tag = match name {
        "comment" | "comment.doc" | "keyword.jsdoc" | "type.jsdoc" | "variable.jsdoc" => Tag::Comment,
        "string" | "string.special" | "string.special.symbol" | "string.escape" | "string.regex" | "text.literal" => {
            Tag::String
        }
        "number" => Tag::Number,
        "boolean" | "constant.builtin" => Tag::Bool,
        "function" | "function.method" | "function.definition" => Tag::Function,
        "property" | "property.json_key" => Tag::Property,
        "variable.parameter" => Tag::Definition,
        "variable" | "constant" => Tag::Variable,
        "type" | "type.builtin" | "type.class" | "enum" | "constructor" | "variant" | "tag.component.jsx" => Tag::Type,
        "operator" => Tag::Operator,
        "punctuation" | "punctuation.bracket" | "punctuation.delimiter" | "punctuation.special"
        | "punctuation.list_marker" | "punctuation.markup" | "punctuation.bracket.html"
        | "punctuation.delimiter.html" | "punctuation.bracket.jsx" | "punctuation.delimiter.jsx" => Tag::Punctuation,
        "tag" | "tag.jsx" | "tag.doctype" => Tag::TagName,
        "attribute" | "attribute.jsx" => Tag::AttributeName,
        "title" => Tag::Heading,
        "link_text" | "link_uri" => Tag::Link,
        "emphasis" => Tag::Emphasis,
        "emphasis.strong" => Tag::Strong,
        "selector" => Tag::TagName,
        "selector.class" | "selector.pseudo" => Tag::Type,
        "preproc" => Tag::Keyword,
        "variable.special" => match lang {
            "python" | "rust" => Tag::Variable,
            _ => Tag::Keyword,
        },
        n if n.starts_with("keyword") => Tag::Keyword,
        _ => return None,
    };
    Some(tag)
}

/// Spans of one line: byte ranges within the line, in order, without trailing spaces.
pub type LineSpans = Vec<(Range<usize>, Tag)>;

/// A lexer for one file and the tags of the chunks it lexed.
pub struct Highlights {
    lexer: Highlighter,
    lang: Option<(&'static str, Lang)>,
    /// Chunk start line to (hash of the lexed text, the chunk's lines).
    cache: HashMap<usize, (u64, Vec<LineSpans>)>,
}

impl Highlights {
    #[must_use]
    pub fn for_path(path: &str) -> Self {
        let lang = language_for_path(path).and_then(|name| Lang::from_name(name).map(|l| (name, l)));
        Self { lexer: Highlighter::new(), lang, cache: HashMap::new() }
    }

    #[must_use]
    pub fn language(&self) -> Option<&'static str> {
        self.lang.map(|(name, _)| name)
    }

    /// The spans of lines `lines` of `text`, lexed with up to [`LOOKBACK`] lines above them.
    /// Reuses the last result for the same chunk while the lexed text is unchanged.
    pub fn chunk(&mut self, text: &Rope, lines: Range<usize>) -> &[LineSpans] {
        let Some((name, lang)) = self.lang else {
            return &[];
        };
        let first = lines.start.saturating_sub(LOOKBACK);
        let end_line = lines.end.min(text.len_lines());
        let from = text.line_to_byte(first);
        let to = text.line_to_byte(end_line);
        let source = text.byte_slice(from..to);
        let mut hasher = DefaultHasher::new();
        for chunk in source.chunks() {
            chunk.hash(&mut hasher);
        }
        let hash = hasher.finish();
        let fresh = self.cache.get(&lines.start).is_none_or(|(h, _)| *h != hash);
        if fresh {
            let source = source.to_string();
            let tokens = self.lexer.highlight(lang, &source);
            let spans = split_lines(name, &source, &tokens, lines.start - first, end_line - lines.start);
            self.cache.insert(lines.start, (hash, spans));
        }
        &self.cache[&lines.start].1
    }

    /// Drops every cached chunk (a file reloaded from disk).
    pub fn clear(&mut self) {
        self.cache.clear();
    }
}

/// Cuts the styled `tokens` of `source` into per-line spans, skipping the first `skip` lines and keeping
/// `keep`. A token across a line break splits at it.
fn split_lines(lang: &str, source: &str, tokens: &[(Range<usize>, Token)], skip: usize, keep: usize) -> Vec<LineSpans> {
    let mut starts = vec![0];
    starts.extend(source.match_indices('\n').map(|(i, _)| i + 1));
    let mut out: Vec<LineSpans> = vec![Vec::new(); keep];
    for (range, tag) in crate::recut::recut(lang, source, tokens) {
        let mut line = starts.partition_point(|s| *s <= range.start) - 1;
        let mut at = range.start;
        while at < range.end {
            let line_end = starts.get(line + 1).map_or(source.len(), |s| s - 1);
            let end = range.end.min(line_end);
            let trimmed = at + source[at..end].trim_end().len();
            if trimmed > at && line >= skip && line - skip < keep {
                out[line - skip].push((at - starts[line]..trimmed - starts[line], tag));
            }
            line += 1;
            match starts.get(line) {
                Some(next) => at = (*next).max(end),
                None => break,
            }
        }
    }
    out
}

/// The spans of every line of `source` as `path`'s language colours them, lexed whole: the
/// parity dump's reference path.
#[must_use]
pub fn whole(path: &str, source: &str) -> Vec<LineSpans> {
    let mut h = Highlights::for_path(path);
    let Some((name, lang)) = h.lang else {
        return vec![Vec::new(); source.split('\n').count()];
    };
    let tokens = h.lexer.highlight(lang, source);
    split_lines(name, source, &tokens, 0, source.split('\n').count())
}
