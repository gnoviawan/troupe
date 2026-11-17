#![cfg(not(target_os = "windows"))]
//! Bash generator: command names and quoted or escaped paths (#1879).

use super::inject_test_support::{bash_supports_integration, on_path};
use std::path::{Path, PathBuf};

const BASH_COMPLETION: &str = "/usr/share/bash-completion/bash_completion";

fn shells() -> Vec<PathBuf> {
    let mut paths = vec![on_path("bash").unwrap(), PathBuf::from("/bin/bash")];
    paths.dedup();
    paths.retain(|shell| bash_supports_integration(shell));
    paths
}

fn functions() -> String {
    let source = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/resources/shell-integration/bash.sh"
    ))
    .unwrap();
    source
        .split("# Original Troupe session protocol helpers.")
        .nth(1)
        .unwrap()
        .split("# Install at script scope:")
        .next()
        .unwrap()
        .to_owned()
}

fn project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("dir with space")).unwrap();
    std::fs::create_dir(dir.path().join("dirb")).unwrap();
    std::fs::write(dir.path().join("file one.txt"), "").unwrap();
    dir
}

fn candidates(shell: &Path, cwd: &Path, prelude: &str, draft: &str) -> Vec<String> {
    let script = format!("{}\n{prelude}\n__termul_candidates \"$DRAFT\"\n", functions());
    let output = std::process::Command::new(shell)
        .args(["--noprofile", "--norc", "-c", &script])
        .current_dir(cwd)
        .env_clear()
        .env("HOME", cwd)
        .env("PATH", "/usr/bin:/bin")
        .env("DRAFT", draft)
        .output()
        .unwrap();
    assert!(output.status.success(), "{shell:?}: {draft}: {output:?}");
    String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect()
}

/// bash-completion's loader installs `_minimal` for any unknown word, so a
/// command-position word must reach `compgen -c` before any `-F` function.
#[test]
fn command_names_complete_even_when_a_loader_claims_the_word() {
    let dir = project();
    let loader = "_termul_test_minimal() { COMPREPLY=(); }\n\
                  _completion_loader() { complete -F _termul_test_minimal -- \"$1\"; }";
    for shell in shells() {
        for draft in ["ech", "\"ech", "ec\\h"] {
            let rows = candidates(&shell, dir.path(), loader, draft);
            assert!(rows.iter().any(|row| row == "echo"), "{shell:?}: {draft}: {rows:?}");
        }
    }
}

/// Completers see the dequoted word, as readline hands it to them.
#[test]
fn quoted_and_escaped_words_reach_completers_dequoted() {
    let dir = project();
    let probe = "_termul_test_files() { local IFS=$'\\n'; COMPREPLY=($(compgen -f -- \"$2\")); }\n\
                 complete -F _termul_test_files cat";
    for shell in shells() {
        for draft in [
            "cat dir\\ w",
            "cat \"dir w",
            "cat 'dir w",
            "cat \"dir\"\\ w",
        ] {
            let rows = candidates(&shell, dir.path(), probe, draft);
            assert_eq!(rows, ["dir with space"], "{shell:?}: {draft}");
        }
        let rows = candidates(&shell, dir.path(), probe, "cat dir");
        assert!(rows.contains(&"dir with space".to_owned()), "{shell:?}: {rows:?}");
    }
}

/// The real bash-completion package, when the host has it (Debian/Ubuntu).
#[test]
fn bash_completion_package_completes_names_and_spaced_paths() {
    if !Path::new(BASH_COMPLETION).is_file() {
        return;
    }
    let dir = project();
    for shell in shells() {
        let rows = candidates(&shell, dir.path(), "", "ech");
        assert!(rows.iter().any(|row| row == "echo"), "{shell:?}: {rows:?}");
        for draft in [
            "cat dir\\ w",
            "cat \"dir w",
            "ls 'dir w",
            "cd \"dir w",
            "cd dir\\ w",
        ] {
            let rows = candidates(&shell, dir.path(), "", draft);
            assert!(
                rows.iter().any(|row| row.trim_end_matches('/') == "dir with space"),
                "{shell:?}: {draft}: {rows:?}"
            );
        }
        let rows = candidates(&shell, dir.path(), "", "cat \"file o");
        assert_eq!(rows, ["file one.txt"], "{shell:?}");
    }
}
