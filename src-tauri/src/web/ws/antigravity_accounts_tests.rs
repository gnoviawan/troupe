use super::*;
use crate::acp::AntigravityAgentService;

fn manager_with_account(signed_out: &[&str]) -> (tempfile::TempDir, Arc<AcpManager>) {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("antigravity-accounts.json"),
        br#"{"version":1,"accounts":[{"id":"default","name":"Default","createdAt":0},{"id":"ab12","name":"Work","createdAt":1}],"defaultAccountId":"default","projectAccounts":{},"autoSwitch":true}"#,
    )
    .unwrap();
    std::fs::create_dir_all(root.path().join("antigravity-accounts").join("ab12")).unwrap();
    let service = AntigravityAgentService::with_logins_for_test(root.path(), signed_out);
    let acp = Arc::new(
        AcpManager::with_host_agents(
            vec![],
            Arc::new(crate::acp::ClaudeAgentService::without_host_state()),
            Arc::new(crate::acp::CodexAgentService::without_host_state()),
        )
        .with_antigravity_agent(Arc::new(service)),
    );
    (root, acp)
}

fn stored(root: &tempfile::TempDir) -> Value {
    serde_json::from_slice(&std::fs::read(root.path().join("antigravity-accounts.json")).unwrap())
        .unwrap()
}

#[tokio::test]
async fn set_default_and_project_account_persist_on_the_host() {
    let (root, acp) = manager_with_account(&[]);
    let reply =
        handle_set_default_antigravity_account("1".into(), &json!({ "accountId": "ab12" }), &acp)
            .await;
    assert!(reply.ok);
    let reply = handle_set_project_antigravity_account(
        "2".into(),
        &json!({ "projectId": "office", "accountId": "ab12" }),
        &acp,
    )
    .await;
    assert!(reply.ok);
    let file = stored(&root);
    assert_eq!(file["defaultAccountId"], "ab12");
    assert_eq!(file["projectAccounts"]["office"], "ab12");

    let reply = handle_set_project_antigravity_account(
        "3".into(),
        &json!({ "projectId": "office", "accountId": null }),
        &acp,
    )
    .await;
    assert!(reply.ok);
    assert!(stored(&root)["projectAccounts"].get("office").is_none());
}

#[tokio::test]
async fn an_expected_account_makes_the_project_choice_a_compare_and_set() {
    let (root, acp) = manager_with_account(&[]);
    let first = handle_set_project_antigravity_account(
        "1".into(),
        &json!({ "projectId": "office", "accountId": "ab12", "expectedAccountId": "default" }),
        &acp,
    )
    .await;
    assert_eq!(first.payload.unwrap(), json!({ "applied": true }));
    let second = handle_set_project_antigravity_account(
        "2".into(),
        &json!({ "projectId": "office", "accountId": null, "expectedAccountId": "default" }),
        &acp,
    )
    .await;
    assert_eq!(second.payload.unwrap(), json!({ "applied": false }));
    assert_eq!(stored(&root)["projectAccounts"]["office"], "ab12");
}

#[tokio::test]
async fn a_chat_choice_is_a_compare_and_set_and_remove_clears_it() {
    let (root, acp) = manager_with_account(&[]);
    let lost = handle_set_chat_antigravity_account(
        "1".into(),
        &json!({
            "projectId": "office",
            "sessionId": "chat-1",
            "accountId": "ab12",
            "expectedAccountId": "ab12"
        }),
        &acp,
    )
    .await;
    assert_eq!(lost.payload.unwrap(), json!({ "applied": false }));
    let applied = handle_set_chat_antigravity_account(
        "2".into(),
        &json!({
            "projectId": "office",
            "sessionId": "chat-1",
            "accountId": "ab12",
            "expectedAccountId": "default"
        }),
        &acp,
    )
    .await;
    assert_eq!(applied.payload.unwrap(), json!({ "applied": true }));
    assert_eq!(stored(&root)["chatAccounts"]["chat-1"], "ab12");
    let removed =
        handle_remove_antigravity_account("3".into(), &json!({ "accountId": "ab12" }), &acp).await;
    assert_eq!(removed.payload.unwrap()["movedChatIds"], json!(["chat-1"]));
    assert!(stored(&root)
        .get("chatAccounts")
        .is_none_or(|chats| chats.get("chat-1").is_none()));
}

