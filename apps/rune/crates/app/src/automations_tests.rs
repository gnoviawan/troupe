//! The automations driver and its screens, against the fixture automation.

use serde_json::{Value, json};

use crate::State;
use crate::automations::*;
use crate::tests::{connected, projects_frame, read_reply};

fn fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../../../../src/shared/types/__fixtures__/automation.json"
    ))
    .expect("automation.json")
}

/// A live state on project alpha with its Automations tab open and nothing yet answered.
fn on_alpha() -> State {
    let mut s = connected();
    s.apply(read_reply(2, Value::Null));
    s.apply(read_reply(3, Value::Null));
    s.apply(projects_frame());
    for call in 4..=10 {
        s.apply(read_reply(call, Value::Null));
    }
    s.open_automations();
    s.now = 1.0;
    s.automations_tick(1.0);
    s
}

fn automation(id: &str, name: &str) -> Value {
    let mut a = fixture()["automation"].clone();
    a["id"] = json!(id);
    a["name"] = json!(name);
    a
}

fn detail(a: &Value) -> Value {
    json!({ "automation": a, "issues": [], "policy": fixture()["run_policy"] })
}

/// Answers every call in flight with `answer(method, path)`, until none is left.
fn answer_all(s: &mut State, answer: &dyn Fn(&str, &str) -> Value) -> Vec<String> {
    let mut seen = Vec::new();
    for _ in 0..8 {
        let calls = std::mem::take(&mut s.automations.unsent);
        if calls.is_empty() {
            break;
        }
        for (tag, request) in calls {
            let method = request.method.as_str();
            seen.push(format!("{method} {}", request.path));
            s.automation_replied(tag, Ok(answer(method, &request.path)));
        }
        s.automations_tick(s.now);
    }
    seen
}

fn host(method: &str, path: &str) -> Value {
    let a = automation("a1", "Digest");
    match (method, path) {
        ("GET", p) if p.starts_with("/api/automations?project_id=") => {
            json!([a, automation("a2", "Second")])
        }
        ("GET", "/api/automations/a1") => detail(&a),
        ("GET", "/api/automations/a2") => detail(&automation("a2", "Second")),
        ("GET", p) if p.ends_with("/runs?limit=25") => fixture()["runs"].clone(),
        ("GET", p) if p.ends_with("/steps") => fixture()["steps"].clone(),
        ("POST", "/api/automations/a1/run") => fixture()["runs"][1].clone(),
        _ => Value::Null,
    }
}

#[test]
fn the_tab_reads_the_list_then_the_first_automation_and_its_runs() {
    let mut s = on_alpha();
    let calls = answer_all(&mut s, &host);
    assert_eq!(calls[0], "GET /api/automations?project_id=p-a");
    assert!(
        calls.contains(&"GET /api/automations/a1".to_owned()),
        "{calls:?}"
    );
    assert!(
        calls.contains(&"GET /api/automations/a1/runs?limit=25".to_owned()),
        "{calls:?}"
    );
    assert_eq!(s.automations.selected(), Some("a1"));
    assert_eq!(
        s.automations
            .detail
            .of("a1")
            .map(|d| d.automation.name.as_str()),
        Some("Digest")
    );
    assert_eq!(s.automations.runs.of("a1").map(Vec::len), Some(3));
}

#[test]
fn picking_another_automation_reads_its_detail() {
    let mut s = on_alpha();
    answer_all(&mut s, &host);
    s.automation_select(Some("a2".into()));
    s.automations_tick(2.0);
    let calls = answer_all(&mut s, &host);
    assert!(
        calls.contains(&"GET /api/automations/a2".to_owned()),
        "{calls:?}"
    );
    assert_eq!(
        s.automations
            .detail
            .of("a2")
            .map(|d| d.automation.name.as_str()),
        Some("Second")
    );
}

#[test]
fn delete_asks_first_and_only_the_confirm_deletes() {
    let mut s = on_alpha();
    answer_all(&mut s, &host);
    s.automation_ask_delete();
    assert!(s.overlays.is_open(DELETE_CONFIRM));
    assert!(
        s.automations.unsent.is_empty(),
        "nothing is deleted before the confirm"
    );
    s.automation_confirm_delete();
    assert!(!s.overlays.is_open(DELETE_CONFIRM));
    let calls = answer_all(&mut s, &host);
    assert_eq!(calls[0], "DELETE /api/automations/a1");
    assert!(
        calls.contains(&"GET /api/automations?project_id=p-a".to_owned()),
        "the list is read again: {calls:?}"
    );
}

