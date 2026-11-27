//! Automations in the Rune client: the host contract, the canvas draft and its op mirror,
//! where cards sit, what a run says about each step, how it all reads, and the routes. The
//! web client's sources are `components/automations/` (`automation-format.ts`, `canvas/draft/`,
//! `canvas/layout.ts`, `canvas/ports.ts`, `canvas/palette/node-kinds.ts`, `canvas/run/`) and
//! `lib/automation-api.ts`; `tests/vectors.rs` replays what `tools/parity/vectors/` recorded
//! from them.

pub mod draft;
pub mod format;
pub mod gestures;
pub mod kinds;
pub mod layout;
pub mod ops;
pub mod routes;
pub mod run;
pub mod types;

pub use types::*;
