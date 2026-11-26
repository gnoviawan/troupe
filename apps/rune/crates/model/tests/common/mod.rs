//! What the vector replays share: the recorded files, the id source, the
//! event decoding and the JSON comparison.

#![allow(dead_code)]

use std::collections::VecDeque;
use std::path::{Path, PathBuf};

use model::Ids;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use sha1::{Digest, Sha1};

pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..")
}

pub fn out_dir() -> PathBuf {
    repo_root().join("tools/parity/vectors/out")
}

pub fn read_json(path: &Path) -> Value {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// Hands out the ids the TypeScript run drew during one step.
#[derive(Default)]
pub struct Replay {
    pub ids: VecDeque<String>,
    pub overdrawn: usize,
}

impl Ids for Replay {
    fn next_id(&mut self) -> String {
        self.ids.pop_front().unwrap_or_else(|| {
            self.overdrawn += 1;
            format!("extra-{}", self.overdrawn)
        })
    }
}

/// A recorded `{action, args}` as the store's event enum: `action` tags the
/// variant and `args` are its fields (a lone argument is a newtype variant,
/// no argument a unit variant).
pub fn call<T: DeserializeOwned>(event: &Value) -> Result<T, String> {
    let action = event["action"].as_str().ok_or("event without action")?;
    let args = event["args"].as_array().cloned().unwrap_or_default();
    let mut shapes = vec![json!({ action: args })];
    match args.as_slice() {
        [] => shapes.push(json!(action)),
        [only] => shapes.push(json!({ action: only })),
        _ => {}
    }
    let mut last = String::new();
    for shape in shapes {
        match serde_json::from_value(shape) {
            Ok(parsed) => return Ok(parsed),
            Err(error) => last = error.to_string(),
        }
    }
    Err(format!("{action}: {last}"))
}

pub fn to_value<T: Serialize>(value: &T) -> Value {
    serde_json::to_value(value).expect("serializable")
}

/// A value for a failure message: long ones cut, with their length.
fn brief(value: &Value) -> String {
    let text = value.to_string();
    if text.chars().count() <= 300 {
        return text;
    }
    let head: String = text.chars().take(300).collect();
    format!("{head}... ({} chars)", text.chars().count())
}

/// JSON equality with numbers compared as f64 (`50` equals `50.0`).
pub fn diff(path: &str, want: &Value, got: &Value) -> Option<String> {
    match (want, got) {
        (Value::Number(a), Value::Number(b)) => {
            (a.as_f64() != b.as_f64()).then(|| format!("{path}: want {a}, got {b}"))
        }
        (Value::Array(a), Value::Array(b)) => {
            if a.len() != b.len() {
                return Some(format!(
                    "{path}: want {} items, got {}: {}",
                    a.len(),
                    b.len(),
                    brief(got)
                ));
            }
            a.iter()
                .zip(b)
                .enumerate()
                .find_map(|(i, (a, b))| diff(&format!("{path}[{i}]"), a, b))
        }
        (Value::Object(a), Value::Object(b)) => {
            let keys: std::collections::BTreeSet<&String> = a.keys().chain(b.keys()).collect();
            keys.into_iter()
                .find_map(|key| match (a.get(key), b.get(key)) {
                    (Some(a), Some(b)) => diff(&format!("{path}.{key}"), a, b),
                    (Some(a), None) => Some(format!("{path}.{key}: want {}, missing", brief(a))),
                    (None, Some(b)) => Some(format!("{path}.{key}: unexpected {}", brief(b))),
                    (None, None) => None,
                })
        }
        _ => (want != got).then(|| format!("{path}: want {}, got {}", brief(want), brief(got))),
    }
}

pub fn parse<T: DeserializeOwned>(value: &Value) -> Result<T, String> {
    serde_json::from_value(value.clone()).map_err(|e| e.to_string())
}

/// Same hash as `sourceHash` in `tools/parity/vectors/record.ts`.
pub fn source_hash(repo_path: &str) -> String {
    let text = std::fs::read_to_string(repo_root().join(repo_path))
        .unwrap_or_else(|e| panic!("{repo_path}: {e}"))
        .replace("\r\n", "\n");
    Sha1::digest(text.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
