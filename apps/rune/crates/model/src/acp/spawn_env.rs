//! What a spawn reads besides the store: the agent config a saved id
//! launches (`findStoredAgentConfig`), the Project a working directory
//! belongs to (`projectForCwd`), and the env the agent gets
//! (`resolveAgentSpawnEnv`).

use std::sync::OnceLock;

use serde_json::{Map, Value};

use super::types::Object;

const REGISTRY_PREFIX: &str = "acp-registry:";

/// Legacy registry id to canonical id, the map the renderer and the host read.
fn aliases() -> &'static Map<String, Value> {
    static ALIASES: OnceLock<Map<String, Value>> = OnceLock::new();
    ALIASES.get_or_init(|| {
        serde_json::from_str(include_str!(
            "../../../../../../src/shared/acp-agent-aliases.json"
        ))
        .expect("acp-agent-aliases.json is a JSON object")
    })
}

fn bare_agent_id(id: &str) -> &str {
    id.strip_prefix(REGISTRY_PREFIX).unwrap_or(id)
}

fn is_legacy_agent_id(id: &str) -> bool {
    aliases().contains_key(bare_agent_id(id))
}

fn canonical_agent_id(id: &str) -> &str {
    let bare = bare_agent_id(id);
    match aliases().get(bare).and_then(Value::as_str) {
        Some(canonical) if !canonical.is_empty() => canonical,
        _ => bare,
    }
}

/// `sameAgentId`: equal, or one is a legacy registry alias of the other.
fn same_agent_id(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    if !is_legacy_agent_id(a) && !is_legacy_agent_id(b) {
        return false;
    }
    let left = registry_side(a);
    left.is_some() && left == registry_side(b)
}

fn registry_side(id: &str) -> Option<&str> {
    (id.starts_with(REGISTRY_PREFIX) || is_legacy_agent_id(id)).then(|| canonical_agent_id(id))
}

fn id_of(value: &Value) -> Option<&str> {
    value.get("id").and_then(Value::as_str)
}

/// `findStoredAgentConfig`: an exact id wins, else a legacy id launches the
/// config it resolves to.
pub(crate) fn find_stored_agent_config<'a>(configs: &'a [Object], id: &str) -> Option<&'a Object> {
    let exact = configs
        .iter()
        .find(|config| config.get("id").and_then(Value::as_str) == Some(id));
    if exact.is_some() || !is_legacy_agent_id(id) {
        return exact;
    }
    configs.iter().find(|config| {
        config
            .get("id")
            .and_then(Value::as_str)
            .is_some_and(|stored| same_agent_id(stored, id))
    })
}

fn normalize_dir_path(path: &str) -> String {
    let normalized = crate::js_trim(path).replace('\\', "/");
    let normalized = normalized.trim_end_matches('/');
    if normalized.is_empty() {
        "/".to_owned()
    } else {
        normalized.to_owned()
    }
}

/// `projectForCwd`: the Project whose root or worktree is `cwd`, else the
/// one with the deepest root that holds it.
pub(crate) fn project_for_cwd<'a>(projects: &'a [Value], cwd: &str) -> Option<&'a Value> {
    let target = normalize_dir_path(cwd);
    let mut containing = None;
    let mut depth = None;
    for project in projects {
        let worktrees = project.get("worktrees").and_then(Value::as_array);
        if worktrees.into_iter().flatten().any(|worktree| {
            worktree
                .get("path")
                .and_then(Value::as_str)
                .is_some_and(|path| normalize_dir_path(path) == target)
        }) {
            return Some(project);
        }
        let Some(path) = project
            .get("path")
            .and_then(Value::as_str)
            .filter(|path| !path.is_empty())
        else {
            continue;
        };
        let root = normalize_dir_path(path);
        if root == target {
            return Some(project);
        }
        let prefix = if root == "/" {
            "/".to_owned()
        } else {
            format!("{root}/")
        };
        let longer = depth.is_none_or(|depth| root.encode_utf16().count() > depth);
        if target.starts_with(&prefix) && longer {
            containing = Some(project);
            depth = Some(root.encode_utf16().count());
        }
    }
    containing
}

/// `resolveSpawnProject`: the explicit Project (only its id when unknown),
/// else the Project that holds `cwd`.
pub(crate) fn resolve_spawn_project(
    projects: &[Value],
    cwd: &str,
    project_id: Option<&str>,
) -> Option<Value> {
    let explicit = project_id.map(crate::js_trim).filter(|id| !id.is_empty());
    match explicit {
        Some(id) => Some(
            projects
                .iter()
                .find(|project| id_of(project) == Some(id))
                .cloned()
                .unwrap_or_else(|| serde_json::json!({ "id": id })),
        ),
        None => project_for_cwd(projects, cwd).cloned(),
    }
}

/// `resolveAgentSpawnEnv`: the Project's env vars with the agent's own env
/// on top; an agent value `$NAME` reads the Project's `NAME`. Project values
/// pass through unchanged, like the renderer's `passThroughProjectEnv`: the
/// backend expands any remaining `$VAR` against the process env at spawn,
/// which is the only place the real inherited env exists. Expanding here
/// would resolve every reference against nothing.
pub(crate) fn resolve_agent_spawn_env(
    project_env_vars: Option<&Value>,
    agent_env: Option<&Value>,
) -> Object {
    let mut env = Object::new();
    for var in project_env_vars
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let key = super::js::string_of(var.get("key"));
        if crate::js_trim(&key).is_empty() || var.get("key").is_none_or(Value::is_null) {
            continue;
        }
        let value = var.get("value").and_then(Value::as_str).unwrap_or_default();
        env.insert(key, Value::String(value.to_owned()));
    }
    let project_env = env.clone();
    for (key, raw) in agent_env.and_then(Value::as_object).into_iter().flatten() {
        if crate::js_trim(key).is_empty() {
            continue;
        }
        let raw = raw.as_str().unwrap_or_default();
        let value = match raw.strip_prefix('$') {
            Some(name) => project_env
                .get(name)
                .and_then(Value::as_str)
                .unwrap_or_default(),
            None => raw,
        };
        if !value.is_empty() {
            env.insert(key.clone(), Value::String(value.to_owned()));
        }
    }
    env
}
