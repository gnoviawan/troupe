//! The chat index snapshot: the host's rows merged with the local
//! projection so a stale load never drops a just-created row or reverts a
//! fresh title, live turns stamped on, placeholder titles adopted, and the
//! tabs of chats that are gone closed. Port of `applyIndexEntries` and
//! `stampLiveTurnOnIndex` in `slices/session.ts`, `mergeSessionIndexEntries`,
//! `adoptIndexTitles` and `normalizeIndexTitle` in `helpers.ts`, and
//! `pruneRestoredChatTabs` (`chat-tab-prune.ts`).

use std::collections::BTreeSet;

use serde_json::{Value, json};

use super::transcript::strip_handoff_preamble;
use super::types::Object;
use super::{Effect, State};

const UNTITLED: &str = "Untitled Chat";
const HANDOFF_HEADER: &str = "# Conversation handoff";

/// `isUntitledTitle`: `Untitled Chat` or `Untitled Chat <n>`.
pub(crate) fn is_untitled(title: Option<&str>) -> bool {
    let Some(title) = title else {
        return false;
    };
    title == UNTITLED
        || title
            .strip_prefix("Untitled Chat ")
            .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
}

/// A field that is present and not `null`, as `??` reads it.
fn given<'a>(entry: &'a Object, key: &str) -> Option<&'a Value> {
    entry.get(key).filter(|value| !value.is_null())
}

fn set_or_drop(entry: &mut Object, key: &str, value: Option<Value>) {
    match value {
        Some(value) => {
            entry.insert(key.into(), value);
        }
        None => {
            entry.shift_remove(key);
        }
    }
}

fn number(entry: &Object, key: &str) -> f64 {
    given(entry, key).and_then(Value::as_f64).unwrap_or(0.0)
}

/// The local row is at least as new: keep its title and activity, and carry
/// the host's durable fields forward.
fn merge_newer_local(host: &Object, local: &Object) -> Object {
    let mut merged = host.clone();
    for (key, value) in local {
        merged.insert(key.clone(), value.clone());
    }
    for key in ["messageCount", "lastSeq"] {
        merged.insert(key.into(), json!(number(local, key).max(number(host, key))));
    }
    // `first ?? second`: a null or absent first falls to the second as it is,
    // null included.
    let either = |first: &Object, second: &Object, key: &str| {
        given(first, key).or_else(|| second.get(key)).cloned()
    };
    set_or_drop(
        &mut merged,
        "agentConfigId",
        either(local, host, "agentConfigId"),
    );
    set_or_drop(
        &mut merged,
        "continuedFrom",
        either(host, local, "continuedFrom"),
    );
    set_or_drop(&mut merged, "startedBy", either(host, local, "startedBy"));
    set_or_drop(&mut merged, "kind", either(local, host, "kind"));
    let turn = if local.get("turnActive") == Some(&json!(true))
        || host.get("turnActive") == Some(&json!(true))
    {
        Some(json!(true))
    } else {
        either(local, host, "turnActive")
    };
    set_or_drop(&mut merged, "turnActive", turn);
    let host_created = host
        .get("createdAt")
        .and_then(Value::as_f64)
        .filter(|at| at.is_finite() && *at > 0.0);
    let created = host_created
        .map(|at| json!(at))
        .or_else(|| local.get("createdAt").cloned());
    set_or_drop(&mut merged, "createdAt", created);
    if is_untitled(title_of(local)) && !is_untitled(title_of(host)) {
        merged.insert(
            "title".into(),
            host.get("title").cloned().unwrap_or(Value::Null),
        );
    }
    merged
}

/// The host row is newer: it wins, except for what the client projected
/// and the host cannot know yet. `None` keeps the host row as it is.
fn merge_newer_host(host: &Object, local: &Object) -> Option<Object> {
    let keep_title = is_untitled(title_of(host));
    let keep_turn =
        local.get("turnActive") == Some(&json!(true)) && !host.contains_key("turnActive");
    let keep_edge = local.contains_key("continuedFrom") && !host.contains_key("continuedFrom");
    if !(keep_title || keep_turn || keep_edge) {
        return None;
    }
    let mut merged = host.clone();
    if keep_title {
        merged.insert(
            "title".into(),
            local.get("title").cloned().unwrap_or(Value::Null),
        );
    }
    if keep_turn {
        merged.insert("turnActive".into(), json!(true));
    }
    if keep_edge {
        merged.insert("continuedFrom".into(), local["continuedFrom"].clone());
    }
    Some(merged)
}

