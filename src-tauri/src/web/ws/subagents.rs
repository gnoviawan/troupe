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

/// The orchestrator's settings view: the gates and every profile, stored
/// then implicit.
fn settings_view(engine: &OrchestratorEngine) -> Value {
    let launchable = engine.launchable();
    let settings = engine.settings_view();
    let profiles = launchable.resolve(&settings);
    json!({
        "enabled": settings.enabled,
        "maxRunning": settings.max_running,
        "maxDepth": settings.max_depth,
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

/// One profile of a `subagent_settings_set` call. `previous_id` renames:
/// it names the stored or implicit profile `profile` replaces.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct ProfileUpsertPayload {
    #[serde(default)]
    previous_id: Option<String>,
    profile: SubagentProfile,
}

/// `subagent_settings_set`: the gates to change, then the profiles to
/// delete and to upsert, applied in that order; an omitted field keeps its
/// value.
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
    upsert_profiles: Option<Vec<ProfileUpsertPayload>>,
    #[serde(default)]
    delete_profiles: Option<Vec<String>>,
}

/// Apply the payload's fields over `settings`. Profile ids are trimmed and
/// agent ids canonical already ([`apply_settings_patch`]).
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
    Ok(())
}

/// Store `upsert.profile` in place of the profile its `previous_id` (else
/// its own id) names, or append it. A `previous_id` must name a stored or
/// implicit profile. A profile id may not be another agent's key, ignoring
/// ASCII case: that agent's implicit default uses the key, and
/// [`Launchable::resolve`] drops the default once a stored id takes it. A
/// case variant of its own agent's key is fine, since a stored profile of
/// that agent drops the default anyway.
fn upsert_profile(
    settings: &mut OrchestratorSettings,
    upsert: ProfileUpsertPayload,
    launchable: &Launchable,
) -> Result<(), String> {
    let profile = upsert.profile;
    if let Some(agent) = launchable
        .agents()
        .iter()
        .find(|agent| agent.key.eq_ignore_ascii_case(&profile.id) && agent.key != profile.agent_id)
    {
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

/// Merge `patch` into the settings, each profile's agent under the key the
/// listing and `subagent_start` use, so a bare installed id is not saved
/// unread.
fn apply_settings_patch(
    engine: &OrchestratorEngine,
    mut patch: SubagentSettingsPayload,
) -> Result<OrchestratorSettings, String> {
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
