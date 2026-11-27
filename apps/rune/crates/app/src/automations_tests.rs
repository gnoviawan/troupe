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

/// The host for the editor: `apply_ops` and `set_layout` answer with the automation they made.
fn editor_host(method: &str, path: &str) -> Value {
    match (method, path) {
        ("POST", "/api/automations/a1/ops") => {
            let mut a = automation("a1", "Digest");
            a["revision"] = json!(a["revision"].as_u64().unwrap_or(0) + 1);
            json!({ "automation": a, "issues": [] })
        }
        ("POST", "/api/automations/a1/ops/preview") => detail(&automation("a1", "Digest")),
        ("POST", "/api/automations/a1/layout") => automation("a1", "Digest"),
        _ => host(method, path),
    }
}

#[test]
fn the_editor_adds_two_steps_connects_them_and_saves_one_batch() {
    let mut s = on_alpha();
    answer_all(&mut s, &host);
    s.automation_edit();
    // Add a wait after the trigger-less selection (below the cards), then a fail after it.
    s.automation_add(model::automations::NodeKind::Wait);
    s.automation_add(model::automations::NodeKind::Fail);
    let editor = s.automations.editor.as_ref().expect("editor");
    let ids: Vec<&str> = editor
        .draft
        .doc
        .graph
        .nodes
        .iter()
        .map(|n| n.id.as_str())
        .collect();
    assert!(ids.ends_with(&["wait", "fail"]), "{ids:?}");
    assert!(
        editor
            .draft
            .doc
            .graph
            .edges
            .iter()
            .any(|e| e.from == "wait" && e.to == "fail"),
        "the second step chains after the selected first one"
    );
    // A connect the host would flag is refused at the gesture.
    s.automation_flow_event(troupe_flow::canvas::FlowEvent::Connect {
        from: "fail".into(),
        port: "out".into(),
        to: "wait".into(),
    });
    assert_eq!(
        s.automations.editor.as_ref().unwrap().draft.pending.len(),
        2
    );
    s.automation_flow_event(troupe_flow::canvas::FlowEvent::Connect {
        from: "post".into(),
        port: "out".into(),
        to: "wait".into(),
    });
    assert_eq!(
        s.automations.editor.as_ref().unwrap().draft.pending.len(),
        3
    );
    s.automation_save();
    let calls = answer_all(&mut s, &editor_host);
    assert_eq!(calls[0], "POST /api/automations/a1/ops");
    assert!(
        calls.contains(&"POST /api/automations/a1/layout".to_owned()),
        "{calls:?}"
    );
    let editor = s.automations.editor.as_ref().expect("editor");
    assert!(!editor.draft.dirty() && !editor.saving);
}

#[test]
fn closing_a_dirty_editor_asks_first() {
    let mut s = on_alpha();
    answer_all(&mut s, &host);
    s.automation_edit();
    s.automation_add(model::automations::NodeKind::Merge);
    s.automation_close_editor();
    assert!(
        s.overlays
            .is_open(crate::automation_editor::DISCARD_CONFIRM)
    );
    assert!(s.automations.editor.is_some());
    s.automation_discard_and_close();
    assert!(s.automations.editor.is_none());
}

#[test]
fn deleting_a_step_needs_no_confirm_and_undo_brings_it_back() {
    let mut s = on_alpha();
    answer_all(&mut s, &host);
    s.automation_edit();
    s.automation_flow_event(troupe_flow::canvas::FlowEvent::Select(Some(
        troupe_flow::canvas::Selection::Card("check".into()),
    )));
    s.automations.editor.as_mut().unwrap().flow.selection =
        Some(troupe_flow::canvas::Selection::Card("check".into()));
    s.automation_delete_selected();
    assert!(
        s.automations
            .editor
            .as_ref()
            .unwrap()
            .draft
            .doc
            .graph
            .node("check")
            .is_none()
    );
    s.automation_undo();
    let editor = s.automations.editor.as_ref().unwrap();
    assert!(editor.draft.doc.graph.node("check").is_some());
    assert!(
        !editor.draft.dirty(),
        "undoing the only gesture leaves a clean draft"
    );
}

