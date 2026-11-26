use client::http::HttpError;
use client::ws::{Command, Output};
use model::settings::mcp::KEY;
use model::settings::mcp_install::Flow;
use rune::{Runtime, TextEdit};
use serde_json::{Value, json};
use troupe_protocol::ws::{WsError, WsRequestType};

use super::{INSTALL, InstallFetch};
use crate::State;
use crate::mcp_catalog::CatalogFetch;
use crate::tests::{present, runtime_at, tap};

fn edit(rt: &mut Runtime<State>, f: impl FnOnce(&mut State)) {
    rt.update_if(move |s, _| {
        f(s);
        true
    });
    rt.frame(0.016);
    rt.frame(0.032);
}

fn registry_calls(rt: &Runtime<State>) -> Vec<(u64, WsRequestType, Value)> {
    rt.state
        .sync
        .sent
        .iter()
        .filter_map(|c| match c {
            Command::Call { call, kind, payload, .. } if payload["key"] == KEY => Some((*call, *kind, payload.clone())),
            _ => None,
        })
        .collect()
}

fn store_reply(rt: &mut Runtime<State>, call: u64, result: Result<Value, WsError>) {
    edit(rt, move |s| s.apply(Output::Reply { call, result: result.map(Some) }));
}

fn posted(rt: &Runtime<State>, path: &str) -> Vec<Value> {
    rt.state.mcp_ui.posted.iter().filter(|(p, _)| p == path).map(|(_, b)| b.clone()).collect()
}

fn answer(rt: &mut Runtime<State>, tag: InstallFetch, result: Result<Value, HttpError>) {
    edit(rt, move |s| s.catalog_fetched(CatalogFetch::Install(tag), result));
}

fn generation(rt: &Runtime<State>) -> u64 {
    rt.state.mcp_ui.install.generation
}

fn form() -> Value {
    json!({
        "source": { "kind": "catalog", "name": "io.github.acme/notes", "version": "1.2.0" },
        "choice": { "kind": "package", "index": 0 },
        "title": "notes",
        "fields": [
            { "id": "key", "label": "API key", "target": { "kind": "env", "name": "NOTES_KEY" }, "variable": null, "isRequired": true, "isSecret": true, "format": "string", "choices": null, "default": null, "placeholder": null, "description": "From your account page" },
            { "id": "dir", "label": "Folder", "target": { "kind": "argument", "index": 1, "name": null }, "variable": null, "isRequired": false, "isSecret": false, "format": "string", "choices": null, "default": "/tmp/notes", "placeholder": null, "description": null }
        ]
    })
}

fn plan() -> Value {
    json!({
        "planId": "plan-1234567890",
        "name": "notes",
        "source": { "kind": "catalog", "name": "io.github.acme/notes", "version": "1.2.0" },
        "choice": { "kind": "package", "index": 0 },
        "expiresAt": "2026-11-26T12:00:00Z",
        "config": { "transport": "stdio", "command": "npx", "args": ["-y", "@acme/notes", "/tmp/notes"], "env": [{ "name": "NOTES_KEY", "isSecret": true }] },
        "form": [],
        "consentView": {
            "transport": "stdio", "command": "npx", "argv": ["-y", "@acme/notes", "/tmp/notes"], "url": null,
            "env": [{ "name": "NOTES_KEY", "value": null, "isSecret": true, "origin": "user" }],
            "headers": [], "hostMounts": [], "warnings": ["runs-local-code"],
            "provenance": { "namespace": { "kind": "github", "account": "acme" }, "repositoryUrl": "https://github.com/acme/notes", "version": "1.2.0", "mutableTag": false, "snapshotFetchedAt": null }
        },
        "launchDigest": "sha256:abc"
    })
}

fn committed() -> Value {
    json!({
        "name": "notes",
        "config": { "transport": "stdio", "command": "npx", "args": ["-y", "@acme/notes", "/tmp/notes"], "env": [{ "name": "NOTES_KEY", "isSecret": true }] },
        "secrets": [{ "slot": "env", "name": "NOTES_KEY", "value": "s3cret" }],
        "provenance": { "name": "io.github.acme/notes", "version": "1.2.0", "kind": "package", "index": 0, "repositoryId": null },
        "launchDigest": "sha256:abc"
    })
}

