use std::path::{Path, PathBuf};

use agent_client_protocol::schema::v1::{
    PermissionOption, RequestPermissionOutcome, ToolCallUpdate, ToolKind,
};
use serde_json::json;

use super::{ToolAllowlist, ToolPolicies};

fn tool_call(id: &str, kind: Option<&str>, paths: &[&Path]) -> ToolCallUpdate {
    let mut value = json!({ "toolCallId": id });
    if let Some(kind) = kind {
        value["kind"] = json!(kind);
    }
    if !paths.is_empty() {
        value["locations"] = json!(paths
            .iter()
            .map(|p| json!({ "path": p }))
            .collect::<Vec<_>>());
    }
    serde_json::from_value(value).unwrap()
}

fn options(kinds: &[(&str, &str)]) -> Vec<PermissionOption> {
    kinds
        .iter()
        .map(|(id, kind)| {
            serde_json::from_value(json!({ "optionId": id, "name": id, "kind": kind })).unwrap()
        })
        .collect()
}

fn all_options() -> Vec<PermissionOption> {
    options(&[
        ("always", "allow_always"),
        ("once", "allow_once"),
        ("never", "reject_always"),
        ("no", "reject_once"),
    ])
}

struct Fixture {
    dir: tempfile::TempDir,
    policies: ToolPolicies,
}

impl Fixture {
    fn new(kinds: &[ToolKind]) -> Self {
        let mut policies = ToolPolicies::default();
        policies.set("s1".into(), ToolAllowlist::new(kinds.iter().copied()));
        Self {
            dir: tempfile::tempdir().unwrap(),
            policies,
        }
    }

    fn root(&self) -> PathBuf {
        self.dir.path().canonicalize().unwrap()
    }

    /// The chosen option id, or "cancelled"; `None` without an allowlist.
    fn answer(&self, call: &ToolCallUpdate, opts: &[PermissionOption]) -> Option<String> {
        let root = self.root();
        self.policies
            .answer("s1", Some(&root), call, opts)
            .map(|d| match d.outcome {
                RequestPermissionOutcome::Selected(selected) => selected.option_id.0.to_string(),
                _ => "cancelled".to_string(),
            })
    }
}

#[test]
fn a_listed_kind_gets_the_allow_once_option_and_anything_else_reject_once() {
    let f = Fixture::new(&[ToolKind::Fetch, ToolKind::Read]);
    let inside = f.root().join("notes.txt");
    assert_eq!(
        f.answer(&tool_call("t1", Some("fetch"), &[]), &all_options()),
        Some("once".into())
    );
    assert_eq!(
        f.answer(&tool_call("t2", Some("read"), &[&inside]), &all_options()),
        Some("once".into())
    );
    assert_eq!(
        f.answer(&tool_call("t3", Some("execute"), &[]), &all_options()),
        Some("no".into())
    );
}

#[test]
fn a_request_without_a_kind_uses_its_earlier_tool_call_and_an_unknown_one_is_refused() {
    let mut f = Fixture::new(&[ToolKind::Edit, ToolKind::Other]);
    let inside = f.root().join("out.md");
    f.policies
        .note_call("s1", "t1", Some(ToolKind::Edit), Some(vec![inside]));
    assert_eq!(
        f.answer(&tool_call("t1", None, &[]), &all_options()),
        Some("once".into())
    );
    assert_eq!(
        f.answer(&tool_call("t9", None, &[]), &all_options()),
        Some("no".into())
    );
}

#[test]
fn a_file_tool_is_allowed_only_inside_the_working_directory_and_off_protected_paths() {
    let f = Fixture::new(&[ToolKind::Read, ToolKind::Edit]);
    let root = f.root();
    std::fs::create_dir(root.join("docs")).unwrap();
    let answer = |kind: &str, path: PathBuf| {
        f.answer(&tool_call("t", Some(kind), &[&path]), &all_options())
            .unwrap()
    };
    assert_eq!(answer("edit", root.join("docs/new/report.md")), "once");
    assert_eq!(answer("read", root.join("docs/../README.md")), "once");
    assert_eq!(answer("read", PathBuf::from("/etc/passwd")), "no");
    assert_eq!(answer("read", root.join("../outside.txt")), "no");
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink("/etc", root.join("docs/escape")).unwrap();
        assert_eq!(answer("read", root.join("docs/escape/passwd")), "no");
        // The OS follows `link` before `..`, so this names `outside/secret.txt`.
        let outside = tempfile::tempdir().unwrap();
        std::fs::create_dir(outside.path().join("sub")).unwrap();
        std::os::unix::fs::symlink(outside.path().join("sub"), root.join("docs/link")).unwrap();
        assert_eq!(answer("read", root.join("docs/link/../secret.txt")), "no");
        assert_eq!(answer("edit", root.join("docs/link/../new.md")), "no");
        assert_eq!(answer("read", root.join("docs/link/../../x")), "no");
    }
    assert_eq!(answer("read", root.join(".env.local")), "no");
    assert_eq!(answer("edit", root.join(".git/hooks/pre-commit")), "no");
    assert_eq!(
        answer("edit", root.join(".claude/settings.local.json")),
        "no"
    );
    assert_eq!(answer("edit", root.join("sub/.mcp.json")), "no");
    assert_eq!(answer("edit", root.join(".Claude/settings.json")), "no");
    assert_eq!(answer("read", root.join(".ENV")), "no");
    assert_eq!(answer("edit", root.join(".git./config")), "no");
    assert_eq!(answer("edit", root.join(".github/workflows/ci.yml")), "no");
    assert_eq!(answer("read", PathBuf::from("notes.txt")), "no");
    assert_eq!(answer("edit", PathBuf::from("~/.bashrc")), "no");
    assert_eq!(
        f.answer(&tool_call("t", Some("read"), &[]), &all_options()),
        Some("no".into())
    );
}

