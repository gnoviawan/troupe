//! Key bindings and the pure key-matching helpers. Port of
//! `src/renderer/stores/keyboard-shortcuts-store.ts`; the defaults are
//! `DEFAULT_KEYBOARD_SHORTCUTS` and `WEB_DEFAULT_KEY_OVERRIDES` in
//! `src/renderer/types/settings.ts`.

use serde::de::{MapAccess, Visitor};
use serde::ser::SerializeMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// `(id, label, description, desktop key, web key)`. The web keys avoid the
/// browser's own Ctrl bindings; an empty key never matches.
const DEFAULTS: &[(&str, &str, &str, &str, &str)] = &[
    (
        "commandPalette",
        "Command palette",
        "Open the command palette for quick actions",
        "ctrl+k",
        "ctrl+k",
    ),
    (
        "commandPaletteAlt",
        "Command palette (VS Code keys)",
        "Open command palette (VS Code style)",
        "ctrl+shift+p",
        "ctrl+shift+p",
    ),
    (
        "terminalSearch",
        "Search in terminal",
        "Search within terminal output",
        "ctrl+f",
        "ctrl+f",
    ),
    (
        "commandHistory",
        "Command history",
        "Search command history",
        "ctrl+r",
        "",
    ),
    (
        "newProject",
        "New project",
        "Create a new project",
        "ctrl+n",
        "alt+n",
    ),
    (
        "newTerminal",
        "Agent launcher",
        "Show the agent launcher prompt in the active pane",
        "ctrl+t",
        "alt+t",
    ),
    (
        "newBrowserTab",
        "New browser tab",
        "Create a new browser tab",
        "ctrl+shift+n",
        "alt+shift+n",
    ),
    (
        "nextTerminal",
        "Next tab",
        "Switch to next tab (terminal or editor) using the Tauri-safe fallback",
        "ctrl+pagedown",
        "alt+pagedown",
    ),
    (
        "prevTerminal",
        "Previous tab",
        "Switch to previous tab (terminal or editor) using the Tauri-safe fallback",
        "ctrl+pageup",
        "alt+pageup",
    ),
    (
        "zoomIn",
        "Zoom in",
        "Zoom in the entire UI",
        "ctrl+=",
        "alt+=",
    ),
    (
        "zoomOut",
        "Zoom out",
        "Zoom out the entire UI",
        "ctrl+-",
        "alt+-",
    ),
    (
        "zoomReset",
        "Reset zoom",
        "Reset the whole-UI zoom to 100%",
        "ctrl+0",
        "alt+0",
    ),
    (
        "sidebarToggle",
        "Show or hide projects",
        "Expand or collapse the projects sidebar",
        "ctrl+b",
        "ctrl+b",
    ),
    (
        "closeTab",
        "Close tab",
        "Close the active tab (terminal, editor, or browser)",
        "ctrl+w",
        "alt+w",
    ),
    (
        "saveFile",
        "Save file",
        "Save the current editor file",
        "ctrl+s",
        "ctrl+s",
    ),
    (
        "toggleFileExplorer",
        "Show or hide side panel",
        "Show or hide the side panel at the right edge",
        "ctrl+alt+b",
        "ctrl+alt+b",
    ),
    (
        "fileExplorerRename",
        "Rename file",
        "Rename selected file",
        "f2",
        "f2",
    ),
    (
        "fileExplorerDelete",
        "Delete files",
        "Delete selected files",
        "delete",
        "delete",
    ),
    (
        "worktreeCreate",
        "Create worktree",
        "Open the new worktree creation modal",
        "ctrl+shift+alt+n",
        "ctrl+shift+alt+n",
    ),
    (
        "worktreeSwitchNext",
        "Next worktree",
        "Cycle to the next worktree in the sidebar",
        "ctrl+shift+arrowdown",
        "ctrl+shift+arrowdown",
    ),
    (
        "worktreeSwitchPrev",
        "Previous worktree",
        "Cycle to the previous worktree in the sidebar",
        "ctrl+shift+arrowup",
        "ctrl+shift+arrowup",
    ),
    (
        "worktreeOpenTerminal",
        "Open terminal in worktree",
        "Spawn a new terminal in the active worktree",
        "ctrl+shift+alt+t",
        "ctrl+shift+alt+t",
    ),
    (
        "worktreeSwitchRoot",
        "Switch to project root",
        "Switch active context to the project root directory",
        "ctrl+shift+home",
        "ctrl+shift+home",
    ),
    (
        "colorThemePicker",
        "Change color theme",
        "Open the color theme picker with live preview",
        "ctrl+alt+t",
        "ctrl+alt+t",
    ),
    (
        "settings",
        "Settings",
        "Open app settings",
        "ctrl+,",
        "alt+,",
    ),
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Platform {
    /// The desktop app (Tauri today, the Rune desktop app later).
    #[serde(default)]
    pub desktop: bool,
    #[serde(default)]
    pub mac: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Shortcut {
    pub id: String,
    pub label: String,
    pub description: String,
    /// Normalized, e.g. `ctrl+shift+p`.
    pub default_key: String,
    /// The user's binding; `None` uses the default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_key: Option<String>,
}

impl Shortcut {
    #[must_use]
    pub fn active_key(&self) -> &str {
        self.custom_key.as_deref().unwrap_or(&self.default_key)
    }
}

/// Shortcuts by their key in the settings object, in insertion order (the
/// order conflicts are searched in). Updates address the key, as the
/// TypeScript store does; the key and `Shortcut::id` agree in valid settings.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Shortcuts(pub Vec<(String, Shortcut)>);

impl Shortcuts {
    fn get_mut(&mut self, key: &str) -> Option<&mut Shortcut> {
        self.0
            .iter_mut()
            .find(|(slot, _)| slot == key)
            .map(|(_, shortcut)| shortcut)
    }
}

impl Serialize for Shortcuts {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(self.0.len()))?;
        for (slot, shortcut) in &self.0 {
            map.serialize_entry(slot, shortcut)?;
        }
        map.end()
    }
}

