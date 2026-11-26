//! Replays `out/terminal/terminal.json` (`tools/parity/vectors/terminal.ts`) against
//! `model::terminal`: the session reducer, the session store with its drafts, the completion
//! scheduler, the shell words, the PTY guard, the command editor's session calls and the
//! command history.

use model::Level;
use model::terminal::completion::{Context, Effect, Reply, Scheduler};
use model::terminal::history::{self, History};
use model::terminal::routes::{self, Call};
use model::terminal::session::{self, Event, Mode, Outcome, Phase, Session};
use model::terminal::store::{self, Lifecycle, RpcResult, Store};
use model::terminal::words;
use serde_json::{Map, Value, json};

use crate::common::{diff, out_dir, parse, read_json, source_hash, to_value};

fn vectors() -> Value {
    read_json(&out_dir().join("terminal/terminal.json"))
}

/// `prev` with the recorded changes laid over it; a null field is one the state lacks.
fn apply_changes(prev: &Value, changed: &Value) -> Value {
    let mut next = prev.as_object().cloned().unwrap_or_default();
    for (key, value) in changed.as_object().into_iter().flatten() {
        if value.is_null() && key == "cwd" {
            next.remove(key);
        } else {
            next.insert(key.clone(), value.clone());
        }
    }
    Value::Object(next)
}

fn check(failures: &mut Vec<String>, label: String, want: &Value, got: &Value) {
    if let Some(problem) = diff("", want, got) {
        failures.push(format!("{label}: {problem}"));
    }
}

fn report(name: &str, failures: &[String]) {
    assert!(failures.is_empty(), "{name}: {} failed\n{}", failures.len(), failures.iter().take(20).cloned().collect::<Vec<_>>().join("\n"));
}

#[test]
fn terminal_vectors_are_not_older_than_their_sources() {
    let vectors = vectors();
    let stale: Vec<String> = vectors["sources"]
        .as_object()
        .expect("sources")
        .iter()
        .filter(|(path, hash)| source_hash(path) != hash.as_str().unwrap_or_default())
        .map(|(path, _)| path.clone())
        .collect();
    assert!(stale.is_empty(), "TypeScript changed after its vectors were recorded; rerun `RECORD_VECTORS=1 bun run test tools/parity/vectors` and port the change:\n{}", stale.join("\n"));
}

#[test]
fn the_reducer_follows_every_recorded_walk() {
    let mut failures = Vec::new();
    let mut steps = 0;
    for (w, walk) in vectors()["reducer"].as_array().expect("reducer").iter().enumerate() {
        let mut state: Session = parse(&walk["start"]).expect("start state");
        let mut want = walk["start"].clone();
        for (i, step) in walk["steps"].as_array().expect("steps").iter().enumerate() {
            steps += 1;
            let outcome = if let Some(screen) = step["screen"].as_bool() {
                session::apply_screen(&mut state, screen)
            } else {
                let event: Event = parse(&step["event"]).unwrap_or_else(|e| panic!("walk {w} step {i}: {e}"));
                session::reduce(&mut state, &event)
            };
            want = apply_changes(&want, &step["changed"]);
            let label = format!("walk {w} step {i} {}", step["event"]["type"].as_str().unwrap_or("screen"));
            check(&mut failures, label.clone(), &want, &to_value(&state));
            let log = match outcome {
                Outcome::Raw(detail) => json!(detail),
                _ => Value::Null,
            };
            check(&mut failures, format!("{label} log"), &step["log"], &log);
            state = parse(&want).expect("recorded state");
        }
    }
    println!("reducer: {steps} steps");
    report("reducer", &failures);
}

fn level(text: &str) -> Level {
    match text {
        "info" => Level::Info,
        "warn" => Level::Warn,
        _ => Level::Error,
    }
}

fn result(text: &str) -> RpcResult {
    parse(&json!(text)).expect("rpc result")
}

