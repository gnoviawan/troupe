//! Troupe's base components for the Rune client: one module per file of the web client's
//! `src/renderer/components/ui/` that needs no overlay layer, with the same names, variants,
//! sizes and states. Metrics come from the browser's computed styles
//! (`apps/rune/tools/dump-registry.ts`), not from Tailwind class names.
//!
//! Every builder ends in `build(&Tokens) -> El`; the caller adds handlers (`on_press`,
//! `on_edit`) and a key to the element it returns.
//!
//! ```ignore
//! let t = theme::tokens("termul", true);
//! ui::Button::new("Run task").variant(ButtonVariant::Outline).build(t).on_press(|s: &mut S| s.runs += 1)
//! ```
//!
//! Dialogs, menus, popovers, sheets, toasts and tooltips are the overlay layer, [`overlay`].

#![forbid(unsafe_code)]

pub mod base;
pub mod chat;
pub mod composer;
pub mod editor;
pub mod explorer;
pub mod git;
pub mod mobile;
pub mod overlay;
pub mod markdown;
pub mod paint;
pub mod settings;
pub mod shell;
pub mod terminal;
pub mod type_;
pub mod viewers;
pub mod workspace;
pub mod zoom;

pub use base::*;
pub use theme::Tokens;
pub use zoom::Zoom;
