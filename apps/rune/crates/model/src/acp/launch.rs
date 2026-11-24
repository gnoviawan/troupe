//! The launch placeholder of `slices/launch.ts`: the chat a launch shows
//! before its agent session exists, the first prompt seeded into it, and its
//! `launchingSessionIds` mark. The prepare, claim and warm-pool parts of the
//! slice are not ported yet (see `docs/rune/acp-model.md`).

use serde_json::{Map, Value, json};

use super::types::{ChatMessage, Object, Role, Session, SessionStatus};
use super::{Env, State};

/// `createLaunchPlaceholder`'s argument.
pub type LaunchPlaceholder = Object;

fn user_message(blocks: Vec<Value>, state: &mut State, env: &mut Env<'_>) -> ChatMessage {
    ChatMessage {
        id: env.new_id("msg"),
        role: Role::User,
        blocks,
        streaming: false,
        timestamp: env.now,
        seq: Some(state.book.next_seq()),
        host_seq: None,
        message_id: None,
        read_only_guard: None,
        rest: Object::from_iter([("sentHere".to_owned(), json!(true))]),
    }
}

/// `a ?? fallback`.
fn or(value: Option<&Value>, fallback: Value) -> Value {
    value
        .filter(|value| !value.is_null())
        .cloned()
        .unwrap_or(fallback)
}

impl State {
    /// `createLaunchPlaceholder`: an initializing chat with the launch's
    /// first prompt, if any, already shown.
    pub(crate) fn create_launch_placeholder(
        &mut self,
        launch: LaunchPlaceholder,
        env: &mut Env<'_>,
    ) {
        let session_id = env.new_id("launch");
        let blocks: Vec<Value> = launch
            .get("initialUserBlocks")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let open_turn_id = (!blocks.is_empty()).then(|| env.new_id("turn"));
        let message = (!blocks.is_empty()).then(|| user_message(blocks, self, env));
        let mut session = Map::new();
        session.insert("id".into(), json!(session_id));
        session.insert("agentId".into(), json!(""));
        for key in ["cwd", "projectId"] {
            if let Some(value) = launch.get(key) {
                session.insert(key.into(), value.clone());
            }
        }
        session.insert("status".into(), json!("initializing"));
        session.insert("title".into(), Value::Null);
        session.insert("activeTurn".into(), json!(message.is_some()));
        session.insert("openTurnId".into(), json!(open_turn_id));
        session.insert("modes".into(), or(launch.get("modes"), Value::Null));
        session.insert("models".into(), or(launch.get("models"), Value::Null));
        session.insert(
            "configOptions".into(),
            or(launch.get("configOptions"), json!([])),
        );
        session.insert("lastError".into(), Value::Null);
        session.insert("createdAt".into(), json!(env.now));
        session.insert("replaying".into(), Value::Null);
        for key in ["worktreePath", "worktreeBranch", "worktreeProgressId"] {
            if let Some(value) = launch.get(key) {
                session.insert(key.into(), value.clone());
            }
        }
        if let Some(config_id) = launch.get("configId") {
            session.insert("launchConfigId".into(), config_id.clone());
        }
        if launch.get("kind") == Some(&json!("quick")) {
            session.insert("kind".into(), json!("quick"));
        }
        let session: Session =
            serde_json::from_value(Value::Object(session)).expect("a placeholder is a session");
        self.sessions.insert(session_id.clone(), session);
        let messages = self.messages.entry(session_id.clone()).or_default();
        if let Some(message) = message {
            *messages = vec![message];
        }
        self.launching_session_ids.insert(session_id, json!(true));
    }

    /// `discardLaunchPlaceholder`: the launch failed or was abandoned.
    pub(crate) fn discard_launch_placeholder(&mut self, session_id: &str) {
        if !self.launching_session_ids.contains_key(session_id)
            && !self.sessions.contains_key(session_id)
        {
            return;
        }
        self.sessions.remove(session_id);
        self.messages.remove(session_id);
        self.launching_session_ids.remove(session_id);
        self.prompt_queues.remove(session_id);
        if self.active_session_id.as_deref() == Some(session_id) {
            self.active_session_id = None;
        }
    }

    /// `seedLaunchUserMessage`: the launcher's first prompt opens the turn,
    /// once; a chat that already shows a user prompt only reopens it.
    pub(crate) fn seed_launch_user_message(
        &mut self,
        session_id: &str,
        blocks: Vec<Value>,
        env: &mut Env<'_>,
    ) {
        if blocks.is_empty() {
            return;
        }
        let Some(current) = self.sessions.get(session_id) else {
            return;
        };
        if current.status == SessionStatus::Closed {
            return;
        }
        let has_prompt = self
            .messages
            .get(session_id)
            .is_some_and(|list| list.iter().any(|message| message.role == Role::User));
        let reopened = current.open_turn_id.clone();
        if !has_prompt {
            let message = user_message(blocks, self, env);
            self.messages
                .entry(session_id.to_owned())
                .or_default()
                .push(message);
        }
        let open_turn_id = match reopened {
            Some(id) if has_prompt => id,
            _ => env.new_id("turn"),
        };
        let session = self.sessions.get_mut(session_id).expect("checked above");
        session.active_turn = true;
        session.open_turn_id = Some(open_turn_id);
        session.last_error = None;
        session.last_error_code = Some(None);
        session.last_error_data = None;
        self.launching_session_ids
            .insert(session_id.to_owned(), json!(true));
    }
}
