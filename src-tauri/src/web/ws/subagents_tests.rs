#![allow(clippy::unwrap_used, clippy::expect_used)]
//! `stop_subagent` maps each refusal to a stable code. The success path, a
//! cancel that reaches the child session, is covered end to end by
//! `acp::manager::native_subagents_loop_tests`.

use super::*;

async fn stop(payload: Value) -> WsReply {
    let acp = Arc::new(AcpManager::new(vec![]));
    handle_stop_subagent("s1".to_string(), &payload, &acp).await
}

#[tokio::test]
async fn an_unknown_agent_is_not_found() {
    let reply = stop(json!({
        "agentId": "agent-gone",
        "rootSessionId": "chat-1",
        "subagentId": "child-1",
    }))
    .await;
    let error = reply.err.expect("refused");
    assert_eq!(error.code, "not_found");
    assert_eq!(error.message, "The agent of this chat is not running");
}

#[tokio::test]
async fn a_payload_without_the_subagent_is_refused() {
    let reply = stop(json!({ "agentId": "agent-1", "rootSessionId": "chat-1" })).await;
    let error = reply.err.expect("refused");
    assert_eq!(error.code, "unsupported");
    assert!(error
        .message
        .contains("want agentId, rootSessionId, subagentId"));
}

fn settings_patch(payload: Value) -> SubagentSettingsPayload {
    serde_json::from_value(payload).expect("the payload parses")
}

#[test]
fn a_settings_set_with_an_unknown_field_is_a_validation_error() {
    let reply = handle_subagent_settings_set("s1".to_string(), &json!({ "maxRuning": 3 }));
    let error = reply.err.expect("refused");
    assert_eq!(error.code, "VALIDATION_ERROR");
}

/// An engine whose registry lists Codex, for the settings view.
fn codex_engine(data: &std::path::Path) -> OrchestratorEngine {
    let codex = crate::acp::config::AgentConfig {
        config_id: Some("acp-registry:codex-acp".to_string()),
        name: "Codex".to_string(),
        command: "codex-acp".to_string(),
        args: Vec::new(),
        env: std::collections::HashMap::new(),
        allow_terminal: false,
    };
    std::fs::write(
        data.join(crate::automation::agents::FILE_NAME),
        json!({"schema_version": 1, "agents": {"acp-registry:codex-acp": codex}}).to_string(),
    )
    .unwrap();
    OrchestratorEngine::new(
        Arc::new(AcpManager::new(vec![])),
        data.to_path_buf(),
        None,
        tokio::runtime::Handle::current(),
    )
}

fn set(engine: &OrchestratorEngine, payload: Value) -> Result<OrchestratorSettings, String> {
    apply_settings_patch(engine, settings_patch(payload))
}

const CODEX: &str = "acp-registry:codex-acp";

/// The view of an agent's untouched default profile.
fn implicit_codex() -> Value {
    json!({
        "id": CODEX,
        "agentId": CODEX,
        "agentName": "Codex",
        "enabled": true,
        "model": null,
        "mode": null,
        "configOptions": {},
        "approvals": "ask",
        "description": null,
        "implicit": true,
    })
}

#[tokio::test]
async fn the_view_lists_the_implicit_default_and_its_legacy_row() {
    let data = tempfile::tempdir().unwrap();
    let engine = codex_engine(data.path());
    assert_eq!(
        settings_view(&engine),
        json!({
            "enabled": true,
            "maxRunning": 4,
            "maxDepth": 2,
            "agents": [{
                "agentId": CODEX,
                "name": "Codex",
                "allowed": true,
                "model": null,
                "description": null,
            }],
            "profiles": [implicit_codex()],
        })
    );
}

#[tokio::test]
async fn a_legacy_patch_keeps_every_omitted_field() {
    let data = tempfile::tempdir().unwrap();
    let engine = codex_engine(data.path());
    set(&engine, json!({ "agents": [{ "agentId": CODEX, "allowed": false }] })).unwrap();
    set(&engine, json!({ "maxDepth": 3 })).unwrap();
    let view = settings_view(&engine);
    assert_eq!(view["enabled"], json!(true));
    assert_eq!(view["maxRunning"], json!(4));
    assert_eq!(view["maxDepth"], json!(3));
    assert_eq!(view["agents"][0]["allowed"], json!(false));
    assert_eq!(view["profiles"][0]["enabled"], json!(false));
    assert_eq!(view["profiles"][0]["implicit"], json!(false));

    set(&engine, json!({ "enabled": false, "agents": [{ "agentId": CODEX, "allowed": true }] }))
        .unwrap();
    let view = settings_view(&engine);
    assert_eq!(view["enabled"], json!(false));
    assert_eq!(view["agents"][0]["allowed"], json!(true));
}

