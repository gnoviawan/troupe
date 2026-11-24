//! The user's own control picks: `setMode`, `setModel` and
//! `setConfigOption` in `slices/prompt.ts`, with the composer record each
//! pick saves (`persistComposerOptions` and `modelValuesPatch` in
//! `composer-options.ts`).
//!
//! A pick is two steps, as sending is. The action returns the set command
//! (`Effect::SetMode`, `SetModel`, `SetConfigOption`); the app sends it and,
//! once the agent acknowledged it, hands the same effect back as
//! [`Event::ControlSet`](super::Event::ControlSet) with the agent's reply. No
//! event is sure to echo the applied value, so the acknowledgement applies it.

use serde::Deserialize;
use serde_json::{Map, Value, json};

use super::controls::{
    MODEL_CATEGORY, align_model_projection, category, config_options, mark_echo_logged,
    merge_config_options, model_catalog, model_snapshot,
};
use super::restore::RunId;
use super::{Effect, Env, State};

/// A mode-category option (Claude's permission mode) stays agent-wide.
pub(super) const MODE_CATEGORY: &str = "mode";

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PickOptions {
    /// `false` applies the pick without saving it as the user's choice.
    #[serde(default)]
    pub persist: Option<bool>,
}

impl PickOptions {
    fn saves(options: Option<&Self>) -> bool {
        options.and_then(|options| options.persist) != Some(false)
    }
}

pub(super) fn unknown_session(session_id: &str) -> Effect {
    Effect::rejected(format!("unknown session {session_id}"))
}

/// `String(value)` for a boolean pick, the value itself otherwise.
fn stored(value: &Value) -> Value {
    match value {
        Value::Bool(flag) => json!(flag.to_string()),
        other => other.clone(),
    }
}

fn single(key: &str, value: Value) -> Value {
    let mut map = Map::new();
    map.insert(key.to_owned(), value);
    Value::Object(map)
}

impl State {
    fn current_model(&self, session_id: &str) -> Option<String> {
        self.sessions
            .get(session_id)
            .and_then(model_snapshot)
            .map(|(model_id, _)| model_id)
    }

    pub(crate) fn set_mode(&self, session_id: String, mode_id: String, fx: &mut Vec<Effect>) {
        let Some(session) = self.sessions.get(&session_id) else {
            fx.push(unknown_session(&session_id));
            return;
        };
        fx.push(Effect::SetMode {
            agent_id: session.agent_id.clone(),
            session_id,
            mode_id,
            retried: false,
        });
    }

    pub(crate) fn set_model(
        &self,
        session_id: String,
        model_id: String,
        options: Option<PickOptions>,
        run: Option<RunId>,
        fx: &mut Vec<Effect>,
    ) {
        let Some(session) = self.sessions.get(&session_id) else {
            fx.push(unknown_session(&session_id));
            return;
        };
        let model_option = config_options(session)
            .iter()
            .find(|option| category(option) == Some(MODEL_CATEGORY))
            .map(|option| option.get("id").cloned().unwrap_or(Value::Null));
        fx.push(Effect::SetModel {
            agent_id: session.agent_id.clone(),
            session_id,
            model_id,
            persist: PickOptions::saves(options.as_ref()),
            model_option,
            run,
            retried: false,
        });
    }

    pub(crate) fn set_config_option(
        &self,
        session_id: String,
        config_id: String,
        value: Value,
        options: Option<PickOptions>,
        run: Option<RunId>,
        fx: &mut Vec<Effect>,
    ) {
        let Some(session) = self.sessions.get(&session_id) else {
            fx.push(unknown_session(&session_id));
            return;
        };
        let is_model = config_options(session).iter().any(|option| {
            option.get("id").and_then(Value::as_str) == Some(config_id.as_str())
                && category(option) == Some(MODEL_CATEGORY)
        });
        fx.push(Effect::SetConfigOption {
            agent_id: session.agent_id.clone(),
            model_at_request: self.current_model(&session_id),
            session_id,
            config_id,
            value,
            persist: PickOptions::saves(options.as_ref()),
            is_model,
            run,
        });
    }

    /// The agent acknowledged a set command; `reply` is its option snapshot
    /// for `SetConfigOption` (`None` when it sent none).
    pub(crate) fn on_control_set(
        &mut self,
        command: Effect,
        reply: Option<Vec<Value>>,
        env: &mut Env<'_>,
        fx: &mut Vec<Effect>,
    ) {
        match command {
            Effect::SetMode {
                agent_id,
                session_id,
                mode_id,
                ..
            } => {
                if let Some(modes) = self
                    .sessions
                    .get_mut(&session_id)
                    .and_then(|session| session.rest.get_mut("modes"))
                    .filter(|modes| modes.is_object())
                {
                    modes["currentModeId"] = json!(mode_id);
                }
                self.cache_options_from_session(&session_id, env, fx);
                if let Some(config_id) = self.config_id_for_agent(&agent_id) {
                    fx.push(Effect::PersistComposerOptions {
                        config_id,
                        patch: json!({ "modeId": mode_id }),
                    });
                }
            }
            Effect::SetModel {
                agent_id,
                session_id,
                model_id,
                persist,
                model_option,
                ..
            } => {
                if let Some(session) = self.sessions.get_mut(&session_id) {
                    if let Some(models) = session.rest.get_mut("models").filter(|m| m.is_object()) {
                        models["currentModelId"] = json!(model_id);
                    }
                    if let Some(Value::Array(options)) = session.rest.get_mut("configOptions") {
                        for option in options
                            .iter_mut()
                            .filter(|option| category(option) == Some(MODEL_CATEGORY))
                        {
                            option["currentValue"] = json!(model_id);
                        }
                    }
                    align_model_projection(session);
                    self.set_index_model(&session_id);
                }
                if let Some(config_id) = self.config_id_for_agent(&agent_id)
                    && persist
                {
                    let mut patch = single("modelId", json!(model_id));
                    if let Some(option) = model_option {
                        patch["configValues"] =
                            single(&super::js::string_of(Some(&option)), json!(model_id));
                    }
                    fx.push(Effect::PersistComposerOptions { config_id, patch });
                }
                self.cache_options_from_session(&session_id, env, fx);
            }
            Effect::SetConfigOption {
                agent_id,
                session_id,
                config_id,
                value,
                persist,
                model_at_request,
                is_model,
                ..
            } => self.on_config_option_set(
                ConfigOptionSet {
                    agent_id,
                    session_id,
                    config_id,
                    value,
                    persist,
                    model_at_request,
                    is_model,
                },
                reply,
                env,
                fx,
            ),
            _ => {}
        }
    }

