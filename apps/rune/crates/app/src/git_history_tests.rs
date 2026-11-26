use serde_json::{Value, json};

use crate::git_history::*;
use crate::tests::{connected, projects_frame, read_reply};

#[test]
fn iso_dates_read_as_utc_milliseconds() {
    assert_eq!(parse_iso("1970-01-01T00:00:00Z"), Some(0.0));
    assert_eq!(parse_iso("2026-11-25T10:00:00+07:00"), parse_iso("2026-11-25T03:00:00Z"));
    assert_eq!(parse_iso("2000-03-01T00:00:00Z"), Some(951_868_800_000.0));
}

#[test]
fn relative_times_match_the_web_client() {
    let now = parse_iso("2026-11-25T12:00:00Z").unwrap();
    assert_eq!(relative_time("2026-11-25T11:59:30Z", now), "now");
    assert_eq!(relative_time("2026-11-25T11:15:00Z", now), "45m");
    assert_eq!(relative_time("2026-11-24T09:00:00Z", now), "1d");
    assert_eq!(relative_time("2026-10-01T09:00:00Z", now), "7w");
    assert_eq!(relative_time("2026-01-05T09:00:00Z", now), "Jan 5, 2026");
    assert_eq!(count_label(1), "1 commit");
    assert_eq!(count_label(200), "200+ commits");
}

#[test]
fn refs_read_like_describe_ref() {
    assert_eq!(describe_ref("HEAD -> refs/heads/main"), ("main".to_owned(), true));
    assert_eq!(describe_ref("tag: v1.0"), ("v1.0".to_owned(), false));
    assert_eq!(describe_ref("refs/remotes/origin/main"), ("origin/main".to_owned(), false));
}

fn commit(hash: &str, author: &str, subject: &str) -> Value {
    json!({ "hash": hash, "shortHash": &hash[..7], "parents": [], "refs": [], "author": author, "date": "2026-11-25T10:00:00Z", "subject": subject })
}

#[test]
fn a_history_tab_reads_the_log_and_a_commit_opens_its_file_diff() {
    let mut s = connected();
    s.apply(read_reply(2, Value::Null));
    s.apply(read_reply(3, Value::Null));
    s.apply(projects_frame());
    for call in 4..=10 {
        s.apply(read_reply(call, Value::Null));
    }
    s.workspace.step(model::workspace::Event::AddGitHistoryTab("/w/alpha".into(), None));
    s.history_tick();
    let (path, tag) = s.history.unsent.pop().expect("a log read");
    assert_eq!(path, "/git/log");
    let log = json!([commit("aaaaaaa1", "Ada", "link the guide"), commit("bbbbbbb2", "Bo", "add utils")]);
    s.history_replied(tag, Ok(log));
    assert_eq!(s.visible_commits("/w/alpha").len(), 2);
    s.history.filters.entry("/w/alpha".into()).or_default().author = Some("Bo".into());
    assert_eq!(s.visible_commits("/w/beta").len(), 0, "another working copy keeps its own filters");
    assert!(s.history_filters("/w/beta").author.is_none());
    assert_eq!(s.visible_commits("/w/alpha").iter().map(|c| c.subject.as_str()).collect::<Vec<_>>(), ["add utils"]);
    s.select_commit("/w/alpha", "bbbbbbb2");
    let (path, tag) = s.history.unsent.pop().expect("a detail read");
    assert_eq!(path, "/git/commit-detail");
    s.history_replied(tag, Ok(json!({ "hash": "bbbbbbb2", "authorEmail": "bo@x", "message": "add utils", "parents": [], "files": [{ "path": "src/utils.ts", "oldPath": null, "status": "added", "added": 3, "removed": 0 }], "filesTruncated": false })));
    let file = match s.history.details.get(&("/w/alpha".to_owned(), "bbbbbbb2".to_owned())) {
        Some(Read::Ready(d)) => d.files[0].clone(),
        other => panic!("{other:?}"),
    };
    s.open_commit_file("/w/alpha", "bbbbbbb2", &file);
    s.history_tick();
    let (path, tag) = s.history.unsent.pop().expect("a file diff read");
    assert_eq!(path, "/git/commit-file-diff");
    let HistoryFetch::FileDiff { tab } = tag.clone() else { panic!() };
    s.history_replied(tag, Ok(json!({ "diff": "diff --git a/src/utils.ts b/src/utils.ts\nnew file mode 100644\n--- /dev/null\n+++ b/src/utils.ts\n@@ -0,0 +1 @@\n+export {}\n", "tooLarge": false })));
    assert_eq!(s.history.diff_views[&tab].files()[0].additions(), 1);
}

#[test]
fn a_stale_log_reply_is_ignored() {
    let mut s = connected();
    s.refresh_log("/r");
    s.refresh_log("/r");
    let (_, second) = s.history.unsent.pop().unwrap();
    let (_, first) = s.history.unsent.pop().unwrap();
    s.history_replied(second, Ok(json!([commit("ccccccc3", "A", "new")])));
    s.history_replied(first, Ok(json!([])));
    assert_eq!(s.visible_commits("/r").len(), 1);
}
