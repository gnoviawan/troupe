//! Code colours: rune-highlights' lexers, with the token kinds re-cut where the web client's
//! TextMate grammars name the same text differently. `docs/rune/decisions.md` ("Code
//! highlighting") records why this engine and how far it reaches;
//! `tools/parity/highlight/parity.ts` measures it.

use rune_kit::syntax::{self, Highlight, Highlighter, Paint, Syntax};

/// Bumped whenever [`recut`] changes, so cached colours from an older cut are not reused.
const CUT: &str = "troupe-recut-1";

/// Words TextMate scopes `constant.language`: the theme draws them as keywords.
const LANGUAGE_CONSTANTS: [&str; 9] = ["true", "false", "null", "undefined", "None", "True", "False", "nil", "NaN"];

/// Types TextMate scopes `support.type.primitive` (TypeScript) or `storage.type` (Go, Rust).
const PRIMITIVES: [&str; 25] = [
    "string", "number", "boolean", "void", "any", "unknown", "never", "object", "bigint", "symbol", "int", "int64",
    "float64", "byte", "rune", "bool", "error", "u8", "u32", "u64", "usize", "i32", "i64", "f64", "str",
];

/// The chat's highlighter: the built-in lexers, re-cut.
pub fn highlighter() -> Highlighter {
    Highlighter::new(|lang, code| recut(code, syntax::builtin(lang, code))).key(CUT)
}

/// Where rune-highlights' kind for a token differs from the scope the web client's grammar gives
/// it: a `Number` that is a word is a language constant (keyword colour) or a namespace
/// (plain); a primitive `Type` is a keyword.
pub fn recut(code: &str, ranges: Vec<Highlight>) -> Vec<Highlight> {
    ranges
        .into_iter()
        .filter_map(|(range, paint)| {
            let text = code.get(range.clone()).unwrap_or("");
            let word = text.starts_with(|c: char| c.is_alphabetic() || c == '_');
            let paint = match paint {
                Paint::Kind(Syntax::Number) if word && LANGUAGE_CONSTANTS.contains(&text) => Paint::Kind(Syntax::Keyword),
                Paint::Kind(Syntax::Number) if word => return None,
                Paint::Kind(Syntax::Type) if PRIMITIVES.contains(&text) => Paint::Kind(Syntax::Keyword),
                other => other,
            };
            Some((range, paint))
        })
        .collect()
}