#[test]
fn the_session_store_follows_every_recorded_walk() {
    let mut failures = Vec::new();
    let mut steps = 0;
    for walk in vectors()["store"].as_array().expect("store") {
        let id = walk["terminalId"].as_str().expect("terminal id");
        let mut store = Store::default();
        let mut epochs: Vec<u64> = Vec::new();
        let mut want = to_value(&store.read(id));
        for (i, step) in walk["steps"].as_array().expect("steps").iter().enumerate() {
            steps += 1;
            let op = step["op"].as_str().unwrap_or_default();
            let label = format!("{id} step {i} {op}");
            let ret = match op {
                "apply" => {
                    let event: Lifecycle = parse(&step["event"]).unwrap_or_else(|e| panic!("{label}: {e}"));
                    store.apply(id, &event);
                    Value::Null
                }
                "screen" => {
                    store.apply_screen(id, step["alternateScreen"].as_bool().unwrap_or_default());
                    Value::Null
                }
                "beginSubmit" => match store.begin_submit(id) {
                    Some(token) => {
                        epochs.push(token.epoch);
                        json!({ "epochIndex": epochs.len() - 1 })
                    }
                    None => Value::Null,
                },
                "recordSubmit" => {
                    let epoch = epochs[step["token"].as_u64().expect("token") as usize];
                    json!(store.record_submit(id, result(step["result"].as_str().unwrap_or_default()), epoch))
                }
                "leaveEditor" => {
                    store.leave_editor(id);
                    Value::Null
                }
                "invalidate" => {
                    store.invalidate(id, level(step["level"].as_str().unwrap_or_default()));
                    Value::Null
                }
                "processExited" => {
                    store.process_exited(id);
                    Value::Null
                }
                "forget" => {
                    store.forget(id);
                    Value::Null
                }
                "offer" => {
                    let mode: Mode = parse(&step["mode"]).expect("mode");
                    store.offer(id, mode, step["text"].as_str().unwrap_or_default());
                    Value::Null
                }
                "bumpRevision" => json!(store.bump_revision(id)),
                other => panic!("{label}: unknown op {other}"),
            };
            want = apply_changes(&want, &step["changed"]);
            check(&mut failures, format!("{label} session"), &want, &to_value(&store.read(id)));
            check(&mut failures, format!("{label} return"), &step["ret"], &ret);
            check(&mut failures, format!("{label} draft"), &step["draft"], &json!(store.draft(id)));
            check(&mut failures, format!("{label} draft version"), &step["draftVersion"], &json!(store.draft_version(id)));
            check(&mut failures, format!("{label} revision"), &step["revision"], &json!(store.revision(id)));
            let current = epochs.last().map_or(Value::Null, |&epoch| json!(store.is_submit_current(id, epoch)));
            check(&mut failures, format!("{label} current"), &step["current"], &current);
            let logs: Vec<Value> = store.logs.drain(..).map(|l| json!({ "level": l.level, "message": l.message })).collect();
            check(&mut failures, format!("{label} logs"), &step["logs"], &json!(logs));
        }
    }
    println!("store: {steps} steps");
    report("store", &failures);
}

fn effect(effect: &Effect) -> Value {
    match effect {
        Effect::Request { terminal_id, request } => json!({ "kind": "request", "terminalId": terminal_id, "request": request }),
        Effect::Cancel { terminal_id, request_id } => json!({ "kind": "cancel", "terminalId": terminal_id, "requestId": request_id }),
        Effect::Show(completion) => json!({ "kind": "show", "completion": completion }),
        Effect::Log { name, detail } => json!({ "kind": "log", "name": name, "detail": detail }),
    }
}

