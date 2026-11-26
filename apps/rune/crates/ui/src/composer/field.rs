//! The composer's field: a rich `TextEdit` whose pills carry a [`Pill`] as their id, read back
//! as a [`Doc`], and the `/` and `@` triggers at its caret (`findSlashTrigger`,
//! `activeMentionToken`), with pill ends counting as boundaries as the token ends do.

use std::ops::Range;

use rune::TextEdit;

use super::doc::{Doc, Part, Pill, is_js_space};

/// The id a pill carries in the field.
pub fn pill_id(pill: &Pill) -> String {
    serde_json::to_string(pill).unwrap_or_default()
}

pub fn pill_of(id: &str) -> Option<Pill> {
    serde_json::from_str(id).ok()
}

/// The field's document: its text, with each pill's label replaced by the pill.
pub fn doc_of(edit: &TextEdit) -> Doc {
    let text = &edit.text;
    let mut parts = Vec::new();
    let mut at = 0;
    let mut pills: Vec<(Range<usize>, Pill)> = edit
        .pills()
        .filter_map(|(range, id)| pill_of(id).map(|pill| (range, pill)))
        .collect();
    pills.sort_by_key(|(range, _)| range.start);
    for (range, pill) in pills {
        if range.start < at {
            continue;
        }
        if range.start > at {
            parts.push(Part::Text(text[at..range.start].to_owned()));
        }
        parts.push(Part::Pill(pill));
        at = range.end;
    }
    if at < text.len() {
        parts.push(Part::Text(text[at..].to_owned()));
    }
    Doc { parts }
}

/// Puts `pill` in place of `range` with a space after it (`insertSkillToken` and its siblings
/// append one), as one undo step.
pub fn put_pill(edit: &mut TextEdit, range: Range<usize>, pill: &Pill) {
    let label = pill.label();
    edit.undo_group(|e| {
        e.replace_with_pill(range, pill_id(pill), &label);
        e.insert(" ");
    });
}

/// A field holding `doc`, for a draft or a quoted message.
pub fn edit_of(doc: &Doc) -> TextEdit {
    let mut edit = TextEdit::new("").rich();
    for part in &doc.parts {
        match part {
            Part::Text(text) => edit.insert(text),
            Part::Pill(pill) => {
                let at = edit.range().1;
                edit.replace_with_pill(at..at, pill_id(pill), &pill.label());
            }
        }
    }
    edit
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TriggerKind {
    Slash,
    Mention,
}

/// A trigger at the caret: its byte range in the field (the `/` or `@` and the query after
/// it) and the query.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Trigger {
    pub kind: TriggerKind,
    pub range: Range<usize>,
    pub query: String,
}

/// `isComposerTriggerBoundary`: the start, after whitespace, or right after a pill.
fn is_boundary(edit: &TextEdit, i: usize) -> bool {
    i == 0
        || edit.text[..i].chars().next_back().is_some_and(is_js_space)
        || edit.pills().any(|(range, _)| range.end == i)
}

fn in_pill(edit: &TextEdit, i: usize) -> bool {
    edit.pills().any(|(range, _)| range.start < i && i < range.end)
}

/// The `/` or `@` trigger at the caret, if any. None while text is selected.
pub fn trigger(edit: &TextEdit) -> Option<Trigger> {
    let (a, caret) = edit.range();
    if a != caret || edit.is_composing() {
        return None;
    }
    slash(edit, caret).or_else(|| mention(edit, caret))
}

fn slash(edit: &TextEdit, caret: usize) -> Option<Trigger> {
    let text = &edit.text;
    let previous = text[..caret].chars().next_back()?;
    if is_js_space(previous) || edit.pills().any(|(range, _)| range.end == caret) {
        return None;
    }
    let mut start = caret - previous.len_utf8();
    while start > 0 && !is_boundary(edit, start) {
        start -= text[..start].chars().next_back()?.len_utf8();
    }
    if !text[start..].starts_with('/') || in_pill(edit, start) {
        return None;
    }
    Some(Trigger {
        kind: TriggerKind::Slash,
        range: start..caret,
        query: text[start + 1..caret].to_owned(),
    })
}

fn mention(edit: &TextEdit, caret: usize) -> Option<Trigger> {
    let text = &edit.text;
    for (at, _) in text[..caret].char_indices().rev().filter(|(_, c)| *c == '@') {
        if !is_boundary(edit, at) || in_pill(edit, at) {
            continue;
        }
        let rest = &text[at + 1..];
        let len = rest.find(|c: char| is_js_space(c) || c == '@').unwrap_or(rest.len());
        let pill = edit.pills().map(|(range, _)| range.start).filter(|s| *s > at).min();
        let end = pill.map_or(at + 1 + len, |p| p.min(at + 1 + len));
        if caret > at && caret <= end {
            return Some(Trigger {
                kind: TriggerKind::Mention,
                range: at..end,
                query: text[at + 1..end].to_owned(),
            });
        }
    }
    None
}
