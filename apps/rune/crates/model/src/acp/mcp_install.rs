//! The agent's MCP install requests and the catalog install entry of
//! `slices/mcp.ts` and `lib/acp-mcp-persistence.ts`.

use serde_json::{Map, Value, json};

use super::js::{self, string_of};
use super::mcp::{InstallAnswer, McpError, present};
use super::types::Object;
use super::{Effect, State};

impl State {
    /// `answerMcpInstall`.
    pub(crate) fn answer_mcp_install(&mut self, answer: InstallAnswer, fx: &mut Vec<Effect>) {
        let InstallAnswer {
            request_id,
            decision,
            inputs,
            secrets: secret_values,
            plan_id,
            digest: launch_digest,
        } = answer;
        let Some(pending) = self.pending_mcp_installs.get_mut(&request_id) else {
            return;
        };
        if pending.get("result").is_some_and(js::truthy) {
            return;
        }
        if decision == "approve" {
            pending["saving"] = json!(true);
            pending["error"] = Value::Null;
        }
        let mut request = Map::new();
        request.insert("requestId".into(), json!(request_id));
        request.insert(
            "sessionId".into(),
            pending.get("sessionId").cloned().unwrap_or_default(),
        );
        request.insert("decision".into(), json!(decision));
        for (key, value) in [("inputs", inputs), ("secretValues", secret_values)] {
            if let Some(value) = value.filter(|value| !value.is_null()) {
                request.insert(key.into(), value);
            }
        }
        for (key, value) in [("planId", plan_id), ("launchDigest", launch_digest)] {
            let value = value
                .filter(|value| !value.is_null())
                .or_else(|| pending.get(key).cloned())
                .unwrap_or_default();
            request.insert(key.into(), value);
        }
        fx.push(Effect::AnswerMcpInstall {
            request: Value::Object(request),
            decision,
        });
    }

    /// `_onMcpInstallRequest`: a request whose outcome is in stays as is.
    pub(crate) fn on_mcp_install_request(&mut self, event: Value) {
        let request_id = string_of(event.get("requestId"));
        let resolved = self
            .pending_mcp_installs
            .get(&request_id)
            .and_then(|previous| previous.get("result"))
            .is_some_and(js::truthy);
        if resolved {
            return;
        }
        let mut pending = Map::new();
        for key in [
            "requestId",
            "agentId",
            "sessionId",
            "name",
            "title",
            "transport",
            "form",
            "consent",
            "planId",
            "launchDigest",
        ] {
            if let Some(value) = event.get(key) {
                pending.insert(key.into(), value.clone());
            }
        }
        let error = present(event.get("error")).cloned().unwrap_or_default();
        pending.insert("error".into(), error);
        pending.insert("saving".into(), json!(false));
        pending.insert("result".into(), Value::Null);
        self.pending_mcp_installs
            .insert(request_id, Value::Object(pending));
    }

    /// `_onMcpInstallResolved`: a declined or cancelled request goes; an
    /// installed one keeps its outcome for the card.
    pub(crate) fn on_mcp_install_resolved(&mut self, event: Value) {
        let request_id = string_of(event.get("requestId"));
        let Some(previous) = self.pending_mcp_installs.get_mut(&request_id) else {
            return;
        };
        let outcome = event.get("outcome").and_then(Value::as_str);
        if matches!(outcome, Some("declined" | "cancelled")) {
            self.pending_mcp_installs.remove(&request_id);
            return;
        }
        let mut result = Map::new();
        for key in ["outcome", "name", "serverId", "nextSteps", "oauth"] {
            if let Some(value) = event.get(key) {
                result.insert(key.into(), value.clone());
            }
        }
        previous["saving"] = json!(false);
        previous["error"] = Value::Null;
        previous["result"] = Value::Object(result);
    }

    /// The host refused an install answer: the card can be answered again.
    pub(super) fn on_mcp_install_answered(
        &mut self,
        request: &Value,
        decision: &str,
        outcome: Result<Value, McpError>,
        fx: &mut Vec<Effect>,
    ) {
        let Err(error) = outcome else {
            return;
        };
        let request_id = string_of(request.get("requestId"));
        if let Some(current) = self.pending_mcp_installs.get_mut(&request_id) {
            current["saving"] = json!(false);
        }
        fx.push(Effect::warn(
            "acp-store.answerMcpInstall",
            format!("mcp install {decision} failed ({})", error.name()),
        ));
        fx.push(error.rejected());
    }
}

/// A plain `Error` the action rejects with.
pub(super) fn plain_rejection(message: impl std::fmt::Display) -> Effect {
    Effect::ActionRejected {
        message: format!("Error: {message}"),
    }
}

/// `uniqueName`.
fn unique_name(proposed: &str, taken: &[Object]) -> String {
    let taken = |name: &str| {
        taken
            .iter()
            .any(|server| server.get("name").and_then(Value::as_str) == Some(name))
    };
    if !taken(proposed) {
        return proposed.to_owned();
    }
    let mut suffix = 2;
    while taken(&format!("{proposed}-{suffix}")) {
        suffix += 1;
    }
    format!("{proposed}-{suffix}")
}

/// `catalogInstallEntry`: the registry entry of a catalog install, its
/// secrets filled in.
pub(super) fn catalog_entry(
    result: &Value,
    taken: &[Object],
    id: String,
) -> Result<Object, String> {
    let provenance = present(result.get("provenance"))
        .ok_or("A catalog install result carries its provenance")?;
    let name = unique_name(&string_of(result.get("name")), taken);
    let mut catalog = Map::new();
    catalog.insert("source".into(), json!("mcp-registry"));
    for (key, value) in provenance.as_object().into_iter().flatten() {
        catalog.insert(key.clone(), value.clone());
    }
    catalog.insert(
        "launchDigest".into(),
        result.get("launchDigest").cloned().unwrap_or_default(),
    );
    let config = result.get("config").cloned().unwrap_or_default();
    let secrets = result.get("secrets").and_then(Value::as_array);
    let filled = |slots: Option<&Value>, slot: &str| -> Result<Value, String> {
        let mut pairs = Vec::new();
        for entry in slots.and_then(Value::as_array).into_iter().flatten() {
            let name = string_of(entry.get("name"));
            let value = secrets
                .into_iter()
                .flatten()
                .find(|secret| {
                    secret.get("slot").and_then(Value::as_str) == Some(slot)
                        && secret.get("name").and_then(Value::as_str) == Some(name.as_str())
                })
                .and_then(|secret| secret.get("value").cloned())
                .ok_or_else(|| format!("The install result has no value for {slot} {name}"))?;
            pairs.push(json!({ "name": name, "value": value }));
        }
        Ok(Value::Array(pairs))
    };
    let mut entry = Map::new();
    entry.insert("id".into(), json!(id));
    let transport = config.get("transport").cloned().unwrap_or_default();
    if transport == json!("stdio") {
        entry.insert("type".into(), json!("stdio"));
        entry.insert("name".into(), json!(name));
        for key in ["command", "args"] {
            if let Some(value) = config.get(key) {
                entry.insert(key.into(), value.clone());
            }
        }
        entry.insert("env".into(), filled(config.get("env"), "env")?);
    } else {
        entry.insert("type".into(), transport);
        entry.insert("name".into(), json!(name));
        if let Some(url) = config.get("url") {
            entry.insert("url".into(), url.clone());
        }
        entry.insert("headers".into(), filled(config.get("headers"), "header")?);
    }
    entry.insert("enabled".into(), json!(true));
    entry.insert("catalog".into(), Value::Object(catalog));
    Ok(entry)
}
