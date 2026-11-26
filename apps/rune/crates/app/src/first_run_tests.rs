use client::http::HttpError;
use model::setup::{AuthGate, Submit};
use rune::{Env, Runtime};
use serde_json::json;

use crate::first_run::{Ask, Gate};
use crate::tests::connected;
use crate::{State, view};

fn web() -> State {
    let mut s = State::default();
    s.first_run.gate = Gate::Checking;
    s.first_run.auth = AuthGate::Checking;
    s
}

fn asked(s: &State) -> Vec<Ask> {
    s.first_run.sent.iter().map(|(a, _)| a.clone()).collect()
}

fn defaults() -> serde_json::Value {
    json!({ "setupRequired": true, "defaults": { "host": "0.0.0.0", "port": 8080, "sessionsDir": "/s", "projectsFile": "/p.json", "allowRemoteWrites": false, "updateChannel": "stable", "updateIntervalSecs": 21600 } })
}

#[test]
fn a_server_in_setup_mode_shows_the_wizard_and_the_answers_go_through_its_dry_run() {
    let mut s = web();
    s.first_run_replied(Ask::Health, Ok(json!({ "status": "ok", "setupRequired": true })));
    assert_eq!(s.first_run.gate, Gate::Setup);
    assert_eq!(asked(&s), [Ask::Status]);
    s.first_run_replied(Ask::Status, Ok(defaults()));
    assert_eq!(s.first_run.wizard.port.text, "8080");
    s.first_run.wizard.port = rune::TextEdit::new("70000");
    s.submit_wizard();
    assert_eq!(s.first_run.wizard.errors.get("port").map(String::as_str), Some("Enter a whole number between 1 and 65535."));
    assert_eq!(asked(&s), [Ask::Status], "nothing is sent with a bad port");
    s.first_run.wizard.port = rune::TextEdit::new("8195");
    s.submit_wizard();
    let (ask, request) = s.first_run.sent.last().cloned().unwrap();
    assert_eq!(ask, Ask::Validate);
    assert_eq!(request.body.as_ref().unwrap()["port"], json!(8195.0));
    s.first_run_replied(Ask::Validate, Ok(json!({ "ok": true, "fieldErrors": {} })));
    assert_eq!(s.first_run.sent.last().unwrap().0, Ask::Apply);
    s.first_run_replied(Ask::Apply, Ok(json!({ "ok": true, "host": "0.0.0.0", "port": 8195, "webAuthToken": "secret" })));
    let applied = s.first_run.wizard.applied.clone().expect("applied");
    assert_eq!((applied.port, applied.token.as_deref()), (8195, Some("secret")));
}

#[test]
fn the_wizard_shows_the_servers_field_errors() {
    let mut s = web();
    s.first_run_replied(Ask::Health, Ok(json!({ "setupRequired": true })));
    s.first_run_replied(Ask::Status, Ok(defaults()));
    s.submit_wizard();
    s.first_run_replied(Ask::Validate, Ok(json!({ "ok": false, "fieldErrors": { "sessionsDir": "Not a folder" } })));
    assert!(!s.first_run.wizard.submitting);
    assert_eq!(s.first_run.wizard.errors.get("sessionsDir").map(String::as_str), Some("Not a folder"));
}

#[test]
fn a_refused_boot_probe_shows_the_token_screen_and_a_wrong_token_says_so() {
    let mut s = web();
    s.first_run_replied(Ask::Health, Ok(json!({ "status": "ok", "allowRemoteWrites": true })));
    assert_eq!((s.first_run.gate, asked(&s).last().cloned()), (Gate::Ok, Some(Ask::Projects { submitted: false })));
    let refused = || Err(HttpError::Refused { status: 401, code: "UNAUTHORIZED".into(), message: "Unauthorized".into() });
    s.first_run_replied(Ask::Projects { submitted: false }, refused());
    assert_eq!(s.first_run.auth, AuthGate::Unauthorized);
    s.submit_token();
    assert_eq!(s.first_run.token_error, Some(Submit::Empty));
    s.first_run.token = rune::TextEdit::new("  wrong ");
    s.submit_token();
    assert_eq!(asked(&s).last(), Some(&Ask::Projects { submitted: true }));
    s.first_run_replied(Ask::Projects { submitted: true }, refused());
    assert_eq!(s.first_run.token_error, Some(Submit::Invalid));
    assert_eq!(s.first_run.auth, AuthGate::Unauthorized);
    s.first_run.token = rune::TextEdit::new("right");
    s.submit_token();
    s.first_run_replied(Ask::Projects { submitted: true }, Ok(json!([])));
    assert_eq!(s.first_run.auth, AuthGate::Ok);
}

#[test]
fn health_retries_then_opens_the_app() {
    let mut s = web();
    for _ in 0..4 {
        s.first_run_replied(Ask::Health, Err(HttpError::Network("down".into())));
        assert_eq!(s.first_run.gate, Gate::Checking);
    }
    s.first_run_replied(Ask::Health, Err(HttpError::Network("down".into())));
    assert_eq!(s.first_run.gate, Gate::Ok);
}

#[test]
fn a_first_run_with_no_projects_shows_onboarding_and_skip_lands_on_the_workspace() {
    let mut s = connected();
    for call in 1..=5 {
        s.apply(crate::tests::read_reply(call, serde_json::Value::Null));
    }
    s.apply(client::ws::Output::WatchState {
        watch: troupe_protocol::ws::WsWatchName::Projects,
        scope: String::new(),
        state: json!({ "projects": [], "defaultProjectId": null }),
    });
    s.first_run_tick(0.0);
    assert!(s.onboarding_shown());
    let mut rt = Runtime::new(s, view, (1280.0, 800.0), Env::new(true, 1.0));
    rt.frame(0.0);
    rt.frame(0.016);
    crate::tests::tap(&mut rt, "onboarding-start");
    crate::tests::tap(&mut rt, "onboarding-later");
    assert!(crate::tests::present(&rt, "onboarding-project"));
    crate::tests::tap(&mut rt, "onboarding-skip");
    assert!(!rt.state.onboarding_shown());
    assert!(rt.state.settings.get("onboardingCompletedAt").as_str().is_some_and(|at| at.ends_with('Z')));
}

#[test]
fn the_token_screen_and_the_wizard_draw_in_place_of_the_shell() {
    let mut s = web();
    s.first_run_replied(Ask::Health, Ok(json!({ "setupRequired": true })));
    s.first_run_replied(Ask::Status, Ok(defaults()));
    let mut rt = Runtime::new(s, view, (1280.0, 800.0), Env::new(true, 1.0));
    rt.frame(0.0);
    assert!(crate::tests::present(&rt, "setup-start"));
    assert!(!crate::tests::present(&rt, "shell-rail"));
    let mut s = web();
    s.first_run.gate = Gate::Ok;
    s.first_run.auth = AuthGate::Unauthorized;
    let mut rt = Runtime::new(s, view, (1280.0, 800.0), Env::new(true, 1.0));
    rt.frame(0.0);
    assert!(crate::tests::present(&rt, "token-continue"));
    assert!(!crate::tests::present(&rt, "shell-rail"));
}
