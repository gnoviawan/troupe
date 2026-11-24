//! `classifySetupError` and the messages it shows (`acp-spawn-errors.ts`):
//! the category of a failed agent setup, its short label, and the detail
//! a person reads (`formatAcpSpawnError`, `formatAcpAgentError`,
//! `describeProviderFailure`). Checked against the TypeScript classifier's
//! answers in `setup_error_tests.rs`.

use serde_json::{Value, json};

use super::host_error::{self, HostError};

const ENOENT: [&str; 5] = [
    "enoent",
    "no such file",
    "program not found",
    "command not found",
    "failed to spawn",
];

const TRANSPORT: [&str; 18] = [
    "destroyed",
    "broken pipe",
    "econnreset",
    "econnrefused",
    "disconnected",
    "connection closed",
    "connection lost",
    "connection reset",
    "connection refused",
    "connection aborted",
    "stream closed",
    "stream ended",
    "stream destroyed",
    "stream error",
    "transport",
    "agent thread is no longer running",
    "agent thread dropped",
    "pipe",
];

const AUTH: [&str; 7] = [
    "auth",
    "login",
    "credential",
    "unauthoris",
    "unauthoriz",
    "logged out",
    "forbidden",
];

pub(crate) fn word(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// `needle` starts somewhere in `text` and the test accepts what follows.
pub(crate) fn find_with(text: &str, needle: &str, rest: impl Fn(&str) -> bool) -> bool {
    text.match_indices(needle)
        .any(|(at, _)| rest(&text[at + needle.len()..]))
}

/// `a.*b` on one line, as `.` stops at a line break.
pub(crate) fn on_a_line(text: &str, a: &str, b: &str) -> bool {
    text.split(['\n', '\r', '\u{2028}', '\u{2029}'])
        .any(|line| find_with(line, a, |rest| rest.contains(b)))
}

/// `word[\s-]?word`.
pub(crate) fn joined(text: &str, head: &str, tail: &str) -> bool {
    find_with(text, head, |rest| {
        let rest = rest
            .strip_prefix(|c: char| c == '-' || c.is_whitespace())
            .unwrap_or(rest);
        rest.starts_with(tail)
    }) || text.contains(&format!("{head}{tail}"))
}

/// `\bcode\b`.
pub(crate) fn whole_number(text: &str, code: &str) -> bool {
    text.match_indices(code).any(|(at, _)| {
        let before = text[..at].chars().next_back().is_none_or(|c| !word(c));
        let after = text[at + code.len()..]
            .chars()
            .next()
            .is_none_or(|c| !word(c));
        before && after
    })
}

/// The `classifySetupError` categories.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SetupCategory {
    /// Several sign-in methods and none chosen.
    MultiAuth,
    Auth,
    Spawn,
    Provider,
    /// A broken stream or connection: the process cannot be reused.
    Transport,
    Timeout,
    Other,
}

impl SetupCategory {
    fn name(self) -> &'static str {
        match self {
            Self::MultiAuth => "multi-auth",
            Self::Auth => "auth",
            Self::Spawn => "spawn",
            Self::Provider => "provider",
            Self::Transport => "transport",
            Self::Timeout => "timeout",
            Self::Other => "unknown",
        }
    }

    /// `SETUP_ERROR_LABELS`.
    fn label(self) -> &'static str {
        match self {
            Self::MultiAuth => "Multiple sign-in methods",
            Self::Auth => "Authentication required",
            Self::Spawn => "Agent unavailable",
            Self::Provider => "Provider refused the request",
            Self::Transport => "Agent connection lost",
            Self::Timeout => "Session setup timed out",
            Self::Other => "Setup failed",
        }
    }
}

const NPX_CACHE_REPAIR_GUIDANCE: &str = "The npx cache may be incomplete. Run `npm config get cache`, rename the `_npx` directory there as a backup, then retry.";
const RATE_LIMIT_MESSAGE: &str =
    "Connection was rate-limited by the agent service. Wait a moment, then retry.";
