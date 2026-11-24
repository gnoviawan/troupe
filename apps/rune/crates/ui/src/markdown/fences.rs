//! The text clean-up the web client runs on an agent reply before it renders: drop persisted
//! `termul-plan` fences (`lib/plan-session.ts`), drop empty fences and put a `termul-plan`
//! opener on its own line (`lib/strip-empty-fences.ts`).

/// The info string of the fence an agent's plan snapshot rides in.
pub const PLAN_LANGUAGE: &str = "termul-plan";

/// `stripTermulPlanFences`: every complete `termul-plan` fence goes, with the line break that
/// ends it. The fence stays on the message so a reopened chat can restore the plan panel.
pub fn strip_plan_fences(text: &str) -> String {
    const OPEN: &str = "```termul-plan";
    if !text.contains(PLAN_LANGUAGE) {
        return text.to_owned();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    loop {
        let Some(at) = find_plan_opener(rest) else {
            out.push_str(rest);
            return out;
        };
        let after_open = &rest[at + OPEN.len()..];
        let body_start = if after_open.starts_with("\r\n") {
            2
        } else if after_open.starts_with('\n') {
            1
        } else {
            out.push_str(&rest[..at + OPEN.len()]);
            rest = after_open;
            continue;
        };
        let body = &after_open[body_start..];
        let Some(close) = find_close(body) else {
            out.push_str(rest);
            return out;
        };
        // The line break before the opener stays: the TypeScript replacement puts it back.
        out.push_str(&rest[..at]);
        let mut end = close.end;
        if body[end..].starts_with('\n') {
            end += 1;
        }
        rest = &body[end..];
    }
}

/// The byte offset of a `termul-plan` opener at the start of the text or right after a `\n`.
fn find_plan_opener(text: &str) -> Option<usize> {
    const OPEN: &str = "```termul-plan";
    let mut from = 0;
    while let Some(i) = text[from..].find(OPEN) {
        let at = from + i;
        if at == 0 || text.as_bytes()[at - 1] == b'\n' {
            return Some(at);
        }
        from = at + 1;
    }
    None
}

/// The lazy `[\s\S]*?\r?\n```` close of a plan fence's body: the first "\n```" (an optional
/// `\r` before the line break).
fn find_close(body: &str) -> Option<std::ops::Range<usize>> {
    let i = body.find("\n```")?;
    let start = if i > 0 && body.as_bytes()[i - 1] == b'\r' { i - 1 } else { i };
    Some(start..i + 4)
}

/// The fence run a line (with its indent already trimmed) starts with: its character and length.
pub(crate) fn fence_marker(trimmed: &str) -> Option<(u8, usize)> {
    b"`~".iter()
        .map(|&ch| (ch, trimmed.bytes().take_while(|&b| b == ch).count()))
        .find(|&(_, n)| n >= 3)
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Fence {
    ch: u8,
    len: usize,
}

/// A CommonMark fence line: at most three spaces, then three or more backticks or tildes. A
/// backtick fence's info string has no backtick. Returns the fence and the text after the run.
fn parse_fence(line: &str) -> Option<(Fence, &str)> {
    let indent = line.bytes().take_while(|&b| b == b' ').count();
    if indent > 3 {
        return None;
    }
    let rest = &line[indent..];
    let ch = *rest.as_bytes().first()?;
    if ch != b'`' && ch != b'~' {
        return None;
    }
    let len = rest.bytes().take_while(|&b| b == ch).count();
    if len < 3 {
        return None;
    }
    let after = &rest[len..];
    if ch == b'`' && after.contains('`') {
        return None;
    }
    Some((Fence { ch, len }, after))
}

fn closes(line: &str, open: Fence) -> bool {
    parse_fence(line).is_some_and(|(f, after)| f.ch == open.ch && f.len >= open.len && after.trim().is_empty())
}

/// `stripEmptyFences`: drop each closed fence with a blank body and, once the reply has
/// settled, a trailing open fence with nothing after it. While streaming, an open fence stays:
/// its empty frame is the cue that code is coming.
pub fn strip_empty_fences(text: &str, streaming: bool) -> String {
    let out = strip_closed_empty(text);
    if streaming { out } else { strip_trailing_empty(&out) }
}

fn strip_closed_empty(text: &str) -> String {
    let lines: Vec<&str> = text.split('\n').collect();
    let mut out: Vec<&str> = Vec::with_capacity(lines.len());
    let mut i = 0;
    while i < lines.len() {
        let Some((open, _)) = parse_fence(lines[i]) else {
            out.push(lines[i]);
            i += 1;
            continue;
        };
        let Some(j) = (i + 1..lines.len()).find(|&j| closes(lines[j], open)) else {
            out.extend_from_slice(&lines[i..]);
            break;
        };
        if !lines[i + 1..j].iter().all(|l| l.trim().is_empty()) {
            out.extend_from_slice(&lines[i..=j]);
        }
        i = j + 1;
    }
    out.join("\n")
}

fn strip_trailing_empty(text: &str) -> String {
    let lines: Vec<&str> = text.split('\n').collect();
    let mut open: Option<(usize, Fence)> = None;
    for (i, line) in lines.iter().enumerate() {
        match open {
            Some((_, fence)) => {
                if closes(line, fence) {
                    open = None;
                }
            }
            None => open = parse_fence(line).map(|(f, _)| (i, f)),
        }
    }
    let Some((at, _)) = open else {
        return text.to_owned();
    };
    if !lines[at + 1..].join("\n").trim().is_empty() {
        return text.to_owned();
    }
    lines[..at].join("\n")
}

/// `normalizePlanFenceBoundary`: a `termul-plan` opener glued to the prose before it (text
/// blocks joined with no line break) moves to its own line, so the fence parses as one.
pub fn normalize_plan_fence_boundary(text: &str) -> String {
    const OPEN: &str = "```termul-plan";
    let mut out = String::with_capacity(text.len() + 8);
    let mut from = 0;
    while let Some(i) = text[from..].find(OPEN) {
        let at = from + i;
        let end = at + OPEN.len();
        let tail = &text[end..];
        let exact = tail.is_empty() || tail.starts_with('\n') || tail.starts_with("\r\n");
        out.push_str(&text[from..at]);
        if exact {
            let line_start = text[..at].rfind('\n').map_or(0, |n| n + 1);
            let prefix = &text[line_start..at];
            let indented = prefix.len() <= 3 && prefix.bytes().all(|b| b == b' ' || b == b'\t');
            let in_run = (1..=3).contains(&prefix.len()) && prefix.bytes().all(|b| b == b'`');
            if !indented && !in_run {
                out.push('\n');
            }
        }
        out.push_str(OPEN);
        from = end;
    }
    out.push_str(&text[from..]);
    out
}

/// What an agent reply's prose renders: `blocksToText`'s plan strip, then the empty fences
/// and the plan opener boundary, as `ChatMessage` prepares it.
pub fn agent_prose(text: &str, streaming: bool) -> String {
    normalize_plan_fence_boundary(&strip_empty_fences(&strip_plan_fences(text), streaming))
}
