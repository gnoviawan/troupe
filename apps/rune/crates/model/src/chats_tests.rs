use super::*;

fn reply() -> Vec<Chat> {
    serde_json::from_value(serde_json::json!([
        { "sessionId": "old", "projectId": "matrix-alpha", "title": "Older", "lastActivityAt": 10.0, "status": "active" },
        { "sessionId": "quick", "projectId": null, "title": "", "lastActivityAt": 30.0 },
        { "sessionId": "new", "projectId": "matrix-alpha", "title": "Newer", "lastActivityAt": 20.0 }
    ]))
    .unwrap()
}

#[test]
fn a_reply_lists_newest_first_and_splits_quick_chats() {
    let mut chats = Chats::default();
    chats.replace(reply());
    let alpha: Vec<&str> = chats.of_project("matrix-alpha").map(|c| c.session_id.as_str()).collect();
    assert_eq!(alpha, ["new", "old"]);
    let quick: Vec<&str> = chats.quick().map(Chat::label).collect();
    assert_eq!(quick, ["New chat"]);
    assert!(chats.is_loaded);
}

#[test]
fn a_removed_chat_leaves_the_lists() {
    let mut chats = Chats::default();
    chats.replace(reply());
    chats.remove("new");
    assert_eq!(chats.of_project("matrix-alpha").count(), 1);
    assert!(chats.get("new").is_none());
}
