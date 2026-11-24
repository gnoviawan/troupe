//! File paths in agent prose open in the editor (`chat-markdown-file-links.ts` with
//! `findFilePathMatches` from `lib/file-path-links.ts`). A bare path in the text becomes a
//! Markdown link to `termul-file:<path>`, and a link whose target is a local path opens the
//! file rather than a browser. Inline code, fenced code, link targets and URLs keep their text.

/// The scheme the rewrite gives a bare path's link.
pub const FILE_SCHEME: &str = "termul-file:";

/// What a link in agent prose opens.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LinkTarget {
    /// A file, as written (relative paths resolve against the chat's folder), with any
    /// `:line` suffix the text carried.
    File(String),
    /// A web or mail address.
    Url(String),
    /// A `#fragment` inside the message.
    Fragment(String),
}

/// `localLinkPath`, reduced: a `termul-file:` link, a `file:` URL, or a target with no URI
/// scheme names a file; `#x` is a fragment; anything else is a URL.
pub fn classify(url: &str) -> LinkTarget {
    let url = url.trim();
    if let Some(path) = url.strip_prefix(FILE_SCHEME) {
        return LinkTarget::File(percent_decode(path));
    }
    if let Some(fragment) = url.strip_prefix('#') {
        return LinkTarget::Fragment(fragment.to_owned());
    }
    if let Some(rest) = url.strip_prefix("file://").or_else(|| url.strip_prefix("file:")) {
        let rest = rest.strip_prefix("localhost").unwrap_or(rest);
        return LinkTarget::File(percent_decode(rest));
    }
    let drive = url.len() > 2 && url.as_bytes()[0].is_ascii_alphabetic() && &url[1..2] == ":";
    if !drive && has_scheme(url) {
        return LinkTarget::Url(url.to_owned());
    }
    let (path, fragment) = url.split_once('#').unwrap_or((url, ""));
    let path = percent_decode(path.split('?').next().unwrap_or(path));
    let line = fragment
        .strip_prefix(['L', 'l'])
        .map(|f| f.chars().take_while(char::is_ascii_digit).collect::<String>())
        .filter(|n| !n.is_empty());
    LinkTarget::File(match line {
        Some(n) => format!("{path}:{n}"),
        None => path,
    })
}

fn has_scheme(url: &str) -> bool {
    let Some(colon) = url.find(':') else {
        return false;
    };
    let scheme = &url[..colon];
    let mut chars = scheme.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || "+.-".contains(c))
        // `App.tsx:12` is a path with a line, not a scheme.
        && !url[colon + 1..].chars().next().is_some_and(|c| c.is_ascii_digit())
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = |b: u8| (b as char).to_digit(16);
        match (bytes[i], bytes.get(i + 1).copied().and_then(hex), bytes.get(i + 2).copied().and_then(hex)) {
            (b'%', Some(h), Some(l)) => {
                out.push((h * 16 + l) as u8);
                i += 3;
            }
            (b, ..) => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8(out).unwrap_or_else(|_| s.to_owned())
}

/// The characters a path segment may not hold (`[^\s\\/:*?"<>|`]`, the separators aside).
fn path_char(c: char) -> bool {
    !c.is_whitespace() && !"*?\"<>|`".contains(c)
}

const WRAPPERS: [(char, char); 5] = [('`', '`'), ('"', '"'), ('\'', '\''), ('(', ')'), ('[', ']')];

/// `trimTrailingPathPunctuation`.
fn trim_trailing(mut s: &str) -> &str {
    s = s.trim_end_matches(['.', ',', ';', '!', '?']);
    for (open, close) in WRAPPERS {
        while s.ends_with(close) && !s.contains(open) {
            s = &s[..s.len() - close.len_utf8()];
        }
    }
    s
}

/// `hasPathEvidence`: a path prefix, a line suffix, a lettered extension, a dot directory, or
/// three segments with a letter.
fn has_path_evidence(text: &str) -> bool {
    let normalized = text.replace('\\', "/");
    let n = normalized.as_str();
    let prefixed = n.starts_with("./")
        || n.starts_with("../")
        || n.starts_with("~/")
        || n.starts_with('/')
        || (n.len() > 2 && n.as_bytes()[0].is_ascii_alphabetic() && &n[1..3] == ":/");
    if prefixed {
        return true;
    }
    let (bare, line) = split_line_suffix(n);
    if line {
        return true;
    }
    let segments: Vec<&str> = bare.split('/').filter(|s| !s.is_empty()).collect();
    let last = segments.last().copied().unwrap_or("");
    if let Some(dot) = last.rfind('.')
        && last[dot + 1..].chars().any(char::is_alphabetic)
    {
        return true;
    }
    let first = segments.first().copied().unwrap_or("");
    if first.starts_with('.') && first[1..].starts_with(|c: char| c.is_ascii_alphabetic()) {
        return true;
    }
    segments.len() >= 3 && segments.iter().any(|s| s.chars().any(|c| c.is_ascii_alphabetic()))
}

/// The text without a `:line[:col]` suffix, and whether it had one.
fn split_line_suffix(text: &str) -> (&str, bool) {
    let mut end = text.len();
    for _ in 0..2 {
        let digits = text[..end].bytes().rev().take_while(u8::is_ascii_digit).count();
        if digits == 0 || end - digits == 0 || text.as_bytes()[end - digits - 1] != b':' {
            break;
        }
        end -= digits + 1;
    }
    (&text[..end], end != text.len())
}

