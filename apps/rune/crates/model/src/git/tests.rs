use serde_json::json;

use super::*;

fn status(path: &str, status: &str, staged: bool) -> StatusDetail {
    StatusDetail { path: path.into(), status: status.into(), staged }
}

fn calls(effects: &[Effect]) -> Vec<(TaskId, &'static str)> {
    effects
        .iter()
        .filter_map(|e| match e {
            Effect::Call { task, call } => Some((*task, call.api().0)),
            _ => None,
        })
        .collect()
}

#[test]
fn a_staged_and_unstaged_file_counts_once() {
    let mut state = State::default();
    state.statuses.insert("/p".into(), vec![status("a", "modified", true), status("a", "modified", false), status("b", "untracked", false)]);
    assert_eq!(changed_file_count(&state, "/p/"), 2);
    let counts = count_statuses(state.statuses("/p"));
    assert_eq!(counts, Counts { modified: 1, staged: 1, untracked: 1, has_changes: true });
    assert!(!count_statuses(&[]).has_changes);
}

#[test]
fn the_deepest_segment_bounded_key_wins() {
    let keys = ["/repo", "/repo/sub", "/repo/subway"];
    assert_eq!(deepest_tracked_key("/repo/sub/file.rs", keys), Some("/repo/sub"));
    assert_eq!(deepest_tracked_key("/repo/subway", keys), Some("/repo/subway"));
    assert_eq!(deepest_tracked_key("/elsewhere", keys), None);
}

#[test]
fn a_watched_diff_stat_reads_once_per_30_s_with_one_trailing_read() {
    let mut state = State::default();
    step(&mut state, Event::WatchDiffStat("/p".into()), 0.0);
    let first = step(&mut state, Event::RefreshDiffStat("/p".into()), 1_000.0);
    assert_eq!(calls(&first.effects).len(), 1);
    let throttled = step(&mut state, Event::RefreshDiffStat("/p".into()), 2_000.0);
    assert!(matches!(throttled.effects[0], Effect::ScheduleDiffStat { at, .. } if at == 31_000.0));
    let again = step(&mut state, Event::RefreshDiffStat("/p".into()), 3_000.0);
    assert!(!again.effects.iter().any(|e| matches!(e, Effect::ScheduleDiffStat { .. })), "one trailing read");
    let due = step(&mut state, Event::DiffStatDue("/p".into()), 31_000.0);
    assert_eq!(calls(&due.effects).len(), 1);
    step(&mut state, Event::UnwatchDiffStat("/p".into()), 32_000.0);
    let unwatched = step(&mut state, Event::RefreshDiffStat("/p".into()), 90_000.0);
    assert!(calls(&unwatched.effects).is_empty());
}

#[test]
fn a_commit_reads_the_head_then_sends_it_and_refreshes() {
    let mut state = State::default();
    let started = step(&mut state, Event::Commit("/p".into(), "fix".into(), String::new(), false), 0.0);
    let (ctx_task, op) = calls(&started.effects)[0];
    assert_eq!(op, "getCommitContext");
    let context = json!({ "branch": "main", "hasUpstream": false, "ahead": 0, "behind": 0, "stagedCount": 1, "hasHead": true, "lastSubject": "", "lastBody": "" });
    let head = step(&mut state, Event::Reply(ctx_task, Ok(context)), 0.0);
    let (log_task, op) = calls(&head.effects)[0];
    assert_eq!(op, "getLog");
    let commit = step(&mut state, Event::Reply(log_task, Ok(json!([{ "hash": "abc" }]))), 0.0);
    let Effect::Call { call: Call::Commit { expected_head, .. }, task } = &commit.effects[0] else {
        panic!("{:?}", commit.effects)
    };
    assert_eq!(expected_head, "abc");
    assert_eq!(state.branch("/p"), Some("main"));
    let refreshed = step(&mut state, Event::Reply(*task, Ok(Value::Null)), 0.0);
    assert_eq!(calls(&refreshed.effects)[0].1, "getStatus");
    assert!(state.running(started.task.unwrap()));
}

#[test]
fn routes_match_the_web_client() {
    let route = Call::StashList { cwd: "C:\\a b".into() }.route();
    assert_eq!(route.path, "/git/stash-list?cwd=C%3A%5Ca%20b");
    assert!(!route.post);
    let save = Call::StashSave { cwd: "/p".into(), message: None, include_untracked: Some(true) }.route();
    assert_eq!(save.body, Some(json!({ "cwd": "/p", "includeUntracked": true })));
}

#[test]
fn ledger_keys_match_the_web_clients_journal() {
    let drop = Call::StashDrop { cwd: "/p".into(), index: 0 }.ledger().unwrap();
    assert_eq!((drop.kind, drop.entity.as_str(), drop.fresh), ("git_stash_drop", r#"["/p","0"]"#, false));
    let commit = Call::Commit { cwd: "/p".into(), summary: "s".into(), description: String::new(), amend: false, expected_head: "h".into() }.ledger().unwrap();
    assert_eq!((commit.entity.as_str(), commit.fresh), ("/p", true));
    let branch = Call::CreateBranch { cwd: "/p".into(), name: "x".into() }.ledger().unwrap();
    assert_eq!(branch.entity, r#"["/p","x",""]"#);
    assert!(Call::Stage { cwd: "/p".into(), path: "a".into() }.ledger().is_none());
}

#[test]
fn the_diff_revision_moves_only_when_a_diff_changes() {
    let mut state = State::default();
    let start = state.diff_revision();
    let read = step(&mut state, Event::FetchDiff("/p".into(), "a".into(), false, None), 0.0);
    assert_eq!(state.diff_revision(), start, "a read in flight changes nothing");
    let (task, _) = calls(&read.effects)[0];
    step(&mut state, Event::Reply(task, Ok(json!("diff"))), 0.0);
    let after = state.diff_revision();
    assert!(after > start);
    step(&mut state, Event::RefreshStatus("/p".into()), 0.0);
    assert_eq!(state.diff_revision(), after);
}