/// The MCP page with an empty registry and one catalog card, Install pressed.
fn describing() -> Runtime<State> {
    let mut rt = runtime_at((1280.0, 800.0));
    edit(&mut rt, |s| s.open_settings(Some(crate::settings_mcp::PAGE)));
    let (call, _, _) = registry_calls(&rt).into_iter().next().expect("the page reads the registry");
    store_reply(&mut rt, call, Ok(json!({ "value": { "_version": 1, "data": [] }, "revision": 4 })));
    let g = rt.state.mcp_ui.catalog.search_generation();
    let row = json!({ "name": "io.github.acme/notes", "title": null, "description": "Notes", "version": "1.2.0", "status": "active", "state": "installable", "reason": null, "defaultOption": { "kind": "package", "index": 0 }, "repositoryUrl": null, "hasIcon": false });
    let page = json!({ "items": [row], "nextCursor": null, "total": 1, "updatedAt": "2026-11-26T10:00:00Z", "sample": false });
    edit(&mut rt, move |s| s.catalog_fetched(CatalogFetch::Search { generation: g, query: String::new(), append: false }, Ok(page)));
    tap(&mut rt, "install");
    rt
}

fn at_consent() -> Runtime<State> {
    let mut rt = describing();
    let g = generation(&rt);
    answer(&mut rt, InstallFetch::Describe(g), Ok(json!({ "outcome": "form", "form": form() })));
    edit(&mut rt, |s| {
        s.mcp_ui.install.edits.insert("key".into(), TextEdit::new("s3cret"));
    });
    tap(&mut rt, "review");
    answer(&mut rt, InstallFetch::Plan(g), Ok(json!({ "outcome": "plan", "plan": plan() })));
    rt
}

#[test]
fn install_describes_the_row_then_the_form_asks_for_what_is_required() {
    let mut rt = describing();
    assert_eq!(posted(&rt, "/mcp-catalog/install/form"), [json!({ "source": { "kind": "catalog", "name": "io.github.acme/notes", "version": "1.2.0" }, "choice": { "kind": "package", "index": 0 } })]);
    assert!(present(&rt, "describing"));
    let g = generation(&rt);
    answer(&mut rt, InstallFetch::Describe(g), Ok(json!({ "outcome": "form", "form": form() })));
    assert!(present(&rt, "field-key") && present(&rt, "field-dir"));
    tap(&mut rt, "review");
    assert!(matches!(&rt.state.mcp_ui.install.flow, Flow::Form { field_errors, .. } if field_errors.get("key").map(String::as_str) == Some("Fill this in to continue.")));
    assert!(posted(&rt, "/mcp-catalog/install/plan").is_empty(), "a blank required field plans nothing");
}

#[test]
fn the_plan_gets_the_plain_answers_and_only_the_commit_gets_the_secret() {
    let mut rt = at_consent();
    assert_eq!(
        posted(&rt, "/mcp-catalog/install/plan"),
        [json!({ "source": { "kind": "catalog", "name": "io.github.acme/notes", "version": "1.2.0" }, "choice": { "kind": "package", "index": 0 }, "inputs": { "dir": "/tmp/notes" }, "clientInstanceId": "" })]
    );
    for key in ["consent-review", "command", "arg-0", "arg-2", "env", "warnings-caution", "source", "digest", "approve"] {
        assert!(present(&rt, key), "the consent shows {key}");
    }
    assert!(posted(&rt, "/mcp-catalog/install/commit").is_empty(), "nothing is committed before Approve");
    tap(&mut rt, "approve");
    assert_eq!(posted(&rt, "/mcp-catalog/install/commit"), [json!({ "planId": "plan-1234567890", "launchDigest": "sha256:abc", "secretValues": { "key": "s3cret" }, "clientInstanceId": "" })]);
    assert!(rt.state.install_busy());
    edit(&mut rt, |s| {
        s.close_top();
    });
    assert!(rt.state.overlays.is_open(INSTALL), "Escape leaves a commit in flight up");
}

