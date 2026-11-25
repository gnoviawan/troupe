use serde_json::{Value, json};

use crate::State;
use crate::git::*;
use crate::tests::{connected, projects_frame, read_reply};

/// A live state on project alpha (`/w/alpha`), with the first refresh's calls answered.
fn on_alpha() -> State {
    let mut s = connected();
    s.apply(read_reply(2, Value::Null));
    s.apply(read_reply(3, Value::Null));
    s.apply(projects_frame());
    for call in 4..=10 {
        s.apply(read_reply(call, Value::Null));
    }
    assert!(s.workspace.record().is_some(), "alpha's tree is on screen");
    s.now = 1.0;
    s.git_tick();
    s
}

/// The routes the driver sent since the last call, oldest first.
fn sent(s: &mut State) -> Vec<String> {
    s.git.unsent.drain(..).map(|(_, route)| route.path).collect()
}

/// Answers every call in flight with `answer(path)`, until none is left.
fn answer_all(s: &mut State, answer: impl Fn(&str) -> Value) -> Vec<String> {
    let mut paths = Vec::new();
    while !s.git.unsent.is_empty() {
        let calls = std::mem::take(&mut s.git.unsent);
        for (task, route) in calls {
            let reply = answer(&route.path);
            paths.push(route.path);
            s.git_replied(task, Ok(reply));
        }
    }
    paths
}

fn context(branch: &str, staged: u32) -> Value {
    json!({ "branch": branch, "hasUpstream": true, "ahead": 0, "behind": 0, "stagedCount": staged, "hasHead": true, "lastSubject": "last", "lastBody": "" })
}

fn host(path: &str) -> Value {
    match path {
        "/git/status" => json!([{ "path": "src/a.rs", "status": "modified", "staged": false }, { "path": "b.md", "status": "untracked", "staged": false }]),
        "/git/commit-context" => context("feature/login", 1),
        "/git/log" => json!([{ "hash": "abc123" }]),
        "/git/diff-stat" => json!({ "added": 2, "removed": 1 }),
        p if p.starts_with("/git/branch-list") => json!(["main", "feature/login"]),
        p if p.starts_with("/git/stash-list") => json!([]),
        _ => Value::Null,
    }
}

#[test]
fn the_active_project_reads_its_working_copy_and_names_its_branch() {
    let mut s = on_alpha();
    let paths = answer_all(&mut s, host);
    assert!(paths.contains(&"/git/stash-list?cwd=%2Fw%2Falpha".to_owned()), "{paths:?}");
    assert!(paths.contains(&"/git/status".to_owned()));
    s.git_tick();
    let alpha = s.active_project().unwrap();
    assert_eq!(alpha.client.git_branch.as_deref(), Some("feature/login"));
    assert_eq!(alpha.client.is_git_repo, Some(true));
    assert_eq!(model::git::changed_file_count(&s.git.cache, "/w/alpha"), 2);
}

#[test]
fn the_poll_rereads_after_30_seconds_only() {
    let mut s = on_alpha();
    answer_all(&mut s, host);
    s.now = 20.0;
    s.git_tick();
    assert!(sent(&mut s).is_empty());
    s.now = 31.5;
    s.git_tick();
    let paths = sent(&mut s);
    assert!(paths.contains(&"/git/status".to_owned()), "{paths:?}");
}

#[test]
fn staging_a_row_posts_the_stage_then_refreshes() {
    let mut s = on_alpha();
    answer_all(&mut s, host);
    s.git_stage(vec!["src/a.rs".into()]);
    assert!(s.git.mutating);
    let paths = answer_all(&mut s, host);
    assert_eq!(paths[..2], ["/git/stage".to_owned(), "/git/status".to_owned()]);
    assert!(!s.git.mutating, "the settle clears the busy flag");
}

#[test]
fn a_commit_sends_the_head_and_clears_the_box() {
    let mut s = on_alpha();
    answer_all(&mut s, host);
    s.git.summary = rune::TextEdit::new("fix: login");
    assert!(s.git_can_commit());
    s.git_commit(false);
    let (_, route) = s.git.unsent.first().cloned().expect("a commit call");
    assert_eq!(route.path, "/git/commit");
    assert_eq!(route.body.unwrap()["expectedHead"], "abc123");
    answer_all(&mut s, host);
    assert!(s.git.summary.text.is_empty());
    assert!(!s.git.committing);
}

