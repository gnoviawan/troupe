use serde_json::json;

use super::{Draft, Errors, Profile, restored, saved};

fn draft(name: &str, shell: &str, args: &str, env: &str) -> Draft {
    Draft { name: name.into(), shell: shell.into(), args: args.into(), cwd: " /w ".into(), env: env.into() }
}

#[test]
fn a_full_draft_parses_into_a_profile() {
    let profile = draft(" Node dev ", "zsh", "-l\r\n\n-i", "# note\nNODE_ENV=development\n A = b=c ").parse("p1").expect("valid");
    assert_eq!(
        serde_json::to_value(&profile).unwrap(),
        json!({ "id": "p1", "name": "Node dev", "shell": "zsh", "args": ["-l", "-i"], "cwd": "/w", "env": { "NODE_ENV": "development", "A": " b=c " } })
    );
    assert_eq!(profile.command_line(), "zsh -l -i");
}

#[test]
fn every_failing_field_says_why() {
    let errors = draft("  ", "", "-l", "ok=1\n1BAD=x").parse("p1").unwrap_err();
    assert_eq!(
        errors,
        Errors {
            name: Some("Name is required.".into()),
            args: Some("Choose a shell to pass arguments to.".into()),
            env: Some("Line 2: write KEY=value. A key starts with a letter or _ and holds only letters, digits and _.".into()),
        }
    );
}

#[test]
fn a_profile_reads_back_as_the_draft_it_came_from() {
    let profile = draft("x", "bash", "-c\necho hi", "A=1\nB=2").parse("id").unwrap();
    assert_eq!(Draft::of(&profile), Draft { name: "x".into(), shell: "bash".into(), args: "-c\necho hi".into(), cwd: "/w".into(), env: "A=1\nB=2".into() });
    assert_eq!(Profile::default().command_line(), "Default shell");
}

#[test]
fn undo_puts_a_deleted_profile_back_where_it_was_once() {
    let p = |id: &str| Profile { id: id.into(), name: id.into(), ..Profile::default() };
    let list = vec![p("a"), p("c")];
    let back = restored(&list, p("b"), 1).expect("restored");
    assert_eq!(back.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(), ["a", "b", "c"]);
    assert_eq!(restored(&back, p("b"), 0), None);
    assert_eq!(restored(&[], p("b"), 5).unwrap().len(), 1);
    let edited = saved(&back, Profile { name: "B".into(), ..p("b") });
    assert_eq!(edited[1].name, "B");
    assert_eq!(saved(&edited, p("d")).len(), 4);
}
