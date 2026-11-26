//! What Send dispatches for a composer document and its attachments
//! (`useChatComposer.buildPromptParts`, then `composer-send-blocks.ts` `buildComposerSend`):
//! a plain text prompt, or content blocks with the display blocks the transcript keeps.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::doc::{Doc, Pill, js_trim};
use super::uri::{encode_uri, encode_uri_component};
use super::wire::{self, Gap};

/// A staged attachment (`PendingAttachment`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", rename_all_fields = "camelCase")]
pub enum Attachment {
    Image {
        id: String,
        name: String,
        mime_type: String,
        base64: String,
    },
    FileRef {
        id: String,
        name: String,
        mime_type: String,
        path: String,
    },
    FileEmbed {
        id: String,
        name: String,
        mime_type: String,
        text: String,
    },
}

impl Attachment {
    /// `attachmentToBlock`.
    pub fn block(&self) -> Value {
        match self {
            Attachment::Image { mime_type, base64, .. } => {
                json!({ "type": "image", "mimeType": mime_type, "data": base64 })
            }
            Attachment::FileRef {
                name, mime_type, path, ..
            } => {
                json!({ "type": "resource_link", "uri": file_url(path), "name": name, "mimeType": mime_type })
            }
            Attachment::FileEmbed {
                name, mime_type, text, ..
            } => json!({
                "type": "resource",
                "resource": {
                    "uri": format!("attachment:///{}", encode_uri_component(name)),
                    "mimeType": mime_type,
                    "text": text,
                },
            }),
        }
    }
}

/// Transcripts resolved for the chats a draft mentions (`ConversationContexts`).
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub struct ChatContexts {
    pub gaps: BTreeMap<String, Gap>,
    pub blocks: Vec<Value>,
}

/// What Send dispatches (`ComposerSend`).
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Send {
    Text {
        text: String,
    },
    Blocks {
        wire: Vec<Value>,
        #[serde(skip_serializing_if = "Option::is_none")]
        display: Option<Vec<Value>>,
    },
}

/// A skill pill whose path neither the pick nor the agent's skills list knows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MissingSkillPath(pub String);

impl std::fmt::Display for MissingSkillPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Skill '{}' is missing a path", self.0)
    }
}

/// The send for `doc`, or None when there is nothing to send. `skills` is the agent's skills
/// list (name, path); `chats` the resolved transcripts of mentioned chats, if any.
pub fn compose(
    doc: &Doc,
    skills: &[(String, String)],
    chats: Option<&ChatContexts>,
    attachments: &[Attachment],
) -> Result<Option<Send>, MissingSkillPath> {
    let value = doc.display_text();
    let picked = |name: &str| {
        doc.pills().find_map(|p| match p {
            Pill::Skill { name: n, path } if n == name && !path.is_empty() => Some(path.clone()),
            _ => None,
        })
    };
    let mut framed = Vec::new();
    for name in wire::skill_names(&value) {
        let path = picked(&name)
            .or_else(|| skills.iter().find(|(n, _)| *n == name).map(|(_, p)| p.clone()))
            .filter(|p| !p.is_empty())
            .ok_or_else(|| MissingSkillPath(name.clone()))?;
        framed.push((name, path));
    }
    let wire_text = wire::wire_text(&value, &framed, chats.map(|c| &c.gaps));
    let file_blocks = file_mention_blocks(&value);
    let context_blocks = chats.map(|c| c.blocks.clone()).unwrap_or_default();
    let has_command = !wire::command_names(&value).is_empty();
    let has_contexts = wire::has_contexts(&value);
    let has_chats = !wire::conversation_refs(&value).is_empty();
    let has_tools = wire::has_tools(&value);
    let wire_trimmed = js_trim(&wire_text);
    let display_trimmed = js_trim(&value);
    let has_files = !file_blocks.is_empty();
    if wire_trimmed.is_empty() && attachments.is_empty() && !has_files {
        return Ok(None);
    }
    let text_block = |text: &str| json!({ "type": "text", "text": text });
    if !attachments.is_empty() {
        let with_attachments = |text: &str| -> Vec<Value> {
            let lead = (!js_trim(text).is_empty()).then(|| text_block(text));
            lead.into_iter()
                .chain(attachments.iter().map(Attachment::block))
                .collect()
        };
        let mut wire = with_attachments(&wire_text);
        wire.extend(file_blocks);
        wire.extend(context_blocks);
        let wire = dedupe(wire);
        if !(!framed.is_empty() || has_files || has_command || has_contexts || has_chats || has_tools) {
            return Ok(Some(Send::Blocks { wire, display: None }));
        }
        return Ok(Some(Send::Blocks {
            wire,
            display: Some(dedupe(with_attachments(&value))),
        }));
    }
    let only_if = |trimmed: &str, block: Value| if trimmed.is_empty() { vec![] } else { vec![block] };
    if !framed.is_empty() || has_files || has_chats {
        let mut wire = only_if(wire_trimmed, text_block(&wire_text));
        wire.extend(file_blocks);
        wire.extend(context_blocks);
        return Ok(Some(Send::Blocks {
            wire: dedupe(wire),
            display: Some(only_if(display_trimmed, text_block(&value))),
        }));
    }
    if has_command || has_contexts || has_tools {
        return Ok(Some(Send::Blocks {
            wire: only_if(wire_trimmed, text_block(wire_trimmed)),
            display: Some(only_if(display_trimmed, text_block(display_trimmed))),
        }));
    }
    Ok(Some(Send::Text {
        text: wire_trimmed.to_owned(),
    }))
}