#[test]
fn a_refused_commit_toasts_the_host_message() {
    let mut s = on_alpha();
    answer_all(&mut s, host);
    s.git.summary = rune::TextEdit::new("fix");
    s.git_commit(false);
    let (task, _) = s.git.unsent.remove(0);
    s.git_replied(task, Err(client::http::HttpError::Refused { status: 500, code: "GIT_ERROR".into(), message: "hook failed".into() }));
    assert!(s.toasts.visible().iter().any(|t| t.title == "Failed to commit: Error: hook failed"), "{:?}", s.toasts.visible());
    assert_eq!(s.git.summary.text, "fix", "the draft stays");
}

#[test]
fn switching_with_changes_asks_first_and_stash_switch_pops_after() {
    let mut s = on_alpha();
    answer_all(&mut s, host);
    s.git_switch_branch("main".into());
    assert!(s.overlays.is_open(SWITCH_BRANCH));
    assert!(s.git.unsent.is_empty());
    s.git_execute_switch(true);
    let paths = answer_all(&mut s, host);
    let order: Vec<&str> = paths.iter().map(String::as_str).filter(|p| ["/git/stash-save", "/git/branch-switch", "/git/stash-pop"].contains(p)).collect();
    assert_eq!(order, ["/git/stash-save", "/git/branch-switch", "/git/stash-pop"]);
    assert!(s.toasts.visible().iter().any(|t| t.title == "Switched to branch main and reapplied changes"));
}

#[test]
fn an_open_diff_tab_reads_its_diff_quietly_once() {
    let mut s = on_alpha();
    answer_all(&mut s, host);
    s.git_open_diff("src/a.rs".into(), false);
    s.git_tick();
    s.git_tick();
    let diffs: Vec<_> = s.git.unsent.iter().filter(|(_, r)| r.path == "/git/diff").cloned().collect();
    assert_eq!(diffs.len(), 1, "one read in flight");
    let (task, _) = diffs[0].clone();
    s.git.unsent.clear();
    s.git_replied(task, Ok(json!("diff --git a/src/a.rs b/src/a.rs\n--- a/src/a.rs\n+++ b/src/a.rs\n@@ -1 +1 @@\n-old\n+new\n")));
    s.git_tick();
    let (_, view) = s.git.diff_views.values().next().expect("a built view");
    assert_eq!(view.files().len(), 1);
    assert_eq!(view.files()[0].additions(), 1);
}

#[test]
fn the_git_tab_shows_the_changed_file_count_and_the_panel_lists_the_files() {
    let mut rt = crate::tests::runtime_at((1280.0, 800.0));
    for call in 7..=10 {
        rt.state.apply(read_reply(call, Value::Null));
    }
    assert!(rt.state.workspace.record().is_some(), "alpha's tree is on screen");
    rt.state.now = 1.0;
    rt.state.git_tick();
    answer_all(&mut rt.state, host);
    rt.state.workspace.step(model::workspace::Event::AddGitTab("/w/alpha".into(), None));
    rt.invalidate();
    rt.frame(0.05);
    rt.frame(0.066);
    let badge = rt.elements().iter().find(|e| e.key.as_deref().is_some_and(|k| k.starts_with("tab-badge-"))).map(|e| e.label.clone());
    assert!(badge.is_some(), "the Git tab draws its badge");
    crate::tests::tap(&mut rt, "tool-strip-changes");
    assert!(crate::tests::present(&rt, "git-unstaged-src/a.rs"));
    assert!(crate::tests::present(&rt, "git-unstaged-b.md"));
    crate::tests::tap(&mut rt, "git-unstaged-src/a.rs");
    let opened = model::workspace::leaves(rt.state.workspace.root())
        .iter()
        .flat_map(|l| l.tabs.iter())
        .any(|t| matches!(t, model::workspace::Tab::Diff { path, staged: false, .. } if path == "src/a.rs"));
    assert!(opened, "a row click opens its diff tab");
}

#[test]
fn ledgered_routes_carry_a_command_id() {
    let commit = super::git::tests_request("/git/commit");
    assert!(commit.command_id.is_some_and(|id| !id.is_empty()));
    assert!(super::git::tests_request("/git/status").command_id.is_none());
}
