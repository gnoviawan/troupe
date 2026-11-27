use serde_json::{Value, json};

use super::*;

fn obj(v: Value) -> Object {
    v.as_object().cloned().unwrap()
}

fn view() -> Value {
    json!({ "enabled": true, "maxRunning": 3, "maxDepth": 1, "profiles": [
        { "id": "claude", "agentId": "claude-acp", "enabled": true, "model": null, "mode": null, "configOptions": {}, "approvals": "ask", "description": null, "agentName": "Claude Agent", "implicit": true },
        { "id": "codex", "agentId": "codex-acp", "enabled": false, "model": "gpt-5", "mode": null, "configOptions": {}, "approvals": "allowAll", "description": "Fast", "agentName": "Codex", "implicit": false },
        { "id": "gone", "agentId": "old-acp", "enabled": true, "model": null, "mode": null, "configOptions": {}, "approvals": "ask", "description": null, "agentName": null, "implicit": false }
    ] })
}

#[test]
fn ids_follow_the_host_rule() {
    let all = profiles_of(&view());
    assert_eq!(id_error("", &all), Some("Enter a name."));
    assert_eq!(id_error("a b", &all), Some("Use letters, digits, and . _ : - only."));
    assert_eq!(id_error("CODEX", &all), Some("Another profile has this name."));
    assert_eq!(id_error(&"x".repeat(65), &all), Some("Use 64 characters or fewer."));
    assert_eq!(id_error("codex-fast", &all), None);
    assert_eq!(unique_id("codex", &all), "codex-2");
    assert_eq!(unique_id("codex-7", &all), "codex-2");
    assert_eq!(agent_short_name("acp-registry:claude-acp"), "claude");
    assert_eq!(agent_short_name("custom:My Agent!!x"), "my-agent-x");
}

#[test]
fn an_agents_only_profile_stays_and_a_gone_agents_profile_can_go() {
    let all = profiles_of(&view());
    assert!(!can_delete(&all[0], &all));
    assert!(!can_delete(&all[1], &all), "codex's only stored profile");
    assert!(can_delete(&all[2], &all));
    assert_eq!(agent_lock_reason(&all[1], &all), Some("This agent's only profile stays on it. Duplicate it to use another agent."));
    assert!(skips_prompts(&all[1]) && !skips_prompts(&all[0]));
    assert!(skips_prompts(&obj(json!({ "approvals": "ask", "mode": "bypassPermissions" }))));
}

#[test]
fn the_first_stored_profile_of_an_agent_stores_its_implicit_default_too() {
    let all = profiles_of(&view());
    let copy = fresh("claude-2", "claude-acp");
    let upserts = profile_upserts(&all, &copy, None);
    assert_eq!(upserts.len(), 2);
    assert_eq!(upserts[0]["profile"]["id"], json!("claude"));
    assert!(upserts[0]["profile"].get("implicit").is_none());
    let next = apply_input(&view(), &json!({ "upsertProfiles": upserts, "maxRunning": 4 }));
    let ids: Vec<_> = profiles_of(&next).iter().map(|p| p["id"].clone()).collect();
    assert_eq!(ids, [json!("claude"), json!("codex"), json!("gone"), json!("claude-2")]);
    assert_eq!(next["maxRunning"], json!(4));
    assert_eq!(profiles_of(&next)[3]["agentName"], json!("Claude Agent"));
    let renamed = apply_input(&next, &json!({ "upsertProfiles": [{ "previousId": "claude-2", "profile": fresh("fast", "claude-acp") }], "deleteProfiles": ["gone"] }));
    let ids: Vec<_> = profiles_of(&renamed).iter().map(|p| p["id"].clone()).collect();
    assert_eq!(ids, [json!("claude"), json!("codex"), json!("fast")]);
}

#[test]
fn the_footnote_lists_the_profiles_the_caller_sees() {
    let all = profiles_of(&view());
    assert_eq!(footnote(&all), "The calling agent sees claude with their notes. Off profiles are hidden from it.");
    assert_eq!(footnote(&[]), "Every profile is off, so the calling agent sees none.");
    assert_eq!(subtitle(&all[2], "old-acp"), "old-acp is not installed. Its profile cannot start.");
    assert_eq!(subtitle(&all[0], "Claude Agent"), "Claude Agent. No note yet.");
}
