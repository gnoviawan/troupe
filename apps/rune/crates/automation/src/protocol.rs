//! The wire protocol and the commands that act on a `Runtime` directly. Pure of sockets and GPU,
//! so tests run them on a headless `Runtime` with no window.

use std::path::PathBuf;

use rune::{Key, KeyEvent, Mods, Runtime};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, PartialEq, Deserialize)]
#[serde(tag = "cmd", rename_all = "lowercase")]
pub enum Command {
    Tree,
    A11y,
    Find(Target),
    Wait {
        #[serde(flatten)]
        target: Target,
        #[serde(default = "default_timeout_ms")]
        timeout_ms: u64,
    },
    Click(ClickAt),
    Type {
        text: String,
    },
    Key {
        name: String,
        #[serde(default)]
        mods: Vec<Modifier>,
    },
    Snap {
        path: PathBuf,
    },
}

fn default_timeout_ms() -> u64 {
    5000
}

/// What `find` and `wait` look for: an element's key, or its text or label.
#[derive(Debug, PartialEq, Deserialize)]
#[serde(untagged)]
pub enum Target {
    Key { key: String },
    Text { text: String },
}

#[derive(Debug, PartialEq, Deserialize)]
#[serde(untagged)]
pub enum ClickAt {
    Key { key: String },
    Point { x: f32, y: f32 },
}

#[derive(Debug, PartialEq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Modifier {
    /// The primary shortcut modifier: Cmd on macOS, Ctrl elsewhere.
    Cmd,
    Ctrl,
    Shift,
    Alt,
    Logo,
}

/// What a client sees of one element.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Element {
    pub key: Option<String>,
    pub path: String,
    pub role: &'static str,
    pub label: Option<String>,
    pub text: Option<String>,
    pub rect: [f32; 4],
    pub focused: bool,
}

pub type Reply = Result<Value, String>;

impl Target {
    pub fn matches(&self, e: &Element) -> bool {
        match self {
            Target::Key { key } => e.key.as_deref() == Some(key),
            Target::Text { text } => {
                e.text.as_deref() == Some(text) || e.label.as_deref() == Some(text)
            }
        }
    }
}

/// The elements on screen, without those playing an exit transition.
pub fn elements<S: 'static>(rt: &Runtime<S>) -> Vec<Element> {
    rt.elements()
        .into_iter()
        .filter(|e| !e.exiting)
        .map(|e| Element {
            key: e.key,
            path: e.path,
            role: e.role,
            label: e.label,
            text: e.text,
            rect: e.rect,
            focused: e.focused,
        })
        .collect()
}

pub fn find(elements: &[Element], target: &Target) -> Reply {
    elements
        .iter()
        .find(|e| target.matches(e))
        .map(|e| json!({ "element": e }))
        .ok_or_else(|| "not found".to_string())
}

/// Runs `cmd` on `rt` now. `wait` and `snap` need the live window's frames and return an error.
pub fn act<S: 'static>(rt: &mut Runtime<S>, cmd: &Command) -> Reply {
    match cmd {
        Command::Tree => Ok(json!({ "elements": elements(rt) })),
        Command::A11y => Ok(json!({ "a11y": rt.accessibility_json() })),
        Command::Find(target) => find(&elements(rt), target),
        Command::Click(at) => {
            let (x, y) = match at {
                ClickAt::Key { key } => rt
                    .find(key)
                    .ok_or_else(|| format!("no element keyed {key}"))?,
                ClickAt::Point { x, y } => (*x, *y),
            };
            rt.pointer_move(x, y);
            rt.pointer_down(x, y);
            rt.pointer_up(x, y);
            Ok(json!({ "at": [x, y] }))
        }
        Command::Type { text } => {
            for c in text.chars() {
                rt.key(KeyEvent {
                    key: Key::Text(c.to_string()),
                    mods: Mods::default(),
                });
            }
            Ok(json!({}))
        }
        Command::Key { name, mods } => {
            rt.key(key_event(name, mods)?);
            Ok(json!({}))
        }
        Command::Wait { .. } | Command::Snap { .. } => {
            Err("wait and snap need the live window".into())
        }
    }
}

/// `act`, as the JSON reply a client reads.
pub fn respond<S: 'static>(rt: &mut Runtime<S>, cmd: &Command) -> Value {
    reply_json(act(rt, cmd))
}

pub fn reply_json(reply: Reply) -> Value {
    match reply {
        Ok(mut v) => {
            v["ok"] = json!(true);
            v
        }
        Err(error) => json!({ "ok": false, "error": error }),
    }
}

/// Mirrors what the platform sends: off macOS, Ctrl also sets `cmd`, and a character typed with
/// `cmd` arrives as `Key::Command`.
pub fn key_event(name: &str, mods: &[Modifier]) -> Result<KeyEvent, String> {
    let mut m = Mods::default();
    for modifier in mods {
        match modifier {
            Modifier::Cmd if cfg!(target_os = "macos") => m.cmd = true,
            Modifier::Cmd | Modifier::Ctrl => {
                m.ctrl = true;
                m.cmd |= !cfg!(target_os = "macos");
            }
            Modifier::Shift => m.shift = true,
            Modifier::Alt => m.alt = true,
            Modifier::Logo => m.logo = true,
        }
    }
    let key = match name {
        "Enter" => Key::Enter,
        "Tab" => Key::Tab,
        "Escape" => Key::Escape,
        "Backspace" => Key::Backspace,
        "Delete" => Key::Delete,
        "Left" => Key::Left,
        "Right" => Key::Right,
        "Up" => Key::Up,
        "Down" => Key::Down,
        "Home" => Key::Home,
        "End" => Key::End,
        "PageUp" => Key::PageUp,
        "PageDown" => Key::PageDown,
        "Space" => Key::Text(" ".into()),
        _ => {
            let mut chars = name.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) if m.cmd => Key::Command(c.to_ascii_lowercase()),
                (Some(c), None) => Key::Text(c.to_string()),
                _ => return Err(format!("unknown key {name}")),
            }
        }
    };
    Ok(KeyEvent { key, mods: m })
}

#[cfg(test)]
#[path = "protocol_tests.rs"]
mod tests;
