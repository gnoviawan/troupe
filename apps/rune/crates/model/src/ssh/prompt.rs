//! The two prompts a connect can pause on: a host key to trust, a password or passphrase.

use serde::{Deserialize, Serialize};

/// A host-key question (`SSHHostKeyPrompt`): the `ssh` watch's `host_key` delta.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostKeyPrompt {
    /// The attempt id the question belongs to.
    pub session_id: String,
    /// `unknown` or `changed`.
    pub status: String,
    pub host: String,
    pub port: u16,
    pub key_type: String,
    pub fingerprint: String,
    /// The keys known_hosts recorded for this host before (a changed key).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub previous_fingerprints: Vec<String>,
}

impl HostKeyPrompt {
    #[must_use]
    pub fn changed(&self) -> bool {
        self.status == "changed"
    }

    /// `host`, or `host:port` off 22.
    #[must_use]
    pub fn target(&self) -> String {
        if self.port == 22 { self.host.clone() } else { format!("{}:{}", self.host, self.port) }
    }

    /// The question's title, as React words it.
    #[must_use]
    pub fn title(&self) -> &'static str {
        if self.changed() { "Server identity changed" } else { "Verify this server" }
    }

    /// The explanation under the title.
    #[must_use]
    pub fn body(&self) -> String {
        if self.changed() {
            format!(
                "{} presented a different key than the one you trusted before. This happens when a server is reinstalled — or when someone intercepts the connection.",
                self.target()
            )
        } else {
            format!(
                "First connection to {}. Check that this fingerprint matches the one from the server's administrator, then trust it. Troupe remembers it for next time.",
                self.target()
            )
        }
    }

    /// The label over the presented key's fingerprint.
    #[must_use]
    pub fn fingerprint_label(&self) -> String {
        if self.changed() { format!("New {} key fingerprint", self.key_type) } else { format!("{} key fingerprint", self.key_type) }
    }

    /// The trust button: replacing a changed key says so.
    #[must_use]
    pub fn trust_label(&self) -> &'static str {
        if self.changed() { "Replace key and connect" } else { "Trust and connect" }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SecretKind {
    Password,
    Passphrase,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PromptReason {
    /// None was available.
    Required,
    /// The one tried did not work.
    Rejected,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CredentialPrompt {
    pub kind: SecretKind,
    pub reason: PromptReason,
}

/// `credentialPromptFor`: which prompt, if any, answers a failed connect.
#[must_use]
pub fn credential_prompt_for(code: &str, auth_method: Option<&str>) -> Option<CredentialPrompt> {
    let prompt = |kind, reason| Some(CredentialPrompt { kind, reason });
    match code {
        "SSH_PASSWORD_REQUIRED" => prompt(SecretKind::Password, PromptReason::Required),
        "SSH_PASSPHRASE_REQUIRED" => prompt(SecretKind::Passphrase, PromptReason::Required),
        "SSH_PASSPHRASE_INCORRECT" => prompt(SecretKind::Passphrase, PromptReason::Rejected),
        "SSH_AUTH_FAILED" if auth_method == Some("password") => prompt(SecretKind::Password, PromptReason::Rejected),
        _ => None,
    }
}