#[test]
fn run_now_opens_the_new_run_and_polls_its_steps_while_it_works() {
    let mut s = on_alpha();
    answer_all(&mut s, &host);
    s.automation_run_now();
    let calls = answer_all(&mut s, &host);
    assert_eq!(calls[0], "POST /api/automations/a1/run");
    assert_eq!(s.automations.run_id.as_deref(), Some("run-2"));
    assert!(
        calls.contains(&"GET /api/automation-runs/run-2/steps".to_owned()),
        "{calls:?}"
    );
    // run-2 is parked on a wait: it changes only through change events, so no poll.
    assert_eq!(s.automations_tick(10.0), None);
}

#[test]
fn cancel_run_posts_without_a_confirm_as_react_does() {
    let mut s = on_alpha();
    answer_all(&mut s, &host);
    s.automation_open_run("run-3".into());
    s.automations_tick(2.0);
    answer_all(&mut s, &host);
    s.automation_cancel_run();
    assert!(s.automations.cancelling);
    let calls = answer_all(&mut s, &host);
    assert_eq!(calls[0], "POST /api/automation-runs/run-3/cancel");
    assert!(!s.automations.cancelling);
}

#[test]
fn a_change_for_the_project_rereads_the_list_and_the_shown_automation() {
    let mut s = on_alpha();
    answer_all(&mut s, &host);
    s.automation_changed(Some(
        serde_json::from_value(json!({ "project_id": "p-a", "automation_id": "a1" })).unwrap(),
    ));
    let calls = answer_all(&mut s, &host);
    assert!(calls.contains(&"GET /api/automations?project_id=p-a".to_owned()));
    assert!(calls.contains(&"GET /api/automations/a1".to_owned()));
}

#[test]
fn the_screens_draw_the_list_the_detail_and_a_run() {
    let mut s = on_alpha();
    answer_all(&mut s, &host);
    let mut rt = rune::Runtime::new(s, crate::view, (1280.0, 800.0), rune::Env::new(true, 1.0));
    rt.frame(0.0);
    rt.frame(0.016);
    for key in [
        "automation-row-a1",
        "automation-row-a2",
        "automation-run-policy",
        "automation-diagram",
        "automation-runs",
    ] {
        assert!(crate::tests::present(&rt, key), "{key} draws");
    }
    // The runs list sits below the diagram, past the fold at 800 px.
    assert!(crate::tests::present(&rt, "automation-run-row-run-1"));
    rt.state.automation_open_run("run-1".into());
    rt.state.automations_tick(3.0);
    let calls = answer_all(&mut rt.state, &host);
    assert!(
        calls.contains(&"GET /api/automation-runs/run-1/steps".to_owned()),
        "{calls:?}"
    );
    rt.invalidate();
    rt.frame(0.1);
    rt.frame(0.116);
    for key in [
        "automation-run",
        "automation-run-canvas",
        "automation-steps",
        "automation-step-trigger",
        "automation-step-each",
    ] {
        assert!(crate::tests::present(&rt, key), "{key} draws");
    }
}

#[test]
fn delete_removes_the_automation_the_confirm_named_even_after_the_selection_moved() {
    let mut s = on_alpha();
    answer_all(&mut s, &host);
    s.automation_ask_delete();
    // a1 is deleted elsewhere: the list now holds only a2, which the pane falls back to.
    let only_a2 = |method: &str, path: &str| match (method, path) {
        ("GET", p) if p.starts_with("/api/automations?project_id=") => {
            json!([automation("a2", "Second")])
        }
        _ => host(method, path),
    };
    s.automation_changed(None);
    answer_all(&mut s, &only_a2);
    assert_eq!(s.automations.selected(), Some("a2"));
    s.automation_confirm_delete();
    let calls = answer_all(&mut s, &only_a2);
    assert_eq!(calls[0], "DELETE /api/automations/a1", "{calls:?}");
    assert!(!calls.contains(&"DELETE /api/automations/a2".to_owned()));
}

#[test]
fn a_cancelled_delete_forgets_its_automation() {
    let mut s = on_alpha();
    answer_all(&mut s, &host);
    s.automation_ask_delete();
    s.automation_cancel_delete();
    s.automation_confirm_delete();
    assert!(
        s.automations.unsent.is_empty(),
        "nothing is deleted after a cancel"
    );
}
