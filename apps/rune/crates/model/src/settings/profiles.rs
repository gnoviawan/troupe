//! Terminal profiles as the Profiles page edits them: the editor's draft, its parse into a
//! profile, and the command line a row shows. Port of `src/renderer/lib/terminal-profiles.ts`;
//! the stored list's normalize is `normalize.rs`.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::normalize::env_key;
use crate::js_trim;

/// The settings field holding the list.
pub const KEY: &str = "terminalProfiles";

/// One saved profile (`TerminalProfile`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub shell: String,
    pub args: Vec<String>,
    pub cwd: String,
    /// Kept in the order typed, as a JavaScript object keeps it.
    pub env: Map<String, Value>,
}

impl Profile {
    /// `terminalProfileCommandLine`.
    #[must_use]
    pub fn command_line(&self) -> String {
        let shell = if self.shell.is_empty() { "Default shell" } else { &self.shell };
        std::iter::once(shell).chain(self.args.iter().map(String::as_str)).collect::<Vec<_>>().join(" ")
    }

    /// The profiles in a `settings/app` record; normalize already dropped malformed ones.
    #[must_use]
    pub fn list(settings: &Map<String, Value>) -> Vec<Profile> {
        settings
            .get(KEY)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|p| serde_json::from_value(p.clone()).ok())
            .collect()
    }
}

/// The editor's fields as typed (`TerminalProfileDraft`): one argument per line, one
/// `KEY=value` per env line.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Draft {
    pub name: String,
    pub shell: String,
    pub args: String,
    pub cwd: String,
    pub env: String,
}

impl Draft {
    /// `terminalProfileToDraft`.
    #[must_use]
    pub fn of(profile: &Profile) -> Draft {
        Draft {
            name: profile.name.clone(),
            shell: profile.shell.clone(),
            args: profile.args.join("\n"),
            cwd: profile.cwd.clone(),
            env: profile
                .env
                .iter()
                .map(|(k, v)| format!("{k}={}", v.as_str().unwrap_or_default()))
                .collect::<Vec<_>>()
                .join("\n"),
        }
    }

    /// `parseTerminalProfileDraft`: `id` keeps an edited profile's identity.
    pub fn parse(&self, id: &str) -> Result<Profile, Errors> {
        let mut errors = Errors::default();
        let name = js_trim(&self.name);
        let shell = js_trim(&self.shell);
        let args: Vec<String> = lines(&self.args).filter(|a| !a.is_empty()).map(str::to_owned).collect();
        if name.is_empty() {
            errors.name = Some("Name is required.".into());
        }
        if !args.is_empty() && shell.is_empty() {
            errors.args = Some("Choose a shell to pass arguments to.".into());
        }
        let env = env_lines(&self.env);
        if let Err(line) = env {
            errors.env = Some(format!(
                "Line {line}: write KEY=value. A key starts with a letter or _ and holds only letters, digits and _."
            ));
        }
        match env {
            Ok(env) if errors == Errors::default() => Ok(Profile {
                id: id.to_owned(),
                name: name.to_owned(),
                shell: shell.to_owned(),
                args,
                cwd: js_trim(&self.cwd).to_owned(),
                env,
            }),
            _ => Err(errors),
        }
    }
}

/// `TerminalProfileDraftErrors`: the message under each field that failed.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Errors {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env: Option<String>,
}

/// `text.split(/\r?\n/)`.
fn lines(text: &str) -> impl Iterator<Item = &str> {
    text.split('\n').map(|l| l.strip_suffix('\r').unwrap_or(l))
}

/// `parseEnvLines`: blank and `#` lines are skipped; a value is kept exactly as typed. The
/// error is the 1-based line that is not `KEY=value`.
fn env_lines(text: &str) -> Result<Map<String, Value>, usize> {
    let mut env = Map::new();
    for (index, line) in lines(text).enumerate() {
        if js_trim(line).is_empty() || line.trim_start().starts_with('#') {
            continue;
        }
        let (key, value) = match line.find('=') {
            Some(eq) => (js_trim(&line[..eq]), &line[eq + 1..]),
            None => ("", ""),
        };
        if !env_key(key) {
            return Err(index + 1);
        }
        env.insert(key.to_owned(), Value::String(value.to_owned()));
    }
    Ok(env)
}

/// The list with `profile` replacing the one of its id, else added at the end (`save`).
#[must_use]
pub fn saved(list: &[Profile], profile: Profile) -> Vec<Profile> {
    let mut out = list.to_vec();
    match out.iter_mut().find(|p| p.id == profile.id) {
        Some(slot) => *slot = profile,
        None => out.push(profile),
    }
    out
}

/// The undo of a delete: `profile` back at `index`, unless a profile of its id is there now.
#[must_use]
pub fn restored(list: &[Profile], profile: Profile, index: usize) -> Option<Vec<Profile>> {
    if list.iter().any(|p| p.id == profile.id) {
        return None;
    }
    let mut out = list.to_vec();
    out.insert(index.min(out.len()), profile);
    Some(out)
}

#[cfg(test)]
#[path = "profiles_tests.rs"]
mod tests;
