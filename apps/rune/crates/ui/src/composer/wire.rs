//! The text the agent receives for a display string (`lib/composer/wire-text.ts`,
//! `composerWireText`): selection contexts become leading quote blocks, then commands, files,
//! chats and tools become plain text where they stand, and skills are framed by path. Each
//! step reads the display string the way its TypeScript twin does, so malformed tokens fall
//! back the same way.

use std::collections::BTreeMap;

use super::doc::*;
use super::uri::decode_uri_component;

/// Why a mentioned chat's transcript is not in the send (`TranscriptGap`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Gap {
    Unavailable,
    Omitted,
    Partial,
}

impl Gap {
    fn note(self) -> &'static str {
        match self {
            Gap::Unavailable => ", transcript unavailable",
            Gap::Omitted => ", transcript omitted: context limit",
            Gap::Partial => ", transcript partial: earlier history unavailable",
        }
    }
}

/// `TERMUL_TOOLS`: a tool pill's label and what the agent reads in its place.
pub struct Tool {
    pub id: &'static str,
    pub label: &'static str,
    pub description: &'static str,
    pub wire: &'static str,
}

pub const TOOLS: [Tool; 4] = [
    Tool {
        id: "browser",
        label: "Browser",
        description: "Open, click and read pages",
        wire: "(Use the browser tool from the troupe MCP server)",
    },
    Tool {
        id: "canvas",
        label: "Canvas",
        description: "Design on the project canvas",
        wire: "(Use the canvas tools from the troupe MCP server)",
    },
    Tool {
        id: "automations",
        label: "Automations",
        description: "Schedule background work",
        wire: "(Use the automation tools from the troupe MCP server)",
    },
    Tool {
        id: "orchestrate",
        label: "Orchestrate",
        description: "Hand parts of the task to other agents",
        wire: "(Use the subagent tools from the troupe MCP server)",
    },
];

pub fn tool(id: &str) -> Option<&'static Tool> {
    TOOLS.iter().find(|t| t.id == id)
}

/// The byte index of `c` at or after `from`.
fn find(s: &str, from: usize, c: char) -> Option<usize> {
    s.get(from..)?.find(c).map(|i| i + from)
}

fn at(s: &str, i: usize) -> Option<char> {
    s.get(i..)?.chars().next()
}

/// `composerWireText`. `chats` is the resolved transcripts of mentioned chats; without it
/// every mentioned chat is labelled unavailable.
pub fn wire_text(value: &str, skills: &[(String, String)], chats: Option<&BTreeMap<String, Gap>>) -> String {
    let with_commands = replace_commands(&serialize_contexts(value));
    let plain = replace_conversations(&replace_files(&with_commands), |title, id| {
        let gap = match chats {
            None => Some(Gap::Unavailable),
            Some(gaps) => gaps.get(id).copied(),
        };
        format!(
            "(conversation: {title}{} [termul-chat:{id}])",
            gap.map_or("", Gap::note)
        )
    });
    format_with_skills(skills, &replace_tools(&plain))
}

pub enum SkillSeg {
    Text(String),
    Skill(String),
}

/// `parseSkillSegments`.
pub fn skill_segments(value: &str) -> Vec<SkillSeg> {
    let mut out = Vec::new();
    let mut text = String::new();
    let mut i = 0;
    while let Some(c) = at(value, i) {
        if c != SKILL_START {
            text.push(c);
            i += c.len_utf8();
            continue;
        }
        let Some(end) = find(value, i + c.len_utf8(), SKILL_END) else {
            text.push_str(&value[i..]);
            break;
        };
        let after = end + SKILL_END.len_utf8();
        let name = &value[i + c.len_utf8()..end];
        if name.is_empty() {
            text.push_str(&value[i..after]);
            i = after;
            continue;
        }
        let mut cursor = after;
        if at(value, cursor) == Some(PAD_START)
            && let Some(pad_end) = find(value, cursor + PAD_START.len_utf8(), PAD_END)
        {
            cursor = pad_end + PAD_END.len_utf8();
        }
        if !text.is_empty() {
            out.push(SkillSeg::Text(std::mem::take(&mut text)));
        }
        out.push(SkillSeg::Skill(name.to_owned()));
        i = cursor;
    }
    if !text.is_empty() {
        out.push(SkillSeg::Text(text));
    }
    out
}

