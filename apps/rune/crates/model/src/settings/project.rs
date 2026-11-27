//! The project settings window's pure logic: the draft it edits and the save that normalizes it
//! (`pages/ProjectSettings.tsx`), the `.env` import (`lib/env-parser.ts`) and the shell select
//! (`components/settings/shell-options.ts`).

use serde::{Deserialize, Serialize};

use super::normalize::env_key;
use crate::js_trim;
use crate::prefs::{ClientFields, EnvVar};
use crate::projects::{Project, ProjectUpdates};

/// What the window edits, as `ProjectSettingsModal` keeps it in state.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Draft {
    pub name: String,
    pub color: String,
    pub path: String,
    /// `''` follows the app default shell.
    pub shell: String,
    pub env_vars: Vec<EnvVar>,
    pub symlink_dirs: Vec<String>,
}

impl Draft {
    /// `resetForm`: every field as the project saved it.
    #[must_use]
    pub fn of(project: &Project) -> Self {
        Self {
            name: project.name.clone(),
            color: project.color.clone(),
            path: project.path.clone().unwrap_or_default(),
            shell: project.client.default_shell.clone().unwrap_or_default(),
            env_vars: project.client.env_vars.clone().unwrap_or_default(),
            symlink_dirs: project.client.symlink_dirs.clone().unwrap_or_default(),
        }
    }

    /// `handleSave`: keys trimmed and blank ones dropped, directories trimmed and blank ones
    /// dropped, then every field as one `updateProject`.
    #[must_use]
    pub fn updates(&self) -> ProjectUpdates {
        let env_vars = self
            .env_vars
            .iter()
            .map(|v| EnvVar { key: js_trim(&v.key).to_owned(), ..v.clone() })
            .filter(|v| !v.key.is_empty())
            .collect();
        let symlink_dirs = self.symlink_dirs.iter().map(|d| js_trim(d).to_owned()).filter(|d| !d.is_empty()).collect();
        ProjectUpdates {
            name: Some(self.name.clone()),
            color: Some(self.color.clone()),
            path: Some(self.path.clone()),
            client: ClientFields {
                default_shell: Some(self.shell.clone()),
                env_vars: Some(env_vars),
                symlink_dirs: Some(symlink_dirs),
                ..ClientFields::default()
            },
        }
    }
}

/// `resolveProjectEnvPath`: the `.env` right under the root, with the root's own separator.
#[must_use]
pub fn env_path(root: &str) -> String {
    let bytes = root.as_bytes();
    let drive = bytes.len() >= 3 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && matches!(bytes[2], b'\\' | b'/');
    let windows = drive || root.starts_with("\\\\");
    let separator = if windows && root.contains('\\') { '\\' } else { '/' };
    let bare = if windows { root.trim_end_matches(['\\', '/']) } else { root.trim_end_matches('/') };
    format!("{bare}{separator}.env")
}

/// A line `parse_env` skipped, numbered from 1, as written.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvalidLine {
    pub line: usize,
    pub content: String,
}

/// `EnvParseResult`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvParse {
    pub vars: Vec<EnvVar>,
    pub invalid_lines: Vec<InvalidLine>,
}

/// `parseEnvFile`: `KEY=value` lines; blank lines and `#` comments skipped; one pair of
/// matching quotes stripped; anything else is an invalid line.
#[must_use]
pub fn parse_env(content: &str) -> EnvParse {
    let mut out = EnvParse::default();
    for (i, raw) in content.split('\n').enumerate() {
        let raw = raw.strip_suffix('\r').unwrap_or(raw);
        let line = js_trim(raw);
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some(eq) = line.find('=') else {
            out.invalid_lines.push(InvalidLine { line: i + 1, content: raw.to_owned() });
            continue;
        };
        let key = js_trim(&line[..eq]);
        if !env_key(key) {
            out.invalid_lines.push(InvalidLine { line: i + 1, content: raw.to_owned() });
            continue;
        }
        let mut value = &line[eq + 1..];
        let quoted = |q: char| value.starts_with(q) && value.ends_with(q);
        if quoted('"') || quoted('\'') {
            value = value.get(1..value.len() - 1).unwrap_or_default();
        }
        out.vars.push(EnvVar { key: key.to_owned(), value: value.to_owned(), is_secret: None });
    }
    out
}

