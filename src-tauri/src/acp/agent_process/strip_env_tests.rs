//! Unix tests for stripping inherited Codex and Antigravity credentials from a child process.
//!
//! The child is `sh`, so this module is compiled only on Unix.

use agent_client_protocol::schema::v1::EnvVariable;

use super::apply_stdio_env;

fn print_credentials() -> std::process::Command {
    let mut command = std::process::Command::new("sh");
    command.arg("-c").arg(
        "printf '%s|%s|%s' \"$OPENAI_API_KEY\" \"$CODEX_ACCESS_TOKEN\" \"$TERMUL_STRIP_CODEX_API_CREDENTIALS\"",
    );
    command.env("OPENAI_API_KEY", "host-key");
    command.env("CODEX_ACCESS_TOKEN", "host-token");
    command
}

#[test]
fn account_mode_removes_inherited_api_credentials_from_the_child() {
    let mut command = print_credentials();
    apply_stdio_env(
        &mut command,
        &[
            EnvVariable::new(crate::acp::codex_agent::STRIP_CODEX_API_CREDENTIALS, "1"),
            EnvVariable::new("CODEX_HOME", "/tmp/codex-home"),
        ],
    );
    let output = command.output().expect("sh");
    assert!(output.status.success());
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "||");
}

#[test]
fn without_the_marker_an_inherited_api_credential_stays() {
    let mut command = print_credentials();
    apply_stdio_env(
        &mut command,
        &[EnvVariable::new("CODEX_HOME", "/tmp/codex-home")],
    );
    let output = command.output().expect("sh");
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "host-key|host-token|"
    );
}

fn print_google_credentials() -> std::process::Command {
    let mut command = std::process::Command::new("sh");
    command.arg("-c").arg(
        "printf '%s|%s|%s|%s' \"$GEMINI_API_KEY\" \"$GOOGLE_API_KEY\" \"$TERMUL_STRIP_ANTIGRAVITY_API_CREDENTIALS\" \"$OPENAI_API_KEY\"",
    );
    command.env("GEMINI_API_KEY", "host-gemini");
    command.env("GOOGLE_API_KEY", "host-google");
    command.env("OPENAI_API_KEY", "host-key");
    command
}

#[test]
fn antigravity_account_mode_removes_inherited_google_credentials_only() {
    let mut command = print_google_credentials();
    apply_stdio_env(
        &mut command,
        &[
            EnvVariable::new(
                crate::acp::antigravity_agent::STRIP_ANTIGRAVITY_API_CREDENTIALS,
                "1",
            ),
            EnvVariable::new("GEMINI_API_KEY", "project-gemini"),
            EnvVariable::new("GEMINI_HOME", "/tmp/gemini-home"),
        ],
    );
    let output = command.output().expect("sh");
    assert!(output.status.success());
    // The Codex credential is not the Antigravity marker's to remove.
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "|||host-key");
}

#[test]
fn without_the_antigravity_marker_google_credentials_stay() {
    let mut command = print_google_credentials();
    apply_stdio_env(
        &mut command,
        &[EnvVariable::new("GEMINI_HOME", "/tmp/gemini-home")],
    );
    let output = command.output().expect("sh");
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "host-gemini|host-google||host-key"
    );
}
