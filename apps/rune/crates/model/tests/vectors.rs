//! Replays every vector `tools/parity/vectors/record.test.ts` recorded from
//! the TypeScript stores and compares the model's state, effects and replies
//! with the recorded ones after every step. A second test fails when a
//! store's TypeScript source changed after its vectors were recorded.

mod bug_report;
mod automations;
mod common;
mod git;
mod github;
mod settings;
mod setup;
mod snapshots;
mod terminal;

use common::{Replay, call, diff, out_dir, parse, read_json, source_hash, to_value};

use model::persistence::{TerminalInfo, pane_tree};
use model::shortcuts::{self, KeyEvent, Platform, Shortcuts};
use model::workspace::PaneNode;
use model::{Effect, overlay, prefs, projects, shell_layout, workspace};
use serde::Serialize;
use serde_json::{Value, json};

/// One store under replay.
trait Store {
    type State: Serialize;
    fn setup(context: &Value, initial: &Value) -> Result<Self::State, String>;
    /// Apply a step; the effects and the reply, if the action returns one.
    fn step(
        state: &mut Self::State,
        event: &Value,
        ids: &mut Replay,
    ) -> Result<(Vec<Effect>, Option<Value>), String>;
    fn query(state: &Self::State, event: &Value) -> Result<Value, String>;
}

struct Workspace;

impl Store for Workspace {
    type State = workspace::State;
    fn setup(_: &Value, initial: &Value) -> Result<Self::State, String> {
        parse(initial)
    }
    fn step(
        state: &mut Self::State,
        event: &Value,
        ids: &mut Replay,
    ) -> Result<(Vec<Effect>, Option<Value>), String> {
        let out = workspace::step(state, call(event)?, ids);
        Ok((out.effects, out.reply.as_ref().map(to_value)))
    }
    fn query(state: &Self::State, event: &Value) -> Result<Value, String> {
        Ok(to_value(&workspace::query(state, call(event)?)))
    }
}

struct Shortcut;

impl Store for Shortcut {
    type State = shortcuts::State;
    /// The platform's defaults: every vector checks them against `initial`.
    fn setup(context: &Value, _: &Value) -> Result<Self::State, String> {
        let platform: Platform = context
            .get("platform")
            .map(parse)
            .transpose()?
            .unwrap_or_default();
        Ok(shortcuts::State::new(platform))
    }
    fn step(
        state: &mut Self::State,
        event: &Value,
        _: &mut Replay,
    ) -> Result<(Vec<Effect>, Option<Value>), String> {
        shortcuts::step(state, call(event)?);
        Ok((Vec::new(), None))
    }
    fn query(state: &Self::State, event: &Value) -> Result<Value, String> {
        let args = &event["args"];
        let platform = state.platform;
        Ok(match event["action"].as_str().unwrap_or_default() {
            "findConflictingShortcut" => {
                let shortcuts: Shortcuts = parse(&args[0])?;
                let key: String = parse(&args[1])?;
                let exclude: String = parse(&args[2])?;
                to_value(&shortcuts::find_conflicting_shortcut(
                    &shortcuts, &key, &exclude,
                ))
            }
            "normalizeKeyEvent" => {
                let event: KeyEvent = parse(&args[0])?;
                json!(shortcuts::normalize_key_event(&event, platform))
            }
            "formatKeyForDisplay" => json!(shortcuts::format_key_for_display(
                args[0].as_str().unwrap_or_default(),
                platform
            )),
            "matchesShortcut" => {
                let event: KeyEvent = parse(&args[0])?;
                json!(shortcuts::matches_shortcut(
                    &event,
                    args[1].as_str().unwrap_or_default(),
                    platform
                ))
            }
            other => return Err(format!("no shortcuts helper {other}")),
        })
    }
}

struct ShellLayout;

impl Store for ShellLayout {
    type State = shell_layout::State;
    fn setup(_: &Value, initial: &Value) -> Result<Self::State, String> {
        parse(initial)
    }
    fn step(
        state: &mut Self::State,
        event: &Value,
        _: &mut Replay,
    ) -> Result<(Vec<Effect>, Option<Value>), String> {
        // Setters take a raw `Value`, which an args array would also match.
        let action = event["action"].as_str().ok_or("event without action")?;
        let shape = match event["args"].as_array().map(Vec::as_slice) {
            Some([only]) => json!({ action: only }),
            _ => json!({ action: event["args"] }),
        };
        Ok((shell_layout::step(state, parse(&shape)?), None))
    }
    fn query(_: &Self::State, event: &Value) -> Result<Value, String> {
        Err(format!("no shell-layout query {}", event["action"]))
    }
}