#[test]
fn the_completion_scheduler_follows_every_recorded_walk() {
    let mut failures = Vec::new();
    let mut steps = 0;
    for (w, walk) in vectors()["scheduler"].as_array().expect("scheduler").iter().enumerate() {
        let mut scheduler = Scheduler::default();
        scheduler.auto_refresh = walk["autoRefresh"].as_bool().unwrap_or(true);
        let mut now = 0.0;
        for (i, step) in walk["steps"].as_array().expect("steps").iter().enumerate() {
            steps += 1;
            let op = step["op"].as_str().unwrap_or_default();
            let label = format!("walk {w} step {i} {op}");
            let mut out = Vec::new();
            match op {
                "request" => {
                    let context: Context = parse(&step["context"]).unwrap_or_else(|e| panic!("{label}: {e}"));
                    scheduler.request(context, step["showing"].as_bool().unwrap_or_default(), now, &mut out);
                }
                "update" => {
                    let context: Context = parse(&step["context"]).unwrap_or_else(|e| panic!("{label}: {e}"));
                    scheduler.update(context, now, &mut out);
                }
                "prepareSubmit" => {
                    scheduler.prepare_submit(&mut out);
                }
                "invalidate" => scheduler.invalidate(&mut out),
                "advance" => {
                    now += step["ms"].as_f64().expect("ms");
                    scheduler.tick(now, &mut out);
                }
                "reply" => {
                    let reply: Reply = parse(&step["reply"]).unwrap_or_else(|e| panic!("{label}: {e}"));
                    scheduler.reply(step["requestId"].as_str().unwrap_or_default(), &reply, now, &mut out);
                }
                other => panic!("{label}: unknown op {other}"),
            }
            let got: Vec<Value> = out.iter().map(effect).collect();
            check(&mut failures, format!("{label} effects"), &step["effects"], &json!(got));
            check(&mut failures, format!("{label} in flight"), &step["inFlight"], &json!(scheduler.snapshot().in_flight));
        }
    }
    println!("scheduler: {steps} steps");
    report("scheduler", &failures);
}

fn utf16_len(text: &str) -> usize {
    text.encode_utf16().count()
}

#[test]
fn shell_words_controls_and_the_pty_guard_match() {
    let vectors = vectors();
    let words_v = &vectors["words"];
    let output = words_v["output"].as_str().expect("output");
    let mut failures = Vec::new();
    for case in words_v["parse"].as_array().expect("parse") {
        let draft = case["draft"].as_str().expect("draft");
        let cursor = case["cursor"].as_u64().expect("cursor") as usize;
        let rows = words::parse_output(draft, cursor, output);
        check(&mut failures, format!("parse {draft:?} at {cursor}"), &case["rows"], &to_value(&rows));
    }
    for case in words_v["continues"].as_array().expect("continues") {
        let draft = case["draft"].as_str().expect("draft");
        let got: Vec<bool> = (0..=utf16_len(draft)).map(|c| words::word_continues_past_cursor(draft, c)).collect();
        check(&mut failures, format!("continues {draft:?}"), &case["continues"], &json!(got));
    }
    for case in words_v["controls"].as_array().expect("controls") {
        let text = case["text"].as_str().expect("text");
        let got = json!({
            "text": text,
            "pty": words::contains_disallowed_controls(text),
            "editor": words::contains_editor_disallowed_controls(text),
            "paste": words::sanitize_paste(text),
        });
        check(&mut failures, format!("controls {text:?}"), case, &got);
    }
    for case in words_v["guard"].as_array().expect("guard") {
        let mode: Mode = parse(&case["mode"]).expect("mode");
        let phase: Phase = parse(&case["phase"]).expect("phase");
        let data = case["data"].as_str().expect("data");
        let got = store::can_write_to_pty(mode, phase, data);
        check(&mut failures, format!("guard {mode:?} {phase:?} {data:?}"), &case["write"], &json!(got));
    }
    report("words", &failures);
}

fn call(case: &Value) -> Call {
    let terminal_id = case["terminalId"].as_str().expect("terminal id").to_owned();
    match case["call"].as_str().expect("call") {
        "submit" => Call::Submit { terminal_id, draft: case["draft"].as_str().expect("draft").to_owned() },
        "requestCompletion" => {
            let mut request = case["request"].as_object().cloned().expect("request");
            for key in ["bufferRevision", "cursor", "promptGeneration"] {
                // 2^53 is past what `u64` deserialization of the JS number needs; keep it exact.
                if let Some(n) = request.get(key).and_then(Value::as_f64) {
                    request.insert(key.into(), json!(n as u64));
                }
            }
            Call::RequestCompletion { terminal_id, request: parse(&Value::Object(request)).expect("request") }
        }
        "cancel" => Call::CancelCompletion { terminal_id, request_id: case["requestId"].as_str().expect("request id").to_owned() },
        "restore" => Call::RestorePrompt { terminal_id },
        other => panic!("unknown call {other}"),
    }
}

