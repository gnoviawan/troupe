//! Replays the git store's vectors. The store's actions wait on `gitApi`
//! calls: each call answers the next `context.git` reply in the order the
//! calls went out, as the recorder's mock does. `{ hold }` keeps a call in
//! flight until a `releaseHeld` step answers it with the step's reply.

use std::collections::VecDeque;

use model::git::{self, Effect, Event, Failure, LOG_SOURCE, State, TaskId};
use serde_json::{Value, json};

use crate::common::{call, diff, parse, to_value};

/// The model's effects as the recorder's mocks emit them; timers have no mock.
fn recorded(effect: &Effect) -> Option<Value> {
    Some(match effect {
        Effect::Call { call, .. } => {
            let (op, args) = call.api();
            json!({ "kind": "git", "op": op, "args": args })
        }
        Effect::Log { level, message } => {
            json!({ "kind": "log", "level": level, "source": LOG_SOURCE, "message": message })
        }
        Effect::Toast { message } => json!({ "kind": "toast", "level": "error", "message": message }),
        Effect::ReconcileWorktree { cwd } => json!({ "kind": "reconcileWorktree", "cwd": cwd }),
        Effect::ScheduleDiffStat { .. } | Effect::Settled { .. } => return None,
    })
}

fn answer(reply: &Value) -> Result<Value, Failure> {
    match reply.get("reject") {
        Some(message) => Err(Failure {
            message: message.as_str().unwrap_or_default().to_owned(),
            code: reply.get("code").and_then(Value::as_str).map(str::to_owned),
        }),
        None => Ok(reply.get("ok").cloned().unwrap_or(Value::Null)),
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
    fn take(&mut self, step: git::Step) {
        for effect in &step.effects {
            if let Effect::Call { task, .. } = effect {
                self.pending.push_back(*task);
            }
            if let Effect::Settled { task, outcome } = effect
                && Some(*task) == self.root
            {
                self.result = Some(match outcome {
                    Ok(value) => value.clone().unwrap_or(Value::Null),
                    Err(failure) => json!({ "reject": failure.message }),
                });
            }
            self.effects.extend(recorded(effect));
        }
    }

    /// Answers every call in flight, oldest first, until all are answered or held.
    fn drain(&mut self, now: f64) -> Result<(), String> {
        while let Some(task) = self.pending.pop_front() {
            let reply = self.replies.pop_front().ok_or("ran out of git replies")?;
            if reply.get("hold").is_some() {
                self.held.push_back(task);
                continue;
            }
            let step = git::step(&mut self.state, Event::Reply(task, answer(&reply)), now);
            self.take(step);
        }
        Ok(())
    }
}

pub fn replay(vector: &Value) -> Result<usize, String> {
    let context = vector.get("context").cloned().unwrap_or(json!({}));
    let state: State = parse(&vector["initial"])?;
    if let Some(problem) = diff("initial", &vector["initial"], &to_value(&state)) {
        return Err(format!("initial state does not round-trip: {problem}"));
    }
    let replies = context.get("git").and_then(Value::as_array).cloned().unwrap_or_default();
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
        let now = 1000.0 * (index as f64 + 1.0);
        run.effects.clear();
        run.root = None;
        run.result = None;
        match event["action"].as_str().unwrap_or_default() {
            "releaseHeld" => {
                let task = run.held.pop_front().ok_or_else(|| at("nothing held"))?;
                let step = git::step(&mut run.state, Event::Reply(task, answer(&event["args"][0])), now);
                run.take(step);
            }
            "unwatchDiffStat" => {
                let cwd = event["args"][0].as_str().unwrap_or_default().to_owned();
                let step = git::step(&mut run.state, Event::UnwatchDiffStat(cwd), now);
                run.take(step);
            }
            _ => {
                let parsed: Event = call(event).map_err(|e| at(&e))?;
                let step = git::step(&mut run.state, parsed, now);
                run.root = step.task;
                run.take(step);
            }
        }
        run.drain(now).map_err(|e| at(&e))?;
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

/// Replays `git-routes`: each step's `gitApi` call must map to the request the web client
/// sent (method, path, JSON body) and carry `X-Command-Id` exactly when the host ledgers it.
pub fn replay_routes(vector: &Value) -> Result<usize, String> {
    let steps = vector["steps"].as_array().ok_or("no steps")?;
    for (index, recorded) in steps.iter().enumerate() {
        let event = &recorded["event"];
        let op = event["action"].as_str().unwrap_or_default();
        let args = event["args"].as_array().cloned().unwrap_or_default();
        let call = model::git::Call::from_api(op, &args).ok_or_else(|| format!("step {index}: no call for {op}"))?;
        let route = call.route();
        let got = json!({
            "kind": "http",
            "method": if route.post { "POST" } else { "GET" },
            "path": route.path,
            "body": route.body,
            "commandId": call.ledger().is_some(),
        });
        if let Some(problem) = diff(&format!("step {index} ({op})"), &recorded["result"], &got) {
            return Err(problem);
        }
    }
    Ok(steps.len())
}
