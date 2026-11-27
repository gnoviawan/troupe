//! The JSON Schema of every contract type, the input of
//! `scripts/protocol/gen-ts.mjs`.

use schemars::generate::SchemaSettings;
use schemars::transform::{transform_subschemas, Transform};
use schemars::{JsonSchema, Schema, SchemaGenerator};
use serde_json::{json, Map, Value};

use crate::{terminal, version, ws};

/// Path of the committed schema, relative to the crate root.
pub const SCHEMA_PATH: &str = "schema/protocol.schema.json";

/// Marks a property whose `null` differs from absent (`Option<Option<T>>`).
const NULLABLE_KEY: &str = "x-nullable";

/// The schemas describe what each side writes. An optional property is left
/// out when empty, never written as `null`, so its `null` branch is dropped
/// unless the property carries [`NULLABLE_KEY`].
#[derive(Clone)]
struct OptionalIsNeverNull;

impl Transform for OptionalIsNeverNull {
    fn transform(&mut self, schema: &mut Schema) {
        transform_subschemas(self, schema);
        let Some(object) = schema.as_object_mut() else {
            return;
        };
        let required: Vec<Value> = object
            .get("required")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let Some(Value::Object(properties)) = object.get_mut("properties") else {
            return;
        };
        for (name, property) in properties.iter_mut() {
            let Some(property) = property.as_object_mut() else {
                continue;
            };
            if property.remove(NULLABLE_KEY).is_some() || required.contains(&json!(name)) {
                continue;
            }
            drop_null_branch(property);
        }
    }
}

fn drop_null_branch(property: &mut Map<String, Value>) {
    let null = json!({ "type": "null" });
    if let Some(Value::Array(types)) = property.get_mut("type") {
        types.retain(|kind| kind != "null");
        if let [only] = types.as_slice() {
            let only = only.clone();
            property.insert("type".into(), only);
        }
    }
    if let Some(Value::Array(values)) = property.get_mut("enum") {
        values.retain(|value| !value.is_null());
        if let [only] = values.as_slice() {
            let only = only.clone();
            property.remove("enum");
            property.insert("const".into(), only);
        }
    }
    if let Some(Value::Array(branches)) = property.get_mut("anyOf") {
        branches.retain(|branch| *branch != null);
        if let [only] = branches.as_slice() {
            let Value::Object(only) = only.clone() else {
                return;
            };
            property.remove("anyOf");
            property.extend(only);
        }
    }
}

fn add<T: JsonSchema>(generator: &mut SchemaGenerator) {
    generator.subschema_for::<T>();
}

/// One document: the protocol constants and a `$defs` entry per type.
#[must_use]
pub fn protocol_schema() -> Value {
    let mut generator = SchemaSettings::draft2020_12()
        .for_serialize()
        .with_transform(OptionalIsNeverNull)
        .into_generator();
    let g = &mut generator;
    add::<ws::WsRequestType>(g);
    add::<ws::WsEventType>(g);
    add::<ws::WsWatchName>(g);
    add::<ws::WsErrorCode>(g);
    add::<ws::ReliabilityTier>(g);
    add::<ws::WsRequest>(g);
    add::<ws::WsServerFrame>(g);
    add::<ws::WsWatchReply>(g);
    add::<ws::WatchResyncDelta>(g);
    add::<ws::ChatHistoryWatchDelta>(g);
    add::<ws::AgentsWatchDelta>(g);
    add::<ws::ChatActivityWatchState>(g);
    add::<ws::ChatActivityWatchDelta>(g);
    add::<ws::FsWatchState>(g);
    add::<ws::FsWatchDelta>(g);
    add::<ws::StoreWatchState>(g);
    add::<ws::TerminalsWatchState>(g);
    add::<ws::SshWatchState>(g);
    add::<ws::SshWatchDelta>(g);
    add::<ws::SshHostKeyAnswer>(g);
    add::<ws::AcpAuthenticatePayload>(g);
    add::<ws::AcpAuthenticateReply>(g);
    add::<ws::DeleteSessionPayload>(g);
    add::<ws::RecordAgentSwitchPayload>(g);
    add::<ws::ChatReadMark>(g);
    add::<ws::ChatReadMarkReply>(g);
    add::<ws::SessionSubscribeReply>(g);
    add::<ws::SessionSnapshotEvent>(g);
    add::<ws::SessionCursorReply>(g);
    add::<ws::PersistedSessionSummary>(g);
    add::<ws::UserPromptEvent>(g);
    add::<ws::StoreReadPayload>(g);
    add::<ws::StoreReadReply>(g);
    add::<ws::StoreWritePayload>(g);
    add::<ws::StoreDeviceWriteReply>(g);
    add::<ws::StoreSharedWriteReply>(g);
    add::<ws::StoreDeletePayload>(g);
    add::<ws::StoreDeleteReply>(g);
    add::<ws::AddProjectPayload>(g);
    add::<ws::UpdateProjectPayload>(g);
    add::<ws::RemoveProjectPayload>(g);
    add::<ws::WsCanvasMountReply>(g);
    add::<ws::WsCanvasReplyPayload>(g);
    add::<ws::WsAgentSummary>(g);
    add::<ws::BackgroundCommandList>(g);
    add::<terminal::WebTerminalRequestType>(g);
    add::<terminal::WebTerminalErrorCode>(g);
    add::<terminal::WebTerminalRequest>(g);
    add::<terminal::WebTerminalFrame>(g);
    add::<terminal::TerminalSpawnOptions>(g);
    add::<terminal::SpawnedTerminal>(g);
    add::<terminal::TerminalAttachResult>(g);
    add::<terminal::RotatedClaim>(g);
    let defs: Map<String, Value> = generator.take_definitions(true).into_iter().collect();
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "title": "troupe-protocol",
        "x-constants": {
            "WS_PROTOCOL_VERSION": version::WS_PROTOCOL_VERSION,
            "MAX_CLIENT_ID_LEN": version::MAX_CLIENT_ID_LEN,
            "MAX_COMMAND_ID_LEN": version::MAX_COMMAND_ID_LEN,
        },
        "$defs": defs,
    })
}
