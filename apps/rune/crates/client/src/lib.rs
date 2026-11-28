//! Troupe's client transport, shared by the Rune desktop app, the Rune web
//! client and `troupe-probe`.
//!
//! Each socket is a pure state machine: [`ws::WsClient`] for `/ws` (sessions,
//! watches, ledgered commands) and [`terminal::TerminalClient`] for
//! `/terminal/ws` (attach, replay, claims). Both share [`link::Link`], the
//! connect, `authenticate`, heartbeat and backoff lifecycle. A machine takes an
//! [`Event`] and returns [`Effect`]s; it never touches a socket or a clock. A
//! backend carries the effects out: [`native`] on desktop (blocking sockets on
//! threads) and [`web`] in the browser (`web-sys` WebSocket and fetch). [`http`]
//! holds the HTTP routes' body contract with a backend per target.

pub mod cursor;
pub mod http;
pub mod ledger;
pub mod link;
pub mod machine;
pub mod terminal;
pub mod ws;

#[cfg(not(target_arch = "wasm32"))]
pub mod native;
#[cfg(target_arch = "wasm32")]
pub mod web;

pub use machine::{CallId, Config, Effect, Event, HostLocalCredential, Machine, SocketEvent};
pub use rune_core::machine::{Attempt, Effects};

#[cfg(test)]
mod ledger_tests;
#[cfg(test)]
mod link_tests;
#[cfg(test)]
mod reconnect_tests;
#[cfg(test)]
mod terminal_tests;
