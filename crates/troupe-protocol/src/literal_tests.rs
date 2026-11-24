use serde_json::json;

use crate::{False, True};

literal!(GapTag = str "gap");

#[test]
fn literals_accept_only_their_value() {
    assert_eq!(serde_json::to_value(True).unwrap(), json!(true));
    assert_eq!(serde_json::to_value(GapTag).unwrap(), json!("gap"));
    assert!(serde_json::from_value::<True>(json!(true)).is_ok());
    assert!(serde_json::from_value::<True>(json!(false)).is_err());
    assert!(serde_json::from_value::<False>(json!(false)).is_ok());
    assert!(serde_json::from_value::<GapTag>(json!("heartbeat")).is_err());
}

#[test]
fn literal_schema_is_a_const() {
    let schema = schemars::schema_for!(GapTag);
    assert_eq!(schema.get("const"), Some(&json!("gap")));
}