/// `extractSkillNames`: unique, in first-appearance order.
pub fn skill_names(value: &str) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for seg in skill_segments(value) {
        if let SkillSeg::Skill(name) = seg
            && !names.contains(&name)
        {
            names.push(name);
        }
    }
    names
}

fn replace_skills(value: &str) -> String {
    skill_segments(value)
        .into_iter()
        .map(|s| match s {
            SkillSeg::Skill(name) => format!("({name})"),
            SkillSeg::Text(text) => text,
        })
        .collect()
}

/// What a sent message's Copy puts on the clipboard (`ChatMessage`):
/// `sanitizeDisplayText(serializeSelectionContexts(text))`, every pill as plain text.
pub fn copy_text(value: &str) -> String {
    let plain = replace_files(&replace_skills(&replace_commands(&serialize_contexts(value))));
    replace_tools(&replace_conversations(&plain, |title, _| format!("(conversation: {title})")))
}

/// `formatPromptWithSkills`.
pub fn format_with_skills(skills: &[(String, String)], user_text: &str) -> String {
    let inline = replace_skills(user_text);
    let inline = js_trim(&inline);
    let mut seen: Vec<&str> = Vec::new();
    let mut lines = Vec::new();
    for (name, path) in skills {
        let (name, path) = (js_trim(name), js_trim(path));
        if name.is_empty() || path.is_empty() || seen.contains(&name) {
            continue;
        }
        seen.push(name);
        lines.push(format!("{name}: {path}"));
    }
    if lines.is_empty() {
        return inline.to_owned();
    }
    let section = format!("# Agent Skills\n\n{}", lines.join("\n"));
    if inline.is_empty() {
        section
    } else {
        format!("{section}\n\n---\n\n{inline}")
    }
}

/// `extractCommandNames`.
pub fn command_names(value: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut i = 0;
    while let Some(c) = at(value, i) {
        if c != CMD_START {
            i += c.len_utf8();
            continue;
        }
        let Some(end) = find(value, i + c.len_utf8(), CMD_END) else {
            break;
        };
        let name = &value[i + c.len_utf8()..end];
        if !name.is_empty() {
            names.push(name.to_owned());
        }
        i = end + CMD_END.len_utf8();
    }
    names
}

/// `replaceCommandTokensInline`: each command becomes `/name`; stray sentinels go.
pub fn replace_commands(value: &str) -> String {
    let mut out = String::new();
    let mut i = 0;
    while let Some(c) = at(value, i) {
        if c != CMD_START {
            out.push(c);
            i += c.len_utf8();
            continue;
        }
        let Some(end) = find(value, i + c.len_utf8(), CMD_END) else {
            out.push_str(&value[i..]);
            break;
        };
        let after = end + CMD_END.len_utf8();
        let name = &value[i + c.len_utf8()..end];
        if name.is_empty() {
            out.push_str(&value[i..after]);
        } else {
            out.push('/');
            out.push_str(name);
        }
        i = after;
    }
    out.replace([CMD_START, CMD_END], "")
}

pub enum FileSeg {
    Text(String),
    File { display: String, abs_path: String },
}

