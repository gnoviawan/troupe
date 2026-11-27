//! Replays the settings store's vectors. The boot read's idle-window read and a failing
//! idle-window push answer the vector's `context.idleWindow`, in order, as the recorder's
//! `acpApi` mock does; a push the context does not fail succeeds.

use std::collections::VecDeque;

use model::settings::routes::Request;
use model::settings::{self, Effect, Event, IdleWindow, Panel, Read, State};
use serde_json::{Map, Value, json};

use crate::common::{diff, parse, to_value};

/// jsdom's `matchMedia` matches no `prefers-color-scheme: dark`, so the recorder's OS is light.
const SYSTEM_DARK: bool = false;

fn recorded(effect: &Effect) -> Value {
    match effect {
        Effect::Write { record, immediate } => {
            json!({ "kind": "writeSettings", "record": record, "immediate": immediate })
        }
        Effect::PushTimeout { timeout, secs } => {
            json!({ "kind": "acp", "op": timeout.op(), "secs": secs })
        }
        Effect::ReadIdleWindow => json!({ "kind": "acp", "op": "getAgentIdleWindow" }),
        Effect::PushIdleWindow { secs, .. } => {
            json!({ "kind": "acp", "op": "setAgentIdleWindow", "secs": secs })
        }
        Effect::ShowPanel { panel, visible } => {
            let action = match panel {
                Panel::Rail => "setRailExpanded",
                Panel::SidePanel => "setSidePanelVisible",
                Panel::SshSection => "setSshSectionVisible",
            };
            json!({ "kind": "shellLayout", "action": action, "value": visible })
        }
        Effect::Log {
            level,
            source,
            message,
        } => {
            json!({ "kind": "log", "level": level, "source": source, "message": message })
        }
    }
}

struct Run {
    state: State,
    replies: VecDeque<Value>,
    effects: Vec<Value>,
}

