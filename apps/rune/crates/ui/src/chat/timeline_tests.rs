use model::acp::types::{ChatMessage, Object};
use serde_json::json;

use super::timeline::{Activity, Item, Row, RowKind, rows};

fn message(id: &str, role: &str, text: &str, seq: f64, ts: f64) -> ChatMessage {
    serde_json::from_value(json!({
        "id": id, "role": role, "streaming": false, "timestamp": ts, "seq": seq,
        "blocks": [{ "type": "text", "text": text }],
    }))
    .unwrap()
}

fn tool(id: &str, seq: f64, ts: f64, status: &str) -> Object {
    serde_json::from_value(json!({ "toolCallId": id, "title": id, "status": status, "seq": seq, "timestamp": ts })).unwrap()
}

fn keys(rows: &[Row]) -> Vec<&str> {
    rows.iter().map(|r| r.key.as_str()).collect()
}

#[test]
fn a_finished_turn_folds_its_work_and_keeps_the_last_reply_outside() {
    let messages = [
        message("u1", "user", "How?", 1.0, 1_000.0),
        message("t1", "thought", "Read the router.", 2.0, 2_000.0),
        message("t2", "thought", " Then answer.", 3.0, 2_500.0),
        message("a1", "agent", "Reading first.", 5.0, 4_000.0),
        message("a2", "agent", "## Answer", 7.0, 13_000.0),
    ];
    let tools = [tool("read", 4.0, 3_000.0, "completed"), tool("grep", 6.0, 9_000.0, "completed")];
    let out = rows(&messages, &tools, false);
    assert_eq!(keys(&out), ["u1", "activity:u1", "a2"]);
    assert_eq!(out[0].kind, RowKind::User(0));
    assert_eq!(
        out[1].kind,
        RowKind::Activity(Activity {
            items: vec![Item::Thoughts(vec![1, 2]), Item::Tool(0), Item::Message(3), Item::Tool(1)],
            active: false,
            duration_ms: Some(12_000.0),
            attention: false,
            has_final: true,
        })
    );
    assert_eq!(out[2].kind, RowKind::Reply { index: 4, tail: true });
}

#[test]
fn a_running_turn_keeps_every_reply_inside_its_activity() {
    let messages = [message("u1", "user", "Go", 1.0, 1_000.0), message("a1", "agent", "Working on it", 3.0, 2_000.0)];
    let tools = [tool("edit", 2.0, 1_500.0, "in_progress")];
    let out = rows(&messages, &tools, true);
    assert_eq!(keys(&out), ["u1", "activity:u1"]);
    let RowKind::Activity(a) = &out[1].kind else { panic!("{:?}", out[1]) };
    assert_eq!((a.active, a.duration_ms, a.has_final), (true, None, false));
    assert_eq!(a.items, [Item::Tool(0), Item::Message(1)]);
    assert!(a.attention, "an unfinished tool call flags the turn");
}

#[test]
fn seqless_history_sorts_before_live_items_and_plan_tools_draw_no_row() {
    let mut old = message("u0", "user", "Earlier", 0.0, 500.0);
    old.seq = None;
    let messages = [message("a1", "agent", "Live reply", 2.0, 100.0), old];
    let mut plan = tool("todo", 1.0, 50.0, "completed");
    plan.insert("planTool".into(), json!(true));
    let out = rows(&messages, &[plan], false);
    assert_eq!(keys(&out), ["u0", "a1"]);
    assert_eq!(out[1].kind, RowKind::Reply { index: 0, tail: true });
}

#[test]
fn an_empty_session_with_a_running_turn_shows_one_working_row() {
    let out = rows(&[], &[], true);
    assert_eq!(keys(&out), ["activity:0"]);
}
