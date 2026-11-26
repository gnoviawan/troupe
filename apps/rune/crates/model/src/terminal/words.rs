//! The shell words a completion replaces and how a candidate is spelled
//! (`lib/command-lifecycle/completion-output.ts`), and which text may reach the editor.
//!
//! Offsets are UTF-16 code units, as CodeMirror's and the host's are: the host checks a
//! completion's cursor in those units. The text is read the same way, one code unit at a time,
//! so a word splits exactly where the React editor splits it.

use serde::Serialize;

/// One completion candidate: `text` shown, `insert` (when it differs) replacing `from..to`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Row {
    pub text: String,
    pub from: usize,
    pub to: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub insert: Option<String>,
}

/// The quote open at the cursor: `$'` is ANSI-C quoting, `$"` bash locale quoting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Quote {
    Single,
    Double,
    AnsiC,
    Locale,
}

impl Quote {
    fn open(self) -> &'static str {
        match self {
            Quote::Single => "'",
            Quote::Double => "\"",
            Quote::AnsiC => "$'",
            Quote::Locale => "$\"",
        }
    }

    fn close(self) -> &'static str {
        match self {
            Quote::Single | Quote::AnsiC => "'",
            Quote::Double | Quote::Locale => "\"",
        }
    }
}

type Units = Vec<u16>;

fn units(s: &str) -> Units {
    s.encode_utf16().collect()
}

fn text(u: &[u16]) -> String {
    String::from_utf16_lossy(u)
}

const fn c(ch: char) -> u16 {
    ch as u16
}

/// JavaScript's `\s` on one code unit.
fn is_space(u: u16) -> bool {
    matches!(u, 0x09..=0x0d | 0x20 | 0xa0 | 0x1680 | 0x2000..=0x200a | 0x2028 | 0x2029 | 0x202f | 0x205f | 0x3000 | 0xfeff)
}

/// Unquoted characters the shell would split, expand or treat as syntax.
fn unquoted_special(u: u16) -> bool {
    is_space(u) || "'\"\\$`&;|<>()*?[!{}".encode_utf16().any(|s| s == u)
}

fn escape_where(u: &[u16], special: impl Fn(u16) -> bool) -> Units {
    let mut out = Vec::with_capacity(u.len());
    for &unit in u {
        if special(unit) {
            out.push(c('\\'));
        }
        out.push(unit);
    }
    out
}

fn starts_with(u: &[u16], prefix: &[u16]) -> bool {
    u.len() >= prefix.len() && u[..prefix.len()] == *prefix
}

/// The candidates in a completer's output for the word that ends at `cursor`
/// (`parseCompletionOutput`). Each line is a hex-encoded candidate; lines that do not decode,
/// hold control characters, or repeat are dropped.
#[must_use]
pub fn parse_output(draft: &str, cursor: usize, output: &str) -> Vec<Row> {
    let draft = units(draft);
    let point = cursor.min(draft.len());
    let at = word_at(&draft, point);
    let typed = &draft[at.from..point];
    let mut seen = std::collections::BTreeSet::new();
    let mut rows = Vec::new();
    for raw in output.split('\n') {
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        if line.is_empty() {
            continue;
        }
        let Some(candidate) = decode_hex_line(line) else { continue };
        if contains_disallowed_controls(&candidate) || !seen.insert(candidate.clone()) {
            continue;
        }
        let spelled = units(&candidate);
        let insert = continue_typed(&spelled, at.quote, typed).unwrap_or_else(|| quote_candidate(&spelled, at.quote, typed));
        let insert = (insert != spelled).then(|| text(&insert));
        rows.push(Row { text: candidate, from: at.from, to: point, insert });
    }
    rows
}

struct WordAt {
    from: usize,
    quote: Option<Quote>,
    /// The cursor sits right after an unquoted backslash, splitting an escape.
    escaped: bool,
}

