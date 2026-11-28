//! A session's model, mode and config option controls as the agent reports
//! them, with the guard that keeps a stale echo of a creation default from
//! undoing a pick, and the per-agent options cache the launcher paints from.
//! Port of `_onModeUpdate`, `_onModelsUpdate` and `_onConfigOptionsUpdate` in
//! `slices/transcript.ts`, of `mergeAgentConfigOptions`,
//! `cacheOptionsFromSession` and `writeAgentOptionsCache` in `helpers.ts`, of
//! the model helpers in `components/chat/chat-input-bar-config.ts`, and of
//! `modelCatalogFromOptions` in `model-catalog.ts`.

use serde::Deserialize;
use serde_json::{Value, json};

use super::types::{Object, Session};
use super::{Effect, Env, State};

pub(super) const MODEL_CATEGORY: &str = "model";
/// The `creationEchoLogged` key of the native mode picker.
const MODE_ECHO_KEY: &str = "__mode__";

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModeUpdateEvent {
    pub session_id: String,
    pub current_mode_id: Value,
    #[serde(default)]
    pub available_modes: Option<Vec<Value>>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelsUpdateEvent {
    pub session_id: String,
    pub models: Value,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigOptionsUpdateEvent {
    pub session_id: String,
    pub config_options: Vec<Value>,
}

/// One selectable value of a config option, groups flattened.
pub(super) struct FlatValue<'a> {
    pub(super) value: &'a str,
    name: Option<&'a Value>,
    description: Option<&'a Value>,
}

/// `flattenConfigOptionValues`: an option's values with groups expanded.
pub(super) fn flatten(option: &Value) -> Vec<FlatValue<'_>> {
    let entries = option
        .get("options")
        .and_then(Value::as_array)
        .into_iter()
        .flatten();
    let mut flat = Vec::new();
    for entry in entries {
        let grouped = entry.get("group").is_some_and(super::js::truthy);
        match entry.get("options").and_then(Value::as_array) {
            Some(children) if grouped => flat.extend(children.iter().filter_map(|child| {
                Some(FlatValue {
                    value: child.get("value")?.as_str()?,
                    name: child.get("name"),
                    description: child.get("description"),
                })
            })),
            None => {
                if let Some(value) = entry.get("value").and_then(Value::as_str) {
                    flat.push(FlatValue {
                        value,
                        name: entry.get("name"),
                        description: entry.get("description"),
                    });
                }
            }
            Some(_) => {}
        }
    }
    flat
}

pub(super) fn option_count(option: &Value) -> usize {
    option
        .get("options")
        .and_then(Value::as_array)
        .map_or(0, Vec::len)
}

pub(super) fn category(option: &Value) -> Option<&str> {
    option.get("category").and_then(Value::as_str)
}

/// A boolean value as the stored string, as `String(value)` makes it.
fn comparable(value: Option<&Value>) -> Option<Value> {
    match value {
        Some(Value::Bool(flag)) => Some(json!(flag.to_string())),
        other => other.cloned(),
    }
}

/// `mergeAgentConfigOptions`: a snapshot that re-asserts the creation default
/// (or any model value) keeps the value the session moved to, while that value
/// is still offered. `echoed` collects the options kept over an echo.
pub(super) fn merge_config_options(
    previous: &[Value],
    next: Vec<Value>,
    creation: Option<&Value>,
    opted: Option<&str>,
    echoed: &mut Vec<String>,
) -> Vec<Value> {
    if previous.is_empty() {
        return next;
    }
    next.into_iter()
        .map(|option| {
            let id = option.get("id");
            if opted.is_some() && id.and_then(Value::as_str) == opted {
                return option;
            }
            let Some(prior) = previous.iter().find(|prior| prior.get("id") == id) else {
                return option;
            };
            let prior_value = prior.get("currentValue");
            let incoming = option.get("currentValue");
            if prior_value == incoming {
                return option;
            }
            let prior_comparable = comparable(prior_value);
            let boolean = prior_value.is_some_and(Value::is_boolean)
                || incoming.is_some_and(Value::is_boolean);
            if !boolean
                && !flatten(&option).iter().any(|entry| {
                    prior_comparable.as_ref().and_then(Value::as_str) == Some(entry.value)
                })
            {
                return option;
            }
            let id_text = id.and_then(Value::as_str).unwrap_or_default();
            let creation = creation.and_then(|values| values.get(id_text));
            let default_echo = creation.is_some() && comparable(incoming).as_ref() == creation;
            if category(&option) != Some(MODEL_CATEGORY) && !default_echo {
                return option;
            }
            if default_echo {
                echoed.push(id_text.to_owned());
            }
            let mut kept = option;
            kept["currentValue"] = prior_value.cloned().unwrap_or(Value::Null);
            kept
        })
        .collect()
}

