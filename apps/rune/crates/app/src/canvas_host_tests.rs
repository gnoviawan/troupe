//! Unit tests for `canvas_host`: theme tokens, UTF-8 reply slicing, `.figcraft` routing from
//! `open_file`, and `/ws` gateway frame dispatch (`canvas_call` / `canvas_lease_lost`).

use client::ws::Command;
use model::projects::{Event as ProjectEvent, ProjectList, ProjectSummary};
use model::workspace::Tab;
use serde_json::json;
use troupe_protocol::ws::{CanvasCallTag, CanvasLeaseLostReason, CanvasLeaseLostTag, WsCanvasCallFrame, WsCanvasLeaseLostFrame, WsRequestType, WsServerFrame};
use ui::canvas::{Phase, Then};

use crate::State;
use crate::canvas_host::{CanvasFetch, CanvasWsPending, slice_end_utf8, theme_tokens};

fn state_with_project(id: &str, root: &str) -> State {
    let mut s = State::default();
    let project = ProjectSummary {
        id: id.into(),
        name: "Alpha".into(),
        path: Some(root.into()),
        color: "#5e6ad2".into(),
        is_archived: false,
        is_default: true,
    };
    s.projects_step(ProjectEvent::ApplyRegistryList(
        ProjectList { projects: vec![project], default_project_id: Some(id.into()) },
        Some(id.into()),
    ));
    s.workspace.activate(id, None);
    s
}

#[test]
fn theme_tokens_emit_all_required_rrggbb_fields() {
    let t = theme::tokens(theme::DEFAULT_FAMILY, true);
    let payload = theme_tokens(t);
    assert_eq!(payload["mode"], "dark");
    for key in ["background", "card", "foreground", "mutedForeground", "border", "primary", "primaryFill", "destructive"] {
        let hex = payload[key].as_str().expect(key);
        assert_eq!(hex.len(), 7, "{key}={hex}");
        assert!(hex.starts_with('#'));
        assert!(hex[1..].bytes().all(|b| b.is_ascii_hexdigit()));
    }
}

#[test]
fn slice_end_utf8_respects_multibyte_boundaries() {
    let text = "a😀b"; // 'a' (1) + '😀' (4) + 'b' (1) = 6 bytes
    assert_eq!(slice_end_utf8(text, 0, 3), 1);
    assert_eq!(slice_end_utf8(text, 1, 4), 5);
    assert_eq!(slice_end_utf8(text, 5, 4), 6);
}

#[test]
fn open_file_routes_figcraft_documents_to_canvas_tab() {
    let mut s = state_with_project("p1", "/work/alpha");
    s.open_file("/work/alpha/sample.figcraft");
    let has_canvas_tab = model::workspace::leaves(s.workspace.root())
        .into_iter()
        .flat_map(|l| l.tabs.iter())
        .any(|tab| matches!(tab, Tab::Canvas { project_id, doc_path, .. } if project_id == "p1" && doc_path == "/work/alpha/sample.figcraft"));
    assert!(has_canvas_tab, "opening .figcraft creates a Tab::Canvas");
    assert!(s.editors.docs.docs.is_empty(), "binary .figcraft must not open in text editor");
}

#[test]
fn ws_canvas_call_and_lease_lost_drive_session_and_gateway_replies() {
    let mut s = state_with_project("p1", "/work/alpha");
    s.canvases.open("p1", "/work/alpha/sample.figcraft", true, 0.0);
    let _ = s.canvases.take_effects();
    // Simulate `/canvas/open` success so a mount is held on mountSeq 12.
    s.canvas_fetched(
        CanvasFetch::Post { project_id: "p1".into(), then: Then::Open { generation: 1 } },
        Ok(json!({
            "canvasId": "cv1", "baseUrl": "", "canvasToken": "tok", "pinTag": "termul-v0.6.0",
            "docKey": "/work/alpha/sample.figcraft", "mountSeq": 12, "evicted": [], "fallbackFonts": []
        })),
    );
    assert!(s.sync.sent.iter().any(|cmd| matches!(cmd, Command::Call { kind: WsRequestType::CanvasMount, payload, .. } if payload["mountSeq"] == 12)));

    // A canvas_call for a stale mountSeq is answered CANVAS_CLOSED over /ws.
    s.canvas_ws_frame(WsServerFrame::CanvasCall(WsCanvasCallFrame {
        kind: CanvasCallTag,
        call_id: 99,
        project_id: "p1".into(),
        mount_seq: 11,
        method: "document.inspect".into(),
        params: "{}".into(),
        timeout_ms: Some(5_000),
    }));
    assert!(s.sync.sent.iter().any(|cmd| matches!(
        cmd,
        Command::Call { kind: WsRequestType::CanvasReply, payload, .. }
        if payload["callId"] == 99 && payload["ok"] == false && payload["error"]["code"] == "CANVAS_CLOSED"
    )));

    // canvas_lease_lost moves the session to Elsewhere.
    s.canvas_ws_frame(WsServerFrame::CanvasLeaseLost(WsCanvasLeaseLostFrame {
        kind: CanvasLeaseLostTag,
        project_id: "p1".into(),
        mount_seq: 12,
        reason: CanvasLeaseLostReason::TakenOver,
    }));
    assert_eq!(s.canvases.session("p1").unwrap().phase, Phase::Elsewhere);

    // Mount refusal also transitions cleanly without panicking.
    s.canvas_ws_reply(CanvasWsPending::Mount { project_id: "p1".into(), mount_seq: 12 }, Ok(Some(json!({"attached": false, "heldElsewhere": true}))));
}