/// The word ends at `point` and starts after unquoted, unescaped whitespace.
fn word_at(draft: &[u16], point: usize) -> WordAt {
    let (mut from, mut quote, mut escaped) = (0, None, false);
    let mut i = 0;
    while i < point {
        let ch = draft[i];
        let next = draft.get(i + 1).copied();
        match quote {
            Some(Quote::Single) => {
                if ch == c('\'') {
                    quote = None;
                }
            }
            Some(Quote::AnsiC) => {
                if ch == c('\'') {
                    quote = None;
                } else if ch == c('\\') {
                    i += 1;
                }
            }
            _ if ch == c('\\') => {
                escaped = i + 1 >= point;
                i += 1;
            }
            Some(Quote::Double | Quote::Locale) => {
                if ch == c('"') {
                    quote = None;
                }
            }
            None if ch == c('$') && (next == Some(c('\'')) || next == Some(c('"'))) => {
                quote = Some(if next == Some(c('\'')) { Quote::AnsiC } else { Quote::Locale });
                i += 1;
            }
            None if ch == c('\'') => quote = Some(Quote::Single),
            None if ch == c('"') => quote = Some(Quote::Double),
            None if is_space(ch) => from = i + 1,
            None => {}
        }
        i += 1;
    }
    WordAt { from, quote, escaped }
}

/// Whether the shell word at `cursor` goes on past it (`wordContinuesPastCursor`): a candidate
/// would replace only the part before the cursor, so no completion is asked for.
#[must_use]
pub fn word_continues_past_cursor(draft: &str, cursor: usize) -> bool {
    let draft = units(draft);
    let Some(&next) = draft.get(cursor) else { return false };
    if !is_space(next) {
        return true;
    }
    let at = word_at(&draft, cursor);
    at.quote.is_some() || at.escaped
}

/// `~/`, `~user/`, `~+/`, `~-/`: the tilde prefix of a candidate, which needs no quoting.
fn tilde_prefix(u: &[u16]) -> &[u16] {
    if u.first() != Some(&c('~')) {
        return &[];
    }
    let word = |x: u16| x < 0x80 && ((x as u8).is_ascii_alphanumeric() || b"_.+-".contains(&(x as u8)));
    let mut end = 1;
    while end < u.len() && word(u[end]) {
        end += 1;
    }
    if u.get(end) == Some(&c('/')) { &u[..=end] } else { &[] }
}

/// A raw candidate spelled for the quoting open at the cursor. A directory keeps its quote open
/// so the path can go on.
fn quote_candidate(candidate: &[u16], quote: Option<Quote>, typed: &[u16]) -> Units {
    let dir = candidate.last() == Some(&c('/'));
    if let Some(quote) = quote {
        // Bash expands a tilde prefix only while it is unquoted, so one typed before the quote
        // (`~/"Doc`) stays outside it.
        let prefix = tilde_prefix(candidate);
        let home = if !prefix.is_empty() && starts_with(typed, prefix) { prefix } else { &[] };
        let mut out = home.to_vec();
        out.extend(units(quote.open()));
        out.extend(quote_body(&candidate[home.len()..], quote));
        if !dir {
            out.extend(units(quote.close()));
        }
        return out;
    }
    // A leading `~` stays a tilde prefix only when the user typed one; a bare `~` candidate is a
    // file named `~`. zsh expands a leading `=` (EQUALS).
    let tilde = typed.first() == Some(&c('~'));
    let bare_tilde = candidate == [c('~')];
    let mut out = escape_where(candidate, unquoted_special);
    let lead = out.first().copied();
    let leading = |u: u16| u == c('#') || u == c('=') || (u == c('~') && (bare_tilde || !tilde));
    if lead.is_some_and(leading) {
        out.insert(0, c('\\'));
    }
    out
}

