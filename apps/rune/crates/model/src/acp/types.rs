//! The records the chat model holds, typed where the model reads them. Each
//! keeps the fields a later slice owns in a flattened `rest`, so a record
//! round-trips through the model unchanged.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{Map, Value};

/// A JSON object: a content block or a tool call, as the agent sent it.
pub type Object = Map<String, Value>;

/// A field that is absent (`None`), `null` (`Some(None)`) or set.
pub mod nullable {
    use super::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<T: Serialize, S: Serializer>(
        value: &Option<Option<T>>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        match value {
            Some(inner) => inner.serialize(serializer),
            None => serializer.serialize_none(),
        }
    }

    pub fn deserialize<'de, T: Deserialize<'de>, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<Option<T>>, D::Error> {
        Option::<T>::deserialize(deserializer).map(Some)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionStatus {
    Initializing,
    Active,
    Error,
    Closed,
}

/// A `session/load` replay in flight: `pending` until the first replayed
/// chunk, `streaming` after it replaced the transcript.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Replaying {
    Pending,
    Streaming,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Compaction {
    Started,
    Failed,
}

/// The live OpenCode compaction and retry line. `null` fields are cleared.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpencodeNotice {
    pub compaction: Option<Compaction>,
    pub retry_attempt: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub id: String,
    pub agent_id: String,
    pub status: SessionStatus,
    pub title: Option<String>,
    pub active_turn: bool,
    pub open_turn_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_working: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none", with = "nullable")]
    pub replaying: Option<Option<Replaying>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opencode_notice: Option<OpencodeNotice>,
    pub last_error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none", with = "nullable")]
    pub last_error_code: Option<Option<f64>>,
    #[serde(default, skip_serializing_if = "Option::is_none", with = "nullable")]
    pub last_error_data: Option<Option<Value>>,
    /// The commands the agent that reported them left running.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background_commands: Option<Value>,
    #[serde(flatten)]
    pub rest: Object,
}

impl Session {
    pub fn replaying(&self) -> Option<Replaying> {
        self.replaying.flatten()
    }

    /// `acceptsSessionTranscriptEvents`: a live session, or a closed one
    /// whose `session/load` replay is in flight.
    pub fn accepts_transcript_events(&self) -> bool {
        self.status != SessionStatus::Closed || self.replaying().is_some()
    }

    /// `sessionTurnBusy`: a prompt turn is open, or the agent works on its
    /// own while the chat is live.
    pub fn turn_busy(&self) -> bool {
        if self
            .open_turn_id
            .as_deref()
            .is_some_and(|id| !id.is_empty())
            || self.active_turn
        {
            return true;
        }
        self.agent_working == Some(true)
            && !matches!(self.status, SessionStatus::Closed | SessionStatus::Error)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Agent,
    Thought,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatMessage {
    pub id: String,
    pub role: Role,
    pub blocks: Vec<Value>,
    pub streaming: bool,
    pub timestamp: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seq: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host_seq: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub read_only_guard: Option<String>,
    #[serde(flatten)]
    pub rest: Object,
}

/// Where a record sits in a session's durable log.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogPoint {
    pub log_epoch: String,
    pub seq: u64,
}

impl LogPoint {
    /// `isAfter`: another epoch, or a later seq in the same one.
    pub fn is_after(&self, earlier: &Self) -> bool {
        self.log_epoch != earlier.log_epoch || self.seq > earlier.seq
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageCost {
    pub amount: f64,
    pub currency: String,
}

/// Context window use the agent reported for a session.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
    pub used: f64,
    pub size: f64,
    pub baseline_used: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost: Option<UsageCost>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rate_limit: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<f64>,
    pub source: String,
}

/// One row of a session's plan. An agent sends it as an open object, and
/// its key order shows in the snapshot fence, so it stays one.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PlanEntry(pub Object);

impl PlanEntry {
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.0.get(key)
    }

    /// A string field, as `typeof entry[key] === 'string'`.
    pub fn text(&self, key: &str) -> Option<&str> {
        self.0.get(key).and_then(Value::as_str)
    }

    pub fn content(&self) -> &str {
        self.text("content").unwrap_or_default()
    }
}
