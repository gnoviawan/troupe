//! Replays the ACP chat store's vectors (`tools/parity/vectors/out/acp/`,
//! recorded by `record-acp.test.ts`) through `model::acp` and compares the
//! state and the effects after every step. A second test fails when a
//! TypeScript source changed after the vectors were recorded.

mod common;

use common::{Replay, diff, out_dir, parse, read_json, source_hash, to_value};
use std::collections::VecDeque;

use model::acp::prompt::{DispatchOutcome, PromptSettled};
use model::acp::{self, Effect, Env, Event, State};
use serde_json::{Value, json};

/// The vector clock: `context.now` while seeding, 1 s later at every step.
const NOW: f64 = 1_700_000_000_000.0;

fn acp_dir() -> std::path::PathBuf {
    out_dir().join("acp")
}

fn event(recorded: &Value) -> Result<Event, String> {
    let action = recorded["action"].as_str().ok_or("event without action")?;
    let args = recorded["args"].as_array().cloned().unwrap_or_default();
    let shape = match args.as_slice() {
        [only] => json!({ action: only }),
        _ => json!({ action: args }),
    };
    serde_json::from_value(shape.clone())
        .or_else(|_| serde_json::from_value(json!({ action: args })))
        .map_err(|e| format!("{action}: {e}"))
}

/// How the recorder's mocked `sendPromptBlocks` answered: `{ busy }` rejects
/// as a busy session, `{ error }` and `{ agentDead }` reject, anything else
/// resolves. With no outcome listed it resolves `{ receipt: "accepted" }`.
fn dispatch_outcome(outcome: Option<Value>) -> DispatchOutcome {
    let outcome = outcome.unwrap_or_else(|| json!({ "receipt": "accepted" }));
    if outcome.get("busy").is_some() {
        DispatchOutcome::Busy
    } else if let Some(error) = outcome.get("error").and_then(Value::as_str) {
        DispatchOutcome::Failed {
            message: format!("Error: {error}"),
            agent_dead: false,
        }
    } else if outcome.get("agentDead").is_some() {
        DispatchOutcome::Failed {
            message: "Error: agent thread is no longer running".into(),
            agent_dead: true,
        }
    } else {
        DispatchOutcome::Answered(outcome)
    }
}

