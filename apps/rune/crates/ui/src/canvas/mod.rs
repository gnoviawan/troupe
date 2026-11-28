//! The design canvas (FigCraft): the host side of its control protocol ([`bridge`]), one mount per
//! project with its persistence and the agent gateway ([`Canvases`]), and the canvas tab's pane
//! ([`view`]). The editor itself is the entry's: the desktop app runs FigCraft in process
//! (`troupe-canvas`), the web client frames the server's figcraft bundle.

pub mod bridge;
mod flow;
mod persistence;
mod session;
pub mod view;

#[cfg(all(test, not(target_arch = "wasm32")))]
mod bridge_tests;
#[cfg(test)]
mod session_tests;
#[cfg(test)]
mod view_tests;

pub use flow::{Canvases, Failure, HostFacts, Then};
pub use persistence::{CANVAS_DOC_EXTENSION, NEW_CANVAS_DOC_NAME, canvas_doc_file, is_canvas_doc_path, pick_canvas_doc, same_doc};
pub use session::{Choice, Conflict, Effect, LeaseLoss, Lost, OpenInfo, Phase, Remount, Session, UnloadReason, View, pin_refused};
