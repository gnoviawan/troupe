use std::time::{Duration, UNIX_EPOCH};

use log::LevelFilter;

use super::{level, timestamp};

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
