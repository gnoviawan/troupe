use serde_json::json;

use super::preserved_entry;

#[test]
fn a_preserved_entry_needs_this_tab_id_and_a_claim() {
    let reply = json!({
        "projectId": "alpha",
        "terminals": [
            { "id": "pty-1", "shell": "/bin/bash", "cwd": "/w/alpha", "pid": 77, "cols": 100, "rows": 30, "claim": "c1", "hasLiveAttachment": false },
            { "id": "pty-2", "shell": "/bin/bash", "cwd": "/w/alpha", "pid": 78, "cols": 80, "rows": 24, "hasLiveAttachment": true }
        ]
    });
    let found = preserved_entry(&reply, "pty-1").expect("pty-1 has a claim");
    assert_eq!((found.pid, found.cols, found.rows, found.claim.as_str()), (77, 100, 30, "c1"));
    assert!(preserved_entry(&reply, "pty-2").is_none(), "no claim, nothing to attach with");
    assert!(preserved_entry(&reply, "pty-3").is_none(), "not preserved");
}

#[test]
fn the_screens_rows_reach_the_accessibility_tree_as_text_runs() {
    let mut rt = crate::term_select::tests::shown(b"hello rows\r\nsecond line");
    rt.frame(0.1);
    let tree = rt.accessibility_tree();
    let runs: Vec<String> = tree
        .nodes
        .iter()
        .filter(|(_, node)| format!("{:?}", node.role()) == "TextRun")
        .filter_map(|(_, node)| node.value().map(str::to_owned))
        .collect();
    assert_eq!(runs.get(..3), Some(&["hello rows\n".to_owned(), "second line\n".to_owned(), "\n".to_owned()][..]), "{runs:?}");
    assert_eq!(runs.len(), 24, "one run per screen row");
}
