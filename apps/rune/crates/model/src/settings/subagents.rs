//! The Sub-agents page's rules (`subagent-profiles/profile-model.ts` and the queue of
//! `use-subagent-settings.ts`): profile ids, which profile may move or go, the upserts that save
//! one, and the host's view with queued changes applied. Profiles stay JSON objects, as the host
//! sends them.

use serde_json::{Map, Value, json};

pub type Object = Map<String, Value>;

/// `MIN_RUNNING`, `MAX_RUNNING`, `MIN_DEPTH` and `MAX_DEPTH`.
pub const RUNNING: (i64, i64) = (1, 8);
pub const DEPTH: (i64, i64) = (1, 4);
/// The host's `MAX_DESCRIPTION_CHARS`.
pub const MAX_DESCRIPTION_CHARS: usize = 1000;
const REGISTRY_PREFIX: &str = "acp-registry:";

fn str_of<'a>(p: &'a Object, key: &str) -> &'a str {
    p.get(key).and_then(Value::as_str).unwrap_or_default()
}

fn id_of(p: &Object) -> &str {
    str_of(p, "id")
}

fn agent_of(p: &Object) -> &str {
    str_of(p, "agentId")
}

fn installed(p: &Object) -> bool {
    p.get("agentName").is_some_and(|n| !n.is_null())
}

fn implicit(p: &Object) -> bool {
    p.get("implicit") == Some(&json!(true))
}

/// `storedProfile`: the view without its derived fields.
#[must_use]
pub fn stored(view: &Object) -> Object {
    view.iter().filter(|(k, _)| *k != "agentName" && *k != "implicit").map(|(k, v)| (k.clone(), v.clone())).collect()
}

/// `profileIdError`.
#[must_use]
pub fn id_error(id: &str, others: &[Object]) -> Option<&'static str> {
    if id.is_empty() {
        return Some("Enter a name.");
    }
    if id.encode_utf16().count() > 64 {
        return Some("Use 64 characters or fewer.");
    }
    if !id.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | ':' | '-')) {
        return Some("Use letters, digits, and . _ : - only.");
    }
    let lower = id.to_lowercase();
    others.iter().any(|o| id_of(o).to_lowercase() == lower).then_some("Another profile has this name.")
}

/// `uniqueProfileId`: `base` with the first free `-N` suffix from 2, unique ignoring case.
#[must_use]
pub fn unique_id(base: &str, taken: &[Object]) -> String {
    let trimmed = match base.rfind('-') {
        Some(at) if at + 1 < base.len() && base[at + 1..].bytes().all(|b| b.is_ascii_digit()) => &base[..at],
        _ => base,
    };
    let stem = String::from_utf16_lossy(&trimmed.encode_utf16().take(60).collect::<Vec<_>>());
    let stem = if stem.is_empty() { "agent".to_owned() } else { stem };
    let used: Vec<String> = taken.iter().map(|p| id_of(p).to_lowercase()).collect();
    (2..).map(|n| format!("{stem}-{n}")).find(|c| !used.contains(&c.to_lowercase())).unwrap_or_default()
}

/// `agentShortName`: `acp-registry:claude-acp` reads as `claude`.
#[must_use]
pub fn agent_short_name(key: &str) -> String {
    let bare = key.strip_prefix(REGISTRY_PREFIX).unwrap_or(key);
    let tail = &bare[bare.rfind(':').map_or(0, |i| i + 1)..];
    let tail = tail.strip_suffix("-acp").unwrap_or(tail).to_lowercase();
    let mut out = String::new();
    let mut in_run = false;
    for c in tail.chars() {
        let keep = c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '_' | '-');
        if keep {
            out.push(c);
        } else if !in_run {
            out.push('-');
        }
        in_run = !keep;
    }
    if out.is_empty() { "agent".into() } else { out }
}

/// `canDeleteProfile`: an agent's only profile stays; one whose agent is gone can always go.
#[must_use]
pub fn can_delete(profile: &Object, all: &[Object]) -> bool {
    if implicit(profile) {
        return false;
    }
    if !installed(profile) {
        return true;
    }
    all.iter().filter(|o| agent_of(o) == agent_of(profile)).count() > 1
}

/// `agentLockReason`.
#[must_use]
pub fn agent_lock_reason(profile: &Object, all: &[Object]) -> Option<&'static str> {
    if !can_delete(profile, all) {
        return Some("This agent's only profile stays on it. Duplicate it to use another agent.");
    }
    (id_of(profile).to_lowercase() == agent_of(profile).to_lowercase()).then_some("Named after its agent, so it stays on it. Rename it to move it.")
}

