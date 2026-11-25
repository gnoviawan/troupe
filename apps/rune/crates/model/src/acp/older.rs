//! `loadOlderMessages` (`slices/transcript.ts`): scrolling up past the live
//! window's head pages the next older messages in from the durable payload.
//! The read is an effect; a cached copy that stops short of the head is read
//! again from the host first. The page lands only where it still abuts the
//! head, without the bubbles the window already shows, and the window keeps
//! it (`backfillCounts`) until the reader returns to the live edge.
//!
//! Not here: paging into the bands an in-chat switch spliced before the
//! chat's own head (`olderInSourceBand`, `newestOfPreviousBand`). Only the
//! live switch splice makes such a band, and it is not ported.

use std::collections::BTreeSet;

use serde_json::Value;

use super::types::{ChatMessage, Role};
use super::window::{PayloadRead, PayloadReadOutcome, Reload};
use super::{Effect, State};

const SOURCE: &str = "acp.loadOlderMessages";
/// `SWITCH_SPLICE_ID_PREFIX`.
const SPLICE: &str = "switch-splice:";
/// `TWIN_SEAM_SLACK`: how far from the seam a streaming bubble's persisted
/// twin may sit and still fold by prefix.
const TWIN_SEAM_SLACK: usize = 4;

/// `hostSeqOf`.
fn host_seq_of(message: &ChatMessage) -> Option<f64> {
    message.host_seq.or(message.seq)
}

/// `payloadReachesHead`: the payload holds the window's oldest bubble. A
/// head with no host seq (this client's own prompt) is never proven covered.
fn payload_reaches_head(payload: &Value, head: &ChatMessage) -> bool {
    if head.id.starts_with(SPLICE) {
        return true;
    }
    let messages = payload.get("messages").and_then(Value::as_array);
    if messages
        .into_iter()
        .flatten()
        .any(|message| message.get("id").and_then(Value::as_str) == Some(head.id.as_str()))
    {
        return true;
    }
    let last_seq = payload
        .get("metadata")
        .and_then(|metadata| metadata.get("lastSeq"))
        .and_then(Value::as_f64);
    matches!((head.host_seq, last_seq), (Some(seq), Some(last)) if seq <= last)
}

/// `anchorIndex`: the row that holds the head, by id, else the last row at
/// or below the head's host seq.
fn anchor_index(rows: &[ChatMessage], id: &str, host_seq: Option<f64>) -> Option<usize> {
    if let Some(index) = rows.iter().position(|row| row.id == id) {
        return Some(index);
    }
    let host_seq = host_seq.filter(|seq| seq.is_finite())?;
    rows.iter().rposition(|row| {
        row.seq
            .is_some_and(|seq| seq.is_finite() && seq <= host_seq)
    })
}

/// `transcriptText`.
fn transcript_text(message: &ChatMessage) -> String {
    message
        .blocks
        .iter()
        .filter(|block| block.get("type").and_then(Value::as_str) == Some("text"))
        .filter_map(|block| block.get("text").and_then(Value::as_str))
        .collect()
}

/// `twinFileEvidence`, from the blocks: the mention tokens of the display
/// text belong to the composer's wire framing, which is not ported.
fn file_evidence(message: &ChatMessage) -> BTreeSet<String> {
    let text = |block: &Value, key: &str| {
        block
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned()
    };
    message
        .blocks
        .iter()
        .filter_map(|block| match block.get("type").and_then(Value::as_str) {
            Some("resource_link" | "resource") => {
                let name = block
                    .get("name")
                    .and_then(Value::as_str)
                    .map_or_else(|| text(block, "uri"), str::to_owned);
                Some(format!("file:{name}"))
            }
            Some(kind @ ("image" | "audio")) => Some(format!("{kind}:{}", text(block, "mimeType"))),
            _ => None,
        })
        .collect()
}

/// `isPersistedTwin`, on the text as sent: the canonical form undoes the
/// composer's wire framing, which is not ported.
pub(crate) fn is_persisted_twin(
    persisted: &ChatMessage,
    live: &ChatMessage,
    seam_aligned: bool,
) -> bool {
    if persisted.role != live.role {
        return false;
    }
    let persisted_text = transcript_text(persisted);
    let persisted_text = persisted_text.trim();
    let live_text = transcript_text(live);
    let live_text = live_text.trim();
    if persisted_text == live_text {
        if persisted_text.is_empty() {
            return persisted.blocks == live.blocks;
        }
        return persisted.role != Role::User || file_evidence(persisted) == file_evidence(live);
    }
    seam_aligned && live.streaming && !live_text.is_empty() && persisted_text.starts_with(live_text)
}

impl State {
    /// `loadOlderMessages`. One load per session at a time.
    pub(crate) fn load_older_messages(
        &mut self,
        session_id: String,
        count: usize,
        fx: &mut Vec<Effect>,
    ) {
        if !self.book.loading_older.insert(session_id.clone()) {
            return;
        }
        fx.push(Effect::ReadSessionPayload {
            session_id,
            read: PayloadRead::Older { count },
        });
    }

