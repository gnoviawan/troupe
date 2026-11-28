//! User-facing copy for SSH states (`lib/ssh-messages.ts`). Every failure says what happened
//! and what to do next; the host's detail shows separately.

use super::{Profile, SessionEnd};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Message {
    pub title: String,
    pub body: String,
    /// The fix is in the host settings: offer "Edit host".
    pub suggest_edit: bool,
}

fn msg(title: impl Into<String>, body: impl Into<String>, suggest_edit: bool) -> Message {
    Message { title: title.into(), body: body.into(), suggest_edit }
}

/// `hostLabel`: an IPv6 literal gets brackets before a port.
#[must_use]
pub fn host_label(host: &str, port: u16) -> String {
    if port == 22 {
        return host.to_owned();
    }
    let bare = host.contains(':') && !host.starts_with('[');
    if bare { format!("[{host}]:{port}") } else { format!("{host}:{port}") }
}

/// `addressLabel`.
#[must_use]
pub fn address_label(profile: &Profile) -> String {
    format!("{}@{}", profile.username, host_label(&profile.host, profile.port))
}

pub const STAGES: [&str; 4] = ["connecting", "verifyingHostKey", "authenticating", "startingShell"];

#[must_use]
pub fn stage_label(stage: &str, profile: &Profile) -> String {
    match stage {
        "connecting" => format!("Connect to {}", host_label(&profile.host, profile.port)),
        "verifyingHostKey" => "Verify server identity".into(),
        "authenticating" => format!("Sign in as {}", profile.username),
        "startingShell" => "Open shell".into(),
        other => other.to_owned(),
    }
}

#[must_use]
pub fn auth_method_label(method: &str) -> &str {
    match method {
        "password" => "Password",
        "key" => "Private key",
        "agent" => "SSH agent",
        other => other,
    }
}

/// `describeSSHError`.
#[must_use]
pub fn describe_error(code: &str, profile: &Profile) -> Message {
    let host = host_label(&profile.host, profile.port);
    let name = &profile.host;
    let port = profile.port;
    let user = &profile.username;
    match code {
        "SSH_INVALID_TARGET" => msg("Host details are incomplete", "Check the address, port and username in the host settings.", true),
        "SSH_DNS_FAILED" => msg(format!("Can't find {name}"), "The address did not resolve. Check the spelling, or your network and DNS settings.", true),
        "SSH_CONNECTION_REFUSED" => msg(
            format!("{host} refused the connection"),
            format!("No SSH server is listening on port {port}. Check the port, or that the SSH service is running."),
            true,
        ),
        "SSH_TIMEOUT" => msg(
            format!("{name} didn't respond in time"),
            format!("The server may be offline, or a firewall may block port {port}. Check your network, then try again."),
            false,
        ),
        "SSH_UNREACHABLE" => msg(format!("Can't reach {name}"), "The network path to the server is unavailable. Check your connection or VPN, then try again.", false),
        "SSH_HANDSHAKE_FAILED" => msg(
            "Couldn't start a secure connection",
            format!("The service on {host} did not complete the SSH handshake. Make sure this port belongs to an SSH server."),
            true,
        ),
        "SSH_HOST_KEY_REJECTED" => msg(
            "Server not trusted",
            "Sign-in stopped before your credentials were sent, because the server key was not trusted. Connect again to review its key.",
            false,
        ),
        "SSH_HOST_KEY_CHANGED" => msg("The server's identity changed", "Open a terminal session to this host to review the new key before browsing files.", false),
        "SSH_HOST_KEY_UNKNOWN" => msg("Server not verified yet", "Open a terminal session to this host first to verify its key.", false),
        "SSH_PASSWORD_REQUIRED" => msg("Password needed", format!("Enter the password for {user}."), false),
        "SSH_PASSPHRASE_REQUIRED" => msg("Key passphrase needed", "This private key is protected by a passphrase.", false),
        "SSH_PASSPHRASE_INCORRECT" => msg("Wrong passphrase", "The passphrase did not unlock the private key. Try again.", false),
        "SSH_AUTH_FAILED" => match profile.auth_method.as_str() {
            "key" => msg(
                "The server rejected your key",
                format!("Add the public key to ~/.ssh/authorized_keys for {user} on the server, or choose another key."),
                true,
            ),
            "agent" => msg("No agent key was accepted", "Load the right key into your SSH agent (ssh-add), or choose a key file in the host settings.", true),
            _ => msg("Wrong username or password", format!("Check the password for {user}."), false),
        },
        "SSH_AUTH_METHOD_UNAVAILABLE" => msg(
            "The server doesn't allow this sign-in method",
            format!("{} sign-in is turned off on this server. Choose another method in the host settings.", auth_method_label(&profile.auth_method)),
            true,
        ),
        "SSH_KEY_NOT_FOUND" => msg(
            "Private key not found",
            match &profile.private_key_path {
                Some(path) => format!("Troupe can't find {path}. Choose the key again in the host settings."),
                None => "Choose a private key in the host settings.".into(),
            },
            true,
        ),
        "SSH_KEY_INVALID" => msg("Troupe can't use this key", "Choose an OpenSSH or PEM private key. Convert PuTTY .ppk keys with PuTTYgen first.", true),
        "SSH_AGENT_UNAVAILABLE" => msg("SSH agent not available", "Start your SSH agent and add a key (ssh-add), or choose a key file in the host settings.", true),
        "SSH_CANCELLED" => msg("Connection cancelled", "Connect again when you are ready.", false),
        "SSH_SHELL_FAILED" => msg(
            "Signed in, but the shell didn't start",
            "The server refused a terminal session. Check the account's login shell and the server's session limits.",
            false,
        ),
        "SSH_TERMINAL_LIMIT" => msg("Too many terminals open", "Close a terminal or a session tab, then try again.", false),
        "SSH_PROFILE_NOT_FOUND" => msg("This host was removed", "Close this tab, or add the host again.", false),
        _ => msg("Couldn't connect", "Something went wrong while connecting. The details are below.", false),
    }
}

