use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use serde_json::{json, Value};
use tempfile::TempDir;
use tokio::sync::Notify;
use troupe_comu_engine::{
    ComuCallOutput, ComuContentBlock, OsPermissionKind, OsPermissionsStatus, WindowOwnerInfo,
};

use crate::acp::host_ticket::ComuTicketRequest;
use crate::browser_automation::core::ports::{ConsentOption, TicketOutcome};

use super::consent::{ComuTicketPort, ConsentFuture};
use super::host::{ComuBackend, DesktopComuHost};
use super::policy::{ComuCallerScope, ComuPolicyService};
use super::{
    clear_comu_host, describe_tools, dispatch, set_comu_host, ComuFuture, ComuHost,
    TEST_COMU_HOST_LOCK,
};

const OWN_PID: u32 = 4242;
const OTHER_PID: i64 = 9001;

struct ScriptedTickets {
    answers: Mutex<VecDeque<TicketOutcome>>,
    requests: Mutex<Vec<ComuTicketRequest>>,
    cancelled_sessions: Mutex<Vec<String>>,
}

impl ScriptedTickets {
    fn new(outcomes: impl IntoIterator<Item = TicketOutcome>) -> Arc<Self> {
        Arc::new(Self {
            answers: Mutex::new(outcomes.into_iter().collect()),
            requests: Mutex::new(Vec::new()),
            cancelled_sessions: Mutex::new(Vec::new()),
        })
    }

    fn prompt_count(&self) -> usize {
        self.requests.lock().len()
    }
}

impl ComuTicketPort for ScriptedTickets {
    fn request_comu(
        &self,
        ticket: ComuTicketRequest,
        _wait: Duration,
    ) -> ConsentFuture<'_, TicketOutcome> {
        self.requests.lock().push(ticket);
        let outcome = self
            .answers
            .lock()
            .pop_front()
            .unwrap_or(TicketOutcome::Unattended);
        Box::pin(async move { outcome })
    }

    fn cancel_session(&self, session_id: &str) {
        self.cancelled_sessions.lock().push(session_id.to_string());
    }
}

struct FakeComuBackend {
    windows: Mutex<Vec<WindowOwnerInfo>>,
    calls: AtomicUsize,
    cursor_states: Mutex<Vec<(String, bool, Option<String>)>>,
    ended_sessions: Mutex<Vec<String>>,
    hold_calls: Option<Arc<Notify>>,
}

impl FakeComuBackend {
    fn new(windows: Vec<WindowOwnerInfo>) -> Arc<Self> {
        Arc::new(Self {
            windows: Mutex::new(windows),
            calls: AtomicUsize::new(0),
            cursor_states: Mutex::new(Vec::new()),
            ended_sessions: Mutex::new(Vec::new()),
            hold_calls: None,
        })
    }

    fn blocking(windows: Vec<WindowOwnerInfo>, hold: Arc<Notify>) -> Arc<Self> {
        Arc::new(Self {
            windows: Mutex::new(windows),
            calls: AtomicUsize::new(0),
            cursor_states: Mutex::new(Vec::new()),
            ended_sessions: Mutex::new(Vec::new()),
            hold_calls: Some(hold),
        })
    }

    fn call_count(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

impl ComuBackend for FakeComuBackend {
    fn definitions(&self) -> Vec<Value> {
        troupe_comu_engine::tool_definitions()
    }

    fn call_tool<'a>(
        &'a self,
        _session_id: &'a str,
        tool_name: &'a str,
        _args: Value,
    ) -> ComuFuture<'a, ComuCallOutput> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let hold = self.hold_calls.clone();
        let name = tool_name.to_string();
        Box::pin(async move {
            if let Some(notify) = hold {
                notify.notified().await;
            }
            ComuCallOutput {
                ok: true,
                blocks: vec![ComuContentBlock::Text {
                    text: format!("executed {name}"),
                }],
                structured: Some(json!({ "status": "completed", "tool": name })),
            }
        })
    }