fn title_of(entry: &Object) -> Option<&str> {
    entry.get("title").and_then(Value::as_str)
}

fn id_of(entry: &Object) -> Option<&str> {
    entry.get("id").and_then(Value::as_str)
}

/// `normalizeIndexTitle`: a title that is the handoff wire block shows its
/// draft, its last turn line, or the placeholder.
fn normalize_title(title: &str) -> String {
    if crate::js_trim(title) == HANDOFF_HEADER {
        return UNTITLED.to_owned();
    }
    let Some(stripped) = strip_handoff_preamble(title) else {
        let topic = title
            .split('\n')
            .map(crate::js_trim)
            .rfind(|line| line.starts_with("User: ") || line.starts_with("Agent: "))
            .map(|line| {
                let rest = line.split_once(':').map_or("", |(_, rest)| rest);
                rest.trim_start_matches(|c: char| c.is_whitespace())
            })
            .filter(|topic| !topic.is_empty());
        return topic.unwrap_or(UNTITLED).to_owned();
    };
    if stripped != title && !stripped.is_empty() {
        let first = stripped.split('\n').next().unwrap_or_default();
        let first = crate::js_trim(first);
        if !first.is_empty() {
            return first.to_owned();
        }
    }
    title.to_owned()
}

/// `mergeSessionIndexEntries`.
fn merge_index(local: &[Object], host: Vec<Object>, live: &BTreeSet<String>) -> Vec<Object> {
    if local.is_empty() {
        return host;
    }
    let host_ids: BTreeSet<String> = host.iter().filter_map(id_of).map(str::to_owned).collect();
    let mut merged = host.clone();
    for entry in local {
        let Some(id) = id_of(entry) else {
            continue;
        };
        if let Some(host_entry) = host.iter().rev().find(|row| id_of(row) == Some(id)) {
            let next = if number(entry, "lastActivityAt") >= number(host_entry, "lastActivityAt") {
                Some(merge_newer_local(host_entry, entry))
            } else {
                merge_newer_host(host_entry, entry)
            };
            if let Some(next) = next
                && let Some(slot) = merged.iter_mut().find(|row| id_of(row) == Some(id))
            {
                *slot = next;
            }
        } else if live.contains(id) && !host_ids.contains(id) {
            merged.push(entry.clone());
        }
    }
    for entry in &mut merged {
        let title = entry.get("title").and_then(Value::as_str);
        if let Some(title) = title.filter(|title| title.contains(HANDOFF_HEADER)) {
            let normalized = normalize_title(title);
            entry.insert("title".into(), json!(normalized));
        }
    }
    merged
}

impl State {
    /// `applySessionIndexSnapshot`: install the host's index rows, the
    /// answer to a `refreshSessionIndex` or the web host's push.
    pub(crate) fn apply_session_index(&mut self, entries: Vec<Object>, fx: &mut Vec<Effect>) {
        let live: BTreeSet<String> = self.sessions.keys().cloned().collect();
        let mut merged = merge_index(&self.session_index, entries, &live);
        for entry in &mut merged {
            if entry.get("turnActive") == Some(&json!(true)) {
                continue;
            }
            let turn_open = id_of(entry)
                .and_then(|id| self.sessions.get(id))
                .is_some_and(|s| s.active_turn || s.open_turn_id.is_some());
            if turn_open {
                entry.insert("turnActive".into(), json!(true));
            }
        }
        for (id, session) in &mut self.sessions {
            if !is_untitled(session.title.as_deref()) {
                continue;
            }
            let title = merged
                .iter()
                .find(|entry| id_of(entry) == Some(id))
                .and_then(|entry| title_of(entry))
                .filter(|title| !title.is_empty() && !is_untitled(Some(title)));
            if let Some(title) = title {
                session.title = Some(title.to_owned());
            }
        }
        let indexed: BTreeSet<&str> = merged.iter().filter_map(id_of).collect();
        for session_id in &self.book.open_chat_tabs {
            if !live.contains(session_id) && !indexed.contains(session_id.as_str()) {
                fx.push(Effect::CloseChatTab {
                    session_id: session_id.clone(),
                });
            }
        }
        self.session_index = merged;
    }
}
