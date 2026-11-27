use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;

use super::{Holder, LOCK_FILE, holder};

fn dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("r10a-lock-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn no_lock_file_means_no_holder() {
    let dir = dir("none");
    assert_eq!(holder(&dir).unwrap(), None);
}

#[test]
fn an_unheld_lock_file_means_no_holder_and_stays_unheld() {
    let dir = dir("free");
    std::fs::write(dir.join(LOCK_FILE), "123\n").unwrap();
    assert_eq!(holder(&dir).unwrap(), None);
    assert_eq!(holder(&dir).unwrap(), None);
}

#[test]
fn a_held_lock_names_the_pid_its_holder_wrote() {
    let dir = dir("held");
    let mut file = OpenOptions::new().create(true).truncate(false).read(true).write(true).open(dir.join(LOCK_FILE)).unwrap();
    file.lock().unwrap();
    file.write_all(b"4242\n").unwrap();
    let found = holder(&dir).unwrap().expect("held");
    // Windows locks are mandatory: the probe cannot read the record, as the hosts write none.
    let pid = if cfg!(windows) { None } else { Some(4242) };
    assert_eq!(found, Holder { lock_file: dir.join(LOCK_FILE), pid });
    let message = found.message(&dir);
    assert!(message.starts_with("Troupe (the desktop app"), "{message}");
    assert!(message.contains("Quit it, then open Troupe Preview again."), "{message}");
    drop(file);
    assert_eq!(holder(&dir).unwrap(), None);
}