/// The recorded reply as the transport sees it: a reply's data, or the failure code.
fn reply(case: &Value) -> Result<Value, String> {
    match &case["reply"] {
        Value::String(kind) if kind == "throw" => Err("SEND_FAILED".into()),
        Value::String(_) => Err("NETWORK_ERROR".into()),
        reply if reply["success"] == json!(true) => Ok(reply.get("data").cloned().unwrap_or(Value::Null)),
        reply => Err(reply["code"].as_str().unwrap_or_default().to_owned()),
    }
}

#[test]
fn session_calls_send_the_recorded_frames_and_read_the_recorded_replies() {
    let mut failures = Vec::new();
    for case in vectors()["routes"].as_array().expect("routes") {
        let call = call(case);
        let label = format!("{} {}", call.kind(), case["reply"]);
        let frames: Vec<Value> = if call.sendable() { vec![json!({ "id": "req-1", "type": call.kind(), "payload": call.payload() })] } else { vec![] };
        let frames = if case["reply"] == json!("throw") { vec![] } else { frames };
        check(&mut failures, format!("{label} frames"), &case["frames"], &json!(frames));
        let got = match &call {
            Call::RequestCompletion { .. } if !call.sendable() => json!({ "result": "PreWrite" }),
            Call::RequestCompletion { .. } => to_value(&routes::read_completion(&reply(case))),
            _ => to_value(&routes::read_result(&reply(case))),
        };
        check(&mut failures, format!("{label} result"), &case["result"], &got);
    }
    report("routes", &failures);
}

#[test]
fn history_keeps_and_writes_what_the_web_client_does() {
    let vectors = vectors();
    let recorded = &vectors["history"];
    let mut history = History::default();
    let mut failures = Vec::new();
    for (i, step) in recorded["steps"].as_array().expect("steps").iter().enumerate() {
        let project_id = step["projectId"].as_str().expect("project");
        let command = step["command"].as_str().expect("command");
        let newest = &step["write"]["value"][0];
        let id = newest["id"].as_str().unwrap_or_default().to_owned();
        let timestamp = newest["timestamp"].as_i64().unwrap_or_default();
        let write = history.add(command, "zsh", "t1", project_id, timestamp, id);
        let got = write.map_or(Value::Null, |w| json!({ "key": w.key, "value": w.value["data"] }));
        check(&mut failures, format!("add {i} write"), &step["write"], &got);
        let mut recall = Map::new();
        for p in ["p1", "p2"] {
            recall.insert(p.into(), json!(history.recall(p)));
        }
        check(&mut failures, format!("add {i} recall"), &step["recall"], &Value::Object(recall));
    }
    let cap = &recorded["cap"];
    let mut history = History::default();
    let from = cap["from"].as_i64().expect("from");
    let long = "y".repeat(700);
    for i in 0..520 {
        let at = from + i + 1;
        history.add(&format!("{long}{i}"), "zsh", "t1", "p3", at, format!("{at}-i"));
    }
    let written = history.write("p3").value["data"].as_array().cloned().unwrap_or_default();
    let suffix = |v: Option<&Value>| json!(v.and_then(|e| e["command"].as_str()).map(|c| &c[700..]));
    let got = json!({
        "from": from,
        "stored": history.entries.len(),
        "written": written.len(),
        "first": suffix(written.first()),
        "last": suffix(written.last()),
    });
    check(&mut failures, "cap".into(), cap, &got);
    assert_eq!(history::key("p3"), "projects/p3/command-history");
    report("history", &failures);
}
