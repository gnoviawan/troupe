//! The chat composer (`components/chat/ChatInputBar.tsx`, `AgentLauncher`, `lib/composer/`):
//! its document of text and pills, and what Send makes of it. The display string and the
//! prompt equal the web client's byte for byte; `serialize_tests` replays the vectors
//! recorded from `lib/composer/` (`tools/parity/vectors/composer.ts`).

pub mod agent_menu;
pub mod controls;
mod doc;
pub mod field;
mod foot;
pub mod menu;
mod send;
mod uri;
pub mod view;
pub mod wire;

#[cfg(test)]
mod field_tests;
#[cfg(test)]
mod serialize_tests;

pub use doc::{Doc, Part, Pill, context_preview, conversation_title_text, is_js_space, js_trim, parse_display};
pub use send::{Attachment, ChatContexts, MissingSkillPath, Send, compose, file_url, guess_mime_type};
pub use uri::{base64, encode_uri_component as encode_query};
pub use wire::{Context, Gap, create_context};
