//! The lifecycle both sockets share: open, `authenticate`, stay live on host
//! frames, probe a silent host, and reconnect with jittered exponential
//! backoff. It mirrors the React client's timing policy
//! (`ws-transport-timing.ts`, `liveness.ts`).

use rune_core::machine::{Attempt, Effects, Seq};

/// Upgrade plus `authenticate` budget.
pub const HANDSHAKE_S: f64 = 10.0;
pub const BACKOFF_BASE_S: f64 = 0.5;
pub const BACKOFF_MAX_S: f64 = 8.0;
/// The delay is scaled by a uniform factor in `1 ± BACKOFF_JITTER`.
pub const BACKOFF_JITTER: f64 = 0.25;
/// A connection that stayed up this long reconnects at the base delay.
pub const STABLE_S: f64 = 20.0;
/// The `/ws` host writes a heartbeat every 10 s; this much silence starts a
/// probe. `/terminal/ws` has no heartbeat (see [`Liveness::CloseOnly`]).
pub const SILENCE_S: f64 = 25.0;
/// How long a probe `ping` may go unanswered before the socket is replaced.
pub const PROBE_S: f64 = 5.0;
/// Keepalive `ping` cadence while live, under the host's 75 s idle timeout.
pub const PING_EVERY_S: f64 = 30.0;

/// Where the connection is.
#[derive(Clone, Debug, PartialEq)]
pub enum Link {
    Idle,
    Opening {
        socket: Attempt,
        tries: u32,
        deadline: f64,
    },
    Authenticating {
        socket: Attempt,
        tries: u32,
        deadline: f64,
    },
    Live {
        socket: Attempt,
        tries: u32,
        since: f64,
        heard: f64,
        next_ping: f64,
        probe: Probe,
    },
    Backoff {
        tries: u32,
        due: f64,
    },
    /// The host refused this client for good (bad token, protocol mismatch).
    /// Only a new [`LinkEvent::Start`] tries again.
    Halted {
        code: String,
        message: String,
    },
}

/// Whether a silent host is being asked if it is still there.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Probe {
    Quiet,
    Waiting { deadline: f64 },
}

/// What the app shows about the connection.
#[derive(Clone, Debug, PartialEq)]
pub enum Status {
    Connecting,
    Live,
    /// Waiting `delay_s` before attempt number `tries`.
    Reconnecting {
        tries: u32,
        delay_s: f64,
    },
    Halted {
        code: String,
        message: String,
    },
    Stopped,
}

#[derive(Clone, Debug, PartialEq)]
pub enum LinkEvent {
    Start,
    Stop,
    Opened(Attempt),
    Closed(Attempt),
    /// Any frame from the host on `socket`.
    Heard(Attempt),
    AuthOk(Attempt),
    AuthRefused {
        socket: Attempt,
        code: String,
        message: String,
        fatal: bool,
    },
    Tick,
}

#[derive(Clone, Debug, PartialEq)]
pub enum LinkEffect {
    Open(Attempt),
    Close(Attempt),
    /// Send the `authenticate` request on `socket`.
    Authenticate(Attempt),
    /// Send a `ping` request on `socket`.
    Ping(Attempt),
    WakeAt(f64),
    /// `socket` is authenticated: resubscribe everything.
    Up(Attempt),
    /// The live socket is gone: everything sent on it is unanswered.
    Down,
    Status(Status),
}

/// How a live socket is judged alive.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Liveness {
    /// The host writes a heartbeat (`/ws`): silence starts a `ping` probe, and
    /// an unanswered probe replaces the socket.
    Heartbeat,
    /// The host writes nothing on a timer (`/terminal/ws`): only a close or a
    /// failed read or write ends the socket.
    CloseOnly,
}

/// The lifecycle state with its attempt counter and jitter source.
#[derive(Clone, Debug)]
pub struct LinkMachine {
    pub state: Link,
    liveness: Liveness,
    seq: Seq,
    rng: u64,
}

