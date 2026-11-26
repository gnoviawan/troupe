//! Where rune-highlights and the React editor's Lezer grammars name the same text differently,
//! re-cut the lexer's tokens to the style CodeMirror gives them. Each rule is measured by
//! `tools/parity/editor-highlight/parity.ts`; the decision record lists the gaps left.

use std::ops::Range;

use rune_highlights::Token;

use crate::highlight::{Tag, tag_of};

/// The styled spans of `source`, from the lexer's `tokens` in `lang`.
pub fn recut(lang: &str, source: &str, tokens: &[(Range<usize>, Token)]) -> Vec<(Range<usize>, Tag)> {
    let mut out = Vec::with_capacity(tokens.len());
    let mut js = JsScope::default();
    let mut rust = RustScope::default();
    for (i, (range, token)) in tokens.iter().enumerate() {
        let text = source[range.clone()].trim_end();
        let name = token.name();
        let prev = |n: usize| i.checked_sub(n).map(|j| source[tokens[j].0.clone()].trim());
        let after_dot = source[..range.start].trim_end().ends_with('.');
        let after_path = source[..range.start].trim_end().ends_with("::");
        let Some(tag) = tag_of(lang, *token) else {
            if lang == "rust" {
                rust.step(text);
            }
            continue;
        };
        let tag = match (lang, name, tag) {
            (_, "function" | "function.method", _) if after_dot => Tag::Property,
            ("css", "function", _) => Tag::Keyword,
            ("css", "number", _) => {
                split_unit(range.start, text, &mut out);
                continue;
            }
            ("html", "tag.doctype", _) => Tag::Keyword,
            ("rust", _, _) if rust.in_attribute() || (rust.hash && text.starts_with('[')) => Tag::Keyword,
            ("rust", "attribute", _) => Tag::Keyword,
            ("rust", "variable.special", _) => Tag::Keyword,
            ("rust", "variable", _) if rust.binds() => Tag::Definition,
            ("rust", _, Tag::Punctuation) if matches!(text, "." | ":") && !rust.in_attribute() => continue,
            ("rust", _, Tag::Type | Tag::Function) if after_path && !next_is(source, range.end, "::") => Tag::Variable,
            ("rust", _, Tag::Type) if matches!(text, "None" | "Some" | "Ok" | "Err") => Tag::Variable,
            ("python", _, Tag::Punctuation) if text == ":" => continue,
            ("python", _, Tag::Punctuation) if text == "." => Tag::Operator,
            ("python", "type", _) if prev(1) != Some("class") => Tag::Variable,
            ("markdown", "text.literal", _) => {
                marks(range.start, text, '`', Tag::Keyword, None, &mut out);
                continue;
            }
            ("markdown", "emphasis" | "emphasis.strong", _) => {
                let mark = text.chars().next().unwrap_or('*');
                marks(range.start, text, mark, Tag::Keyword, Some(tag), &mut out);
                continue;
            }
            ("markdown", "punctuation.markup", _) if text == "|" => continue,
            ("markdown", "punctuation.list_marker", _) => Tag::Keyword,
            ("yaml", "string", _) if !text.starts_with(['"', '\'']) => continue,
            ("javascript" | "typescript", _, Tag::Variable) if js.defines() => Tag::Definition,
            (_, _, tag) => tag,
        };
        if matches!(lang, "javascript" | "typescript") {
            js.step(name, text);
        }
        if lang == "rust" {
            rust.step(text);
        }
        out.push((range.start..range.start + text.len(), tag));
    }
    out
}

fn next_is(source: &str, at: usize, what: &str) -> bool {
    source[at..].trim_start().starts_with(what)
}

/// A CSS number with its unit: the digits are a number, the unit (`px`, `%`) a keyword.
fn split_unit(start: usize, text: &str, out: &mut Vec<(Range<usize>, Tag)>) {
    let digits = text.find(|c: char| !(c.is_ascii_digit() || matches!(c, '.' | '-' | '+'))).unwrap_or(text.len());
    if digits > 0 {
        out.push((start..start + digits, Tag::Number));
    }
    if digits < text.len() {
        out.push((start + digits..start + text.len(), Tag::Keyword));
    }
}

