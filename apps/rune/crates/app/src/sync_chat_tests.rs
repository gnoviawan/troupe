use super::*;

#[test]
fn malformed_chat_does_not_hide_valid_neighbors() {
    let chats = parse_chat_list(serde_json::json!([
        {"sessionId": "one", "lastActivityAt": 1},
        {"sessionId": "bad", "title": 42},
        {"sessionId": "two", "title": null, "lastActivityAt": 2}
    ])).unwrap();
    assert_eq!(chats.len(), 2);
    assert_eq!(chats[0].session_id, "one");
    assert_eq!(chats[1].session_id, "two");
    assert!(chats.iter().all(|chat| chat.title == "Untitled Chat"));
}
