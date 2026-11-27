//! The host's persisted chats as the rail and the phone drawer list them: the
//! `list_persisted_sessions` reply, newest first, read again on every
//! `chat_history` watch frame.

use serde::Deserialize;

/// One persisted chat (`PersistedSessionSummary`, the fields the lists show).
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Chat {
    pub session_id: String,
    #[serde(default)]
    pub project_id: Option<String>,
    /// A chat nobody has prompted yet comes with `"title": null`.
    #[serde(default, deserialize_with = "null_as_empty")]
    pub title: String,
    /// Epoch milliseconds.
    #[serde(default)]
    pub last_activity_at: f64,
    /// The folder the chat's agent runs in.
    #[serde(default)]
    pub cwd: Option<String>,
    /// Epoch milliseconds.
    #[serde(default)]
    pub created_at: f64,
    /// `quick` for a quick chat.
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub discovered: bool,
    #[serde(default)]
    pub model_name: Option<String>,
    #[serde(default)]
    pub model_id: Option<String>,
    #[serde(default)]
    pub worktree_path: Option<String>,
    #[serde(default)]
    pub worktree_branch: Option<String>,
}

fn null_as_empty<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    Option::<String>::deserialize(d).map(Option::unwrap_or_default)
}

impl Chat {
    /// A chat with no project is a quick chat.
    #[must_use]
    pub fn is_quick(&self) -> bool {
        self.project_id.as_deref().is_none_or(str::is_empty)
    }

    /// The title a row shows; an untitled chat reads "New chat".
    #[must_use]
    pub fn label(&self) -> &str {
        if self.title.trim().is_empty() {
            "New chat"
        } else {
            &self.title
        }
    }
}

/// The chats this client knows, newest first.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Chats {
    pub list: Vec<Chat>,
    pub is_loaded: bool,
}

impl Chats {
    /// Replaces the list with a reply, ordered newest first (ties by id, so a
    /// reload paints the same order).
    pub fn replace(&mut self, mut list: Vec<Chat>) {
        list.sort_by(|a, b| {
            b.last_activity_at
                .total_cmp(&a.last_activity_at)
                .then_with(|| a.session_id.cmp(&b.session_id))
        });
        self.list = list;
        self.is_loaded = true;
    }

    /// The project's chats, newest first.
    pub fn of_project<'a>(&'a self, project_id: &'a str) -> impl Iterator<Item = &'a Chat> + 'a {
        self.list
            .iter()
            .filter(move |c| c.project_id.as_deref() == Some(project_id))
    }

    pub fn quick(&self) -> impl Iterator<Item = &Chat> {
        self.list.iter().filter(|c| c.is_quick())
    }

    pub fn get(&self, session_id: &str) -> Option<&Chat> {
        self.list.iter().find(|c| c.session_id == session_id)
    }

    /// Drops a chat the host deleted, before the next list lands.
    pub fn remove(&mut self, session_id: &str) {
        self.list.retain(|c| c.session_id != session_id);
    }
}

#[cfg(test)]
#[path = "chats_tests.rs"]
mod tests;
