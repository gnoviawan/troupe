use std::io::Read;
use std::net::TcpListener;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use rune_core::machine::Attempt;
use tungstenite::Message;

use super::{connect, run_socket};
use crate::machine::SocketEvent;

#[test]
fn wss_to_an_ipv6_literal_starts_tls() {
    let Ok(listener) = TcpListener::bind("[::1]:0") else {
        eprintln!("no IPv6 loopback on this host");
        return;
    };
    let port = listener.local_addr().unwrap().port();
    let client = thread::spawn(move || {
        connect(&format!("wss://[::1]:{port}/ws"), Duration::from_secs(5)).err()
    });
    let (mut peer, _) = listener.accept().unwrap();
    peer.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    let mut first = [0u8; 1];
    let read = peer.read(&mut first);
    drop(peer);
    let error = client.join().unwrap();

    assert_eq!(read.ok(), Some(1), "the client sent nothing: {error:?}");
    assert_eq!(first[0], 0x16, "a TLS handshake record");
}

#[test]
fn a_host_that_never_upgrades_fails_within_the_budget() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("ws://{}/ws", listener.local_addr().unwrap());
    let started = Instant::now();

    let result = connect(&url, Duration::from_millis(300));

    assert!(result.is_err());
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "{:?}",
        started.elapsed()
    );
    drop(listener);
}

#[test]
fn a_close_from_the_host_is_answered_before_the_socket_drops() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("ws://{}/ws", listener.local_addr().unwrap());
    let (events_tx, events) = mpsc::channel();
    let (_outgoing_tx, outgoing) = mpsc::channel();
    let client = thread::spawn(move || run_socket(Attempt(1), &url, &events_tx, &outgoing));
    let (stream, _) = listener.accept().unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut host = tungstenite::accept(stream).unwrap();

    host.close(None).unwrap();
    let reply = loop {
        match host.read() {
            Ok(Message::Close(_) | Message::Ping(_) | Message::Pong(_)) => {}
            other => break other,
        }
    };
    client.join().unwrap();

    assert!(
        matches!(reply, Err(tungstenite::Error::ConnectionClosed)),
        "{reply:?}"
    );
    assert_eq!(events.recv().unwrap(), SocketEvent::Opened(Attempt(1)));
}

struct Echo;

impl crate::machine::Machine for Echo {
    type Command = ();
    type Output = SocketEvent;
    const PATH: &'static str = "/echo";

    fn step(
        &mut self,
        event: crate::machine::Event<()>,
        _now: f64,
    ) -> rune_core::machine::Effects<crate::machine::Effect<SocketEvent>> {
        match event {
            crate::machine::Event::Socket(e) => {
                rune_core::machine::Effects::one(crate::machine::Effect::Output(e))
            }
            _ => rune_core::machine::Effects::none(),
        }
    }
}

#[test]
fn a_zero_timeout_still_takes_a_ready_socket_event() {
    let mut driver = super::Driver::new(Echo);
    driver
        .events_tx
        .send(SocketEvent::Opened(Attempt(3)))
        .unwrap();

    assert_eq!(
        driver.next(Duration::ZERO),
        Some(SocketEvent::Opened(Attempt(3)))
    );
}
