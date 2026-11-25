//! A chat's in-chat agent switch (`useAgentSwitch`, `useSwitchGate`, the selector's busy
//! confirm). Picking another agent in the chat's menu arms the switch through the model
//! (`armAgentSwitch`); the model runs it on the next send. A busy chat holds the pick until
//! the user waits or cancels the turn, and cancel-then-switch arms once the turn cleared.

use model::acp::Event;
use rune::Tx;
use serde_json::{Value, json};
use ui::composer::agent_menu::{SwitchAct, SwitchView};
use ui::composer::view::AgentChoice;
use ui::overlay::ToastKind;

use crate::State;
use crate::composer::Host;

/// `TURN_CLEAR_TIMEOUT_MS`: how long cancel-then-switch waits for the turn to end.
const TURN_CLEAR_TIMEOUT_S: f64 = 5.0;

impl State {
    /// The config a chat's armed switch targets (`session.switching.toConfigId`).
    pub(crate) fn armed_config(&self, session_id: &str) -> Option<String> {
        let switching = self.chat.acp.sessions.get(session_id)?.rest.get("switching")?;
        switching.get("toConfigId").and_then(Value::as_str).map(str::to_owned)
    }

    fn config_name(&self, config_id: &str) -> String {
        self.composer
            .agents
            .iter()
            .find(|a| a.id == config_id)
            .map_or_else(|| config_id.to_owned(), |a| a.name.clone())
    }

    /// The switch a chat's menu shows: every agent, the armed target, a held busy pick.
    pub(crate) fn switch_view(&self, host: &Host) -> Option<SwitchView> {
        let Host::Chat(id) = host else { return None };
        if self.composer.agents.is_empty() || !self.chat.loaded.contains(id) {
            return None;
        }
        let current = self.chat_config_id(id);
        let armed = self.armed_config(id);
        let shown = armed.clone().or(current.clone());
        let agents = self
            .composer
            .agents
            .iter()
            .map(|a| AgentChoice { id: a.id.clone(), name: a.name.clone(), picked: shown.as_deref() == Some(a.id.as_str()) })
            .collect();
        let busy = self.draft(host).and_then(|d| d.switch_confirm.as_ref()).map(|_| {
            let name = current.as_deref().map_or_else(|| "The agent".to_owned(), |c| self.config_name(c));
            (name, self.chat.acp.switch_gate(id).turn_busy)
        });
        Some(SwitchView { agents, armed: armed.map(|c| self.config_name(&c)), busy })
    }

    /// The agent chip opened or closed its menu; closing drops a held busy pick.
    pub(crate) fn composer_agent_menu(&mut self, host: &Host, open: bool) {
        let draft = self.draft_mut(host);
        draft.agent_open = open;
        if !open {
            draft.switch_confirm = None;
        }
    }

    /// A press in a chat's agent menu or on its cancel button.
    pub(crate) fn composer_switch(&mut self, host: &Host, act: SwitchAct, tx: &mut Tx) {
        let Host::Chat(session) = host else { return };
        let session = session.clone();
        match act {
            SwitchAct::Agent(config) => {
                let armed = self.armed_config(&session);
                if armed.as_deref() == Some(config.as_str()) {
                    return self.close_agent_menu(host, tx);
                }
                if self.chat_config_id(&session).as_deref() == Some(config.as_str()) {
                    self.drop_pending_arm(&session);
                    if armed.is_some() {
                        log::info!("[composer] chat {session} keeps its agent");
                        self.acp_step(Event::CancelAgentSwitch(session));
                    }
                    return self.close_agent_menu(host, tx);
                }
                if self.chat.acp.switch_gate(&session).blocked {
                    log::info!("[composer] chat {session} is busy; holding the switch to {config}");
                    self.draft_mut(host).switch_confirm = Some(config);
                    return;
                }
                self.close_agent_menu(host, tx);
                self.arm_switch(&session, &config);
            }
            SwitchAct::Cancel => {
                log::info!("[composer] chat {session} cancels its agent switch");
                self.drop_pending_arm(&session);
                self.acp_step(Event::CancelAgentSwitch(session));
            }
            SwitchAct::Wait => {
                self.drop_pending_arm(&session);
                self.close_agent_menu(host, tx);
            }
            SwitchAct::OpenSheet => self.open_model_sheet(host),
            SwitchAct::CancelTurnAndSwitch => {
                let Some(config) = self.draft_mut(host).switch_confirm.take() else { return };
                self.close_agent_menu(host, tx);
                log::info!("[composer] chat {session} stops its turn to switch to {config}");
                self.acp_step(Event::CancelPrompt(session.clone()));
                self.chat.pending_arm.insert(session, (config, self.now + TURN_CLEAR_TIMEOUT_S));
                self.arm_when_turn_cleared();
            }
        }
    }

