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

    fn cancel_comu_session(&self, session_id: &str) {
        self.cancelled_sessions.lock().push(session_id.to_string());
    }
}

struct FakeComuBackend {
    windows: Mutex<Vec<WindowOwnerInfo>>,
    calls: Arc<AtomicUsize>,
    cursor_states: Mutex<Vec<(String, bool, Option<String>)>>,
    ended_sessions: Mutex<Vec<String>>,
    started_calls: Option<Arc<Notify>>,
    hold_calls: Mutex<Option<Arc<Notify>>>,
}

impl FakeComuBackend {
    fn new(windows: Vec<WindowOwnerInfo>) -> Arc<Self> {
        Arc::new(Self {
            windows: Mutex::new(windows),
            calls: Arc::new(AtomicUsize::new(0)),
            cursor_states: Mutex::new(Vec::new()),
            ended_sessions: Mutex::new(Vec::new()),
            started_calls: None,
            hold_calls: Mutex::new(None),
        })
    }

    fn blocking(
        windows: Vec<WindowOwnerInfo>,
        started: Arc<Notify>,
        hold: Arc<Notify>,
    ) -> Arc<Self> {
        Arc::new(Self {
            windows: Mutex::new(windows),
            calls: Arc::new(AtomicUsize::new(0)),
            cursor_states: Mutex::new(Vec::new()),
            ended_sessions: Mutex::new(Vec::new()),
            started_calls: Some(started),
            hold_calls: Mutex::new(Some(hold)),
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
        args: Value,
    ) -> ComuFuture<'a, ComuCallOutput> {
        let calls = Arc::clone(&self.calls);
        let started = self.started_calls.clone();
        let hold = self.hold_calls.lock().take();
        let name = tool_name.to_string();
        Box::pin(async move {
            if troupe_comu_engine::resolve_driver_tool_name(&name).is_none() {
                return ComuCallOutput::error_text(
                    "unknown_tool",
                    format!("unknown Troupe Comu tool `{name}`"),
                );
            }
            calls.fetch_add(1, Ordering::SeqCst);
            if let Some(s) = started {
                s.notify_waiters();
            }
            if let Some(notify) = hold {
                notify.notified().await;
            }
            if args.get("simulate_error").and_then(Value::as_bool) == Some(true) {
                return ComuCallOutput::error_text("step_failed", "simulated step failure");
            }
            if name == "comu_zoom" {
                return ComuCallOutput {
                    ok: true,
                    blocks: vec![ComuContentBlock::Image {
                        data: "iVBORw0KGgo=".to_string(),
                        mime_type: "image/png".to_string(),
                    }],
                    structured: Some(json!({ "status": "completed", "tool": name })),
                };
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
    let started = Arc::new(Notify::new());
    let hold = Arc::new(Notify::new());
    let backend = FakeComuBackend::blocking(sample_windows(), started.clone(), hold.clone());
    let host = DesktopComuHost::with_backend(
        backend,
        vec![],
        tickets,
        policy,
        Arc::new(|_| "proj-1".to_string()),
        OWN_PID,
    );
    set_comu_host(host.clone());

    let started_waiter = started.notified();
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

    tokio::time::timeout(Duration::from_secs(2), started_waiter)
        .await
        .expect("backend call_tool should start before Stop");
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

    // User's next prompt turn clears Stop and allows new calls to succeed.
    host.user_turn("sess-stop");
    let resumed = host
        .execute("sess-stop", "a1", "comu_list_windows", json!({}))
        .await;
    assert!(resumed.ok);
    hold.notify_waiters();

    // Idle Stop (zero active receivers) followed by user_turn also recovers cleanly.
    assert!(host.stop("sess-idle"));
    host.user_turn("sess-idle");
    let after_idle_stop = host
        .execute("sess-idle", "a1", "comu_list_windows", json!({}))
        .await;
    assert!(after_idle_stop.ok);

    clear_comu_host();
}

#[tokio::test]
async fn distinct_bundle_ids_sharing_display_name_do_not_cross_authorize() {
    let dir = TempDir::new().unwrap();
    let policy = Arc::new(ComuPolicyService::new(dir.path().to_path_buf(), vec![]));
    let tickets = ScriptedTickets::new([
        TicketOutcome::Answered(ConsentOption::AllowSession),
        TicketOutcome::Answered(ConsentOption::Deny),
    ]);
    let windows = vec![
        WindowOwnerInfo {
            window_id: 301,
            pid: 9101,
            app_name: "Helper".to_string(),
            bundle_id: Some("com.vendor-a.helper".to_string()),
            bounds: Some([0.0, 0.0, 400.0, 400.0]),
        },
        WindowOwnerInfo {
            window_id: 302,
            pid: 9102,
            app_name: "Helper".to_string(),
            bundle_id: Some("com.vendor-b.helper".to_string()),
            bounds: Some([500.0, 0.0, 400.0, 400.0]),
        },
    ];
    let backend = FakeComuBackend::new(windows);
    let host = DesktopComuHost::with_backend(
        backend,
        vec![],
        tickets.clone(),
        policy,
        Arc::new(|_| "proj-1".to_string()),
        OWN_PID,
    );

    let first = host
        .execute("s1", "a1", "comu_click", json!({ "window_id": 301, "x": 10.0, "y": 10.0 }))
        .await;
    assert!(first.ok);
    assert_eq!(tickets.prompt_count(), 1);

    let second = host
        .execute("s1", "a1", "comu_click", json!({ "window_id": 302, "x": 510.0, "y": 10.0 }))
        .await;
    assert!(!second.ok);
    assert_eq!(tickets.prompt_count(), 2);
}

#[tokio::test]
async fn saved_display_name_entry_does_not_authorize_stable_id_app_and_empty_bundle_id_falls_back() {
    let dir = TempDir::new().unwrap();
    let policy = Arc::new(ComuPolicyService::new(dir.path().to_path_buf(), vec![]));
    // Persist a display-name-only approval ("Helper") in proj-1.
    policy.record_project_approval("proj-1", "Helper").await;

    let tickets = ScriptedTickets::new([
        // 1. Stable-ID app with display name "Helper" MUST still prompt (Deny proves it prompted).
        TicketOutcome::Answered(ConsentOption::Deny),
        // 2. Launch with empty bundle_id + non-empty path MUST fall back to path and prompt.
        TicketOutcome::Answered(ConsentOption::AllowSession),
    ]);
    let windows = vec![
        WindowOwnerInfo {
            window_id: 401,
            pid: 9201,
            app_name: "Helper".to_string(),
            bundle_id: None,
            bounds: Some([0.0, 0.0, 300.0, 300.0]),
        },
        WindowOwnerInfo {
            window_id: 402,
            pid: 9202,
            app_name: "Helper".to_string(),
            bundle_id: Some("com.vendor-b.helper".to_string()),
            bounds: Some([400.0, 0.0, 300.0, 300.0]),
        },
    ];
    let backend = FakeComuBackend::new(windows);
    let host = DesktopComuHost::with_backend(
        backend,
        vec![],
        tickets.clone(),
        policy,
        Arc::new(|_| "proj-1".to_string()),
        OWN_PID,
    );

    // Unidentified target with display name "Helper" matches the saved display-name entry.
    let unidentified = host
        .execute("s1", "a1", "comu_click", json!({ "window_id": 401, "x": 10.0, "y": 10.0 }))
        .await;
    assert!(unidentified.ok);
    assert_eq!(tickets.prompt_count(), 0);

    // Stable-ID target sharing display name "Helper" does NOT match the saved display name.
    let stable = host
        .execute("s1", "a1", "comu_click", json!({ "window_id": 402, "x": 410.0, "y": 10.0 }))
        .await;
    assert!(!stable.ok);
    assert_eq!(refusal_code(&stable), Some("consent_denied"));
    assert_eq!(tickets.prompt_count(), 1);

    // Empty bundle_id does not mask launch path.
    let launched = host
        .execute(
            "s1",
            "a1",
            "comu_launch_app",
            json!({ "bundle_id": "  ", "path": "C:\\Tools\\helper.exe" }),
        )
        .await;
    assert!(launched.ok);
    assert_eq!(tickets.prompt_count(), 2);
    assert_eq!(
        tickets.requests.lock()[1].app_id,
        "C:\\Tools\\helper.exe"
    );
}

#[tokio::test]
async fn run_steps_rejects_nested_batches_normalizes_raw_names_and_preserves_step_outputs() {
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

    // 1. Nested run_steps is rejected before prompting or dispatching.
    let nested = host
        .execute(
            "s1",
            "a1",
            "comu_run_steps",
            json!({
                "steps": [
                    {
                        "tool": "run_steps",
                        "args": {
                            "steps": [{ "tool": "click", "args": { "window_id": 200, "x": 920.0, "y": 150.0 } }]
                        }
                    }
                ]
            }),
        )
        .await;
    assert!(!nested.ok);
    assert_eq!(refusal_code(&nested), Some("disallowed_step_tool"));
    assert_eq!(tickets.prompt_count(), 0);
    assert_eq!(backend.call_count(), 0);

    // 2. Raw step names (`get_screen_size`, `zoom`, `click`) normalize to `comu_*`
    //    and `stop_on_error: false` preserves structured output, image blocks, and error text.
    let batch = host
        .execute(
            "s1",
            "a1",
            "comu_run_steps",
            json!({
                "window_id": 200,
                "stop_on_error": false,
                "steps": [
                    { "tool": "get_screen_size", "args": {} },
                    { "tool": "zoom", "args": { "x": 910.0, "y": 120.0 } },
                    { "tool": "click", "args": { "x": 920.0, "y": 150.0, "simulate_error": true } }
                ]
            }),
        )
        .await;
    assert!(!batch.ok);
    let summary = batch.structured.expect("batch summary");
    assert_eq!(summary["status"], "partial");
    assert_eq!(summary["steps"][0]["tool"], "comu_get_screen_size");
    assert_eq!(summary["steps"][0]["ok"], true);
    assert_eq!(summary["steps"][0]["structured"]["tool"], "comu_get_screen_size");
    assert_eq!(summary["steps"][2]["ok"], false);
    assert_eq!(
        summary["steps"][2]["structured"]["refusal"]["code"],
        "step_failed"
    );
    assert!(
        batch
            .blocks
            .iter()
            .any(|b| matches!(b, ComuContentBlock::Image { .. })),
        "expected zoom image block to be preserved in batch blocks"
    );
    assert!(
        batch.blocks.iter().any(|b| match b {
            ComuContentBlock::Text { text } => text.contains("simulated step failure"),
            ComuContentBlock::Image { .. } => false,
        }),
        "expected failing step error text in batch blocks"
    );
}



/// The in-chat "The agent is using Computer Use on this machine." bar must
/// clear when the turn (or the agent's own work) that drove Computer Use ends,
/// not linger until the chat closes.
#[test]
fn turn_end_clears_the_active_computer_use_indicator() {
    use crate::web::sink::{AcpEvent, EventSink};

    #[derive(Default)]
    struct ActiveEvents(Mutex<Vec<bool>>);
    impl EventSink for ActiveEvents {
        fn emit(&self, event: &AcpEvent) {
            if event.type_ == super::stop::EVENT_COMU_SESSION_ACTIVE {
                self.0.lock().push(event.payload["active"] == true);
            }
        }
    }

    let sink = Arc::new(ActiveEvents::default());
    let registry = super::stop::ComuStopRegistry::new(vec![sink.clone()]);
    let session = "sess-comu-indicator";
    registry.mark_active(session);
    assert!(super::stop::session_runtime_state(session).active);

    registry.mark_inactive(session);
    assert!(!super::stop::session_runtime_state(session).active);
    // A second end is a no-op; the next action shows the bar again.
    registry.mark_inactive(session);
    registry.mark_active(session);
    assert_eq!(*sink.0.lock(), vec![true, false, true]);
    registry.forget_session(session);
}

#[derive(Default)]
struct RecordingIndicator {
    calls: Mutex<Vec<String>>,
    ids: Vec<i64>,
}

impl super::indicator::ComuScreenIndicator for RecordingIndicator {
    fn show(&self, label: &str) {
        self.calls.lock().push(format!("show:{label}"));
    }
    fn hide(&self) {
        self.calls.lock().push("hide".to_string());
    }
    fn window_ids(&self) -> Vec<i64> {
        self.ids.clone()
    }
}

fn indicator_host(
    windows: Vec<WindowOwnerInfo>,
    policy: Arc<ComuPolicyService>,
    indicator: Arc<RecordingIndicator>,
) -> Arc<DesktopComuHost> {
    let tickets = ScriptedTickets::new(
        std::iter::repeat_n(TicketOutcome::Answered(ConsentOption::AllowSession), 8),
    );
    let host = DesktopComuHost::with_backend(
        FakeComuBackend::new(windows),
        vec![],
        tickets,
        policy,
        Arc::new(|_| "proj-1".to_string()),
        OWN_PID,
    );
    host.install_screen_indicator(indicator);
    host
}

fn calculator_click() -> Value {
    json!({ "window_id": 200, "x": 950.0, "y": 150.0 })
}

/// The screen-edge indicator shows while a session drives the computer and
/// goes away on turn end, Stop, and close.
#[tokio::test]
async fn screen_indicator_follows_driving_sessions() {
    let dir = TempDir::new().unwrap();
    let policy = Arc::new(ComuPolicyService::new(dir.path().to_path_buf(), vec![]));
    let indicator = Arc::new(RecordingIndicator::default());
    let host = indicator_host(sample_windows(), policy, indicator.clone());
    host.name_agent("a1", "Claude");

    // Passive probes do not count as driving.
    assert!(host.execute("s1", "a1", "comu_list_windows", json!({})).await.ok);
    assert!(indicator.calls.lock().is_empty());

    assert!(host.execute("s1", "a1", "comu_click", calculator_click()).await.ok);
    host.turn_ended("s1");

    assert!(host.execute("s2", "a1", "comu_click", calculator_click()).await.ok);
    host.stop("s2");

    assert!(host.execute("s3", "a1", "comu_click", calculator_click()).await.ok);
    host.session_ended("s3");

    assert_eq!(
        *indicator.calls.lock(),
        vec![
            "show:Claude is using this computer",
            "hide",
            "show:Claude is using this computer",
            "hide",
            "show:Claude is using this computer",
            "hide",
        ]
    );
}

#[tokio::test]
async fn screen_indicator_respects_the_policy_toggle() {
    let dir = TempDir::new().unwrap();
    let policy = Arc::new(ComuPolicyService::new(dir.path().to_path_buf(), vec![]));
    policy
        .apply_app(
            &ComuCallerScope::App,
            0,
            &json!({ "op": "set_screen_indicator_enabled", "enabled": false }),
        )
        .await
        .unwrap();
    let indicator = Arc::new(RecordingIndicator::default());
    let host = indicator_host(sample_windows(), policy, indicator.clone());

    assert!(host.execute("s1", "a1", "comu_click", calculator_click()).await.ok);
    host.turn_ended("s1");
    assert!(indicator.calls.lock().is_empty());
}

/// The indicator covers every monitor and is click-through, so a desktop
/// click under it must not count as clicking Troupe. Troupe's main window and
/// targeting the indicator directly stay blocked.
#[tokio::test]
async fn self_guard_hit_test_skips_the_screen_indicator_window() {
    const OVERLAY_ID: i64 = 300;
    let mut windows = sample_windows();
    windows.push(WindowOwnerInfo {
        window_id: OVERLAY_ID,
        pid: i64::from(OWN_PID),
        app_name: "Troupe".to_string(),
        bundle_id: Some("com.termul-manager.app".to_string()),
        bounds: Some([0.0, 0.0, 3000.0, 2000.0]),
    });
    let desktop_click = json!({ "x": 1500.0, "y": 900.0 });
    let dir = TempDir::new().unwrap();
    let policy = Arc::new(ComuPolicyService::new(dir.path().to_path_buf(), vec![]));

    // Without the indicator's ids the overlay blocks every desktop click.
    let unaware = indicator_host(
        windows.clone(),
        Arc::clone(&policy),
        Arc::new(RecordingIndicator::default()),
    );
    let out = unaware.execute("s1", "a1", "comu_click", desktop_click.clone()).await;
    assert_eq!(refusal_code(&out), Some("self_target_blocked"));

    let host = indicator_host(
        windows,
        policy,
        Arc::new(RecordingIndicator {
            ids: vec![OVERLAY_ID],
            ..RecordingIndicator::default()
        }),
    );
    let out = host.execute("s1", "a1", "comu_click", desktop_click).await;
    assert_ne!(refusal_code(&out), Some("self_target_blocked"));

    let main_window = host
        .execute("s1", "a1", "comu_click", json!({ "x": 200.0, "y": 200.0 }))
        .await;
    assert_eq!(refusal_code(&main_window), Some("self_target_blocked"));

    let overlay_by_id = host
        .execute(
            "s1",
            "a1",
            "comu_click",
            json!({ "window_id": OVERLAY_ID, "x": 10.0, "y": 10.0 }),
        )
        .await;
    assert_eq!(refusal_code(&overlay_by_id), Some("self_target_blocked"));
}

/// A `comu_run_steps` batch only counts as driving when a step acts.
#[tokio::test]
async fn screen_indicator_ignores_read_only_batches() {
    let dir = TempDir::new().unwrap();
    let policy = Arc::new(ComuPolicyService::new(dir.path().to_path_buf(), vec![]));
    let indicator = Arc::new(RecordingIndicator::default());
    let host = indicator_host(sample_windows(), policy, indicator.clone());
    host.name_agent("a1", "Claude");

    let read_only = json!({ "steps": [{ "tool": "comu_list_windows", "args": {} }] });
    host.execute("s1", "a1", "comu_run_steps", read_only).await;
    host.execute("s1", "a1", "comu_run_steps", json!({ "steps": [] }))
        .await;
    assert!(indicator.calls.lock().is_empty());

    let acting = json!({
        "steps": [
            { "tool": "comu_list_windows", "args": {} },
            { "tool": "comu_click", "args": calculator_click() }
        ]
    });
    host.execute("s1", "a1", "comu_run_steps", acting).await;
    assert_eq!(
        *indicator.calls.lock(),
        vec!["show:Claude is using this computer"]
    );
}
