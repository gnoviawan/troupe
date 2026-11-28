//! Composer configs are read on every listing and resolution.
use crate::{acp::config::AgentConfig, web::store::WebStore};
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredAgent {
    id: String,
    config_id: Option<String>,
    name: String,
    command: String,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    env: HashMap<String, String>,
    #[serde(default)]
    allow_terminal: bool,
}

pub(super) fn read(store: Option<&WebStore>) -> Option<Vec<AgentConfig>> {
    let value = match store?.peek("shared/lww/acp/agents") {
        Ok(value) => value?,
        Err(error) => {
            log::warn!("[orchestrator] saved agent configs unreadable: {error}");
            return None;
        }
    };
    parse(value)
}

fn parse(value: serde_json::Value) -> Option<Vec<AgentConfig>> {
    let entries = value.get("data")?.as_array()?;
    let mut seen = std::collections::HashSet::new();
    Some(
        entries
            .iter()
            .filter_map(|entry| {
                let entry: StoredAgent = serde_json::from_value(entry.clone()).ok()?;
                let key = entry
                    .config_id
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .unwrap_or(entry.id.trim());
                // Two configs under one key read as the first, as the composer lists them.
                if key.is_empty() || entry.command.trim().is_empty() || !seen.insert(key.to_owned())
                {
                    return None;
                }
                Some(AgentConfig {
                    config_id: Some(key.to_owned()),
                    name: entry.name,
                    command: entry.command,
                    args: entry.args,
                    env: entry.env,
                    allow_terminal: entry.allow_terminal,
                })
            })
            .collect(),
    )
}

#[cfg(test)]
#[path = "stored_agents_tests.rs"]
mod tests;
