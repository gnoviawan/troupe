//! The back stack: every open overlay that Escape, the scrim or system back can close, oldest
//! first, as `stores/overlay-stack-store.ts` keeps it. A dialog, sheet, menu, select, popover
//! or command palette pushes its key when it opens; a phone sheet pushes one more key per
//! sub-view. Tooltips, hover cards and toasts never join it: no back press closes them.
//!
//! Rune's modal stack (`El::modal`) traps focus and hands Escape to the top surface only; the
//! surface's close handler calls [`Overlays::close`] with its own key, so the dismiss order and
//! this stack never disagree. [`HistorySentinels`] keeps the web build's history entries equal
//! to [`Overlays::depth`], so the browser's back button closes the top overlay.

/// Open overlays, oldest first; the last one is on top.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Overlays {
    stack: Vec<String>,
}

impl Overlays {
    /// Opens `key` on top. Opening a key that is already open changes nothing.
    pub fn open(&mut self, key: impl Into<String>) {
        let key = key.into();
        if !self.is_open(&key) {
            self.stack.push(key);
        }
    }

    /// Closes `key` and everything opened above it (a menu's submenu, a sheet's sub-views).
    /// Returns the keys it closed, topmost first; empty when `key` was not open.
    pub fn close(&mut self, key: &str) -> Vec<String> {
        let Some(at) = self.stack.iter().position(|k| k == key) else { return Vec::new() };
        let mut closed = self.stack.split_off(at);
        closed.reverse();
        closed
    }

    /// Closes the top overlay only: what Escape and a back press do.
    pub fn pop(&mut self) -> Option<String> {
        self.stack.pop()
    }

    pub fn is_open(&self, key: &str) -> bool {
        self.stack.iter().any(|k| k == key)
    }

    pub fn top(&self) -> Option<&str> {
        self.stack.last().map(String::as_str)
    }

    pub fn depth(&self) -> usize {
        self.stack.len()
    }

    /// Every open key, bottom first.
    pub fn keys(&self) -> &[String] {
        &self.stack
    }

    /// The keys above `key`, bottom first: a phone sheet's open sub-views.
    pub fn above(&self, key: &str) -> &[String] {
        match self.stack.iter().position(|k| k == key) {
            Some(at) => &self.stack[at + 1..],
            None => &[],
        }
    }
}

/// What the web build does to `window.history` to match the stack.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HistoryStep {
    /// Push one sentinel per depth in the range; each entry's state records its depth.
    Push(std::ops::Range<usize>),
    /// Go back over this many of our own sentinels (`history.go(-n)`).
    Back(usize),
}

/// The overlay entries on the browser's history, as the mobile shell's reconciler keeps them:
/// one sentinel per open overlay, each recording its depth. A close by Escape, the scrim or a
/// button consumes its sentinel. A traversal the user made (Back, several entries at once, or
/// Forward) lands on an entry whose depth says how many overlays to close; the `popstate` our
/// own [`HistoryStep::Back`] causes closes none.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HistorySentinels {
    /// The depth of the history entry the page is on: 0 off our sentinels.
    pushed: usize,
    /// `popstate` events still to come from our own `history.go(-n)`.
    expected: usize,
}

impl HistorySentinels {
    /// The step that brings the sentinels to `depth`, or None when they match.
    pub fn reconcile(&mut self, depth: usize) -> Option<HistoryStep> {
        if self.expected > 0 {
            return None;
        }
        match depth.cmp(&self.pushed) {
            std::cmp::Ordering::Greater => {
                let step = HistoryStep::Push(self.pushed + 1..depth + 1);
                self.pushed = depth;
                Some(step)
            }
            std::cmp::Ordering::Less => {
                let n = self.pushed - depth;
                self.pushed = depth;
                self.expected = 1;
                Some(HistoryStep::Back(n))
            }
            std::cmp::Ordering::Equal => None,
        }
    }

    /// The page moved to an entry of depth `landed` (0 for one that is not our sentinel).
    /// Returns how many overlays to close: none for our own step or a Forward, one per entry
    /// for a Back.
    pub fn popstate(&mut self, landed: usize) -> usize {
        let closes = if self.expected > 0 {
            self.expected -= 1;
            0
        } else {
            self.pushed.saturating_sub(landed)
        };
        self.pushed = landed;
        closes
    }

    /// A history call threw: the page is on depth `pushed`, and no `popstate` is coming.
    pub fn failed(&mut self, pushed: usize) {
        self.pushed = pushed;
        self.expected = 0;
    }
}