/// `parseFileSegments`.
pub fn file_segments(value: &str) -> Vec<FileSeg> {
    let mut out = Vec::new();
    let mut text = String::new();
    let mut i = 0;
    while let Some(c) = at(value, i) {
        if c != FILE_START {
            text.push(c);
            i += c.len_utf8();
            continue;
        }
        let Some(end) = find(value, i + c.len_utf8(), FILE_END) else {
            text.push_str(&value[i..]);
            break;
        };
        let after = end + FILE_END.len_utf8();
        let inner = &value[i + c.len_utf8()..end];
        match inner.split_once(SEP) {
            Some((display, path)) if !display.is_empty() && !path.is_empty() => {
                if !text.is_empty() {
                    out.push(FileSeg::Text(std::mem::take(&mut text)));
                }
                out.push(FileSeg::File {
                    display: display.to_owned(),
                    abs_path: path.to_owned(),
                });
            }
            _ => text.push_str(&value[i..after]),
        }
        i = after;
    }
    if !text.is_empty() {
        out.push(FileSeg::Text(text));
    }
    out
}

/// `replaceFileTokensInline`: a file reads `(display)`, a folder its bare name.
pub fn replace_files(value: &str) -> String {
    file_segments(value)
        .into_iter()
        .map(|s| match s {
            FileSeg::Text(text) => text,
            FileSeg::File { display, abs_path } if abs_path.ends_with('/') => display,
            FileSeg::File { display, .. } => format!("({display})"),
        })
        .collect()
}

/// `parseConversationSegments` with each chat as `label(title, id)`.
pub fn replace_conversations(value: &str, label: impl Fn(&str, &str) -> String) -> String {
    let mut out = String::new();
    let mut i = 0;
    while let Some(start) = find(value, i, CONV_START) {
        let Some(end) = find(value, start + CONV_START.len_utf8(), CONV_END) else {
            break;
        };
        let after = end + CONV_END.len_utf8();
        let inner = &value[start + CONV_START.len_utf8()..end];
        out.push_str(&value[i..start]);
        match inner.split_once(SEP) {
            Some((title, id)) if !title.is_empty() && !id.is_empty() => out.push_str(&label(title, id)),
            _ => out.push_str(&value[start..after]),
        }
        i = after;
    }
    out.push_str(&value[i..]);
    out
}

/// `extractConversationRefs`: (title, session id), unique by id.
pub fn conversation_refs(value: &str) -> Vec<(String, String)> {
    let refs = std::cell::RefCell::new(Vec::<(String, String)>::new());
    replace_conversations(value, |title, id| {
        let mut refs = refs.borrow_mut();
        if !refs.iter().any(|(_, seen)| seen == id) {
            refs.push((title.to_owned(), id.to_owned()));
        }
        String::new()
    });
    refs.into_inner()
}

/// `parseToolSegments` joined with each known tool's wire text.
fn tool_text(value: &str, with: impl Fn(&Tool) -> &'static str) -> (String, bool) {
    let strip = |s: &str| s.replace([TOOL_START, TOOL_END], "");
    let mut out = String::new();
    let mut text = String::new();
    let mut found = false;
    let mut i = 0;
    while let Some(start) = find(value, i, TOOL_START) {
        text.push_str(&value[i..start]);
        let body = start + TOOL_START.len_utf8();
        let end = find(value, body, TOOL_END);
        let next = find(value, body, TOOL_START);
        let Some(end) = end.filter(|e| next.is_none_or(|n| n > *e)) else {
            i = body;
            continue;
        };
        let id = &value[body..end];
        match tool(id) {
            Some(t) => {
                out.push_str(&strip(&std::mem::take(&mut text)));
                out.push_str(with(t));
                found = true;
            }
            None => text.push_str(id),
        }
        i = end + TOOL_END.len_utf8();
    }
    text.push_str(&value[i..]);
    out.push_str(&strip(&text));
    (out, found)
}

/// `replaceToolTokensInline`.
pub fn replace_tools(value: &str) -> String {
    if !value.contains([TOOL_START, TOOL_END]) {
        return value.to_owned();
    }
    tool_text(value, |t| t.wire).0
}