const STREAM_INTERRUPT_MESSAGE: &str =
    "Agent connection was interrupted. Wait a moment, then retry.";

/// `PROVIDER_ERROR_LABELS`.
fn provider_label(code: &str) -> &'static str {
    match code {
        "provider_quota_exceeded" => "Usage limit reached",
        "provider_session_limit" => "Session limit reached",
        _ => "Provider rejected the request",
    }
}

/// `npxDamagedCacheBin`: the package executable an npx child could not
/// find (`exit status: 127` and a shell's `not found`).
fn npx_damaged_bin(message: &str) -> Option<String> {
    let exited = find_with(message, "exit status:", |rest| {
        let rest = rest.trim_start();
        rest.starts_with("127") && rest[3..].chars().next().is_none_or(|c| !word(c))
    });
    if !exited {
        return None;
    }
    message.match_indices("sh:").find_map(|(at, _)| {
        let before = message[..at].chars().next_back();
        if before.is_some_and(word) {
            return None;
        }
        let mut rest = message[at + 3..].trim_start_matches(char::is_whitespace);
        if rest.len() == message[at + 3..].len() {
            return None;
        }
        let digits = rest.chars().take_while(char::is_ascii_digit).count();
        if digits > 0 && rest[digits..].starts_with(':') {
            rest = rest[digits + 1..].trim_start_matches(char::is_whitespace);
        }
        let bin: String = rest
            .chars()
            .take_while(|c| !c.is_whitespace() && !matches!(c, ':' | '\'' | '"'))
            .collect();
        let after = rest[bin.len()..].strip_prefix(':')?.trim_start();
        let after = after.strip_prefix("command ").unwrap_or(after);
        let found = after
            .strip_prefix("not found")
            .is_some_and(|rest| rest.chars().next().is_none_or(|c| !word(c)));
        (!bin.is_empty() && found).then_some(bin)
    })
}

/// `NPX_ENTRY_MANIFEST_PATTERN`: npm could not read an npx entry's
/// `package.json` (`enoent … _npx/<hex>/package.json`).
fn npx_entry_manifest(lower: &str) -> bool {
    let slashes = |text: &str| text.len() - text.trim_start_matches(['/', '\\']).len();
    find_with(lower, "enoent", |rest| {
        rest.match_indices("_npx").any(|(at, _)| {
            let tail = &rest[at + 4..];
            let lead = slashes(tail);
            let tail = &tail[lead..];
            let hex = tail.chars().take_while(char::is_ascii_hexdigit).count();
            let tail = &tail[hex..];
            lead > 0
                && hex > 0
                && slashes(tail) > 0
                && tail[slashes(tail)..].starts_with("package.json")
        })
    })
}