    fn list_windows_snapshot<'a>(
        &'a self,
        _session_id: &'a str,
    ) -> ComuFuture<'a, Vec<WindowOwnerInfo>> {
        let snap = self.windows.lock().clone();
        Box::pin(async move { snap })
    }

    fn configure_session_cursor<'a>(
        &'a self,
        session_id: &'a str,
        enabled: bool,
        badge_label: Option<&'a str>,
    ) -> ComuFuture<'a, ()> {
        self.cursor_states.lock().push((
            session_id.to_string(),
            enabled,
            badge_label.map(str::to_string),
        ));
        Box::pin(async {})
    }

    fn end_session<'a>(&'a self, session_id: &'a str) -> ComuFuture<'a, ()> {
        self.ended_sessions.lock().push(session_id.to_string());
        Box::pin(async {})
    }

    fn check_os_permissions(&self) -> OsPermissionsStatus {
        OsPermissionsStatus {
            platform: "linux".to_string(),
            accessibility_granted: true,
            screen_recording_granted: true,
            display_available: true,
            ready: true,
            summary: "Ready".to_string(),
        }
    }

    fn request_os_permission(&self, _kind: OsPermissionKind) -> OsPermissionsStatus {
        self.check_os_permissions()
    }
}

fn sample_windows() -> Vec<WindowOwnerInfo> {
    vec![
        WindowOwnerInfo {
            window_id: 100,
            pid: i64::from(OWN_PID),
            app_name: "Troupe".to_string(),
            bundle_id: Some("com.termul-manager.app".to_string()),
            bounds: Some([10.0, 10.0, 800.0, 600.0]),
        },
        WindowOwnerInfo {
            window_id: 200,
            pid: OTHER_PID,
            app_name: "Calculator".to_string(),
            bundle_id: Some("com.apple.calculator".to_string()),
            bounds: Some([900.0, 100.0, 400.0, 500.0]),
        },
    ]
}

fn refusal_code(out: &ComuCallOutput) -> Option<&str> {
    out.structured
        .as_ref()?
        .pointer("/refusal/code")?
        .as_str()
}

#[tokio::test]
async fn dispatch_without_registered_host_fails_closed() {
    let _guard = TEST_COMU_HOST_LOCK.lock().await;
    clear_comu_host();

    let defs = describe_tools();
    assert_eq!(defs.len(), super::COMU_TOOL_PAIRS.len());

    let out = dispatch("sess-1", "agent-1", "comu_list_windows", json!({})).await;
    assert!(!out.ok);
    assert_eq!(refusal_code(&out), Some("capability_unavailable"));
}

#[tokio::test]
async fn self_guard_blocks_own_pid_window_desktop_click_and_run_steps() {
    let dir = TempDir::new().unwrap();
    let policy = Arc::new(ComuPolicyService::new(dir.path().to_path_buf(), vec![]));
    let tickets = ScriptedTickets::new([TicketOutcome::Answered(ConsentOption::AllowSession)]);
    let backend = FakeComuBackend::new(sample_windows());
    let host = DesktopComuHost::with_backend(
        backend.clone(),
        vec![],
        tickets.clone(),
        policy,
        Arc::new(|_| "proj-1".to_string()),
        OWN_PID,
    );

    // 1. Direct own PID
    let out = host
        .execute("s1", "a1", "comu_kill_app", json!({ "pid": OWN_PID }))
        .await;
    assert!(!out.ok);
    assert_eq!(refusal_code(&out), Some("self_target_blocked"));

    // 2. Own window_id
    let out = host
        .execute(
            "s1",
            "a1",
            "comu_click",
            json!({ "window_id": 100, "x": 50.0, "y": 50.0 }),
        )
        .await;
    assert!(!out.ok);
    assert_eq!(refusal_code(&out), Some("self_target_blocked"));

    // 3. Desktop click landing inside Troupe window bounds [10, 10, 800, 600]
    let out = host
        .execute("s1", "a1", "comu_click", json!({ "x": 200.0, "y": 200.0 }))
        .await;
    assert!(!out.ok);
    assert_eq!(refusal_code(&out), Some("self_target_blocked"));

    // 4. Nested comu_run_steps targeting Troupe window
    let out = host
        .execute(
            "s1",
            "a1",
            "comu_run_steps",
            json!({
                "steps": [
                    { "tool": "comu_click", "args": { "window_id": 200, "x": 920.0, "y": 150.0 } },
                    { "tool": "comu_type_text", "args": { "window_id": 100, "text": "rm -rf" } }
                ]
            }),
        )
        .await;
    assert!(!out.ok);
    assert_eq!(refusal_code(&out), Some("self_target_blocked"));
    assert_eq!(backend.call_count(), 0);
    assert_eq!(tickets.prompt_count(), 0);
}

