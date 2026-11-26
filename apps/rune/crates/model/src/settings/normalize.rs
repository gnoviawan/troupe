//! `DEFAULT_APP_SETTINGS` and `normalizeStoredSettings`: a stored record filled in from the
//! defaults, with what older versions wrote dropped or rewritten.

use serde_json::{Map, Value, json};

use super::{Effect, log};
use crate::{Level, js_trim};

/// `MONO_STACK` in `lib/font-stacks.ts`.
const MONO_STACK: &str = "\"Paper Mono\", \"Termul Symbols\", monospace";

/// Settings an older version persisted that no longer have any effect.
const RETIRED: [&str; 3] = [
    "maxTerminalsPerProject",
    "orphanDetectionEnabled",
    "orphanDetectionTimeout",
];

/// `DEFAULT_APP_SETTINGS`, in its key order.
#[must_use]
pub fn defaults() -> Map<String, Value> {
    let value = json!({
        "terminalFontFamily": MONO_STACK,
        "terminalFontSize": 14,
        "terminalBufferSize": 10000,
        "terminalRenderer": "auto",
        "terminalScreenReaderMode": false,
        "automaticCompletionRefresh": true,
        "suggestAutomations": true,
        "defaultShell": "",
        "defaultProjectColor": "blue",
        "confirmTerminalClose": true,
        "linkOpenMode": "system",
        "remoteBindMode": "localhost",
        "colorTheme": "termul",
        "appearanceMode": "dark",
        "uiZoomLevel": 1,
        "translucentRail": false,
        "acpTurnTimeoutSecs": null,
        "editorAutoSave": false,
        "editorAutoSaveDelayMs": 1000,
        "canvasAutosaveMs": 1500,
        "canvasAutosaveMaxMs": 10000,
        "canvasLeaseGraceMs": 5000,
        "canvasMaxMounted": 2,
        "canvasCrashTimeouts": 3,
        "acpTurnIdleTimeoutSecs": null,
        "acpSessionNewTimeoutSecs": null,
        "acpSessionReopenTimeoutSecs": null,
        "agentIdleWindowSecs": null,
        "notifyOnTerminalIdle": true,
        "notifyOnAgentChatTurnFinished": true,
        "notifyOnAgentChatNeedsYou": true,
        "terminalProfiles": [],
        "onboardingCompletedAt": null
    });
    match value {
        Value::Object(map) => map,
        _ => unreachable!("the defaults are an object"),
    }
}

pub struct Normalized {
    pub settings: Map<String, Value>,
    /// The stored record differs from `settings`: write it back.
    pub changed: bool,
}

/// `normalizeStoredSettings`. A stored `system` appearance becomes `system_dark`'s.
pub fn normalize(data: &Value, system_dark: bool, effects: &mut Vec<Effect>) -> Normalized {
    let empty = Map::new();
    let data = data.as_object().unwrap_or(&empty);
    let mut settings = defaults();
    for (key, value) in data {
        settings.insert(key.clone(), value.clone());
    }
    let profiles = terminal_profiles(data.get("terminalProfiles"), effects);
    settings.insert("terminalProfiles".into(), Value::Array(profiles));
    let onboarding = data
        .get("onboardingCompletedAt")
        .filter(|v| v.is_string())
        .cloned();
    settings.insert(
        "onboardingCompletedAt".into(),
        onboarding.unwrap_or(Value::Null),
    );
    let mut changed = false;

    // "Open links in" moved from the terminal (`terminalUrlOpenMode`) to the app (`linkOpenMode`).
    if let Some(legacy) = data.get("terminalUrlOpenMode") {
        settings.shift_remove("terminalUrlOpenMode");
        if !data.contains_key("linkOpenMode") {
            let mode = if legacy == "termul" {
                "troupe"
            } else {
                "system"
            };
            settings.insert("linkOpenMode".into(), json!(mode));
        }
        changed = true;
    }

    for key in RETIRED {
        changed |= settings.shift_remove(key).is_some();
    }

    let raw_appearance = data.get("appearanceMode");
    let theme = settings
        .get("colorTheme")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let legacy_light = theme.ends_with("-light");
    if legacy_light {
        settings.insert("colorTheme".into(), json!(theme.trim_end_matches("-light")));
        if settings.get("appearanceMode").is_none_or(Value::is_null) {
            settings.insert("appearanceMode".into(), json!("light"));
        }
        changed = true;
    }
    if raw_appearance.is_none() && !legacy_light {
        settings.insert("appearanceMode".into(), json!("dark"));
        changed = true;
    }
    let appearance = settings.get("appearanceMode").and_then(Value::as_str);
    if raw_appearance.and_then(Value::as_str) == Some("system") {
        settings.insert(
            "appearanceMode".into(),
            json!(if system_dark { "dark" } else { "light" }),
        );
        changed = true;
    } else if appearance != Some("light") && appearance != Some("dark") {
        settings.insert("appearanceMode".into(), json!("dark"));
        changed = true;
    }

    // xterm 6 removed the canvas renderer; DOM is the built-in fallback.
    if settings.get("terminalRenderer").and_then(Value::as_str) == Some("canvas") {
        settings.insert("terminalRenderer".into(), json!("dom"));
        changed = true;
    }
    Normalized { settings, changed }
}

/// `normalizeTerminalProfiles`: malformed entries and repeated ids are dropped.
fn terminal_profiles(value: Option<&Value>, effects: &mut Vec<Effect>) -> Vec<Value> {
    let Some(Value::Array(entries)) = value else {
        return Vec::new();
    };
    let mut seen = std::collections::HashSet::new();
    let profiles: Vec<Value> = entries
        .iter()
        .filter_map(stored_profile)
        .filter(|profile| seen.insert(profile["id"].as_str().unwrap_or_default().to_owned()))
        .collect();
    if profiles.len() != entries.len() {
        effects.push(log(
            Level::Warn,
            "terminal-profiles.normalizeTerminalProfiles",
            format!(
                "Dropped {} malformed terminal profile(s) from settings",
                entries.len() - profiles.len()
            ),
        ));
    }
    profiles
}

/// `ENV_KEY`: `^[A-Za-z_][A-Za-z0-9_]*$`.
pub(crate) fn env_key(key: &str) -> bool {
    let mut chars = key.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// `parseStoredProfile`: the profile with only its own fields, or None.
fn stored_profile(value: &Value) -> Option<Value> {
    let profile = value.as_object()?;
    let id = profile.get("id")?.as_str().filter(|id| !id.is_empty())?;
    let name = profile
        .get("name")?
        .as_str()
        .filter(|name| !js_trim(name).is_empty())?;
    let shell = profile.get("shell")?.as_str()?;
    let cwd = profile.get("cwd")?.as_str()?;
    let args = profile.get("args")?.as_array()?;
    if !args.iter().all(Value::is_string) || (!args.is_empty() && shell.is_empty()) {
        return None;
    }
    let env = profile.get("env")?.as_object()?;
    if !env
        .iter()
        .all(|(key, value)| env_key(key) && value.is_string())
    {
        return None;
    }
    Some(json!({ "id": id, "name": name, "shell": shell, "args": args, "cwd": cwd, "env": env }))
}
