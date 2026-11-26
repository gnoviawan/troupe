use super::*;

fn var(key: &str, value: &str) -> EnvVar {
    EnvVar { key: key.into(), value: value.into(), is_secret: None }
}

#[test]
fn a_save_trims_keys_and_directories_and_drops_the_blank_ones() {
    let draft = Draft {
        name: "Web".into(),
        color: "green".into(),
        path: "/w".into(),
        shell: String::new(),
        env_vars: vec![var(" A ", " 1 "), var("  ", "lost"), var("B", "")],
        symlink_dirs: vec![" node_modules ".into(), "   ".into(), String::new(), ".venv".into()],
    };
    let updates = draft.updates();
    assert_eq!(updates.name.as_deref(), Some("Web"));
    assert_eq!(updates.client.default_shell.as_deref(), Some(""));
    assert_eq!(updates.client.env_vars, Some(vec![var("A", " 1 "), var("B", "")]));
    assert_eq!(updates.client.symlink_dirs, Some(vec!["node_modules".to_owned(), ".venv".to_owned()]));
}

#[test]
fn an_import_merges_what_parsed_and_names_the_skipped_lines() {
    let got = import(&[var("A", "old"), var("KEEP", "k")], "A=new\nbad line\n1X=2\nB='q'\nno\nstill bad");
    assert_eq!(got.vars, Some(vec![var("A", "new"), var("KEEP", "k"), var("B", "q")]));
    assert_eq!(got.error, None);
    assert_eq!(
        got.warning.as_deref(),
        Some("Imported 2 variables.\nSkipped 4 invalid line(s):\nLine 2: bad line\nLine 3: 1X=2\nLine 5: no (+1 more)")
    );
}

#[test]
fn an_import_of_only_comments_is_empty_and_of_only_bad_lines_changes_nothing() {
    assert_eq!(import(&[], "# c\n\n  \n").error.as_deref(), Some("The .env file is empty."));
    let bad = import(&[var("A", "1")], "nope");
    assert_eq!((bad.vars, bad.error), (None, None));
    assert_eq!(bad.warning.as_deref(), Some("Imported 0 variables.\nSkipped 1 invalid line(s):\nLine 1: nope"));
}
