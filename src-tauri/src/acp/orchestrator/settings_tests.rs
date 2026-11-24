//! The settings store: defaults, persistence, the move to profiles,
//! corruption and validation.

use super::*;
use crate::acp::orchestrator::profiles::SubagentApprovals;

fn dir() -> tempfile::TempDir {
    tempfile::tempdir().expect("tempdir")
}

fn profile(id: &str, agent_id: &str) -> SubagentProfile {
    SubagentProfile {
        id: id.to_string(),
        ..SubagentProfile::default_for(agent_id)
    }
}

fn with_profiles(profiles: Vec<SubagentProfile>) -> OrchestratorSettings {
    OrchestratorSettings {
        profiles,
        ..OrchestratorSettings::default()
    }
}

/// The settings a file with these bytes loads.
fn load(bytes: &[u8]) -> (OrchestratorSettings, tempfile::TempDir) {
    let dir = dir();
    std::fs::write(dir.path().join(FILE_NAME), bytes).expect("write");
    let settings = SettingsStore::open(dir.path()).get();
    (settings, dir)
}

#[test]
fn a_missing_file_loads_the_defaults() {
    let store = SettingsStore::open(dir().path());
    assert_eq!(
        store.get(),
        OrchestratorSettings {
            enabled: true,
            max_running: DEFAULT_MAX_RUNNING,
            max_depth: DEFAULT_MAX_DEPTH,
            profiles: Vec::new(),
        }
    );
}

#[test]
fn a_saved_settings_file_round_trips() {
    let dir = dir();
    let store = SettingsStore::open(dir.path());
    let saved = store
        .set(OrchestratorSettings {
            enabled: false,
            max_running: 2,
            max_depth: 3,
            profiles: vec![
                SubagentProfile {
                    model: Some("gpt-5.5-mini".to_string()),
                    mode: Some("full-access".to_string()),
                    config_options: [("reasoning_effort".to_string(), "low".to_string())].into(),
                    approvals: SubagentApprovals::AllowAll,
                    description: Some("  Fast at UI tweaks; weak at Rust.\n".to_string()),
                    ..profile("pi-fast", " acp-registry:pi ")
                },
                SubagentProfile {
                    enabled: false,
                    ..SubagentProfile::default_for("acp-registry:codex")
                },
            ],
        })
        .expect("saved");
    assert_eq!(saved.profiles[0].agent_id, "acp-registry:pi");
    assert_eq!(
        saved.profiles[0].description.as_deref(),
        Some("Fast at UI tweaks; weak at Rust.")
    );
    assert_eq!(SettingsStore::open(dir.path()).get(), saved);
    let file: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.path().join(FILE_NAME)).expect("file"))
            .expect("json");
    assert_eq!(
        file,
        serde_json::json!({
            "schema_version": 4,
            "enabled": false,
            "maxRunning": 2,
            "maxDepth": 3,
            "profiles": [
                {
                    "id": "pi-fast",
                    "agentId": "acp-registry:pi",
                    "enabled": true,
                    "model": "gpt-5.5-mini",
                    "mode": "full-access",
                    "configOptions": { "reasoning_effort": "low" },
                    "approvals": "allowAll",
                    "description": "Fast at UI tweaks; weak at Rust.",
                },
                {
                    "id": "acp-registry:codex",
                    "agentId": "acp-registry:codex",
                    "enabled": false,
                    "model": null,
                    "mode": null,
                    "configOptions": {},
                    "approvals": "ask",
                    "description": null,
                },
            ],
        })
    );
}

#[test]
fn a_version_1_file_still_loads_and_saves_as_profiles() {
    let dir = dir();
    std::fs::write(
        dir.path().join(FILE_NAME),
        br#"{"schema_version":1,"enabled":false,"maxRunning":3,"maxDepth":1,"agents":{"acp-registry:cursor":false}}"#,
    )
    .expect("write");
    let store = SettingsStore::open(dir.path());
    assert_eq!(
        store.get(),
        OrchestratorSettings {
            enabled: false,
            max_running: 3,
            max_depth: 1,
            profiles: vec![SubagentProfile {
                enabled: false,
                ..SubagentProfile::default_for("acp-registry:cursor")
            }],
        }
    );
    store
        .update(|settings| settings.enabled = true)
        .expect("a v1 file is writable");
    assert!(corrupt_backups(dir.path()).is_empty());
    let file: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.path().join(FILE_NAME)).expect("file"))
            .expect("json");
    assert_eq!(file["schema_version"], 4);
    assert_eq!(file["profiles"][0]["id"], "acp-registry:cursor");
    assert_eq!(file["profiles"][0]["enabled"], false);
}

