//! The engine's view of the launchable agents and the profiles that start
//! them: the listing, start-time resolution, and the move of switches an
//! older host saved under bare ids.

use crate::acp::config::AgentConfig;
use crate::acp::orchestrator::engine::OrchestratorEngine;
use crate::acp::orchestrator::profiles::{
    Launchable, LaunchableAgent, ResolvedProfile, SubagentProfile,
};
use crate::acp::orchestrator::run::{AgentListing, StartError};

/// The config id prefix the renderer saves a catalog agent under: the
/// installed agent `x` is the registry's `acp-registry:x`.
pub(super) const REGISTRY_PREFIX: &str = "acp-registry:";

impl OrchestratorEngine {
    /// Switches were once saved under an installed agent's bare id, which
    /// the move to profiles kept as a default profile whose id and agent are
    /// that bare id: any such id without the config-id prefix that the
    /// registry lacks, since registry keys are never removed. An agent the
    /// registry lacked had only that row, so its profile moves to the config
    /// id. An agent the registry knew also had its own row, which the bare
    /// switch never applied to, so that profile is dropped. The move applies
    /// in memory even when it cannot be saved, and a later registry record
    /// or install cannot lift a block. An unreadable registry cannot tell
    /// the two apart, so the move waits, and the bare switch keeps applying
    /// through [`Launchable`]'s fallback.
    pub(super) fn migrate_bare_switches(&self) {
        if !self.registry.loaded() {
            log::warn!("[orchestrator] known agents unreadable; bare sub-agent switches kept");
            return;
        }
        let moved = self.settings.apply_and_save(|settings| {
            let (bare, kept): (Vec<SubagentProfile>, Vec<SubagentProfile>) =
                std::mem::take(&mut settings.profiles)
                    .into_iter()
                    .partition(|profile| {
                        profile.id == profile.agent_id
                            && !profile.id.starts_with(REGISTRY_PREFIX)
                            && !self.registry.contains(&profile.id)
                    });
            settings.profiles = kept;
            for profile in bare {
                let key = format!("{REGISTRY_PREFIX}{}", profile.id);
                let taken = settings
                    .profiles
                    .iter()
                    .any(|other| other.id.eq_ignore_ascii_case(&key));
                if !taken && !self.registry.contains(&key) {
                    settings.profiles.push(SubagentProfile {
                        id: key.clone(),
                        agent_id: key,
                        ..profile
                    });
                }
            }
        });
        if let Err(error) = moved {
            log::warn!("[orchestrator] could not save the moved sub-agent switches: {error}");
        }
    }

    /// The launchable agents: the known-agents registry merged with the
    /// installed overlay, one entry per config id. Command, args and env
    /// never leave the host.
    pub(crate) fn launchable(&self) -> Launchable {
        self.registry.reload();
        let stored = super::stored_agents::read(self.store.as_deref());
        let mut agents: Vec<LaunchableAgent> = self
            .registry
            .list()
            .into_iter()
            .map(|known| LaunchableAgent {
                key: known.agent_id,
                name: known.name,
            })
            .collect();
        if let Some(configs) = &stored {
            agents = configs
                .iter()
                .map(|config| LaunchableAgent {
                    key: config.config_id.clone().unwrap(),
                    name: config.name.clone(),
                })
                .collect();
        }
        if let Some(installed) = &self.installed {
            for agent in installed.installed_agents() {
                let key = format!("{REGISTRY_PREFIX}{}", agent.agent_id);
                if !agents.iter().any(|known| known.key == key) {
                    agents.push(LaunchableAgent {
                        key,
                        name: agent.agent_id,
                    });
                }
            }
        }
        agents.sort_by(|a, b| a.key.cmp(&b.key));
        Launchable {
            agents,
            // Saved composer keys are authoritative; bare migration fallback is legacy only.
            bare_fallback: stored.is_none() && !self.registry.loaded(),
        }
    }

    /// Every profile, stored and implicit, with its agent's name.
    pub fn profiles(&self) -> Vec<ResolvedProfile> {
        self.launchable().resolve(&self.settings.get())
    }

    /// One entry per startable profile, stored then implicit: the profile
    /// id, its agent's name and key, and the profile's switch, default model
    /// and description.
    pub fn list_agents(&self) -> Vec<AgentListing> {
        self.profiles()
            .into_iter()
            .filter_map(|resolved| {
                let profile = resolved.profile;
                Some(AgentListing {
                    name: resolved.agent_name?,
                    agent_id: profile.id,
                    agent: profile.agent_id,
                    allowed: profile.enabled,
                    default_model: profile.model,
                    description: profile.description,
                })
            })
            .collect()
    }

    /// The listing key `agent` names: a bare catalog id the registry does
    /// not know is the installed agent's `acp-registry:` config id.
    pub(crate) fn canonical_agent(&self, agent: &str) -> String {
        self.registry.reload();
        let installed = self.installed.as_ref().is_some_and(|installed| {
            installed
                .installed_agents()
                .iter()
                .any(|known| known.agent_id == agent)
        });
        if installed
            && !self
                .launchable()
                .agents
                .iter()
                .any(|known| known.key == agent)
        {
            format!("{REGISTRY_PREFIX}{agent}")
        } else {
            agent.to_string()
        }
    }

    /// Resolve the launch config of `agent`: the known-agents registry
    /// first, then the installed-agents overlay.
    pub(super) fn resolve_agent(&self, agent: &str) -> Result<AgentConfig, StartError> {
        // The spawn path records launch configs into the automation
        // service's registry; read the file it writes, not a startup copy.
        self.registry.reload();
        if !self
            .launchable()
            .agents
            .iter()
            .any(|known| known.key == agent)
        {
            return Err(StartError::UnknownAgent {
                agent: agent.to_string(),
                allowed: Vec::new(),
            });
        }
        if let Some(config) = self.registry.resolve(agent) {
            return Ok(config);
        }
        if let Some(config) = super::stored_agents::read(self.store.as_deref())
            .unwrap_or_default()
            .into_iter()
            .find(|config| config.config_id.as_deref() == Some(agent))
        {
            return Ok(config);
        }
        if let Some(installed) = &self.installed {
            let registry_id = agent.strip_prefix(REGISTRY_PREFIX).unwrap_or(agent);
            if let Some(config) = installed
                .installed_agents()
                .into_iter()
                .find(|known| known.agent_id == registry_id)
                .map(|known| AgentConfig {
                    config_id: Some(format!("{REGISTRY_PREFIX}{}", known.agent_id)),
                    name: known.agent_id,
                    command: known.command,
                    args: known.args,
                    env: std::collections::HashMap::new(),
                    allow_terminal: false,
                })
            {
                return Ok(config);
            }
        }
        Err(StartError::UnknownAgent {
            agent: agent.to_string(),
            allowed: Vec::new(),
        })
    }
}
