//! The Codex limit sink reacts only to Codex plan limits.

use serde_json::json;

use super::{CodexLimitHandle, Input};
use crate::acp::events::EVENT_AGENT_ERROR;
use crate::web::sink::AcpEvent;

fn agent_error(data: serde_json::Value) -> AcpEvent {
    AcpEvent {
        sid: Some("s1".to_string()),
        type_: EVENT_AGENT_ERROR,
        payload: json!({ "agentId": "a1", "sessionId": "s1", "data": data }),
    }
}

#[test]
fn a_codex_plan_limit_reaches_the_supervisor() {
    let (handle, mut inbox) = CodexLimitHandle::channel();
    handle
        .sink()
        .emit(&agent_error(json!({ "errorKind": "rate_limited" })));
    match inbox.rx.try_recv() {
        Ok(Input::Limit { kind, .. }) => assert_eq!(kind, "rate_limited"),
        _ => panic!("expected a Codex limit"),
    }
}

#[test]
fn an_antigravity_plan_limit_is_not_a_codex_limit() {
    let (handle, mut inbox) = CodexLimitHandle::channel();
    handle.sink().emit(&agent_error(json!({
        "errorKind": "quota_exhausted",
        "agent": "antigravity",
        "resetsAt": null
    })));
    assert!(inbox.rx.try_recv().is_err());
}
