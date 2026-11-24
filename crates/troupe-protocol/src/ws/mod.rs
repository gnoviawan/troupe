//! The `/ws` contract: one multiplexed socket for requests, replies, session
//! log events and watches. Envelope fields are snake_case; payloads are
//! camelCase.

mod agents;
mod frames;
mod names;
mod payloads;
mod watch;

pub use agents::*;
pub use frames::*;
pub use names::*;
pub use payloads::*;
pub use watch::*;
