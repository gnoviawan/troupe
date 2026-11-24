//! Model switching for an agent that advertises its models only through the
//! legacy `models` field (Hermes Agent, issue #822): `set_model` reaches the
//! agent as a raw `session/set_model` request, and an agent that rejects it
//! surfaces its own error. Drives the production command loop through a
//! scripted mock agent shaped like `hermes acp`.

use std::sync::Arc;

use super::*;

const MOCK_LEGACY_MODELS: &str = r#"
import json, sys

def send(obj):
    sys.stdout.write(json.dumps(obj) + "\n")
    sys.stdout.flush()

current = "openrouter:anthropic/claude-sonnet-4"
for line in sys.stdin:
    line = line.strip()
    if not line:
        continue
    msg = json.loads(line)
    method, rid = msg.get("method"), msg.get("id")
    if method == "initialize":
        send({"jsonrpc": "2.0", "id": rid, "result": {
            "protocolVersion": 1, "agentCapabilities": {}, "authMethods": []}})
    elif method == "session/new":
        send({"jsonrpc": "2.0", "id": rid, "result": {
            "sessionId": "sess-hermes",
            "models": {"currentModelId": current, "availableModels": [
                {"modelId": "openrouter:anthropic/claude-sonnet-4", "name": "anthropic/claude-sonnet-4"},
                {"modelId": "openrouter:openai/gpt-5", "name": "openai/gpt-5"}]},
            "modes": {"currentModeId": "default", "availableModes": [
                {"id": "default", "name": "Default"}]}}})
    elif method == "session/set_model":
        params = msg.get("params", {})
        model = params.get("modelId")
        if params.get("sessionId") != "sess-hermes" or model == "unknown:model":
            send({"jsonrpc": "2.0", "id": rid, "error": {
                "code": -32602, "message": "unknown model " + str(model)}})
        else:
            current = model
            send({"jsonrpc": "2.0", "id": rid, "result": {}})
    elif method == "session/set_config_option":
        send({"jsonrpc": "2.0", "id": rid, "error": {
            "code": -32601, "message": "Method not found"}})
"#;

async fn spawn_mock(acp: &Arc<AcpManager>, root: &std::path::Path) -> AgentId {
    let python = super::real_loop_tests::find_python()
        .expect("python3 on PATH (set TERMUL_SKIP_REAL_LOOP_TESTS=1 to skip)");
    let script = root.join("agent.py");
    std::fs::write(&script, MOCK_LEGACY_MODELS).expect("script");
    acp.spawn(AgentConfig {
        config_id: None,
        name: "hermes-mock".to_string(),
        command: python.to_string(),
        args: vec!["-u".to_string(), script.to_string_lossy().into_owned()],
        env: HashMap::new(),
        allow_terminal: false,
    })
    .await
    .expect("mock agent initializes")
    .agent_id
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_legacy_model_list_switches_through_session_set_model() {
    if super::real_loop_tests::find_python().is_none()
        && std::env::var(super::real_loop_tests::SKIP_ENV).as_deref() == Ok("1")
    {
        return;
    }
    let _serial = super::real_loop_tests::TIMEOUT_TEST_LOCK.lock().await;
    let root = tempfile::tempdir().expect("tempdir");
    let acp = Arc::new(AcpManager::new(vec![]));
    let agent = spawn_mock(&acp, root.path()).await;
    let created = acp
        .new_session_with_context(
            &agent,
            root.path().to_string_lossy().into_owned(),
            vec![],
            SessionCreationContext::default(),
        )
        .await
        .expect("session");
    let models = created.models.expect("legacy models reach the picker");
    assert_eq!(models.available_models.len(), 2);

    acp.set_model(
        &agent,
        created.session_id.clone(),
        "openrouter:openai/gpt-5".to_string(),
    )
    .await
    .expect("the legacy switch is accepted");

    let error = acp
        .set_model(
            &agent,
            created.session_id.clone(),
            "unknown:model".to_string(),
        )
        .await
        .expect_err("the agent's rejection surfaces");
    assert!(error.contains("unknown model"), "{error}");
    let _ = acp.kill(&agent).await;
}
