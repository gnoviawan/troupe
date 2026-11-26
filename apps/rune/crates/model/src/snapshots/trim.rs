//! `trimSnapshotList` from `lib/persistence-trim.ts`: a project's snapshot list cut to the
//! host store's value cap. The output inside older snapshots goes first, then the newest
//! snapshot's output as little as needed, and whole snapshots only last, oldest first. The
//! newest snapshot is never removed. Sizes are the UTF-8 length of the JSON the host stores
//! (`{ _version: 1, data }`), as `jsonBytes` measures it.

use serde::Serialize;
use serde_json::{Value, json};

/// The largest serialized value the host store accepts (`STORE_VALUE_MAX_BYTES`).
pub const STORE_VALUE_MAX_BYTES: usize = 256 * 1024;

/// The granularity of the output share a trimmed terminal keeps (1/`KEEP_STEPS`).
const KEEP_STEPS: usize = 1024;

/// What a trim did (`TrimmedSnapshotList`).
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Trimmed {
    pub list: Value,
    /// Snapshots whose saved terminal output was cut.
    pub shrunk: usize,
    /// Ids of snapshots removed entirely, oldest first.
    pub dropped_ids: Vec<String>,
}

fn json_bytes(value: &Value) -> usize {
    serde_json::to_string(value).map_or(0, |text| text.len())
}

fn stored_bytes(data: &Value) -> usize {
    json_bytes(&json!({ "_version": 1, "data": data }))
}

/// The largest `n` in `[0, max)` for which `fits(n)` holds; `max` is known not to fit.
fn largest_fitting(max: usize, fits: impl Fn(usize) -> bool) -> usize {
    let (mut lo, mut hi) = (0, max);
    while hi - lo > 1 {
        let mid = (lo + hi) / 2;
        if fits(mid) {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    lo
}

fn is_line_break(unit: u16) -> bool {
    unit == u16::from(b'\n') || unit == u16::from(b'\r')
}

/// The newest `keep` UTF-16 units of `text`, starting at a clean line boundary (`tailChars`).
fn tail_chars(text: &str, keep: usize) -> String {
    let units: Vec<u16> = text.encode_utf16().collect();
    if keep >= units.len() {
        return text.to_owned();
    }
    if keep == 0 {
        return String::new();
    }
    let cut = units.len() - keep;
    let mut tail = &units[cut..];
    if tail.first().is_some_and(|u| (0xdc00..=0xdfff).contains(u)) {
        tail = &tail[1..];
    }
    if !is_line_break(units[cut - 1])
        && let Some(at) = tail.iter().position(|&u| is_line_break(u))
    {
        let crlf = tail[at] == u16::from(b'\r') && tail.get(at + 1) == Some(&u16::from(b'\n'));
        tail = &tail[at + if crlf { 2 } else { 1 }..];
    }
    String::from_utf16_lossy(tail)
}

/// `terminal` with only the newest `step / KEEP_STEPS` of its output kept (`shrinkTerminal`).
fn shrink_terminal(terminal: &Value, step: usize) -> Value {
    let Some(fields) = terminal.as_object() else {
        return terminal.clone();
    };
    let mut next = fields.clone();
    next.shift_remove("scrollback");
    next.shift_remove("transcript");
    if let Some(lines) = fields.get("scrollback").and_then(Value::as_array) {
        let keep = lines.len() * step / KEEP_STEPS;
        if keep > 0 {
            next.insert("scrollback".into(), Value::Array(lines[lines.len() - keep..].to_vec()));
        }
    }
    if let Some(text) = fields.get("transcript").and_then(Value::as_str) {
        let keep = text.encode_utf16().count() * step / KEEP_STEPS;
        let tail = tail_chars(text, keep);
        if !tail.is_empty() {
            next.insert("transcript".into(), Value::String(tail));
        }
    }
    Value::Object(next)
}

/// `withTerminalOutput`: every terminal of `snapshot` shrunk to `step`.
fn with_terminal_output(snapshot: &Value, step: usize) -> Value {
    let mut next = snapshot.clone();
    if let Some(terminals) = next.get_mut("terminals").and_then(Value::as_array_mut) {
        for terminal in terminals.iter_mut() {
            *terminal = shrink_terminal(terminal, step);
        }
    }
    next
}

/// Cuts `list` (`{ snapshots, updatedAt }`, newest snapshot first) to fit `cap`.
#[must_use]
pub fn trim(list: &Value, cap: usize) -> Trimmed {
    let unchanged = || Trimmed {
        list: list.clone(),
        shrunk: 0,
        dropped_ids: Vec::new(),
    };
    if stored_bytes(list) <= cap {
        return unchanged();
    }
    let Some(mut snapshots) = list.get("snapshots").and_then(Value::as_array).cloned() else {
        return unchanged();
    };
    let mut empty = list.clone();
    empty["snapshots"] = json!([]);
    let base = stored_bytes(&empty);
    let mut sizes: Vec<usize> = snapshots.iter().map(json_bytes).collect();
    let total = |sizes: &[usize]| base + sizes.iter().sum::<usize>() + sizes.len().saturating_sub(1);

    let mut shrunk = 0;
    let mut i = snapshots.len().saturating_sub(1);
    while i > 0 && total(&sizes) > cap {
        let stripped = with_terminal_output(&snapshots[i], 0);
        let size = json_bytes(&stripped);
        if size < sizes[i] {
            shrunk += 1;
        }
        snapshots[i] = stripped;
        sizes[i] = size;
        i -= 1;
    }
    if total(&sizes) > cap && !snapshots.is_empty() {
        let others = total(&sizes) - sizes[0];
        let newest = snapshots[0].clone();
        let step = largest_fitting(KEEP_STEPS, |s| others + json_bytes(&with_terminal_output(&newest, s)) <= cap);
        snapshots[0] = with_terminal_output(&newest, step);
        let size = json_bytes(&snapshots[0]);
        if size < sizes[0] {
            shrunk += 1;
        }
        sizes[0] = size;
    }
    let mut dropped_ids = Vec::new();
    while total(&sizes) > cap && snapshots.len() > 1 {
        if let Some(dropped) = snapshots.pop() {
            dropped_ids.push(dropped["id"].as_str().unwrap_or_default().to_owned());
        }
        sizes.pop();
    }
    let mut list = list.clone();
    list["snapshots"] = Value::Array(snapshots);
    Trimmed {
        list,
        shrunk,
        dropped_ids,
    }
}
