//! Replays the snapshot store's vectors against `model::snapshots`. Each recorded action runs
//! to completion: the model's reads are answered from the vector's `context.reads` and
//! `context.readFailures`, its writes from `context.writeFailures`, as the recorder's mocked
//! host store answered them.

use std::collections::HashMap;

use model::snapshots::{self, Effect, Event, Read, State};
use serde_json::{Value, json};

use crate::common::{diff, parse, to_value};

/// The id and `createdAt` the recorder's fixed clock and `Math.random` give a new snapshot.
const NEW_ID: &str = "1795168800000-4fzzzxjyl";
const NEW_AT: &str = "2026-11-20T10:00:00.000Z";

#[derive(Default)]
struct Host {
    reads: HashMap<String, Value>,
    read_failures: HashMap<String, String>,
    write_failures: HashMap<String, String>,
}

impl Host {
    fn read(&self, key: &str) -> Read {
        if let Some(code) = self.read_failures.get(key) {
            return Read::Failed { code: code.clone(), error: key.to_owned() };
        }
        match self.reads.get(key) {
            Some(data) => Read::Found(data.clone()),
            None => Read::Missing { error: key.to_owned() },
        }
    }
}

fn map(context: &Value, field: &str) -> HashMap<String, Value> {
    context[field].as_object().map(|o| o.iter().map(|(k, v)| (k.clone(), v.clone())).collect()).unwrap_or_default()
}

/// Runs `fx` and what answering it brings until nothing is left; the recorded effects and
/// the action's outcome.
fn drive(state: &mut State, host: &Host, mut fx: Vec<Effect>) -> (Vec<Value>, Option<Value>) {
    let mut seen = Vec::new();
    let mut outcome = None;
    while !fx.is_empty() {
        let mut next = Vec::new();
        for effect in fx {
            match effect {
                Effect::Read { key, project_id, load } => {
                    let read = host.read(&key);
                    let event = if load {
                        Event::Loaded { project_id, read }
                    } else {
                        Event::Read { project_id, read }
                    };
                    next.extend(state.step(event));
                }
                Effect::WriteSnapshots { ref key, .. } => {
                    let result = host.write_failures.get(key).map_or(Ok(()), |e| Err(e.clone()));
                    let project_id = key.trim_start_matches("snapshots/").to_owned();
                    seen.push(to_value(&effect));
                    next.extend(state.step(Event::Written { project_id, result }));
                }
                Effect::Settled { error, .. } => {
                    outcome = Some(match error {
                        Some(error) => json!({ "error": error }),
                        None => json!({ "ok": true }),
                    });
                }
                other => seen.push(to_value(&other)),
            }
        }
        fx = next;
    }
    (seen, outcome)
}

pub fn replay(vector: &Value) -> Result<usize, String> {
    let context = &vector["context"];
    let mut host = Host {
        reads: map(context, "reads"),
        read_failures: map(context, "readFailures").into_iter().map(|(k, v)| (k, v.as_str().unwrap_or_default().to_owned())).collect(),
        write_failures: map(context, "writeFailures").into_iter().map(|(k, v)| (k, v.as_str().unwrap_or_default().to_owned())).collect(),
    };
    let mut state = State::default();
    let steps = vector["steps"].as_array().ok_or("no steps")?;
    for (index, step) in steps.iter().enumerate() {
        let event = &step["event"];
        let args = &event["args"];
        let text = |i: usize| args[i].as_str().unwrap_or_default().to_owned();
        let (effects, result) = match event["action"].as_str().unwrap_or_default() {
            "loadSnapshots" => {
                let fx = state.step(Event::Load(text(0)));
                (drive(&mut state, &host, fx).0, Value::Null)
            }
            "createSnapshot" => {
                let fx = state.step(Event::Create {
                    id: NEW_ID.into(),
                    created_at: NEW_AT.into(),
                    name: text(0),
                    description: args[1].as_str().map(str::to_owned),
                    project_id: text(2),
                    terminals: parse(&args[3])?,
                    active_terminal_id: args[4].as_str().map(str::to_owned),
                });
                let created = state.find(NEW_ID).map(to_value);
                let (seen, outcome) = drive(&mut state, &host, fx);
                let outcome = match outcome {
                    Some(o) if o.get("ok").is_some() => json!({ "ok": created }),
                    other => other.unwrap_or(Value::Null),
                };
                (seen, outcome)
            }
            "renameSnapshot" => {
                let fx = state.step(Event::Rename { id: text(0), name: text(1) });
                let (seen, outcome) = drive(&mut state, &host, fx);
                (seen, outcome.unwrap_or(Value::Null))
            }
            "deleteSnapshot" => {
                let fx = state.step(Event::Delete(text(0)));
                let (seen, outcome) = drive(&mut state, &host, fx);
                (seen, outcome.unwrap_or(Value::Null))
            }
            "getSnapshot" => {
                let got = state
                    .find(&text(0))
                    .and_then(|s| match host.read(&snapshots::key(&s.project_id)) {
                        Read::Found(list) => snapshots::saved(&list, &text(0)),
                        _ => None,
                    })
                    .unwrap_or(Value::Null);
                (Vec::new(), got)
            }
            "trim" => {
                let cap = args[1].as_u64().ok_or("trim without a cap")? as usize;
                (Vec::new(), to_value(&snapshots::trim::trim(&args[0], cap)))
            }
            "hostForgets" => {
                host.reads.remove(&text(0));
                (Vec::new(), Value::Null)
            }
            "hostFails" => {
                host.read_failures.insert(text(0), text(1));
                (Vec::new(), Value::Null)
            }
            other => return Err(format!("step {index}: no action {other}")),
        };
        let checks = [
            ("effects", &step["effects"], &Value::Array(effects)),
            ("result", step.get("result").unwrap_or(&Value::Null), &result),
            ("state", &step["state"], &to_value(&state)),
        ];
        for (what, want, got) in checks {
            if let Some(problem) = diff(&format!("step {index} {what}"), want, got) {
                return Err(problem);
            }
        }
    }
    Ok(steps.len())
}
