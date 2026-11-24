//! The network leg of the Antigravity usage read. Every request is https only
//! and bounded by one 30-second budget. The refresh token, the access token
//! and the client secret stay in memory; none is logged or written.

use std::path::Path;
use std::time::Duration;

use serde_json::{json, Value};

use crate::acp::antigravity_accounts::usage::{
    code_assist_project, is_invalid_grant, plan_name, quota_endpoint, ProbeVerdict,
    GCP_TOS_ENDPOINT, KEYCHAIN_UNAVAILABLE, USAGE_UNAVAILABLE,
};
use crate::acp::antigravity_agent::login::{keychain_login, token_files};

const PROBE_TIMEOUT: Duration = Duration::from_secs(30);
const DEFAULT_TOKEN_URI: &str = "https://oauth2.googleapis.com/token";
const READ_FAILED: &str = "Antigravity usage could not be read";

/// The refresh half of a stored Google login. Deliberately not `Debug`.
pub(crate) struct OAuthLogin {
    refresh_token: String,
    client_id: Option<String>,
    client_secret: Option<String>,
    token_uri: String,
    project_id: Option<String>,
}

impl OAuthLogin {
    /// The token endpoint this login refreshes against.
    #[must_use]
    pub(crate) fn token_uri(&self) -> &str {
        &self.token_uri
    }

    /// The Cloud project the login file names, when it names one.
    #[must_use]
    pub(crate) fn project_id(&self) -> Option<&str> {
        self.project_id.as_deref()
    }
}

/// What the token files of a home hold.
pub(crate) enum LoginFile {
    Missing,
    Unreadable,
    /// A file exists, but holds no refresh token.
    Invalid,
    Login(OAuthLogin),
}

/// Read the first token file of `home` that exists (consumer, then Gemini
/// Enterprise).
pub(crate) fn read_login(home: &Path) -> LoginFile {
    let mut found = LoginFile::Missing;
    for path in token_files(home) {
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                log::warn!(
                    "[antigravity-accounts] usage read could not open a login file: {:?}",
                    error.kind()
                );
                found = LoginFile::Unreadable;
                continue;
            }
        };
        match parse_login(&bytes) {
            Some(login) => return LoginFile::Login(login),
            None => {
                if matches!(found, LoginFile::Missing) {
                    found = LoginFile::Invalid;
                }
            }
        }
    }
    found
}

fn parse_login(bytes: &[u8]) -> Option<OAuthLogin> {
    let value: Value = serde_json::from_slice(bytes).ok()?;
    let text = |key: &str| {
        value
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(str::to_string)
    };
    Some(OAuthLogin {
        refresh_token: text("refresh_token")?,
        client_id: text("client_id"),
        client_secret: text("client_secret"),
        token_uri: text("token_uri").unwrap_or_else(|| DEFAULT_TOKEN_URI.to_string()),
        project_id: text("project_id"),
    })
}

/// Read the quota buckets of one home.
pub(super) async fn probe_home(home: &Path, builtin: bool) -> ProbeVerdict {
    let login = match read_login(home) {
        LoginFile::Login(login) => login,
        LoginFile::Missing if builtin && keychain_login().await == Some(true) => {
            return ProbeVerdict::Unavailable(KEYCHAIN_UNAVAILABLE);
        }
        LoginFile::Missing | LoginFile::Invalid => return ProbeVerdict::SignedOut,
        LoginFile::Unreadable => {
            return ProbeVerdict::Failed("Could not read the Antigravity login")
        }
    };
    match tokio::time::timeout(PROBE_TIMEOUT, probe_with_login(&login)).await {
        Ok(verdict) => verdict,
        Err(_) => {
            log::warn!("[antigravity-accounts] usage probe timed out");
            ProbeVerdict::Failed("Antigravity usage timed out")
        }
    }
}