    /// The payload read answered. A failed read keeps the current view: the
    /// reader can scroll up again.
    pub(crate) fn on_older_payload(
        &mut self,
        session_id: String,
        count: usize,
        outcome: PayloadReadOutcome,
        fx: &mut Vec<Effect>,
    ) {
        let Ok(Some(payload)) = outcome else {
            self.book.loading_older.remove(&session_id);
            return;
        };
        let head = self.messages.get(&session_id).and_then(|list| list.first());
        if head.is_some_and(|head| !payload_reaches_head(&payload, head)) {
            return fx.push(Effect::ReloadSessionPayload {
                session_id,
                read: Reload::Older {
                    count,
                    cached: payload,
                },
            });
        }
        self.page_older(&session_id, count, &payload, fx);
    }

    /// `reloadSessionPayloadFromHost` answered: its payload, else the cached
    /// one when the host has none. A failed re-read waits for the next
    /// scroll-up rather than page from a stale copy.
    pub(crate) fn on_older_reload(
        &mut self,
        session_id: String,
        count: usize,
        cached: Value,
        outcome: PayloadReadOutcome,
        fx: &mut Vec<Effect>,
    ) {
        match outcome {
            Ok(payload) => {
                let payload = payload.unwrap_or(cached);
                self.page_older(&session_id, count, &payload, fx);
            }
            Err(_) => {
                self.book.loading_older.remove(&session_id);
                let short: String = session_id.chars().take(8).collect();
                fx.push(Effect::warn(
                    SOURCE,
                    format!(
                        "Host re-read for session {short}… failed; scroll-back waits for the next scroll-up instead of paging from a stale copy"
                    ),
                ));
            }
        }
    }

    /// The page before the window's head, without the bubbles the window
    /// already shows, lands in front of it.
    fn page_older(
        &mut self,
        session_id: &str,
        count: usize,
        payload: &Value,
        fx: &mut Vec<Effect>,
    ) {
        self.book.loading_older.remove(session_id);
        let Some(head) = self
            .messages
            .get(session_id)
            .and_then(|list| list.first())
            .cloned()
        else {
            return;
        };
        if head.id.starts_with(SPLICE) {
            return;
        }
        let rows = super::history_install::renderable_rows(payload);
        let Some(oldest) = anchor_index(&rows, &head.id, host_seq_of(&head)) else {
            return fx.push(Effect::warn(
                SOURCE,
                format!(
                    "Oldest live message for session {session_id} is not in the persisted payload and could not be anchored by seq; older history will not load"
                ),
            ));
        };
        // Only a head anchored on its renderer seq can leave its persisted
        // twins inside the page under other ids.
        let mut seq_anchored: Option<BTreeSet<String>> = None;
        if rows[oldest].id != head.id {
            fx.push(Effect::warn(
                SOURCE,
                format!(
                    "Oldest live message id for session {session_id} missing from persisted payload; anchored scroll-back by seq at index {oldest}"
                ),
            ));
            if head.host_seq.is_none() {
                seq_anchored = Some(rows.iter().map(|row| row.id.clone()).collect());
            }
        }
        let page = &rows[oldest.saturating_sub(count)..oldest];
        // At the head of the chat's own band: the band an in-chat switch
        // spliced before it is not ported, so there is nothing older.
        if page.is_empty() {
            return;
        }
        let live = &self.messages[session_id];
        let live_ids: BTreeSet<&str> = live.iter().map(|m| m.id.as_str()).collect();
        let live_host_seqs: Vec<Option<f64>> = live.iter().map(|m| m.host_seq).collect();
        let mut twin_used = vec![false; live.len()];
        let mut kept: Vec<ChatMessage> = Vec::new();
        for (from_end, candidate) in page.iter().rev().enumerate() {
            if live_ids.contains(candidate.id.as_str()) {
                continue;
            }
            if let Some(persisted) = &seq_anchored {
                if candidate.seq.is_some() && live_host_seqs.contains(&candidate.seq) {
                    continue;
                }
                let twin = live.iter().enumerate().position(|(index, message)| {
                    let gap = (live.len() - 1 - index).checked_sub(from_end);
                    !twin_used[index]
                        && message.host_seq.is_none()
                        && !persisted.contains(&message.id)
                        && !message.id.starts_with(SPLICE)
                        && is_persisted_twin(
                            candidate,
                            message,
                            gap.is_some_and(|gap| gap <= TWIN_SEAM_SLACK),
                        )
                });
                if let Some(twin) = twin {
                    twin_used[twin] = true;
                    continue;
                }
            }
            kept.push(candidate.clone());
        }
        if kept.is_empty() {
            return;
        }
        kept.reverse();
        *self
            .book
            .backfill_counts
            .entry(session_id.to_owned())
            .or_default() += kept.len();
        let list = self.messages.get_mut(session_id).expect("read above");
        kept.append(list);
        *list = kept;
    }
}
