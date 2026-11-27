//! When to restart the server. The runner feeds it what happened and carries out what it
//! returns; it never touches a process. A server that exits is started again after 250 ms,
//! 1 s, 2 s, then 5 s. Five exits in a row, each within 30 s of its start, stop the restarts
//! until the user retries. A server that ran longer than that resets the count. A restarted
//! server counts as back only once it answers `/health` (`Started`).

use std::time::{Duration, Instant};

pub const BACKOFF: [Duration; 4] = [
    Duration::from_millis(250),
    Duration::from_secs(1),
    Duration::from_secs(2),
    Duration::from_secs(5),
];
/// An exit this soon after the start counts toward giving up.
pub const FAST: Duration = Duration::from_secs(30);
pub const MAX_FAST_EXITS: u32 = 5;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Phase {
    Running { since: Instant },
    Restarting { due: Instant },
    /// A new server was spawned and has not answered `/health` yet.
    Starting,
    GaveUp,
    Stopped,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    /// A server started and answered `/health` (the first, a restart, or a retry).
    Started { at: Instant },
    /// The server exited or stopped answering; `status` says how.
    Exited { at: Instant, status: String },
    /// A clock tick; restarts that are due fire.
    Tick { at: Instant },
    /// The user asked to try again after the supervisor gave up.
    Retry,
    /// The app is quitting.
    Stop,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Effect {
    Spawn,
    Notify(Notice),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Notice {
    Restarting { status: String, delay: Duration },
    Recovered,
    GaveUp { status: String, exits: u32 },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Supervisor {
    pub phase: Phase,
    /// Exits in a row that came within `FAST` of their start.
    pub fast_exits: u32,
}

impl Supervisor {
    pub fn running(since: Instant) -> Supervisor {
        Supervisor { phase: Phase::Running { since }, fast_exits: 0 }
    }

    pub fn step(&mut self, event: Event) -> Option<Effect> {
        match (&self.phase, event) {
            (Phase::Stopped, _) => None,
            (_, Event::Stop) => {
                self.phase = Phase::Stopped;
                None
            }
            // From a phase other than Running, the spawn itself failed: it ran for no time.
            (phase, Event::Exited { at, status }) => {
                let ran = match phase {
                    Phase::Running { since } => at.saturating_duration_since(*since),
                    _ => Duration::ZERO,
                };
                self.fast_exits = if ran < FAST { self.fast_exits + 1 } else { 1 };
                if self.fast_exits >= MAX_FAST_EXITS {
                    self.phase = Phase::GaveUp;
                    return Some(Effect::Notify(Notice::GaveUp { status, exits: self.fast_exits }));
                }
                let delay = BACKOFF[(self.fast_exits as usize - 1).min(BACKOFF.len() - 1)];
                self.phase = Phase::Restarting { due: at + delay };
                Some(Effect::Notify(Notice::Restarting { status, delay }))
            }
            (Phase::Restarting { due }, Event::Tick { at }) if at >= *due => {
                self.phase = Phase::Starting;
                Some(Effect::Spawn)
            }
            (Phase::GaveUp, Event::Retry) => {
                self.fast_exits = 0;
                self.phase = Phase::Starting;
                Some(Effect::Spawn)
            }
            (Phase::Starting, Event::Started { at }) => {
                self.phase = Phase::Running { since: at };
                Some(Effect::Notify(Notice::Recovered))
            }
            _ => None,
        }
    }
}
