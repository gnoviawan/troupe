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

#[test]
fn a_restored_tab_asks_once_and_adopts_the_preserved_pty() {
    let mut t = Terminals::default();
    assert!(t.should_restore("pty-r"));
    t.restore(9, "alpha", "pty-r");
    assert!(!t.should_restore("pty-r"), "asked once");
    let session = t.restored(9, Some(spawned("pty-r"))).expect("the preserved PTY");
    assert_eq!((session.id.as_str(), session.project_id.as_str(), session.status.clone()), ("pty-r", "alpha", Status::Live));
    assert!(t.restored(9, Some(spawned("pty-r"))).is_none(), "a second reply is ignored");
}

#[test]
fn a_restored_tab_whose_pty_is_gone_holds_no_session() {
    let mut t = Terminals::default();
    t.restore(3, "alpha", "pty-gone");
    assert!(t.restored(3, None).is_none());
    t.restore(4, "alpha", "pty-x");
    assert!(t.restored(4, Some(spawned("pty-other"))).is_none(), "an entry for another PTY is not this tab's");
    assert!(t.get("pty-gone").is_none() && t.get("pty-x").is_none());
    assert!(!t.should_restore("pty-gone"), "not asked again");
}

#[test]
fn a_restored_tab_closed_before_the_reply_attaches_nothing_later() {
    let mut t = Terminals::default();
    t.restore(5, "alpha", "pty-r");
    assert!(t.close("pty-r").is_none(), "this client held no session to kill");
    assert!(t.restoring.is_empty(), "the pending ask is dropped");
    assert!(t.restored(5, Some(spawned("pty-r"))).is_none(), "the late reply adopts nothing");
    assert!(t.get("pty-r").is_none());
}
