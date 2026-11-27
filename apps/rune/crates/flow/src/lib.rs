//! Troupe's node canvas for the Rune client, standing in for React Flow and dagre in the
//! automation editor.
//!
//! - [`layout`]: where nodes sit; saved positions are kept exactly, the rest are laid out in
//!   layers left to right.
//! - [`edge`]: the curve an edge draws between two ports.
//! - `camera` and `canvas` (feature `view`): the pan and zoom camera in Rune `Live` values, and
//!   the canvas element with selection, node drag and port-to-port connect.

#![forbid(unsafe_code)]

pub mod edge;
pub mod layout;

#[cfg(feature = "view")]
pub mod camera;
#[cfg(feature = "view")]
pub mod canvas;

pub use layout::{Link, Node, Pos, Spacing, auto_place, place};