/// `hasToolTokens`.
pub fn has_tools(value: &str) -> bool {
    value.contains(TOOL_START) && tool_text(value, |t| t.wire).1
}

/// A selection context (`SelectionContext`).
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Context {
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
}

pub enum ContextSeg {
    Text(String),
    Context(Context),
}

/// `parseSelectionContextSegments`: malformed tokens stay text.
pub fn context_segments(value: &str) -> Vec<ContextSeg> {
    let mut out = Vec::new();
    let mut text = String::new();
    let mut i = 0;
    while i < value.len() {
        let Some(start) = find(value, i, CONTEXT_START) else {
            text.push_str(&value[i..]);
            break;
        };
        text.push_str(&value[i..start]);
        let Some(end) = find(value, start + CONTEXT_START.len_utf8(), CONTEXT_END) else {
            text.push_str(&value[start..]);
            break;
        };
        let after = end + CONTEXT_END.len_utf8();
        match decode_context(&value[start + CONTEXT_START.len_utf8()..end]) {
            Some(context) => {
                if !text.is_empty() {
                    out.push(ContextSeg::Text(std::mem::take(&mut text)));
                }
                out.push(ContextSeg::Context(context));
            }
            None => text.push_str(&value[start..after]),
        }
        i = after;
    }
    if !text.is_empty() {
        out.push(ContextSeg::Text(text));
    }
    out
}

fn decode_context(inner: &str) -> Option<Context> {
    let (language, payload) = match inner.split_once(SEP) {
        Some((language, payload)) => (Some(language.to_owned()), payload),
        None => (None, inner),
    };
    let text = decode_uri_component(payload)?;
    (!text.is_empty()).then_some(Context { text, language })
}

pub fn has_contexts(value: &str) -> bool {
    context_segments(value)
        .iter()
        .any(|s| matches!(s, ContextSeg::Context(_)))
}

const LABEL_SUFFIX: &str = " (selected from the conversation):";

fn is_token_start(c: char) -> bool {
    matches!(
        c,
        SKILL_START | PAD_START | CMD_START | FILE_START | CONTEXT_START | CONV_START
    )
}

fn is_token_end(c: char) -> bool {
    matches!(c, SKILL_END | PAD_END | CMD_END | FILE_END | CONTEXT_END | CONV_END)
}

/// One more backslash before each typed `[Context N]`.
fn escape_markers_in(text: &str, out: &mut String) {
    let mut rest = text;
    while let Some(at) = rest.find("[Context ") {
        let after = &rest[at + "[Context ".len()..];
        let digits = after.bytes().take_while(u8::is_ascii_digit).count();
        out.push_str(&rest[..at]);
        if digits > 0 && after[digits..].starts_with(']') {
            out.push('\\');
        }
        out.push_str("[Context ");
        rest = after;
    }
    out.push_str(rest);
}

/// `escapeTypedMarkers`: markers outside any pill token.
fn escape_markers(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut last = 0;
    let mut i = 0;
    while let Some(c) = at(text, i) {
        let ender = is_token_start(c)
            .then(|| {
                text[i + c.len_utf8()..]
                    .find(is_token_end)
                    .map(|e| i + c.len_utf8() + e)
            })
            .flatten();
        match ender {
            Some(end) => {
                let end = end + at(text, end).map_or(0, char::len_utf8);
                escape_markers_in(&text[last..i], &mut out);
                out.push_str(&text[i..end]);
                last = end;
                i = end;
            }
            None => i += c.len_utf8(),
        }
    }
    escape_markers_in(&text[last..], &mut out);
    out
}

fn fence_for(code: &str) -> String {
    let mut longest = 0;
    let mut run = 0;
    for c in code.chars() {
        run = if c == '`' { run + 1 } else { 0 };
        longest = longest.max(run);
    }
    "`".repeat((longest + 1).max(3))
}

