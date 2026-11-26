use model::github::{FilterPatch, ItemKind, ItemRef, KindFilter, MergeAction, MergeMethod, RepoSlot};
use serde_json::{Value, json};

use crate::State;
use crate::github::*;
use crate::tests::{connected, projects_frame, read_reply};

/// A live state on project alpha (`/w/alpha`) with the GitHub side panel open.
fn on_alpha() -> State {
    let mut s = connected();
    s.apply(read_reply(2, Value::Null));
    s.apply(read_reply(3, Value::Null));
    s.apply(projects_frame());
    for call in 4..=10 {
        s.apply(read_reply(call, Value::Null));
    }
    let project = s.active_project().unwrap().id.clone();
    s.layout.record.last_column_tab_by_project.insert(project, model::shell_layout::Tab::Github);
    s.layout.record.side_panel_visible = true;
    s.now = 1.0;
    s
}

const REPO: &str = r#"{ "host": "github.com", "owner": "acme", "name": "app" }"#;

fn repo() -> Value {
    serde_json::from_str(REPO).unwrap()
}

fn item(number: u64) -> Value {
    json!({
        "kind": "pr", "number": number, "title": format!("Item {number}"), "state": "open", "author": "ann",
        "labels": [], "assignees": [], "commentCount": 0, "updatedAt": "2026-11-20T10:00:00Z",
        "url": format!("https://github.com/acme/app/pull/{number}"), "checks": null,
    })
}

fn detail(status: &str) -> Value {
    json!({
        "repo": repo(), "item": item(7), "body": "", "createdAt": "2026-11-20T10:00:00Z", "comments": [], "timeline": [],
        "stateReason": null, "fetchedAt": "2026-11-20T10:00:00Z",
        "pr": {
            "headRefName": "feature/x", "headSha": "abc", "baseRefName": "main", "isCrossRepository": false,
            "maintainerCanModify": true, "mergeable": "mergeable", "mergeStateStatus": status, "autoMergeEnabled": false,
            "additions": 1, "deletions": 0, "changedFiles": 1, "reviewThreads": [],
        },
    })
}

fn host(path: &str) -> Value {
    match path {
        "/github/context" => json!({
            "auth": { "state": "connected", "host": "github.com", "account": "ann", "scopes": [], "tokenSource": "keyring", "missingScopes": [], "fixCommands": [] },
            "repo": { "kind": "resolved", "origin": repo(), "upstream": null },
        }),
        "/github/items" => json!({ "repo": repo(), "items": [item(7), item(8)], "fetchedAt": "2026-11-20T10:00:00Z" }),
        "/github/item" => detail("CLEAN"),
        "/github/checks" => json!({ "headSha": "abc", "rollup": null, "checks": [], "fetchedAt": "x" }),
        "/github/repo-meta" => json!({ "repo": repo(), "labels": [], "assignableUsers": ["ann"], "viewer": "ann", "fetchedAt": "x" }),
        "/github/merge" => json!({ "number": 7 }),
        "/github/comment" => json!({ "comment": { "id": "C1", "author": "ann", "isBot": false, "body": "hi", "createdAt": "x", "url": "u" } }),
        _ => json!({ "number": 7 }),
    }
}

/// Answers every call in flight with `host`, until none is left. Returns the paths and bodies.
fn answer_all(s: &mut State) -> Vec<(String, Value)> {
    let mut sent = Vec::new();
    while !s.github.unsent.is_empty() {
        for (task, request) in std::mem::take(&mut s.github.unsent) {
            let reply = host(&request.path);
            sent.push((request.path.clone(), request.body.clone().unwrap_or_default()));
            s.github_replied(task, Ok(reply));
        }
    }
    sent
}

fn paths(sent: &[(String, Value)]) -> Vec<&str> {
    sent.iter().map(|(p, _)| p.as_str()).collect()
}

fn pr() -> ItemRef {
    ItemRef { cwd: "/w/alpha".into(), repo: RepoSlot::Origin, kind: ItemKind::Pr, number: 7 }
}

#[test]
fn the_open_panel_reads_the_context_then_the_list() {
    let mut s = on_alpha();
    s.github_tick();
    assert_eq!(paths(&answer_all(&mut s)), ["/github/context"]);
    s.github_tick();
    let sent = answer_all(&mut s);
    assert_eq!(paths(&sent), ["/github/items"]);
    assert_eq!(sent[0].1["filter"]["state"], "open");
    assert_eq!(s.github.cache.list("/w/alpha").data.map(|l| l.items.len()), Some(2));
    s.now = 60.0;
    s.github_tick();
    assert!(s.github.unsent.is_empty(), "the list polls every two minutes, not sooner");
}

#[test]
fn a_filter_change_reloads_the_list_with_that_filter() {
    let mut s = on_alpha();
    s.github_tick();
    answer_all(&mut s);
    s.github_tick();
    answer_all(&mut s);
    s.github_set_filter("/w/alpha", FilterPatch { kind: Some(KindFilter::Issue), ..FilterPatch::default() });
    let sent = answer_all(&mut s);
    assert_eq!(sent[0].1["filter"]["kind"], "issue");
}

#[test]
fn opening_a_row_adds_its_tab_and_the_tab_reads_the_item_checks_and_meta() {
    let mut s = on_alpha();
    s.github_tick();
    answer_all(&mut s);
    s.github_tick();
    answer_all(&mut s);
    let row = s.github.cache.list("/w/alpha").data.unwrap().items[0].clone();
    s.github_open_item("/w/alpha", &row);
    assert_eq!(s.github_visible_item(), Some(pr()));
    s.github_tick();
    let sent = answer_all(&mut s);
    assert_eq!(paths(&sent), ["/github/item", "/github/checks", "/github/repo-meta"]);
}