impl<'de> Deserialize<'de> for Shortcuts {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct InOrder;
        impl<'de> Visitor<'de> for InOrder {
            type Value = Shortcuts;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("shortcuts by id")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Shortcuts, A::Error> {
                let mut shortcuts = Vec::new();
                while let Some(entry) = map.next_entry::<String, Shortcut>()? {
                    shortcuts.push(entry);
                }
                Ok(Shortcuts(shortcuts))
            }
        }
        deserializer.deserialize_map(InOrder)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct State {
    pub shortcuts: Shortcuts,
    pub is_loaded: bool,
    #[serde(skip)]
    pub platform: Platform,
}

impl State {
    /// The platform's defaults, not yet loaded from settings.
    #[must_use]
    pub fn new(platform: Platform) -> Self {
        Self {
            shortcuts: defaults(platform),
            is_loaded: false,
            platform,
        }
    }
}

#[must_use]
pub fn defaults(platform: Platform) -> Shortcuts {
    Shortcuts(
        DEFAULTS
            .iter()
            .map(|(id, label, description, desktop, web)| {
                let shortcut = Shortcut {
                    id: (*id).to_owned(),
                    label: (*label).to_owned(),
                    description: (*description).to_owned(),
                    default_key: (if platform.desktop { desktop } else { web }).to_string(),
                    custom_key: None,
                };
                ((*id).to_owned(), shortcut)
            })
            .collect(),
    )
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Event {
    /// Settings loaded: replace every binding.
    SetShortcuts(Shortcuts),
    /// Bind `id` to a key; the default key clears the custom one.
    UpdateShortcut(String, String),
    ResetShortcut(String),
    ResetAllShortcuts,
}

pub fn step(state: &mut State, event: Event) {
    match event {
        Event::SetShortcuts(shortcuts) => {
            state.shortcuts = shortcuts;
            state.is_loaded = true;
        }
        Event::UpdateShortcut(id, key) => {
            if let Some(shortcut) = state.shortcuts.get_mut(&id) {
                let is_default = shortcuts_equal(&key, &shortcut.default_key);
                shortcut.custom_key = (!is_default).then_some(key);
            }
        }
        Event::ResetShortcut(id) => {
            if let Some(shortcut) = state.shortcuts.get_mut(&id) {
                shortcut.custom_key = None;
            }
        }
        Event::ResetAllShortcuts => state.shortcuts = defaults(state.platform),
    }
}

/// A key event as the browser reports it.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct KeyEvent {
    pub key: String,
    pub code: String,
    pub ctrl_key: bool,
    pub meta_key: bool,
    pub shift_key: bool,
    pub alt_key: bool,
}

/// The other shortcut bound to `key`, skipping `exclude_id`.
#[must_use]
pub fn find_conflicting_shortcut<'a>(
    shortcuts: &'a Shortcuts,
    key: &str,
    exclude_id: &str,
) -> Option<&'a Shortcut> {
    shortcuts
        .0
        .iter()
        .map(|(_, shortcut)| shortcut)
        .find(|shortcut| shortcut.id != exclude_id && shortcuts_equal(shortcut.active_key(), key))
}