#[test]
fn a_mode_switch_is_never_allowed() {
    let f = Fixture::new(&[ToolKind::SwitchMode, ToolKind::Other]);
    assert_eq!(
        f.answer(&tool_call("t1", Some("switch_mode"), &[]), &all_options()),
        Some("no".into())
    );
}

#[test]
fn missing_options_fall_back_to_a_refusal_or_a_cancel() {
    let f = Fixture::new(&[ToolKind::Fetch]);
    assert_eq!(
        f.answer(
            &tool_call("t1", Some("fetch"), &[]),
            &options(&[("always", "allow_always"), ("no", "reject_once")])
        ),
        Some("no".into())
    );
    assert_eq!(
        f.answer(
            &tool_call("t2", Some("execute"), &[]),
            &options(&[("once", "allow_once"), ("never", "reject_always")])
        ),
        Some("cancelled".into())
    );
}

fn selected(outcome: Option<RequestPermissionOutcome>) -> Option<String> {
    outcome.map(|outcome| match outcome {
        RequestPermissionOutcome::Selected(selected) => selected.option_id.0.to_string(),
        _ => "cancelled".to_string(),
    })
}

#[test]
fn an_auto_approved_session_gets_allow_once_and_never_allow_always() {
    let mut policies = ToolPolicies::default();
    policies.auto_approve("child-1".to_string());
    let execute = tool_call("t1", Some("execute"), &[]);
    assert_eq!(
        selected(policies.auto_approval(
            "child-1",
            &execute,
            &options(&[("always", "allow_always"), ("once", "allow_once"), ("no", "reject_once")])
        )),
        Some("once".into())
    );
    assert_eq!(
        selected(policies.auto_approval(
            "child-1",
            &execute,
            &options(&[("always", "allow_always"), ("no", "reject_once")])
        )),
        None,
        "a request without allow-once goes to the user"
    );
    assert_eq!(
        selected(policies.auto_approval("chat-1", &execute, &all_options())),
        None,
        "only the approved session"
    );
    policies.forget("child-1");
    assert_eq!(
        selected(policies.auto_approval("child-1", &execute, &all_options())),
        None,
        "a closed session's approval is forgotten"
    );
}

/// Claude asks `ExitPlanMode` as a permission request whose allow options
/// leave plan mode; its kind comes only on the preceding `tool_call`.
#[test]
fn an_auto_approved_session_never_gets_a_mode_switch_or_an_unseen_kind_answered() {
    let mut policies = ToolPolicies::default();
    policies.auto_approve("child-1".to_string());
    let exit_plan = options(&[
        ("acceptEdits", "allow_always"),
        ("default", "allow_once"),
        ("plan", "reject_once"),
    ]);
    assert_eq!(
        selected(policies.auto_approval(
            "child-1",
            &tool_call("t1", Some("switch_mode"), &[]),
            &exit_plan
        )),
        None,
        "a mode switch named on the request goes to the user"
    );
    assert_eq!(
        selected(policies.auto_approval("child-1", &tool_call("t2", None, &[]), &exit_plan)),
        None,
        "a kind the host never saw goes to the user"
    );
    policies.note_auto_call("child-1", "t2", Some(ToolKind::SwitchMode));
    assert_eq!(
        selected(policies.auto_approval("child-1", &tool_call("t2", None, &[]), &exit_plan)),
        None,
        "a mode switch named on the earlier tool call goes to the user"
    );
    policies.note_auto_call("child-1", "t3", Some(ToolKind::Edit));
    assert_eq!(
        selected(policies.auto_approval("child-1", &tool_call("t3", None, &[]), &all_options())),
        Some("once".into()),
        "an earlier tool call's kind answers a request that omits it"
    );
}

#[test]
fn a_session_without_an_allowlist_gets_no_answer() {
    let mut f = Fixture::new(&[ToolKind::Fetch]);
    f.policies.forget("s1");
    assert_eq!(
        f.answer(&tool_call("t1", Some("fetch"), &[]), &all_options()),
        None
    );
}
