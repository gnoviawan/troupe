//! The client half of the host's session log contract (`cursor.ts`).
//!
//! The host stamps every session event with a gap-free `seq`. A subscribe or
//! snapshot reply carries `headSeq`, the frontier at registration. Events at
//! or below it are replay, where holes are allowed (non-durable events never
//! reach the log). Events above it are live and must arrive as `last + 1`;
//! anything else means an event was lost, and the client installs a snapshot
//! instead of waiting for it.

use troupe_protocol::ws::WsEvent;

/// Where a session's stream stands on this client.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Cursor {
    /// The log incarnation `last_seq` belongs to; `None` before the first reply.
    pub log_epoch: Option<String>,
    /// The last applied seq.
    pub last_seq: u64,
    /// The registration frontier of the last subscribe or snapshot.
    pub head_seq: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    Duplicate,
    Apply,
    Gap,
}

impl Cursor {
    #[must_use]
    pub fn judge(&self, seq: u64) -> Verdict {
        if seq <= self.last_seq {
            Verdict::Duplicate
        } else if seq <= self.head_seq || seq == self.last_seq.max(self.head_seq) + 1 {
            Verdict::Apply
        } else {
            Verdict::Gap
        }
    }
}

/// Events held at most while one subscribe or snapshot is in flight.
pub const HELD_LIMIT: usize = 2_048;

/// What arrived for a session while its subscribe or snapshot was in flight.
#[derive(Clone, Debug, PartialEq)]
pub enum HeldItem {
    Event(WsEvent),
    /// The host's `gap` frame for the session.
    Gap,
}

/// Frames that overtook a reply on the wire, judged once the reply says
/// where the replay ends.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Held {
    pub items: Vec<HeldItem>,
    /// The buffer overflowed, so the stream has a hole.
    pub lost: bool,
}

impl Held {
    pub fn push(&mut self, item: HeldItem) {
        if self.items.len() >= HELD_LIMIT {
            self.lost = true;
        } else {
            self.items.push(item);
        }
    }
}
