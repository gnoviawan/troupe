use rune_core::machine::Attempt;

use crate::link::{HANDSHAKE_S, LinkEffect, LinkEvent, LinkMachine, Liveness, Status};

fn reconnecting(fx: &[LinkEffect]) -> bool {
    fx.iter()
        .any(|e| matches!(e, LinkEffect::Status(Status::Reconnecting { .. })))
}

fn opened(link: &mut LinkMachine, at: f64) -> Attempt {
    link.step(LinkEvent::Start, 0.0);
    let socket = link.socket().unwrap();
    link.step(LinkEvent::Opened(socket), at);
    socket
}

#[test]
fn a_slow_upgrade_does_not_extend_the_handshake_budget() {
    let mut link = LinkMachine::new(1, Liveness::Heartbeat);
    opened(&mut link, HANDSHAKE_S - 1.0);

    let fx: Vec<_> = link
        .step(LinkEvent::Tick, HANDSHAKE_S + 0.1)
        .into_iter()
        .collect();

    assert!(reconnecting(&fx), "{fx:?}");
}

#[test]
fn an_authenticate_reply_after_the_budget_does_not_go_live() {
    let mut link = LinkMachine::new(1, Liveness::Heartbeat);
    let socket = opened(&mut link, HANDSHAKE_S - 1.0);

    let fx: Vec<_> = link
        .step(LinkEvent::AuthOk(socket), HANDSHAKE_S + 8.0)
        .into_iter()
        .collect();

    assert_eq!(link.live(), None);
    assert!(reconnecting(&fx), "{fx:?}");
}

#[test]
fn a_handshake_inside_the_budget_goes_live() {
    let mut link = LinkMachine::new(1, Liveness::Heartbeat);
    let socket = opened(&mut link, HANDSHAKE_S - 1.0);

    link.step(LinkEvent::AuthOk(socket), HANDSHAKE_S - 0.5);

    assert_eq!(link.live(), Some(socket));
}

fn live(liveness: Liveness) -> (LinkMachine, Attempt) {
    let mut link = LinkMachine::new(1, liveness);
    let socket = opened(&mut link, 0.1);
    link.step(LinkEvent::AuthOk(socket), 0.2);
    (link, socket)
}

#[test]
fn a_silent_heartbeat_socket_is_probed_and_then_replaced() {
    let (mut link, _) = live(Liveness::Heartbeat);
    let mut fx = Vec::new();
    for t in 1..=40 {
        fx.extend(link.step(LinkEvent::Tick, f64::from(t)));
    }
    assert!(fx.iter().any(|e| matches!(e, LinkEffect::Ping(_))));
    assert!(reconnecting(&fx), "{fx:?}");
}

#[test]
fn a_quiet_close_only_socket_stays_up() {
    let (mut link, socket) = live(Liveness::CloseOnly);
    let mut fx = Vec::new();
    for t in 1..=300 {
        fx.extend(link.step(LinkEvent::Tick, f64::from(t)));
    }
    assert_eq!(fx, vec![]);
    assert_eq!(link.live(), Some(socket));
}
