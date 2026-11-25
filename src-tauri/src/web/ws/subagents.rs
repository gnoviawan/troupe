//! Subagent WS handlers: `stop_subagent`, and the orchestrator's settings
//! view `subagent_settings_get` / `subagent_settings_set`.

use super::*;
use crate::acp::orchestrator::{
    Launchable, OrchestratorEngine, OrchestratorSettings, ResolvedProfile, SubagentProfile,
};
use crate::acp::subagents::StopSubagentError;

/// `stop_subagent` → `AcpManager::stop_subagent(agent_id, root, subagent)`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct StopSubagentPayload {
    agent_id: crate::acp::AgentId,
    root_session_id: String,
    subagent_id: String,
}

/// Replies `{ "outcome": "sent" | "already_ended" }`. An unknown agent or
/// subagent is `not_found`; a subagent the agent offers no stop for is
/// `unsupported`.
pub(super) async fn handle_stop_subagent(
    id: String,
    payload: &Value,
    acp: &Arc<AcpManager>,
) -> WsReply {
    let parsed: StopSubagentPayload = match serde_json::from_value(payload.clone()) {
        Ok(parsed) => parsed,
        Err(error) => {
            return WsReply::err(
                id,
                WsErrorCode::Unsupported,
                format!(
                    "malformed stop_subagent payload (want agentId, rootSessionId, subagentId): {error}"
                ),
            )
        }
    };
    let result = acp
        .stop_subagent(&parsed.agent_id, parsed.root_session_id, parsed.subagent_id)
        .await;
    match result {
        Ok(outcome) => WsReply::ok(id, Some(json!({ "outcome": outcome }))),
        Err(error @ (StopSubagentError::UnknownAgent | StopSubagentError::UnknownSubagent)) => {
            WsReply::err(id, WsErrorCode::NotFound, error.to_string())
        }
        Err(error @ StopSubagentError::NotStoppable) => {
            WsReply::err(id, WsErrorCode::Unsupported, error.to_string())
        }
        Err(StopSubagentError::Failed(error)) => acp_err_to_reply(id, error),
    }
}

/// The orchestrator's settings view: the gates, every profile (stored and
/// implicit) and the legacy per-agent rows.
fn settings_view(engine: &OrchestratorEngine) -> Value {
    let launchable = engine.launchable();
    let settings = engine.settings_view();
    let profiles = launchable.resolve(&settings);
    json!({
        "enabled": settings.enabled,
        "maxRunning": settings.max_running,
        "maxDepth": settings.max_depth,
        "agents": legacy_agents(&launchable, &profiles),
        "profiles": profiles.iter().map(profile_view).collect::<Vec<_>>(),
    })
}

/// `{id, agentId, agentName, enabled, model, mode, configOptions,
/// approvals, description, implicit}`; `agentName` is null for a profile
/// whose agent is not installed.
fn profile_view(resolved: &ResolvedProfile) -> Value {
    let mut view = json!(resolved.profile);
    view["agentName"] = json!(resolved.agent_name);
    view["implicit"] = json!(resolved.implicit);
    view
}

/// Legacy: one row per launchable agent, `{agentId, name, allowed, model,
/// description}`, from the agent's default profile (the one whose id is
/// the agent's key). An agent whose default profile was renamed or deleted
/// has no row. The profiles UI replaces these rows; they go with it.
fn legacy_agents(launchable: &Launchable, profiles: &[ResolvedProfile]) -> Vec<Value> {
    launchable
        .agents()
        .iter()
        .filter_map(|agent| {
            let profile = &profiles
                .iter()
                .find(|resolved| {
                    resolved.profile.id == agent.key && resolved.profile.agent_id == agent.key
                })?
                .profile;
            Some(json!({
                "agentId": agent.key,
                "name": agent.name,
                "allowed": profile.enabled,
                "model": profile.model,
                "description": profile.description,
            }))
        })
        .collect()
}

