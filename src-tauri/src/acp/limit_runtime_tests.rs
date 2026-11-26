use std::collections::HashMap;
use std::time::{Duration, SystemTime};

use super::roomiest_account;
use crate::acp::claude_limit::decide::Candidate;

fn candidate(id: &str, ready: bool, percent: f64) -> Candidate {
    Candidate {
        account_id: id.to_string(),
        ready,
        percent: Some(percent),
    }
}

#[test]
fn the_roomiest_account_skips_one_whose_limit_has_not_reset() {
    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000);
    let candidates = [
        candidate("signed-out", false, 0.0),
        candidate("just-limited", true, 5.0),
        candidate("spare", true, 40.0),
        candidate("full", true, 100.0),
    ];
    let limited = HashMap::from([("just-limited".to_string(), now + Duration::from_secs(60))]);
    assert_eq!(
        roomiest_account(&candidates, &limited, now),
        Some("spare".to_string())
    );
    assert_eq!(
        roomiest_account(&candidates, &limited, now + Duration::from_secs(60)),
        Some("just-limited".to_string())
    );
    assert_eq!(
        roomiest_account(&candidates[3..], &HashMap::new(), now),
        None
    );
}