/// `profileSkipsPrompts`: Allow everything, or a bypass-type mode.
#[must_use]
pub fn skips_prompts(profile: &Object) -> bool {
    str_of(profile, "approvals") == "allowAll" || matches!(str_of(profile, "mode"), "bypassPermissions" | "agent-full-access")
}

/// `upsertProfileView`.
fn upsert_view(profiles: &[Object], profile: &Object, previous: Option<&str>) -> Vec<Object> {
    let target = previous.unwrap_or(id_of(profile));
    let agent_name = profiles.iter().find(|o| agent_of(o) == agent_of(profile)).and_then(|o| o.get("agentName").cloned()).unwrap_or(Value::Null);
    let mut next = profile.clone();
    next.insert("agentName".into(), agent_name);
    next.insert("implicit".into(), json!(false));
    let mut out = profiles.to_vec();
    match out.iter().position(|o| id_of(o) == target) {
        Some(at) => out[at] = next,
        None => out.push(next),
    }
    out
}

/// `profileUpserts`: an agent's implicit default is stored first when `profile` would be its
/// agent's first stored profile, so it stays in the list.
#[must_use]
pub fn profile_upserts(profiles: &[Object], profile: &Object, previous: Option<&str>) -> Vec<Value> {
    let target = previous.unwrap_or(id_of(profile));
    let implicit_default = profiles.iter().find(|o| implicit(o) && agent_of(o) == agent_of(profile) && id_of(o) != target);
    let upsert = match previous {
        Some(p) => json!({ "previousId": p, "profile": profile }),
        None => json!({ "profile": profile }),
    };
    match implicit_default {
        Some(d) => vec![json!({ "profile": stored(d) }), upsert],
        None => vec![upsert],
    }
}

/// `applySettingsInput`: the gates, the deletes, then the upserts, as the host applies them.
#[must_use]
pub fn apply_input(view: &Value, input: &Value) -> Value {
    let deletes: Vec<&str> = input.get("deleteProfiles").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).collect();
    let mut profiles: Vec<Object> = profiles_of(view).into_iter().filter(|p| !deletes.contains(&id_of(p))).collect();
    for u in input.get("upsertProfiles").and_then(Value::as_array).into_iter().flatten() {
        if let Some(p) = u.get("profile").and_then(Value::as_object) {
            profiles = upsert_view(&profiles, p, u.get("previousId").and_then(Value::as_str));
        }
    }
    let mut out = view.as_object().cloned().unwrap_or_default();
    for key in ["enabled", "maxRunning", "maxDepth"] {
        if let Some(v) = input.get(key).filter(|v| !v.is_null()) {
            out.insert(key.into(), v.clone());
        }
    }
    out.insert("profiles".into(), Value::Array(profiles.into_iter().map(Value::Object).collect()));
    Value::Object(out)
}

/// The view's profiles.
#[must_use]
pub fn profiles_of(view: &Value) -> Vec<Object> {
    view.get("profiles").and_then(Value::as_array).into_iter().flatten().filter_map(|p| p.as_object().cloned()).collect()
}

/// A new profile of `agent` (`addProfile` without a base).
#[must_use]
pub fn fresh(id: &str, agent: &str) -> Object {
    let v = json!({ "id": id, "agentId": agent, "enabled": true, "model": null, "mode": null, "configOptions": {}, "approvals": "ask", "description": null });
    v.as_object().cloned().unwrap_or_default()
}

/// The group footnote: the ids the calling agent sees, as "a, b and c".
#[must_use]
pub fn footnote(profiles: &[Object]) -> String {
    let ids: Vec<&str> = profiles.iter().filter(|p| p.get("enabled") == Some(&json!(true)) && installed(p)).map(id_of).collect();
    if ids.is_empty() {
        return "Every profile is off, so the calling agent sees none.".into();
    }
    let mut list = String::new();
    for (i, id) in ids.iter().enumerate() {
        list.push_str(if i == 0 {
            ""
        } else if i == ids.len() - 1 {
            " and "
        } else {
            ", "
        });
        list.push_str(id);
    }
    format!("The calling agent sees {list} with their notes. Off profiles are hidden from it.")
}

/// The row's second line.
#[must_use]
pub fn subtitle(profile: &Object, agent_name: &str) -> String {
    if !installed(profile) {
        return format!("{} is not installed. Its profile cannot start.", agent_of(profile));
    }
    profile.get("description").and_then(Value::as_str).map_or_else(|| format!("{agent_name}. No note yet."), str::to_owned)
}

#[cfg(test)]
#[path = "subagents_tests.rs"]
mod tests;
