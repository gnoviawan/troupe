//! The Antigravity limit sink reacts only to Antigravity plan limits.

use serde_json::json;

use super::{AntigravityLimitHandle, Input};
use crate::acp::events::{AcpErrorDetail, EVENT_AGENT_ERROR};
use crate::web::sink::AcpEvent;

fn agent_error(data: serde_json::Value) -> AcpEvent {
    AcpEvent {
        sid: Some("s1".to_string()),
        type_: EVENT_AGENT_ERROR,
        payload: json!({ "agentId": "a1", "sessionId": "s1", "data": data }),
    }
}

#[test]
fn an_antigravity_plan_limit_reaches_the_supervisor_with_its_reset() {
    let (handle, mut inbox) = AntigravityLimitHandle::channel();
    let detail = AcpErrorDetail::antigravity_plan_limit(Some(1_700_000_000_000));
    handle.sink().emit(&agent_error(detail.data.unwrap()));
    match inbox.rx.try_recv() {
        Ok(Input::Limit {
            kind, resets_at, ..
        }) => {
            assert_eq!(kind, "quota_exhausted");
            assert_eq!(resets_at, Some(1_700_000_000_000));
        }
        _ => panic!("expected an Antigravity limit"),
    }
}

#[test]
fn a_codex_or_claude_limit_is_not_an_antigravity_limit() {
    let (handle, mut inbox) = AntigravityLimitHandle::channel();
    let sink = handle.sink();
    sink.emit(&agent_error(json!({ "errorKind": "quota_exhausted" })));
    sink.emit(&agent_error(json!({ "errorKind": "rate_limited" })));
    sink.emit(&agent_error(json!({ "errorKind": "rate_limit" })));
    sink.emit(&agent_error(
        json!({ "codexErrorInfo": "usageLimitExceeded" }),
    ));
    assert!(inbox.rx.try_recv().is_err());
}

#[test]
fn the_plan_limit_error_has_the_renderer_shape() {
    let detail = AcpErrorDetail::antigravity_plan_limit(None);
    assert_eq!(detail.message, "Antigravity reached its usage limit");
    assert_eq!(detail.code, Some(-32603));
    assert_eq!(
        detail.data,
        Some(json!({ "errorKind": "quota_exhausted", "agent": "antigravity", "resetsAt": null }))
    );
    assert!(
        crate::acp::codex_accounts::usage::codex_plan_limit_kind(detail.data.as_ref().unwrap())
            .is_some(),
        "the renderer's plan-limit check reads the same errorKind"
    );
}
