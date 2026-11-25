use super::*;

fn spawned(id: &str) -> Spawned {
    Spawned { id: id.into(), shell: "/bin/bash".into(), cwd: "/repo".into(), pid: 41, cols: 80, rows: 24, claim: "c".repeat(64) }
}

#[test]
fn a_spawn_reply_opens_a_session_in_the_asking_pane() {
    let mut t = Terminals::default();
    t.spawn(7, "alpha", Some("pane-1".into()));
    let (session, pane) = t.spawned(7, spawned("pty-1")).expect("the reply to call 7");
    assert_eq!((session.project_id.as_str(), session.pid, pane.as_deref()), ("alpha", 41, Some("pane-1")));
    assert!(t.spawned(7, spawned("pty-2")).is_none(), "a second reply to the same call is ignored");
}

#[test]
fn sessions_outlive_project_switches_and_only_close_removes_them() {
    let mut t = Terminals::default();
    t.spawn(1, "alpha", None);
    t.spawned(1, spawned("pty-a"));
    t.spawn(2, "beta", None);
    t.spawned(2, spawned("pty-b"));
    assert_eq!(t.of_project("alpha").map(|s| s.id.as_str()).collect::<Vec<_>>(), ["pty-a"]);
    assert_eq!(t.of_project("beta").count(), 1);
    assert_eq!(t.close("pty-a").map(|s| s.pid), Some(41));
    assert!(t.get("pty-a").is_none() && t.get("pty-b").is_some());
}

#[test]
fn resize_asks_only_for_a_changed_size_of_a_live_terminal() {
    let mut t = Terminals::default();
    t.spawn(1, "alpha", None);
    t.spawned(1, spawned("pty-a"));
    assert!(!t.resize("pty-a", 80, 24), "same size");
    assert!(t.resize("pty-a", 120, 40));
    assert_eq!(t.get("pty-a").map(|s| (s.cols, s.rows)), Some((120, 40)));
    t.exited("pty-a", Some(0));
    assert!(!t.resize("pty-a", 100, 30), "an exited PTY takes no resize");
    assert_eq!(t.get("pty-a").map(|s| s.status.clone()), Some(Status::Exited { code: Some(0) }));
}
