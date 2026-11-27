use std::time::{Duration, Instant};

use super::process;
use super::supervisor::{BACKOFF, Effect, Event, FAST, MAX_FAST_EXITS, Notice, Phase, Supervisor};

fn exited(at: Instant) -> Event {
    Event::Exited { at, status: "signal 9".into() }
}

/// Crashes the running server `at`, then ticks to its restart and starts it again.
fn crash_and_restart(sup: &mut Supervisor, at: Instant) -> Duration {
    let Some(Effect::Notify(Notice::Restarting { delay, .. })) = sup.step(exited(at)) else {
        panic!("a crash schedules a restart: {sup:?}");
    };
    assert_eq!(sup.step(Event::Tick { at: at + delay / 2 }), None, "not due yet");
    assert_eq!(sup.step(Event::Tick { at: at + delay }), Some(Effect::Spawn));
    assert_eq!(sup.step(Event::Started { at: at + delay }), Some(Effect::Notify(Notice::Recovered)));
    delay
}

#[test]
fn five_fast_crashes_give_up_after_backing_off() {
    let t0 = Instant::now();
    let mut sup = Supervisor::running(t0);
    let mut at = t0 + Duration::from_secs(1);
    let mut delays = Vec::new();
    for _ in 0..MAX_FAST_EXITS - 1 {
        let delay = crash_and_restart(&mut sup, at);
        delays.push(delay);
        at += delay + Duration::from_secs(1);
    }
    assert_eq!(delays, BACKOFF.to_vec(), "250 ms, 1 s, 2 s, 5 s");
    assert_eq!(
        sup.step(exited(at)),
        Some(Effect::Notify(Notice::GaveUp { status: "signal 9".into(), exits: 5 }))
    );
    assert_eq!(sup.phase, Phase::GaveUp);
    assert_eq!(sup.step(Event::Tick { at: at + Duration::from_secs(60) }), None, "no restart after giving up");
}

#[test]
fn a_retry_after_giving_up_starts_again_with_a_fresh_count() {
    let t0 = Instant::now();
    let mut sup = Supervisor { phase: Phase::GaveUp, fast_exits: MAX_FAST_EXITS };
    assert_eq!(sup.step(Event::Retry), Some(Effect::Spawn));
    assert_eq!(sup.step(Event::Started { at: t0 }), Some(Effect::Notify(Notice::Recovered)));
    assert_eq!(crash_and_restart(&mut sup, t0 + Duration::from_secs(1)), BACKOFF[0]);
}

#[test]
fn a_server_that_ran_past_the_window_resets_the_count() {
    let t0 = Instant::now();
    let mut sup = Supervisor::running(t0);
    let mut at = t0 + Duration::from_secs(1);
    for _ in 0..3 {
        at += crash_and_restart(&mut sup, at) + Duration::from_secs(1);
    }
    assert_eq!(sup.fast_exits, 3);
    let long_run = at + FAST + Duration::from_secs(1);
    assert_eq!(crash_and_restart(&mut sup, long_run), BACKOFF[0]);
    assert_eq!(sup.fast_exits, 1);
}

#[test]
fn a_failed_spawn_counts_as_a_fast_exit() {
    let t0 = Instant::now();
    let mut sup = Supervisor::running(t0);
    let Some(Effect::Notify(Notice::Restarting { delay, .. })) = sup.step(exited(t0)) else { panic!() };
    assert_eq!(sup.step(Event::Tick { at: t0 + delay }), Some(Effect::Spawn));
    let failed = Event::Exited { at: t0 + delay, status: "cannot start".into() };
    assert_eq!(
        sup.step(failed),
        Some(Effect::Notify(Notice::Restarting { status: "cannot start".into(), delay: BACKOFF[1] }))
    );
    assert_eq!(sup.fast_exits, 2);
}

#[test]
fn stop_ends_supervision() {
    let t0 = Instant::now();
    let mut sup = Supervisor::running(t0);
    assert_eq!(sup.step(Event::Stop), None);
    assert_eq!(sup.step(exited(t0)), None);
    assert_eq!(sup.phase, Phase::Stopped);
}

#[test]
fn tokens_are_64_hex_digits_and_differ() {
    let (a, b) = (process::new_token().unwrap(), process::new_token().unwrap());
    assert_eq!(a.len(), 64);
    assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
    assert_ne!(a, b);
}

#[test]
fn a_taken_port_falls_back_to_a_free_one() {
    let held = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let taken = held.local_addr().unwrap().port();
    let picked = process::pick_port(Some(taken)).unwrap();
    assert_ne!(picked, taken);
    drop(held);
    assert_eq!(process::pick_port(Some(taken)).unwrap(), taken);
}

#[test]
fn health_needs_a_200_with_status_ok() {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    let replies = ["HTTP/1.1 200 OK\r\n\r\n{\"status\":\"ok\"}", "HTTP/1.1 503 Busy\r\n\r\n{\"status\":\"ok\"}"];
    let server = std::thread::spawn(move || {
        for reply in replies {
            let (mut conn, _) = listener.accept().unwrap();
            let mut buf = [0u8; 512];
            let _ = conn.read(&mut buf);
            conn.write_all(reply.as_bytes()).unwrap();
        }
    });
    assert!(process::healthy(port));
    assert!(!process::healthy(port));
    server.join().unwrap();
    assert!(!process::healthy(port), "nothing listens any more");
}

#[cfg(unix)]
#[test]
fn the_watchdog_stops_the_server_when_the_app_dies() {
    use std::process::Command;
    let mut app = Command::new("sleep").arg("60").spawn().unwrap();
    let mut server = Command::new("sleep").arg("60").spawn().unwrap();
    process::watchdog(app.id(), server.id());
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(server.try_wait().unwrap(), None, "the server runs while the app does");
    app.kill().unwrap();
    app.wait().unwrap();
    let until = Instant::now() + Duration::from_secs(5);
    while Instant::now() < until && server.try_wait().unwrap().is_none() {
        std::thread::sleep(Duration::from_millis(100));
    }
    let status = server.try_wait().unwrap().expect("the watchdog stopped the server within 5 s");
    use std::os::unix::process::ExitStatusExt;
    assert_eq!(status.signal(), Some(15), "SIGTERM first");
}
