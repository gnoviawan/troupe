//! A chat's own controls between the model and the host: the set commands the model's picks
//! send (`set_mode`, `set_model`, `set_config_option`), the saved picks a model pick reads
//! first, and the wait for the agent to advertise its options again after a model switch.

use model::acp::{Effect, Event, RunId};
use serde_json::{Value, json};
use troupe_protocol::ws::{WsError, WsRequestType};

use ui::composer::controls::{ControlAct, ModeRow, ModeView, UsageView};

use crate::State;
use crate::composer::Host;
use crate::sync::{Pending, stored_value};

/// `PersistenceKeys.lastComposerOptions`.
fn composer_options_key(config_id: &str) -> String {
    format!("agents/composer-options/{config_id}")
}

impl State {
    /// Runs a control effect; any other effect comes back.
    pub(crate) fn run_control(&mut self, effect: Effect) -> Option<Effect> {
        let (kind, payload) = match &effect {
            Effect::SetMode { agent_id, session_id, mode_id, .. } => {
                (WsRequestType::SetMode, json!({ "agentId": agent_id, "sessionId": session_id, "modeId": mode_id }))
            }
            Effect::SetModel { agent_id, session_id, model_id, .. } => {
                (WsRequestType::SetModel, json!({ "agentId": agent_id, "sessionId": session_id, "modelId": model_id }))
            }
            Effect::SetConfigOption { agent_id, session_id, config_id, value, .. } => (
                WsRequestType::SetConfigOption,
                json!({ "agentId": agent_id, "sessionId": session_id, "configId": config_id, "valueId": value }),
            ),
            Effect::ReadComposerOptions { config_id, .. } => {
                let key = self.device_key(&composer_options_key(config_id));
                self.call(WsRequestType::StoreRead, json!({ "key": key }), Pending::ComposerOptions(Box::new(effect)));
                return None;
            }
            Effect::WaitForReadvertisedOptions { delay_ms, run, .. } => {
                self.chat.readvertise.push((self.now + delay_ms / 1000.0, *run));
                return None;
            }
            _ => return Some(effect),
        };
        log::info!("[chat] {kind:?}");
        self.call(kind, payload, Pending::Control(Box::new(effect)));
        None
    }

    /// The host's answer to a set command.
    pub(crate) fn control_settled(&mut self, effect: Effect, result: Result<Option<Value>, WsError>) {
        match result {
            Ok(reply) => {
                let options = reply.and_then(|r| r.as_array().cloned());
                self.acp_step(Event::ControlSet(effect, options));
            }
            Err(e) => {
                log::warn!("[chat] a control change was refused: {}: {}", e.code, e.message);
                self.acp_step(Event::ControlFailed(effect, e.message, Some(e.code)));
            }
        }
    }

    /// The saved picks a model pick asked for; an unreadable record reads as none.
    pub(crate) fn composer_options_read(&mut self, effect: Effect, result: Result<Option<Value>, WsError>) {
        let saved = match result {
            Ok(reply) => stored_value(reply).unwrap_or(Value::Null),
            Err(e) => {
                log::warn!("[chat] reading the saved composer picks failed: {}", e.code);
                Value::Null
            }
        };
        self.acp_step(Event::ComposerOptionsRead(effect, saved));
    }

    /// Ends the re-advertise waits that came due; returns when the next one is due.
    pub(crate) fn fire_readvertise_waits(&mut self, now: f64) -> (bool, Option<f64>) {
        let (due, waiting): (Vec<(f64, RunId)>, Vec<_>) =
            std::mem::take(&mut self.chat.readvertise).into_iter().partition(|w| w.0 <= now);
        self.chat.readvertise = waiting;
        let fired = !due.is_empty();
        for (_, run) in due {
            self.acp_step(Event::ReadvertiseWaitOver(run));
        }
        (fired, self.chat.readvertise.iter().map(|w| w.0).reduce(f64::min))
    }
}