/// `describeSessionEnd`.
#[must_use]
pub fn describe_end(end: &SessionEnd, profile: &Profile) -> Message {
    match end.reason.as_str() {
        "lost" => msg(
            "Connection lost",
            format!("The connection to {} dropped. Output so far is kept below.", host_label(&profile.host, profile.port)),
            false,
        ),
        "exited" => msg(
            "Session ended",
            match end.exit_code {
                Some(code) if code != 0 => format!("The remote shell exited with code {code}."),
                _ => "The remote shell exited.".into(),
            },
            false,
        ),
        _ => msg("Session closed", "This session was closed.", false),
    }
}

/// The delete-host confirm (`deleteMessage`): every tab of the host closes.
#[must_use]
pub fn delete_host_message(name: &str, open_sessions: usize) -> String {
    let sessions = match open_sessions {
        0 => String::new(),
        1 => " Its session will be closed.".into(),
        n => format!(" Its {n} sessions will be closed."),
    };
    format!("Delete “{name}”?{sessions} Saved credentials are removed from your system keychain. This can't be undone.")
}

/// The delete confirm of a remote entry: its title and message.
#[must_use]
pub fn delete_remote(name: &str, directory: bool, host: &str) -> (&'static str, String) {
    if directory {
        ("Delete folder", format!("Delete the folder “{name}” on {host}? It must be empty. This can't be undone."))
    } else {
        ("Delete file", format!("Delete “{name}” on {host}? This can't be undone."))
    }
}

/// `nameList`: `a`, `a and b`, `a, b and 3 more`.
fn name_list(names: &[String]) -> String {
    let quoted: Vec<String> = names.iter().map(|n| format!("“{n}”")).collect();
    match quoted.as_slice() {
        [] => String::new(),
        [one] => one.clone(),
        [a, b] => format!("{a} and {b}"),
        [a, b, rest @ ..] if rest.len() == 1 => format!("{a}, {b} and {}", rest[0]),
        [a, b, rest @ ..] => format!("{a}, {b} and {} more", rest.len()),
    }
}

/// The replace confirm of an upload over existing files: its title and message.
#[must_use]
pub fn replace_remote(conflicts: &[String], dir: &str, host: &str) -> (String, String) {
    let n = conflicts.len();
    let title = if n == 1 { "Replace 1 file?".to_owned() } else { format!("Replace {n} files?") };
    let (exist, them) = if n == 1 { ("exists", "it") } else { ("exist", "them") };
    (title, format!("{} already {exist} in {dir} on {host}. Uploading replaces {them}. This can't be undone.", name_list(conflicts)))
}
