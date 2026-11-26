use std::sync::Arc;
use std::time::Duration;

use super::*;

fn label(text: &str) -> Element {
    Element {
        key: Some("label".into()),
        path: "root/label".into(),
        role: "text",
        label: None,
        text: Some(text.into()),
        rect: [0.0, 0.0, 10.0, 10.0],
        focused: false,
    }
}

fn store(shared: &Shared, text: &str) {
    let mut s = shared.snapshot.lock().unwrap();
    s.seq += 1;
    s.elements = vec![label(text)];
    drop(s);
    shared.fresh.notify_all();
}

#[test]
fn wait_returns_once_a_later_frame_shows_the_target() {
    let shared = Arc::new(Shared::default());
    store(&shared, "loading");
    let writer = shared.clone();
    let t = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(30));
        store(&writer, "still loading");
        std::thread::sleep(Duration::from_millis(30));
        store(&writer, "ready");
    });
    let reply = wait(
        &shared,
        &Target::Text {
            text: "ready".into(),
        },
        Duration::from_secs(5),
    )
    .unwrap();
    t.join().unwrap();
    assert_eq!(reply["element"]["text"], "ready");
    assert_eq!(shared.snapshot.lock().unwrap().seq, 3);
}

#[test]
fn wait_times_out_when_no_frame_shows_the_target() {
    let shared = Shared::default();
    store(&shared, "loading");
    assert_eq!(
        wait(
            &shared,
            &Target::Key {
                key: "missing".into()
            },
            Duration::from_millis(40)
        ),
        Err("timeout".into())
    );
}