#[test]
fn an_approved_install_saves_the_registry_entry_react_writes_then_probes_it() {
    let mut rt = at_consent();
    tap(&mut rt, "approve");
    answer(&mut rt, InstallFetch::Commit, Ok(committed()));
    let (call, _, payload) = registry_calls(&rt).into_iter().rfind(|(_, k, _)| *k == WsRequestType::StoreWrite).expect("the install is saved");
    let saved = payload["value"]["data"][0].clone();
    let id = saved["id"].as_str().unwrap().to_owned();
    assert_eq!(
        saved,
        json!({ "id": id, "type": "stdio", "name": "notes", "command": "npx", "args": ["-y", "@acme/notes", "/tmp/notes"], "env": [{ "name": "NOTES_KEY", "value": "s3cret" }], "enabled": true,
                "catalog": { "source": "mcp-registry", "name": "io.github.acme/notes", "version": "1.2.0", "kind": "package", "index": 0, "repositoryId": null, "launchDigest": "sha256:abc" } })
    );
    store_reply(&mut rt, call, Ok(json!({ "revision": 5 })));
    assert!(matches!(rt.state.mcp_ui.install.flow, Flow::Probing { .. }), "{:?}", rt.state.mcp_ui.install.flow);
    let server = rt.state.chat.acp.mcp_servers[0].clone();
    let effect = model::acp::Effect::ProbeMcpServer { config: Default::default(), server, deletions: rt.state.chat.acp.mcp_deletions(&id) };
    edit(&mut rt, move |s| s.mcp_fetched(effect, Ok(json!({ "status": "connected", "tools": [{ "name": "a" }, { "name": "b" }] }))));
    assert!(matches!(&rt.state.mcp_ui.install.flow, Flow::Done { probe, .. } if probe.text() == "It responds and offers 2 tools."));
    tap(&mut rt, "done");
    assert!(!rt.state.overlays.is_open(INSTALL));
}

#[test]
fn cancel_at_consent_cancels_the_plan_on_the_host() {
    let mut rt = at_consent();
    tap(&mut rt, "cancel");
    assert_eq!(posted(&rt, "/mcp-catalog/install/cancel"), [json!({ "planId": "plan-1234567890", "clientInstanceId": "" })]);
    assert_eq!(rt.state.mcp_ui.install.flow, Flow::Idle);
    assert!(!rt.state.overlays.is_open(INSTALL));
}

#[test]
fn a_failed_install_says_why_and_offers_start_over_only_when_it_can_help() {
    let mut rt = describing();
    let g = generation(&rt);
    let gone = HttpError::Refused { status: 404, code: "MCP_CATALOG_NOT_FOUND".into(), message: "gone".into() };
    answer(&mut rt, InstallFetch::Describe(g), Err(gone));
    assert!(present(&rt, "failure") && !present(&rt, "start-over"));
    tap(&mut rt, "close");
    let mut rt = at_consent();
    tap(&mut rt, "approve");
    let expired = HttpError::Refused { status: 409, code: "MCP_PLAN_EXPIRED".into(), message: "late".into() };
    answer(&mut rt, InstallFetch::Commit, Err(expired));
    assert_eq!(posted(&rt, "/mcp-catalog/install/cancel").len(), 1, "a refused commit cancels its plan");
    assert!(present(&rt, "start-over"));
    tap(&mut rt, "start-over");
    assert_eq!(posted(&rt, "/mcp-catalog/install/form").len(), 2);
}

#[test]
fn a_plan_that_answers_after_cancel_is_cancelled_too() {
    let mut rt = describing();
    let g = generation(&rt);
    answer(&mut rt, InstallFetch::Describe(g), Ok(json!({ "outcome": "form", "form": form() })));
    edit(&mut rt, |s| {
        s.mcp_ui.install.edits.insert("key".into(), TextEdit::new("k"));
    });
    tap(&mut rt, "review");
    tap(&mut rt, "cancel");
    answer(&mut rt, InstallFetch::Plan(g), Ok(json!({ "outcome": "plan", "plan": plan() })));
    assert_eq!(posted(&rt, "/mcp-catalog/install/cancel"), [json!({ "planId": "plan-1234567890", "clientInstanceId": "" })]);
    assert_eq!(rt.state.mcp_ui.install.flow, Flow::Idle);
}
