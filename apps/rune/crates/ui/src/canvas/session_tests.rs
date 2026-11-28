//! Unit tests for `Canvases`: mount lifecycle, pin checks, autosave, disk conflict resolution,
//! crash recovery, lease loss, unmount confirm, gateway forwarding, and document path matching.

use serde_json::{Value, json};

use super::bridge::Code;
use super::flow::{Canvases, Then};
use super::persistence::{canvas_doc_file, is_canvas_doc_path, pick_canvas_doc, same_doc};
use super::session::{Choice, Effect, LeaseLoss, LOAD_TIMEOUT_MS, Phase, UnloadReason, pin_refused};

fn open_info(mount_seq: u64, pin_tag: &str) -> Value {
    json!({
        "canvasId": "cv1",
        "baseUrl": "",
        "canvasToken": "tok-secret",
        "pinTag": pin_tag,
        "docKey": "/proj/design.figcraft",
        "mountSeq": mount_seq,
        "evicted": [],
        "fallbackFonts": [],
    })
}

fn open_to_ready(c: &mut Canvases, project_id: &str, doc_path: &str, mount_seq: u64) {
    c.facts.client_instance_id = "win-1".into();
    c.facts.locale = "en-US".into();
    c.facts.in_process = true;
    c.open(project_id, doc_path, true, 1_000.0);
    let fx = c.take_effects();
    let generation = fx
        .iter()
        .find_map(|e| match e {
            Effect::Post { route: "/canvas/open", body, then: Then::Open { generation } } => {
                assert_eq!(body["clientInstanceId"], "win-1");
                Some(*generation)
            }
            _ => None,
        })
        .expect("POST /canvas/open");
    c.replied(project_id, Then::Open { generation }, Ok(open_info(mount_seq, "termul-v0.6.0")), 1_010.0);
    let fx = c.take_effects();
    assert!(fx.iter().any(|e| matches!(e, Effect::MountEditor { .. })));
    assert!(fx.iter().any(|e| matches!(e, Effect::Ws { kind: "canvas_mount", .. })));

    // Editor becomes ready -> host.describe -> /canvas/read-document -> host.openDocument -> /canvas/document-loaded
    c.editor_messages(project_id, &[json!({"method": "event.ready", "params": "{}", "mountSeq": mount_seq})], 1_020.0);
    let fx = c.take_effects();
    let desc_id = fx
        .iter()
        .find_map(|e| match e {
            Effect::ToEditor { messages, .. } => messages.iter().find(|m| m["method"] == "host.describe").and_then(|m| m["id"].as_u64()),
            _ => None,
        })
        .expect("host.describe sent");
    c.editor_messages(
        project_id,
        &[json!({"id": desc_id, "ok": true, "result": r#"{"tag":"termul-v0.6.0","engineVersion":"0.15.0"}"#, "mountSeq": mount_seq})],
        1_030.0,
    );
    let _ = c.take_effects();

    c.replied(project_id, Then::ReadDocument { mount_seq }, Ok(json!({"bytes": "UEsDBA=="})), 1_040.0);
    let fx = c.take_effects();
    let open_id = fx
        .iter()
        .find_map(|e| match e {
            Effect::ToEditor { messages, .. } => messages.iter().find(|m| m["method"] == "host.openDocument").and_then(|m| m["id"].as_u64()),
            _ => None,
        })
        .expect("host.openDocument sent");
    c.editor_messages(
        project_id,
        &[json!({"id": open_id, "ok": true, "result": r#"{"contentVersion":1}"#, "mountSeq": mount_seq})],
        1_050.0,
    );
    let _ = c.take_effects();

    c.replied(
        project_id,
        Then::DocumentLoaded { mount_seq },
        Ok(json!({"dirty": false, "contentVersion": "1", "savedVersion": "1", "saveError": null, "conflict": null})),
        1_060.0,
    );
    let _ = c.take_effects();
    assert!(matches!(c.session(project_id).unwrap().phase, Phase::Ready { .. }));
}

#[test]
fn full_lifecycle_dirty_autosave_and_gateway_call() {
    let mut c = Canvases::default();
    open_to_ready(&mut c, "p1", "/proj/design.figcraft", 3);

    // Agent sends engine.execute through gateway.
    c.gateway_call(41, "p1", 3, "engine.execute", r#"{"command":"node.create"}"#, Some(5_000.0), 2_000.0);
    let fx = c.take_effects();
    let req_id = fx
        .iter()
        .find_map(|e| match e {
            Effect::ToEditor { messages, .. } => messages.iter().find(|m| m["method"] == "engine.execute").and_then(|m| m["id"].as_u64()),
            _ => None,
        })
        .expect("forwarded to editor");
    c.editor_messages(
        "p1",
        &[
            json!({"id": req_id, "ok": true, "result": r#"{"id":99}"#, "mountSeq": 3}),
            json!({"method": "event.dirtyChanged", "params": r#"{"dirty":true,"contentVersion":2}"#, "mountSeq": 3}),
        ],
        2_050.0,
    );
    let fx = c.take_effects();
    assert!(fx.iter().any(|e| matches!(e, Effect::Ws { kind: "canvas_reply", payload } if payload["callId"] == 41 && payload["ok"] == true)));
    let dirty_then = fx
        .iter()
        .find_map(|e| match e {
            Effect::Post { route: "/canvas/dirty-changed", then, .. } => Some(then.clone()),
            _ => None,
        })
        .expect("POST /canvas/dirty-changed");

    // Host settles debounce with saveDue -> host.snapshot -> write-snapshot -> host.markSaved.
    c.replied(
        "p1",
        dirty_then,
        Ok(json!({"kind": "saveDue", "view": {"dirty": true, "contentVersion": "2", "savedVersion": "1", "saveError": null, "conflict": null}})),
        3_500.0,
    );
    let fx = c.take_effects();
    let snap_id = fx
        .iter()
        .find_map(|e| match e {
            Effect::ToEditor { messages, .. } => messages.iter().find(|m| m["method"] == "host.snapshot").and_then(|m| m["id"].as_u64()),
            _ => None,
        })
        .expect("host.snapshot sent");
    c.editor_messages(
        "p1",
        &[json!({"id": snap_id, "ok": true, "result": r#"{"contentVersion":2,"bytes":"AA=="}"#, "mountSeq": 3})],
        3_510.0,
    );
    let fx = c.take_effects();
    assert!(fx.iter().any(|e| matches!(e, Effect::Post { route: "/canvas/write-snapshot", .. })));

    c.replied(
        "p1",
        Then::Write { mount_seq: 3 },
        Ok(json!({"markSaved": "2", "view": {"dirty": false, "contentVersion": "2", "savedVersion": "2", "saveError": null, "conflict": null}})),
        3_520.0,
    );
    let fx = c.take_effects();
    assert!(fx.iter().any(|e| matches!(e, Effect::ToEditor { messages, .. } if messages.iter().any(|m| m["method"] == "host.markSaved"))));
    assert!(!c.session("p1").unwrap().dirty);
}

#[test]
fn pin_mismatch_refused_in_release_and_watchdog_times_out() {
    assert!(pin_refused(Some("dev"), "termul-v0.6.0", true));
    assert!(!pin_refused(Some("dev"), "termul-v0.6.0", false));
    assert!(!pin_refused(Some("termul-v0.6.0"), "termul-v0.6.0", true));

    let mut c = Canvases::default();
    c.facts.release = true;
    c.open("p1", "/proj/a.figcraft", true, 100.0);
    let _ = c.take_effects();
    c.replied("p1", Then::Open { generation: 1 }, Ok(open_info(1, "termul-v0.6.0")), 110.0);
    let _ = c.take_effects();
    // Watchdog fires after LOAD_TIMEOUT_MS if event.ready never arrives.
    assert!(c.tick(110.0 + LOAD_TIMEOUT_MS - 1.0).is_some());
    assert!(matches!(c.session("p1").unwrap().phase, Phase::Loading { .. }));
    let _ = c.tick(110.0 + LOAD_TIMEOUT_MS);
    assert!(matches!(&c.session("p1").unwrap().phase, Phase::Failed { code, .. } if code == "EDITOR_LOAD_TIMEOUT"));
}

#[test]
fn disk_conflict_crash_remount_lease_loss_and_unmount_confirm() {
    let mut c = Canvases::default();
    open_to_ready(&mut c, "p1", "/proj/design.figcraft", 4);

    // Disk changed while dirty -> conflict banner + toast.
    c.replied(
        "p1",
        Then::Disk { mount_seq: 4 },
        Ok(json!({
            "kind": "conflict",
            "view": {"dirty": true, "contentVersion": "2", "savedVersion": "1", "saveError": null, "conflict": {"diskHash": "abc", "version": "2"}}
        })),
        4_000.0,
    );
    assert!(c.session("p1").unwrap().conflict.is_some());
    let fx = c.take_effects();
    assert!(fx.iter().any(|e| matches!(e, Effect::Toast { error: false, .. })));

    // KeepMine resolves conflict and triggers save.
    c.resolve_conflict("p1", Choice::KeepMine);
    let fx = c.take_effects();
    assert!(fx.iter().any(|e| matches!(e, Effect::Post { route: "/canvas/resolve-conflict", body, .. } if body["choice"] == "keepMine")));

    // Crash -> /canvas/crashed -> remount to mountSeq 5 with crash notice.
    c.editor_messages("p1", &[json!({"method": "event.crashed", "params": r#"{"reason":"panic"}"#, "mountSeq": 4})], 4_100.0);
    let fx = c.take_effects();
    assert!(fx.iter().any(|e| matches!(e, Effect::Post { route: "/canvas/crashed", .. })));
    c.replied(
        "p1",
        Then::Crashed { mount_seq: 4 },
        Ok(json!({"previousMountSeq": 4, "mountSeq": 5, "reason": "panic", "lost": {"count": 2, "sinceMs": null}})),
        4_120.0,
    );
    let s = c.session("p1").unwrap();
    assert_eq!(s.phase.mount_seq(), Some(5));
    assert_eq!(s.crash.as_ref().unwrap().lost.as_ref().unwrap().count, 2);

    // Unmount refused with SAVE_FAILED opens confirm dialog; Discard closes it.
    c.unmount("p1", false);
    let _ = c.take_effects();
    c.replied("p1", Then::Close { mount_seq: 5, discard: false }, Err(("SAVE_FAILED".into(), "disk full".into())), 4_200.0);
    assert_eq!(c.session("p1").unwrap().unsaved_close.as_deref(), Some("disk full"));
    assert!(c.has_unmount_confirm());

    c.keep_editing("p1");
    assert!(!c.has_unmount_confirm());

    // Lease lost -> Elsewhere / Evicted.
    c.lease_lost("p1", 5, LeaseLoss::Elsewhere);
    assert_eq!(c.session("p1").unwrap().phase, Phase::Elsewhere);

    // Gateway call for unmounted seq returns CANVAS_CLOSED.
    c.gateway_call(77, "p1", 5, "document.inspect", "{}", None, 4_300.0);
    let fx = c.take_effects();
    assert!(fx.iter().any(|e| matches!(e, Effect::Ws { kind: "canvas_reply", payload } if payload["error"]["code"] == Code::Closed.as_str())));

    // Eviction on open_to_ready -> Unloaded(Evicted).
    open_to_ready(&mut c, "p2", "/proj2/a.figcraft", 6);
    c.lease_lost("p2", 6, LeaseLoss::Evicted);
    assert_eq!(c.session("p2").unwrap().phase, Phase::Unloaded { reason: UnloadReason::Evicted, message: None });
}

#[test]
fn doc_path_helpers_match_react_rules() {
    assert!(same_doc(r"\\?\D:\Work\design.figcraft", "d:/work/design.figcraft"));
    assert!(same_doc("/a/b/design.figcraft", "/a/b/design.figcraft"));
    assert!(!same_doc("/a/b/A.figcraft", "/a/b/a.figcraft"));
    assert!(is_canvas_doc_path("Hero.FIGCRAFT"));
    assert!(!is_canvas_doc_path("README.md"));
    assert_eq!(pick_canvas_doc([("z.figcraft", false), ("a.figcraft", false), ("dir.figcraft", true)]), Some("a.figcraft"));
    assert_eq!(
        canvas_doc_file("/work/alpha/sub/sample.figcraft", Some("Alpha"), Some("/work/alpha")),
        json!({"name": "sample", "path": "sub/sample.figcraft", "location": "Alpha"})
    );
}
