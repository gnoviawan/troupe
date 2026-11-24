use serde_json::{Value, json};

use super::tool_call::{Summary, describe, readable_output};

fn obj(v: Value) -> model::acp::types::Object {
    serde_json::from_value(v).unwrap()
}

fn summary(verb: &str, primary: &str, detail: Option<&str>, diff_stat: Option<(usize, usize)>) -> Summary {
    Summary { verb: verb.into(), primary: primary.into(), detail: detail.map(Into::into), diff_stat }
}

#[test]
fn describes_each_kind_as_the_renderer_does() {
    let read = obj(json!({ "toolCallId": "r", "kind": "read", "status": "completed", "rawInput": { "path": "/x/src/App.tsx", "offset": 10, "limit": 5 } }));
    assert_eq!(describe(&read), summary("Read", "App.tsx", Some("L10-15"), None));
    let edit = obj(json!({ "toolCallId": "e", "kind": "edit", "status": "in_progress", "content": [
        { "type": "diff", "path": "/x/a.ts", "oldText": "a\nb\n", "newText": "a\nB\nc\n" }
    ] }));
    assert_eq!(describe(&edit), summary("Editing", "a.ts", Some("+2 \u{2212}1"), Some((2, 1))));
    let restored = obj(json!({ "toolCallId": "e2", "kind": "edit", "status": "completed", "title": "Edit a.ts", "diffStat": { "added": 4, "removed": 0 } }));
    assert_eq!(describe(&restored), summary("Edited", "Edit a.ts", Some("+4"), Some((4, 0))));
    let run = obj(json!({ "toolCallId": "x", "kind": "execute", "status": "completed", "title": "Run tests", "rawInput": { "command": "bun test" } }));
    assert_eq!(describe(&run), summary("Ran", "bun test", None, None));
    let search = obj(json!({ "toolCallId": "s", "kind": "search", "status": "pending", "rawInput": { "pattern": "/c/" } }));
    assert_eq!(describe(&search), summary("Searching", "/c/", None, None));
    let other = obj(json!({ "toolCallId": "o", "status": "completed", "title": "mcp__x__y" }));
    assert_eq!(describe(&other), summary("", "mcp__x__y", None, None));
    let dispatch = obj(json!({ "toolCallId": "d", "kind": "think", "subagent": { "title": "Explore" } }));
    assert_eq!(describe(&dispatch), summary("", "Explore", None, None));
}

#[test]
fn readable_output_skips_the_envelope() {
    assert_eq!(readable_output(Some(&json!("  done \n"))), "done");
    assert_eq!(readable_output(Some(&json!({ "metadata": { "id": 1 }, "stdout": "ok" }))), "ok");
    assert_eq!(readable_output(Some(&json!({ "metadata": { "diff": "@@ -1 +1 @@" } }))), "@@ -1 +1 @@");
    assert_eq!(readable_output(Some(&json!({ "fileDiff": { "patch": "p" } }))), "p");
    assert_eq!(readable_output(Some(&json!({ "id": 3 }))), "");
    assert_eq!(readable_output(None), "");
}

#[test]
fn a_run_rolls_up_by_category() {
    let calls: Vec<model::acp::types::Object> = [
        json!({ "toolCallId": "a", "kind": "read" }),
        json!({ "toolCallId": "b", "kind": "edit", "content": [{ "type": "diff", "path": "a", "oldText": "x\n", "newText": "y\n" }] }),
        json!({ "toolCallId": "c", "kind": "execute" }),
        json!({ "toolCallId": "d", "kind": "execute" }),
        json!({ "toolCallId": "e", "kind": "search" }),
        json!({ "toolCallId": "f", "kind": "fetch" }),
    ]
    .into_iter()
    .map(obj)
    .collect();
    let refs: Vec<&model::acp::types::Object> = calls.iter().collect();
    assert_eq!(super::tool_call::summarize(&refs[..5], 0), Some(("Read 1 file, edited 1 file, ran 2 commands, ran 1 search".into(), 1, 1)));
    assert_eq!(super::tool_call::summarize(&refs, 2), Some(("Thought 2 times, read 1 file, edited 1 file, ran 2 commands, and 2 more".into(), 1, 1)));
    assert_eq!(super::tool_call::summarize(&[], 0), None);
}