#[test]
fn a_version_3_file_moves_each_agents_switch_model_and_note_to_its_profile() {
    let (settings, _dir) = load(
        br#"{"schema_version":3,"enabled":true,"maxRunning":4,"maxDepth":2,
            "agents":{"acp-registry:codex":true,"acp-registry:cursor":false},
            "models":{"acp-registry:codex":"gpt-5.1-mini","acp-registry:gemini":"gemini-3-pro"},
            "descriptions":{"acp-registry:cursor":"Good at docs."}}"#,
    );
    assert_eq!(
        settings.profiles,
        vec![
            SubagentProfile {
                model: Some("gpt-5.1-mini".to_string()),
                ..SubagentProfile::default_for("acp-registry:codex")
            },
            SubagentProfile {
                enabled: false,
                description: Some("Good at docs.".to_string()),
                ..SubagentProfile::default_for("acp-registry:cursor")
            },
            SubagentProfile {
                model: Some("gemini-3-pro".to_string()),
                ..SubagentProfile::default_for("acp-registry:gemini")
            },
        ]
    );
}

#[test]
fn a_version_3_file_with_a_blocked_agent_keeps_the_block() {
    let (settings, _dir) = load(
        br#"{"schema_version":3,"enabled":true,"maxRunning":4,"maxDepth":2,
            "agents":{"Mock Sub":false}}"#,
    );
    assert_eq!(
        settings.profiles,
        vec![SubagentProfile {
            enabled: false,
            ..SubagentProfile::default_for("Mock Sub")
        }],
        "a custom agent's key keeps its spaces as its default profile id"
    );
    let store = SettingsStore::in_memory();
    store
        .set(settings.clone())
        .expect("a migrated default profile validates");
}

#[test]
fn a_corrupt_file_is_backed_up_and_the_defaults_load() {
    let dir = dir();
    std::fs::write(dir.path().join(FILE_NAME), b"{not json").expect("write");
    let store = SettingsStore::open(dir.path());
    assert_eq!(store.get(), OrchestratorSettings::default());
    let backups = corrupt_backups(dir.path());
    assert_eq!(backups.len(), 1, "the corrupt file was kept aside");
    assert_eq!(std::fs::read(&backups[0]).expect("backup"), b"{not json");
}

#[test]
fn a_version_4_file_with_the_old_maps_is_corrupt() {
    let dir = dir();
    let bytes =
        br#"{"schema_version":4,"enabled":true,"maxRunning":4,"maxDepth":2,"agents":{"x":false}}"#;
    std::fs::write(dir.path().join(FILE_NAME), bytes).expect("write");
    assert_eq!(
        SettingsStore::open(dir.path()).get(),
        OrchestratorSettings::default()
    );
    assert_eq!(corrupt_backups(dir.path()).len(), 1);
}

fn corrupt_backups(dir: &Path) -> Vec<PathBuf> {
    let prefix = format!("{FILE_NAME}.corrupt-");
    std::fs::read_dir(dir)
        .expect("dir")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| {
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            name.starts_with(&prefix) && name.ends_with(".bak")
        })
        .collect()
}

#[test]
fn a_newer_file_loads_the_defaults_and_is_never_overwritten() {
    let dir = dir();
    let path = dir.path().join(FILE_NAME);
    let newer = br#"{"schema_version":5,"futureField":true}"#;
    std::fs::write(&path, newer).expect("write");
    let store = SettingsStore::open(dir.path());
    assert_eq!(store.get(), OrchestratorSettings::default());

    let error = store
        .set(OrchestratorSettings::default())
        .expect_err("a newer file is read-only");
    assert!(error.contains("newer Troupe"), "{error}");
    assert_eq!(std::fs::read(&path).expect("file"), newer);
    assert!(corrupt_backups(dir.path()).is_empty());
}

#[test]
fn concurrent_updates_keep_each_others_changes() {
    let dir = dir();
    let store = std::sync::Arc::new(SettingsStore::open(dir.path()));
    let writers: Vec<_> = (0..8)
        .map(|index| {
            let store = store.clone();
            std::thread::spawn(move || {
                store
                    .update(|settings| {
                        settings
                            .profiles
                            .push(SubagentProfile::default_for(&format!("agent-{index}")));
                    })
                    .expect("saved");
            })
        })
        .collect();
    for writer in writers {
        writer.join().expect("writer");
    }
    let saved = SettingsStore::open(dir.path()).get();
    assert_eq!(saved.profiles.len(), 8);
    assert_eq!(store.get(), saved);
}

