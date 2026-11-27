//! Replays the GitHub store's vectors. Each `githubApi` call answers the next
//! `context.github` reply in the order the calls went out, as the recorder's
//! mock does; `{ hold }` keeps a call in flight until a `releaseHeld` step.

use std::collections::VecDeque;

use model::github::{self, Effect, Event, Failure, Reply, State, TaskId, ipc_json};
use serde_json::{Value, json};

use crate::common::{call, diff, parse, to_value};

fn reply(ipc: &Value) -> Reply {
    if ipc["success"] == json!(true) {
        Ok(ipc["data"].clone())
    } else {
        Err(Failure {
            code: ipc["code"].as_str().unwrap_or_default().to_owned(),
            error: ipc["error"].as_str().unwrap_or_default().to_owned(),
        })
    }
}

struct Run {
    state: State,
    replies: VecDeque<Value>,
    held: VecDeque<TaskId>,
    pending: VecDeque<TaskId>,
    effects: Vec<Value>,
    root: Option<TaskId>,
    result: Option<Value>,
}

impl Run {
    fn take(&mut self, step: github::Step) -> Result<(), String> {
        for effect in step.effects {
            match effect {
                Effect::Call { task, call } => {
                    self.pending.push_back(task);
                    self.effects.push(json!({ "kind": "github", "op": call.op, "args": [call.arg()] }));
                }
                Effect::Settled { task, outcome } if Some(task) == self.root => {
                    self.result = Some(outcome.as_ref().map_or(Value::Null, ipc_json));
                }
                Effect::Settled { .. } => {}
                Effect::Log { message } => return Err(format!("unexpected log: {message}")),
            }
        }
        Ok(())
    }

    fn drain(&mut self) -> Result<(), String> {
        while let Some(task) = self.pending.pop_front() {
            let answer = self.replies.pop_front().ok_or("ran out of github replies")?;
            if answer.get("hold").is_some() {
                self.held.push_back(task);
                continue;
            }
            let step = github::step(&mut self.state, Event::Reply(task, reply(&answer)));
            self.take(step)?;
        }
        Ok(())
    }
}

pub fn replay(vector: &Value) -> Result<usize, String> {
    let state: State = parse(&vector["initial"])?;
    let replies = vector["context"]["github"].as_array().cloned().unwrap_or_default();
    let mut run = Run {
        state,
        replies: replies.into(),
        held: VecDeque::new(),
        pending: VecDeque::new(),
        effects: Vec::new(),
        root: None,
        result: None,
    };
    let steps = vector["steps"].as_array().ok_or("no steps")?;
    for (index, recorded) in steps.iter().enumerate() {
        let at = |what: &str| format!("step {index} ({}) {what}", recorded["event"]["action"]);
        let event = &recorded["event"];
        run.effects.clear();
        run.root = None;
        run.result = None;
        if event["action"] == "releaseHeld" {
            let task = run.held.pop_front().ok_or_else(|| at("nothing held"))?;
            let step = github::step(&mut run.state, Event::Reply(task, reply(&event["args"][0])));
            run.take(step).map_err(|e| at(&e))?;
        } else {
            let parsed: Event = call(event).map_err(|e| at(&e))?;
            let step = github::step(&mut run.state, parsed);
            run.root = step.task;
            run.take(step).map_err(|e| at(&e))?;
        }
        run.drain().map_err(|e| at(&e))?;
        let result = match (run.root, &run.result) {
            (_, Some(value)) => value.clone(),
            (Some(_), None) => json!({ "pending": true }),
            (None, None) => Value::Null,
        };
        let checks = [
            ("effects", &recorded["effects"], &Value::Array(run.effects.clone())),
            ("result", recorded.get("result").unwrap_or(&Value::Null), &result),
            ("state", &recorded["state"], &to_value(&run.state)),
        ];
        for (what, want, got) in checks {
            if let Some(problem) = diff(what, want, got) {
                return Err(at(&problem));
            }
        }
    }
    Ok(steps.len())
}

/// Replays `github-routes`: each call's route is the one the web client posts to, with the
/// request as its JSON body and no `X-Command-Id` (the host does not ledger GitHub writes).
pub fn replay_routes(vector: &Value) -> Result<usize, String> {
    let steps = vector["steps"].as_array().ok_or("no steps")?;
    for (index, recorded) in steps.iter().enumerate() {
        let op = recorded["event"]["action"].as_str().unwrap_or_default();
        let request = github::Call::body_of(op, recorded["event"]["args"][0].clone());
        let path = github::Call::path_of(op).ok_or_else(|| format!("step {index}: no route for {op}"))?;
        let got = json!({ "kind": "http", "method": "POST", "path": path, "body": request, "commandId": false });
        if let Some(problem) = diff(&format!("step {index} ({op})"), &recorded["result"], &got) {
            return Err(problem);
        }
    }
    Ok(steps.len())
}