const MODIFIER_ORDER: [&str; 4] = ["ctrl", "cmd", "shift", "alt"];

/// Older builds saved arrows as `downarrow`; they must match `arrowdown`.
fn legacy_arrow(key: &str) -> &str {
    match key {
        "downarrow" => "arrowdown",
        "uparrow" => "arrowup",
        "leftarrow" => "arrowleft",
        "rightarrow" => "arrowright",
        other => other,
    }
}

fn canonical(key: &str) -> String {
    let parts: Vec<&str> = key.split('+').filter(|part| !part.is_empty()).collect();
    let Some((raw, modifiers)) = parts.split_last() else {
        return key.to_owned();
    };
    let mut ordered: Vec<&str> = MODIFIER_ORDER
        .iter()
        .copied()
        .filter(|modifier| modifiers.contains(modifier))
        .collect();
    ordered.push(legacy_arrow(raw));
    ordered.join("+")
}

fn shortcuts_equal(left: &str, right: &str) -> bool {
    canonical(left) == canonical(right)
}

/// A key event in the stored key format, e.g. `ctrl+shift+p`.
#[must_use]
pub fn normalize_key_event(event: &KeyEvent, platform: Platform) -> String {
    let mut parts: Vec<String> = Vec::new();
    if platform.mac {
        // Both may be held on macOS (cmd+ctrl+t); each is its own token.
        if event.ctrl_key {
            parts.push("ctrl".into());
        }
        if event.meta_key {
            parts.push("cmd".into());
        }
    } else if event.ctrl_key {
        parts.push("ctrl".into());
    } else if event.meta_key {
        parts.push("cmd".into());
    }
    if event.shift_key {
        parts.push("shift".into());
    }
    if event.alt_key {
        parts.push("alt".into());
    }
    let key = match event.key.to_lowercase().as_str() {
        " " => "space".to_owned(),
        "escape" => "esc".to_owned(),
        "-" | "\u{2013}" | "\u{2014}" | "_" => "-".to_owned(),
        "=" | "+" => "=".to_owned(),
        other => other.to_owned(),
    };
    if !matches!(key.as_str(), "control" | "alt" | "shift" | "meta") {
        parts.push(key);
    }
    parts.join("+")
}