fn no_engine(id: String) -> WsReply {
    WsReply::err(
        id,
        WsErrorCode::Unsupported,
        "sub-agents are not available on this host",
    )
}

/// `subagent_settings_get` → the settings view.
pub(super) fn handle_subagent_settings_get(id: String) -> WsReply {
    let Some(engine) = crate::acp::orchestrator::installed() else {
        return no_engine(id);
    };
    WsReply::ok(id, Some(settings_view(&engine)))
}

/// Legacy: one allow switch of a `subagent_settings_set` call.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct AgentAllowPayload {
    agent_id: String,
    allowed: bool,
}

/// Legacy: one default model of a `subagent_settings_set` call; `null`
/// clears it.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct AgentModelPayload {
    agent_id: String,
    model: Option<String>,
}

/// Legacy: one description of a `subagent_settings_set` call; `null` or a
/// blank string clears it.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct AgentDescriptionPayload {
    agent_id: String,
    description: Option<String>,
}

/// One profile of a `subagent_settings_set` call. `previous_id` renames:
/// it names the stored or implicit profile `profile` replaces.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct ProfileUpsertPayload {
    #[serde(default)]
    previous_id: Option<String>,
    profile: SubagentProfile,
}

/// `subagent_settings_set`: a partial of the view; an omitted field keeps
/// its value. Applied in order: `deleteProfiles`, `upsertProfiles`, then
/// the legacy per-agent `agents` / `models` / `descriptions`, which edit
/// the agent's default profile.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct SubagentSettingsPayload {
    #[serde(default)]
    enabled: Option<bool>,
    #[serde(default)]
    max_running: Option<u32>,
    #[serde(default)]
    max_depth: Option<u8>,
    #[serde(default)]
    agents: Option<Vec<AgentAllowPayload>>,
    #[serde(default)]
    models: Option<Vec<AgentModelPayload>>,
    #[serde(default)]
    descriptions: Option<Vec<AgentDescriptionPayload>>,
    #[serde(default)]
    upsert_profiles: Option<Vec<ProfileUpsertPayload>>,
    #[serde(default)]
    delete_profiles: Option<Vec<String>>,
}

/// Apply the payload's fields over `settings`. Agent ids are canonical
/// already ([`apply_settings_patch`]).
fn merge_settings(
    settings: &mut OrchestratorSettings,
    patch: SubagentSettingsPayload,
    launchable: &Launchable,
) -> Result<(), String> {
    if let Some(enabled) = patch.enabled {
        settings.enabled = enabled;
    }
    if let Some(max_running) = patch.max_running {
        settings.max_running = max_running;
    }
    if let Some(max_depth) = patch.max_depth {
        settings.max_depth = max_depth;
    }
    for id in patch.delete_profiles.unwrap_or_default() {
        settings.profiles.retain(|profile| profile.id != id.trim());
    }
    for upsert in patch.upsert_profiles.unwrap_or_default() {
        upsert_profile(settings, upsert, launchable)?;
    }
    for agent in patch.agents.unwrap_or_default() {
        default_profile(settings, &agent.agent_id, launchable).enabled = agent.allowed;
    }
    for agent in patch.models.unwrap_or_default() {
        default_profile(settings, &agent.agent_id, launchable).model = agent.model;
    }
    for agent in patch.descriptions.unwrap_or_default() {
        default_profile(settings, &agent.agent_id, launchable).description = agent
            .description
            .filter(|description| !description.trim().is_empty());
    }
    Ok(())
}