fn quote_body(body: &[u16], quote: Quote) -> Units {
    match quote {
        Quote::Single => {
            let mut out = Vec::new();
            for &u in body {
                if u == c('\'') {
                    out.extend(units("'\\''"));
                } else {
                    out.push(u);
                }
            }
            out
        }
        Quote::AnsiC => escape_where(body, |u| u == c('\\') || u == c('\'')),
        // History expansion sees `!` inside double quotes and a backslash before it stays in the
        // word, so the `!` steps out of the quotes to be escaped.
        Quote::Double | Quote::Locale => {
            let mut out = Vec::new();
            for u in escape_where(body, |u| u == c('"') || u == c('\\') || u == c('$') || u == c('`')) {
                if u == c('!') {
                    out.extend(units("\"\\!\""));
                } else {
                    out.push(u);
                }
            }
            out
        }
    }
}

/// Digits that would extend a short numeric escape ending an open `$'...'`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Digits {
    Octal,
    Hex,
}

impl Digits {
    fn test(self, u: Option<u16>) -> bool {
        let Some(u) = u.filter(|&u| u < 0x80) else { return false };
        match self {
            Digits::Octal => (b'0'..=b'7').contains(&(u as u8)),
            Digits::Hex => (u as u8).is_ascii_hexdigit(),
        }
    }
}

/// What a typed word means to the shell, and how its spelling may go on.
struct Typed {
    /// The word without its quoting, as the host scripts dequote it.
    value: Units,
    /// The spelling ends in `$`, which would read the tail as a parameter name.
    dollar: bool,
    /// The spelling ends in an unquoted backslash that escapes the tail's first character.
    backslash: bool,
    digits: Option<Digits>,
}

/// The typed word as a literal, or None when it is not one or no tail could follow it verbatim
/// (bash and zsh history-expand a `!`, zsh drops a `!"` pair).
fn read_typed(typed: &[u16]) -> Option<Typed> {
    let (mut value, mut quote, mut dollar, mut digits, mut braces) = (Vec::new(), None, false, None, (false, false));
    let syntax = |u: u16| "`&;|<>()*?[!".encode_utf16().any(|s| s == u);
    let mut i = 0;
    while i < typed.len() {
        let ch = typed[i];
        let next = typed.get(i + 1).copied();
        dollar = false;
        match quote {
            Some(Quote::Single) => {
                if ch == c('\'') {
                    quote = None;
                } else {
                    value.push(ch);
                }
            }
            Some(_) => {
                if ch == c('"') {
                    quote = None;
                } else if ch == c('`') || ch == c('!') || (ch == c('\\') && next.is_none()) {
                    return None;
                } else if ch == c('\\') && next.is_some_and(|n| [c('$'), c('`'), c('"'), c('\\')].contains(&n)) {
                    i += 1;
                    value.push(typed[i]);
                } else if ch == c('$') && next.is_some() {
                    return None;
                } else {
                    dollar = ch == c('$');
                    value.push(ch);
                }
            }
            None if ch == c('\\') => {
                let Some(n) = next else { return Some(Typed { value, dollar, backslash: true, digits }) };
                value.push(n);
                i += 1;
            }
            None if ch == c('$') && next == Some(c('\'')) => {
                let ansi = read_ansi_c(typed, i + 2)?;
                value.extend(ansi.value);
                digits = ansi.digits;
                i = ansi.end;
            }
            None if ch == c('$') && next == Some(c('"')) => {
                quote = Some(Quote::Locale);
                i += 1;
            }
            None if ch == c('\'') => quote = Some(Quote::Single),
            None if ch == c('"') => quote = Some(Quote::Double),
            None if syntax(ch) || (i == 0 && (ch == c('#') || ch == c('='))) => return None,
            None if ch == c('$') && next.is_some() => return None,
            None => {
                dollar = ch == c('$');
                braces.0 |= ch == c('{');
                braces.1 |= ch == c('}');
                value.push(ch);
            }
        }
        i += 1;
    }
    // An unescaped `{` and `}` in one word may already be a brace expansion.
    if braces.0 && braces.1 {
        return None;
    }
    Some(Typed { value, dollar, backslash: false, digits })
}