struct Overlay;

impl Store for Overlay {
    type State = overlay::State;
    fn setup(_: &Value, initial: &Value) -> Result<Self::State, String> {
        parse(initial)
    }
    fn step(
        state: &mut Self::State,
        event: &Value,
        _: &mut Replay,
    ) -> Result<(Vec<Effect>, Option<Value>), String> {
        let out = overlay::step(state, call(event)?);
        Ok((out.effects, out.closed.map(Value::Bool)))
    }
    fn query(_: &Self::State, event: &Value) -> Result<Value, String> {
        Ok(json!(overlay::sentinel_depth(&event["args"][0])))
    }
}

/// The project store with the prefs layout, as the recorder snapshots them.
struct ProjectsAndPrefs(projects::Model);

impl Serialize for ProjectsAndPrefs {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct View<'a> {
            #[serde(flatten)]
            store: &'a projects::State,
            layout: &'a prefs::Layout,
        }
        View {
            store: &self.0.store,
            layout: &self.0.prefs.layout,
        }
        .serialize(serializer)
    }
}

struct Projects;

impl Store for Projects {
    type State = ProjectsAndPrefs;
    fn setup(_: &Value, initial: &Value) -> Result<Self::State, String> {
        let mut model = projects::Model {
            store: parse(initial)?,
            ..Default::default()
        };
        model.prefs.layout = parse(&initial["layout"])?;
        Ok(ProjectsAndPrefs(model))
    }
    fn step(
        state: &mut Self::State,
        event: &Value,
        ids: &mut Replay,
    ) -> Result<(Vec<Effect>, Option<Value>), String> {
        let out = projects::step(&mut state.0, call(event)?, ids);
        Ok((out.effects, out.reply))
    }
    fn query(_: &Self::State, _: &Value) -> Result<Value, String> {
        Err("the projects store has no reads".into())
    }
}

/// The record's pane tree helpers: no state, each step's result is the function's.
struct PaneTree;

#[derive(Serialize)]
struct Stateless {}

impl Store for PaneTree {
    type State = Stateless;
    fn setup(_: &Value, _: &Value) -> Result<Self::State, String> {
        Ok(Stateless {})
    }
    fn step(
        _: &mut Self::State,
        event: &Value,
        ids: &mut Replay,
    ) -> Result<(Vec<Effect>, Option<Value>), String> {
        let args = &event["args"];
        let mut effects = Vec::new();
        let result = match event["action"].as_str().unwrap_or_default() {
            "serializePaneTree" => {
                let node: PaneNode = parse(&args[0])?;
                to_value(&pane_tree::serialize(&node))
            }
            "deserializePaneTree" => to_value(&pane_tree::deserialize(&args[0], ids, &mut effects)),
            "reconcileTerminalTabs" => {
                let node: PaneNode = parse(&args[0])?;
                let open: std::collections::BTreeSet<String> = parse(&args[1])?;
                let live: Vec<TerminalInfo> = parse(&args[2])?;
                let layout: Option<Vec<TerminalInfo>> =
                    args[3].get("terminals").map(parse).transpose()?;
                to_value(&pane_tree::reconcile_terminal_tabs(
                    &node,
                    &open,
                    &live,
                    layout.as_deref(),
                    ids,
                ))
            }
            other => return Err(format!("no pane-tree function {other}")),
        };
        Ok((effects, Some(result)))
    }
    fn query(_: &Self::State, event: &Value) -> Result<Value, String> {
        Err(format!("no pane-tree read {}", event["action"]))
    }
}