#[test]
fn the_inspector_writes_a_steps_fields_as_one_update() {
    let mut s = on_alpha();
    answer_all(&mut s, &host);
    s.automation_edit();
    s.automation_inspect(Some("alert".into()));
    let editor = s.automations.editor.as_mut().unwrap();
    editor.fields.a = rune::TextEdit::new("Something broke");
    editor.fields.label = rune::TextEdit::new("Alarm");
    s.automation_commit_fields();
    let draft = &s.automations.editor.as_ref().unwrap().draft;
    let node = draft.doc.graph.node("alert").unwrap();
    assert_eq!(node.label, "Alarm");
    assert!(
        matches!(&node.spec, model::automations::NodeSpec::Fail { message } if message == "Something broke")
    );
    assert_eq!(draft.pending_ops().len(), 1);
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

/// The detail the editor opens on: a condition with no right operand and an HTTP step with an
/// empty body, the shapes an untouched inspector must keep.
fn tricky_host(method: &str, path: &str) -> Value {
    match (method, path) {
        ("GET", "/api/automations/a1") => {
            let mut a = automation("a1", "Digest");
            let nodes = a["graph"]["nodes"].as_array_mut().expect("nodes");
            for node in nodes {
                match node["id"].as_str() {
                    Some("check") => node["spec"]["right"] = Value::Null,
                    Some("post") => node["spec"]["body"] = json!(""),
                    _ => {}
                }
            }
            detail(&a)
        }
        _ => editor_host(method, path),
    }
}

#[test]
fn inspecting_every_card_without_typing_makes_no_edit() {
    let mut s = on_alpha();
    answer_all(&mut s, &tricky_host);
    s.automation_edit();
    let ids: Vec<String> = std::iter::once("trigger".to_owned())
        .chain(
            s.automations
                .editor
                .as_ref()
                .unwrap()
                .draft
                .doc
                .graph
                .nodes
                .iter()
                .map(|n| n.id.clone()),
        )
        .collect();
    for id in ids {
        s.automation_inspect(Some(id.clone()));
        s.automation_commit_fields();
        assert!(
            !s.automations.editor.as_ref().unwrap().draft.dirty(),
            "inspecting {id} made an edit"
        );
    }
}

#[test]
fn exists_drops_the_right_operand_as_react_does() {
    use model::automations::{ConditionOp, NodeSpec};
    let mut s = on_alpha();
    answer_all(&mut s, &host);
    s.automation_edit();
    s.automation_inspect(Some("check".into()));
    s.automation_set_spec("check", |spec| {
        crate::automation_fields::set_op(spec, ConditionOp::Exists)
    });
    let node = s
        .automations
        .editor
        .as_ref()
        .unwrap()
        .draft
        .doc
        .graph
        .node("check")
        .unwrap()
        .clone();
    assert!(
        matches!(
            node.spec,
            NodeSpec::Condition {
                op: ConditionOp::Exists,
                right: None,
                ..
            }
        ),
        "{:?}",
        node.spec
    );
    s.automation_set_spec("check", |spec| {
        crate::automation_fields::set_op(spec, ConditionOp::Eq)
    });
    let node = s
        .automations
        .editor
        .as_ref()
        .unwrap()
        .draft
        .doc
        .graph
        .node("check")
        .unwrap()
        .clone();
    assert!(matches!(node.spec, NodeSpec::Condition { right: Some(ref r), .. } if r.is_empty()));
}

#[test]
fn a_refused_position_write_keeps_the_saved_revision() {
    let mut s = on_alpha();
    answer_all(&mut s, &host);
    s.automation_edit();
    let base = s.automations.editor.as_ref().unwrap().draft.base.revision;
    s.automation_add(model::automations::NodeKind::Wait);
    s.automation_save();
    let refusing = |method: &str, path: &str| match (method, path) {
        ("POST", "/api/automations/a1/layout") => Value::Null,
        _ => editor_host(method, path),
    };
    answer_all(&mut s, &refusing);
    let editor = s.automations.editor.as_ref().unwrap();
    assert_eq!(
        editor.draft.base.revision,
        base + 1,
        "the draft sits on the saved head"
    );
    assert_eq!(
        editor.draft.pending.len(),
        1,
        "the positions wait for the next Save"
    );
    assert!(editor.draft.pending[0].ops.is_empty());
}

#[test]
fn a_late_reply_for_another_automations_editor_is_dropped() {
    let mut s = on_alpha();
    answer_all(&mut s, &host);
    s.automation_edit();
    s.automation_add(model::automations::NodeKind::Wait);
    s.automation_save();
    let late = std::mem::take(&mut s.automations.unsent);
    // The user leaves A's editor and opens B's before A's Save answers.
    s.automations.editor = None;
    s.automations.editing = None;
    s.automation_select(Some("a2".into()));
    s.automations_tick(2.0);
    answer_all(&mut s, &host);
    s.automation_edit();
    let before = s.automations.editor.as_ref().unwrap().draft.base.clone();
    for (tag, _) in late {
        s.automation_replied(tag, Ok(editor_host("POST", "/api/automations/a1/ops")));
    }
    let editor = s.automations.editor.as_ref().unwrap();
    assert_eq!(
        editor.draft.base, before,
        "A's reply did not touch B's draft"
    );
    assert!(
        s.automations.unsent.is_empty(),
        "nothing was written on B's behalf"
    );
}