#[tokio::test]
async fn a_default_model_set_over_the_wire_reads_back_in_the_view() {
    let data = tempfile::tempdir().unwrap();
    let engine = codex_engine(data.path());

    set(&engine, json!({ "models": [{ "agentId": CODEX, "model": "gpt-5.1-mini" }] })).unwrap();
    assert_eq!(
        settings_view(&engine)["agents"],
        json!([{
            "agentId": CODEX,
            "name": "Codex",
            "allowed": true,
            "model": "gpt-5.1-mini",
            "description": null,
        }])
    );
    assert_eq!(
        settings_view(&engine)["profiles"][0]["model"],
        json!("gpt-5.1-mini")
    );

    set(&engine, json!({ "models": [{ "agentId": CODEX, "model": null }] })).unwrap();
    assert_eq!(settings_view(&engine)["agents"][0]["model"], Value::Null);

    let error = set(&engine, json!({ "models": [{ "agentId": CODEX, "model": " " }] }))
        .unwrap_err();
    assert_eq!(error, "a default model must be 1 to 512 characters");
}

#[tokio::test]
async fn a_padded_agent_key_updates_and_clears_the_saved_default_model() {
    let data = tempfile::tempdir().unwrap();
    let engine = codex_engine(data.path());

    set(&engine, json!({ "models": [{ "agentId": CODEX, "model": "fast" }] })).unwrap();
    set(&engine, json!({ "models": [{ "agentId": " acp-registry:codex-acp ", "model": "deep" }] }))
        .unwrap();
    assert_eq!(settings_view(&engine)["agents"][0]["model"], json!("deep"));
    assert_eq!(
        settings_view(&engine)["profiles"].as_array().unwrap().len(),
        1
    );

    set(&engine, json!({ "models": [{ "agentId": " acp-registry:codex-acp ", "model": null }] }))
        .unwrap();
    assert_eq!(settings_view(&engine)["agents"][0]["model"], Value::Null);
}

#[tokio::test]
async fn a_description_set_over_the_wire_reads_back_trimmed_and_clears() {
    let data = tempfile::tempdir().unwrap();
    let engine = codex_engine(data.path());
    let description =
        |engine: &OrchestratorEngine| settings_view(engine)["agents"][0]["description"].clone();

    set(&engine, json!({ "descriptions": [{
        "agentId": " acp-registry:codex-acp ",
        "description": "  Fast at UI tweaks; weak at Rust.\n",
    }] }))
    .unwrap();
    assert_eq!(
        description(&engine),
        json!("Fast at UI tweaks; weak at Rust.")
    );

    set(&engine, json!({ "descriptions": [{ "agentId": CODEX, "description": "  " }] })).unwrap();
    assert_eq!(description(&engine), Value::Null);

    set(&engine, json!({ "descriptions": [{ "agentId": CODEX, "description": "Docs" }] })).unwrap();
    set(&engine, json!({ "descriptions": [{ "agentId": CODEX, "description": null }] })).unwrap();
    assert_eq!(description(&engine), Value::Null);

    let error = set(&engine, json!({ "descriptions": [{
        "agentId": CODEX,
        "description": "d".repeat(1001),
    }] }))
    .unwrap_err();
    assert_eq!(error, "a description must be 1 to 1000 characters");
}

#[tokio::test]
async fn a_bare_installed_id_saves_under_the_listed_key() {
    let (engine, _data) =
        crate::acp::orchestrator::engine_with_installed(&[], &["opencode"]).await;
    apply_settings_patch(
        &engine,
        settings_patch(json!({
            "agents": [{ "agentId": "opencode", "allowed": false }],
            "models": [{ "agentId": " opencode ", "model": "big" }],
            "descriptions": [{ "agentId": "opencode", "description": "Good at long refactors." }],
        })),
    )
    .unwrap();
    assert_eq!(
        settings_view(&engine)["agents"],
        json!([{
            "agentId": "acp-registry:opencode",
            "name": "opencode",
            "allowed": false,
            "model": "big",
            "description": "Good at long refactors.",
        }])
    );
}

#[tokio::test]
async fn an_upserted_profile_reads_back_whole_after_the_agents_default() {
    let data = tempfile::tempdir().unwrap();
    let engine = codex_engine(data.path());
    set(&engine, json!({ "upsertProfiles": [{ "profile": {
        "id": " codex-fast ",
        "agentId": CODEX,
        "enabled": true,
        "model": "gpt-5.5-mini",
        "mode": "full-access",
        "configOptions": { "reasoning_effort": "low" },
        "approvals": "allowAll",
        "description": " Quick fixes. ",
    } }] }))
    .unwrap();
    let view = settings_view(&engine);
    assert_eq!(
        view["profiles"],
        json!([
            {
                "id": "codex-fast",
                "agentId": CODEX,
                "agentName": "Codex",
                "enabled": true,
                "model": "gpt-5.5-mini",
                "mode": "full-access",
                "configOptions": { "reasoning_effort": "low" },
                "approvals": "allowAll",
                "description": "Quick fixes.",
                "implicit": false,
            },
        ]),
        "a stored profile of the agent replaces its implicit default"
    );
    assert_eq!(view["agents"], json!([]), "no default profile, no legacy row");

    set(&engine, json!({ "upsertProfiles": [{ "profile": {
        "id": "codex-fast",
        "agentId": CODEX,
        "enabled": false,
    } }] }))
    .unwrap();
    assert_eq!(
        settings_view(&engine)["profiles"][0],
        json!({
            "id": "codex-fast",
            "agentId": CODEX,
            "agentName": "Codex",
            "enabled": false,
            "model": null,
            "mode": null,
            "configOptions": {},
            "approvals": "ask",
            "description": null,
            "implicit": false,
        }),
        "an upsert without previousId replaces the profile of the same id"
    );
}

