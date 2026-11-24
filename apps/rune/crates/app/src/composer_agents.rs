//! The composer's agent: the one a chip names (`useComposerAgent`), the launcher's pick
//! (`agents/last-selected`), and whether it takes pasted images (`imageCapable`).

use rune::{ClipboardContent, Tx};
use serde_json::Value;
use ui::composer::Attachment;

use crate::State;
use crate::composer::{AgentConfig, Host};
use crate::sync::Pending;

/// `MAX_IMAGE_BYTES`: the largest image sent inline.
pub const MAX_IMAGE_BYTES: usize = 10 * 1024 * 1024;
/// `useComposerAttachments`' toast for an image the agent cannot take: the web client has no
/// temp file to link it by path, and neither has the Rune client.
pub const NO_IMAGES: &str = "This agent can't accept images in the browser";

impl State {
    /// The agent a composer's chip names; a chat with a model choice names its model
    /// (`AgentModelSelector`'s pill).
    pub fn agent_label(&self, host: &Host) -> Option<String> {
        if let Host::Chat(id) = host
            && let Some(model) = self.chat_models(id).and_then(|m| m.current_name())
        {
            return Some(model);
        }
        self.shown_agent(host).map(|a| a.name.clone())
    }

    /// The name of the agent a composer runs.
    pub(crate) fn shown_agent_name(&self, host: &Host) -> Option<String> {
        self.shown_agent(host).map(|a| a.name.clone())
    }

    /// A chat's own agent config (`useCurrentAgentConfigId`), else none; the launcher's pick,
    /// else the first config.
    fn shown_agent(&self, host: &Host) -> Option<&AgentConfig> {
        let agents = &self.composer.agents;
        let by_id = |id: &str| agents.iter().find(|a| a.id == id);
        match host {
            Host::Chat(id) => self.chat_config_id(id).as_deref().and_then(by_id),
            Host::Launcher(_) => self.composer.picked.as_deref().and_then(by_id).or_else(|| agents.first()),
        }
    }

    /// `useCurrentAgentConfigId`: the live reuse key that runs the chat's agent, then the
    /// config its history names (`stableAgentNamespace` `config:<id>`).
    fn chat_config_id(&self, session_id: &str) -> Option<String> {
        let acp = &self.chat.acp;
        let agent = acp.sessions.get(session_id).map(|s| s.agent_id.as_str()).filter(|a| !a.is_empty());
        let live = agent.and_then(|agent| {
            acp.config_to_live_agent
                .iter()
                .find(|(_, id)| id.as_str() == Some(agent))
                .and_then(|(key, _)| key.split('\0').next().map(str::to_owned))
        });
        live.or_else(|| {
            self.composer
                .namespaces
                .get(session_id)
                .and_then(|ns| ns.strip_prefix("config:"))
                .map(str::to_owned)
        })
    }

    /// `promptCapabilities.image` of the agent a composer sends to, when that agent is known:
    /// a chat's running agent. The launcher's agent starts on Send, so it is checked then.
    pub(crate) fn image_capable(&self, host: &Host) -> Option<bool> {
        let Host::Chat(id) = host else { return None };
        let agent = &self.chat.acp.sessions.get(id)?.agent_id;
        let capabilities = self.chat.acp.agents.get(agent)?.get("capabilities")?;
        Some(capabilities.pointer("/promptCapabilities/image").and_then(Value::as_bool) == Some(true))
    }

    /// A paste with an image becomes an image attachment (`addFiles`): refused over
    /// [`MAX_IMAGE_BYTES`] or when the agent takes no images. Text pastes as usual.
    pub(crate) fn composer_paste(&mut self, host: &Host, content: &ClipboardContent) -> bool {
        let Some(image) = content.image() else {
            return false;
        };
        let bytes = image.png();
        if self.composer_locked(host) {
            return true;
        }
        if bytes.len() > MAX_IMAGE_BYTES {
            log::warn!("[composer] pasted image refused: {} bytes", bytes.len());
            self.toast_error("File too large", "");
            return true;
        }
        if self.image_capable(host) == Some(false) {
            log::warn!("[composer] pasted image refused: the agent takes no images");
            self.toast_error(NO_IMAGES, "");
            return true;
        }
        let draft = self.draft_mut(host);
        let n = draft.attachments.len() + 1;
        log::info!("[composer] pasted an image ({} bytes) into {}", bytes.len(), host.key());
        draft.attachments.push(Attachment::Image {
            id: format!("paste-{n}"),
            name: "pasted-image.png".into(),
            mime_type: "image/png".into(),
            base64: ui::composer::base64(bytes),
        });
        true
    }

    /// The launcher's composer while its chat starts (`composerLocked`): nothing writes into
    /// the draft the launch is sending.
    pub(crate) fn composer_locked(&self, host: &Host) -> bool {
        matches!(host, Host::Launcher(_)) && self.composer.launching.is_some()
    }

    /// The launcher's agent menu: `rememberLastSelectedAgent`, then back to the field. A
    /// chat's menu lists its models: a pick goes through the model's `pickModel`.
    pub fn composer_pick_agent(&mut self, host: &Host, id: &str, tx: &mut Tx) {
        self.draft_mut(host).agent_open = false;
        if let Host::Chat(session) = host {
            log::info!("[composer] chat {session} model: {id}");
            self.acp_step(model::acp::Event::PickModel(session.clone(), id.to_owned(), None));
            return;
        }
        if !self.composer.agents.iter().any(|a| a.id == id) {
            return;
        }
        log::info!("[composer] launcher agent: {id}");
        self.composer.picked = Some(id.to_owned());
        let key = "agents/last-selected";
        self.write(key, serde_json::json!({ "agentId": id, "mode": "acp" }), Pending::Write(key.to_owned()));
        tx.focus(format!("{}-field", host.key()), false);
    }

    /// What the launcher's agent menu lists; a chat's menu lists its models.
    pub fn agent_choices(&self, host: &Host) -> Vec<ui::composer::view::AgentChoice> {
        if let Host::Chat(id) = host {
            return self.chat_models(id).map(|m| m.choices()).unwrap_or_default();
        }
        let shown = self.shown_agent(host).map(|a| a.id.as_str());
        self.composer
            .agents
            .iter()
            .map(|a| ui::composer::view::AgentChoice {
                id: a.id.clone(),
                name: a.name.clone(),
                picked: shown == Some(a.id.as_str()),
            })
            .collect()
    }

    /// The agent configs (`acp/agents`) as read.
    pub(crate) fn agents_loaded(&mut self, data: Option<Value>) {
        self.composer.agents = data
            .and_then(|v| v.as_array().cloned())
            .unwrap_or_default()
            .into_iter()
            .filter_map(|raw| {
                let id = raw.get("id").and_then(Value::as_str)?.to_owned();
                let name = raw.get("name").and_then(Value::as_str).unwrap_or(&id).to_owned();
                Some(AgentConfig { id, name, raw })
            })
            .collect();
        log::info!("[composer] {} agent configs", self.composer.agents.len());
    }

    /// `readLastSelectedAgentId`: the saved pick counts only when its mode is `acp`, and never
    /// over an agent the user picked while the read was out.
    pub(crate) fn last_agent_loaded(&mut self, data: Option<Value>) {
        self.composer.picked_read = true;
        if self.composer.picked.is_some() {
            return;
        }
        self.composer.picked = data
            .filter(|v| v.get("mode").and_then(Value::as_str) == Some("acp"))
            .and_then(|v| v.get("agentId").and_then(Value::as_str).map(str::to_owned));
    }
}
