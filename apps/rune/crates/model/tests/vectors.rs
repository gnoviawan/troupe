//! Replays every vector `tools/parity/vectors/record.test.ts` recorded from
//! the TypeScript stores and compares the model's state, effects and replies
//! with the recorded ones after every step. A second test fails when a
//! store's TypeScript source changed after its vectors were recorded.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};

use model::shortcuts::{self, KeyEvent, Platform, Shortcuts};
use model::{Effect, Ids, overlay, prefs, projects, shell_layout, workspace};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use sha1::{Digest, Sha1};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..")
}

fn out_dir() -> PathBuf {
    repo_root().join("tools/parity/vectors/out")
}

fn read_json(path: &Path) -> Value {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// Hands out the ids the TypeScript run drew during one step.
#[derive(Default)]
struct Replay {
    ids: VecDeque<String>,
    overdrawn: usize,
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
fn call<T: DeserializeOwned>(event: &Value) -> Result<T, String> {
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

fn to_value<T: Serialize>(value: &T) -> Value {
    serde_json::to_value(value).expect("serializable")
}

/// JSON equality with numbers compared as f64 (`50` equals `50.0`).
fn diff(path: &str, want: &Value, got: &Value) -> Option<String> {
    match (want, got) {
        (Value::Number(a), Value::Number(b)) => {
            (a.as_f64() != b.as_f64()).then(|| format!("{path}: want {a}, got {b}"))
        }
        (Value::Array(a), Value::Array(b)) => {
            if a.len() != b.len() {
                return Some(format!(
                    "{path}: want {} items, got {}: {got}",
                    a.len(),
                    b.len()
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
                    (Some(a), None) => Some(format!("{path}.{key}: want {a}, missing")),
                    (None, Some(b)) => Some(format!("{path}.{key}: unexpected {b}")),
                    (None, None) => None,
                })
        }
        _ => (want != got).then(|| format!("{path}: want {want}, got {got}")),
    }
}

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

fn parse<T: DeserializeOwned>(value: &Value) -> Result<T, String> {
    serde_json::from_value(value.clone()).map_err(|e| e.to_string())
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

/// Same hash as `sourceHash` in `tools/parity/vectors/record.ts`.
fn source_hash(repo_path: &str) -> String {
    let text = std::fs::read_to_string(repo_root().join(repo_path))
        .unwrap_or_else(|e| panic!("{repo_path}: {e}"))
        .replace("\r\n", "\n");
    Sha1::digest(text.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
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
