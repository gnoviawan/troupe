//! Desktop `#[tauri::command]` wrappers over the Antigravity Account operations of
//! [`AntigravityAgentService`]. Adding, renaming, removing and the Google sign-in
//! spawn are `/ws` requests (`web::ws::antigravity_accounts`). The desktop renderer
//! reaches those over its Tauri Channel carrier, the same way it reaches the
//! Claude Account writes.

use std::sync::Arc;

use tauri::State;

use crate::acp::antigravity_accounts::usage::AntigravityAccountUsage;
use crate::acp::antigravity_accounts::AntigravityAccountsSnapshot;
use crate::acp::{AntigravityAgentService, AntigravitySignInCheck};

#[tauri::command]
pub async fn antigravity_accounts_usage(
    service: State<'_, Arc<AntigravityAgentService>>,
    force: Option<bool>,
) -> Result<Vec<AntigravityAccountUsage>, String> {
    service.accounts_usage(force.unwrap_or(false)).await
}

#[tauri::command]
pub async fn antigravity_accounts_list(
    service: State<'_, Arc<AntigravityAgentService>>,
) -> Result<AntigravityAccountsSnapshot, String> {
    service.accounts_snapshot().await
}

#[tauri::command]
pub async fn antigravity_accounts_set_default(
    service: State<'_, Arc<AntigravityAgentService>>,
    account_id: String,
) -> Result<(), String> {
    let service = Arc::clone(service.inner());
    service
        .set_default_account(&account_id, AntigravitySignInCheck::Required)
        .await
}

/// With `expected_account_id`, the choice applies only while the Project still
/// uses that Account. Returns whether it applied.
#[tauri::command]
pub async fn antigravity_accounts_set_project(
    service: State<'_, Arc<AntigravityAgentService>>,
    project_id: String,
    account_id: Option<String>,
    expected_account_id: Option<String>,
) -> Result<bool, String> {
    let service = Arc::clone(service.inner());
    service
        .set_project_account(
            &project_id,
            account_id.as_deref(),
            expected_account_id.as_deref(),
            AntigravitySignInCheck::Required,
        )
        .await
}

#[tauri::command]
pub fn antigravity_accounts_set_auto_switch(
    service: State<'_, Arc<AntigravityAgentService>>,
    enabled: bool,
) -> Result<(), String> {
    service.set_auto_switch(enabled)
}