impl LinkMachine {
    #[must_use]
    pub fn new(seed: u64, liveness: Liveness) -> Self {
        Self {
            state: Link::Idle,
            liveness,
            seq: Seq::default(),
            rng: seed | 1,
        }
    }

    /// The socket a frame must arrive on to count, while one is open.
    #[must_use]
    pub fn socket(&self) -> Option<Attempt> {
        match self.state {
            Link::Opening { socket, .. }
            | Link::Authenticating { socket, .. }
            | Link::Live { socket, .. } => Some(socket),
            Link::Idle | Link::Backoff { .. } | Link::Halted { .. } => None,
        }
    }

    /// The authenticated socket, if any.
    #[must_use]
    pub fn live(&self) -> Option<Attempt> {
        match self.state {
            Link::Live { socket, .. } => Some(socket),
            _ => None,
        }
    }

    pub fn step(&mut self, event: LinkEvent, now: f64) -> Effects<LinkEffect> {
        if let LinkEvent::Closed(s) = event {
            return if self.socket() == Some(s) {
                self.fail(now)
            } else {
                Effects::none()
            };
        }
        match (&mut self.state, event) {
            (Link::Idle | Link::Halted { .. }, LinkEvent::Start) => self.open(0, now, true),
            (_, LinkEvent::Start) => Effects::none(),
            (_, LinkEvent::Stop) => {
                let mut fx = Effects::none();
                if let Some(socket) = self.socket() {
                    fx.push(LinkEffect::Close(socket));
                }
                if self.live().is_some() {
                    fx.push(LinkEffect::Down);
                }
                self.state = Link::Idle;
                fx.push(LinkEffect::Status(Status::Stopped));
                fx
            }
            (
                Link::Opening {
                    socket,
                    tries,
                    deadline,
                },
                LinkEvent::Opened(s),
            ) if *socket == s => {
                let (socket, tries, deadline) = (*socket, *tries, *deadline);
                self.state = Link::Authenticating {
                    socket,
                    tries,
                    deadline,
                };
                Effects::one(LinkEffect::Authenticate(socket))
            }
            (
                Link::Live {
                    socket,
                    heard,
                    probe,
                    ..
                },
                LinkEvent::Heard(s),
            ) if *socket == s => {
                *heard = now;
                *probe = Probe::Quiet;
                Effects::none()
            }
            (
                Link::Authenticating {
                    socket,
                    tries,
                    deadline,
                },
                LinkEvent::AuthOk(s),
            ) if *socket == s => {
                if now >= *deadline {
                    log::info!("[client] handshake did not finish within {HANDSHAKE_S}s");
                    return self.fail(now);
                }
                let (socket, tries) = (*socket, *tries);
                let next_ping = now + PING_EVERY_S;
                self.state = Link::Live {
                    socket,
                    tries,
                    since: now,
                    heard: now,
                    next_ping,
                    probe: Probe::Quiet,
                };
                let mut fx = Effects::none();
                fx.push(LinkEffect::Status(Status::Live));
                fx.push(LinkEffect::Up(socket));
                if self.liveness == Liveness::Heartbeat {
                    fx.push(LinkEffect::WakeAt(next_ping.min(now + SILENCE_S)));
                }
                fx
            }
            (
                Link::Authenticating { socket, .. },
                LinkEvent::AuthRefused {
                    socket: s,
                    code,
                    message,
                    fatal,
                },
            ) if *socket == s => {
                if !fatal {
                    return self.fail(now);
                }
                log::warn!("[client] host refused authenticate: {code}");
                self.state = Link::Halted {
                    code: code.clone(),
                    message: message.clone(),
                };
                Effects::two(
                    LinkEffect::Close(s),
                    LinkEffect::Status(Status::Halted { code, message }),
                )
            }
            (_, LinkEvent::Tick) => self.tick(now),
            _ => Effects::none(),
        }
    }

