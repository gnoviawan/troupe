//! Rune `Runtime` view tests for the canvas panel, phone refusal, conflict bar, crash notice,
//! and unmount confirm dialog.

use rune::{El, Runtime, col, el};
use serde_json::json;

use super::flow::{Canvases, Then};
use super::session::{Effect, Phase};
use super::view::{PHONE_REFUSAL, lost_text, panel, unmount_confirms};

struct Host {
    canvases: Canvases,
    phone: bool,
}

fn get(s: &Host) -> &Canvases {
    &s.canvases
}

fn lens(s: &mut Host) -> &mut Canvases {
    &mut s.canvases
}

fn ready_canvases() -> Canvases {
    let mut c = Canvases::default();
    c.facts.in_process = true;
    c.open("p1", "/proj/sample.figcraft", true, 0.0);
    let _ = c.take_effects();
    c.replied(
        "p1",
        Then::Open { generation: 1 },
        Ok(json!({
            "canvasId": "cv1", "baseUrl": "", "canvasToken": "t", "pinTag": "termul-v0.6.0",
            "docKey": "/proj/sample.figcraft", "mountSeq": 1, "evicted": [], "fallbackFonts": []
        })),
        10.0,
    );
    let _ = c.take_effects();
    c.editor_messages("p1", &[json!({"method": "event.ready", "params": "{}", "mountSeq": 1})], 20.0);
    let _ = c.take_effects();
    c.editor_messages("p1", &[json!({"id": 1, "ok": true, "result": r#"{"tag":"termul-v0.6.0"}"#, "mountSeq": 1})], 30.0);
    let _ = c.take_effects();
    c.replied("p1", Then::ReadDocument { mount_seq: 1 }, Ok(json!({"bytes": null})), 40.0);
    let _ = c.take_effects();
    c.replied(
        "p1",
        Then::DocumentLoaded { mount_seq: 1 },
        Ok(json!({"dirty": true, "contentVersion": "2", "savedVersion": "1", "saveError": null, "conflict": null})),
        50.0,
    );
    let _ = c.take_effects();
    assert!(matches!(c.session("p1").unwrap().phase, Phase::Ready { .. }));
    c
}

fn render(s: &Host, _cx: &mut rune::Cx<Host>) -> El {
    let t = theme::tokens(theme::DEFAULT_FAMILY, true);
    let mut layer = Vec::new();
    let main = panel(s, get, lens, t, "p1", "/proj/sample.figcraft", s.phone, |seq| {
        el().key(format!("test-slot-{seq}")).w_full().grow(1.0)
    });
    unmount_confirms(s, get, lens, t, &mut layer);
    crate::overlay::root(col().w_full().h_full().child(main), layer)
}

#[test]
fn phone_profile_shows_refusal() {
    let mut rt = Runtime::new(Host { canvases: ready_canvases(), phone: true }, render, (390.0, 844.0), rune::Env::new(true, 1.0));
    rt.frame(1.0 / 60.0);
    assert!(rt.find("canvas-unsupported-surface").is_some());
    assert!(rt.find("test-slot-1").is_none());
    assert_eq!(PHONE_REFUSAL, "The canvas is not available at this screen width.");
}

#[test]
fn desktop_panel_conflict_bar_crash_notice_and_unmount_confirm() {
    let mut canvases = ready_canvases();
    // Inject conflict + crash notice + unmount confirm.
    canvases.replied(
        "p1",
        Then::Disk { mount_seq: 1 },
        Ok(json!({
            "kind": "conflict",
            "view": {"dirty": true, "contentVersion": "2", "savedVersion": "1", "saveError": null, "conflict": {"diskHash": "h", "version": "2"}}
        })),
        100.0,
    );
    let _ = canvases.take_effects();
    canvases.sessions.get_mut("p1").unwrap().crash = Some(super::session::Remount {
        previous_mount_seq: 0,
        mount_seq: 1,
        reason: "panic".into(),
        lost: Some(super::session::Lost { count: 1, since_ms: None }),
    });
    assert!(lost_text(canvases.session("p1").unwrap().crash.as_ref().unwrap()).contains("1 unsaved change was lost."));

    let mut rt = Runtime::new(Host { canvases, phone: false }, render, (1280.0, 800.0), rune::Env::new(true, 1.0));
    rt.frame(1.0 / 60.0);
    assert!(rt.find("canvas-header").is_some());
    assert!(rt.find("canvas-dirty").is_some());
    assert!(rt.find("canvas-conflict").is_some());
    assert!(rt.find("canvas-conflict-keepMine").is_some());
    assert!(rt.find("canvas-conflict-takeDisk").is_some());
    assert!(rt.find("canvas-conflict-saveCopy").is_some());
    assert!(rt.find("canvas-crash-notice").is_some());
    assert!(rt.find("test-slot-1").is_some());

    // Dismiss crash notice.
    let (x, y) = rt.find("canvas-crash-dismiss").expect("dismiss button");
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
    assert!(rt.state.canvases.session("p1").unwrap().crash.is_none());
    assert!(rt.find("canvas-crash-notice").is_none());

    // Trigger unmount confirm and check Retry / Discard / Keep editing.
    rt.update(|s, _| s.canvases.sessions.get_mut("p1").unwrap().unsaved_close = Some("disk full".into()));
    rt.frame(1.0 / 60.0);
    assert!(rt.find("canvas-unmount-p1-layer").is_some());
    let (x, y) = rt.find("secondary").expect("Discard changes button");
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
    let fx = rt.state.canvases.take_effects();
    assert!(fx.iter().any(|e| matches!(e, Effect::Post { route: "/canvas/close", body, .. } if body["discard"] == true)));
}