/// `sessionModelSnapshot`: the model the composer shows, and its name. A
/// `model` config option wins over the native `models` state.
pub(super) fn model_snapshot(session: &Session) -> Option<(String, String)> {
    model_snapshot_of(
        config_options(session),
        session
            .rest
            .get("models")
            .filter(|models| !models.is_null()),
    )
}

/// [`model_snapshot`] of any record with `configOptions` and `models`: a
/// session, or a config's options cache entry.
pub(super) fn model_snapshot_of(
    options: &[Value],
    models: Option<&Value>,
) -> Option<(String, String)> {
    let config_model = options.iter().find(|option| {
        category(option) == Some(MODEL_CATEGORY)
            && option
                .get("currentValue")
                .and_then(Value::as_str)
                .is_some_and(|v| !v.is_empty())
            && option_count(option) > 0
    });
    let synthesized;
    let option = match config_model {
        Some(option) => option,
        None => {
            let models = models?;
            let available = models.get("availableModels").and_then(Value::as_array)?;
            if available.is_empty() {
                return None;
            }
            synthesized = json!({
                "currentValue": models.get("currentModelId"),
                "options": available.iter().map(|model| json!({
                    "value": model.get("modelId"),
                    "name": model.get("name"),
                })).collect::<Vec<_>>(),
            });
            &synthesized
        }
    };
    let current = option
        .get("currentValue")?
        .as_str()
        .filter(|v| !v.is_empty())?;
    let named = flatten(option)
        .into_iter()
        .find(|entry| entry.value == current);
    let name = named
        .and_then(|entry| entry.name)
        .and_then(Value::as_str)
        .map(crate::js_trim)
        .filter(|name| !name.is_empty())
        .unwrap_or(current);
    Some((current.to_owned(), name.to_owned()))
}

pub(super) fn config_options(session: &Session) -> &[Value] {
    session
        .rest
        .get("configOptions")
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice)
}

/// `alignSessionModelProjection`: copy the shown model onto `models`.
pub(super) fn align_model_projection(session: &mut Session) {
    let Some((model_id, _)) = model_snapshot(session) else {
        return;
    };
    if let Some(models) = session
        .rest
        .get_mut("models")
        .filter(|models| models.is_object())
        && models.get("currentModelId").and_then(Value::as_str) != Some(model_id.as_str())
    {
        models["currentModelId"] = json!(model_id);
    }
}

/// `catalogModels` of an option: its values without their groups.
fn catalog_models(option: &Value) -> Vec<Value> {
    flatten(option)
        .into_iter()
        .map(|entry| {
            json!({
                "value": entry.value,
                "name": entry.name,
                "description": entry.description.cloned().unwrap_or(Value::Null),
            })
        })
        .collect()
}

/// `modelCatalogFromOptions`: the model and mode lists of a cache entry, kept
/// across restarts; `None` when the entry has neither.
pub(super) fn model_catalog(entry: &Value, now: f64) -> Option<Value> {
    let option = entry["configOptions"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|option| {
            category(option) == Some(MODEL_CATEGORY)
                && option.get("currentValue").is_some_and(Value::is_string)
                && option_count(option) > 0
        });
    let models = entry
        .get("models")
        .filter(|models| models.get("availableModels").and_then(Value::as_array).is_some_and(|m| !m.is_empty()))
        .map(|models| {
            json!({
                "currentModelId": models.get("currentModelId"),
                "availableModels": models["availableModels"].as_array().into_iter().flatten().map(|model| json!({
                    "modelId": model.get("modelId"),
                    "name": model.get("name"),
                    "description": model.get("description").cloned().unwrap_or(Value::Null),
                })).collect::<Vec<_>>(),
            })
        });
    let model_option = option.map(|option| {
        json!({
            "id": option.get("id"),
            "name": option.get("name"),
            "currentValue": option.get("currentValue"),
            "options": catalog_models(option),
        })
    });
    let modes = entry
        .get("modes")
        .filter(|modes| modes.get("availableModes").and_then(Value::as_array).is_some_and(|m| !m.is_empty()))
        .map(|modes| {
            json!({
                "currentModeId": modes.get("currentModeId"),
                "availableModes": modes["availableModes"].as_array().into_iter().flatten().map(|mode| json!({
                    "id": mode.get("id"),
                    "name": mode.get("name"),
                    "description": mode.get("description").cloned().unwrap_or(Value::Null),
                })).collect::<Vec<_>>(),
            })
        });
    if model_option.is_none() && models.is_none() && modes.is_none() {
        return None;
    }
    Some(json!({ "models": models, "modelOption": model_option, "modes": modes, "updatedAt": now }))
}