#[tokio::test]
async fn automatic_switching_is_on_by_default_and_toggles_on_the_host() {
    let (root, acp) = manager_with_account(&[]);
    let listed = handle_list_antigravity_accounts("1".into(), &acp).await;
    assert_eq!(listed.payload.unwrap()["autoSwitch"], true);
    let reply =
        handle_set_antigravity_auto_switch("2".into(), &json!({ "enabled": false }), &acp).await;
    assert_eq!(reply.payload.unwrap(), json!({}));
    assert_eq!(stored(&root)["autoSwitch"], false);
    let listed = handle_list_antigravity_accounts("3".into(), &acp).await;
    assert_eq!(listed.payload.unwrap()["autoSwitch"], false);
    let malformed = handle_set_antigravity_auto_switch("4".into(), &json!({}), &acp).await;
    assert_eq!(malformed.err.unwrap().code, "unsupported");
}

#[tokio::test]
async fn the_list_fills_signed_in_and_hides_secrets() {
    let (_root, acp) = manager_with_account(&["ab12"]);
    let listed = handle_list_antigravity_accounts("1".into(), &acp).await;
    let payload = listed.payload.unwrap();
    let text = payload.to_string();
    assert!(!text.to_ascii_lowercase().contains("token"));
    assert!(!text.contains("sk-"));
    assert_eq!(payload["accounts"][0]["signedIn"], true);
    assert_eq!(payload["accounts"][1]["id"], "ab12");
    assert_eq!(payload["accounts"][1]["signedIn"], false);
}

#[tokio::test]
async fn a_signed_out_account_is_refused_with_its_code_and_the_choice_stays() {
    let (root, acp) = manager_with_account(&["ab12"]);
    let reply = handle_set_project_antigravity_account(
        "1".into(),
        &json!({ "projectId": "office", "accountId": "ab12" }),
        &acp,
    )
    .await;
    let err = reply.err.unwrap();
    assert_eq!(err.code, "antigravity_account_signed_out");
    assert_eq!(
        err.message,
        "The Antigravity account \"Work\" is not signed in. Sign in to it in Settings → Provider, then choose it again."
    );
    let reply =
        handle_set_default_antigravity_account("2".into(), &json!({ "accountId": "ab12" }), &acp)
            .await;
    assert_eq!(reply.err.unwrap().code, "antigravity_account_signed_out");
    let file = stored(&root);
    assert_eq!(file["defaultAccountId"], "default");
    assert!(file["projectAccounts"].get("office").is_none());
}

#[tokio::test]
async fn add_rename_and_remove_round_trip() {
    let (root, acp) = manager_with_account(&[]);
    let added = handle_add_antigravity_account("1".into(), &json!({ "name": "Side" }), &acp).await;
    assert!(added.ok);
    let id = added.payload.unwrap()["id"].as_str().unwrap().to_string();
    let renamed = handle_rename_antigravity_account(
        "2".into(),
        &json!({ "accountId": id, "name": "Side 2" }),
        &acp,
    )
    .await;
    assert!(renamed.ok);
    handle_set_project_antigravity_account(
        "3".into(),
        &json!({ "projectId": "office", "accountId": id }),
        &acp,
    )
    .await;
    let removed =
        handle_remove_antigravity_account("4".into(), &json!({ "accountId": id }), &acp).await;
    assert_eq!(
        removed.payload.unwrap()["movedProjectIds"],
        json!(["office"])
    );
    let again =
        handle_remove_antigravity_account("5".into(), &json!({ "accountId": id }), &acp).await;
    assert_eq!(again.payload.unwrap()["movedProjectIds"], json!([]));
    let builtin =
        handle_remove_antigravity_account("6".into(), &json!({ "accountId": "default" }), &acp)
            .await;
    assert!(!builtin.ok);
    assert!(stored(&root)["accounts"]
        .as_array()
        .unwrap()
        .iter()
        .any(|row| row["id"] == "default"));
}

