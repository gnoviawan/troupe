//! The overlay layer: Dialog, AlertDialog, ConfirmDialog, Sheet, Popover, HoverCard, Tooltip,
//! DropdownMenu, ContextMenu, Select, the Command palette and the Toast stack, drawn as the web
//! client draws `components/ui/` (metrics from `apps/rune/tools/dump-overlays.ts`).
//!
//! An overlay renders in two parts. Its trigger stays where the view puts it, and its surface
//! goes into a `layer` the view mounts above everything with [`root`]. A surface is anchored to
//! its trigger's key (it flips to the other side and stays 8 points inside the window when it
//! doesn't fit), or centred (dialogs), or pinned to an edge (sheets, toasts).
//!
//! Who does what:
//! - Rune's modal stack (`El::modal`) traps focus in the top surface, makes the rest inert,
//!   hands Escape and presses outside to the top surface only, and returns focus to where it
//!   was when the surface opened.
//! - [`Overlays`] is the app's back stack of open keys. A surface's close handler closes its
//!   own key, so Escape closes one surface at a time, top first.
//! - [`history`] maps that stack to the browser's history in the web build.
//!
//! ```ignore
//! let mut layer = Vec::new();
//! let trigger = Button::new("Rename").build(t).key("rename-trigger").on_press(|s: &mut S| s.overlays.open("rename"));
//! Dialog::new("rename", "Rename session").open(s.overlays.is_open("rename"))
//!     .on_close(|s: &mut S| { s.overlays.close("rename"); })
//!     .build(t, &mut layer);
//! ui::overlay::root(page.child(trigger), layer)
//! ```

pub mod command;
pub mod dialog;
pub mod floating;
pub mod history;
pub mod menu;
pub mod mobile_sheet;
pub mod select;
pub mod sheet;
pub mod stack;
pub mod toast;

#[cfg(test)]
mod tests;

use std::rc::Rc;

use rune::{Color, El, el};
use theme::Tokens;

pub use command::{Command, CommandItem};
pub use dialog::{ConfirmDialog, ConfirmVariant, Dialog, DialogKind};
pub use floating::{HoverCard, Placement, Popover, Tooltip};
pub use menu::{Menu, MenuEntry, MenuItem};
pub use mobile_sheet::{MobileSheet, SheetDrag, SheetRelease};
pub use select::Select;
pub use sheet::{Sheet, SheetSide};
pub use stack::{HistorySentinels, HistoryStep, Overlays};
pub use toast::{Toast, ToastKind, Toasts};

use crate::paint::shadows;

/// Mounts `layer` above `base`, in paint and hit order: what is pushed last draws on top.
pub fn root(base: El, layer: Vec<El>) -> El {
    el().key("overlay-root").w_full().h_full().child(base.w_full().h_full()).children(layer)
}

/// The full-window wash under a modal (`bg-overlay/{alpha}`); presses on it reach the modal's
/// outside-press dismissal.
pub(crate) fn scrim(key: String, color: Color) -> El {
    el().key(key).absolute(0.0, 0.0).w_full().h_full().fill(color).a11y_hidden()
}

/// The popover surface (`bg-popover border shadow-md`) at `radius`.
pub(crate) fn popover_surface(t: &Tokens, e: El, radius: f32) -> El {
    shadows(e.radius(radius).fill(t.colors.popover).border(1.0, t.colors.border), &t.shadows.elevation_md)
}

/// A close request in the two places a surface takes it: as its modal's dismissal (Escape and
/// presses outside) and on its own buttons (✕, Cancel).
#[derive(Clone)]
pub(crate) struct Close {
    modal: Rc<dyn Fn(El) -> El>,
    press: Rc<dyn Fn(El) -> El>,
}

impl Close {
    pub(crate) fn new<S: 'static>(f: impl Fn(&mut S) + Clone + 'static) -> Close {
        let dismiss = f.clone();
        Close {
            modal: Rc::new(move |e: El| e.modal(true, dismiss.clone())),
            press: Rc::new(move |e: El| e.on_press(f.clone())),
        }
    }

    pub(crate) fn modal(&self, e: El) -> El {
        (self.modal)(e)
    }

    pub(crate) fn press(&self, e: El) -> El {
        (self.press)(e)
    }
}
