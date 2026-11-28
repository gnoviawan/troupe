//! Consent and batch rules: nested clipboard reads, path identity, a policy
//! change during the consent wait, and partial batch results.

use super::*;

fn host_with(
    tickets: Arc<dyn ComuTicketPort>,
    policy: Arc<ComuPolicyService>,
) -> (Arc<DesktopComuHost>, Arc<FakeComuBackend>) {
    let backend = FakeComuBackend::new(sample_windows());
    let host = DesktopComuHost::with_backend(
        backend.clone(),
        vec![],
        tickets,
        policy,
        Arc::new(|_| "proj-1".to_string()),
        OWN_PID,
    );
    (host, backend)
}

fn policy_in(dir: &TempDir) -> Arc<ComuPolicyService> {
    Arc::new(ComuPolicyService::new(dir.path().to_path_buf(), vec![]))
}

#[tokio::test]
async fn batch_stop_on_error_keeps_the_steps_that_already_ran() {
    let dir = TempDir::new().unwrap();
    let tickets = ScriptedTickets::new([TicketOutcome::Answered(ConsentOption::AllowSession)]);
    let (host, backend) = host_with(tickets, policy_in(&dir));

    let out = host
        .execute(
            "s1",
            "a1",
            "comu_run_steps",
            json!({
                "window_id": 200,
                "steps": [
                    { "tool": "comu_get_screen_size", "args": {} },
                    { "tool": "comu_click", "args": { "x": 920.0, "y": 150.0, "simulate_error": true } },
                    { "tool": "comu_click", "args": { "x": 920.0, "y": 150.0 } }
                ]
            }),
        )
        .await;

    assert!(!out.ok);
    let summary = out.structured.expect("batch summary");
    assert_eq!(summary["status"], "partial");
    let steps = summary["steps"].as_array().unwrap();
    assert_eq!(steps.len(), 2, "the step after the failure does not run");
    assert_eq!(steps[0]["ok"], true);
    assert_eq!(steps[1]["ok"], false);
    assert_eq!(steps[1]["structured"]["refusal"]["code"], "step_failed");
    assert_eq!(backend.call_count(), 2);
}

#[tokio::test]
async fn clipboard_read_inside_a_batch_asks_for_clipboard_consent() {
    let dir = TempDir::new().unwrap();
    let tickets = ScriptedTickets::new([TicketOutcome::Answered(ConsentOption::Deny)]);
    let (host, backend) = host_with(tickets.clone(), policy_in(&dir));

    let out = host
        .execute(
            "s1",
            "a1",
            "comu_run_steps",
            json!({
                "steps": [{ "tool": "clipboard_read", "args": { "include_text": true } }]
            }),
        )
        .await;

    assert_eq!(refusal_code(&out), Some("consent_denied"));
    assert_eq!(tickets.prompt_count(), 1);
    assert_eq!(tickets.requests.lock()[0].app_id, "clipboard");
    assert_eq!(backend.call_count(), 0);
}

/// Turns Computer Use off while the consent card is open, then allows.
struct DisableWhilePrompting {
    policy: Arc<ComuPolicyService>,
}

impl ComuTicketPort for DisableWhilePrompting {
    fn request_comu(
        &self,
        _ticket: ComuTicketRequest,
        _wait: Duration,
    ) -> ConsentFuture<'_, TicketOutcome> {
        Box::pin(async move {
            let revision = self.policy.read_app_policy().await.revision;
            self.policy
                .apply_app(
                    &ComuCallerScope::App,
                    revision,
                    &json!({ "op": "set_enabled", "enabled": false }),
                )
                .await
                .unwrap();
            TicketOutcome::Answered(ConsentOption::AllowSession)
        })
    }

    fn cancel_comu_session(&self, _session_id: &str) {}
}

#[tokio::test]
async fn turning_computer_use_off_during_consent_refuses_the_call() {
    let dir = TempDir::new().unwrap();
    let policy = policy_in(&dir);
    let tickets = Arc::new(DisableWhilePrompting {
        policy: policy.clone(),
    });
    let (host, backend) = host_with(tickets, policy);

    let out = host
        .execute(
            "s1",
            "a1",
            "comu_click",
            json!({ "window_id": 200, "x": 920.0, "y": 150.0 }),
        )
        .await;

    assert_eq!(refusal_code(&out), Some("comu_disabled"));
    assert_eq!(backend.call_count(), 0);
}

/// `/opt/Editor` and `/opt/editor` can be different programs on a
/// case-sensitive filesystem, so a grant for one must not cover the other.
#[cfg(not(windows))]
#[tokio::test]
async fn case_distinct_executable_paths_are_separate_session_grants() {
    let dir = TempDir::new().unwrap();
    let tickets = ScriptedTickets::new([
        TicketOutcome::Answered(ConsentOption::AllowSession),
        TicketOutcome::Answered(ConsentOption::Deny),
    ]);
    let (host, _backend) = host_with(tickets.clone(), policy_in(&dir));

    let first = host
        .execute("s1", "a1", "comu_launch_app", json!({ "path": "/opt/Editor" }))
        .await;
    assert!(first.ok);
    let second = host
        .execute("s1", "a1", "comu_launch_app", json!({ "path": "/opt/editor" }))
        .await;
    assert_eq!(refusal_code(&second), Some("consent_denied"));
    assert_eq!(tickets.prompt_count(), 2);

    // The exact path reuses the session grant.
    let again = host
        .execute("s1", "a1", "comu_launch_app", json!({ "path": "/opt/Editor" }))
        .await;
    assert!(again.ok);
    assert_eq!(tickets.prompt_count(), 2);
}