#[tokio::test]
async fn unknown_accounts_and_malformed_payloads_are_rejected() {
    let (_root, acp) = manager_with_account(&[]);
    let reply =
        handle_set_default_antigravity_account("1".into(), &json!({ "accountId": "ffff" }), &acp)
            .await;
    assert_eq!(reply.err.unwrap().code, "not_found");
    let reply =
        handle_set_project_antigravity_account("2".into(), &json!({ "accountId": "ab12" }), &acp)
            .await;
    assert_eq!(reply.err.unwrap().code, "unsupported");
    let reply = handle_start_antigravity_account_login(
        "3".into(),
        &json!({ "accountId": "nope" }),
        &acp,
        None,
    )
    .await;
    assert_eq!(reply.err.unwrap().code, "unsupported");
}

#[tokio::test]
async fn a_sign_in_spawn_refuses_a_non_antigravity_config_before_a_process_starts() {
    let (_root, acp) = manager_with_account(&[]);
    let reply = handle_start_antigravity_account_login(
        "1".into(),
        &json!({
            "accountId": "ab12",
            "config": {
                "configId": "acp-registry:claude-acp",
                "name": "Claude",
                "command": "node",
                "args": [],
                "env": {}
            }
        }),
        &acp,
        None,
    )
    .await;
    assert!(!reply.ok);
    assert!(reply.err.unwrap().message.contains("Antigravity agent"));
}

#[tokio::test]
async fn a_host_without_state_reports_no_accounts() {
    let acp = Arc::new(AcpManager::new(vec![]));
    let reply = handle_list_antigravity_accounts("1".into(), &acp).await;
    assert!(!reply.ok);
}

#[tokio::test]
async fn a_codex_account_list_is_not_the_antigravity_one() {
    let (_root, acp) = manager_with_account(&[]);
    // The Codex service of this host has no state; the Antigravity one does.
    assert!(!handle_list_codex_accounts("1".into(), &acp).await.ok);
    let listed = handle_list_antigravity_accounts("2".into(), &acp).await;
    assert!(listed.ok);
    assert_eq!(listed.payload.unwrap()["accounts"][1]["name"], "Work");
}

#[tokio::test]
async fn a_signed_out_spawn_error_maps_to_the_antigravity_code() {
    let reply = acp_err_to_reply(
        "1".into(),
        crate::acp::antigravity_accounts::signed_out_error(
            "The Antigravity account \"Work\" is not signed in.",
        ),
    );
    let err = reply.err.unwrap();
    assert_eq!(err.code, "antigravity_account_signed_out");
    assert_eq!(
        err.message,
        "The Antigravity account \"Work\" is not signed in."
    );
}

fn antigravity_config(config_id: &str) -> Value {
    json!({
        "configId": config_id,
        "name": "Antigravity",
        "command": "agy_acp_server_that_does_not_exist",
        "args": [],
        "env": {}
    })
}

fn record_tier(acp: &AcpManager, account_id: &str, tier: &str) {
    acp.antigravity_agent().record_session_tier(
        Some(crate::acp::antigravity_agent::AntigravityLaunchAuth::Account(account_id.to_string())),
        Some(tier.to_string()),
    );
}

