//! The keys `configToLiveAgent` maps to a live agent (`acp-reuse-keys.ts`):
//! `config\0cwd`, then optionally `\0project\0<id>`, `\0chat\0<id>`, and a
//! detached agent id.

const NUL: char = '\0';
const PROJECT_SEGMENT: &str = "project";
const CHAT_SEGMENT: &str = "chat";

/// `agentReuseKey`.
pub(crate) fn agent_reuse_key(
    config_id: &str,
    cwd: &str,
    project_id: Option<&str>,
    chat_id: Option<&str>,
) -> String {
    let mut key = format!("{config_id}{NUL}{}", crate::js_trim(cwd));
    let project = project_id.map(crate::js_trim).unwrap_or_default();
    if !project.is_empty() {
        key.push_str(&format!("{NUL}{PROJECT_SEGMENT}{NUL}{project}"));
    }
    let chat = chat_id.map(crate::js_trim).unwrap_or_default();
    if !chat.is_empty() {
        key.push_str(&format!("{NUL}{CHAT_SEGMENT}{NUL}{chat}"));
    }
    key
}

/// `detachedReuseKey`: keeps the agent reachable without letting the next
/// chat reuse the canonical key.
pub(crate) fn detached_reuse_key(reuse_key: &str, agent_id: &str) -> String {
    format!("{reuse_key}{NUL}{agent_id}")
}

/// `parseReuseKey`.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct ParsedReuseKey<'a> {
    pub config_id: &'a str,
    pub cwd: &'a str,
    pub project_id: Option<&'a str>,
    pub chat_id: Option<&'a str>,
    pub detached_agent_id: Option<&'a str>,
}

pub(crate) fn parse_reuse_key(key: &str) -> ParsedReuseKey<'_> {
    let parts: Vec<&str> = key.split(NUL).collect();
    let part = |index: usize| parts.get(index).copied();
    let mut parsed = ParsedReuseKey {
        config_id: part(0).unwrap_or_default(),
        cwd: part(1).unwrap_or_default(),
        ..ParsedReuseKey::default()
    };
    let mut next = 2;
    if part(next) == Some(PROJECT_SEGMENT) {
        parsed.project_id = Some(part(next + 1).unwrap_or_default());
        next += 2;
    }
    if part(next) == Some(CHAT_SEGMENT) {
        parsed.chat_id = Some(part(next + 1).unwrap_or_default());
        next += 2;
    }
    parsed.detached_agent_id = part(next);
    parsed
}
