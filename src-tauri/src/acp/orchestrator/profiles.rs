//! Sub-agent profiles: what `subagent_start`'s `agent` names. A profile
//! picks an installed agent and the model, session mode, session config
//! values and approvals policy its runs start with. Several profiles may
//! launch the same agent.
//!
//! Every launchable agent that no stored profile points at has an implicit
//! default profile, `{id: <listing key>, agentId: <listing key>, enabled}`,
//! so a newly installed agent is startable at once. It is stored only when
//! the user edits it, and comes back when the agent's last stored profile
//! is deleted.
//!
//! Mode and approvals are the user's alone: neither the listing nor
//! `subagent_start` exposes them to the calling agent, so an agent can
//! never raise its own child's permissions.

use std::collections::BTreeMap;

use agent_client_protocol::schema::v1::{SessionConfigKind, SessionConfigOption};
use serde::{Deserialize, Serialize};

use crate::acp::config::{AgentId, SessionId};
use crate::acp::manager::{AcpManager, ConfigOptionValue};
use crate::acp::orchestrator::agents::REGISTRY_PREFIX;
use crate::acp::orchestrator::settings::OrchestratorSettings;

/// How the host treats a sub-agent's tool permission requests.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SubagentApprovals {
    /// The request waits for the user in the chat.
    #[default]
    Ask,
    /// The host picks the request's allow-once option itself, never an
    /// allow-always one. A request without one still goes to the chat.
    AllowAll,
}

/// One sub-agent profile as the user saved it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SubagentProfile {
    /// What `subagent_start`'s `agent` names.
    pub id: String,
    /// The listing key of the agent the profile launches.
    pub agent_id: String,
    pub enabled: bool,
    /// The model a start that names none asks for; `None` keeps the agent's.
    #[serde(default)]
    pub model: Option<String>,
    /// The ACP session mode the run switches to after `session/new`.
    #[serde(default)]
    pub mode: Option<String>,
    /// ACP session config option id to value, set after the model.
    #[serde(default)]
    pub config_options: BTreeMap<String, String>,
    #[serde(default)]
    pub approvals: SubagentApprovals,
    /// The user's note on what this profile is good at.
    #[serde(default)]
    pub description: Option<String>,
}

impl SubagentProfile {
    /// The default profile of the agent listed under `agent_key`.
    pub fn default_for(agent_key: &str) -> Self {
        Self {
            id: agent_key.to_string(),
            agent_id: agent_key.to_string(),
            enabled: true,
            model: None,
            mode: None,
            config_options: BTreeMap::new(),
            approvals: SubagentApprovals::Ask,
            description: None,
        }
    }
}

/// One launchable agent: its listing key and display name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchableAgent {
    pub key: String,
    pub name: String,
}

/// A profile as the listing, the start and the settings view see it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedProfile {
    pub profile: SubagentProfile,
    /// The agent's display name; `None` when its agent is not launchable
    /// (uninstalled), so it can be neither listed nor started.
    pub agent_name: Option<String>,
    /// Synthesized, not stored.
    pub implicit: bool,
}

/// The launchable agents, sorted by key.
#[derive(Debug, Clone)]
pub struct Launchable {
    pub(super) agents: Vec<LaunchableAgent>,
    /// The known-agents registry could not be read, so a switch saved
    /// under an installed agent's bare id still applies to its config id
    /// (see `OrchestratorEngine::migrate_bare_switches`).
    pub(super) bare_fallback: bool,
}

impl Launchable {
    pub fn agents(&self) -> &[LaunchableAgent] {
        &self.agents
    }

