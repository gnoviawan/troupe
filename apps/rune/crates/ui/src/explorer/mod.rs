//! The Explorer side panel (`components/file-explorer/FileExplorer.tsx`): the project's file
//! tree drawn with rune-kit's tree rows, its header actions, the row context menu for files and
//! folders, the header's More menu, the inline create and rename field, the delete confirm, and
//! dragging a file onto a pane.
//!
//! [`Explorer`] is the state the app keeps: the tree (`model::explorer`) plus the field's text,
//! the open menu and a file drag. [`Explorer::effects`] collects what needs the server or the
//! workspace; the app drains it. [`view`] draws the panel body and [`layer`] its menus and
//! confirm.

mod view;

#[cfg(test)]
mod tests;

use model::explorer::{self as store, Effect, Event, Inline};
use rune::{PointerPress, TextEdit};

pub use view::{NAME_KEY, Hooks, TREE_KEY, header_actions, layer, row_key, view};

/// A file row dragged toward a pane (`startFileDrag`).
#[derive(Clone, Debug, PartialEq)]
pub struct FileDrag {
    pub path: String,
    /// The pointer in window points.
    pub point: [f32; 2],
    /// Past the 4 pt slop.
    pub moving: bool,
}

/// What a context menu is open on.
#[derive(Clone, Debug, PartialEq)]
pub struct MenuAt {
    pub path: String,
    pub at: PointerPress,
}

#[derive(Default)]
pub struct Explorer {
    pub tree: store::Explorer,
    pub effects: Vec<Effect>,
    /// The inline create or rename field.
    pub name: TextEdit,
    pub menu: Option<MenuAt>,
    /// The header's More menu.
    pub more: bool,
    pub drag: Option<FileDrag>,
}

impl Explorer {
    /// Applies `event`; a new inline field takes its starting text (a rename selects the name
    /// up to its extension, as `InlineNameInput` does).
    pub fn step(&mut self, event: Event) {
        let before = self.tree.inline.clone();
        let fx = store::step(&mut self.tree, event);
        self.effects.extend(fx);
        if self.tree.inline != before {
            self.name = match &self.tree.inline {
                Some(Inline::Rename { entry }) => {
                    let mut edit = TextEdit::new(entry.name.clone());
                    let stem = if entry.dir { None } else { entry.name.rfind('.').filter(|i| *i > 0) };
                    edit.set_selection(0, stem.unwrap_or(entry.name.len()));
                    edit
                }
                _ => TextEdit::default(),
            };
        }
    }

    /// Return in the name field.
    pub fn submit(&mut self) {
        let typed = self.name.text.clone();
        self.step(Event::Submit(typed));
    }

    /// The rows the tree scrolls to, taken out of the effects.
    pub fn take_reveals(&mut self) -> Vec<String> {
        let mut out = Vec::new();
        self.effects.retain(|e| match e {
            Effect::Reveal(path) => {
                out.push(path.clone());
                false
            }
            _ => true,
        });
        out
    }
}
