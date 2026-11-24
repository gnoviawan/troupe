//! Merges Troupe's registry overlay into a resolved catalog.

use super::{compute_catalog_agent, BundledAgent, CatalogAgent, CatalogSource, HostCapability};
use crate::acp::troupe_overlay::TroupeOverlay;

/// Overlay agents join only under ids the bundled and CDN entries do not use,
/// so the overlay can never replace an upstream agent's launch command.
/// Then `hidden` overrides drop their agent and `name` overrides rename it.
pub(super) fn apply_troupe_overlay(
    agents: &mut Vec<CatalogAgent>,
    overlay: &TroupeOverlay,
    host: &HostCapability,
    platform_arch: &str,
) {
    let mut added = 0;
    for agent in &overlay.agents {
        if crate::acp::agent_aliases::is_legacy_agent_id(&agent.id) {
            continue;
        }
        if agents.iter().any(|existing| existing.id == agent.id) {
            log::debug!(
                "[acp-overlay] agent {} already listed upstream; the overlay entry is ignored",
                agent.id
            );
            continue;
        }
        let entry = BundledAgent {
            id: agent.id.clone(),
            name: agent.name.clone(),
            version: agent.version.clone(),
            description: agent.description.clone(),
            distribution: agent.distribution.clone(),
        };
        agents.push(compute_catalog_agent(
            &entry,
            host,
            platform_arch,
            CatalogSource::Troupe,
        ));
        added += 1;
    }
    let before = agents.len();
    agents.retain(|agent| {
        !overlay
            .overrides
            .iter()
            .any(|entry| entry.hidden && entry.id == agent.id)
    });
    let hidden = before - agents.len();
    for entry in &overlay.overrides {
        let Some(name) = &entry.name else {
            continue;
        };
        if let Some(agent) = agents.iter_mut().find(|agent| agent.id == entry.id) {
            agent.name = name.clone();
        }
    }
    log::debug!("[acp-overlay] merged added={added} hidden={hidden}");
}

#[cfg(test)]
#[path = "overlay_tests.rs"]
mod tests;