/// A chat's model choice: the `model`-category option (or the older `models` list), its
/// current value and the choices.
pub(crate) struct ChatModels {
    current: Option<String>,
    choices: Vec<(String, String)>,
}

impl ChatModels {
    pub(crate) fn current_name(&self) -> Option<String> {
        let current = self.current.as_deref()?;
        self.choices.iter().find(|(id, _)| id == current).map(|(_, name)| name.clone())
    }

    pub(crate) fn choices(&self) -> Vec<ui::composer::view::AgentChoice> {
        self.choices
            .iter()
            .map(|(id, name)| ui::composer::view::AgentChoice {
                id: id.clone(),
                name: name.clone(),
                picked: self.current.as_deref() == Some(id.as_str()),
            })
            .collect()
    }
}

impl State {
    /// The models a chat's agent offers, when it offers a choice.
    pub(crate) fn chat_models(&self, session_id: &str) -> Option<ChatModels> {
        let rest = &self.chat.acp.sessions.get(session_id)?.rest;
        let text = |v: &Value, k: &str| v.get(k).and_then(Value::as_str).map(str::to_owned);
        let option = rest
            .get("configOptions")
            .and_then(Value::as_array)
            .and_then(|options| options.iter().find(|o| text(o, "category").as_deref() == Some("model")));
        let models = match option {
            Some(option) => ChatModels {
                current: text(option, "currentValue"),
                choices: option
                    .get("options")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(|o| Some((text(o, "value")?, text(o, "name").unwrap_or_default())))
                    .collect(),
            },
            None => {
                let models = rest.get("models").filter(|m| !m.is_null())?;
                ChatModels {
                    current: text(models, "currentModelId"),
                    choices: models
                        .get("availableModels")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                        .filter_map(|m| Some((text(m, "modelId")?, text(m, "name").unwrap_or_default())))
                        .collect(),
                }
            }
        };
        (!models.choices.is_empty()).then_some(models)
    }

    /// A chat's modes for the mode chip.
    pub(crate) fn mode_view(&self, host: &Host) -> Option<ModeView> {
        let Host::Chat(id) = host else { return None };
        let modes = self.chat.acp.sessions.get(id)?.rest.get("modes").filter(|m| !m.is_null())?;
        let text = |v: &Value, k: &str| v.get(k).and_then(Value::as_str).map(str::to_owned);
        let rows: Vec<ModeRow> = modes
            .get("availableModes")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|m| Some(ModeRow { id: text(m, "id")?, name: text(m, "name").unwrap_or_default(), description: text(m, "description") }))
            .collect();
        Some(ModeView {
            current: text(modes, "currentModeId").unwrap_or_default(),
            modes: rows,
            agent_name: self.shown_agent_name(host),
            open: self.draft(host).is_some_and(|d| d.mode_open),
        })
    }

    /// A chat's context window for the ring (`sessionUsage`), shown once the user wrote.
    pub(crate) fn usage_view(&self, host: &Host) -> Option<UsageView> {
        let Host::Chat(id) = host else { return None };
        let usage = self.chat.acp.session_usage.get(id)?;
        let wrote = self.chat.acp.messages.get(id).is_some_and(|m| m.iter().any(|m| m.role == model::acp::types::Role::User));
        wrote.then(|| UsageView {
            used: usage.used,
            size: usage.size,
            baseline: usage.baseline_used,
            open: self.draft(host).is_some_and(|d| d.usage_open),
        })
    }

    /// A press on the mode chip, a mode, or the ring.
    pub(crate) fn composer_control(&mut self, host: &Host, act: ControlAct) {
        match act {
            ControlAct::ModeMenu(open) => self.draft_mut(host).mode_open = open,
            ControlAct::Usage(open) => self.draft_mut(host).usage_open = open,
            ControlAct::Mode(mode_id) => {
                self.draft_mut(host).mode_open = false;
                if let Host::Chat(session) = host {
                    log::info!("[composer] chat {session} mode: {mode_id}");
                    self.acp_step(Event::SetMode(session.clone(), mode_id));
                }
            }
        }
    }
}