impl State {
    pub(crate) fn on_mode_update(
        &mut self,
        event: ModeUpdateEvent,
        env: &mut Env<'_>,
        fx: &mut Vec<Effect>,
    ) {
        let session_id = event.session_id.as_str();
        if let Some(session) = self.sessions.get_mut(session_id) {
            let modes = session.rest.get("modes").filter(|modes| !modes.is_null());
            let available = match event.available_modes {
                Some(modes) if !modes.is_empty() => modes,
                _ => modes
                    .and_then(|modes| modes.get("availableModes"))
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default(),
            };
            let moved_off = modes.and_then(|modes| modes.get("currentModeId")).cloned();
            let creation_default = session
                .rest
                .get("creationOptionDefaults")
                .and_then(|defaults| defaults.get("modeId"));
            let mut current = event.current_mode_id.clone();
            let mut preserved = None;
            if let (Some(default), Some(moved)) = (creation_default, &moved_off)
                && *moved != event.current_mode_id
                && event.current_mode_id == *default
                && available.iter().any(|mode| mode.get("id") == Some(moved))
            {
                current = moved.clone();
                let logged = session
                    .rest
                    .get("creationEchoLogged")
                    .and_then(|logged| logged.get(MODE_ECHO_KEY))
                    .is_some_and(super::js::truthy);
                if !logged {
                    preserved = Some(moved.clone());
                }
            }
            session.rest.insert(
                "modes".into(),
                json!({ "currentModeId": current, "availableModes": available }),
            );
            if let Some(kept) = preserved {
                mark_echo_logged(session, MODE_ECHO_KEY);
                fx.push(Effect::warn(
                    "acp.modeEchoPreserved",
                    format!(
                        "mode_update for session {session_id} re-asserted creation default '{}'; kept current mode '{}'",
                        super::js::string_of(Some(&event.current_mode_id)),
                        super::js::string_of(Some(&kept)),
                    ),
                ));
            }
        }
        self.cache_options_from_session(session_id, env, fx);
    }

    /// `_onModelsUpdate`: a legacy-only session's model switch carries the
    /// whole list, which replaces the session's copy. The agent accepted the
    /// switch, so only the options cache is refreshed (the heal would switch
    /// the chat back to an older saved model), and the accepted model is
    /// saved, so a later heal reads it instead of the older pick.
    pub(crate) fn on_models_update(
        &mut self,
        event: ModelsUpdateEvent,
        env: &mut Env<'_>,
        fx: &mut Vec<Effect>,
    ) {
        let session_id = event.session_id.as_str();
        let model_id = event.models.get("currentModelId").cloned();
        if let Some(session) = self.sessions.get_mut(session_id) {
            session.rest.insert("models".into(), event.models);
            align_model_projection(session);
            self.set_index_model(session_id);
        }
        if let (Some(config_id), Some(model_id)) =
            (self.cache_session_options(session_id, env, fx), model_id)
        {
            fx.push(Effect::PersistComposerOptions {
                config_id,
                patch: json!({ "modelId": model_id }),
            });
        }
    }

