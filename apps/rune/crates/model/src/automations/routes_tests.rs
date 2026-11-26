use serde_json::json;

use super::*;
use crate::automations::{Schedule, Trigger};

#[test]
fn a_schedule_goes_out_with_integer_milliseconds() {
    // The host's `Schedule::Every` is `{ every_ms: u64, anchor_ms: i64 }`: `3600000.0` is refused.
    let trigger = Trigger::Schedule {
        schedule: Schedule::Every {
            every_ms: 3_600_000,
            anchor_ms: -5,
        },
    };
    let route = Call::ApplyOps {
        id: "a1".into(),
        base_revision: 3,
        ops: vec![Op::SetTrigger { trigger }],
    }
    .route();
    let body = serde_json::to_string(&route.body).expect("body");
    assert!(
        body.contains(r#""every_ms":3600000,"#) || body.contains(r#""every_ms":3600000}"#),
        "{body}"
    );
    assert!(body.contains(r#""anchor_ms":-5"#), "{body}");
    let at = Trigger::Schedule {
        schedule: Schedule::At {
            at: 1_773_046_800_000,
        },
    };
    let text = serde_json::to_string(&json!(Op::SetTrigger { trigger: at })).expect("op");
    assert!(text.contains(r#""at":1773046800000"#), "{text}");
}

#[test]
fn the_fixture_triggers_read_back_as_sent() {
    let f: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../../src/shared/types/__fixtures__/automation.json"
    ))
    .expect("automation.json");
    for trigger in f["triggers"].as_array().expect("triggers") {
        let parsed: Trigger = serde_json::from_value(trigger.clone()).expect("trigger");
        assert_eq!(serde_json::to_value(&parsed).expect("value"), *trigger);
    }
}