/// `NPM_MISSING_DEP_PATTERN`.
fn npm_missing_dep(lower: &str) -> bool {
    [
        "missing optional dependency",
        "cannot find module",
        "err_module_not_found",
        "module_not_found",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

/// `isNpxCacheDamage`.
fn npx_cache_damage(message: &str, command: Option<&str>) -> bool {
    let lower = message.to_ascii_lowercase();
    command == Some("npx")
        && (npx_damaged_bin(message).is_some()
            || npx_entry_manifest(&lower)
            || npm_missing_dep(&lower))
}

fn enoent(lower: &str) -> bool {
    ENOENT.iter().any(|needle| lower.contains(needle)) || on_a_line(lower, "spawn", "fail")
}

/// `classifySetupError`'s category, in its order: the ambiguous code, the
/// auth-required signal, a signed-out Account, a definite rejection, a
/// provider code, then the wording.
pub(crate) fn setup_category(error: &HostError, command: Option<&str>) -> SetupCategory {
    let code = error.code.as_deref();
    if code == Some(host_error::AMBIGUOUS_AUTH) {
        return SetupCategory::MultiAuth;
    }
    if code == Some(host_error::AUTH_REQUIRED) || error.message.starts_with("ACP_AUTH_REQUIRED") {
        return SetupCategory::Auth;
    }
    if error.is(&host_error::SIGNED_OUT) {
        return SetupCategory::Auth;
    }
    if error.is(&host_error::DEFINITE_SPAWN_REJECTION) {
        return SetupCategory::Spawn;
    }
    if code.is_some_and(|code| host_error::PROVIDER.contains(&code)) {
        return SetupCategory::Provider;
    }
    let text = error.message.to_ascii_lowercase();
    if enoent(&text) || npx_cache_damage(&error.message, command) {
        return SetupCategory::Spawn;
    }
    if transport(&text) {
        return SetupCategory::Transport;
    }
    let auth = AUTH.iter().any(|needle| text.contains(needle))
        || joined(&text, "sign", "in")
        || joined(&text, "api", "key")
        || text.contains("not logged in")
        || whole_number(&text, "401")
        || whole_number(&text, "403");
    if auth {
        SetupCategory::Auth
    } else if text.contains("timed out") || text.contains("timeout") {
        SetupCategory::Timeout
    } else {
        SetupCategory::Other
    }
}

fn pipe(lower: &str) -> bool {
    find_with(lower, "pipe", |rest| {
        rest.chars().next().is_none_or(|c| !word(c))
    })
}

fn transport(lower: &str) -> bool {
    TRANSPORT[..17].iter().any(|needle| lower.contains(needle))
        || pipe(lower)
        || on_a_line(lower, "connection", "timed out")
}

/// `formatAcpAgentError`: the rate-limit and stream-drop wording rewritten,
/// any other message as it is.
fn agent_error_text(message: &str) -> String {
    let lower = message.to_ascii_lowercase();
    let rate_limited = lower.contains("enhance_your_calm")
        || joined_any(&lower, "enhance", "your")
        || joined_any(&lower, "rate", "limit")
        || lower.contains("too many requests")
        || whole_number(&lower, "429");
    if rate_limited {
        return RATE_LIMIT_MESSAGE.into();
    }
    if lower.contains("retriableerror") || lower.contains("nghttp2_") || transport_interrupt(&lower)
    {
        return STREAM_INTERRUPT_MESSAGE.into();
    }
    message.to_owned()
}

/// `word[\s_-]?word`, with `calm` after `enhance your`.
fn joined_any(lower: &str, head: &str, tail: &str) -> bool {
    find_with(lower, head, |rest| {
        let rest = rest
            .strip_prefix(|c: char| c == '-' || c == '_' || c.is_whitespace())
            .unwrap_or(rest);
        if tail == "your" {
            rest.strip_prefix("your").is_some_and(|rest| {
                let rest = rest
                    .strip_prefix(|c: char| c == '-' || c == '_' || c.is_whitespace())
                    .unwrap_or(rest);
                rest.starts_with("calm")
            })
        } else {
            rest.starts_with(tail)
        }
    })
}

/// `STREAM_INTERRUPT_PATTERN` without the transport word.
fn transport_interrupt(lower: &str) -> bool {
    TRANSPORT[..14]
        .iter()
        .chain(&TRANSPORT[15..17])
        .any(|needle| lower.contains(needle))
        || pipe(lower)
        || on_a_line(lower, "connection", "timed out")
}

/// `RESOLVER_FAILURE_PATTERN`: the file the resolver could not prepare.
fn resolver_named_path(message: &str) -> Option<String> {
    let lower = message.to_ascii_lowercase();
    let at = lower.find("program not found")?;
    let rest = &message[at + "program not found".len()..];
    let lower_rest = rest.to_ascii_lowercase();
    let rest = if lower_rest.starts_with(" or not executable") {
        &rest[" or not executable".len()..]
    } else if lower_rest.starts_with(" on path") {
        &rest[" on path".len()..]
    } else {
        rest
    };
    let path = rest.strip_prefix(": ")?;
    let path = path.lines().last().unwrap_or(path);
    Some(path.trim().trim_matches('"').to_owned())
}

fn base_name(path: &str) -> Option<&str> {
    path.split(['/', '\\']).rfind(|part| !part.is_empty())
}

/// `formatAcpSpawnError`: a missing command becomes guidance; any other
/// message stays as it is.
fn spawn_error_text(message: &str, command: Option<&str>) -> String {
    let message = definite_message(message);
    let lower = message.to_ascii_lowercase();
    let damage = npx_cache_damage(message, command);
    if !enoent(&lower) && !damage {
        return message.to_owned();
    }
    if command == Some("npx") {
        if npx_entry_manifest(&message.to_ascii_lowercase()) {
            return format!(
                "npm could not read package.json in its npx cache entry for this agent. {NPX_CACHE_REPAIR_GUIDANCE}"
            );
        }
        if let Some(bin) = npx_damaged_bin(message) {
            return format!(
                "npx started, but its package executable \"{bin}\" was not found. {NPX_CACHE_REPAIR_GUIDANCE}"
            );
        }
        let lower = message.to_ascii_lowercase();
        if let Some(at) = lower.find("missing optional dependency ") {
            let dep: String = message[at + "missing optional dependency ".len()..]
                .chars()
                .take_while(|c| !c.is_whitespace() && !matches!(c, '"' | '\'' | '('))
                .collect();
            if !dep.is_empty() {
                return format!(
                    "npx started, but the package is missing its \"{dep}\" dependency. {NPX_CACHE_REPAIR_GUIDANCE}"
                );
            }
        }
        if npm_missing_dep(&lower) {
            return format!(
                "npx started, but the package failed to load a module it needs. {NPX_CACHE_REPAIR_GUIDANCE}"
            );
        }
    }
    if let Some(named) = resolver_named_path(message)
        && let (Some(named), Some(command)) = (base_name(&named), command.and_then(base_name))
        && !named.eq_ignore_ascii_case(command)
    {
        return message.to_owned();
    }
    match command {
        Some("npx") => {
            "Could not run npx. Install Node.js and ensure npx is on your PATH, then try again."
                .into()
        }
        Some("uvx") => {
            "Could not run uvx. Install uv and ensure uvx is on your PATH, then try again.".into()
        }
        Some(command) if !command.is_empty() => {
            format!(
                "Could not start \"{command}\". Check that the binary exists and is on your PATH."
            )
        }
        _ => "Could not start the ACP agent. Check that the command exists and is on your PATH."
            .into(),
    }
}

/// `definiteSpawnRejectionMessage`: the desktop tag comes off.
fn definite_message(message: &str) -> &str {
    host_error::DEFINITE_SPAWN_REJECTION
        .iter()
        .find_map(|(_, tag)| message.strip_prefix(&format!("{tag}: ")))
        .unwrap_or(message)
}

/// `classifySetupError` as the launcher reads it: `{ category, label,
/// detail }`, with `definite` for a definite rejection and `providerCode`
/// for a provider's refusal.
pub(crate) fn classify_setup_error(error: &HostError, command: Option<&str>) -> Value {
    let category = setup_category(error, command);
    let message = error.message.as_str();
    let mut classified = json!({ "category": category.name(), "label": category.label() });
    let detail = match category {
        SetupCategory::MultiAuth | SetupCategory::Timeout => message.to_owned(),
        SetupCategory::Auth => host_error::SIGNED_OUT
            .iter()
            .find_map(|(_, tag)| message.strip_prefix(&format!("{tag}: ")))
            .unwrap_or(message)
            .to_owned(),
        SetupCategory::Spawn => {
            if error.is(&host_error::DEFINITE_SPAWN_REJECTION) {
                classified["definite"] = json!(true);
            }
            spawn_error_text(message, command)
        }
        SetupCategory::Provider => {
            let code = error.code.as_deref().unwrap_or_default();
            classified["label"] = json!(provider_label(code));
            classified["providerCode"] = json!(code);
            super::agents::describe_provider_failure(Some(code), message)
                .unwrap_or_else(|| message.to_owned())
        }
        SetupCategory::Transport | SetupCategory::Other => agent_error_text(message),
    };
    classified["detail"] = json!(detail);
    classified
}