fn replay(vector: &Value) -> Result<usize, String> {
    let context = vector.get("context").cloned().unwrap_or(json!({}));
    let start = context.get("now").and_then(Value::as_f64).unwrap_or(NOW);
    let server_history = context.get("serverHistory") == Some(&Value::Bool(true));
    let mut config_replies: VecDeque<Value> = context
        .get("configReplies")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into();
    let mut outcomes: VecDeque<Value> = context
        .get("dispatch")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into();
    let mut state: State = parse(&vector["initial"])?;
    if let Some(problem) = diff("initial", &vector["initial"], &to_value(&state)) {
        return Err(format!("initial state does not round-trip: {problem}"));
    }
    state.book.open_chat_tabs = context
        .get("openChatTabs")
        .map(parse)
        .transpose()?
        .unwrap_or_default();
    let steps = vector["steps"].as_array().ok_or("no steps")?;
    for (index, recorded) in steps.iter().enumerate() {
        let at = |what: &str| format!("step {index} ({}) {what}", recorded["event"]["action"]);
        let mut ids = Replay {
            ids: parse(&recorded["ids"]).map_err(|e| at(&e))?,
            overdrawn: 0,
        };
        let mut env = Env {
            now: start + 1000.0 * (index as f64 + 1.0),
            ids: &mut ids,
            server_history,
        };
        let mut effects = acp::step(
            &mut state,
            event(&recorded["event"]).map_err(|e| at(&e))?,
            &mut env,
        );
        // The recorder awaits each dispatched command (a microtask) and then
        // the deferred turn end (a macrotask) after every step; the replay
        // feeds both back in that order, the way the app does.
        let mut settled = std::collections::BTreeSet::new();
        let mut acknowledged = 0;
        loop {
            // A set command is acknowledged at once; the recorder's mocks
            // resolve it before the action goes on.
            if let Some(offset) = effects[acknowledged..].iter().position(|effect| {
                matches!(
                    effect,
                    Effect::SetMode { .. }
                        | Effect::SetModel { .. }
                        | Effect::SetConfigOption { .. }
                )
            }) {
                let command = effects[acknowledged + offset].clone();
                acknowledged += offset + 1;
                let reply = match command {
                    Effect::SetConfigOption { .. } => config_replies
                        .pop_front()
                        .filter(|reply| !reply.is_null())
                        .map(|reply| parse(&reply))
                        .transpose()
                        .map_err(|e| at(&e))?,
                    _ => None,
                };
                let more = acp::step(&mut state, Event::ControlSet(command, reply), &mut env);
                effects.extend(more);
                continue;
            }
            let dispatched = effects.iter().find_map(|effect| match effect {
                Effect::DispatchPrompt {
                    session_id,
                    turn_id,
                    ..
                } if !settled.contains(turn_id) => Some((session_id.clone(), turn_id.clone())),
                _ => None,
            });
            if let Some((session_id, turn_id)) = dispatched {
                settled.insert(turn_id.clone());
                let outcome = dispatch_outcome(outcomes.pop_front());
                let event = Event::PromptSettled(PromptSettled {
                    session_id,
                    turn_id,
                    outcome,
                });
                effects.extend(acp::step(&mut state, event, &mut env));
                continue;
            }
            // The app frees a transcript right after the persist that read it.
            if let Some(index) = effects
                .iter()
                .position(|e| matches!(e, Effect::DropSessionTranscript { .. }))
            {
                let Effect::DropSessionTranscript { session_id } = effects.remove(index) else {
                    unreachable!()
                };
                let more = acp::step(
                    &mut state,
                    Event::DropSessionTranscript(session_id),
                    &mut env,
                );
                effects.extend(more);
                continue;
            }
            if let Some(index) = effects.iter().position(|e| matches!(e, Effect::EndTurn(_))) {
                let Effect::EndTurn(end) = effects.remove(index) else {
                    unreachable!()
                };
                effects.extend(acp::step(&mut state, Event::TurnEnd(end), &mut env));
                continue;
            }
            break;
        }
        // The recorder cannot see the busy-retry timer (`setTimeout`).
        effects.retain(|effect| !matches!(effect, Effect::RetryQueueAfter { .. }));
        if !ids.ids.is_empty() || ids.overdrawn > 0 {
            return Err(at(&format!(
                "drew {} ids too few or {} too many",
                ids.ids.len(),
                ids.overdrawn
            )));
        }
        for (what, want, got) in [
            ("effects", &recorded["effects"], &to_value(&effects)),
            ("state", &recorded["state"], &to_value(&state)),
        ] {
            if let Some(problem) = diff(what, want, got) {
                return Err(at(&problem));
            }
        }
    }
    Ok(steps.len())
}

#[test]
fn replays_every_acp_vector() {
    let manifest = read_json(&acp_dir().join("manifest.json"));
    let vectors = read_json(&acp_dir().join("acp.json"));
    let vectors = vectors
        .as_array()
        .expect("acp.json holds an array of vectors");
    let names: Vec<&Value> = vectors.iter().map(|vector| &vector["name"]).collect();
    let listed: Vec<&Value> = manifest["stores"]["acp"]["vectors"]
        .as_array()
        .expect("manifest vector list")
        .iter()
        .collect();
    assert_eq!(
        names, listed,
        "acp.json vector names differ from the manifest"
    );
    let mut failures = Vec::new();
    let mut total = 0;
    for vector in vectors {
        match replay(vector) {
            Ok(steps) => total += steps,
            Err(problem) => failures.push(format!("{}: {problem}", vector["name"])),
        }
    }
    println!("acp: {} vectors, {total} steps replayed", vectors.len());
    assert!(
        failures.is_empty(),
        "{} acp vectors failed:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn acp_vectors_are_not_older_than_their_sources() {
    let manifest = read_json(&acp_dir().join("manifest.json"));
    let sources = manifest["stores"]["acp"]["sources"]
        .as_object()
        .expect("manifest sources");
    let stale: Vec<&String> = sources
        .iter()
        .filter(|(path, recorded)| source_hash(path) != recorded.as_str().unwrap_or_default())
        .map(|(path, _)| path)
        .collect();
    assert!(
        stale.is_empty(),
        "TypeScript changed after the acp vectors were recorded; rerun \
         `RECORD_VECTORS=1 bun run test tools/parity/vectors` and port the change: {stale:?}"
    );
}
