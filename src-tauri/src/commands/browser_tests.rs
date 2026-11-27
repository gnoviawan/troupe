use super::browser::{process_outbox_item, OutboxOutcome};
use crate::agentation::store::SqliteStore;
use crate::agentation::types::{AFSEventType, AnnotationKind, AnnotationStore};
use serde_json::json;

#[test]
fn outbox_saves_annotation_with_valid_nonce_and_reuses_session() {
    let store = SqliteStore::open_in_memory().unwrap();
    let mut rx = store.event_bus().subscribe();
    let nonce = "secret-nonce-123";

    let first = process_outbox_item(
        &store,
        "tab-user-1",
        nonce,
        None,
        json!({
            "reqId": "req-1",
            "nonce": nonce,
            "kind": "annotation",
            "url": "https://absenin.id",
            "payload": {
                "x": 120,
                "y": 240,
                "comment": "Fix headline spacing",
                "element": "h1.hero",
                "elementPath": "main > h1.hero",
                "timestamp": 1700000000,
                "kind": "feedback",
                "viewportWidth": 1280
            }
        }),
    );

    let (ann_id, session_id) = match first {
        OutboxOutcome::Ack(ack) => {
            assert_eq!(ack.msg_type, "outbox_ack");
            assert_eq!(ack.req_id, "req-1");
            assert!(ack.ok);
            assert!(ack.error.is_none());
            (ack.id.unwrap(), ack.session_id.unwrap())
        }
        OutboxOutcome::Rejected => panic!("expected ack"),
    };

    let stored = store.get_annotation(&ann_id).expect("annotation persisted");
    assert_eq!(stored.comment, "Fix headline spacing");
    assert_eq!(stored.tab_id.as_deref(), Some("tab-user-1"));
    assert_eq!(stored.url.as_deref(), Some("https://absenin.id"));
    assert_eq!(stored.kind, AnnotationKind::Feedback);

    // Second annotation reuses existing session
    let second = process_outbox_item(
        &store,
        "tab-user-1",
        nonce,
        None,
        json!({
            "reqId": "req-2",
            "nonce": nonce,
            "kind": "annotation",
            "url": "https://absenin.id",
            "sessionId": session_id,
            "payload": {
                "x": 369,
                "y": 200,
                "comment": "Align column",
                "element": "Vertical guide · x = 369px",
                "elementPath": "div.hero",
                "timestamp": 1700000001,
                "kind": "guide",
                "measurement": { "axis": "vertical", "positionPx": 369 }
            }
        }),
    );
    match second {
        OutboxOutcome::Ack(ack) => {
            assert!(ack.ok);
            assert_eq!(ack.session_id.as_deref(), Some(session_id.as_str()));
        }
        OutboxOutcome::Rejected => panic!("expected ack"),
    }

    assert_eq!(store.list_sessions().len(), 1);
    assert_eq!(store.get_pending_annotations(&session_id).len(), 2);

    // Event bus emitted SessionCreated + two AnnotationCreated events
    let mut created_count = 0;
    while let Ok(ev) = rx.try_recv() {
        if ev.event_type == AFSEventType::AnnotationCreated {
            created_count += 1;
        }
    }
    assert_eq!(created_count, 2);
}

#[test]
fn outbox_rejects_mismatched_or_empty_nonce() {
    let store = SqliteStore::open_in_memory().unwrap();
    let res = process_outbox_item(
        &store,
        "tab-user-1",
        "expected-secret",
        None,
        json!({
            "reqId": "req-bad",
            "nonce": "wrong-secret",
            "kind": "annotation",
            "url": "https://absenin.id",
            "payload": {
                "x": 1,
                "y": 2,
                "comment": "Spoofed",
                "element": "div",
                "elementPath": "div",
                "timestamp": 1
            }
        }),
    );
    assert_eq!(res, OutboxOutcome::Rejected);
    assert!(store.list_sessions().is_empty());
}

#[test]
fn outbox_dispatches_action_event_on_event_bus() {
    let store = SqliteStore::open_in_memory().unwrap();
    let mut rx = store.event_bus().subscribe();
    let nonce = "action-nonce";

    let outcome = process_outbox_item(
        &store,
        "tab-user-2",
        nonce,
        None,
        json!({
            "reqId": "req-act-1",
            "nonce": nonce,
            "kind": "action",
            "url": "https://absenin.id",
            "payload": {
                "output": "{\"kind\":\"screenshot_area\",\"rect\":{\"x\":10,\"y\":20,\"width\":100,\"height\":50}}"
            }
        }),
    );

    match outcome {
        OutboxOutcome::Ack(ack) => {
            assert!(ack.ok);
            assert_eq!(ack.req_id, "req-act-1");
        }
        OutboxOutcome::Rejected => panic!("expected ack"),
    }

    let mut saw_action = false;
    while let Ok(ev) = rx.try_recv() {
        if ev.event_type == AFSEventType::ActionRequested {
            assert_eq!(ev.payload["tabId"].as_str(), Some("tab-user-2"));
            assert!(ev.payload["output"]
                .as_str()
                .unwrap_or("")
                .contains("screenshot_area"));
            saw_action = true;
        }
    }
    assert!(saw_action, "expected ActionRequested event on bus");
}

#[test]
fn outbox_carries_fallback_session_id_across_same_poll_batch() {
    let store = SqliteStore::open_in_memory().unwrap();
    let nonce = "batch-nonce";

    let first = process_outbox_item(
        &store,
        "tab-user-3",
        nonce,
        None,
        json!({
            "reqId": "req-b1",
            "nonce": nonce,
            "kind": "annotation",
            "url": "https://absenin.id",
            "payload": {
                "x": 10,
                "y": 20,
                "comment": "First in batch",
                "element": "h1",
                "elementPath": "h1",
                "timestamp": 1700000100
            }
        }),
    );
    let batch_sid = match first {
        OutboxOutcome::Ack(ack) => ack.session_id.expect("session created"),
        OutboxOutcome::Rejected => panic!("expected ack"),
    };

    let second = process_outbox_item(
        &store,
        "tab-user-3",
        nonce,
        Some(&batch_sid),
        json!({
            "reqId": "req-b2",
            "nonce": nonce,
            "kind": "annotation",
            "url": "https://absenin.id",
            "payload": {
                "x": 30,
                "y": 40,
                "comment": "Second in batch without sessionId",
                "element": "p",
                "elementPath": "p",
                "timestamp": 1700000101
            }
        }),
    );
    match second {
        OutboxOutcome::Ack(ack) => {
            assert!(ack.ok);
            assert_eq!(ack.session_id.as_deref(), Some(batch_sid.as_str()));
        }
        OutboxOutcome::Rejected => panic!("expected ack"),
    }
    assert_eq!(store.list_sessions().len(), 1);
    assert_eq!(store.get_pending_annotations(&batch_sid).len(), 2);
}
