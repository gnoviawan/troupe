use agent_client_protocol::schema::v1::{ContentBlock, ContentChunk, SessionUpdate, TextContent};

use super::QuotaHits;

const LIMIT: &str = "Usage Limit Reached\n\nYou have reached your current quota for this period. Your limit will reset in 1 hour.";

fn chunk(text: &str) -> SessionUpdate {
    SessionUpdate::AgentMessageChunk(ContentChunk::new(ContentBlock::Text(TextContent::new(
        text,
    ))))
}

#[test]
fn a_disabled_agent_never_marks_a_session() {
    let mut hits = QuotaHits::default();
    assert!(!hits.note("s1", &chunk(LIMIT)));
    assert_eq!(hits.take("s1"), None);
}

#[test]
fn the_limit_message_marks_its_session_until_taken() {
    let mut hits = QuotaHits::default();
    hits.enable();
    assert!(!hits.note("s1", &chunk("Working on it.")));
    assert!(hits.note("s1", &chunk(LIMIT)));
    assert!(hits.take("s2").is_none());
    let hit = hits.take("s1").expect("marked");
    assert!(hit.reset_at.is_some());
    assert!(hits.take("s1").is_none());
}

#[test]
fn a_new_turn_clears_an_old_mark() {
    let mut hits = QuotaHits::default();
    hits.enable();
    assert!(hits.note("s1", &chunk(LIMIT)));
    hits.clear("s1");
    assert!(hits.take("s1").is_none());
}

#[test]
fn a_user_or_thought_chunk_is_not_the_limit_message() {
    let mut hits = QuotaHits::default();
    hits.enable();
    let text = ContentChunk::new(ContentBlock::Text(TextContent::new(LIMIT)));
    assert!(!hits.note("s1", &SessionUpdate::UserMessageChunk(text.clone())));
    assert!(!hits.note("s1", &SessionUpdate::AgentThoughtChunk(text)));
}
