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
    assert_eq!(sup.step(Event::Tick { at: at + delay * 2 }), None, "no second spawn while it starts");
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
fn recovered_waits_for_the_new_server_to_answer() {
    let t0 = Instant::now();
    let mut sup = Supervisor::running(t0);
    sup.step(exited(t0));
    assert_eq!(sup.step(Event::Tick { at: t0 + BACKOFF[0] }), Some(Effect::Spawn));
    assert_eq!(sup.phase, Phase::Starting);
    for i in 1..20 {
        assert_eq!(sup.step(Event::Tick { at: t0 + BACKOFF[0] * i }), None, "no Recovered before /health");
    }
    assert_eq!(sup.step(Event::Started { at: t0 + BACKOFF[0] * 20 }), Some(Effect::Notify(Notice::Recovered)));
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
    // Tests in parallel may take a freed ephemeral port; one of a few tries keeps its own.
    let kept = (0..5).any(|_| {
        let free = process::pick_port(None).unwrap();
        process::pick_port(Some(free)).unwrap() == free
    });
    assert!(kept, "a free preferred port is kept");
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
    let watchdog = process::watchdog(app.id(), server.id()).expect("the watchdog's pid");
    std::thread::sleep(Duration::from_millis(300));
    // A closed terminal hangs up the app's process group; the watchdog is in its own and
    // ignores the signal anyway.
    Command::new("kill").args(["-HUP", &watchdog.to_string()]).status().unwrap();
    std::thread::sleep(Duration::from_millis(200));
    assert!(Command::new("kill").args(["-0", &watchdog.to_string()]).status().unwrap().success(), "the watchdog survives SIGHUP");
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

fn paths(name: &str) -> crate::paths::Paths {
    let dir = std::env::temp_dir().join(format!("r10a-reuse-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let dir = dir.to_string_lossy().into_owned();
    crate::paths::Paths::resolve(crate::paths::Os::Linux, |k| (k == "TROUPE_PREVIEW_DATA_DIR").then(|| dir.clone())).unwrap()
}

fn record_for(paths: &crate::paths::Paths, pid: u32, port: u16) {
    let record = super::record::ServerRecord {
        pid,
        port,
        binary: "troupe-server".into(),
        data_dir: paths.data_dir.clone(),
        keep_alive: true,
    };
    super::record::write_record(&paths.record_file(), &record).unwrap();
    super::record::write_private(&paths.token_file(), b"secret-token").unwrap();
}

/// Holds `paths`' data lock with `pid` written in it, as a host does.
#[cfg(unix)]
fn hold_lock(paths: &crate::paths::Paths, pid: u32) -> std::fs::File {
    use std::io::Write;
    let mut lock = std::fs::OpenOptions::new()
        .create(true).truncate(false).read(true).write(true)
        .open(paths.data_dir.join(crate::data_lock::LOCK_FILE)).unwrap();
    lock.lock().unwrap();
    lock.write_all(format!("{pid}\n").as_bytes()).unwrap();
    lock
}

#[test]
fn a_stale_record_is_deleted_without_probing_its_port() {
    let paths = paths("stale");
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    listener.set_nonblocking(true).unwrap();
    record_for(&paths, 4_000_000, listener.local_addr().unwrap().port());
    assert!(super::reusable(&paths).is_none());
    assert!(!paths.record_file().exists(), "the stale record is gone");
    assert_eq!(
        listener.accept().map_err(|e| e.kind()).err(),
        Some(std::io::ErrorKind::WouldBlock),
        "nothing connected to the recorded port, so no token was offered"
    );
}

#[cfg(unix)]
#[test]
fn a_record_whose_pid_is_not_the_lock_holder_is_not_reused() {
    let paths = paths("other-holder");
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    listener.set_nonblocking(true).unwrap();
    record_for(&paths, 4_000_000, listener.local_addr().unwrap().port());
    let _held = hold_lock(&paths, 4_000_001);
    assert!(super::reusable(&paths).is_none());
    assert!(!paths.record_file().exists());
    assert_eq!(listener.accept().map_err(|e| e.kind()).err(), Some(std::io::ErrorKind::WouldBlock));
}

#[cfg(unix)]
#[test]
fn the_lock_holder_that_answers_is_reused() {
    use std::io::{Read, Write};
    let paths = paths("holder");
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = std::thread::spawn(move || {
        let (mut conn, _) = listener.accept().unwrap();
        let mut buf = [0u8; 512];
        let _ = conn.read(&mut buf);
        conn.write_all(b"HTTP/1.1 200 OK\r\n\r\n{\"status\":\"ok\"}").unwrap();
    });
    record_for(&paths, 4_000_000, port);
    let _held = hold_lock(&paths, 4_000_000);
    let (record, token) = super::reusable(&paths).expect("reused");
    assert_eq!((record.pid, record.port, token.as_str()), (4_000_000, port, "secret-token"));
    server.join().unwrap();
}

#[cfg(unix)]
#[test]
fn a_reused_server_that_holds_the_lock_stays_alive_while_it_is_slow_to_answer() {
    use std::io::{Read, Write};
    let paths = paths("slow");
    // A server too busy to answer: every probe gets a 503. Holding the port keeps another
    // test's listener from taking it.
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for mut conn in listener.incoming().flatten() {
            let mut buf = [0u8; 512];
            let _ = conn.read(&mut buf);
            let _ = conn.write_all(b"HTTP/1.1 503 Busy\r\n\r\n");
        }
    });
    let held = hold_lock(&paths, 4_000_000);
    let mut runner = super::Runner::new(&paths, "troupe-server".into(), port, "t".into(), false, super::Proc::adopted(4_000_000));
    let t0 = Instant::now();
    for i in 1..=4 {
        assert_eq!(runner.exited(t0 + Duration::from_millis(1100 * i)), None, "probe {i}: the lock says it lives");
    }
    drop(held);
    assert_eq!(runner.exited(t0 + Duration::from_millis(1100 * 5)), None, "one miss is not enough");
    // A process another test forks in parallel holds a copy of the lock until it execs, so
    // give the release a few probes.
    let gone = (6..=20).any(|i| {
        std::thread::sleep(Duration::from_millis(50));
        runner.exited(t0 + Duration::from_millis(1100 * i)).is_some()
    });
    assert!(gone, "gone once it let go of the lock");
}

#[test]
fn the_lock_names_its_holder_by_pid() {
    use crate::data_lock::Holder;
    let held = |pid| Holder { lock_file: "x".into(), pid };
    assert!(super::holds_lock(7, Some(&held(Some(7)))));
    assert!(!super::holds_lock(7, Some(&held(Some(8)))));
    assert!(!super::holds_lock(7, None));
    assert_eq!(super::holds_lock(7, Some(&held(None))), cfg!(windows));
}

#[test]
fn a_logged_server_url_hides_its_credentials() {
    assert_eq!(super::redact_url("https://me:pw@host:8080/x?y@z"), "https://***@host:8080/x?y@z");
    assert_eq!(super::redact_url("http://127.0.0.1:8190"), "http://127.0.0.1:8190");
    assert_eq!(super::redact_url("not a url"), "not a url");
}

#[test]
fn a_failed_restart_appends_to_the_server_log_instead_of_rotating_it() {
    use std::io::Write;
    let paths = paths("serverlog");
    let log = paths.server_log();
    process::server_log(&log).unwrap().write_all(b"first run\n").unwrap();
    process::server_log(&log).unwrap().write_all(b"failed restart\n").unwrap();
    assert_eq!(std::fs::read_to_string(&log).unwrap(), "first run\nfailed restart\n");
    assert!(!log.with_extension("log.prev").exists());
}