/// The file paths in one run of plain text, as byte ranges.
pub fn find_paths(text: &str) -> Vec<std::ops::Range<usize>> {
    if !text.contains(['/', '\\']) {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut i = 0;
    while i < text.len() {
        let rest = &text[i..];
        let Some(start) = rest.find(path_char) else { break };
        let start = i + start;
        let len = text[start..].find(|c: char| !path_char(c)).unwrap_or(text.len() - start);
        let run = &text[start..start + len];
        i = start + len;
        // A URL's host and path are not a file.
        if run.contains("://") {
            continue;
        }
        let (mut from, mut cand) = (start, cut_at_colon(run));
        while let Some(c) = cand.chars().next().filter(|&c| matches!(c, '(' | '[' | '\'' | '"')) {
            let (open, close) = WRAPPERS.into_iter().find(|w| w.0 == c).unwrap_or((c, c));
            if cand[open.len_utf8()..].contains(close) && !cand.ends_with(close) {
                break;
            }
            from += c.len_utf8();
            cand = &cand[c.len_utf8()..];
        }
        let cand = trim_trailing(cand);
        if cand.contains(['/', '\\']) && has_path_evidence(cand) {
            out.push(from..from + cand.len());
        }
    }
    out
}

/// A run cut at its first `:` unless that colon is a drive letter's or starts a line suffix.
fn cut_at_colon(run: &str) -> &str {
    let mut end = 0;
    let bytes = run.as_bytes();
    while end < bytes.len() {
        if bytes[end] == b':' {
            let drive = end == 1 && bytes[0].is_ascii_alphabetic() && matches!(bytes.get(2), Some(b'/' | b'\\'));
            if drive {
                end += 1;
                continue;
            }
            let digits = bytes[end + 1..].iter().take_while(|b| b.is_ascii_digit()).count();
            if digits == 0 {
                break;
            }
            end += 1 + digits;
            if bytes.get(end) == Some(&b':') {
                let col = bytes[end + 1..].iter().take_while(|b| b.is_ascii_digit()).count();
                if col > 0 {
                    end += 1 + col;
                }
            }
            break;
        }
        end += 1;
    }
    &run[..end]
}

/// Escapes what Markdown would read as markup inside a link's text.
fn escape_label(path: &str, out: &mut String) {
    for c in path.chars() {
        if "\\`*_[]<>!#".contains(c) {
            out.push('\\');
        }
        out.push(c);
    }
}

fn push_target(path: &str, out: &mut String) {
    out.push_str(FILE_SCHEME);
    for c in path.chars() {
        match c {
            '(' => out.push_str("%28"),
            ')' => out.push_str("%29"),
            '%' => out.push_str("%25"),
            '\\' => out.push_str("%5C"),
            ' ' => out.push_str("%20"),
            c => out.push(c),
        }
    }
}

/// The reply with every bare file path in its prose written as a `termul-file:` link. Code
/// (fenced or inline), links, images and `<…>` spans keep their text.
pub fn link_file_paths(src: &str) -> String {
    let mut out = String::with_capacity(src.len() + 64);
    let mut fence: Option<(u8, usize)> = None;
    for line in src.split_inclusive('\n') {
        let trimmed = line.trim_start_matches(' ');
        let marker = super::fences::fence_marker(trimmed);
        let indent = line.len() - trimmed.len();
        match (fence, marker) {
            (Some((ch, n)), Some((mch, m))) if indent <= 3 && ch == mch && m >= n && trimmed[m..].trim().is_empty() => {
                fence = None;
                out.push_str(line);
            }
            (Some(_), _) => out.push_str(line),
            (None, Some(open)) if indent <= 3 => {
                fence = Some(open);
                out.push_str(line);
            }
            (None, _) => link_line(line, &mut out),
        }
    }
    out
}

/// One line outside a fence: plain stretches get their paths linked.
fn link_line(line: &str, out: &mut String) {
    let bytes = line.as_bytes();
    let mut plain = 0;
    let mut i = 0;
    while i < bytes.len() {
        let skip = match bytes[i] {
            b'\\' => Some((i + 2).min(bytes.len())),
            b'`' => {
                let n = bytes[i..].iter().take_while(|&&b| b == b'`').count();
                let ticks = &line[i..i + n];
                Some(line[i + n..].find(ticks).map_or(i + n, |close| i + n + close + n))
            }
            b'<' => Some(line[i..].find('>').map_or(i + 1, |close| i + close + 1)),
            b'[' | b'!' => link_end(line, i),
            _ => None,
        };
        match skip {
            Some(end) => {
                link_plain(&line[plain..i], out);
                out.push_str(&line[i..end]);
                i = end;
                plain = end;
            }
            None => i += 1,
        }
    }
    link_plain(&line[plain..], out);
}

/// The end of a `[text](target)` or `![alt](src)` that starts at `i`, if one does.
fn link_end(line: &str, i: usize) -> Option<usize> {
    let open = if line[i..].starts_with("![") { i + 1 } else if line[i..].starts_with('[') { i } else { return None };
    let close = open + line[open..].find("](")?;
    let target = close + 2;
    Some(target + line[target..].find(')')? + 1)
}

fn link_plain(text: &str, out: &mut String) {
    let mut at = 0;
    for range in find_paths(text) {
        out.push_str(&text[at..range.start]);
        let path = &text[range.clone()];
        out.push('[');
        escape_label(path, out);
        out.push_str("](");
        push_target(path, out);
        out.push(')');
        at = range.end;
    }
    out.push_str(&text[at..]);
}