struct AnsiC {
    value: Units,
    /// The closing quote's index, or the last character's when it is still open.
    end: usize,
    digits: Option<Digits>,
}

/// The code point at `i` as JavaScript's `codePointAt` reads it, and its length in units.
fn code_point(u: &[u16], i: usize) -> (u32, usize) {
    let hi = u[i];
    if (0xd800..0xdc00).contains(&hi)
        && let Some(&lo) = u.get(i + 1)
        && (0xdc00..0xe000).contains(&lo)
    {
        return (0x10000 + ((u32::from(hi) - 0xd800) << 10) + (u32::from(lo) - 0xdc00), 2);
    }
    (u32::from(hi), 1)
}

fn utf8(code: u32, bytes: &mut Vec<u8>) {
    let ch = char::from_u32(code).unwrap_or('\u{fffd}');
    bytes.extend_from_slice(ch.encode_utf8(&mut [0; 4]).as_bytes());
}

/// Decodes `$'...'` from `start` (just after the `$'`) as bash does. None for escapes that are
/// not literal text (`\n`, `\cX`, an unknown `\q`) and for bytes that are not UTF-8.
fn read_ansi_c(typed: &[u16], start: usize) -> Option<AnsiC> {
    let mut bytes = Vec::new();
    let mut digits = None;
    let mut i = start;
    while i < typed.len() {
        digits = None;
        let (code, len) = code_point(typed, i);
        if code == u32::from('\'') {
            break;
        }
        if code != u32::from('\\') {
            utf8(code, &mut bytes);
            i += len;
            continue;
        }
        i += 1;
        let kind = *typed.get(i)?;
        if [c('\''), c('"'), c('\\'), c('?')].contains(&kind) {
            bytes.push(kind as u8);
            i += 1;
            continue;
        }
        let (digit, max, base, unicode) = match kind {
            k if (c('0')..=c('7')).contains(&k) => (Digits::Octal, 3, 8, false),
            k if k == c('x') => (Digits::Hex, 2, 16, false),
            k if k == c('u') => (Digits::Hex, 4, 16, true),
            k if k == c('U') => (Digits::Hex, 8, 16, true),
            _ => return None,
        };
        let mut number = if digit == Digits::Octal { vec![kind] } else { Vec::new() };
        while number.len() < max && digit.test(typed.get(i + 1).copied()) {
            i += 1;
            number.push(typed[i]);
        }
        if number.is_empty() {
            return None;
        }
        let code = u32::from_str_radix(&text(&number), base).ok()?;
        if !unicode {
            bytes.push((code & 0xff) as u8);
        } else if code > 0x10ffff || (0xd800..=0xdfff).contains(&code) {
            return None;
        } else {
            utf8(code, &mut bytes);
        }
        if number.len() < max {
            digits = Some(digit);
        }
        i += 1;
    }
    let value = String::from_utf8(bytes).ok()?;
    Some(AnsiC { value: units(&value), end: i.min(typed.len().saturating_sub(1)), digits })
}

/// The typed spelling kept verbatim and only the untyped tail spelled, so the insert starts with
/// the draft (ghost text, Right Arrow). None when the typed word must be spelled in full.
fn continue_typed(candidate: &[u16], quote: Option<Quote>, typed: &[u16]) -> Option<Units> {
    if typed.is_empty() || (candidate == [c('~')] && typed.first() == Some(&c('~'))) {
        return None;
    }
    let word = read_typed(typed)?;
    if !starts_with(candidate, &word.value) {
        return None;
    }
    let tail = &candidate[word.value.len()..];
    let mut out = typed.to_vec();
    if let Some(quote) = quote {
        // `"a$""b"` keeps the `$` literal in bash and zsh; `$'\41'$'7'` keeps the `7` out of the
        // octal escape.
        if word.dollar && !tail.is_empty() {
            out.extend(units("\"\""));
        } else if word.digits.is_some_and(|d| d.test(tail.first().copied())) {
            out.push(c('\''));
            out.extend(units(quote.open()));
        }
        out.extend(quote_body(tail, quote));
        if candidate.last() != Some(&c('/')) {
            out.extend(units(quote.close()));
        }
        return Some(out);
    }
    if word.backslash {
        let (&first, rest) = tail.split_first()?;
        out.push(first);
        out.extend(escape_where(rest, unquoted_special));
        return Some(out);
    }
    let spelled = escape_where(tail, unquoted_special);
    // `$\b` is a literal `$` and `b`; a bare `$b` would expand.
    if word.dollar && !tail.is_empty() && spelled.first() != Some(&c('\\')) {
        out.push(c('\\'));
    }
    out.extend(spelled);
    Some(out)
}