/// Store `upsert.profile` in place of the profile its `previous_id` (else
/// its own id) names, or append it. A `previous_id` must name a stored or
/// implicit profile. A profile id may not be another agent's key, which
/// that agent's implicit default uses, nor a case variant of its own
/// agent's key, which the legacy fields store that agent's default under.
fn upsert_profile(
    settings: &mut OrchestratorSettings,
    upsert: ProfileUpsertPayload,
    launchable: &Launchable,
) -> Result<(), String> {
    let profile = upsert.profile;
    if let Some(agent) = launchable.agents().iter().find(|agent| {
        agent.key.eq_ignore_ascii_case(&profile.id)
            && !(agent.key == profile.agent_id && agent.key == profile.id)
    }) {
        return Err(format!(
            "the profile id {} is the id of the agent {}",
            profile.id, agent.name
        ));
    }
    let target = match upsert.previous_id.as_deref().map(str::trim) {
        Some(previous) => {
            let stored = settings.profiles.iter().position(|p| p.id == previous);
            let implicit = launchable
                .resolve(settings)
                .iter()
                .any(|resolved| resolved.implicit && resolved.profile.id == previous);
            if stored.is_none() && !implicit {
                return Err(format!("no sub-agent profile is named {previous}"));
            }
            stored
        }
        None => settings.profiles.iter().position(|p| p.id == profile.id),
    };
    match target {
        Some(index) => settings.profiles[index] = profile,
        None => settings.profiles.push(profile),
    }
    Ok(())
}

/// The stored default profile of the agent `key`, stored now from its
/// implicit default when it has none.
fn default_profile<'a>(
    settings: &'a mut OrchestratorSettings,
    key: &str,
    launchable: &Launchable,
) -> &'a mut SubagentProfile {
    let index = match settings
        .profiles
        .iter()
        .position(|profile| profile.id.eq_ignore_ascii_case(key))
    {
        Some(index) => index,
        None => {
            let implicit = launchable
                .resolve(settings)
                .into_iter()
                .find(|resolved| resolved.implicit && resolved.profile.id == key)
                .map_or_else(|| SubagentProfile::default_for(key), |resolved| resolved.profile);
            settings.profiles.push(implicit);
            settings.profiles.len() - 1
        }
    };
    &mut settings.profiles[index]
}

/// Merge `patch` into the settings, each agent under the key the listing
/// and `subagent_start` use, so a bare installed id is not saved unread.
fn apply_settings_patch(
    engine: &OrchestratorEngine,
    mut patch: SubagentSettingsPayload,
) -> Result<OrchestratorSettings, String> {
    for agent in patch.agents.iter_mut().flatten() {
        agent.agent_id = engine.canonical_agent(agent.agent_id.trim());
    }
    for agent in patch.models.iter_mut().flatten() {
        agent.agent_id = engine.canonical_agent(agent.agent_id.trim());
    }
    for agent in patch.descriptions.iter_mut().flatten() {
        agent.agent_id = engine.canonical_agent(agent.agent_id.trim());
    }
    for upsert in patch.upsert_profiles.iter_mut().flatten() {
        let profile = &mut upsert.profile;
        profile.id = profile.id.trim().to_string();
        profile.agent_id = engine.canonical_agent(profile.agent_id.trim());
    }
    let launchable = engine.launchable();
    engine.try_update_settings(|settings| merge_settings(settings, patch, &launchable))
}

/// `subagent_settings_set` → merge into the current settings, persist, and
/// answer the new view. Out-of-bounds values are `VALIDATION_ERROR`.
pub(super) fn handle_subagent_settings_set(id: String, payload: &Value) -> WsReply {
    let parsed: SubagentSettingsPayload = match serde_json::from_value(payload.clone()) {
        Ok(parsed) => parsed,
        Err(error) => {
            return WsReply::err_with_code(
                id,
                "VALIDATION_ERROR",
                format!("malformed subagent_settings_set payload: {error}"),
            )
        }
    };
    let Some(engine) = crate::acp::orchestrator::installed() else {
        return no_engine(id);
    };
    match apply_settings_patch(&engine, parsed) {
        Ok(_) => WsReply::ok(id, Some(settings_view(&engine))),
        Err(error) => {
            info!("[orchestrator] sub-agent settings change refused: {error}");
            WsReply::err_with_code(id, "VALIDATION_ERROR", error)
        }
    }
}

#[cfg(test)]
#[path = "subagents_tests.rs"]
mod subagents_tests;
