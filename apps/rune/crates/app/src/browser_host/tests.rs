use base64::Engine;
use client::link::Status;
use client::ws::{Command, Output};
use model::projects::{ProjectList, ProjectSummary};
use rune::{Key, KeyEvent, Mods};
use serde_json::json;
use troupe_protocol::ws::WsWatchName;
use ui::browser::KeyDisposition;

use crate::State;

const CHROMIUM_BANDS: &[u8] =
    include_bytes!("../../../ui/src/browser/testdata/frame-colors.jpg");

fn state_with_project() -> State {
    let mut s = State::default();
    s.apply(Output::Status(Status::Live));
    let project = ProjectSummary {
        id: "proj-1".into(),
        name: "Demo".into(),
        path: Some("/tmp/demo".into()),
        color: "#3366ff".into(),
        is_archived: false,
        is_default: true,
    };
    s.projects_step(model::projects::Event::ApplyRegistryList(
        ProjectList {
            projects: vec![project],
            default_project_id: Some("proj-1".into()),
        },
        Some("proj-1".into()),
    ));
    s.workspace.activate("proj-1", None);
    s.sync.sent.clear();
    s
}

#[test]
fn opening_browser_tab_and_navigating_sends_open_watch_and_decodes_frame() {
    let mut s = state_with_project();
    s.open_browser_tab(None, Some("127.0.0.1:8199/test".into()));

    let open_cmd = s.sync.sent.iter().find_map(|c| match c {
        Command::RawCall {
            call,
            kind,
            payload,
        } if kind == "browser_tab_open" => Some((*call, payload.clone())),
        _ => None,
    });
    let (call_id, payload) = open_cmd.expect("browser_tab_open sent");
    assert_eq!(payload["projectId"], "proj-1");
    assert_eq!(payload["url"], "http://127.0.0.1:8199/test");

    let tab_id = s.browser.focused.clone().expect("focused tab");
    s.apply(Output::Reply {
        call: call_id,
        result: Ok(Some(json!({
            "tabId": "host-tab-1",
            "tab": {
                "tabId": "host-tab-1",
                "url": "http://127.0.0.1:8199/test",
                "title": "Test Page",
                "loading": false,
                "agentControlled": false,
                "epoch": 1
            }
        }))),
    });

    assert!(s.sync.sent.iter().any(|c| matches!(
        c,
        Command::Watch {
            watch: WsWatchName::BrowserScreencast,
            scope
        } if scope == "host-tab-1"
    )));
    assert_eq!(
        s.browser
            .session
            .tabs
            .get(&tab_id)
            .map(|t| t.title.as_str()),
        Some("Test Page")
    );

    // Feed a real Chromium non-interleaved JPEG screencast frame.
    let b64 = base64::engine::general_purpose::STANDARD.encode(CHROMIUM_BANDS);
    s.apply(Output::WatchDelta {
        watch: WsWatchName::BrowserScreencast,
        scope: "host-tab-1".into(),
        delta: json!({
            "format": "jpeg",
            "data": b64,
            "width": 1280,
            "height": 800
        }),
    });
    assert!(s.browser.panes.get(&tab_id).is_some_and(|r| r.has_frame));

    // Simulate renderer crash (`tab_closed` pushed over BrowserScreencast).
    s.apply(Output::WatchDelta {
        watch: WsWatchName::BrowserScreencast,
        scope: "host-tab-1".into(),
        delta: json!({
            "event": "tab_closed",
            "tabId": "host-tab-1"
        }),
    });
    assert!(
        s.browser
            .session
            .tabs
            .get(&tab_id)
            .is_some_and(|t| t.crashed)
    );
    assert!(s.browser.panes.get(&tab_id).is_some_and(|r| !r.has_frame));
}

#[test]
fn reconnect_reattaches_live_tabs_and_raw_events_track_agent_control() {
    let mut s = state_with_project();
    s.apply(Output::RawEvent {
        sid: Some("sess-1".into()),
        seq: 1,
        kind: "browser_agent_tab".into(),
        payload: json!({
            "action": "opened",
            "tabId": "agent-tab-9",
            "sessionId": "sess-1",
            "projectId": "proj-1",
            "url": "http://127.0.0.1:8199/"
        }),
    });
    assert!(s.browser.control.session_drives_browser("sess-1"));
    assert!(s.browser.panes.contains_key("agent-tab-9"));
    assert!(s.sync.sent.iter().any(|c| matches!(
        c,
        Command::RawCall { kind, payload, .. }
            if kind == "browser_tab_attach" && payload["tabId"] == "agent-tab-9"
    )));

    s.apply(Output::RawEvent {
        sid: None,
        seq: 0,
        kind: "browser_stop_changed".into(),
        payload: json!({ "sessionId": "sess-1", "stopped": true }),
    });
    assert!(s.browser.control.is_session_stopped("sess-1"));

    // Drop and reconnect -> re-attaches agent-tab-9.
    s.sync.sent.clear();
    s.apply(Output::Status(Status::Reconnecting {
        tries: 1,
        delay_s: 1.0,
    }));
    s.apply(Output::Status(Status::Live));
    assert!(s.sync.sent.iter().any(|c| matches!(
        c,
        Command::RawCall { kind, payload, .. }
            if kind == "browser_tab_attach" && payload["tabId"] == "agent-tab-9"
    )));
}

#[test]
fn palette_shortcut_bubbles_while_plain_keys_forward_on_browser_pane() {
    let mut s = state_with_project();
    s.open_browser_tab(None, Some("https://example.com".into()));
    let tab_id = s.browser.focused.clone().unwrap();

    let mut tx = rune::Tx::default();
    let palette_key = KeyEvent {
        key: Key::Command('k'),
        mods: Mods {
            cmd: true,
            ctrl: true,
            ..Default::default()
        },
    };
    assert_eq!(
        s.browser_key_event(&tab_id, &palette_key, true, &mut tx),
        KeyDisposition::Bubble
    );

    let plain_a = KeyEvent {
        key: Key::Text("a".into()),
        mods: Mods::default(),
    };
    assert_eq!(
        s.browser_key_event(&tab_id, &plain_a, true, &mut tx),
        KeyDisposition::Forward
    );
}