#[tokio::test]
async fn previous_id_renames_a_stored_or_implicit_profile_and_must_exist() {
    let data = tempfile::tempdir().unwrap();
    let engine = codex_engine(data.path());
    let profile = |id: &str| json!({ "id": id, "agentId": CODEX, "enabled": true });

    set(&engine, json!({ "upsertProfiles": [
        { "previousId": CODEX, "profile": profile("codex-a") },
    ] }))
    .unwrap();
    set(&engine, json!({ "upsertProfiles": [
        { "previousId": "codex-a", "profile": profile("codex-b") },
    ] }))
    .unwrap();
    let ids: Vec<Value> = settings_view(&engine)["profiles"]
        .as_array()
        .unwrap()
        .iter()
        .map(|profile| profile["id"].clone())
        .collect();
    assert_eq!(ids, [json!("codex-b")]);

    let error = set(&engine, json!({ "upsertProfiles": [
        { "previousId": "codex-gone", "profile": profile("codex-c") },
    ] }))
    .unwrap_err();
    assert_eq!(error, "no sub-agent profile is named codex-gone");
}

#[tokio::test]
async fn deleting_profiles_is_idempotent_and_brings_the_implicit_default_back() {
    let data = tempfile::tempdir().unwrap();
    let engine = codex_engine(data.path());
    set(&engine, json!({ "upsertProfiles": [
        { "profile": { "id": "codex-a", "agentId": CODEX, "enabled": true } },
        { "profile": { "id": "codex-b", "agentId": CODEX, "enabled": true } },
    ] }))
    .unwrap();
    set(&engine, json!({ "deleteProfiles": ["codex-a", "codex-a", "never-was"] })).unwrap();
    assert_eq!(settings_view(&engine)["profiles"][0]["id"], json!("codex-b"));
    assert_eq!(settings_view(&engine)["profiles"].as_array().unwrap().len(), 1);

    set(&engine, json!({ "deleteProfiles": ["codex-b"] })).unwrap();
    assert_eq!(settings_view(&engine)["profiles"], json!([implicit_codex()]));
}

#[tokio::test]
async fn an_upsert_names_its_agent_by_the_listed_key() {
    let (engine, _data) =
        crate::acp::orchestrator::engine_with_installed(&[], &["opencode"]).await;
    set(&engine, json!({ "upsertProfiles": [{ "profile": {
        "id": "oc-big",
        "agentId": " opencode ",
        "enabled": true,
    } }] }))
    .unwrap();
    let view = settings_view(&engine);
    assert_eq!(view["profiles"][0]["agentId"], json!("acp-registry:opencode"));
    assert_eq!(view["profiles"][0]["agentName"], json!("opencode"));
}

#[tokio::test]
async fn an_upsert_refuses_a_taken_or_malformed_id() {
    let (engine, _data) =
        crate::acp::orchestrator::engine_with_installed(&[], &["codex", "opencode"]).await;
    set(&engine, json!({ "upsertProfiles": [{ "profile": {
        "id": "Fast", "agentId": "acp-registry:codex", "enabled": true,
    } }] }))
    .unwrap();

    let error = set(&engine, json!({ "upsertProfiles": [{ "profile": {
        "id": "fast", "agentId": "acp-registry:opencode", "enabled": true,
    } }] }))
    .unwrap_err();
    assert_eq!(error, "two sub-agent profiles are named fast");

    let error = set(&engine, json!({ "upsertProfiles": [{ "profile": {
        "id": "acp-registry:opencode", "agentId": "acp-registry:codex", "enabled": true,
    } }] }))
    .unwrap_err();
    assert_eq!(
        error,
        "the profile id acp-registry:opencode is the id of the agent opencode"
    );

    let error = set(&engine, json!({ "upsertProfiles": [{ "profile": {
        "id": "pi fast", "agentId": "acp-registry:codex", "enabled": true,
    } }] }))
    .unwrap_err();
    assert_eq!(
        error,
        "a profile id must be 1 to 64 letters, digits, '.', '_', ':' or '-'"
    );

    let unknown_approvals = json!({ "upsertProfiles": [{ "profile": {
        "id": "codex-x", "agentId": "acp-registry:codex", "enabled": true, "approvals": "always",
    } }] });
    assert!(
        serde_json::from_value::<SubagentSettingsPayload>(unknown_approvals).is_err(),
        "an unknown approvals value never parses"
    );
}
