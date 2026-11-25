//! Profile resolution: stored profiles, implicit defaults, the bare-switch
//! fallback, and how a saved config value reaches the agent.

use super::*;

fn launchable(keys: &[(&str, &str)], bare_fallback: bool) -> Launchable {
    Launchable {
        agents: keys
            .iter()
            .map(|(key, name)| LaunchableAgent {
                key: key.to_string(),
                name: name.to_string(),
            })
            .collect(),
        bare_fallback,
    }
}

fn settings(profiles: Vec<SubagentProfile>) -> OrchestratorSettings {
    OrchestratorSettings {
        profiles,
        ..OrchestratorSettings::default()
    }
}

fn ids(resolved: &[ResolvedProfile]) -> Vec<(&str, Option<&str>, bool)> {
    resolved
        .iter()
        .map(|r| (r.profile.id.as_str(), r.agent_name.as_deref(), r.implicit))
        .collect()
}

#[test]
fn stored_profiles_come_first_then_each_untouched_agents_implicit_default() {
    let agents = launchable(
        &[("acp-registry:codex", "Codex"), ("acp-registry:pi", "Pi")],
        false,
    );
    let saved = settings(vec![
        SubagentProfile {
            id: "pi-research".to_string(),
            ..SubagentProfile::default_for("acp-registry:pi")
        },
        SubagentProfile {
            id: "gone".to_string(),
            ..SubagentProfile::default_for("acp-registry:uninstalled")
        },
        SubagentProfile {
            id: "pi-fast".to_string(),
            ..SubagentProfile::default_for("acp-registry:pi")
        },
    ]);
    let resolved = agents.resolve(&saved);
    assert_eq!(
        ids(&resolved),
        [
            ("pi-research", Some("Pi"), false),
            ("gone", None, false),
            ("pi-fast", Some("Pi"), false),
            ("acp-registry:codex", Some("Codex"), true),
        ]
    );
    assert_eq!(
        resolved[3].profile,
        SubagentProfile::default_for("acp-registry:codex")
    );
    assert!(agents.find(&saved, "pi-fast").is_some());
    assert!(agents.find(&saved, "gone").is_none(), "an uninstalled agent cannot start");
    assert!(agents.find(&saved, "PI-FAST").is_none(), "ids match exactly");
    assert!(agents.find(&saved, "acp-registry:pi").is_none(), "pi has stored profiles");
}

#[test]
fn an_implicit_default_whose_id_a_stored_profile_took_is_left_out() {
    let agents = launchable(&[("cursor", "Cursor")], false);
    let saved = settings(vec![SubagentProfile {
        id: "Cursor".to_string(),
        ..SubagentProfile::default_for("acp-registry:pi")
    }]);
    assert_eq!(ids(&agents.resolve(&saved)), [("Cursor", None, false)]);
}

#[test]
fn an_unmoved_bare_switch_blocks_the_implicit_default_while_the_registry_is_unreadable() {
    let saved = settings(vec![SubagentProfile {
        enabled: false,
        ..SubagentProfile::default_for("cursor")
    }]);
    let unreadable = launchable(&[("acp-registry:cursor", "cursor")], true);
    let implicit = unreadable
        .find(&saved, "acp-registry:cursor")
        .expect("the implicit default");
    assert!(!implicit.profile.enabled);
    let readable = launchable(&[("acp-registry:cursor", "cursor")], false);
    assert!(
        readable
            .find(&saved, "acp-registry:cursor")
            .expect("the implicit default")
            .profile
            .enabled
    );
}

#[test]
fn a_saved_value_is_a_bool_only_for_an_option_the_session_offers_as_boolean() {
    let advertised: Vec<SessionConfigOption> = serde_json::from_value(serde_json::json!([
        { "id": "fast_mode", "name": "Fast", "type": "boolean", "currentValue": false },
        { "id": "thinking", "name": "Thinking", "type": "select", "currentValue": "low",
          "options": [{ "value": "low", "name": "Low" }, { "value": "true", "name": "On" }] },
    ]))
    .expect("options parse");
    let value = |id: &str, value: &str| match config_value(Some(&advertised), id, value) {
        ConfigOptionValue::Bool(flag) => format!("bool:{flag}"),
        ConfigOptionValue::Text(text) => format!("text:{text}"),
    };
    assert_eq!(value("fast_mode", "true"), "bool:true");
    assert_eq!(value("fast_mode", "false"), "bool:false");
    assert_eq!(value("thinking", "true"), "text:true");
    assert_eq!(value("unadvertised", "false"), "text:false");
    assert!(matches!(
        config_value(None, "fast_mode", "true"),
        ConfigOptionValue::Text(_)
    ));
}
