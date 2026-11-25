use serde_json::{Value, json};

use super::State;
use super::queries::{
    config_id_from_reuse_key, has_model_relevant_options_cache, is_background_work_block,
    is_resume_failure_error, last_own_user_prompt_blocks,
};
use super::types::ChatMessage;

fn state(sessions: Value, session_index: Value) -> State {
    let mut value = serde_json::to_value(State::default()).expect("a state");
    value["sessions"] = sessions;
    value["sessionIndex"] = session_index;
    serde_json::from_value(value).expect("a state")
}

fn session(id: &str, project_id: Value) -> Value {
    json!({
        "id": id,
        "agentId": "agent-1",
        "status": "active",
        "title": null,
        "activeTurn": false,
        "openTurnId": null,
        "projectId": project_id,
    })
}

fn message(id: &str, role: &str, text: &str) -> ChatMessage {
    serde_json::from_value(json!({
        "id": id,
        "role": role,
        "blocks": [{ "type": "text", "text": text }],
        "streaming": false,
        "timestamp": 1,
    }))
    .expect("a message")
}

#[test]
fn only_the_two_background_reasons_are_background_work() {
    let cases = [
        (
            Some("background subagents are still running — switching would stop them"),
            true,
        ),
        (
            Some("commands are still running in the background — switching would stop them"),
            true,
        ),
        (Some("a turn is still running"), false),
        (None, false),
    ];
    for (reason, expected) in cases {
        assert_eq!(is_background_work_block(reason), expected, "{reason:?}");
    }
}

#[test]
fn a_resume_failure_is_told_from_a_turn_note() {
    assert!(is_resume_failure_error(Some(
        "Resume failed: session not found"
    )));
    assert!(!is_resume_failure_error(Some(
        "Interrupted by server restart."
    )));
    assert!(!is_resume_failure_error(Some("resume failed: lower case")));
    assert!(!is_resume_failure_error(None));
}

#[test]
fn model_relevant_cache_needs_models_or_a_model_option_with_choices() {
    let cases = [
        (None, false),
        (Some(json!({ "configOptions": [] })), false),
        (
            Some(json!({ "models": { "availableModels": [] }, "configOptions": [] })),
            false,
        ),
        (
            Some(
                json!({ "models": { "availableModels": [{ "modelId": "m1" }] }, "configOptions": [] }),
            ),
            true,
        ),
        (
            Some(json!({ "configOptions": [{ "category": "model", "options": [] }] })),
            false,
        ),
        (
            Some(
                json!({ "configOptions": [{ "category": "mode", "options": [{ "value": "a" }] }] }),
            ),
            false,
        ),
        (
            Some(
                json!({ "configOptions": [{ "category": "model", "options": [{ "value": "a" }] }] }),
            ),
            true,
        ),
    ];
    for (entry, expected) in cases {
        assert_eq!(
            has_model_relevant_options_cache(entry.as_ref()),
            expected,
            "{entry:?}"
        );
    }
}

#[test]
fn the_config_id_is_the_first_field_of_a_reuse_key() {
    assert_eq!(
        config_id_from_reuse_key("gemini\u{0}/work\u{0}project\u{0}p1"),
        "gemini"
    );
    assert_eq!(config_id_from_reuse_key("codex"), "codex");
}

#[test]
fn the_resend_skips_spliced_history_and_agent_messages() {
    let messages = [
        message("u1", "user", "first"),
        message("u2", "user", "second"),
        message("switch-splice:s0:u0", "user", "old agent's prompt"),
        message("a1", "agent", "reply"),
    ];
    let blocks = last_own_user_prompt_blocks(&messages).expect("an own prompt");
    assert_eq!(blocks, [json!({ "type": "text", "text": "second" })]);
    assert_eq!(last_own_user_prompt_blocks(&messages[2..]), None);
}

#[test]
fn a_session_project_reads_the_live_record_then_the_index() {
    let state = state(
        json!({
            "live": session("live", json!("p1")),
            "stub": session("stub", json!("")),
            "quick": session("quick", json!("__unassigned__")),
        }),
        json!([
            { "id": "stub", "projectId": "p2" },
            { "id": "closed", "projectId": "p3" },
            { "id": "loose", "projectId": "__unassigned__" },
        ]),
    );
    let cases = [
        ("live", Some("p1")),
        ("stub", Some("p2")),
        ("closed", Some("p3")),
        ("quick", None),
        ("loose", None),
        ("missing", None),
    ];
    let found: Vec<_> = cases
        .iter()
        .map(|(id, _)| (*id, state.session_project_id(id)))
        .collect();
    assert_eq!(found, cases);
}

#[test]
fn ephemeral_sessions_auth_flights_and_mcp_deletions_read_the_book() {
    let mut state = State::default();
    assert!(!state.is_ephemeral_session("w1"));
    assert!(!state.is_any_agent_auth_in_flight());
    assert_eq!(state.mcp_deletions("docs"), 0);

    state.book.ephemeral_sessions.insert("w1".into());
    state
        .book
        .auth_flights
        .insert("agent-1\u{0}oauth".into(), 1);
    state.book.mcp.deletions.insert("docs".into(), 2);

    assert!(state.is_ephemeral_session("w1"));
    assert!(!state.is_ephemeral_session("w2"));
    assert!(state.is_any_agent_auth_in_flight());
    assert_eq!(state.mcp_deletions("docs"), 2);
}