fn replay<S: Store>(vector: &Value) -> Result<usize, String> {
    let empty = json!({});
    let context = vector.get("context").unwrap_or(&empty);
    let mut state = S::setup(context, &vector["initial"])?;
    if let Some(problem) = diff("initial", &vector["initial"], &to_value(&state)) {
        return Err(format!("initial state does not round-trip: {problem}"));
    }
    let steps = vector["steps"].as_array().ok_or("no steps")?;
    for (index, recorded) in steps.iter().enumerate() {
        let at = |what: &str| format!("step {index} ({}) {what}", recorded["event"]["action"]);
        let mut ids = Replay {
            ids: parse(&recorded["ids"])?,
            overdrawn: 0,
        };
        let before = to_value(&state);
        let (effects, reply) = if recorded.get("query").is_some() {
            (
                Vec::new(),
                Some(S::query(&state, &recorded["event"]).map_err(|e| at(&e))?),
            )
        } else {
            S::step(&mut state, &recorded["event"], &mut ids).map_err(|e| at(&e))?
        };
        if recorded.get("query").is_some() && to_value(&state) != before {
            return Err(at("a read changed the state"));
        }
        if !ids.ids.is_empty() || ids.overdrawn > 0 {
            return Err(at(&format!(
                "drew {} ids too few or {} too many",
                ids.ids.len(),
                ids.overdrawn
            )));
        }
        let checks = [
            ("effects", &recorded["effects"], &to_value(&effects)),
            (
                "result",
                recorded.get("result").unwrap_or(&Value::Null),
                &reply.unwrap_or(Value::Null),
            ),
            ("state", &recorded["state"], &to_value(&state)),
        ];
        for (what, want, got) in checks {
            if let Some(problem) = diff(what, want, got) {
                return Err(at(&problem));
            }
        }
    }
    Ok(steps.len())
}

fn manifest() -> Value {
    read_json(&out_dir().join("manifest.json"))
}

#[test]
fn replays_every_vector() {
    let manifest = manifest();
    let stores = manifest["stores"].as_object().expect("manifest stores");
    let mut failures = Vec::new();
    let mut total = 0;
    for (store, entry) in stores {
        let names = entry["vectors"].as_array().expect("vector list");
        let file = format!("{store}.json");
        let vectors = read_json(&out_dir().join(&file));
        let vectors = vectors.as_array().unwrap_or_else(|| {
            panic!("{file}: want an array of vectors");
        });
        let recorded: Vec<&Value> = vectors.iter().map(|vector| &vector["name"]).collect();
        let listed: Vec<&Value> = names.iter().collect();
        if recorded != listed {
            failures.push(format!("{file}: vector names differ from the manifest"));
        }
        for vector in vectors {
            let name = vector["name"].as_str().unwrap_or("?");
            let result = match store.as_str() {
                "workspace" => replay::<Workspace>(vector),
                "shortcuts" => replay::<Shortcut>(vector),
                "overlay" => replay::<Overlay>(vector),
                "projects" => replay::<Projects>(vector),
                "shell-layout" => replay::<ShellLayout>(vector),
                "pane-tree" => replay::<PaneTree>(vector),
                "git-status" => git::replay(vector),
                "git-routes" => git::replay_routes(vector),
                "github" => github::replay(vector),
                "github-routes" => github::replay_routes(vector),
                "app-settings" => settings::replay(vector),
                "settings-routes" => settings::replay_routes(vector),
                "palette" => settings::replay_palette(vector),
                "terminal-profiles" => settings::replay_terminal_profiles(vector),
                "project-settings" => settings::replay_project_settings(vector),
                "mcp-servers" => settings::replay_mcp_servers(vector),
                "snapshots" => snapshots::replay(vector),
                "setup" => setup::replay(vector),
                "bug-report" => bug_report::replay(vector),
                "automation-draft" => replay::<automations::DraftStore>(vector),
                "automation-run" => replay::<automations::RunStore>(vector),
                "automation-routes" => replay::<automations::RoutesStore>(vector),
                "subagents" => settings::replay_subagents(vector),
                "accounts" => settings::replay_accounts(vector),
                other => Err(format!("no replay for store {other}")),
            };
            match result {
                Ok(steps) => total += steps,
                Err(problem) => failures.push(format!("{file} {name}: {problem}")),
            }
        }
        println!("{store}: {} vectors", vectors.len());
    }
    println!("{total} steps replayed");
    assert!(
        failures.is_empty(),
        "{} vectors failed:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn vectors_are_not_older_than_their_stores() {
    let manifest = manifest();
    let mut stale = Vec::new();
    for (store, entry) in manifest["stores"].as_object().expect("manifest stores") {
        for (path, recorded) in entry["sources"].as_object().expect("sources") {
            if source_hash(path) != recorded.as_str().unwrap_or_default() {
                stale.push(format!("{store}: {path}"));
            }
        }
    }
    assert!(
        stale.is_empty(),
        "TypeScript changed after its vectors were recorded; rerun \
         `RECORD_VECTORS=1 bun run test tools/parity/vectors` and port the change:\n{}",
        stale.join("\n")
    );
}