/// `mergeEnvVars`: a key keeps its first place; the last value for it wins.
#[must_use]
pub fn merge_env(existing: &[EnvVar], imported: &[EnvVar]) -> Vec<EnvVar> {
    let mut merged: Vec<EnvVar> = Vec::new();
    for var in existing.iter().chain(imported) {
        match merged.iter_mut().find(|m| m.key == var.key) {
            Some(slot) => *slot = var.clone(),
            None => merged.push(var.clone()),
        }
    }
    merged
}

/// What an import of `.env` text does to the window (`handleImportEnvFile` after its read).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Import {
    /// The merged list, when at least one line parsed; the draft is then dirty.
    pub vars: Option<Vec<EnvVar>>,
    pub error: Option<String>,
    pub warning: Option<String>,
}

#[must_use]
pub fn import(existing: &[EnvVar], content: &str) -> Import {
    let parsed = parse_env(content);
    if parsed.vars.is_empty() && parsed.invalid_lines.is_empty() {
        return Import { error: Some("The .env file is empty.".into()), ..Import::default() };
    }
    let warning = (!parsed.invalid_lines.is_empty()).then(|| {
        let details = parsed.invalid_lines.iter().take(3).map(|l| format!("Line {}: {}", l.line, l.content)).collect::<Vec<_>>().join("\n");
        let skipped = parsed.invalid_lines.len();
        let more = if skipped > 3 { format!(" (+{} more)", skipped - 3) } else { String::new() };
        format!("Imported {} variables.\nSkipped {skipped} invalid line(s):\n{details}{more}", parsed.vars.len())
    });
    Import {
        vars: (!parsed.vars.is_empty()).then(|| merge_env(existing, &parsed.vars)),
        error: None,
        warning,
    }
}

/// A shell the host detected (`ShellInfo` from `/shells`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Shell {
    #[serde(default)]
    pub name: String,
    pub path: String,
    #[serde(default)]
    pub display_name: String,
}

/// The select's value for "no shell chosen" (`NO_SHELL_VALUE`).
pub const NO_SHELL: &str = "__system_default__";

/// `shellSelectValue`: a stored name picks the detected shell of that name or file name.
#[must_use]
pub fn shell_value(stored: &str, shells: &[Shell]) -> String {
    if stored.is_empty() {
        return NO_SHELL.to_owned();
    }
    if stored.contains(['\\', '/']) {
        return stored.to_owned();
    }
    shells
        .iter()
        .find(|s| s.name == stored || s.path.rsplit(['\\', '/']).next() == Some(stored))
        .map_or_else(|| stored.to_owned(), |s| s.path.clone())
}

/// One select option: the value and its label.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShellOption {
    pub value: String,
    pub label: String,
}

/// `shellSelectOptions`: the fallback, every detected shell, and the stored value when
/// detection did not find it.
#[must_use]
pub fn shell_options(value: &str, shells: &[Shell], fallback: &str) -> Vec<ShellOption> {
    let mut out = vec![ShellOption { value: NO_SHELL.to_owned(), label: fallback.to_owned() }];
    out.extend(shells.iter().map(|s| ShellOption { value: s.path.clone(), label: s.display_name.clone() }));
    if !out.iter().any(|o| o.value == value) {
        out.push(ShellOption { value: value.to_owned(), label: value.to_owned() });
    }
    out
}

/// `shellFromSelectValue`: the fallback stores `''`.
#[must_use]
pub fn shell_from_value(value: &str) -> String {
    if value == NO_SHELL { String::new() } else { value.to_owned() }
}

#[cfg(test)]
#[path = "project_tests.rs"]
mod tests;