    fn tick(&mut self, now: f64) -> Effects<LinkEffect> {
        match &mut self.state {
            Link::Opening { deadline, .. } | Link::Authenticating { deadline, .. }
                if now >= *deadline =>
            {
                log::info!("[client] handshake did not finish within {HANDSHAKE_S}s");
                self.fail(now)
            }
            Link::Backoff { tries, due } if now >= *due => {
                let tries = *tries;
                self.open(tries, now, false)
            }
            Link::Live {
                socket,
                heard,
                next_ping,
                probe,
                ..
            } if self.liveness == Liveness::Heartbeat => {
                let socket = *socket;
                let mut fx = Effects::none();
                match *probe {
                    Probe::Waiting { deadline } if now >= deadline => {
                        log::info!(
                            "[client] host silent for {:.0}s and the probe went unanswered",
                            now - *heard
                        );
                        return self.fail(now);
                    }
                    Probe::Quiet if now - *heard >= SILENCE_S => {
                        *probe = Probe::Waiting {
                            deadline: now + PROBE_S,
                        };
                        fx.push(LinkEffect::Ping(socket));
                    }
                    _ => {}
                }
                if now >= *next_ping {
                    *next_ping = now + PING_EVERY_S;
                    fx.push(LinkEffect::Ping(socket));
                }
                let due = match *probe {
                    Probe::Waiting { deadline } => deadline,
                    Probe::Quiet => *heard + SILENCE_S,
                };
                fx.push(LinkEffect::WakeAt(due.min(*next_ping)));
                fx
            }
            _ => Effects::none(),
        }
    }

    fn open(&mut self, tries: u32, now: f64, first: bool) -> Effects<LinkEffect> {
        let socket = self.seq.next();
        let deadline = now + HANDSHAKE_S;
        self.state = Link::Opening {
            socket,
            tries,
            deadline,
        };
        let mut fx = Effects::none();
        if first {
            fx.push(LinkEffect::Status(Status::Connecting));
        }
        fx.push(LinkEffect::Open(socket));
        fx.push(LinkEffect::WakeAt(deadline));
        fx
    }

    /// The socket failed or was given up: close it and wait out the backoff.
    fn fail(&mut self, now: f64) -> Effects<LinkEffect> {
        let mut fx = Effects::none();
        if let Some(socket) = self.socket() {
            fx.push(LinkEffect::Close(socket));
        }
        let tries = match self.state {
            Link::Live { tries, since, .. } => {
                fx.push(LinkEffect::Down);
                if now - since >= STABLE_S { 0 } else { tries }
            }
            Link::Opening { tries, .. } | Link::Authenticating { tries, .. } => tries,
            Link::Idle | Link::Backoff { .. } | Link::Halted { .. } => return Effects::none(),
        };
        let delay_s = self.backoff(tries);
        let due = now + delay_s;
        self.state = Link::Backoff {
            tries: tries + 1,
            due,
        };
        fx.push(LinkEffect::Status(Status::Reconnecting {
            tries: tries + 1,
            delay_s,
        }));
        fx.push(LinkEffect::WakeAt(due));
        fx
    }

    /// `base * 2^tries` capped at the max, scaled by the jitter factor.
    fn backoff(&mut self, tries: u32) -> f64 {
        let raw = (BACKOFF_BASE_S * 2f64.powi(tries.min(16) as i32)).min(BACKOFF_MAX_S);
        raw * (1.0 + (self.unit() * 2.0 - 1.0) * BACKOFF_JITTER)
    }

    /// A uniform value in `[0, 1)` from xorshift64*.
    fn unit(&mut self) -> f64 {
        self.rng ^= self.rng >> 12;
        self.rng ^= self.rng << 25;
        self.rng ^= self.rng >> 27;
        let bits = self.rng.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 11;
        bits as f64 / (1u64 << 53) as f64
    }
}