/// A key combination for display, e.g. `Ctrl+Shift+P` (or glyphs on macOS).
#[must_use]
pub fn format_key_for_display(key: &str, platform: Platform) -> String {
    if key.is_empty() {
        return String::new();
    }
    let mac = platform.mac;
    let parts: Vec<String> = key
        .split('+')
        .map(|part| {
            match part {
                "ctrl" => {
                    if mac {
                        "\u{2303}"
                    } else {
                        "Ctrl"
                    }
                }
                "cmd" => {
                    if mac {
                        "\u{2318}"
                    } else {
                        "Meta"
                    }
                }
                "alt" => {
                    if mac {
                        "\u{2325}"
                    } else {
                        "Alt"
                    }
                }
                "shift" => {
                    if mac {
                        "\u{21e7}"
                    } else {
                        "Shift"
                    }
                }
                "tab" => "Tab",
                "esc" => "Esc",
                "space" => "Space",
                "pageup" => "PageUp",
                "pagedown" => "PageDown",
                other => return other.to_uppercase(),
            }
            .to_owned()
        })
        .collect();
    parts.join(if mac { "" } else { "+" })
}

/// Whether a key event triggers `shortcut_key`.
#[must_use]
pub fn matches_shortcut(event: &KeyEvent, shortcut_key: &str, platform: Platform) -> bool {
    // On macOS Ctrl without Cmd belongs to the shell, never to the app.
    if platform.mac && event.ctrl_key && !event.meta_key {
        return false;
    }
    let normalized = normalize_key_event(event, platform);
    if matches_normalized(&normalized, shortcut_key, platform) {
        return true;
    }
    // macOS Option turns a letter into a symbol, so with Alt held a letter
    // key also matches by its physical key.
    let letter = event
        .code
        .strip_prefix("Key")
        .filter(|rest| rest.len() == 1 && rest.as_bytes()[0].is_ascii_uppercase())
        .map(str::to_ascii_lowercase);
    if let (true, Some(letter)) = (event.alt_key, letter) {
        let mut parts: Vec<&str> = normalized.split('+').collect();
        if let Some(last) = parts.last_mut() {
            *last = &letter;
        }
        return matches_normalized(&parts.join("+"), shortcut_key, platform);
    }
    false
}

fn matches_normalized(normalized: &str, shortcut_key: &str, platform: Platform) -> bool {
    if shortcuts_equal(normalized, shortcut_key) {
        return true;
    }
    if !platform.mac {
        return false;
    }
    // On macOS a single cmd or ctrl modifier matches the other.
    let parts: Vec<&str> = normalized.split('+').collect();
    let has_cmd = parts.contains(&"cmd");
    let has_ctrl = parts.contains(&"ctrl");
    if has_cmd == has_ctrl {
        return false;
    }
    let aliased: Vec<&str> = parts
        .iter()
        .map(|part| match *part {
            "cmd" => "ctrl",
            "ctrl" => "cmd",
            other => other,
        })
        .collect();
    shortcuts_equal(&aliased.join("+"), shortcut_key)
}

/// The shortcut a key event fires, in settings order; an empty binding never
/// fires.
#[must_use]
pub fn find_match<'a>(state: &'a State, event: &KeyEvent) -> Option<&'a Shortcut> {
    state
        .shortcuts
        .0
        .iter()
        .map(|(_, shortcut)| shortcut)
        .find(|s| !s.active_key().is_empty() && matches_shortcut(event, s.active_key(), state.platform))
}

impl Shortcuts {
    /// Every binding's `customKey` over the platform's defaults, as
    /// `mergeWithDefaults` reads a saved record: shortcuts added since stay
    /// on their defaults and unknown ids drop.
    #[must_use]
    pub fn merged(saved: &serde_json::Value, platform: Platform) -> Shortcuts {
        let mut merged = defaults(platform);
        for (id, shortcut) in &mut merged.0 {
            shortcut.custom_key = saved
                .get(id.as_str())
                .and_then(|s| s.get("customKey"))
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned);
        }
        merged
    }

    #[must_use]
    pub fn get(&self, id: &str) -> Option<&Shortcut> {
        self.0.iter().find(|(slot, _)| slot == id).map(|(_, s)| s)
    }
}

#[cfg(test)]
#[path = "shortcuts_tests.rs"]
mod tests;