#[test]
fn merging_asks_first_and_only_the_confirm_posts_the_merge() {
    let mut s = on_alpha();
    s.github_step(model::github::Event::LoadItem(pr(), None));
    answer_all(&mut s);
    s.overlays.open(MERGE_CONFIRM);
    assert!(s.github.unsent.is_empty(), "opening the confirm posts nothing");
    s.close(MERGE_CONFIRM);
    assert!(s.github.unsent.is_empty(), "cancel posts nothing");
    s.github_merge(&pr(), MergeAction::Merge { method: MergeMethod::Squash });
    assert!(s.github_item_ui(&pr()).acting);
    let sent = answer_all(&mut s);
    assert_eq!(sent[0].0, "/github/merge");
    assert_eq!(sent[0].1, json!({ "cwd": "/w/alpha", "repo": "origin", "number": 7, "action": { "kind": "merge", "method": "squash" } }));
    assert!(!s.github_item_ui(&pr()).acting, "the re-read settles the merge");
}

#[test]
fn a_comment_clears_only_once_github_accepts_it() {
    let mut s = on_alpha();
    s.github_item_ui(&pr()).comment.body = rune::TextEdit::new("hi");
    s.github_post_comment(&pr(), None);
    assert!(s.github_item_ui(&pr()).comment.posting);
    let (task, _) = s.github.unsent.remove(0);
    s.github_replied(task, Err(client::http::HttpError::Refused { code: "GH_FAILED".into(), message: "nope".into(), status: 502 }));
    let draft = &s.github_item_ui(&pr()).comment;
    assert_eq!((draft.body.text.as_str(), draft.error.as_deref(), draft.posting), ("hi", Some("nope"), false));
    s.github_post_comment(&pr(), None);
    answer_all(&mut s);
    assert_eq!(s.github_item_ui(&pr()).comment.body.text, "");
}

#[test]
fn a_picker_applies_only_the_difference_from_what_it_opened_with() {
    let mut s = on_alpha();
    s.github_open_picker(PickerFor::Assignees(pr()), vec!["bob".into()]);
    s.github.unsent.clear();
    s.github_toggle_pick("bob");
    s.github_toggle_pick("ann");
    s.github_apply_picker();
    let sent = answer_all(&mut s);
    assert_eq!(sent[0].1["addAssignees"], json!(["ann"]));
    assert_eq!(sent[0].1["removeAssignees"], json!(["bob"]));
    assert!(s.github.picker.is_none(), "Done closes the picker once GitHub accepts it");
}

#[test]
fn the_timeline_orders_by_time_and_drops_the_close_a_merge_implies() {
    let mut d: model::github::ItemDetail = serde_json::from_value(detail("CLEAN")).unwrap();
    let event = |id: &str, kind: &str, at: &str| serde_json::from_value(json!({ "id": id, "kind": kind, "actor": "ann", "createdAt": at, "label": null, "assignee": null })).unwrap();
    d.timeline = vec![event("M", "merged", "2026-11-20T10:00:00Z"), event("X", "closed", "2026-11-20T10:00:02Z"), event("R", "reopened", "2026-11-20T09:00:00Z")];
    d.comments = vec![serde_json::from_value(json!({ "id": "C", "author": "b", "isBot": false, "body": "", "createdAt": "2026-11-20T10:00:00Z", "url": "" })).unwrap()];
    let order: Vec<String> = crate::github_logic::timeline_entries(&d)
        .into_iter()
        .map(|e| match e {
            crate::github_logic::Entry::Comment(c) => c.id.clone(),
            crate::github_logic::Entry::Event(e) => e.id.clone(),
        })
        .collect();
    assert_eq!(order, ["R", "C", "M"]);
}

#[test]
fn the_merge_box_follows_githubs_merge_state() {
    let d: model::github::ItemDetail = serde_json::from_value(detail("BLOCKED")).unwrap();
    let pr = d.pr.unwrap();
    let rules = crate::github_logic::merge_box(model::github::ItemState::Open, &pr, MergeMethod::Rebase).unwrap();
    assert!(!rules.can_merge);
    assert_eq!(rules.reason.as_deref(), Some("Merging is blocked by required reviews or checks."));
    assert_eq!(rules.auto_merge, Some(MergeAction::EnableAutoMerge { method: MergeMethod::Rebase }));
    assert_eq!(crate::github_logic::merge_box(model::github::ItemState::Merged, &pr, MergeMethod::Merge), None);
}

#[test]
fn small_words_match_the_web_client() {
    use crate::github_logic::*;
    assert_eq!(done_label(&["a".into(), "b".into()], &["c".into()], "label"), "Done \u{b7} +2 \u{2212}1 labels");
    assert_eq!(done_label(&["a".into()], &["a".into()], "label"), "Done");
    assert_eq!(plain_preview("<!-- x --> **Hi** [docs](http://x) <details>y</details> `ok`"), "Hi docs ok");
    let now = crate::git_history::parse_iso("2026-11-20T10:05:00Z").unwrap();
    assert_eq!(relative_ago("2026-11-20T10:00:00Z", now), "5m ago");
    assert_eq!(relative_ago("2026-11-20T10:04:30Z", now), "just now");
    assert_eq!(relative_ago("bad", now), "earlier");
}
