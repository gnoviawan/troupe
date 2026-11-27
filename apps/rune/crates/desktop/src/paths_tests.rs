use std::path::PathBuf;

use super::{Os, Paths};

fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
    let pairs: Vec<(String, String)> = pairs.iter().map(|(k, v)| ((*k).into(), (*v).into())).collect();
    move |name| pairs.iter().find(|(k, _)| k == name).map(|(_, v)| v.clone())
}

#[test]
fn linux_uses_the_tauri_data_dir_and_its_logs_folder() {
    let paths = Paths::resolve(Os::Linux, env(&[("HOME", "/home/u")])).unwrap();
    assert_eq!(paths.data_dir, PathBuf::from("/home/u/.local/share/com.termul-manager.app"));
    assert_eq!(paths.log_dir, PathBuf::from("/home/u/.local/share/com.termul-manager.app/logs"));
    assert_eq!(
        paths.launcher_dir,
        PathBuf::from("/home/u/.local/share/com.termul-manager.app/troupe-preview")
    );
}

#[test]
fn linux_follows_xdg_data_home() {
    let paths = Paths::resolve(Os::Linux, env(&[("HOME", "/home/u"), ("XDG_DATA_HOME", "/x")])).unwrap();
    assert_eq!(paths.data_dir, PathBuf::from("/x/com.termul-manager.app"));
}

#[test]
fn mac_keeps_logs_under_library_logs() {
    let paths = Paths::resolve(Os::Mac, env(&[("HOME", "/Users/u")])).unwrap();
    assert_eq!(
        paths.data_dir,
        PathBuf::from("/Users/u/Library/Application Support/com.termul-manager.app")
    );
    assert_eq!(paths.log_dir, PathBuf::from("/Users/u/Library/Logs/com.termul-manager.app"));
}

#[test]
fn windows_splits_roaming_data_from_local_logs() {
    let paths = Paths::resolve(
        Os::Windows,
        env(&[("APPDATA", "C:/R"), ("LOCALAPPDATA", "C:/L")]),
    )
    .unwrap();
    assert_eq!(paths.data_dir, PathBuf::from("C:/R/com.termul-manager.app"));
    assert_eq!(paths.log_dir, PathBuf::from("C:/L/com.termul-manager.app/logs"));
}

#[test]
fn the_override_puts_everything_under_one_folder() {
    let paths = Paths::resolve(Os::Linux, env(&[("TROUPE_PREVIEW_DATA_DIR", "/tmp/t")])).unwrap();
    assert_eq!(paths.data_dir, PathBuf::from("/tmp/t"));
    assert_eq!(paths.log_dir, PathBuf::from("/tmp/t/logs"));
    assert_eq!(paths.record_file(), PathBuf::from("/tmp/t/troupe-preview/server.json"));
}

#[test]
fn a_missing_home_is_an_error_not_a_guess() {
    assert_eq!(Paths::resolve(Os::Mac, env(&[])).unwrap_err(), "HOME is not set");
}

#[test]
fn each_data_dir_gets_its_own_instance_id() {
    let a = Paths::resolve(Os::Linux, env(&[("TROUPE_PREVIEW_DATA_DIR", "/tmp/a")])).unwrap();
    let b = Paths::resolve(Os::Linux, env(&[("TROUPE_PREVIEW_DATA_DIR", "/tmp/b")])).unwrap();
    assert_ne!(a.instance_id(), b.instance_id());
    assert_eq!(a.instance_id(), a.clone().instance_id());
    assert!(a.instance_id().starts_with("com.termul-manager.app.preview-"));
}
