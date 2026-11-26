use std::collections::BTreeMap;

use serde_json::json;

use super::*;

fn fields() -> Vec<Field> {
    serde_json::from_value(json!([
        { "id": "f1", "label": "API key", "target": { "kind": "env", "name": "API_KEY" }, "isRequired": true, "isSecret": true, "format": "string", "choices": null, "default": null, "placeholder": null, "description": null },
        { "id": "f2", "label": "Verbose", "target": { "kind": "argument", "index": 0, "name": null }, "isRequired": false, "isSecret": false, "format": "boolean", "choices": null, "default": " TRUE ", "placeholder": null, "description": null },
        { "id": "f3", "label": "Region", "target": { "kind": "header", "name": "X-Region" }, "isRequired": true, "isSecret": false, "format": "string", "choices": ["eu", "us"], "default": "eu", "placeholder": null, "description": "Where" }
    ]))
    .unwrap()
}

#[test]
fn the_form_starts_from_the_defaults_and_keeps_secrets_out_of_the_plan() {
    let f = fields();
    let values = initial_values(&f);
    assert_eq!(values, BTreeMap::from([("f1".into(), String::new()), ("f2".into(), "true".into()), ("f3".into(), "eu".into())]));
    let (inputs, secrets) = split_values(&f, &values);
    assert_eq!(serde_json::Value::Object(inputs), json!({ "f2": "true", "f3": "eu" }));
    assert_eq!(serde_json::Value::Object(secrets), json!({ "f1": "" }));
    assert_eq!(missing(&f, &values), BTreeMap::from([("f1".into(), "Fill this in to continue.".into())]));
    assert_eq!([f[0].control(), f[1].control(), f[2].control()], [Control::Secret, Control::Boolean, Control::Choice]);
}

#[test]
fn failures_and_probes_read_as_react_words_them() {
    assert!(!Failure::Host("MCP_CATALOG_NOT_FOUND".into()).can_start_over());
    assert!(Failure::Host("MCP_PLAN_EXPIRED".into()).can_start_over());
    assert_eq!(Failure::Host("MCP_PLAN_LIMIT".into()).text(), "Too many installs are waiting for approval. Close the others, then start over.");
    assert_eq!(Failure::Host("weird".into()).text(), "Troupe couldn't install this server. Try again.");
    assert_eq!(Probe::read(Some("connected"), 1, None, true).text(), "It responds and offers 1 tool.");
    assert_eq!(Probe::read(Some("authRequired"), 0, None, false), Probe::SignInRequired);
    assert_eq!(Probe::read(Some("authRequired"), 0, Some("401"), true).text(), "Troupe couldn't reach it yet: 401");
    assert_eq!(Probe::read(None, 0, None, true).text(), "Troupe couldn't reach it yet. Check it in the MCP servers list.");
}

#[test]
fn the_consent_review_words() {
    assert_eq!(namespace_label(&json!({ "kind": "github", "account": "octo" })), "GitHub account octo");
    assert_eq!(namespace_label(&json!({ "kind": "domain", "domain": "example.com" })), "Domain example.com");
    assert_eq!(snapshot_age("now"), "just now");
    assert_eq!(snapshot_age("5m"), "5m ago");
    assert_eq!(snapshot_age("3w"), "3w ago");
    assert_eq!(snapshot_age("Nov 2, 2026"), "Nov 2, 2026");
    assert!(is_danger("mounts-sensitive-path") && !is_danger("runs-local-code"));
    assert_eq!(warning_text("sends-data-remote"), "This sends your data to a remote server.");
}

#[test]
fn a_target_takes_its_title_from_the_name_when_the_row_has_none() {
    let row: crate::settings::mcp_catalog::Row = serde_json::from_value(json!({ "name": "io.github.x/fs", "title": null, "state": "installable", "version": "1.0.0", "defaultOption": { "kind": "package", "index": 0 } })).unwrap();
    let target = Target::of(&row).unwrap();
    assert_eq!((target.title.as_str(), target.choice.clone()), ("fs", json!({ "kind": "package", "index": 0 })));
    let filling = Filling::new(target, json!({ "source": { "kind": "catalog", "name": "io.github.x/fs", "version": "1.0.0" }, "choice": null, "title": "fs", "fields": [] })).unwrap();
    assert_eq!(filling.plan_request(), json!({ "source": { "kind": "catalog", "name": "io.github.x/fs", "version": "1.0.0" }, "choice": { "kind": "package", "index": 0 }, "inputs": {} }));
}
