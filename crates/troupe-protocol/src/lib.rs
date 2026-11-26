//! The wire contracts of Troupe's two sockets, `/ws` and `/terminal/ws`.
//!
//! Every type serializes to the exact JSON the host and the React client
//! exchange today. Fields the hand-written TypeScript contract leaves open
//! (`unknown`) stay `serde_json::Value`. `schema::protocol_schema` exports the
//! JSON Schema that `scripts/protocol/gen-ts.mjs` turns into TypeScript.

#[macro_use]
mod literal;

pub mod schema;
pub mod terminal;
pub mod version;
pub mod ws;

pub use literal::{False, True};

#[cfg(test)]
mod fixtures_tests;
#[cfg(test)]
mod literal_tests;
#[cfg(test)]
mod schema_tests;