#[tokio::test]
async fn listing_usage_never_starts_a_check() {
    let (_root, acp) = manager_with_account(&[]);
    record_tier(&acp, "ab12", "Google AI Pro");

    for n in 0..3 {
        assert!(
            handle_list_antigravity_accounts(n.to_string(), &acp)
                .await
                .ok
        );
        let reply = handle_list_antigravity_usage(n.to_string(), &acp);
        let usage = reply.payload.unwrap()["usage"].clone();
        assert_eq!(usage[0]["status"], "not_checked");
        assert_eq!(usage[1]["status"], "not_checked");
        assert_eq!(usage[1]["planType"], "Google AI Pro");
        assert_eq!(usage[1]["checkedAt"], 0);
    }
    for _ in 0..10 {
        tokio::task::yield_now().await;
    }
    assert_eq!(acp.antigravity_agent().usage_checks_started(), 0);
    assert!(acp.list_agents().is_empty());
}

#[tokio::test]
async fn a_usage_check_refuses_other_agents_and_unknown_accounts_before_a_process_starts() {
    let (_root, acp) = manager_with_account(&[]);

    let other = handle_check_antigravity_usage(
        "1".into(),
        &json!({ "accountId": "ab12", "config": antigravity_config("acp-registry:claude-acp") }),
        &acp,
    )
    .await;
    let unknown = handle_check_antigravity_usage(
        "2".into(),
        &json!({ "accountId": "ffff", "config": antigravity_config("acp-registry:antigravity-acp") }),
        &acp,
    )
    .await;
    let malformed =
        handle_check_antigravity_usage("3".into(), &json!({ "accountId": "ab12" }), &acp).await;

    assert_eq!(
        other.err.unwrap().message,
        "Usage checks run only on the Antigravity agent"
    );
    assert_eq!(
        unknown.err.unwrap().message,
        "That Antigravity account does not exist"
    );
    assert_eq!(malformed.err.unwrap().code, "unsupported");
    assert!(acp.list_agents().is_empty());
}

#[tokio::test]
async fn a_signed_out_check_is_a_message_on_that_check_without_a_process() {
    let (_root, acp) = manager_with_account(&["ab12"]);
    record_tier(&acp, "ab12", "Google AI Pro");

    let reply = handle_check_antigravity_usage(
        "1".into(),
        &json!({ "accountId": "ab12", "config": antigravity_config("acp-registry:antigravity-acp") }),
        &acp,
    )
    .await;

    let usage = reply.payload.unwrap()["usage"].clone();
    assert_eq!(usage["status"], "error");
    assert_eq!(
        usage["message"],
        "This account is not signed in. Sign in to it, then check again."
    );
    assert_eq!(usage["accountId"], "ab12");
    assert_eq!(usage["planType"], "Google AI Pro");
    assert!(acp.list_agents().is_empty());
    assert_eq!(acp.antigravity_agent().usage_checks_started(), 1);
    let listed = handle_list_antigravity_usage("2".into(), &acp);
    let listed = listed.payload.unwrap()["usage"][1].clone();
    assert_eq!(listed["status"], "error");
    assert_eq!(listed["planType"], "Google AI Pro");
}

#[tokio::test]
async fn starting_a_sign_in_drops_the_tier_and_the_last_check_of_that_account() {
    let (_root, acp) = manager_with_account(&["ab12"]);
    record_tier(&acp, "ab12", "Google AI Pro");
    record_tier(&acp, "default", "Google AI Ultra");
    let config = antigravity_config("acp-registry:antigravity-acp");
    handle_check_antigravity_usage(
        "1".into(),
        &json!({ "accountId": "ab12", "config": config }),
        &acp,
    )
    .await;

    handle_start_antigravity_account_login(
        "2".into(),
        &json!({ "accountId": "ab12", "config": config }),
        &acp,
        None,
    )
    .await;

    let listed = handle_list_antigravity_usage("3".into(), &acp);
    let usage = listed.payload.unwrap()["usage"].clone();
    assert_eq!(usage[1]["status"], "not_checked");
    assert_eq!(usage[1]["planType"], Value::Null);
    assert_eq!(usage[0]["planType"], "Google AI Ultra");
}