/// Unicode's format characters (`\p{Cf}`), as of Unicode 16.
const FORMAT: &[(u32, u32)] = &[
    (0xad, 0xad),
    (0x600, 0x605),
    (0x61c, 0x61c),
    (0x6dd, 0x6dd),
    (0x70f, 0x70f),
    (0x890, 0x891),
    (0x8e2, 0x8e2),
    (0x180e, 0x180e),
    (0x200b, 0x200f),
    (0x202a, 0x202e),
    (0x2060, 0x2064),
    (0x2066, 0x206f),
    (0xfeff, 0xfeff),
    (0xfff9, 0xfffb),
    (0x110bd, 0x110bd),
    (0x110cd, 0x110cd),
    (0x13430, 0x1343f),
    (0x1bca0, 0x1bca3),
    (0x1d173, 0x1d17a),
    (0xe0001, 0xe0001),
    (0xe0020, 0xe007f),
];

fn disallowed(ch: char) -> bool {
    let code = u32::from(ch);
    code <= 0x1f || code == 0x7f || (0x80..=0x9f).contains(&code) || FORMAT.iter().any(|&(lo, hi)| (lo..=hi).contains(&code))
}

/// C0 and C1 controls, DEL and Unicode format characters: text that must not reach the PTY
/// as a command (`containsDisallowedControls`).
#[must_use]
pub fn contains_disallowed_controls(text: &str) -> bool {
    text.chars().any(disallowed)
}

/// Newline, tab, ZWNJ and ZWJ are editor text even though the PTY check refuses them.
fn editor_allowed(ch: char) -> bool {
    matches!(ch, '\n' | '\t' | '\u{200c}' | '\u{200d}')
}

/// The editor's check (`containsEditorDisallowedControls`).
#[must_use]
pub fn contains_editor_disallowed_controls(text: &str) -> bool {
    text.chars().any(|ch| !editor_allowed(ch) && disallowed(ch))
}

/// Pasted text as the editor takes it (`sanitizeEditorPaste`): bracketed-paste markers dropped,
/// line breaks made `\n`, and controls the editor refuses removed.
#[must_use]
pub fn sanitize_paste(pasted: &str) -> String {
    let unmarked = pasted.replace("\u{1b}[200~", "").replace("\u{1b}[201~", "");
    let unified = unmarked.replace("\r\n", "\n").replace('\r', "\n");
    unified.chars().filter(|&ch| editor_allowed(ch) || !disallowed(ch)).collect()
}

/// One hex-encoded candidate line, decoded as `TextDecoder('utf-8')` does (a leading byte order
/// mark dropped, bad bytes replaced).
fn decode_hex_line(line: &str) -> Option<String> {
    if !line.len().is_multiple_of(2) || !line.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let bytes: Vec<u8> = (0..line.len()).step_by(2).map(|i| u8::from_str_radix(&line[i..i + 2], 16).unwrap_or(0)).collect();
    let body = bytes.strip_prefix(&[0xef, 0xbb, 0xbf][..]).unwrap_or(&bytes);
    Some(String::from_utf8_lossy(body).into_owned())
}