/// Markdown's marks (`` ` ``, `*`, `_`) at both ends of `text` take `marks`; what they wrap
/// takes `inner`, or stays plain.
fn marks(start: usize, text: &str, mark: char, marks: Tag, inner: Option<Tag>, out: &mut Vec<(Range<usize>, Tag)>) {
    let open = text.len() - text.trim_start_matches(mark).len();
    let close = text.len() - text.trim_end_matches(mark).len();
    if open + close >= text.len() {
        out.push((start..start + text.len(), marks));
        return;
    }
    out.push((start..start + open, marks));
    if let Some(inner) = inner {
        out.push((start + open..start + text.len() - close, inner));
    }
    out.push((start + text.len() - close..start + text.len(), marks));
}

/// Where a JavaScript name is a definition (`const x`, `import { x }`, `function f(x)`): Lezer
/// tags it `definition(variableName)`.
#[derive(Default)]
struct JsScope {
    /// The last token declared (`const`, `let`, `var`).
    declaring: bool,
    /// Inside `import ... from`.
    importing: bool,
    /// After `function` and inside its parameter list, with the bracket depth.
    params: Option<u32>,
    after_function: bool,
}

impl JsScope {
    fn defines(&self) -> bool {
        self.declaring || self.importing || self.params.is_some()
    }

    fn step(&mut self, name: &str, text: &str) {
        self.declaring = name == "keyword.declaration" && matches!(text, "const" | "let" | "var");
        match text {
            "import" => self.importing = true,
            "from" => self.importing = false,
            "function" => self.after_function = true,
            _ => {}
        }
        if let Some(depth) = self.params.as_mut() {
            for c in text.chars() {
                match c {
                    '(' => *depth += 1,
                    ')' => *depth = depth.saturating_sub(1),
                    _ => {}
                }
            }
            if *depth == 0 {
                self.params = None;
            }
        } else if self.after_function && text.starts_with('(') {
            self.after_function = false;
            let depth = text.matches('(').count() as u32 - text.matches(')').count().min(1) as u32;
            self.params = (depth > 0).then_some(depth);
        }
    }
}

/// Rust's attributes (`#[derive(..)]`, Lezer's `meta`) and its bindings (`let x`, `mut x`,
/// a parameter before its `:`), which Lezer tags `definition(variableName)`.
#[derive(Default)]
struct RustScope {
    /// Bracket depth inside `#[...]`, when inside one.
    attribute: Option<u32>,
    /// The last token was `let` or `mut`, or a `(` or `,` of a `fn`'s parameters.
    binding: bool,
    /// Inside a `fn`'s parameter list, with its bracket depth.
    params: Option<u32>,
    after_fn: bool,
    /// The last token was `#`: a `[` next opens an attribute.
    hash: bool,
}

impl RustScope {
    fn in_attribute(&self) -> bool {
        self.attribute.is_some()
    }

    fn binds(&self) -> bool {
        self.binding
    }

    fn step(&mut self, text: &str) {
        if let Some(depth) = self.attribute.as_mut() {
            for c in text.chars() {
                match c {
                    '[' => *depth += 1,
                    ']' => *depth = depth.saturating_sub(1),
                    _ => {}
                }
            }
            if *depth == 0 {
                self.attribute = None;
            }
            return;
        }
        if self.hash && text.starts_with('[') {
            self.hash = false;
            let depth = text.matches('[').count() as u32 - text.matches(']').count().min(text.matches('[').count()) as u32;
            self.attribute = (depth > 0).then_some(depth);
            return;
        }
        self.hash = text == "#" || text == "#!";
        self.binding = matches!(text, "let" | "mut" | "let mut");
        if text == "fn" {
            self.after_fn = true;
        }
        if let Some(depth) = self.params.as_mut() {
            self.binding = text.ends_with(['(', ',']) && *depth == 1;
            for c in text.chars() {
                match c {
                    '(' => *depth += 1,
                    ')' => *depth = depth.saturating_sub(1),
                    _ => {}
                }
            }
            if *depth == 0 {
                self.params = None;
            }
        } else if self.after_fn && text.starts_with('(') {
            self.after_fn = false;
            self.params = Some(1);
            self.binding = true;
        }
    }
}
