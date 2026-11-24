//! The MCP OAuth sign-in of `slices/mcp.ts`: the flows, the web paste-back,
//! the token checks and the disconnect.

use serde_json::{Value, json};

use super::js::{self, string_of};
use super::mcp::{Flow, McpError, OAUTH_CANCELLED, TokenCheck};
use super::{Effect, State};

const OAUTH_UNKNOWN_STATE: &str = "OAUTH_UNKNOWN_STATE";

impl State {
    /// Abort the server's running OAuth flow; its cancel logs at the end of
    /// the step's own effects.
    pub(super) fn abort_oauth_flow(&mut self, id: &str, fx: &mut Vec<Effect>) {
        if let Some(flow) = self.book.mcp.flows.remove(id) {
            fx.push(Effect::AbortMcpOAuth { url: flow.url });
            self.book.mcp.aborted.push(flow.name);
        }
    }

    pub(super) fn log_aborted_flows(&mut self, fx: &mut Vec<Effect>) {
        for name in std::mem::take(&mut self.book.mcp.aborted) {
            fx.push(oauth_log(format!(
                "OAuth flow cancelled for server '{name}'"
            )));
        }
    }

    /// `connectMcpOAuth`: a newer flow replaces a running one.
    pub(crate) fn connect_mcp_oauth(
        &mut self,
        project_id: String,
        id: String,
        fx: &mut Vec<Effect>,
    ) {
        let Some((name, url)) = self.mcp_oauth_target(&id) else {
            return;
        };
        let deletions = self.mcp_deletions(&id);
        self.abort_oauth_flow(&id, fx);
        self.book.mcp.flow_seq += 1;
        let flow = self.book.mcp.flow_seq;
        let entry = Flow {
            id: flow,
            name: name.clone(),
            url: url.clone(),
        };
        self.book.mcp.flows.insert(id.clone(), entry);
        self.mcp_oauth_connecting.insert(id.clone(), json!(true));
        fx.push(oauth_log(format!("OAuth flow started for server '{name}'")));
        fx.push(Effect::StartMcpOAuth {
            project_id,
            url,
            server_id: id,
            server_name: name,
            flow,
            deletions,
        });
        self.log_aborted_flows(fx);
    }

    fn current_flow(&self, id: &str, flow: u64) -> bool {
        self.book
            .mcp
            .flows
            .get(id)
            .is_some_and(|entry| entry.id == flow)
    }

    /// The web flow knows its sign-in link: the paste-back dialog opens.
    pub(crate) fn on_mcp_oauth_auth_url(
        &mut self,
        start: Effect,
        auth_url: String,
        redirect_uri: String,
    ) {
        let Effect::StartMcpOAuth {
            project_id,
            url,
            server_id,
            server_name,
            flow,
            deletions,
        } = start
        else {
            return;
        };
        if !self.current_flow(&server_id, flow) || self.mcp_deletions(&server_id) != deletions {
            return;
        }
        let entry = json!({
            "projectId": project_id,
            "serverUrl": url,
            "serverName": server_name,
            "authUrl": auth_url,
            "redirectUri": redirect_uri,
        });
        self.mcp_oauth_paste_back.insert(server_id, entry);
    }

    /// The flow settled: it clears its own spinner and dialog. True when the
    /// server was deleted meanwhile.
    fn settle_flow(&mut self, id: &str, deletions: u64) -> bool {
        self.book.mcp.flows.remove(id);
        if self.mcp_deletions(id) != deletions {
            return true;
        }
        self.mcp_oauth_connecting
            .insert(id.to_owned(), json!(false));
        self.mcp_oauth_paste_back.remove(id);
        false
    }

    fn on_mcp_oauth_settled(
        &mut self,
        id: String,
        name: String,
        flow: u64,
        deletions: u64,
        outcome: Result<Value, McpError>,
        fx: &mut Vec<Effect>,
    ) {
        if !self.current_flow(&id, flow) {
            return;
        }
        let deleted = self.settle_flow(&id, deletions);
        match outcome {
            Err(error) if error.code.as_deref() == Some(OAUTH_CANCELLED) => {
                fx.push(oauth_log(format!(
                    "OAuth flow cancelled for server '{name}'"
                )));
            }
            Err(error) => {
                fx.push(Effect::error(
                    "acp-store.connectMcpOAuth",
                    format!("OAuth flow failed for server '{name}' ({})", error.shown()),
                ));
                fx.push(error.rejected());
            }
            Ok(_) if deleted => {}
            Ok(_) => {
                self.mcp_oauth_connected.insert(id.clone(), json!(true));
                fx.push(oauth_log(format!(
                    "OAuth token confirmed for server '{name}'"
                )));
                self.probe_after_in_flight(&id, fx);
            }
        }
    }

    /// `completeMcpOAuthPaste`: hand the pasted callback to the host.
    pub(crate) fn complete_mcp_oauth_paste(
        &mut self,
        id: String,
        callback_url: String,
        fx: &mut Vec<Effect>,
    ) {
        let Some(pending) = self.mcp_oauth_paste_back.get(&id) else {
            return;
        };
        fx.push(Effect::CompleteMcpOAuth {
            project_id: string_of(pending.get("projectId")),
            callback_url,
            server_id: id,
            server_name: string_of(pending.get("serverName")),
            server_url: string_of(pending.get("serverUrl")),
        });
    }