    fn close_agent_menu(&mut self, host: &Host, tx: &mut Tx) {
        self.composer_agent_menu(host, false);
        tx.focus(format!("{}-field", host.key()), false);
    }

    fn arm_switch(&mut self, session: &str, config: &str) {
        log::info!("[composer] chat {session} arms a switch to {config}");
        self.acp_step(Event::ArmAgentSwitch(session.to_owned(), config.to_owned(), 0));
    }

    /// Cancel-then-switch (`waitForTurnClear`, then arm): the arm goes once the cancelled turn
    /// is over; the model's own gate still refuses a chat something else keeps busy.
    pub(crate) fn arm_when_turn_cleared(&mut self) {
        let ready: Vec<String> =
            self.chat.pending_arm.keys().filter(|session| !self.chat.acp.switch_gate(session).turn_running).cloned().collect();
        for session in ready {
            if let Some((config, _)) = self.chat.pending_arm.remove(&session) {
                self.arm_switch(&session, &config);
            }
        }
    }

    fn drop_pending_arm(&mut self, session: &str) {
        if let Some((config, _)) = self.chat.pending_arm.remove(session) {
            log::info!("[composer] chat {session} drops its held switch to {config}");
        }
    }

    /// Gives up the cancel-then-switches whose turn did not end in time (`waitForTurnClear`
    /// rejects); returns whether any did, and when the next one is due.
    pub(crate) fn expire_pending_arms(&mut self, now: f64) -> (bool, Option<f64>) {
        let expired: Vec<String> =
            self.chat.pending_arm.iter().filter(|(_, (_, due))| *due <= now).map(|(session, _)| session.clone()).collect();
        for session in &expired {
            let Some((config, _)) = self.chat.pending_arm.remove(session) else { continue };
            log::warn!("[composer] cancel-then-switch to {config} failed for {session}: the turn did not end in time");
            self.toasts.push(now, ToastKind::Error, "Could not switch the agent: Error: timed out waiting for turn to clear", None);
        }
        (!expired.is_empty(), self.chat.pending_arm.values().map(|(_, due)| *due).reduce(f64::min))
    }

    /// A model picked while a switch is armed goes with the switch (`armModelPick`): the
    /// target's model, and its config option when the list came from one.
    pub(crate) fn pick_switch_model(&mut self, session: &str, model_id: &str) -> bool {
        let Some(target) = self.armed_config(session) else { return false };
        let from_option = self
            .chat
            .acp
            .switch_target_session(session)
            .and_then(|s| s.rest.get("configOptions"))
            .and_then(Value::as_array)
            .and_then(|options| options.iter().find(|o| o.get("category").and_then(Value::as_str) == Some("model")))
            .and_then(|o| o.get("id").and_then(Value::as_str).map(str::to_owned));
        let mut picks = json!({ "modelId": model_id });
        if let Some(option) = from_option {
            picks["configValues"] = json!({ option: model_id });
        }
        log::info!("[composer] chat {session} picks {model_id} for its switch to {target}");
        match serde_json::from_value(picks) {
            Ok(picks) => self.acp_step(Event::SetSwitchPendingOption(session.to_owned(), picks)),
            Err(e) => log::warn!("[composer] unreadable switch pick: {e}"),
        }
        true
    }
}
