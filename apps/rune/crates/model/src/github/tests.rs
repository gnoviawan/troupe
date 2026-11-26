use serde_json::json;

use super::*;

fn at() -> ItemRef {
    ItemRef { cwd: "/repo".into(), repo: RepoSlot::Origin, kind: ItemKind::Pr, number: 7 }
}

fn calls(step: &Step) -> Vec<(TaskId, &'static str)> {
    step.effects
        .iter()
        .filter_map(|e| match e {
            Effect::Call { task, call } => Some((*task, call.op)),
            _ => None,
        })
        .collect()
}

#[test]
fn list_key_matches_the_web_clients() {
    let mut filter = ListFilter { search: "  bug ".into(), labels: vec!["a".into(), "b c".into()], ..ListFilter::default() };
    filter.assignee = AssigneeFilter::User { login: "ann".into() };
    assert_eq!(
        list_key("/r", &filter),
        "/r\u{0}origin\u{0}all\u{0}open\u{0}bug\u{0}any\u{0}[\"a\",\"b c\"]\u{0}user:ann\u{0}\u{0}updated"
    );
}

#[test]
fn a_slower_older_read_never_overwrites_a_newer_one() {
    let mut state = State::default();
    let first = calls(&step(&mut state, Event::LoadContext("/repo".into())))[0].0;
    let second = calls(&step(&mut state, Event::LoadContext("/repo".into())))[0].0;
    let fresh = json!({ "auth": { "state": "cli_missing" }, "repo": { "kind": "not_github" } });
    step(&mut state, Event::Reply(second, Ok(fresh)));
    step(&mut state, Event::Reply(first, Err(Failure { code: "GH_TIMEOUT".into(), error: "slow".into() })));
    let context = state.context("/repo");
    assert_eq!(context.status, Status::Idle);
    assert_eq!(context.data.map(|c| c.auth), Some(AuthStatus::CliMissing));
}

#[test]
fn a_merge_settles_only_after_the_item_is_read_again() {
    let mut state = State::default();
    let merge = step(&mut state, Event::Merge(at(), MergeAction::Merge { method: MergeMethod::Squash }));
    let task = merge.task.unwrap();
    let after = step(&mut state, Event::Reply(task, Ok(json!({ "number": 7 }))));
    assert_eq!(calls(&after).iter().map(|c| c.1).collect::<Vec<_>>(), ["getChecks", "listItems", "getItem"]);
    assert!(state.running(task));
    let item = calls(&after)[2].0;
    let done = step(&mut state, Event::Reply(item, Err(Failure { code: "GH_FAILED".into(), error: "x".into() })));
    assert!(done.effects.contains(&Effect::Settled { task, outcome: Some(Ok(json!({ "number": 7 }))) }));
}

#[test]
fn a_bad_reply_is_an_error_not_a_crash() {
    let mut state = State::default();
    let id = calls(&step(&mut state, Event::LoadContext("/r".into())))[0].0;
    let done = step(&mut state, Event::Reply(id, Ok(json!({ "auth": 3 }))));
    assert_eq!(state.context("/r").error_code.as_deref(), Some(BAD_REPLY));
    assert!(matches!(done.effects[0], Effect::Log { .. }));
}
