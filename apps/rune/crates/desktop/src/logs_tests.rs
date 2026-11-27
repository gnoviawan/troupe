use std::time::{Duration, UNIX_EPOCH};

use log::LevelFilter;

use super::{default_export_dir, export, level, timestamp, unique_path};

#[test]
fn timestamps_are_utc_iso_with_milliseconds() {
    assert_eq!(timestamp(UNIX_EPOCH), "1970-01-01T00:00:00.000Z");
    let at = UNIX_EPOCH + Duration::from_millis(1_795_752_306_789);
    assert_eq!(timestamp(at), "2026-11-27T04:05:06.789Z");
    let leap = UNIX_EPOCH + Duration::from_secs(951_782_400);
    assert_eq!(timestamp(leap), "2000-02-29T00:00:00.000Z");
}

#[test]
fn rust_log_sets_the_global_level_and_ignores_module_scopes() {
    assert_eq!(level(Some("warn")), LevelFilter::Warn);
    assert_eq!(level(Some("hyper=trace,error")), LevelFilter::Error);
    assert_eq!(level(None), LevelFilter::Info);
    assert_eq!(level(Some("troupe_desktop=trace")), LevelFilter::Info);
    assert_eq!(level(Some("nonsense")), LevelFilter::Info);
}

#[test]
fn export_goes_to_downloads_then_desktop() {
    let home = std::env::temp_dir().join(format!("r10a-home-{}", std::process::id()));
    std::fs::create_dir_all(home.join("Desktop")).unwrap();
    let env = |name: &str| (name == "HOME").then(|| home.to_string_lossy().into_owned());
    assert_eq!(default_export_dir(env), Some(home.join("Desktop")));
    std::fs::create_dir_all(home.join("Downloads")).unwrap();
    assert_eq!(default_export_dir(env), Some(home.join("Downloads")));
    assert_eq!(default_export_dir(|_: &str| None), None);
    std::fs::remove_dir_all(&home).unwrap();
}

#[test]
fn exporting_a_missing_log_says_so() {
    let missing = std::env::temp_dir().join("r10a-no-such.log");
    let dest = std::env::temp_dir().join("r10a-dest.log");
    assert_eq!(export(&missing, &dest).unwrap_err(), "Log file does not exist yet");
}

#[test]
fn an_export_to_the_default_folder_never_replaces_an_earlier_one() {
    let dir = std::env::temp_dir().join(format!("r10a-unique-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let first = unique_path(&dir, "troupe-preview.log");
    assert_eq!(first, dir.join("troupe-preview.log"));
    std::fs::write(&first, "a").unwrap();
    let second = unique_path(&dir, "troupe-preview.log");
    assert_eq!(second, dir.join("troupe-preview (1).log"));
    std::fs::write(&second, "b").unwrap();
    assert_eq!(unique_path(&dir, "troupe-preview.log"), dir.join("troupe-preview (2).log"));
    assert_eq!(std::fs::read_to_string(&first).unwrap(), "a", "the earlier export is intact");
    std::fs::remove_dir_all(&dir).unwrap();
}