fn quote_block(context: &Context, index: usize) -> String {
    let body = match &context.language {
        None => context.text.clone(),
        Some(language) => {
            let fence = fence_for(&context.text);
            format!("{fence}{language}\n{}\n{fence}", context.text)
        }
    };
    let quoted: Vec<String> = body
        .split('\n')
        .map(|line| {
            if line.is_empty() {
                ">".to_owned()
            } else {
                format!("> {line}")
            }
        })
        .collect();
    format!("Context {index}{LABEL_SUFFIX}\n{}", quoted.join("\n"))
}

/// `serializeSelectionContexts`: one labelled quote block per context, then the sentence
/// with each context as `[Context N]`, a leading command kept first.
pub fn serialize_contexts(value: &str) -> String {
    let mut contexts = Vec::new();
    let mut sentence = String::new();
    for seg in context_segments(value) {
        match seg {
            ContextSeg::Text(text) => sentence.push_str(&escape_markers(&text)),
            ContextSeg::Context(context) => {
                contexts.push(context);
                sentence.push_str(&format!("[Context {}]", contexts.len()));
            }
        }
    }
    if contexts.is_empty() {
        return if value.contains(LABEL_SUFFIX) {
            escape_markers(value)
        } else {
            value.to_owned()
        };
    }
    let sentence = js_trim(&sentence);
    let preamble: Vec<String> = contexts
        .iter()
        .enumerate()
        .map(|(i, c)| quote_block(c, i + 1))
        .collect();
    let preamble = preamble.join("\n\n");
    if sentence.starts_with(CMD_START)
        && let Some(end) = sentence.find(CMD_END)
    {
        let split = end + CMD_END.len_utf8();
        let rest = sentence[split..].trim_start_matches(is_js_space);
        return format!("{}\n\n{preamble}\n\n{rest}", &sentence[..split]);
    }
    format!("{preamble}\n\n{sentence}")
}

/// `createSelectionContext`: None when nothing is left. Prose that is one fenced block becomes
/// code.
pub fn create_context(raw: &str, language: Option<&str>) -> Option<Context> {
    let text: String = raw
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .chars()
        .filter(|c| !is_sentinel(*c) && *c != SEP)
        .collect();
    let Some(language) = language else {
        let trimmed = js_trim(&text);
        if trimmed.is_empty() {
            return None;
        }
        return Some(fenced_block(trimmed).unwrap_or_else(|| Context {
            text: trimmed.to_owned(),
            language: None,
        }));
    };
    let body = strip_blank_lead(&text).trim_end_matches(is_js_space);
    if js_trim(body).is_empty() {
        return None;
    }
    let lang: String = language
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || "_+#.-".contains(*c))
        .collect();
    Some(Context {
        text: body.to_owned(),
        language: Some(if lang == "text" { String::new() } else { lang }),
    })
}

fn is_sentinel(c: char) -> bool {
    ('\u{E000}'..='\u{E00F}').contains(&c)
}

/// `/^(?:[ \t]*\n)+/` removed.
fn strip_blank_lead(text: &str) -> &str {
    let mut rest = text;
    loop {
        let line = rest.trim_start_matches([' ', '\t']);
        match line.strip_prefix('\n') {
            Some(next) => rest = next,
            None => return rest,
        }
    }
}

/// `parseFencedBlock`.
fn fenced_block(text: &str) -> Option<Context> {
    let lines: Vec<&str> = text.split('\n').collect();
    if lines.len() < 3 {
        return None;
    }
    let open = lines[0];
    let ticks = open.chars().take_while(|c| *c == '`').count();
    let language = &open[ticks..];
    let fence = &open[..ticks];
    if ticks < 3
        || !language
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "_+#.-".contains(c))
        || lines[lines.len() - 1] != fence
    {
        return None;
    }
    let inner = lines[1..lines.len() - 1].join("\n");
    (!js_trim(&inner).is_empty()).then(|| Context {
        text: inner,
        language: Some(language.to_owned()),
    })
}