#[test]
fn a_refused_update_leaves_the_settings_unchanged() {
    let dir = dir();
    let store = SettingsStore::open(dir.path());
    store
        .update(|settings| settings.max_depth = 3)
        .expect("saved");
    let error = store
        .update(|settings| {
            settings.enabled = false;
            settings.max_running = 0;
        })
        .expect_err("out of range");
    assert!(error.contains("max_running"), "{error}");
    let error = store
        .try_update(|settings| {
            settings.enabled = false;
            Err("refused by the edit".to_string())
        })
        .expect_err("the edit refused");
    assert_eq!(error, "refused by the edit");
    let expected = OrchestratorSettings {
        max_depth: 3,
        ..OrchestratorSettings::default()
    };
    assert_eq!(store.get(), expected);
    assert_eq!(SettingsStore::open(dir.path()).get(), expected);
}

#[test]
fn an_in_memory_store_never_writes() {
    let store = SettingsStore::in_memory();
    store
        .set(OrchestratorSettings {
            enabled: true,
            max_running: 8,
            max_depth: 4,
            profiles: Vec::new(),
        })
        .expect("saved");
    assert_eq!(store.get().max_running, 8);
}

#[test]
fn set_validates_the_bounds() {
    let store = SettingsStore::in_memory();
    for max_running in [0, 9] {
        let error = store
            .set(OrchestratorSettings {
                max_running,
                ..OrchestratorSettings::default()
            })
            .expect_err("out of range");
        assert!(error.contains("max_running"), "{error}");
    }
    for max_depth in [0, 5] {
        let error = store
            .set(OrchestratorSettings {
                max_depth,
                ..OrchestratorSettings::default()
            })
            .expect_err("out of range");
        assert!(error.contains("max_depth"), "{error}");
    }
    assert_eq!(store.get(), OrchestratorSettings::default());
}

#[test]
fn set_validates_each_profile() {
    let store = SettingsStore::in_memory();
    let refused = |profiles: Vec<SubagentProfile>| {
        store
            .set(with_profiles(profiles))
            .expect_err("refused")
    };
    assert_eq!(
        refused(vec![profile("pi-fast", "pi"), profile("PI-FAST", "pi")]),
        "two sub-agent profiles are named PI-FAST"
    );
    let bad_id = "a profile id must be 1 to 64 letters, digits, '.', '_', ':' or '-'";
    let too_long = "p".repeat(MAX_PROFILE_ID_CHARS + 1);
    for id in ["pi fast", "pi/fast", "", too_long.as_str()] {
        assert_eq!(refused(vec![profile(id, "pi")]), bad_id, "{id:?}");
    }
    assert_eq!(
        refused(vec![profile("pi-fast", "  ")]),
        "the sub-agent profile pi-fast names no agent"
    );
    for model in [" ".to_string(), "m".repeat(MAX_MODEL_CHARS + 1)] {
        assert_eq!(
            refused(vec![SubagentProfile {
                model: Some(model),
                ..profile("pi-fast", "pi")
            }]),
            "a default model must be 1 to 512 characters"
        );
    }
    assert_eq!(
        refused(vec![SubagentProfile {
            mode: Some(" ".to_string()),
            ..profile("pi-fast", "pi")
        }]),
        "a mode must be 1 to 256 characters"
    );
    assert_eq!(
        refused(vec![SubagentProfile {
            config_options: [("thinking".to_string(), String::new())].into(),
            ..profile("pi-fast", "pi")
        }]),
        "a config option id and value must be 1 to 256 characters"
    );
    for description in ["  \n".to_string(), "d".repeat(MAX_DESCRIPTION_CHARS + 1)] {
        assert_eq!(
            refused(vec![SubagentProfile {
                description: Some(description),
                ..profile("pi-fast", "pi")
            }]),
            "a description must be 1 to 1000 characters"
        );
    }
    assert_eq!(store.get(), OrchestratorSettings::default());
    store
        .set(with_profiles(vec![
            profile("pi.fast_1:a-b", "pi"),
            SubagentProfile::default_for("Custom Agent With Spaces"),
        ]))
        .expect("valid ids");
}
