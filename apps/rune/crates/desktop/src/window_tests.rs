use rune::WindowPlacement;

use super::{Frame, MIN_SIZE, load, off_screen, options};

fn placement(maximized: bool, restore: Option<((i32, i32), (u32, u32))>) -> WindowPlacement {
    WindowPlacement {
        position: (40, 30),
        content_position: (40, 58),
        content_size: (1500, 900),
        scale: 1.0,
        maximized,
        minimized: false,
        full_screen: false,
        restore,
        display: None,
    }
}

#[test]
fn a_normal_window_saves_where_it_is() {
    assert_eq!(
        Frame::of(&placement(false, None), None),
        Some(Frame { position: (40, 30), size: (1500, 900), maximized: false })
    );
}

#[test]
fn a_maximized_window_keeps_the_frame_it_restores_to() {
    let before = Frame { position: (100, 80), size: (1200, 800), maximized: false };
    assert_eq!(
        Frame::of(&placement(true, None), Some(before)),
        Some(Frame { position: (100, 80), size: (1200, 800), maximized: true })
    );
    assert_eq!(Frame::of(&placement(true, None), None), None, "nothing to restore to yet");
    assert_eq!(
        Frame::of(&placement(true, Some(((5, 6), (900, 700)))), None),
        Some(Frame { position: (5, 6), size: (900, 700), maximized: true }),
        "the system's own record wins"
    );
}

#[test]
fn a_saved_frame_round_trips_and_a_broken_one_is_ignored() {
    let dir = std::env::temp_dir().join(format!("r10a-window-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("window.json");
    std::fs::write(&path, r#"{"position":[10,20],"size":[1300,850],"maximized":false}"#).unwrap();
    assert_eq!(load(&path), Some(Frame { position: (10, 20), size: (1300, 850), maximized: false }));
    std::fs::write(&path, r#"{"position":[10,20],"size":[0,850],"maximized":false}"#).unwrap();
    assert_eq!(load(&path), None);
    std::fs::write(&path, "not json").unwrap();
    assert_eq!(load(&path), None);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn the_window_opens_hidden_at_its_saved_frame() {
    let saved = Frame { position: (10, 20), size: (1300, 850), maximized: false };
    let opts = options(Some(&saved));
    assert!(!opts.visible);
    assert_eq!(opts.min_size, Some(MIN_SIZE));
    assert_eq!(opts.position, Some((10, 20)));
    assert_eq!(opts.physical_size, Some((1300, 850)));
    let fresh = options(None);
    assert_eq!((fresh.position, fresh.physical_size), (None, None));
}

#[test]
fn a_frame_on_a_gone_monitor_is_off_screen() {
    let on = |position, display| WindowPlacement { position, display, ..placement(false, None) };
    let laptop = Some(((0, 0), (1600, 1000)));
    assert!(!off_screen(&on((40, 30), laptop)), "inside");
    assert!(!off_screen(&on((1500, 900), laptop)), "100 x 100 px still showing");
    assert!(off_screen(&on((2600, 100), laptop)), "saved on a right-hand monitor that is gone");
    assert!(off_screen(&on((-1550, 100), laptop)), "only 50 px left of it");
    assert!(!off_screen(&on((2600, 100), Some(((1600, 0), (2560, 1440))))), "that monitor is back");
    assert!(!off_screen(&on((99_000, 0), None)), "no display reported: leave it");
}
