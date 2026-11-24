//! Moving one chat to a new agent. The other chats of its old agent stay
//! where they are.

use super::supervisor::{scrub, Supervisor};
use super::{AccountRef, AntigravityLimitAction};
use crate::acp::config::{AgentId, SessionId};
use crate::acp::manager::HostMove;
use crate::logging::redact_session_id;

pub(super) use crate::acp::limit_runtime::Relaunch;

const LABEL: &str = "antigravity-limit";

impl Supervisor {
    pub(super) async fn move_chat(
        &mut self,
        session_id: &SessionId,
        project_id: &str,
        old_agent: &AgentId,
        from: AccountRef,
        to: AccountRef,
        notice: String,
    ) {
        let chat = redact_session_id(&session_id.0);
        let moving = self.acp.begin_host_move(session_id).await;
        match self.relaunch(&moving, session_id, Some(old_agent)).await {
            Ok(Relaunch::Done(agent_id)) => {
                self.emit(
                    session_id,
                    project_id,
                    AntigravityLimitAction::Switched {
                        from,
                        to,
                        from_agent_id: old_agent.0.clone(),
                        new_agent_id: agent_id.0.clone(),
                    },
                );
                self.send_notice(&agent_id, session_id, &notice).await;
            }
            Ok(Relaunch::HeldElsewhere(holder)) => {
                log::info!(
                    "[antigravity-limit] chat {chat} is open on agent {holder} now; left there"
                )
            }
            Err(reason) => {
                log::warn!(
                    "[antigravity-limit] chat {chat} did not continue on the new account: {}",
                    scrub(&reason, session_id)
                );
                self.emit(
                    session_id,
                    project_id,
                    AntigravityLimitAction::Failed { reason },
                );
            }
        }
        drop(moving);
    }

    pub(super) async fn relaunch(
        &self,
        moving: &HostMove,
        session_id: &SessionId,
        old_agent: Option<&AgentId>,
    ) -> Result<Relaunch, String> {
        crate::acp::limit_runtime::relaunch(&self.acp, moving, session_id, old_agent, LABEL).await
    }

    pub(super) async fn send_notice(
        &self,
        agent_id: &AgentId,
        session_id: &SessionId,
        notice: &str,
    ) {
        crate::acp::limit_runtime::send_notice(
            &self.acp,
            &self.relay,
            agent_id,
            session_id,
            notice,
            LABEL,
        )
        .await
    }
}
