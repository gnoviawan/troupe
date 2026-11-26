use super::State;
use crate::projects::{Group, Project};

fn project(id: &str, name: &str) -> Project {
    Project {
        id: id.to_owned(),
        name: name.to_owned(),
        color: "blue".to_owned(),
        path: None,
        is_active: None,
        is_default: None,
        is_archived: None,
        client: Default::default(),
    }
}

fn group(id: &str) -> Group {
    Group {
        id: id.to_owned(),
        name: id.to_owned(),
        project_ids: Vec::new(),
        is_collapsed: None,
        color: None,
    }
}

#[test]
fn a_failed_write_is_written_again() {
    let mut prefs = State::default();
    let projects = [project("a", "A")];
    let (_, signature) = prefs.persist(&projects, &[]).expect("first write");
    assert_eq!(prefs.persist(&projects, &[]), None);
    prefs.write_failed(&signature);
    let (record, _) = prefs.persist(&projects, &[]).expect("retried write");
    assert_eq!(record.order, ["a"]);
}

#[test]
fn an_older_failure_keeps_a_newer_claim() {
    let mut prefs = State::default();
    let projects = [project("a", "A")];
    let (_, old) = prefs.persist(&projects, &[]).expect("first write");
    prefs
        .persist(&projects, &[group("g")])
        .expect("newer write");
    prefs.write_failed(&old);
    assert_eq!(prefs.persist(&projects, &[group("g")]), None);
}
