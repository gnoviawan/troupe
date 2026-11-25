//! Tests for [`super::expand_env_references`], [`super::expand_custom_env`]
//! and [`super::value_references_var`].
//!
//! The fake base env stands in for the process env: expansion must resolve
//! against it only, never against sibling custom vars.

use std::collections::HashMap;

use super::{expand_custom_env, expand_env_references, value_references_var};

fn fake_base() -> HashMap<String, String> {
    HashMap::from([
        ("HOME".to_string(), "/home/u".to_string()),
        ("PATH".to_string(), "/usr/bin:/bin".to_string()),
        ("USERPROFILE".to_string(), r"C:\Users\u".to_string()),
    ])
}

#[test]
fn expands_bare_and_braced_unix_references() {
    let base = fake_base();
    assert_eq!(
        expand_env_references("$HOME/bin:$PATH", &base, false),
        "/home/u/bin:/usr/bin:/bin"
    );
    assert_eq!(
        expand_env_references("${HOME}/workspace", &base, false),
        "/home/u/workspace"
    );
}

#[test]
fn expands_windows_percent_references() {
    let base = fake_base();
    assert_eq!(
        expand_env_references("%USERPROFILE%\\workspace", &base, false),
        r"C:\Users\u\workspace"
    );
}

#[test]
fn unknown_references_expand_to_empty() {
    let base = fake_base();
    assert_eq!(
        expand_env_references("$UNDEFINED_VAR/path", &base, false),
        "/path"
    );
    assert_eq!(
        expand_env_references("${UNDEFINED}/x", &base, false),
        "/x"
    );
    assert_eq!(
        expand_env_references("%UNDEFINED%/x", &base, false),
        "/x"
    );
}

#[test]
fn incomplete_references_stay_literal() {
    let base = fake_base();
    assert_eq!(expand_env_references("price $", &base, false), "price $");
    assert_eq!(expand_env_references("$$HOME", &base, false), "$/home/u");
    assert_eq!(
        expand_env_references("a ${HOME b", &base, false),
        "a ${HOME b"
    );
    assert_eq!(
        expand_env_references("a ${9X} b", &base, false),
        "a ${9X} b"
    );
    assert_eq!(
        expand_env_references("a $HOME{bar} b", &base, false),
        "a $HOME{bar} b"
    );
    assert_eq!(expand_env_references("100% sure", &base, false), "100% sure");
    assert_eq!(
        expand_env_references("%USERPROFILE% and %USERPROFILE", &base, false),
        r"C:\Users\u and %USERPROFILE"
    );
}

#[test]
fn substituted_text_is_not_re_expanded() {
    let base = HashMap::from([
        ("OUTER".to_string(), "$HOME".to_string()),
        ("HOME".to_string(), "/home/u".to_string()),
    ]);
    // One pass: `$OUTER` becomes the literal `$HOME`, not `/home/u`.
    assert_eq!(
        expand_env_references("$OUTER/bin", &base, false),
        "$HOME/bin"
    );
}

#[test]
fn non_ascii_text_passes_through_untouched() {
    let base = fake_base();
    assert_eq!(
        expand_env_references("héllo $HOME wörld", &base, false),
        "héllo /home/u wörld"
    );
}

#[test]
fn case_insensitive_lookup_matches_windows_keys() {
    let base = HashMap::from([("Path".to_string(), r"C:\Win".to_string())]);
    assert_eq!(
        expand_env_references("%PATH%\\x", &base, true),
        r"C:\Win\x"
    );
    assert_eq!(
        expand_env_references("$path\\x", &base, true),
        r"C:\Win\x"
    );
    // Case-sensitive lookup does not match a differently-cased key.
    assert_eq!(expand_env_references("%PATH%\\x", &base, false), "\\x");
}

#[test]
fn detects_path_references_in_all_three_forms() {
    assert!(value_references_var("$HOME/bin:$PATH", "PATH", false));
    assert!(value_references_var("prefix ${PATH} suffix", "PATH", false));
    assert!(value_references_var("%PATH%;C:\\bin", "PATH", false));
    assert!(value_references_var(
        "a${HOME}b$USER-c%PATH%d",
        "PATH",
        false
    ));
    // The second `$` in `$$PATH` is a real reference.
    assert!(value_references_var("$$PATH", "PATH", false));
    // A complete `%FOO%` does not hide a real reference after it.
    assert!(value_references_var("%FOO%%PATH%", "PATH", false));
    assert!(value_references_var("%FOO% $PATH", "PATH", false));
}

#[test]
fn rejects_lookalikes_and_non_references() {
    // A longer name is a different variable.
    assert!(!value_references_var("$PATHX/bin", "PATH", false));
    assert!(!value_references_var("$PATH2", "PATH", false));
    // A different variable, a literal assignment, a lone `$`.
    assert!(!value_references_var("$HOME/bin", "PATH", false));
    assert!(!value_references_var("PATH=/usr/bin", "PATH", false));
    assert!(!value_references_var("price $", "PATH", false));
    // A `{` right after a bare name is not a reference, same as expansion.
    assert!(!value_references_var("a $PATH{bar} b", "PATH", false));
    // Unterminated `%` form and a bare `%` are not references.
    assert!(!value_references_var("%PATH", "PATH", false));
    assert!(!value_references_var("100% sure", "PATH", false));
    assert!(!value_references_var("", "PATH", false));
    // A closing `%` cannot be reused as the opening `%` of another
    // reference: `%FOO%PATH%` references FOO only, the trailing `PATH%`
    // stays literal under expansion.
    assert!(!value_references_var("%FOO%PATH%", "PATH", false));
    assert!(!value_references_var("%FOO%PATH%", "PATH", true));
}

#[test]
fn reference_matching_follows_lookup_case_rules() {
    assert!(!value_references_var("$path/bin", "PATH", false));
    assert!(value_references_var("$path/bin", "PATH", true));
    assert!(value_references_var("%path%\\x", "PATH", true));
}

/// The merge step: `PATH=$HOME/bin:$PATH` expands against the fake base
/// env, so the spawned process keeps the real inherited PATH.
#[test]
fn merge_expands_path_against_fake_base_env_only() {
    let base = fake_base();
    let custom = HashMap::from([("PATH".to_string(), "$HOME/bin:$PATH".to_string())]);
    let merged = expand_custom_env(custom, &base, false);
    assert_eq!(merged["PATH"], "/home/u/bin:/usr/bin:/bin");
}

#[test]
fn merge_never_expands_against_sibling_custom_vars() {
    let base = fake_base();
    let custom = HashMap::from([
        ("BASE".to_string(), "/app".to_string()),
        ("FULL".to_string(), "$BASE/data".to_string()),
    ]);
    let merged = expand_custom_env(custom, &base, false);
    assert_eq!(merged["BASE"], "/app");
    // `$BASE` is a project var, not a base var: it resolves to empty.
    assert_eq!(merged["FULL"], "/data");
}

#[test]
fn merge_keeps_plain_values_unchanged() {
    let base = fake_base();
    let custom = HashMap::from([
        ("NODE_ENV".to_string(), "production".to_string()),
        ("PORT".to_string(), "3000".to_string()),
    ]);
    let merged = expand_custom_env(custom, &base, false);
    assert_eq!(merged["NODE_ENV"], "production");
    assert_eq!(merged["PORT"], "3000");
}
