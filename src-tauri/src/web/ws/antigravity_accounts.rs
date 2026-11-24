//! Antigravity Account WS handlers: list the Accounts and their usage, choose
//! the Default, a Project or a chat Account, turn automatic switching on or
//! off, schedule a resume, add, rename and remove an Account, and start a
//! Google sign-in by spawning the Antigravity agent with that Account's
//! `GEMINI_HOME`. The client then calls `authenticate_agent` with
//! `oauth-personal` or `oauth-business`. No secret is logged.

use super::*;
use crate::acp::config::AgentConfig;
use crate::acp::AntigravitySignInCheck;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SetDefaultAntigravityAccountPayload {
    account_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SetProjectAntigravityAccountPayload {
    project_id: String,
    /// `null` returns the Project to the Default Account.
    #[serde(default)]
    account_id: Option<String>,
    /// Apply only while the Project still uses this Account.
    #[serde(default)]
    expected_account_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SetChatAntigravityAccountPayload {
    project_id: String,
    session_id: String,
    /// `null` makes the chat follow its Project again.
    #[serde(default)]
    account_id: Option<String>,
    /// Apply only while the chat still runs on this Account.
    #[serde(default)]
    expected_account_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SetAntigravityAutoSwitchPayload {
    enabled: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SetAntigravityResumePayload {
    session_id: String,
    /// Epoch milliseconds; `null` cancels the chat's scheduled resume.
    at: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct AddAntigravityAccountPayload {
    name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct AntigravityAccountIdPayload {
    account_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct RenameAntigravityAccountPayload {
    account_id: String,
    name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct StartAntigravityAccountLoginPayload {
    account_id: String,
    config: AgentConfig,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ListAntigravityUsagePayload {
    #[serde(default)]
    force: bool,
}

/// `list_antigravity_usage { force? }` → the usage windows of every Account.
/// Reads are cached 5 minutes; a forced read still reuses one younger than 60 seconds.
pub(super) async fn handle_list_antigravity_usage(
    id: String,
    payload: &Value,
    acp: &Arc<AcpManager>,
) -> WsReply {
    let parsed: ListAntigravityUsagePayload =
        serde_json::from_value(payload.clone()).unwrap_or_default();
    match acp.antigravity_agent().accounts_usage(parsed.force).await {
        Ok(usage) => ok_with_payload(id, &json!({ "usage": usage })),
        Err(error) => {
            warn!(error = %error, "list_antigravity_usage failed");
            WsReply::err(id, WsErrorCode::Unsupported, error)
        }
    }
}

/// `list_antigravity_accounts` → the secret-free Account snapshot.
pub(super) async fn handle_list_antigravity_accounts(id: String, acp: &Arc<AcpManager>) -> WsReply {
    match acp.antigravity_agent().accounts_snapshot().await {
        Ok(snapshot) => ok_with_payload(id, &snapshot),
        Err(error) => {
            warn!(error = %error, "list_antigravity_accounts failed");
            WsReply::err(id, WsErrorCode::Unsupported, error)
        }
    }
}

/// `set_default_antigravity_account { accountId }`.
pub(super) async fn handle_set_default_antigravity_account(
    id: String,
    payload: &Value,
    acp: &Arc<AcpManager>,
) -> WsReply {
    let parsed: SetDefaultAntigravityAccountPayload =
        match serde_json::from_value(payload.clone()) {
            Ok(parsed) => parsed,
            Err(error) => return WsReply::err(
                id,
                WsErrorCode::Unsupported,
                format!(
                    "malformed set_default_antigravity_account payload (want accountId): {error}"
                ),
            ),
        };
    match acp
        .antigravity_agent()
        .set_default_account(&parsed.account_id, AntigravitySignInCheck::Required)
        .await
    {
        Ok(()) => {
            info!("default Antigravity profile set over ws");
            WsReply::ok(id, Some(json!({})))
        }
        Err(error) => choice_refused(id, error),
    }
}

/// `set_project_antigravity_account { projectId, accountId | null, expectedAccountId? }`
/// → `{ applied }`.
pub(super) async fn handle_set_project_antigravity_account(
    id: String,
    payload: &Value,
    acp: &Arc<AcpManager>,
) -> WsReply {
    let parsed: SetProjectAntigravityAccountPayload = match serde_json::from_value(payload.clone()) {
        Ok(parsed) => parsed,
        Err(error) => {
            return WsReply::err(
                id,
                WsErrorCode::Unsupported,
                format!(
                "malformed set_project_antigravity_account payload (want projectId, accountId): {error}"
            ),
            )
        }
    };
    match acp
        .antigravity_agent()
        .set_project_account(
            &parsed.project_id,
            parsed.account_id.as_deref(),
            parsed.expected_account_id.as_deref(),
            AntigravitySignInCheck::Required,
        )
        .await
    {
        Ok(applied) => {
            info!(applied, "project Antigravity profile set over ws");
            WsReply::ok(id, Some(json!({ "applied": applied })))
        }
        Err(error) => choice_refused(id, error),
    }
}

/// `set_chat_antigravity_account { projectId, sessionId, accountId | null,
/// expectedAccountId? }` → `{ applied }`; `applied` is `false` when the chat
/// no longer runs on `expectedAccountId`.
pub(super) async fn handle_set_chat_antigravity_account(
    id: String,
    payload: &Value,
    acp: &Arc<AcpManager>,
) -> WsReply {
    let parsed: SetChatAntigravityAccountPayload = match serde_json::from_value(payload.clone()) {
        Ok(parsed) => parsed,
        Err(error) => {
            return WsReply::err(
                id,
                WsErrorCode::Unsupported,
                format!(
                "malformed set_chat_antigravity_account payload (want projectId, sessionId, accountId): {error}"
            ),
            )
        }
    };
    match acp
        .antigravity_agent()
        .set_chat_account(
            &parsed.project_id,
            &parsed.session_id,
            parsed.account_id.as_deref(),
            parsed.expected_account_id.as_deref(),
            crate::acp::AntigravitySignInCheck::Required,
        )
        .await
    {
        Ok(applied) => {
            info!(
                project = %parsed.project_id,
                session = %crate::logging::redact_session_id(&parsed.session_id),
                account = parsed.account_id.as_deref().unwrap_or("project"),
                applied,
                "chat Antigravity account set over ws"
            );
            WsReply::ok(id, Some(json!({ "applied": applied })))
        }
        Err(error) => choice_refused(id, error),
    }
}

/// `set_antigravity_resume { sessionId, at }` → `{}`: the host continues the chat
/// at `at` after its Antigravity limit resets, or cancels that with `null`.
pub(super) async fn handle_set_antigravity_resume(
    id: String,
    payload: &Value,
    acp: &Arc<AcpManager>,
) -> WsReply {
    let parsed: SetAntigravityResumePayload = match serde_json::from_value(payload.clone()) {
        Ok(parsed) => parsed,
        Err(error) => {
            return WsReply::err(
                id,
                WsErrorCode::Unsupported,
                format!("malformed set_antigravity_resume payload (want sessionId, at): {error}"),
            )
        }
    };
    let Some(limit) = acp.antigravity_limit() else {
        return WsReply::err(
            id,
            WsErrorCode::Unsupported,
            "this host does not continue Antigravity chats at a reset".to_string(),
        );
    };
    let at = match parsed.at {
        None => None,
        Some(ms) => match std::time::UNIX_EPOCH.checked_add(std::time::Duration::from_millis(ms)) {
            Some(at) => Some(at),
            None => {
                return WsReply::err(
                    id,
                    WsErrorCode::Unsupported,
                    "that resume time is not a real clock time".to_string(),
                );
            }
        },
    };
    match limit
        .set_resume(crate::acp::SessionId(parsed.session_id), at)
        .await
    {
        Ok(()) => WsReply::ok(id, Some(json!({}))),
        Err(error) => {
            warn!(error = %error, "set_antigravity_resume failed");
            WsReply::err(id, WsErrorCode::Unsupported, error)
        }
    }
}

/// `set_antigravity_auto_switch { enabled }` → `{}`. On, a chat at its
/// Account's limit continues on the signed-in Account with the most room.
pub(super) async fn handle_set_antigravity_auto_switch(
    id: String,
    payload: &Value,
    acp: &Arc<AcpManager>,
) -> WsReply {
    let parsed: SetAntigravityAutoSwitchPayload = match serde_json::from_value(payload.clone()) {
        Ok(parsed) => parsed,
        Err(error) => {
            return WsReply::err(
                id,
                WsErrorCode::Unsupported,
                format!("malformed set_antigravity_auto_switch payload (want enabled): {error}"),
            )
        }
    };
    match acp.antigravity_agent().set_auto_switch(parsed.enabled) {
        Ok(()) => {
            info!(
                enabled = parsed.enabled,
                "Antigravity automatic switching flag set over ws"
            );
            WsReply::ok(id, Some(json!({})))
        }
        Err(error) => {
            warn!(error = %error, "set_antigravity_auto_switch failed");
            WsReply::err(id, WsErrorCode::Unsupported, error)
        }
    }
}

/// `add_antigravity_account { name }` → the new account, signed out.
pub(super) async fn handle_add_antigravity_account(
    id: String,
    payload: &Value,
    acp: &Arc<AcpManager>,
) -> WsReply {
    let parsed: AddAntigravityAccountPayload = match serde_json::from_value(payload.clone()) {
        Ok(parsed) => parsed,
        Err(error) => {
            return WsReply::err(
                id,
                WsErrorCode::Unsupported,
                format!("malformed add_antigravity_account payload (want name): {error}"),
            )
        }
    };
    match acp.antigravity_agent().add_account(&parsed.name) {
        Ok(created) => {
            info!("Antigravity profile added over ws");
            ok_with_payload(id, &created)
        }
        Err(error) => {
            warn!(error = %error, "add_antigravity_account failed");
            WsReply::err(id, WsErrorCode::Unsupported, error)
        }
    }
}

/// `rename_antigravity_account { accountId, name }` → `{}`.
pub(super) async fn handle_rename_antigravity_account(
    id: String,
    payload: &Value,
    acp: &Arc<AcpManager>,
) -> WsReply {
    let parsed: RenameAntigravityAccountPayload =
        match serde_json::from_value(payload.clone()) {
            Ok(parsed) => parsed,
            Err(error) => return WsReply::err(
                id,
                WsErrorCode::Unsupported,
                format!(
                    "malformed rename_antigravity_account payload (want accountId, name): {error}"
                ),
            ),
        };
    match acp
        .antigravity_agent()
        .rename_account(&parsed.account_id, &parsed.name)
    {
        Ok(()) => {
            info!("Antigravity profile renamed over ws");
            WsReply::ok(id, Some(json!({})))
        }
        Err(error) => {
            warn!(error = %error, "rename_antigravity_account failed");
            WsReply::err(id, WsErrorCode::Unsupported, error)
        }
    }
}

/// `remove_antigravity_account { accountId }` → `{ movedProjectIds, movedChatIds }`.
/// Deletes that home's login files first. The built-in Account is refused.
/// An Account that is already gone answers empty lists.
pub(super) async fn handle_remove_antigravity_account(
    id: String,
    payload: &Value,
    acp: &Arc<AcpManager>,
) -> WsReply {
    let parsed: AntigravityAccountIdPayload = match serde_json::from_value(payload.clone()) {
        Ok(parsed) => parsed,
        Err(error) => {
            return WsReply::err(
                id,
                WsErrorCode::Unsupported,
                format!("malformed remove_antigravity_account payload (want accountId): {error}"),
            )
        }
    };
    let profile_id = parsed.account_id;
    let service = acp.antigravity_agent();
    match service.remove_account(&profile_id).await {
        Ok(moved) => {
            info!(
                projects = moved.moved_project_ids.len(),
                chats = moved.moved_chat_ids.len(),
                "Antigravity profile removed over ws"
            );
            WsReply::ok(
                id,
                Some(json!({
                    "movedProjectIds": moved.moved_project_ids,
                    "movedChatIds": moved.moved_chat_ids,
                })),
            )
        }
        Err(_) if matches!(service.has_account(&profile_id), Ok(false)) => {
            info!("Antigravity profile already removed");
            WsReply::ok(
                id,
                Some(json!({ "movedProjectIds": [], "movedChatIds": [] })),
            )
        }
        Err(error) => {
            warn!(error = %error, "remove_antigravity_account failed");
            WsReply::err(id, WsErrorCode::Unsupported, error)
        }
    }
}

/// `start_antigravity_account_login { accountId, config }` → the spawn outcome of an
/// agent whose `GEMINI_HOME` is that Account. The client authenticates it with
/// `oauth-personal` or `oauth-business`.
pub(super) async fn handle_start_antigravity_account_login(
    id: String,
    payload: &Value,
    acp: &Arc<AcpManager>,
    command_id: Option<String>,
) -> WsReply {
    let mut parsed: StartAntigravityAccountLoginPayload = match serde_json::from_value(payload.clone()) {
        Ok(parsed) => parsed,
        Err(error) => {
            return WsReply::err(
                id,
                WsErrorCode::Unsupported,
                format!(
                    "malformed start_antigravity_account_login payload (want accountId, config): {error}"
                ),
            )
        }
    };
    if let Err(error) = crate::acp::config::validate_spawn_config(&mut parsed.config) {
        return acp_err_to_reply(id, error);
    }
    match acp
        .spawn_antigravity_sign_in(parsed.config, &parsed.account_id, command_id)
        .await
    {
        Ok(outcome) => {
            info!("Antigravity profile sign-in spawn started over ws");
            ok_with_payload(id, &outcome)
        }
        Err(error) => {
            warn!("start_antigravity_account_login failed");
            acp_err_to_reply(id, error)
        }
    }
}

/// A refused Account choice: `antigravity_account_signed_out` when the Account is
/// not signed in (the stored choice is unchanged), else `not_found`.
fn choice_refused(id: String, error: String) -> WsReply {
    match crate::acp::antigravity_accounts::strip_signed_out_tag(&error) {
        Some(message) => {
            info!("Antigravity account choice refused over ws: the account is not signed in");
            WsReply::err(
                id,
                WsErrorCode::AntigravityAccountSignedOut,
                message.to_string(),
            )
        }
        None => WsReply::err(id, WsErrorCode::NotFound, error),
    }
}

#[cfg(test)]
#[path = "antigravity_accounts_tests.rs"]
mod tests;
