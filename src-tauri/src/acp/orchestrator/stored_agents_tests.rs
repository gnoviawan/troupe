use super::*;
use serde_json::json;

#[test]
fn lenient_entries_and_keys() {
    let configs = parse(json!({"_version":1,"data":[
        {"id":"old","configId":" acp-registry:gemini ","name":"Gemini","command":"gemini","templateId":"extra","future":true},
        {"id":"codex-acp","configId":" ","name":"Codex","command":"codex"},
        {"id":"codex-acp","name":"Codex copy","command":"codex"},
        {"id":"bad"}
    ]})).unwrap();
    assert_eq!(
        configs
            .iter()
            .map(|c| c.config_id.as_deref().unwrap())
            .collect::<Vec<_>>(),
        vec!["acp-registry:gemini", "codex-acp"]
    );
    assert_eq!(configs[1].name, "Codex");
}

#[test]
fn missing_and_empty_are_distinct() {
    assert!(read(None).is_none());
    assert!(parse(json!({})).is_none());
    assert!(parse(json!({"_version":1,"data":[]})).unwrap().is_empty());
}

#[tokio::test]
async fn composer_union_updates_without_restart() {
    use crate::web::store::StorePolicy;
    use std::sync::Arc;
    let config = |id: &str| AgentConfig {
        config_id: Some(format!("acp-registry:{id}")),
        name: id.into(),
        command: id.into(),
        args: vec![],
        env: HashMap::new(),
        allow_terminal: false,
    };
    let (mut engine, dir) = crate::acp::orchestrator::engine_with_installed(
        &[config("codex-acp"), config("oh-my-pi")],
        &["claude-acp"],
    )
    .await;
    let store =
        Arc::new(WebStore::open(dir.path().join("composer.json"), StorePolicy::default()).unwrap());
    Arc::get_mut(&mut engine).unwrap().store = Some(store.clone());
    assert_eq!(
        engine
            .launchable()
            .agents
            .iter()
            .map(|a| a.key.as_str())
            .collect::<Vec<_>>(),
        vec![
            "acp-registry:claude-acp",
            "acp-registry:codex-acp",
            "acp-registry:oh-my-pi"
        ]
    );
    let write = |data| {
        store
            .replace_as_host(
                "shared/lww/acp/agents",
                json!({"_version":1,"data":data}),
                1,
            )
            .unwrap()
    };
    write(json!([
        {"id":"acp-registry:codex-acp","name":"Codex","command":"codex"},
        {"id":"acp-registry:gemini","name":"Gemini","command":"gemini"}
    ]));
    assert_eq!(
        engine
            .launchable()
            .agents
            .iter()
            .map(|a| a.key.as_str())
            .collect::<Vec<_>>(),
        vec![
            "acp-registry:claude-acp",
            "acp-registry:codex-acp",
            "acp-registry:gemini"
        ]
    );
    assert!(engine.resolve_agent("acp-registry:gemini").is_ok());
    assert!(engine.resolve_agent("acp-registry:oh-my-pi").is_err());
    write(json!([]));
    assert_eq!(
        engine
            .launchable()
            .agents
            .iter()
            .map(|a| a.key.as_str())
            .collect::<Vec<_>>(),
        vec!["acp-registry:claude-acp"]
    );
}
