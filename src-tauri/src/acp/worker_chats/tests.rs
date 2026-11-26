use std::collections::HashMap;

use super::{pick_account, AccountChoice, AccountPick, FollowReason, Provider, WorkerChatError};
use crate::acp::antigravity_agent::AntigravityLaunchAuth;
use crate::acp::claude_agent::ClaudeLaunchAuth;
use crate::acp::claude_limit::decide::Candidate;
use crate::acp::codex_agent::CodexLaunchAuth;
use crate::acp::config::AgentConfig;
use crate::acp::manager::AgentLaunch;

fn candidate(id: &str, ready: bool) -> Candidate {
    Candidate {
        account_id: id.to_string(),
        ready,
        percent: Some(10.0),
    }
}

#[test]
fn a_worker_chat_uses_another_account_only_with_the_users_consent() {
    let candidates = [candidate("spare", true), candidate("out", false)];
    let auto = AccountChoice::Auto;
    let spare = AccountChoice::Account("spare".to_string());
    let out = AccountChoice::Account("out".to_string());
    let gone = AccountChoice::Account("gone".to_string());
    let codex = Some(Provider::Codex);
    type Case<'a> = (
        &'a str,
        Option<Provider>,
        bool,
        &'a AccountChoice,
        Option<&'a str>,
        Result<AccountPick, WorkerChatError>,
    );
    let follows = |reason| Ok(AccountPick::FollowsProject(reason));
    let cases: [Case<'_>; 10] = [
        ("unmanaged caller, auto", None, true, &auto, Some("spare"), follows(FollowReason::Unmanaged)),
        ("unmanaged caller, named", None, true, &spare, Some("spare"), follows(FollowReason::Unmanaged)),
        ("switching off, auto", codex, false, &auto, Some("spare"), follows(FollowReason::AutoSwitchOff(Provider::Codex))),
        ("switching off, named", codex, false, &spare, Some("spare"), follows(FollowReason::AutoSwitchOff(Provider::Codex))),
        ("auto takes the roomiest", codex, true, &auto, Some("spare"), Ok(AccountPick::Pinned(Provider::Codex, "spare".to_string()))),
        ("claude caller, auto", Some(Provider::Claude), true, &auto, Some("spare"), Ok(AccountPick::Pinned(Provider::Claude, "spare".to_string()))),
        ("auto with no room", codex, true, &auto, None, follows(FollowReason::NoRoom(Provider::Codex))),
        ("named and ready", codex, true, &spare, None, Ok(AccountPick::Pinned(Provider::Codex, "spare".to_string()))),
        ("named, signed out", codex, true, &out, Some("spare"), Err(WorkerChatError::AccountNotReady(Provider::Codex, "out".to_string()))),
        ("named, unknown", codex, true, &gone, Some("spare"), Err(WorkerChatError::UnknownAccount(Provider::Codex, "gone".to_string()))),
    ];
    for (name, provider, auto_switch, choice, roomiest, expected) in cases {
        assert_eq!(
            pick_account(provider, auto_switch, choice, &candidates, roomiest),
            expected,
            "{name}"
        );
    }
}

fn launch(
    config_id: &str,
    claude: ClaudeLaunchAuth,
    codex: CodexLaunchAuth,
    antigravity: AntigravityLaunchAuth,
) -> AgentLaunch {
    AgentLaunch {
        config: AgentConfig {
            config_id: Some(config_id.to_string()),
            name: "Agent".to_string(),
            command: "agent".to_string(),
            args: Vec::new(),
            env: HashMap::new(),
            allow_terminal: false,
        },
        project_id: Some("proj-1".to_string()),
        claude,
        codex,
        antigravity,
    }
}

#[test]
fn a_caller_is_managed_only_on_its_own_providers_account() {
    let on_account = |config_id| {
        Provider::of(&launch(
            config_id,
            ClaudeLaunchAuth::Account("c1".to_string()),
            CodexLaunchAuth::Account("x1".to_string()),
            AntigravityLaunchAuth::Account("g1".to_string()),
        ))
    };
    let fixed = |config_id| {
        Provider::of(&launch(
            config_id,
            ClaudeLaunchAuth::Fixed,
            CodexLaunchAuth::Fixed,
            AntigravityLaunchAuth::Fixed,
        ))
    };
    let cases = [
        ("acp-registry:claude-acp", on_account("acp-registry:claude-acp"), Some(Provider::Claude)),
        ("acp-registry:codex-acp", on_account("acp-registry:codex-acp"), Some(Provider::Codex)),
        ("acp-registry:antigravity-acp", on_account("acp-registry:antigravity-acp"), Some(Provider::Antigravity)),
        ("another agent", on_account("acp-registry:gemini"), None),
        ("claude on a fixed config dir", fixed("acp-registry:claude-acp"), None),
        ("codex on a fixed home", fixed("acp-registry:codex-acp"), None),
        ("antigravity on a fixed home", fixed("acp-registry:antigravity-acp"), None),
    ];
    for (name, provider, expected) in cases {
        assert_eq!(provider, expected, "{name}");
    }
    let codex_unmanaged_auth = launch(
        "acp-registry:codex-acp",
        ClaudeLaunchAuth::Account("c1".to_string()),
        CodexLaunchAuth::Unmanaged,
        AntigravityLaunchAuth::Account("g1".to_string()),
    );
    assert_eq!(Provider::of(&codex_unmanaged_auth), None);
}

#[test]
fn a_worker_that_follows_its_project_is_told_why_naming_the_provider() {
    let cases = [
        (
            FollowReason::AutoSwitchOff(Provider::Codex),
            "Automatic Codex account switching is off, so the new chat uses the project's account.",
        ),
        (
            FollowReason::NoRoom(Provider::Antigravity),
            "No signed-in Antigravity account has room now, so the new chat uses its project's \
             account.",
        ),
        (
            FollowReason::Unmanaged,
            "This chat does not run on a Claude, Codex or Antigravity account Troupe manages, so \
             the new chat uses its project's agent setup and `account` was ignored.",
        ),
    ];
    for (reason, note) in cases {
        assert_eq!(reason.note(), note, "{reason:?}");
    }
}

#[test]
fn a_named_account_error_names_the_callers_provider() {
    assert_eq!(
        WorkerChatError::UnknownAccount(Provider::Antigravity, "a1".to_string()).message(),
        "Troupe has no Antigravity account with ID a1. Pass \"auto\" to use the account with the \
         most room."
    );
    assert_eq!(
        WorkerChatError::AccountNotReady(Provider::Codex, "x1".to_string()).message(),
        "The Codex account with ID x1 is not signed in. Pass \"auto\" to use the account with \
         the most room, or ask the user to sign in to it."
    );
}

#[test]
fn account_choice_reads_auto_from_nothing_blank_or_auto() {
    for raw in [None, Some(""), Some("  "), Some("auto")] {
        assert_eq!(AccountChoice::parse(raw), AccountChoice::Auto, "{raw:?}");
    }
    assert_eq!(
        AccountChoice::parse(Some(" a1b2 ")),
        AccountChoice::Account("a1b2".to_string())
    );
}