#[tokio::test]
async fn consent_caches_session_grant_and_persists_project_approval() {
    let dir = TempDir::new().unwrap();
    let policy = Arc::new(ComuPolicyService::new(dir.path().to_path_buf(), vec![]));
    let tickets = ScriptedTickets::new([TicketOutcome::Answered(ConsentOption::AllowProject)]);
    let backend = FakeComuBackend::new(sample_windows());
    let host = DesktopComuHost::with_backend(
        backend.clone(),
        vec![],
        tickets.clone(),
        policy.clone(),
        Arc::new(|_| "proj-alpha".to_string()),
        OWN_PID,
    );
    host.name_agent("a1", "Claude Code");

    // First call to Calculator prompts once and persists to proj-alpha.
    let first = host
        .execute(
            "sess-1",
            "a1",
            "comu_click",
            json!({ "window_id": 200, "x": 920.0, "y": 150.0 }),
        )
        .await;
    assert!(first.ok);
    assert_eq!(tickets.prompt_count(), 1);

    // Second call in same session uses cached grant without prompting.
    let second = host
        .execute(
            "sess-1",
            "a1",
            "comu_type_text",
            json!({ "window_id": 200, "text": "42" }),
        )
        .await;
    assert!(second.ok);
    assert_eq!(tickets.prompt_count(), 1);

    // Brand-new session in the same project reads persisted Allowed Apps without prompting!
    let third = host
        .execute(
            "sess-2",
            "a1",
            "comu_click",
            json!({ "window_id": 200, "x": 920.0, "y": 150.0 }),
        )
        .await;
    assert!(third.ok);
    assert_eq!(tickets.prompt_count(), 1);
    assert_eq!(backend.call_count(), 3);

    let proj_view = policy
        .project_view(&ComuCallerScope::App, "proj-alpha")
        .await
        .unwrap();
    assert_eq!(proj_view.allowed_apps, vec!["com.apple.calculator"]);
}

#[tokio::test]
async fn disabled_master_switch_refuses_calls() {
    let dir = TempDir::new().unwrap();
    let policy = Arc::new(ComuPolicyService::new(dir.path().to_path_buf(), vec![]));
    policy
        .apply_app(
            &ComuCallerScope::App,
            0,
            &json!({ "op": "set_enabled", "enabled": false }),
        )
        .await
        .unwrap();

    let tickets = ScriptedTickets::new([]);
    let backend = FakeComuBackend::new(sample_windows());
    let host = DesktopComuHost::with_backend(
        backend,
        vec![],
        tickets,
        policy,
        Arc::new(|_| "proj-1".to_string()),
        OWN_PID,
    );

    let out = host
        .execute("s1", "a1", "comu_list_windows", json!({}))
        .await;
    assert!(!out.ok);
    assert_eq!(refusal_code(&out), Some("comu_disabled"));
}

#[tokio::test]
async fn stop_latch_interrupts_inflight_call_blocks_followup_and_clears_on_user_turn() {
    let _guard = TEST_COMU_HOST_LOCK.lock().await;
    let dir = TempDir::new().unwrap();
    let policy = Arc::new(ComuPolicyService::new(dir.path().to_path_buf(), vec![]));
    let tickets = ScriptedTickets::new([TicketOutcome::Answered(ConsentOption::AllowSession)]);
    let hold = Arc::new(Notify::new());
    let backend = FakeComuBackend::blocking(sample_windows(), hold.clone());
    let host = DesktopComuHost::with_backend(
        backend,
        vec![],
        tickets,
        policy,
        Arc::new(|_| "proj-1".to_string()),
        OWN_PID,
    );
    set_comu_host(host.clone());

    let host_for_task = host.clone();
    let inflight = tokio::spawn(async move {
        host_for_task
            .execute(
                "sess-stop",
                "a1",
                "comu_click",
                json!({ "window_id": 200, "x": 950.0, "y": 150.0 }),
            )
            .await
    });

    tokio::time::sleep(Duration::from_millis(25)).await;
    assert!(host.stop("sess-stop"));

    let interrupted = inflight.await.unwrap();
    assert!(!interrupted.ok);
    assert_eq!(refusal_code(&interrupted), Some("stopped_by_user"));

    // Subsequent call in same turn is immediately refused.
    let blocked = host
        .execute("sess-stop", "a1", "comu_list_windows", json!({}))
        .await;
    assert!(!blocked.ok);
    assert_eq!(refusal_code(&blocked), Some("stopped_by_user"));

    // User's next prompt turn clears Stop.
    host.user_turn("sess-stop");
    hold.notify_waiters();
    clear_comu_host();
}