    /// The stored profiles in the user's order, then the implicit default
    /// of every agent no stored profile points at, in key order. An
    /// implicit id a stored profile already uses is left out.
    pub fn resolve(&self, settings: &OrchestratorSettings) -> Vec<ResolvedProfile> {
        let name_of = |key: &str| {
            self.agents
                .iter()
                .find(|agent| agent.key == key)
                .map(|agent| agent.name.clone())
        };
        let mut resolved: Vec<ResolvedProfile> = settings
            .profiles
            .iter()
            .map(|profile| ResolvedProfile {
                agent_name: name_of(&profile.agent_id),
                profile: profile.clone(),
                implicit: false,
            })
            .collect();
        for agent in &self.agents {
            let taken = settings.profiles.iter().any(|profile| {
                profile.agent_id == agent.key || profile.id.eq_ignore_ascii_case(&agent.key)
            });
            if taken {
                continue;
            }
            resolved.push(ResolvedProfile {
                profile: SubagentProfile {
                    enabled: self.bare_switch(settings, &agent.key).unwrap_or(true),
                    ..SubagentProfile::default_for(&agent.key)
                },
                agent_name: Some(agent.name.clone()),
                implicit: true,
            });
        }
        resolved
    }

    /// The switch a not yet moved bare-id profile holds for `key`.
    fn bare_switch(&self, settings: &OrchestratorSettings, key: &str) -> Option<bool> {
        if !self.bare_fallback {
            return None;
        }
        let bare = key.strip_prefix(REGISTRY_PREFIX)?;
        settings
            .profiles
            .iter()
            .find(|profile| profile.id == bare && profile.agent_id == bare)
            .map(|profile| profile.enabled)
    }

    /// The startable profile named exactly `id`.
    pub fn find(&self, settings: &OrchestratorSettings, id: &str) -> Option<ResolvedProfile> {
        self.resolve(settings)
            .into_iter()
            .find(|resolved| resolved.agent_name.is_some() && resolved.profile.id == id)
    }
}

/// Apply the profile to a run's fresh session, each step best-effort: the
/// model (`model`: the start's, else the profile's), then the mode, then
/// each config option, after the model since an option such as a thinking
/// level depends on it. Returns the model the session took.
pub(super) async fn apply_to_session(
    manager: &AcpManager,
    child_agent: &AgentId,
    session: &SessionId,
    model: Option<String>,
    profile: &SubagentProfile,
    advertised: Option<&[SessionConfigOption]>,
    run_log: &str,
) -> Option<String> {
    let mut model = model;
    if let Some(requested) = model.clone() {
        if let Err(error) = manager
            .set_model(child_agent, session.clone(), requested)
            .await
        {
            log::info!(
                "[orchestrator] run {run_log} model switch failed: {error}; keeping the agent's model"
            );
            model = None;
        }
    }
    if let Some(mode) = &profile.mode {
        if let Err(error) = manager
            .set_mode(child_agent, session.clone(), mode.clone())
            .await
        {
            log::info!(
                "[orchestrator] run {run_log} mode switch failed: {error}; keeping the agent's mode"
            );
        }
    }
    for (id, value) in &profile.config_options {
        if let Err(error) = manager
            .set_config_option(
                child_agent,
                session.clone(),
                id.clone(),
                config_value(advertised, id, value),
            )
            .await
        {
            log::info!(
                "[orchestrator] run {run_log} config option {id} refused: {error}; keeping the agent's value"
            );
        }
    }
    model
}

/// A saved value as the option takes it: `"true"` / `"false"` for an
/// option the session advertised as boolean, else the select value id.
fn config_value(
    advertised: Option<&[SessionConfigOption]>,
    id: &str,
    value: &str,
) -> ConfigOptionValue {
    let boolean = advertised.into_iter().flatten().any(|option| {
        option.id.0.as_ref() == id && matches!(option.kind, SessionConfigKind::Boolean(_))
    });
    match (boolean, value) {
        (true, "true") => ConfigOptionValue::Bool(true),
        (true, "false") => ConfigOptionValue::Bool(false),
        _ => ConfigOptionValue::Text(value.to_string()),
    }
}

#[cfg(test)]
#[path = "profiles_tests.rs"]
mod profiles_tests;
