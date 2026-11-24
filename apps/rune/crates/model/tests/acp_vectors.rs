//! Replays the ACP chat store's vectors (`tools/parity/vectors/out/acp/`,
//! recorded by `record-acp.test.ts`) through `model::acp` and compares the
//! state and the effects after every step. A second test fails when a
//! TypeScript source changed after the vectors were recorded.

mod common;

use common::{Replay, diff, out_dir, parse, read_json, source_hash, to_value};
use std::collections::VecDeque;

use model::acp::mcp::McpError;
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
        [] => json!(action),
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
            message: error.to_owned(),
            agent_dead: false,
        }
    } else if let Some(code) = outcome.get("uncertain").and_then(Value::as_str) {
        DispatchOutcome::Uncertain {
            message: "connection lost".into(),
            outcome_unknown: code == "outcome_unknown",
        }
    } else if outcome.get("agentDead").is_some() {
        DispatchOutcome::Failed {
            message: "agent thread is no longer running".into(),
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
    let desktop = context.get("desktop") == Some(&Value::Bool(true));
    let mut config_replies: VecDeque<Value> = context
        .get("configReplies")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into();
    let saved = context
        .get("composerOptions")
        .cloned()
        .unwrap_or(Value::Null);
    let mut agent_replies: VecDeque<Value> = context
        .get("agentReplies")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into();
    let mut mcp_replies: VecDeque<Value> = context
        .get("mcpReplies")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into();
    let mut set_replies: VecDeque<Value> = context
        .get("setReplies")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into();
    let factory_key = context
        .get("factoryKey")
        .cloned()
        .unwrap_or(Value::Bool(false));
    let mut outcomes: VecDeque<Value> = context
        .get("dispatch")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into();
    let payloads = context.get("payloads").cloned().unwrap_or(json!({}));
    // Commands a `{ hold, release }` reply keeps in flight, oldest first,
    // with the reply a `releaseHeld` step answers them with.
    let mut held: VecDeque<(Effect, Value)> = VecDeque::new();
    // A replay window's end runs on a timer the recorder's step does not
    // wait for: it lands in the next step, after that step's own commands.
    let mut replay_ends: Vec<Effect> = Vec::new();
    let mut state: State = parse(&vector["initial"])?;
    if let Some(problem) = diff("initial", &vector["initial"], &to_value(&state)) {
        return Err(format!("initial state does not round-trip: {problem}"));
    }
    state.book.desktop = desktop;
    state.book.projects = context
        .get("projects")
        .map(parse)
        .transpose()?
        .unwrap_or_default();
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
        let event = if recorded["event"]["action"] == "releaseHeld" {
            let (command, reply) = held.pop_front().ok_or_else(|| at("nothing held"))?;
            answer(command, reply)
        } else {
            event(&recorded["event"]).map_err(|e| at(&e))?
        };
        let mut effects = acp::step(&mut state, event, &mut env);
        // The recorder awaits each dispatched command (a microtask) and then
        // the deferred turn end (a macrotask) after every step; the replay
        // feeds both back in that order, the way the app does.
        let mut settled = std::collections::BTreeSet::new();
        let mut acknowledged = 0;
        loop {
            // A set command is acknowledged and a saved-picks read answered
            // at once, in the order they were sent; the recorder's mocks
            // resolve each before the action goes on.
            if let Some(offset) = effects[acknowledged..].iter().position(|effect| {
                matches!(
                    effect,
                    Effect::SetMode { .. }
                        | Effect::SetModel { .. }
                        | Effect::SetConfigOption { .. }
                        | Effect::ReadComposerOptions { .. }
                        | Effect::ReadFactoryKeyStatus { .. }
                        | Effect::SpawnAgent { .. }
                        | Effect::KillAgent { .. }
                        | Effect::ListSessions { .. }
                        | Effect::ImportAgentSession { .. }
                        | Effect::NewSession { .. }
                        | Effect::LoadSession { .. }
                        | Effect::DisposeEphemeralSession { .. }
                        | Effect::PromoteSession { .. }
                        | Effect::ResumeSession { .. }
                        | Effect::Authenticate { .. }
                        | Effect::Logout { .. }
                        | Effect::RespondElicitation { .. }
                        | Effect::ReadSessionPayload { .. }
                        | Effect::CloseAgentSession { .. }
                        | Effect::DeleteAgentSession { .. }
                        | Effect::DeleteSessionPayload { .. }
                        | Effect::LoadMcpServers { .. }
                        | Effect::SaveMcpServers { .. }
                        | Effect::ProbeMcpServer { .. }
                        | Effect::StartMcpOAuth { .. }
                        | Effect::CompleteMcpOAuth { .. }
                        | Effect::HasMcpOAuthToken { .. }
                        | Effect::DisconnectMcpOAuth { .. }
                        | Effect::AnswerMcpInstall { .. }
                )
            }) {
                let command = effects[acknowledged + offset].clone();
                acknowledged += offset + 1;
                let event = match command {
                    Effect::ReadComposerOptions { .. } => {
                        Event::ComposerOptionsRead(command, saved.clone())
                    }
                    Effect::SetConfigOption { .. } => {
                        let reply = config_replies.pop_front().filter(|reply| !reply.is_null());
                        match reply.as_ref().and_then(|reply| reply.get("reject")) {
                            Some(reason) => Event::ControlFailed(
                                command,
                                reason.as_str().unwrap_or_default().to_owned(),
                                None,
                            ),
                            None => Event::ControlSet(
                                command,
                                reply
                                    .map(|reply| parse(&reply))
                                    .transpose()
                                    .map_err(|e| at(&e))?,
                            ),
                        }
                    }
                    // `setReplies`: `{ reject }` fails the set, else it is acknowledged.
                    Effect::SetMode { .. } | Effect::SetModel { .. } => {
                        let reply = set_replies.pop_front().unwrap_or(Value::Null);
                        match reply.get("reject") {
                            Some(reason) => Event::ControlFailed(
                                command,
                                js_string(Some(reason)),
                                reply.get("code").map(|code| js_string(Some(code))),
                            ),
                            None => Event::ControlSet(command, None),
                        }
                    }
                    // `factoryKey`: the stored-key answer, `{ reject }` when unread.
                    Effect::ReadFactoryKeyStatus { .. } => {
                        let stored = match factory_key.get("reject") {
                            Some(_) => None,
                            None => Some(model::acp::js::truthy(&factory_key)),
                        };
                        Event::FactoryKeyStatus(command, stored)
                    }
                    // A payload read answers `context.payloads` the way an
                    // agent command answers its reply.
                    Effect::ReadSessionPayload { ref session_id, .. } => {
                        let reply = payloads.get(session_id).cloned().unwrap_or(Value::Null);
                        match hold(command, reply, &mut held) {
                            Some((command, reply)) => answer(command, reply),
                            None => continue,
                        }
                    }
                    // An MCP command answers the next `mcpReplies` entry:
                    // `{ reject, code }` fails it, `{ hold }` leaves it in
                    // flight, and a start's `authUrl` reaches the model first.
                    Effect::LoadMcpServers { .. }
                    | Effect::SaveMcpServers { .. }
                    | Effect::ProbeMcpServer { .. }
                    | Effect::StartMcpOAuth { .. }
                    | Effect::CompleteMcpOAuth { .. }
                    | Effect::HasMcpOAuthToken { .. }
                    | Effect::DisconnectMcpOAuth { .. }
                    | Effect::AnswerMcpInstall { .. } => {
                        let reply = mcp_replies.pop_front().unwrap_or(Value::Null);
                        if let (Effect::StartMcpOAuth { .. }, Some(url)) =
                            (&command, reply.get("authUrl").and_then(Value::as_str))
                        {
                            let redirect = js_string(reply.get("redirectUri"));
                            let event =
                                Event::McpOAuthAuthUrl(command.clone(), url.into(), redirect);
                            effects.extend(acp::step(&mut state, event, &mut env));
                        }
                        if let Some(reason) = reply.get("reject") {
                            let error = McpError {
                                message: js_string(Some(reason)),
                                code: reply.get("code").and_then(Value::as_str).map(str::to_owned),
                            };
                            Event::McpCommandFailed(command, error)
                        } else if reply.get("hold").is_some() {
                            continue;
                        } else {
                            Event::McpCommandDone(command, reply)
                        }
                    }
                    // An agent, close or delete command answers the next
                    // `agentReplies` entry.
                    _ => {
                        let reply = agent_replies.pop_front().unwrap_or(Value::Null);
                        match hold(command, reply, &mut held) {
                            Some((command, reply)) => answer(command, reply),
                            None => continue,
                        }
                    }
                };
                let more = acp::step(&mut state, event, &mut env);
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
            if !replay_ends.is_empty() {
                for end in std::mem::take(&mut replay_ends) {
                    effects.extend(acp::step(&mut state, Event::ReplayEnd(end), &mut env));
                }
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
        // The recorder cannot see the busy-retry timer (`setTimeout`), the
        // one-shots' timeouts or the prompt resend ladder.
        effects.retain(|effect| {
            !matches!(
                effect,
                Effect::RetryQueueAfter { .. }
                    | Effect::StartOneShotTimer { .. }
                    | Effect::PromptResendAfter { .. }
            )
        });
        replay_ends.extend(
            effects
                .iter()
                .filter(|effect| matches!(effect, Effect::EndReplay { .. }))
                .cloned(),
        );
        effects.retain(|effect| !matches!(effect, Effect::EndReplay { .. }));
        if !ids.ids.is_empty() || ids.overdrawn > 0 {
            return Err(at(&format!(
                "drew {} ids too few or {} too many",
                ids.ids.len(),
                ids.overdrawn
            )));
        }
        for (what, want, got) in [
            (
                "effects",
                &writes_last(&recorded["effects"]),
                &writes_last(&to_value(&effects)),
            ),
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

/// The effects with the remembered-auth writes moved to the end, in their
/// own order. The TypeScript saves them through a promise chain, so where a
/// save lands among the other calls depends on how many microtasks the chain
/// still has queued; the model does not count them.
fn writes_last(effects: &Value) -> Value {
    let (writes, rest): (Vec<&Value>, Vec<&Value>) = effects
        .as_array()
        .into_iter()
        .flatten()
        .partition(|effect| effect["kind"] == "persistAuthMethods");
    Value::Array(rest.into_iter().chain(writes).cloned().collect())
}

/// A `{ hold }` reply keeps the command in flight: for the rest of the
/// vector, or with `release` until a `releaseHeld` step. `None` when held.
fn hold(
    command: Effect,
    reply: Value,
    held: &mut VecDeque<(Effect, Value)>,
) -> Option<(Effect, Value)> {
    if reply.get("hold").is_none() {
        return Some((command, reply));
    }
    if let Some(release) = reply.get("release") {
        held.push_back((command, release.clone()));
    }
    None
}

/// The event that answers an agent, session or payload-read command with
/// `reply`: `{ reject }` fails it, anything else is the reply.
fn answer(command: Effect, reply: Value) -> Event {
    let rejected = reply.get("reject").map(|reason| js_string(Some(reason)));
    match command {
        Effect::ReadSessionPayload { .. } => {
            let outcome = match rejected {
                Some(reason) => Err(reason),
                None => Ok(Some(reply).filter(|reply| !reply.is_null())),
            };
            Event::SessionPayloadRead(command, outcome)
        }
        Effect::CloseAgentSession { .. }
        | Effect::DeleteAgentSession { .. }
        | Effect::DeleteSessionPayload { .. } => match rejected {
            Some(reason) => Event::SessionCommandFailed(command, reason),
            None => Event::SessionCommandDone(command, reply),
        },
        _ => match rejected {
            Some(reason) => {
                let code = reply.get("code").map(|code| js_string(Some(code)));
                Event::AgentCommandFailed(command, reason, code)
            }
            None => Event::AgentCommandDone(command, reply),
        },
    }
}

/// `String(value)` of a recorded reply field.
fn js_string(value: Option<&Value>) -> String {
    model::acp::js::string_of(value)
}
