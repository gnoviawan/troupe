//! The composer's document: text and the six pill types, and the sentinel-token display
//! string `lib/composer/doc-to-prompt.ts` makes of it (`docToDisplayText`). The display string
//! is what the transcript shows for a sent message and what the wire builder reads.

use serde::{Deserialize, Serialize};

use super::uri::encode_uri_component;

pub const SKILL_START: char = '\u{E000}';
pub const SKILL_END: char = '\u{E001}';
pub const PAD_START: char = '\u{E002}';
pub const PAD_END: char = '\u{E003}';
/// The fixed padding `docToDisplayText` writes after every skill token (`SKILL_PAD_DEFAULT`).
pub const PAD_CHAR: char = '\u{2007}';
pub const CMD_START: char = '\u{E004}';
pub const CMD_END: char = '\u{E005}';
pub const FILE_START: char = '\u{E006}';
pub const FILE_END: char = '\u{E007}';
pub const CONTEXT_START: char = '\u{E008}';
pub const CONTEXT_END: char = '\u{E009}';
pub const CONV_START: char = '\u{E00A}';
pub const CONV_END: char = '\u{E00B}';
pub const TOOL_START: char = '\u{E00C}';
pub const TOOL_END: char = '\u{E00D}';
/// Splits the two fields of a file, context or conversation token.
pub const SEP: char = '\u{001F}';

/// One inline pill. Its fields are the Tiptap node's attributes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Pill {
    /// A skill picked from the slash menu; `path` is its `SKILL.md`, empty when unknown.
    Skill { name: String, path: String },
    /// A slash command, sent as `/name` where it stands.
    Command { name: String },
    /// A file or folder from the mention menu (a folder's path ends in `/`).
    File { display: String, abs_path: String },
    /// Text selected in the transcript; `language` is set (maybe empty) only for code.
    Context { text: String, language: Option<String> },
    /// Another chat, by title and session id.
    Conversation { title: String, session_id: String },
    /// A Troupe tool (`browser`, `canvas`, `automations`, `orchestrate`).
    Tool { id: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Part {
    /// Plain text; `\n` starts a paragraph.
    Text(String),
    Pill(Pill),
}

/// The composer's content in order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Doc {
    pub parts: Vec<Part>,
}

impl Pill {
    /// The pill's sentinel token in the display string.
    pub fn token(&self) -> String {
        match self {
            Pill::Skill { name, .. } => format!("{SKILL_START}{name}{SKILL_END}{PAD_START}{PAD_CHAR}{PAD_END}"),
            Pill::Command { name } => format!("{CMD_START}{name}{CMD_END}"),
            Pill::File { display, abs_path } => format!("{FILE_START}{display}{SEP}{abs_path}{FILE_END}"),
            Pill::Context { text, language } => {
                let head = language.as_ref().map(|l| format!("{l}{SEP}")).unwrap_or_default();
                format!("{CONTEXT_START}{head}{}{CONTEXT_END}", encode_uri_component(text))
            }
            Pill::Conversation { title, session_id } => conversation_token(title, session_id),
            Pill::Tool { id } => format!("{TOOL_START}{id}{TOOL_END}"),
        }
    }

    /// What the chip shows.
    pub fn label(&self) -> String {
        match self {
            Pill::Skill { name, .. } | Pill::Command { name } => name.clone(),
            Pill::File { display, .. } => display.clone(),
            Pill::Context { text, .. } => context_preview(text, 40),
            Pill::Conversation { title, .. } => {
                let title = conversation_title_text(title);
                if title.is_empty() {
                    "Untitled chat".to_owned()
                } else {
                    title
                }
            }
            Pill::Tool { id } => super::wire::tool(id).map_or_else(|| id.clone(), |t| t.label.to_owned()),
        }
    }
}

impl Doc {
    /// `docToDisplayText`: text as typed, each pill as its token.
    pub fn display_text(&self) -> String {
        self.parts
            .iter()
            .map(|part| match part {
                Part::Text(text) => text.clone(),
                Part::Pill(pill) => pill.token(),
            })
            .collect()
    }

    /// The pills in order.
    pub fn pills(&self) -> impl Iterator<Item = &Pill> {
        self.parts.iter().filter_map(|part| match part {
            Part::Pill(pill) => Some(pill),
            Part::Text(_) => None,
        })
    }

    pub fn is_blank(&self) -> bool {
        self.parts
            .iter()
            .all(|part| matches!(part, Part::Text(t) if t.chars().all(is_js_space)))
    }
}

/// JavaScript's `\s` and `String.prototype.trim` set.
pub fn is_js_space(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n' | '\u{0B}' | '\u{0C}' | '\r' | ' ' | '\u{A0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200A}' | '\u{2028}' | '\u{2029}' | '\u{202F}' | '\u{205F}' | '\u{3000}' | '\u{FEFF}'
    )
}

/// JavaScript's `trim`.
pub fn js_trim(s: &str) -> &str {
    s.trim_matches(is_js_space)
}

fn is_pill_control(c: char) -> bool {
    ('\u{E000}'..='\u{E00F}').contains(&c) || c == SEP
}

/// `conversationTitleText`: no sentinels, whitespace runs as one space, trimmed.
pub fn conversation_title_text(title: &str) -> String {
    let clean: String = title.chars().filter(|c| !is_pill_control(*c)).collect();
    let mut out = String::with_capacity(clean.len());
    let mut space = false;
    for c in clean.chars() {
        if is_js_space(c) {
            space = true;
            continue;
        }
        if space && !out.is_empty() {
            out.push(' ');
        }
        space = false;
        out.push(c);
    }
    out
}

/// `conversationToken`.
pub fn conversation_token(title: &str, session_id: &str) -> String {
    let title = conversation_title_text(title);
    let title = if title.is_empty() { "Untitled chat" } else { &title };
    let id: String = session_id.chars().filter(|c| !is_pill_control(*c)).collect();
    format!("{CONV_START}{title}{SEP}{id}{CONV_END}")
}

/// `selectionContextPreview`: whitespace collapsed, cut to `max` characters with an ellipsis.
pub fn context_preview(text: &str, max: usize) -> String {
    let flat = text
        .split(is_js_space)
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let chars: Vec<char> = flat.chars().collect();
    if chars.len() > max {
        chars[..max - 1].iter().chain(std::iter::once(&'…')).collect()
    } else {
        flat
    }
}