    fn paste_accepted(&mut self, id: &str, name: &str, fx: &mut Vec<Effect>) {
        self.mcp_oauth_paste_back.remove(id);
        fx.push(Effect::info(
            "acp-store.completeMcpOAuthPaste",
            format!("OAuth paste-back accepted for server '{name}'"),
        ));
    }

    /// `cancelMcpOAuth`.
    pub(crate) fn cancel_mcp_oauth(&mut self, id: &str, fx: &mut Vec<Effect>) {
        if !self.book.mcp.flows.contains_key(id) {
            return;
        }
        self.abort_oauth_flow(id, fx);
        self.mcp_oauth_connecting
            .insert(id.to_owned(), json!(false));
        self.mcp_oauth_paste_back.remove(id);
        self.log_aborted_flows(fx);
    }

    /// `checkMcpOAuthStatus`.
    pub(crate) fn check_mcp_oauth_status(
        &mut self,
        project_id: String,
        id: String,
        fx: &mut Vec<Effect>,
    ) {
        let Some((_, url)) = self.mcp_oauth_target(&id) else {
            return;
        };
        let deletions = self.mcp_deletions(&id);
        fx.push(Effect::HasMcpOAuthToken {
            project_id,
            url,
            check: TokenCheck::Status {
                server_id: id,
                deletions,
            },
        });
    }

    /// `disconnectMcpOAuth`.
    pub(crate) fn disconnect_mcp_oauth(
        &mut self,
        project_id: String,
        id: String,
        fx: &mut Vec<Effect>,
    ) {
        let Some((name, url)) = self.mcp_oauth_target(&id) else {
            return;
        };
        let deletions = self.mcp_deletions(&id);
        fx.push(Effect::DisconnectMcpOAuth {
            project_id,
            url,
            server_id: id,
            server_name: name,
            deletions,
        });
    }

    /// The answer to an OAuth command effect.
    pub(super) fn on_mcp_oauth_command(
        &mut self,
        command: Effect,
        outcome: Result<Value, McpError>,
        fx: &mut Vec<Effect>,
    ) {
        match command {
            Effect::StartMcpOAuth {
                server_id,
                server_name,
                flow,
                deletions,
                ..
            } => self.on_mcp_oauth_settled(server_id, server_name, flow, deletions, outcome, fx),
            Effect::CompleteMcpOAuth {
                project_id,
                server_id,
                server_name,
                server_url,
                ..
            } => match outcome {
                Ok(_) => self.paste_accepted(&server_id, &server_name, fx),
                Err(error) => {
                    let code = error.code.clone().unwrap_or_else(|| "UNKNOWN".into());
                    fx.push(Effect::warn(
                        "acp-store.completeMcpOAuthPaste",
                        format!("OAuth paste-back failed for server '{server_name}' ({code})"),
                    ));
                    if code != OAUTH_UNKNOWN_STATE {
                        return fx.push(error.rejected());
                    }
                    fx.push(Effect::HasMcpOAuthToken {
                        project_id,
                        url: server_url,
                        check: TokenCheck::Paste {
                            server_id,
                            server_name,
                            error,
                        },
                    });
                }
            },
            Effect::HasMcpOAuthToken { check, .. } => self.on_mcp_token_checked(check, outcome, fx),
            Effect::DisconnectMcpOAuth {
                server_id,
                server_name,
                deletions,
                ..
            } => match outcome {
                Ok(_) if self.mcp_deletions(&server_id) != deletions => {}
                Ok(_) => {
                    self.mcp_oauth_connected
                        .insert(server_id.clone(), json!(false));
                    fx.push(Effect::error(
                        "acp-store.disconnectMcpOAuth",
                        format!("OAuth disconnect completed for server '{server_name}'"),
                    ));
                    self.probe_after_in_flight(&server_id, fx);
                }
                Err(error) => {
                    fx.push(Effect::error(
                        "acp-store.disconnectMcpOAuth",
                        format!(
                            "OAuth disconnect failed for server '{server_name}' ({})",
                            error.shown()
                        ),
                    ));
                    fx.push(error.rejected());
                }
            },
            _ => {}
        }
    }

    fn on_mcp_token_checked(
        &mut self,
        check: TokenCheck,
        outcome: Result<Value, McpError>,
        fx: &mut Vec<Effect>,
    ) {
        match (check, outcome) {
            (
                TokenCheck::Status {
                    server_id,
                    deletions,
                },
                Ok(has),
            ) => {
                if self.mcp_deletions(&server_id) == deletions {
                    self.mcp_oauth_connected.insert(server_id, has);
                }
            }
            (TokenCheck::Status { .. }, Err(_)) => {}
            (
                TokenCheck::Paste {
                    server_id,
                    server_name,
                    error,
                },
                Ok(stored),
            ) => {
                if js::truthy(&stored) {
                    return self.paste_accepted(&server_id, &server_name, fx);
                }
                // Expired: its token can never arrive.
                self.cancel_mcp_oauth(&server_id, fx);
                fx.push(error.rejected());
            }
            (TokenCheck::Paste { .. }, Err(error)) => fx.push(error.rejected()),
        }
    }
}

fn oauth_log(message: String) -> Effect {
    Effect::info("acp-store.connectMcpOAuth", message)
}