    pub(crate) fn on_config_options_update(
        &mut self,
        event: ConfigOptionsUpdateEvent,
        env: &mut Env<'_>,
        fx: &mut Vec<Effect>,
    ) {
        let session_id = event.session_id.as_str();
        if let Some(session) = self.sessions.get_mut(session_id) {
            let logged = session.rest.get("creationEchoLogged").cloned();
            let creation = session
                .rest
                .get("creationOptionDefaults")
                .and_then(|defaults| defaults.get("configValues"))
                .cloned();
            let mut echoed = Vec::new();
            let merged = merge_config_options(
                config_options(session),
                event.config_options,
                creation.as_ref(),
                None,
                &mut echoed,
            );
            echoed.retain(|id| {
                !logged
                    .as_ref()
                    .and_then(|logged| logged.get(id))
                    .is_some_and(super::js::truthy)
            });
            session
                .rest
                .insert("configOptions".into(), Value::Array(merged));
            for id in &echoed {
                mark_echo_logged(session, id);
            }
            align_model_projection(session);
            self.set_index_model(session_id);
            for id in echoed {
                fx.push(Effect::warn(
                    "acp.configOptionEchoPreserved",
                    format!(
                        "config_option_update for session {session_id} re-asserted the creation default on '{id}'; kept the session's current value"
                    ),
                ));
            }
        }
        self.cache_options_from_session(session_id, env, fx);
    }

    /// `withSessionModelOnIndex`: the index row shows the session's model.
    pub(super) fn set_index_model(&mut self, session_id: &str) {
        let Some((model_id, model_name)) = self.sessions.get(session_id).and_then(model_snapshot)
        else {
            return;
        };
        for entry in &mut self.session_index {
            let current = entry.get("id").and_then(Value::as_str) == Some(session_id);
            if current
                && (entry.get("modelId").and_then(Value::as_str) != Some(&model_id)
                    || entry.get("modelName").and_then(Value::as_str) != Some(&model_name))
            {
                entry.insert("modelId".into(), json!(model_id));
                entry.insert("modelName".into(), json!(model_name));
            }
        }
    }

    /// `configIdForAgentId`: the config whose live agent this is, from the
    /// first reuse key that maps to it.
    pub(crate) fn config_id_for_agent(&self, agent_id: &str) -> Option<String> {
        self.config_to_live_agent
            .iter()
            .find(|(_, live)| live.as_str() == Some(agent_id))
            .map(|(key, _)| key.split('\0').next().unwrap_or(key).to_owned())
    }

    /// `cacheOptionsFromSession`: remember the session's options for its
    /// agent's config, save the model list, and heal the user's saved picks.
    pub(crate) fn cache_options_from_session(
        &mut self,
        session_id: &str,
        env: &mut Env<'_>,
        fx: &mut Vec<Effect>,
    ) {
        if let Some(config_id) = self.cache_session_options(session_id, env, fx) {
            self.heal_composer_options(session_id, config_id, fx);
        }
    }

    /// `writeOptionsCacheFromSession`: remember the session's options for its
    /// agent's config and save the model list, without the heal. Returns the
    /// config id.
    pub(crate) fn cache_session_options(
        &mut self,
        session_id: &str,
        env: &mut Env<'_>,
        fx: &mut Vec<Effect>,
    ) -> Option<String> {
        let session = self.sessions.get(session_id)?;
        let config_id = self.config_id_for_agent(&session.agent_id)?;
        let present = |key: &str| {
            session
                .rest
                .get(key)
                .filter(|v| !v.is_null())
                .cloned()
                .unwrap_or(Value::Null)
        };
        let options = session
            .rest
            .get("configOptions")
            .cloned()
            .unwrap_or_else(|| json!([]));
        let entry = json!({
            "models": present("models"),
            "modes": present("modes"),
            "configOptions": options,
            "updatedAt": env.now,
        });
        let has_content = !entry["models"].is_null()
            || !entry["modes"].is_null()
            || entry["configOptions"]
                .as_array()
                .is_some_and(|options| !options.is_empty());
        if has_content {
            if let Some(catalog) = model_catalog(&entry, env.now) {
                fx.push(Effect::PersistModelCatalog {
                    config_id: config_id.clone(),
                    catalog,
                });
            }
            self.agent_options_cache.insert(config_id.clone(), entry);
        } else {
            self.agent_options_cache.remove(&config_id);
        }
        Some(config_id)
    }
}

pub(super) fn mark_echo_logged(session: &mut Session, key: &str) {
    let logged = session
        .rest
        .entry("creationEchoLogged")
        .or_insert_with(|| Value::Object(Object::new()));
    if !logged.is_object() {
        *logged = Value::Object(Object::new());
    }
    logged[key] = json!(true);
}
