//! The contract between a socket machine and the backend that runs it.

use rune_core::machine::{Attempt, Effects};

/// Who the client is and where the host lives.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    /// `http://host:port` or `https://host:port`, no trailing slash.
    pub base_url: String,
    /// The host's web auth token; `None` when the host runs without one.
    pub token: Option<String>,
    /// Per install or browser profile. Not a credential.
    pub device_id: String,
    /// Per window or tab. Not a credential. Every `commandId` this client
    /// mints starts with it (see [`crate::ledger`]).
    pub client_instance_id: String,
    /// Random and fresh for each client built: it seeds the backoff jitter and
    /// is part of every `commandId`, so a client rebuilt for the same window
    /// never reuses an earlier client's ids.
    pub seed: u64,
}

impl Config {
    /// The socket URL for `path` (`/ws`, `/terminal/ws`).
    #[must_use]
    pub fn socket_url(&self, path: &str) -> String {
        let base = self.base_url.trim_end_matches('/');
        let base = if let Some(rest) = base.strip_prefix("https://") {
            format!("wss://{rest}")
        } else if let Some(rest) = base.strip_prefix("http://") {
            format!("ws://{rest}")
        } else {
            base.to_owned()
        };
        format!("{base}{path}")
    }
}

/// The app's handle for one request it asked a machine to send. The app mints
/// it, so it can match the reply before the machine has said anything. A
/// handle still in use is refused with a `duplicate_call` reply.
pub type CallId = u64;

/// What happened to a socket the machine opened, tagged with the attempt that
/// opened it. An event for an attempt the machine no longer holds is stale.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SocketEvent {
    Opened(Attempt),
    Text(Attempt, String),
    /// The socket closed or failed; `code` is the close code when there was one.
    Closed {
        socket: Attempt,
        code: Option<u16>,
    },
}

/// An input to a machine. `now` is seconds on any monotonic clock.
#[derive(Clone, Debug, PartialEq)]
pub enum Event<C> {
    /// Connect, and keep reconnecting until [`Event::Stop`].
    Start,
    /// Close the socket and stay down.
    Stop,
    Socket(SocketEvent),
    /// A [`Effect::WakeAt`] came due (or any later time).
    Tick,
    /// A request from the app.
    Command(C),
}

/// What a machine asks its backend to do, in order.
#[derive(Clone, Debug, PartialEq)]
pub enum Effect<O> {
    /// Open a socket to `url`; report it with `socket`.
    Open {
        socket: Attempt,
        url: String,
    },
    /// Write one text frame.
    Send {
        socket: Attempt,
        text: String,
    },
    Close {
        socket: Attempt,
    },
    /// Step the machine with [`Event::Tick`] at this time.
    WakeAt(f64),
    /// Hand this to the app.
    Output(O),
}

/// A socket machine: [`crate::ws::WsClient`] or [`crate::terminal::TerminalClient`].
pub trait Machine {
    type Command;
    type Output;
    /// The socket path under the base URL.
    const PATH: &'static str;

    fn step(&mut self, event: Event<Self::Command>, now: f64) -> Effects<Effect<Self::Output>>;
}
