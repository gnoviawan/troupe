#![allow(clippy::unwrap_used, clippy::expect_used)]

use serde_json::json;

use super::*;
use crate::acp::troupe_overlay::{parse_overlay, AgentOverride, TroupeOverlay};
use crate::acp::{AcpCatalogService, CatalogRuntimeAvailability, SupportedAcpAgentStatus};

fn host() -> HostCapability {
    HostCapability {
        os: "linux".to_string(),
        arch: "x86_64".to_string(),
        runtimes: CatalogRuntimeAvailability {
            npx: true,
            ..CatalogRuntimeAvailability::default()
        },
    }
}

fn upstream(id: &str, package: &str) -> CatalogAgent {
    let entry = BundledAgent {
        id: id.to_string(),
        name: id.to_string(),
        version: "1.0.0".to_string(),
        description: String::new(),
        distribution: json!({ "npx": { "package": package } }),
    };
    compute_catalog_agent(&entry, &host(), "linux-x86_64", CatalogSource::Bundled)
}

fn overlay(document: serde_json::Value) -> TroupeOverlay {
    parse_overlay(&document).unwrap()
}

fn rename(id: &str, name: &str) -> AgentOverride {
    AgentOverride {
        id: id.to_string(),
        name: Some(name.to_string()),
        hidden: false,
        icon: None,
        brand_color: None,
    }
}

#[test]
fn an_overlay_agent_never_replaces_an_upstream_one() {
    let mut agents = vec![upstream("gemini", "@google/gemini-cli@1.0.0")];
    let overlay = overlay(json!({
        "schemaVersion": 1,
        "agents": [
            { "id": "gemini", "name": "Hijack", "distribution": { "npx": { "package": "evil@6.6.6" } } },
            { "id": "extra", "name": "Extra", "version": "2.0.0", "distribution": { "npx": { "package": "extra@2.0.0" } } }
        ]
    }));

    apply_troupe_overlay(&mut agents, &overlay, &host(), "linux-x86_64");

    let summary: Vec<(String, String, CatalogSource, serde_json::Value)> = agents
        .iter()
        .map(|agent| {
            (
                agent.id.clone(),
                agent.name.clone(),
                agent.source,
                agent.distribution["npx"]["package"].clone(),
            )
        })
        .collect();
    assert_eq!(
        summary,
        vec![
            (
                "gemini".to_string(),
                "gemini".to_string(),
                CatalogSource::Bundled,
                json!("@google/gemini-cli@1.0.0")
            ),
            (
                "extra".to_string(),
                "Extra".to_string(),
                CatalogSource::Troupe,
                json!("extra@2.0.0")
            ),
        ]
    );
    assert_eq!(agents[1].status, SupportedAcpAgentStatus::Ready);
}

#[test]
fn hidden_drops_the_agent_and_name_renames_it() {
    let mut agents = vec![upstream("a", "a@1.0.0"), upstream("b", "b@1.0.0")];
    let mut overlay = TroupeOverlay::default();
    overlay.overrides.push(AgentOverride {
        hidden: true,
        ..rename("a", "Ignored")
    });
    overlay.overrides.push(rename("b", "Bee"));

    apply_troupe_overlay(&mut agents, &overlay, &host(), "linux-x86_64");

    let listed: Vec<(&str, &str)> = agents
        .iter()
        .map(|agent| (agent.id.as_str(), agent.name.as_str()))
        .collect();
    assert_eq!(listed, [("b", "Bee")]);
}

#[tokio::test]
async fn the_catalog_lists_oh_my_pi_from_the_bundled_overlay() {
    let root = tempfile::tempdir().unwrap();
    let service = AcpCatalogService::open(root.path().join("catalog"))
        .await
        .unwrap();
    let catalog = service.list_catalog(false).await.unwrap();
    let omp = catalog
        .agents
        .iter()
        .find(|agent| agent.id == "oh-my-pi")
        .expect("oh-my-pi comes from troupe.json");
    assert_eq!(omp.source, CatalogSource::Troupe);
    assert_eq!(omp.name, "Oh My Pi");
    assert_eq!(
        omp.distribution["binary"]["linux-x86_64"],
        json!({ "cmd": "omp", "args": ["acp"] })
    );
    assert!(catalog
        .agents
        .iter()
        .filter(|agent| agent.id != "oh-my-pi")
        .all(|agent| agent.source == CatalogSource::Bundled));
}
