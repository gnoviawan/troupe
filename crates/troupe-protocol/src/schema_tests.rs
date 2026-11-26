use std::path::Path;

use serde_json::Value;

use crate::schema::{protocol_schema, SCHEMA_PATH};

/// Set `TROUPE_PROTOCOL_BLESS=1` to rewrite the committed schema. The check
/// compares JSON values, so the file may be reformatted freely.
#[test]
fn committed_schema_matches_the_types() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(SCHEMA_PATH);
    let fresh = protocol_schema();
    if std::env::var_os("TROUPE_PROTOCOL_BLESS").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let text = format!("{}\n", serde_json::to_string_pretty(&fresh).unwrap());
        std::fs::write(&path, text).unwrap();
    }
    let committed: Value = std::fs::read_to_string(&path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or(Value::Null);
    assert!(
        committed == fresh,
        "{SCHEMA_PATH} is stale; run `TROUPE_PROTOCOL_BLESS=1 cargo test --manifest-path crates/troupe-protocol/Cargo.toml` and `node scripts/protocol/gen-ts.mjs`, both from the repository root"
    );
}