async fn probe_with_login(login: &OAuthLogin) -> ProbeVerdict {
    if !is_https(login.token_uri()) {
        log::warn!("[antigravity-accounts] usage probe refused a token endpoint that is not https");
        return ProbeVerdict::Unavailable(USAGE_UNAVAILABLE);
    }
    let client = match reqwest::Client::builder()
        .timeout(PROBE_TIMEOUT)
        .redirect(crate::acp::https_only::redirect_policy())
        .https_only(true)
        .user_agent(concat!("troupe/", env!("CARGO_PKG_VERSION")))
        .build()
    {
        Ok(client) => client,
        Err(error) => {
            log::warn!("[antigravity-accounts] usage probe client failed: {error}");
            return ProbeVerdict::Failed(READ_FAILED);
        }
    };
    let access_token = match refresh(&client, login).await {
        Ok(token) => token,
        Err(verdict) => return verdict,
    };
    let mut metadata = json!({ "metadata": { "ideType": "ANTIGRAVITY" } });
    if let Some(project) = login.project_id() {
        metadata["cloudaicompanionProject"] = json!(project);
    }
    let load = match post_json(
        &client,
        &format!("{GCP_TOS_ENDPOINT}/v1internal:loadCodeAssist"),
        &access_token,
        &metadata,
        "loadCodeAssist",
    )
    .await
    {
        Ok(load) => load,
        Err(verdict) => return verdict,
    };
    let Some(project) =
        code_assist_project(&load).or_else(|| login.project_id().map(str::to_string))
    else {
        log::info!("[antigravity-accounts] usage probe: the account has no Code Assist project");
        return ProbeVerdict::Unavailable(USAGE_UNAVAILABLE);
    };
    let endpoint = quota_endpoint(&load);
    let reply = match post_json(
        &client,
        &format!("{endpoint}/v1internal:fetchAvailableModels"),
        &access_token,
        &json!({ "project": project }),
        "fetchAvailableModels",
    )
    .await
    {
        Ok(reply) => reply,
        Err(verdict) => return verdict,
    };
    ProbeVerdict::Models {
        plan_type: plan_name(&load),
        reply,
    }
}

fn is_https(uri: &str) -> bool {
    url::Url::parse(uri).is_ok_and(|url| url.scheme() == "https")
}

/// Exchange the refresh token for an access token, in memory only.
async fn refresh(client: &reqwest::Client, login: &OAuthLogin) -> Result<String, ProbeVerdict> {
    // The serializer is not `Send`; finish it before the first await.
    let form = {
        let mut form = url::form_urlencoded::Serializer::new(String::new());
        form.append_pair("grant_type", "refresh_token");
        form.append_pair("refresh_token", &login.refresh_token);
        if let Some(id) = &login.client_id {
            form.append_pair("client_id", id);
        }
        if let Some(secret) = &login.client_secret {
            form.append_pair("client_secret", secret);
        }
        form.finish()
    };
    let response = client
        .post(login.token_uri())
        .header(
            reqwest::header::CONTENT_TYPE,
            "application/x-www-form-urlencoded",
        )
        .body(form)
        .send()
        .await
        .map_err(|error| {
            log::warn!(
                "[antigravity-accounts] token refresh failed (timeout={}, connect={})",
                error.is_timeout(),
                error.is_connect()
            );
            ProbeVerdict::Failed(READ_FAILED)
        })?;
    let status = response.status().as_u16();
    let body: Value = response.json().await.unwrap_or(Value::Null);
    if matches!(status, 400 | 401) && is_invalid_grant(&body) {
        log::info!("[antigravity-accounts] token refresh: the login is no longer valid");
        return Err(ProbeVerdict::SignedOut);
    }
    if !(200..300).contains(&status) {
        log::warn!("[antigravity-accounts] token refresh answered {status}");
        return Err(ProbeVerdict::Failed(READ_FAILED));
    }
    body.get("access_token")
        .and_then(Value::as_str)
        .filter(|token| !token.is_empty())
        .map(str::to_string)
        .ok_or_else(|| {
            log::warn!("[antigravity-accounts] token refresh answered without an access token");
            ProbeVerdict::Failed(READ_FAILED)
        })
}

async fn post_json(
    client: &reqwest::Client,
    url: &str,
    access_token: &str,
    body: &Value,
    label: &str,
) -> Result<Value, ProbeVerdict> {
    let response = client
        .post(url)
        .bearer_auth(access_token)
        .json(body)
        .send()
        .await
        .map_err(|error| {
            log::warn!(
                "[antigravity-accounts] {label} failed (timeout={}, connect={})",
                error.is_timeout(),
                error.is_connect()
            );
            ProbeVerdict::Failed(READ_FAILED)
        })?;
    let status = response.status().as_u16();
    if let Some(verdict) = verdict_for_status(status) {
        log::warn!("[antigravity-accounts] {label} answered {status}");
        return Err(verdict);
    }
    response.json::<Value>().await.map_err(|_| {
        log::warn!("[antigravity-accounts] {label} answered a body that is not JSON");
        ProbeVerdict::Unavailable(USAGE_UNAVAILABLE)
    })
}

/// The verdict of a non-success Cloud Code status: `401` is a lost login,
/// another client error is an API this account cannot use, a server error
/// is a failed read. `None` for a success.
#[must_use]
pub(crate) fn verdict_for_status(status: u16) -> Option<ProbeVerdict> {
    match status {
        200..=299 => None,
        401 => Some(ProbeVerdict::SignedOut),
        400..=499 => Some(ProbeVerdict::Unavailable(USAGE_UNAVAILABLE)),
        _ => Some(ProbeVerdict::Failed(READ_FAILED)),
    }
}

#[cfg(test)]
#[path = "http_tests.rs"]
mod tests;