    fn on_config_option_set(
        &mut self,
        set: ConfigOptionSet,
        reply: Option<Vec<Value>>,
        env: &mut Env<'_>,
        fx: &mut Vec<Effect>,
    ) {
        let prior = self.sessions.get(&set.session_id);
        let previous = prior.map(config_options).unwrap_or_default();
        let next = reply.unwrap_or_else(|| {
            previous
                .iter()
                .map(|option| {
                    let mut option = option.clone();
                    if option.get("id").and_then(Value::as_str) == Some(set.config_id.as_str()) {
                        option["currentValue"] = set.value.clone();
                    }
                    option
                })
                .collect()
        });
        let creation = prior
            .and_then(|session| session.rest.get("creationOptionDefaults"))
            .and_then(|defaults| defaults.get("configValues"));
        let mut echoed = Vec::new();
        let updated =
            merge_config_options(previous, next, creation, Some(&set.config_id), &mut echoed);
        let logged = prior.and_then(|session| session.rest.get("creationEchoLogged"));
        echoed.retain(|id| {
            !logged
                .and_then(|logged| logged.get(id))
                .is_some_and(super::js::truthy)
        });
        if let Some(session) = self.sessions.get_mut(&set.session_id) {
            session
                .rest
                .insert("configOptions".into(), Value::Array(updated.clone()));
            for id in &echoed {
                mark_echo_logged(session, id);
            }
            align_model_projection(session);
            self.set_index_model(&set.session_id);
        }
        for id in echoed {
            fx.push(Effect::warn(
                "acp.configOptionEchoPreserved",
                format!(
                    "set_config_option response for session {} re-asserted the creation default on '{id}'; kept the session's current value",
                    set.session_id
                ),
            ));
        }
        let Some(config_id) = self.config_id_for_agent(&set.agent_id) else {
            return;
        };
        self.write_options_cache(&config_id, &updated, env, fx);
        if !set.persist {
            return;
        }
        let value = stored(&set.value);
        let values = single(&set.config_id, value.clone());
        let mut patch = Map::new();
        if set.is_model {
            patch.insert("modelId".into(), value);
            patch.insert("configValues".into(), values);
        } else {
            patch.insert("configValues".into(), values.clone());
            let scoped = updated.iter().any(|option| {
                option.get("id").and_then(Value::as_str) == Some(set.config_id.as_str())
                    && !matches!(category(option), Some(MODEL_CATEGORY | MODE_CATEGORY))
            });
            if let Some(model) = set.model_at_request.filter(|model| !model.is_empty())
                && scoped
                && self.current_model(&set.session_id).as_deref() == Some(model.as_str())
            {
                patch.insert("modelValues".into(), single(&model, values));
            }
        }
        fx.push(Effect::PersistComposerOptions {
            config_id,
            patch: Value::Object(patch),
        });
    }

    /// `writeAgentOptionsCache` with only the options patched: the cached
    /// models and modes stay.
    fn write_options_cache(
        &mut self,
        config_id: &str,
        options: &[Value],
        env: &mut Env<'_>,
        fx: &mut Vec<Effect>,
    ) {
        let prev = self.agent_options_cache.get(config_id);
        let kept = |key: &str| {
            prev.and_then(|prev| prev.get(key))
                .filter(|value| !value.is_null())
                .cloned()
                .unwrap_or(Value::Null)
        };
        let entry = json!({
            "models": kept("models"),
            "modes": kept("modes"),
            "configOptions": options,
            "updatedAt": env.now,
        });
        if entry["models"].is_null() && entry["modes"].is_null() && options.is_empty() {
            self.agent_options_cache.remove(config_id);
            return;
        }
        if let Some(catalog) = model_catalog(&entry, env.now) {
            fx.push(Effect::PersistModelCatalog {
                config_id: config_id.to_owned(),
                catalog,
            });
        }
        self.agent_options_cache.insert(config_id.to_owned(), entry);
    }
}

/// What a `SetConfigOption` command carried, back with its acknowledgement.
struct ConfigOptionSet {
    agent_id: String,
    session_id: String,
    config_id: String,
    value: Value,
    persist: bool,
    model_at_request: Option<String>,
    is_model: bool,
}