/// `fileMentionBlocks`: a `resource_link` per mentioned file (not folder), once per path.
pub fn file_mention_blocks(value: &str) -> Vec<Value> {
    let mut seen = Vec::new();
    let mut blocks = Vec::new();
    for seg in wire::file_segments(&wire::replace_commands(&wire::serialize_contexts(value))) {
        let wire::FileSeg::File { display, abs_path } = seg else {
            continue;
        };
        if abs_path.ends_with('/') || seen.contains(&abs_path) {
            continue;
        }
        let mime = guess_mime_type(&display);
        blocks.push(
            Attachment::FileRef {
                id: abs_path.clone(),
                name: display,
                mime_type: mime.to_owned(),
                path: abs_path.clone(),
            }
            .block(),
        );
        seen.push(abs_path);
    }
    blocks
}

/// `dedupeAttachmentBlocks`: one `resource_link` per uri.
fn dedupe(blocks: Vec<Value>) -> Vec<Value> {
    let mut seen = Vec::new();
    blocks
        .into_iter()
        .filter(|b| {
            if b["type"] != "resource_link" {
                return true;
            }
            match b["uri"].as_str().filter(|u| !u.is_empty()) {
                Some(uri) if seen.iter().any(|s| s == uri) => false,
                Some(uri) => {
                    seen.push(uri.to_owned());
                    true
                }
                None => true,
            }
        })
        .collect()
}

/// `pathToFileUrl`.
pub fn file_url(path: &str) -> String {
    let norm = path.replace('\\', "/");
    let with_slash = if norm.starts_with('/') {
        norm
    } else {
        format!("/{norm}")
    };
    format!("file://{}", encode_uri(&with_slash))
}

fn basename(path: &str) -> &str {
    path.rsplit(['/', '\\'])
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or(path)
}

const MEDIA_MIME: &[(&str, &str)] = &[
    ("png", "image/png"),
    ("jpg", "image/jpeg"),
    ("jpeg", "image/jpeg"),
    ("gif", "image/gif"),
    ("webp", "image/webp"),
    ("bmp", "image/bmp"),
    ("avif", "image/avif"),
    ("ico", "image/x-icon"),
    ("svg", "image/svg+xml"),
    ("mp4", "video/mp4"),
    ("m4v", "video/mp4"),
    ("webm", "video/webm"),
    ("mov", "video/quicktime"),
    ("ogv", "video/ogg"),
    ("pdf", "application/pdf"),
];

const EXT_MIME: &[(&str, &str)] = &[
    ("txt", "text/plain"),
    ("md", "text/markdown"),
    ("mdx", "text/markdown"),
    ("csv", "text/csv"),
    ("tsv", "text/tab-separated-values"),
    ("log", "text/plain"),
    ("rtf", "text/rtf"),
    ("json", "application/json"),
    ("jsonc", "application/json"),
    ("yaml", "application/yaml"),
    ("yml", "application/yaml"),
    ("toml", "application/toml"),
    ("xml", "application/xml"),
    ("ini", "text/plain"),
    ("env", "text/plain"),
    ("cfg", "text/plain"),
    ("conf", "text/plain"),
    ("properties", "text/plain"),
    ("html", "text/html"),
    ("htm", "text/html"),
    ("css", "text/css"),
    ("scss", "text/x-scss"),
    ("sass", "text/x-sass"),
    ("less", "text/x-less"),
    ("js", "text/javascript"),
    ("cjs", "text/javascript"),
    ("mjs", "text/javascript"),
    ("jsx", "text/jsx"),
    ("ts", "text/typescript"),
    ("tsx", "text/tsx"),
    ("py", "text/x-python"),
    ("rb", "text/x-ruby"),
    ("rs", "text/x-rust"),
    ("go", "text/x-go"),
    ("java", "text/x-java"),
    ("kt", "text/x-kotlin"),
    ("swift", "text/x-swift"),
    ("c", "text/x-c"),
    ("h", "text/x-c"),
    ("cpp", "text/x-c++"),
    ("cc", "text/x-c++"),
    ("hpp", "text/x-c++"),
    ("cs", "text/x-csharp"),
    ("php", "text/x-php"),
    ("sh", "text/x-shellscript"),
    ("bash", "text/x-shellscript"),
    ("zsh", "text/x-shellscript"),
    ("ps1", "text/x-powershell"),
    ("sql", "application/sql"),
    ("graphql", "application/graphql"),
    ("gql", "application/graphql"),
    ("vue", "text/plain"),
    ("svelte", "text/plain"),
    ("astro", "text/plain"),
];

fn lookup(table: &[(&str, &'static str)], ext: &str) -> Option<&'static str> {
    table.iter().find(|(e, _)| *e == ext).map(|(_, m)| *m)
}

/// `guessMimeType`: media types first, then text and code, else `text/plain`.
pub fn guess_mime_type(name: &str) -> &'static str {
    let base = basename(name);
    let ext = match base.rfind('.') {
        Some(dot) if dot > 0 => base[dot + 1..].to_lowercase(),
        _ => String::new(),
    };
    if !ext.is_empty()
        && let Some(mime) = lookup(MEDIA_MIME, &ext)
    {
        return mime;
    }
    lookup(EXT_MIME, &ext).unwrap_or("text/plain")
}
