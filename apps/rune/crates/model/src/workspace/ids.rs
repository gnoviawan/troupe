//! Tab ids. Port of the id helpers in `workspace-store/tree.ts`,
//! `lib/diff-tab-identity.ts` and `normalizePath` in `lib/working-copy.ts`.

pub fn terminal(terminal_id: &str) -> String {
    format!("term-{terminal_id}")
}

pub fn editor(path: &str) -> String {
    format!("edit-{path}")
}

pub fn browser(browser_tab_id: &str) -> String {
    format!("browser-{browser_tab_id}")
}

pub fn git(cwd: &str) -> String {
    format!("git-{cwd}")
}

pub fn git_history(cwd: &str) -> String {
    format!("git-history-{cwd}")
}

/// Issues and PRs share one number sequence per repository.
pub fn github_item(cwd: &str, repo: &str, number: u64) -> String {
    format!("github-item-{cwd}-{repo}-{number}")
}

pub fn canvas(project_id: &str) -> String {
    format!("canvas-{project_id}")
}

pub fn automations(project_id: &str) -> String {
    format!("automations-{project_id}")
}

pub fn agent_chat(session_id: &str) -> String {
    format!("chat-{session_id}")
}

/// Repo-relative POSIX form: forward slashes, no empty or `.` segments.
pub fn repo_relative(path: &str) -> String {
    path.replace('\\', "/")
        .split('/')
        .filter(|segment| !segment.is_empty() && *segment != ".")
        .collect::<Vec<_>>()
        .join("/")
}

/// `diff:` + a stable hash of `(working copy, repo path, side)`.
#[must_use]
pub fn diff_tab_id(cwd: &str, path: &str, staged: bool) -> String {
    let side = if staged { "staged" } else { "unstaged" };
    identity_id(&[&normalize_path(cwd), &repo_relative(path), side])
}

/// The read-only diff of one file at one commit, in the same id space.
#[must_use]
pub fn commit_diff_tab_id(cwd: &str, hash: &str, path: &str) -> String {
    identity_id(&[&normalize_path(cwd), &repo_relative(path), "commit", hash])
}

fn identity_id(parts: &[&str]) -> String {
    let json = serde_json::to_string(parts).unwrap_or_default();
    format!("diff:{}", stable_hash(&json))
}

/// Two 32-bit FNV-1a style passes over UTF-16 code units, as 16 hex chars.
fn stable_hash(input: &str) -> String {
    let mut h1: u32 = 0x811c_9dc5;
    let mut h2: u32 = 0x0100_0193 ^ 0x9e37_79b9;
    for unit in input.encode_utf16() {
        let code = u32::from(unit);
        h1 = (h1 ^ code).wrapping_mul(0x0100_0193);
        h2 = (h2 ^ code).wrapping_mul(0x85eb_ca6b);
        h2 ^= h2 >> 13;
    }
    format!("{h1:08x}{h2:08x}")
}

/// Canonical comparison form of a path: forward slashes, no verbatim
/// prefix, no duplicate or trailing slashes (roots keep theirs), dot
/// segments resolved, lowercased only for drive-letter and UNC paths.
#[must_use]
pub fn normalize_path(path: &str) -> String {
    let canonical = canonicalize(path);
    if is_windows_form(&canonical) {
        canonical.to_lowercase()
    } else {
        canonical
    }
}

fn canonicalize(path: &str) -> String {
    if path.is_empty() {
        return String::new();
    }
    let slashed = collapse_slashes(&strip_verbatim_prefix(&path.replace('\\', "/")));
    let resolved = resolve_dot_segments(&slashed);
    if resolved == "/" || is_drive_root(&resolved) {
        return resolved;
    }
    let trimmed = resolved.trim_end_matches('/');
    if trimmed.is_empty() {
        "/".to_owned()
    } else {
        trimmed.to_owned()
    }
}

fn has_drive(path: &str) -> bool {
    let bytes = path.as_bytes();
    bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}

fn is_drive_root(path: &str) -> bool {
    path.len() == 3 && has_drive(path) && path.ends_with('/')
}

/// `//` followed by anything but a third slash.
fn is_unc(path: &str) -> bool {
    path.starts_with("//") && !path[2..].starts_with('/')
}

fn is_windows_form(path: &str) -> bool {
    (has_drive(path) && (path.len() == 2 || path.as_bytes()[2] == b'/')) || is_unc(path)
}

fn strip_verbatim_prefix(path: &str) -> String {
    let Some(rest) = path.strip_prefix("//?/") else {
        return path.to_owned();
    };
    if rest
        .get(..4)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("unc/"))
    {
        return format!("//{}", &rest[4..]);
    }
    if rest.len() == 2 && has_drive(rest) {
        return format!("{rest}/");
    }
    if has_drive(rest) && rest.as_bytes().get(2) == Some(&b'/') {
        return rest.to_owned();
    }
    path.to_owned()
}

fn collapse_runs(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    for ch in path.chars() {
        if ch == '/' && out.ends_with('/') {
            continue;
        }
        out.push(ch);
    }
    out
}

fn collapse_slashes(path: &str) -> String {
    if is_unc(path) {
        return format!("//{}", collapse_runs(&path[2..]));
    }
    collapse_runs(path)
}

/// The root a `..` cannot climb above: `//server/share`, `//`, `C:/` or `/`.
fn dot_root(path: &str) -> &str {
    if let Some(rest) = path.strip_prefix("//") {
        if let Some(slash) = rest.find('/') {
            let server = &rest[..slash];
            let after = &rest[slash + 1..];
            let share_end = after.find('/').unwrap_or(after.len());
            let share = &after[..share_end];
            if !server.is_empty() && !share.is_empty() && share != "." && share != ".." {
                return &path[..2 + slash + 1 + share_end];
            }
        }
        return "//";
    }
    if has_drive(path) && path.as_bytes().get(2) == Some(&b'/') {
        return &path[..3];
    }
    if path.starts_with('/') {
        return "/";
    }
    ""
}

fn resolve_dot_segments(path: &str) -> String {
    if !path
        .split('/')
        .any(|segment| segment == "." || segment == "..")
    {
        return path.to_owned();
    }
    let root = dot_root(path);
    let mut kept: Vec<&str> = Vec::new();
    for segment in path[root.len()..].split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                if kept.last().is_some_and(|last| *last != "..") {
                    kept.pop();
                } else if root.is_empty() {
                    kept.push("..");
                }
            }
            other => kept.push(other),
        }
    }
    let joined = kept.join("/");
    let slash = if !joined.is_empty() && !root.ends_with('/') {
        "/"
    } else {
        ""
    };
    format!("{root}{slash}{joined}")
}
