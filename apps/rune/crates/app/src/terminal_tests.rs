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
