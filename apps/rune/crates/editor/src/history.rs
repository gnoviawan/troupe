//! Undo and redo as rope snapshots. A rope clone shares every node it does not change, so a
//! step costs a few nodes, not a copy of the file. Steps join the way CodeMirror's `history()`
//! joins them: the same kind of edit, from the caret the last one left, within
//! [`JOIN_DELAY`] seconds.

use ropey::Rope;

use crate::Selection;

/// CodeMirror's `newGroupDelay`, in seconds.
pub const JOIN_DELAY: f64 = 0.5;

/// Steps kept; the oldest go first.
const DEPTH: usize = 10_000;

/// What an edit was, for joining.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// One typed character or a line break.
    Type,
    DeleteBackward,
    DeleteForward,
    /// A paste, an indent or a replace: never joins.
    Other,
}

type Snapshot = (Rope, Selection);

#[derive(Clone, Default)]
pub struct History {
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    /// The last recorded edit: its kind, time and the caret it left. The next edit joins it
    /// when it starts from that caret.
    last: Option<(Kind, f64, usize)>,
}

impl History {
    /// Records an edit made over `before` that left the caret at `caret`.
    pub fn record(&mut self, before: Snapshot, kind: Kind, caret: usize, now: f64) {
        self.redo.clear();
        let joins = self.last.is_some_and(|(last, time, left)| {
            kind != Kind::Other && last == kind && now - time < JOIN_DELAY && before.1 == Selection::caret(left)
        });
        if !joins {
            if self.undo.len() == DEPTH {
                self.undo.remove(0);
            }
            self.undo.push(before);
        }
        self.last = Some((kind, now, caret));
    }

    pub fn undo(&mut self, text: &Rope, sel: Selection) -> Option<Snapshot> {
        let step = self.undo.pop()?;
        self.redo.push((text.clone(), sel));
        self.last = None;
        Some(step)
    }

    pub fn redo(&mut self, text: &Rope, sel: Selection) -> Option<Snapshot> {
        let step = self.redo.pop()?;
        self.undo.push((text.clone(), sel));
        self.last = None;
        Some(step)
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
}
