use serde_json::json;

use crate::ledger::{Carrier, Ledger};
use crate::machine::Config;
use crate::reconnect_tests::config;

fn first_command_id(config: &Config, carrier: Carrier) -> String {
    let mut ledger = Ledger::new(config, carrier);
    assert!(ledger.add(1, (), json!({}), true));
    ledger.get(1).unwrap().command_id.clone().unwrap()
}

fn with_id(client_instance_id: &str) -> Config {
    Config {
        client_instance_id: client_instance_id.into(),
        ..config()
    }
}

#[test]
fn due_calls_keep_the_order_the_app_made_them_in() {
    let mut ledger = Ledger::new(&config(), Carrier::Terminal);
    assert!(ledger.add(20, (), json!("first"), true));
    assert!(ledger.add(10, (), json!("second"), true));

    assert_eq!(ledger.due(0.0), vec![20, 10]);
}

#[test]
fn command_ids_are_safe_for_the_host_and_bounded() {
    let id = first_command_id(&with_id(&"é.".repeat(200)), Carrier::Ws);

    assert!(id.len() <= 128, "{id}");
    assert!(
        id.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-'),
        "{id}"
    );
}

#[test]
fn distinct_clients_never_share_a_command_id() {
    let ids = [
        first_command_id(&with_id("tab.1"), Carrier::Ws),
        first_command_id(&with_id("tab1"), Carrier::Ws),
        first_command_id(&with_id(&format!("{}x", "a".repeat(96))), Carrier::Ws),
        first_command_id(&with_id(&format!("{}y", "a".repeat(96))), Carrier::Ws),
        first_command_id(&config(), Carrier::Ws),
        first_command_id(&config(), Carrier::Terminal),
        first_command_id(
            &Config {
                seed: 8,
                host_local: None,
                ..config()
            },
            Carrier::Ws,
        ),
    ];

    let unique: std::collections::BTreeSet<_> = ids.iter().collect();
    assert_eq!(unique.len(), ids.len(), "{ids:?}");
}