impl Run {
    /// Applies `event` and answers the host calls its effects make, depth first.
    fn apply(&mut self, event: Event) -> Result<(), String> {
        for effect in settings::step(&mut self.state, event) {
            self.effects.push(recorded(&effect));
            match effect {
                Effect::ReadIdleWindow => {
                    let reply = self
                        .replies
                        .pop_front()
                        .ok_or("ran out of idle-window replies")?;
                    let answer = match reply.get("reject") {
                        Some(message) => Err(message.as_str().unwrap_or_default().to_owned()),
                        None => Ok(parse::<IdleWindow>(&reply)?),
                    };
                    self.apply(Event::IdleWindowRead(answer))?;
                }
                Effect::PushIdleWindow { epoch, .. } => {
                    if !self.state.idle_push_is_current(epoch) {
                        continue;
                    }
                    let failed = self
                        .replies
                        .front()
                        .and_then(|r| r.get("failPush"))
                        .cloned();
                    if let Some(message) = failed {
                        self.replies.pop_front();
                        self.apply(Event::IdleWindowPushFailed(
                            message.as_str().unwrap_or_default().to_owned(),
                        ))?;
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }
}

fn event(recorded: &Value) -> Result<Event, String> {
    let args = recorded["args"].as_array().cloned().unwrap_or_default();
    let arg = |i: usize| args.get(i).cloned().unwrap_or(Value::Null);
    let map = |v: Value| -> Result<Map<String, Value>, String> { parse(&v) };
    Ok(match recorded["action"].as_str().unwrap_or_default() {
        "load" => {
            let read = arg(0);
            let read = match (read.get("data"), read.get("code")) {
                (_, Some(code)) => Read::Failed(code.as_str().unwrap_or_default().to_owned()),
                (Some(data), None) => Read::Found(data.clone()),
                (None, None) => Read::Missing,
            };
            Event::Loaded {
                read,
                system_dark: SYSTEM_DARK,
            }
        }
        "sharedChange" => {
            let change = arg(0);
            let exists = change["exists"] == json!(true);
            Event::SharedChange {
                data: exists.then(|| change["data"].clone()),
                system_dark: SYSTEM_DARK,
            }
        }
        "updateSetting" => Event::UpdateSetting(parse(&arg(0))?, arg(1)),
        "updateSettings" => Event::UpdateSettings(map(arg(0))?),
        "writeNow" => Event::WriteNow(map(arg(0))?),
        "reset" => Event::Reset,
        "setAgentIdleWindow" => Event::SetAgentIdleWindow(arg(0)),
        other => return Err(format!("no settings action {other}")),
    })
}

pub fn replay(vector: &Value) -> Result<usize, String> {
    let replies = vector["context"]["idleWindow"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let mut run = Run {
        state: State::default(),
        replies: replies.into(),
        effects: Vec::new(),
    };
    if let Some(problem) = diff("initial", &vector["initial"], &to_value(&run.state)) {
        return Err(problem);
    }
    let steps = vector["steps"].as_array().ok_or("no steps")?;
    for (index, step) in steps.iter().enumerate() {
        let at = |what: String| format!("step {index} ({}) {what}", step["event"]["action"]);
        run.effects.clear();
        run.apply(event(&step["event"]).map_err(at)?).map_err(at)?;
        let checks = [
            (
                "effects",
                &step["effects"],
                &Value::Array(run.effects.clone()),
            ),
            ("state", &step["state"], &to_value(&run.state)),
        ];
        for (what, want, got) in checks {
            if let Some(problem) = diff(what, want, got) {
                return Err(at(problem));
            }
        }
    }
    Ok(steps.len())
}

/// Each recorded request against [`Request::route`]; no settings route is ledgered.
pub fn replay_routes(vector: &Value) -> Result<usize, String> {
    let steps = vector["steps"].as_array().ok_or("no steps")?;
    for (index, step) in steps.iter().enumerate() {
        let op = step["event"]["action"].as_str().unwrap_or_default();
        let args = step["event"]["args"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let request =
            Request::from_api(op, &args).ok_or_else(|| format!("step {index}: no request {op}"))?;
        let (api, api_args) = request.api();
        if api != op || api_args != args {
            return Err(format!("step {index}: {op} reads back as {api}"));
        }
        let route = request.route();
        let got = json!({
            "kind": "http",
            "method": if route.post { "POST" } else { "GET" },
            "path": route.path,
            "body": route.body,
            "commandId": false,
        });
        if let Some(problem) = diff(&format!("step {index} ({op})"), &step["result"], &got) {
            return Err(problem);
        }
    }
    Ok(steps.len())
}

/// The palette's lists, and cmdk's score as a read.
pub fn replay_palette(vector: &Value) -> Result<usize, String> {
    use model::settings::palette::{ChatCard, Commands, command_score};
    let text = |v: &Value| v.as_str().map(str::to_owned);
    let chat_card = |c: &Value| ChatCard {
        id: text(&c["entry"]["id"]).unwrap_or_default(),
        title: text(&c["entry"]["title"]).unwrap_or_default(),
        agent_name: text(&c["agentName"]),
        index_model: text(&c["indexModel"]),
        branch: text(&c["branch"]).unwrap_or_default(),
    };
    let mut state = Commands::default();
    let steps = vector["steps"].as_array().ok_or("no steps")?;
    for (index, step) in steps.iter().enumerate() {
        let event = &step["event"];
        if event["action"] == "filterChatCards" {
            let args = &event["args"];
            let cards: Vec<ChatCard> = args[0].as_array().ok_or("no cards")?.iter().map(chat_card).collect();
            let query = args[1].as_str().unwrap_or_default();
            let got: Vec<&str> = cards.iter().filter(|c| c.matches(query)).map(|c| c.id.as_str()).collect();
            if let Some(problem) = diff(&format!("step {index} chats"), &step["result"], &json!(got)) {
                return Err(problem);
            }
            continue;
        }
        if step.get("query").is_some() {
            let args = &event["args"];
            let keywords: Vec<String> = args.get(2).and_then(Value::as_array).into_iter().flatten().filter_map(|k| k.as_str().map(str::to_owned)).collect();
            let got = command_score(args[0].as_str().unwrap_or_default(), args[1].as_str().unwrap_or_default(), &keywords);
            let want = step["result"].as_f64().ok_or("no score")?;
            if (got - want).abs() > 1e-12 {
                return Err(format!("step {index}: score of {:?} for {:?}: want {want}, got {got}", args[0], args[1]));
            }
            continue;
        }
        state.step(crate::common::call(event)?);
        if let Some(problem) = diff(&format!("step {index} state"), &step["state"], &to_value(&state)) {
            return Err(problem);
        }
    }
    Ok(steps.len())
}

/// Replays the Profiles page's reads: each step's `result` is what the TypeScript lib returned.
pub fn replay_terminal_profiles(vector: &Value) -> Result<usize, String> {
    use model::settings::profiles::{Draft, Profile};
    let steps = vector["steps"].as_array().ok_or("no steps")?;
    for (index, step) in steps.iter().enumerate() {
        let event = &step["event"];
        let args = &event["args"];
        let got = match event["action"].as_str().unwrap_or_default() {
            "parse" => match parse::<Draft>(&args[0])?.parse(args[1].as_str().unwrap_or_default()) {
                Ok(profile) => json!({ "ok": true, "profile": profile }),
                Err(errors) => json!({ "ok": false, "errors": errors }),
            },
            "toDraft" => to_value(&Draft::of(&parse::<Profile>(&args[0])?)),
            "commandLine" => json!(parse::<Profile>(&args[0])?.command_line()),
            other => return Err(format!("step {index}: no read {other}")),
        };
        if let Some(problem) = diff(&format!("step {index} result"), &step["result"], &got) {
            return Err(problem);
        }
    }
    Ok(steps.len())
}

/// Replays the project settings window's reads: the `.env` import and the shell select.
pub fn replay_project_settings(vector: &Value) -> Result<usize, String> {
    use model::prefs::EnvVar;
    use model::settings::project::{self, Shell};
    let steps = vector["steps"].as_array().ok_or("no steps")?;
    for (index, step) in steps.iter().enumerate() {
        let event = &step["event"];
        let args = &event["args"];
        let text = |i: usize| args[i].as_str().unwrap_or_default();
        let shells = |i: usize| -> Result<Vec<Shell>, String> {
            if args[i].is_null() { Ok(Vec::new()) } else { parse::<Vec<Shell>>(&args[i]["available"]) }
        };
        let got = match event["action"].as_str().unwrap_or_default() {
            "envPath" => json!(project::env_path(text(0))),
            "parseEnv" => to_value(&project::parse_env(text(0))),
            "mergeEnv" => to_value(&project::merge_env(&parse::<Vec<EnvVar>>(&args[0])?, &parse::<Vec<EnvVar>>(&args[1])?)),
            "shellValue" => json!(project::shell_value(text(0), &shells(1)?)),
            "shellOptions" => to_value(&project::shell_options(text(0), &shells(1)?, text(2))),
            "shellFromValue" => json!(project::shell_from_value(text(0))),
            other => return Err(format!("step {index}: no read {other}")),
        };
        if let Some(problem) = diff(&format!("step {index} result"), &step["result"], &got) {
            return Err(problem);
        }
    }
    Ok(steps.len())
}

/// Replays the MCP servers page's reads: the registry, the form's draft and the status word.
pub fn replay_mcp_servers(vector: &Value) -> Result<usize, String> {
    use model::settings::mcp::{self, Auth, Draft, Object};
    let steps = vector["steps"].as_array().ok_or("no steps")?;
    for (index, step) in steps.iter().enumerate() {
        let event = &step["event"];
        let args = &event["args"];
        let text = |i: usize| args[i].as_str().unwrap_or_default();
        let object = |i: usize| -> Result<Object, String> { parse::<Object>(&args[i]) };
        let draft = || -> Result<Draft, String> { parse::<Draft>(&args[0]) };
        let got = match event["action"].as_str().unwrap_or_default() {
            "normalize" => to_value(&mcp::normalize_registry(&args[0])),
            "validate" => to_value(&mcp::validate(&object(0)?)),
            "sameLaunch" => json!(mcp::same_launch(&object(0)?, &object(1)?)),
            "tokenize" => to_value(&mcp::tokenize(text(0))),
            "join" => json!(mcp::join(&parse::<Vec<String>>(&args[0])?)),
            "transportForUrl" => json!(mcp::transport_for_url(text(0)).as_str()),
            "liftEnvPrefix" => to_value(&mcp::lift_env_prefix(&draft()?.local)),
            "rowProblem" => json!(mcp::row_problem(&mcp::Row::new(text(0), text(1)))),
            "toConfig" => match mcp::draft_to_config(&draft()?) {
                Ok(config) => json!({ "config": config }),
                Err(errors) => {
                    let map: Map<String, Value> = errors.into_iter().map(|(f, m)| (to_value(&f).as_str().unwrap_or_default().to_owned(), json!(m))).collect();
                    json!({ "errors": map })
                }
            },
            "uncheckedConfig" => Value::Object(mcp::unchecked_config(&draft()?)),
            "fromServer" => to_value(&mcp::draft_from_server(&object(0)?)),
            "withUrl" => {
                let mut d = draft()?;
                d.remote = mcp::with_url(&d.remote, text(1));
                to_value(&d)
            }
            "withAuth" => {
                let mut d = draft()?;
                d.remote = mcp::with_auth(&d.remote, parse::<Auth>(&args[1])?);
                to_value(&d)
            }
            "status" => to_value(&mcp::status(args[0].as_bool().unwrap_or_default(), args[1].as_bool().unwrap_or_default(), args[2].as_str())),
            "target" => json!(mcp::target(&object(0)?)),
            "toolCount" => json!(mcp::tool_count(args[0].as_u64().unwrap_or_default() as usize)),
            "reasonText" => json!(model::settings::mcp_catalog::reason_text(&args[0])),
            "stateLabel" => json!(model::settings::mcp_catalog::state_label(text(0))),
            "sampleText" => json!(model::settings::mcp_catalog::sample_text(&args[0])),
            "syncText" => json!(model::settings::mcp_catalog::sync_text(&args[0], "")),
            "syncErrorText" => json!(model::settings::mcp_catalog::sync_error_text(text(0))),
            "updatedText" => json!(model::settings::mcp_catalog::updated_text(None, "")),
            "jsonImport" => {
                let out = model::settings::mcp_json::parse_import(text(0));
                json!({ "servers": out.servers, "errors": out.errors })
            }
            "installInitial" => to_value(&model::settings::mcp_install::initial_values(&parse::<Vec<model::settings::mcp_install::Field>>(&args[0])?)),
            "installSplit" => {
                let fields: Vec<model::settings::mcp_install::Field> = parse(&args[0])?;
                let values: std::collections::BTreeMap<String, String> = parse(&args[1])?;
                let (inputs, secrets) = model::settings::mcp_install::split_values(&fields, &values);
                json!({ "inputs": inputs, "secretValues": secrets })
            }
            "canStartOver" => {
                let failure = match (args[0]["kind"].as_str(), args[0]["code"].as_str()) {
                    (Some("host"), Some(code)) => model::settings::mcp_install::Failure::Host(code.into()),
                    (Some("unsupported"), _) => model::settings::mcp_install::Failure::Unsupported(args[0]["reason"].clone()),
                    _ => model::settings::mcp_install::Failure::Save,
                };
                json!(failure.can_start_over())
            }
            other => return Err(format!("step {index}: no read {other}")),
        };
        if let Some(problem) = diff(&format!("step {index} result"), &step["result"], &got) {
            return Err(problem);
        }
    }
    Ok(steps.len())
}

/// Replays the Sub-agents page's reads against `model::settings::subagents`.
pub fn replay_subagents(vector: &Value) -> Result<usize, String> {
    use model::settings::subagents::{self as rules, Object};
    let steps = vector["steps"].as_array().ok_or("no steps")?;
    for (index, step) in steps.iter().enumerate() {
        let event = &step["event"];
        let args = &event["args"];
        let text = |i: usize| args[i].as_str().unwrap_or_default();
        let list = |i: usize| -> Result<Vec<Object>, String> { parse::<Vec<Object>>(&args[i]) };
        let object = |i: usize| -> Result<Object, String> { parse::<Object>(&args[i]) };
        let got = match event["action"].as_str().unwrap_or_default() {
            "idError" => json!(rules::id_error(text(0), &list(1)?)),
            "uniqueId" => json!(rules::unique_id(text(0), &list(1)?)),
            "shortName" => json!(rules::agent_short_name(text(0))),
            "canDelete" => json!(rules::can_delete(&object(0)?, &list(1)?)),
            "lockReason" => json!(rules::agent_lock_reason(&object(0)?, &list(1)?)),
            "skipsPrompts" => json!(rules::skips_prompts(&object(0)?)),
            "upserts" => json!(rules::profile_upserts(&list(0)?, &object(1)?, args[2].as_str())),
            "apply" => rules::apply_input(&args[0], &args[1]),
            other => return Err(format!("step {index}: no read {other}")),
        };
        if let Some(problem) = diff(&format!("step {index} result"), &step["result"], &got) {
            return Err(problem);
        }
    }
    Ok(steps.len())
}
