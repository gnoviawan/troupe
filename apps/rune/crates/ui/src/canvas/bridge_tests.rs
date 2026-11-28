//! Replays recorded `postMessage` bridge traffic from an edit session and asserts identical
//! envelopes, replies, events, and error semantics (`lib/canvas-bridge.ts`).

use serde_json::{Value, json};

use super::bridge::{Bridge, Called, Code, Event, Reply, build_embed_url, canvas_origin_of};

const MOUNT_SEQ: u64 = 7;

/// Recorded transcript of one complete FigCraft edit session:
/// mount -> ready -> describe -> openDocument -> host facts -> inspect -> node.create (with
/// 64-bit node id) -> dirtyChanged -> ui.select -> events.poll (comments) -> snapshot -> markSaved.
struct RecordedCall {
    method: &'static str,
    params: &'static str,
    editor_reply: Value,
    expected_reply: Reply,
}

#[test]
fn replays_recorded_edit_session_and_asserts_exact_replies() {
    let mut bridge = Bridge::new(MOUNT_SEQ);

    // Calls made before `event.ready` wait in the queue.
    let early = bridge.call("host.describe", "{}", None);
    assert_eq!(early, Called::Waiting(1));
    assert!(bridge.take_outbox().is_empty(), "queued until event.ready");

    // Editor boots and posts `event.ready`.
    bridge.receive(&json!({"method": "event.ready", "params": "{}", "mountSeq": MOUNT_SEQ}), 1_000.0);
    assert!(bridge.ready());
    assert_eq!(bridge.take_events(), vec![Event::Ready]);
    assert_eq!(
        bridge.take_outbox(),
        vec![json!({"id": 1, "method": "host.describe", "params": "{}", "mountSeq": MOUNT_SEQ})]
    );

    bridge.receive(
        &json!({
            "id": 1,
            "ok": true,
            "result": r#"{"tag":"termul-v0.6.0","engineVersion":"0.15.0"}"#,
            "mountSeq": MOUNT_SEQ,
        }),
        1_010.0,
    );
    assert_eq!(
        bridge.take_answered(),
        vec![(1, Reply::Ok(r#"{"tag":"termul-v0.6.0","engineVersion":"0.15.0"}"#.into()))]
    );

    // Replay the recorded session calls in order.
    let session = [
        RecordedCall {
            method: "host.openDocument",
            params: r#"{"bytes":"UEsDBBQAAAAIA=="}"#,
            editor_reply: json!({"id": 2, "ok": true, "result": r#"{"contentVersion":1}"#, "mountSeq": MOUNT_SEQ}),
            expected_reply: Reply::Ok(r#"{"contentVersion":1}"#.into()),
        },
        RecordedCall {
            method: "host.theme",
            params: r##"{"tokens":{"mode":"dark","background":"#08090a","card":"#111214","foreground":"#f7f8f8","mutedForeground":"#8a8f98","border":"#23252a","primary":"#5e6ad2","primaryFill":"#5e6ad2","destructive":"#e5484d"}}"##,
            editor_reply: json!({"id": 3, "ok": true, "result": r#"{"dark":true}"#, "mountSeq": MOUNT_SEQ}),
            expected_reply: Reply::Ok(r#"{"dark":true}"#.into()),
        },
        RecordedCall {
            method: "host.file",
            params: r#"{"name":"sample","path":"sample.figcraft","location":"Alpha"}"#,
            editor_reply: json!({"id": 4, "ok": true, "result": r#"{"contentVersion":1}"#, "mountSeq": MOUNT_SEQ}),
            expected_reply: Reply::Ok(r#"{"contentVersion":1}"#.into()),
        },
        RecordedCall {
            method: "document.inspect",
            params: "{}",
            editor_reply: json!({"id": 5, "ok": true, "result": r#"{"name":"Sample","pages":[1]}"#, "mountSeq": MOUNT_SEQ}),
            expected_reply: Reply::Ok(r#"{"name":"Sample","pages":[1]}"#.into()),
        },
        // 64-bit node id survives unparsed through params and result.
        RecordedCall {
            method: "engine.execute",
            params: r#"{"command":"node.create","params":{"parent":18446744073709551615,"type":"rectangle","name":"Hero"}}"#,
            editor_reply: json!({"id": 6, "ok": true, "result": r#"{"id":9007199254740993,"parent":18446744073709551615}"#, "mountSeq": MOUNT_SEQ}),
            expected_reply: Reply::Ok(r#"{"id":9007199254740993,"parent":18446744073709551615}"#.into()),
        },
        // Selection round-trip.
        RecordedCall {
            method: "ui.select",
            params: r#"{"ids":[9007199254740993]}"#,
            editor_reply: json!({"id": 7, "ok": true, "result": r#"{"selection":[9007199254740993]}"#, "mountSeq": MOUNT_SEQ}),
            expected_reply: Reply::Ok(r#"{"selection":[9007199254740993]}"#.into()),
        },
        // Comments / changes polling.
        RecordedCall {
            method: "events.poll",
            params: r#"{"after":0,"topics":["comments"],"limit":100}"#,
            editor_reply: json!({"id": 8, "ok": true, "result": r#"{"cursor":4,"events":[{"seq":4,"kind":"comment.added"}]}"#, "mountSeq": MOUNT_SEQ}),
            expected_reply: Reply::Ok(r#"{"cursor":4,"events":[{"seq":4,"kind":"comment.added"}]}"#.into()),
        },
        // Save snapshot + markSaved.
        RecordedCall {
            method: "host.snapshot",
            params: "{}",
            editor_reply: json!({"id": 9, "ok": true, "result": r#"{"contentVersion":2,"bytes":"UEsDBBQ="}"#, "mountSeq": MOUNT_SEQ}),
            expected_reply: Reply::Ok(r#"{"contentVersion":2,"bytes":"UEsDBBQ="}"#.into()),
        },
        RecordedCall {
            method: "host.markSaved",
            params: r#"{"contentVersion":2}"#,
            editor_reply: json!({"id": 10, "ok": true, "result": r#"{"dirty":false,"contentVersion":2}"#, "mountSeq": MOUNT_SEQ}),
            expected_reply: Reply::Ok(r#"{"dirty":false,"contentVersion":2}"#.into()),
        },
    ];

    for (idx, step) in session.iter().enumerate() {
        let expected_id = (idx + 2) as u64;
        assert_eq!(bridge.call(step.method, step.params, None), Called::Waiting(expected_id));
        let sent = bridge.take_outbox();
        assert_eq!(
            sent,
            vec![json!({
                "id": expected_id,
                "method": step.method,
                "params": step.params,
                "mountSeq": MOUNT_SEQ,
            })]
        );
        bridge.receive(&step.editor_reply, 1_100.0 + idx as f64 * 10.0);
        assert_eq!(bridge.take_answered(), vec![(expected_id, step.expected_reply.clone())]);
    }

    // Editor emits dirtyChanged with a u64 contentVersion exceeding JS safe integer range.
    bridge.receive(
        &json!({
            "method": "event.dirtyChanged",
            "params": r#"{"dirty":true,"contentVersion":18446744073709551615}"#,
            "mountSeq": MOUNT_SEQ,
        }),
        2_000.0,
    );
    bridge.receive(
        &json!({"method": "event.crashed", "params": r#"{"reason":"oom"}"#, "mountSeq": MOUNT_SEQ}),
        2_010.0,
    );
    bridge.receive(
        &json!({"method": "event.selectionChanged", "params": "{}", "mountSeq": MOUNT_SEQ}),
        2_020.0,
    );
    assert_eq!(
        bridge.take_events(),
        vec![
            Event::DirtyChanged { dirty: true, content_version: "18446744073709551615".into() },
            Event::Crashed { reason: "oom".into() },
        ]
    );
}

#[test]
fn drops_wrong_mount_seq_and_unknown_methods_locally() {
    let mut bridge = Bridge::new(MOUNT_SEQ);
    bridge.receive(&json!({"method": "event.ready", "params": "{}", "mountSeq": MOUNT_SEQ}), 100.0);
    let _ = bridge.take_events();

    for method in ["event.ready", "app.quit", "window.close", "bogus", "engine.", ".foo"] {
        assert_eq!(
            bridge.call(method, "{}", None),
            Called::Now(Reply::failure(Code::UnknownMethod, format!("unknown method {method}")))
        );
    }
    assert!(bridge.take_outbox().is_empty());

    assert_eq!(bridge.call("document.inspect", "{}", None), Called::Waiting(1));
    let _ = bridge.take_outbox();

    // Wrong mountSeq reply and event are dropped.
    bridge.receive(&json!({"id": 1, "ok": true, "result": "\"stale\"", "mountSeq": MOUNT_SEQ - 1}), 110.0);
    bridge.receive(&json!({"method": "event.crashed", "params": r#"{"reason":"old"}"#, "mountSeq": MOUNT_SEQ - 1}), 110.0);
    assert!(bridge.take_answered().is_empty());
    assert!(bridge.take_events().is_empty());

    bridge.receive(&json!({"id": 1, "ok": true, "result": "\"current\"", "mountSeq": MOUNT_SEQ}), 120.0);
    assert_eq!(bridge.take_answered(), vec![(1, Reply::Ok("\"current\"".into()))]);
}

#[test]
fn queued_deadline_reload_and_dispose_semantics() {
    let mut bridge = Bridge::new(MOUNT_SEQ);
    assert_eq!(bridge.call("engine.execute", r#"{"command":"x"}"#, Some(1_500.0)), Called::Waiting(1));
    assert_eq!(bridge.call("document.inspect", "{}", Some(5_000.0)), Called::Waiting(2));
    assert_eq!(bridge.call("engine.commands", "{}", None), Called::Waiting(3));

    bridge.receive(&json!({"method": "event.ready", "params": "{}", "mountSeq": MOUNT_SEQ}), 2_000.0);
    let outbox = bridge.take_outbox();
    let sent: Vec<&str> = outbox.iter().map(|v| v["method"].as_str().unwrap()).collect();
    assert_eq!(sent, vec!["document.inspect", "engine.commands"]);
    let answered = bridge.take_answered();
    assert_eq!(answered.len(), 1);
    assert_eq!(answered[0].0, 1);
    assert!(matches!(&answered[0].1, Reply::Err { code: Code::Timeout, .. }));

    // Typed error passthrough + unknown code -> ENGINE_ERROR.
    bridge.receive(
        &json!({"id": 2, "ok": false, "error": {"code": "CANVAS_HIDDEN", "message": "off screen"}, "mountSeq": MOUNT_SEQ}),
        2_010.0,
    );
    bridge.receive(
        &json!({"id": 3, "ok": false, "error": {"code": "FUTURE_CODE", "message": "boom"}, "mountSeq": MOUNT_SEQ}),
        2_010.0,
    );
    assert_eq!(
        bridge.take_answered(),
        vec![
            (2, Reply::failure(Code::Hidden, "off screen")),
            (3, Reply::failure(Code::EngineError, "boom")),
        ]
    );

    // Reload cancels in-flight with CANVAS_CLOSED.
    assert_eq!(bridge.call("host.snapshot", "{}", None), Called::Waiting(4));
    let _ = bridge.take_outbox();
    bridge.receive(&json!({"method": "event.ready", "params": "{}", "mountSeq": MOUNT_SEQ}), 2_100.0);
    let reload_ans = bridge.take_answered();
    assert_eq!(reload_ans.len(), 1);
    assert_eq!(reload_ans[0].0, 4);
    assert!(matches!(&reload_ans[0].1, Reply::Err { code: Code::Closed, .. }));

    // Dispose answers pending and future calls with CANVAS_CLOSED.
    assert_eq!(bridge.call("document.inspect", "{}", None), Called::Waiting(5));
    bridge.dispose();
    let disposed_ans = bridge.take_answered();
    assert_eq!(disposed_ans.len(), 1);
    assert!(matches!(&disposed_ans[0].1, Reply::Err { code: Code::Closed, .. }));
    assert!(matches!(bridge.call("document.inspect", "{}", None), Called::Now(Reply::Err { code: Code::Closed, .. })));
}

#[test]
fn embed_url_and_origin_match_react_contract() {
    assert_eq!(
        build_embed_url("http://127.0.0.1:5199", "cv0123", "ab12", "http://tauri.localhost", 3),
        "http://127.0.0.1:5199/canvas/cv0123/ab12/?embed=kiosk&ho=http%3A%2F%2Ftauri.localhost&ms=3"
    );
    assert_eq!(
        build_embed_url("", "cv1", "ff", "https://termul.example", 1),
        "/canvas/cv1/ff/?embed=kiosk&ho=https%3A%2F%2Ftermul.example&ms=1"
    );
    let url = build_embed_url("", "control&control=1", "?control", "https://termul.example/?control&control=1", 2);
    let query = url.split_once('?').unwrap().1;
    let keys: Vec<&str> = query.split('&').map(|p| p.split('=').next().unwrap()).collect();
    assert_eq!(keys, vec!["embed", "ho", "ms"]);

    assert_eq!(canvas_origin_of("http://127.0.0.1:5199/sub", "http://localhost:8198"), "http://127.0.0.1:5199");
    assert_eq!(canvas_origin_of("", "http://127.0.0.1:8198"), "http://127.0.0.1:8198");
}
